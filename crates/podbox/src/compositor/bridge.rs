//! Bidirectional byte-stream bridge between the container's Wayland client
//! and the host compositor.
//!
//! One cohesive concern, kept as its own module: Wayland message framing
//! plus `SCM_RIGHTS` fd attribution. The attribution rule is subtle —
//! libwayland sends a descriptor in the same datagram as the message bytes
//! that reference it, so fds must be keyed to the stream offset where the
//! read that carried them ended, and handed to the message that completes
//! at that boundary. Getting this wrong leaks descriptors (blocked
//! message) or hands a client someone else's fd.

use std::collections::VecDeque;
use std::io::{IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::Result;
use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, recvmsg, sendmsg};

use super::firewall::{FirewallState, is_blocked_global, rate_allow};

/// Bidirectional byte-stream bridge between two Unix sockets.
///
/// For the host→client direction, `is_client_to_host = false`, and the
/// bridge intercepts `wl_registry::global` events (opcode 0, string
/// payload at offset 12) to filter interfaces on the blocklist.
///
/// File descriptors received via `SCM_RIGHTS` are attributed per message:
/// each read's fds are keyed to the absolute stream offset at which the
/// read ended, and attach to the Wayland message whose completion boundary
/// matches. libwayland transmits an fd in the same datagram as the message
/// bytes referencing it, so the batch delivered by the completing read
/// belongs to that message — never to earlier messages completed by the
/// same or prior reads, and a dropped message only closes the fds it owns.
pub(super) fn bridge_loop(
    in_socket: UnixStream,
    out_socket: UnixStream,
    state: Arc<Mutex<FirewallState>>,
    done: &AtomicBool,
    is_client_to_host: bool,
) -> Result<()> {
    let mut read_buf = [0u8; 16384];
    let mut cmsg_buffer = vec![0u8; 4096];
    let mut bytes_cache = Vec::with_capacity(32768);
    // Fds grouped by the absolute stream offset of the read that delivered
    // them. Batches are consumed (FIFO) by messages as they complete.
    let mut fd_batches: VecDeque<(usize, Vec<OwnedFd>)> = VecDeque::new();
    // Absolute stream offset of bytes_cache[0].
    let mut base_offset: usize = 0;

    // Token-bucket rate limiter for host→client direction.
    // Protects the guest from slow-client memory exhaustion.
    let mut bucket: f64 = 10_000.0;
    let mut last_refill = Instant::now();

    loop {
        if done.load(Ordering::Relaxed) {
            break;
        }

        let msg_bytes = {
            let mut iov = [IoSliceMut::new(&mut read_buf)];
            let msg = match recvmsg::<()>(
                in_socket.as_raw_fd(),
                &mut iov,
                Some(&mut cmsg_buffer),
                MsgFlags::empty(),
            ) {
                Ok(m) => m,
                Err(e) if e == nix::errno::Errno::EINTR => continue,
                Err(e) => {
                    done.store(true, Ordering::Relaxed);
                    let _ = in_socket.shutdown(std::net::Shutdown::Both);
                    let _ = out_socket.shutdown(std::net::Shutdown::Both);
                    return Err(e.into());
                }
            };

            let bytes = msg.bytes;
            if bytes == 0 {
                break;
            }

            let mut read_fds: Vec<OwnedFd> = Vec::new();
            if let Ok(cmsgs) = msg.cmsgs() {
                for cmsg in cmsgs {
                    if let ControlMessageOwned::ScmRights(fds) = cmsg {
                        for fd in fds {
                            read_fds.push(crate::process::adopt_scm_fd(fd));
                        }
                    }
                }
            }

            // Key this read's fds to where the read ends in the stream:
            // the message completed exactly there owns them.
            if !read_fds.is_empty() {
                fd_batches.push_back((base_offset + bytes_cache.len() + bytes, read_fds));
            }

            bytes
        };

        bytes_cache.extend_from_slice(&read_buf[..msg_bytes]);

        // Process complete Wayland messages from the coalesced buffer.
        let mut consumed = 0;
        while consumed + 8 <= bytes_cache.len() {
            let header = &bytes_cache[consumed..consumed + 8];
            let size_and_opcode = u32::from_ne_bytes(header[4..8].try_into().unwrap());
            let msg_size = (size_and_opcode >> 16) as usize;
            let opcode = (size_and_opcode & 0xFFFF) as u16;

            if msg_size < 8 {
                done.store(true, Ordering::Relaxed);
                let _ = in_socket.shutdown(std::net::Shutdown::Both);
                let _ = out_socket.shutdown(std::net::Shutdown::Both);
                anyhow::bail!("Invalid Wayland message size: {msg_size}");
            }

            if consumed + msg_size > bytes_cache.len() {
                break;
            }

            let message_bytes = &bytes_cache[consumed..consumed + msg_size];
            let should_drop =
                !is_client_to_host && is_blocked_global(message_bytes, opcode, &state);

            let message_end_abs = base_offset + consumed + msg_size;
            let mut msg_fds = take_fd_batches(&mut fd_batches, message_end_abs);

            if should_drop {
                // Dropping `msg_fds` closes them — correct: they belonged to
                // the blocked message alone.
                drop(msg_fds);
            } else if is_client_to_host || rate_allow(&mut bucket, &mut last_refill) {
                forward_message(&out_socket, message_bytes, &mut msg_fds)?;
            } else {
                drop(fd_batches);
                done.store(true, Ordering::Relaxed);
                let _ = in_socket.shutdown(std::net::Shutdown::Both);
                let _ = out_socket.shutdown(std::net::Shutdown::Both);
                tracing::warn!("compositor: rate-limited, closing connection");
                return Ok(());
            }

            consumed += msg_size;
        }

        bytes_cache.drain(..consumed);
        base_offset += consumed;
    }

    // Signal shutdown to the sibling thread
    done.store(true, Ordering::Relaxed);
    let _ = in_socket.shutdown(std::net::Shutdown::Both);
    let _ = out_socket.shutdown(std::net::Shutdown::Both);
    Ok(())
}

/// Take ownership of every fd batch delivered by reads ending at or before
/// `message_end` (absolute stream offset). These are the fds carried by the
/// Wayland message completing at `message_end`: libwayland sends an fd in
/// the same datagram as the message bytes referencing it.
fn take_fd_batches<T>(fd_batches: &mut VecDeque<(usize, Vec<T>)>, message_end: usize) -> Vec<T> {
    let mut out = Vec::new();
    while let Some((read_end, _)) = fd_batches.front() {
        if *read_end > message_end {
            break;
        }
        let (_, fds) = fd_batches.pop_front().expect("front checked");
        out.extend(fds);
    }
    out
}

/// Forward a single Wayland message (with any accumulated fds) to the
/// output socket.
fn forward_message(
    out_socket: &UnixStream,
    message_bytes: &[u8],
    pending_fds: &mut Vec<OwnedFd>,
) -> Result<()> {
    let iov = [IoSlice::new(message_bytes)];

    if pending_fds.is_empty() {
        sendmsg::<()>(out_socket.as_raw_fd(), &iov, &[], MsgFlags::empty(), None)?;
    } else {
        let raw_fds: Vec<RawFd> = pending_fds.iter().map(|f| f.as_raw_fd()).collect();
        let cmsg = ControlMessage::ScmRights(&raw_fds);
        sendmsg::<()>(
            out_socket.as_raw_fd(),
            &iov,
            &[cmsg],
            MsgFlags::empty(),
            None,
        )?;
        pending_fds.clear();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- FD batch attribution ----

    /// Fake fd tokens: attribution logic is offset-based, so plain integers
    /// stand in for real descriptors (no drop side effects).
    fn batches(items: &[(usize, &[u32])]) -> VecDeque<(usize, Vec<u32>)> {
        items
            .iter()
            .map(|&(end, fds)| (end, fds.to_vec()))
            .collect()
    }

    #[test]
    fn fd_batch_attaches_to_completing_message() {
        // Read ended at 24 — exactly where message [16..24) completes.
        let mut q = batches(&[(24, &[7, 8])]);
        let fds = take_fd_batches(&mut q, 24);
        assert_eq!(fds, vec![7u32, 8]);
        assert!(q.is_empty());
    }

    #[test]
    fn fd_batch_waits_for_its_own_message() {
        // Review case: message A [0..12) completed by read #1, then read #2
        // ends at 24 completing B [12..24) and delivering fds. The fds must
        // ride with B, not with the earlier-processed A.
        let mut q = batches(&[(24, &[9])]);
        assert!(take_fd_batches(&mut q, 12).is_empty());
        assert_eq!(take_fd_batches(&mut q, 24), vec![9u32]);
    }

    #[test]
    fn split_message_gets_fds_from_tail_read() {
        // wl_shm.create_pool split across reads: header in read #1, tail +
        // fd in read #2 ending at 40. Only when the message completes at 40
        // do its fds become available.
        let mut q = batches(&[(40, &[5])]);
        assert!(take_fd_batches(&mut q, 20).is_empty());
        assert_eq!(take_fd_batches(&mut q, 40), vec![5u32]);
    }

    #[test]
    fn blocked_message_closes_only_its_own_fds() {
        // Blocked global [0..28) owns the batch from read #1; a later valid
        // message's batch (read #2, end 60) must survive the drop.
        let mut q = batches(&[(28, &[3]), (60, &[4])]);
        let dropped = take_fd_batches(&mut q, 28);
        assert_eq!(dropped, vec![3u32]); // caller drops these
        assert_eq!(take_fd_batches(&mut q, 60), vec![4u32]); // survivor intact
    }
}
