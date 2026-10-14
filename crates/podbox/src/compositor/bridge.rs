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

use super::firewall::{
    FirewallState, fd_arity, forbidden_bind, is_blocked_global, rate_allow, track_client_request,
    track_server_event,
};

/// Bidirectional byte-stream bridge between two Unix sockets.
///
/// For the host→client direction, `is_client_to_host = false`, and the
/// bridge intercepts `wl_registry::global` events (opcode 0, string
/// payload at offset 12) to filter interfaces on the blocklist.
///
/// File descriptors received via `SCM_RIGHTS` are attributed to recognized
/// fd-bearing messages (see [`tracker`]'s audited table): each bearer pops
/// exactly its declared count from the earliest non-empty batches,
/// preserving stream order. A bearer with nothing queued forwards bare,
/// exactly as before. Every other message takes nothing. Leftover
/// descriptors whose reads are fully processed flush with the last kept
/// message of the iteration, which reproduces the old behavior for
/// unrecognized traffic; leftovers with no carrier stay queued instead of
/// leaking.
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

        // Phase 1: split coalesced bytes into complete messages, popping one
        // queued descriptor per recognized bearer in completion order.
        // Tuple: (start, end, fds, drop).
        let mut pending: Vec<(usize, usize, Vec<OwnedFd>, bool)> = Vec::new();
        let mut consumed = 0;
        let mut violation: Option<String> = None;
        while consumed + 8 <= bytes_cache.len() {
            let header = &bytes_cache[consumed..consumed + 8];
            let obj_id = u32::from_ne_bytes(header[0..4].try_into().unwrap());
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
            if is_client_to_host
                && let Some(reason) = forbidden_bind(message_bytes, obj_id, opcode, &state)
            {
                violation = Some(reason);
                break;
            }
            let should_drop =
                !is_client_to_host && is_blocked_global(message_bytes, obj_id, opcode, &state);
            let bearer_count = if is_client_to_host {
                track_client_request(&state, obj_id, opcode, message_bytes);
                fd_arity(&state, obj_id, opcode, true)
            } else {
                track_server_event(&state, obj_id, opcode, message_bytes);
                fd_arity(&state, obj_id, opcode, false)
            };
            let msg_fds = pop_n_fd(&mut fd_batches, bearer_count as usize);

            pending.push((consumed, consumed + msg_size, msg_fds, should_drop));
            consumed += msg_size;
        }

        // A refused bind ends the connection before anything in this batch is
        // forwarded, so the client never reaches the global it was refused.
        if let Some(reason) = violation {
            done.store(true, Ordering::Relaxed);
            let _ = in_socket.shutdown(std::net::Shutdown::Both);
            let _ = out_socket.shutdown(std::net::Shutdown::Both);
            tracing::warn!("compositor: {reason}; closing connection");
            return Ok(());
        }

        // Phase 2: descriptors from fully processed reads that no bearer
        // claimed ride with the last kept message (legacy fallback). With no
        // carrier they stay queued rather than leak.
        let frontier = base_offset + consumed;
        let mut leftover = Vec::new();
        while let Some((read_end, _)) = fd_batches.front() {
            if *read_end > frontier {
                break;
            }
            leftover.extend(fd_batches.pop_front().expect("front checked").1);
        }
        if !leftover.is_empty() {
            if let Some((_, _, fds, _)) = pending.iter_mut().filter(|p| !p.3).last() {
                fds.extend(leftover);
            } else {
                fd_batches.push_front((frontier, leftover));
            }
        }

        // Phase 3: forward in order; a dropped message closes only its own fds.
        for (start, end, mut msg_fds, should_drop) in pending {
            if should_drop {
                drop(msg_fds);
            } else if is_client_to_host || rate_allow(&mut bucket, &mut last_refill) {
                forward_message(&out_socket, &bytes_cache[start..end], &mut msg_fds)?;
            } else {
                drop(fd_batches);
                done.store(true, Ordering::Relaxed);
                let _ = in_socket.shutdown(std::net::Shutdown::Both);
                let _ = out_socket.shutdown(std::net::Shutdown::Both);
                tracing::warn!("compositor: rate-limited, closing connection");
                return Ok(());
            }
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

/// Pop up to `n` descriptors from the earliest non-empty batches,
/// preserving stream order. Bearers take exactly their declared count;
/// leftovers stay queued for later bearers or the end-of-read fallback.
fn pop_n_fd<T>(fd_batches: &mut VecDeque<(usize, Vec<T>)>, n: usize) -> Vec<T> {
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        let Some((_, fds)) = fd_batches.front() else {
            break;
        };
        if fds.is_empty() {
            fd_batches.pop_front();
            continue;
        }
        out.push(fd_batches.front_mut().expect("front checked").1.remove(0));
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

    // ---- FD attribution ----

    fn batches(items: &[(usize, &[u32])]) -> VecDeque<(usize, Vec<u32>)> {
        items
            .iter()
            .map(|&(end, fds)| (end, fds.to_vec()))
            .collect()
    }

    #[test]
    fn pop_n_fd_preserves_fifo_order() {
        let mut q = batches(&[(24, &[7, 8]), (40, &[9])]);
        assert_eq!(pop_n_fd(&mut q, 1), vec![7]);
        assert_eq!(pop_n_fd(&mut q, 2), vec![8, 9]);
        assert!(pop_n_fd::<u32>(&mut q, 1).is_empty());
    }

    #[test]
    fn pop_n_fd_skips_empty_batches() {
        let mut q = batches(&[(10, &[]), (20, &[5])]);
        assert_eq!(pop_n_fd(&mut q, 1), vec![5]);
        // The drained batch lingers empty until the next pop sweeps it.
        assert!(pop_n_fd::<u32>(&mut q, 1).is_empty());
        assert!(q.is_empty());
    }

    #[test]
    fn pop_n_fd_takes_declared_count_only() {
        // create_listener takes two; a third descriptor in the same batch
        // stays queued for the next bearer instead of jumping along.
        let mut q = batches(&[(40, &[5, 6, 7])]);
        assert_eq!(pop_n_fd(&mut q, 2), vec![5, 6]);
        assert_eq!(pop_n_fd(&mut q, 1), vec![7]);
        assert!(pop_n_fd::<u32>(&mut q, 1).is_empty());
    }

    #[test]
    fn pop_n_fd_zero_takes_nothing() {
        let mut q = batches(&[(40, &[5])]);
        assert!(pop_n_fd::<u32>(&mut q, 0).is_empty());
        assert_eq!(pop_n_fd(&mut q, 1), vec![5]);
    }

    // ---- end-to-end fd attribution ----
    //
    // Regression test for the foot failure: `create_pool` + `create_buffer`
    // sent in one flush must deliver the descriptor with message 1, not
    // message 2. The old offset-keyed attribution attached it to whichever
    // message completed at the read boundary (the buffer), and the host
    // compositor killed the client with `invalid arguments for create_pool`.

    use std::os::fd::{AsRawFd, OwnedFd};
    use std::os::unix::net::UnixStream;

    use nix::sys::socket::{ControlMessageOwned, MsgFlags, recvmsg, sendmsg};

    fn wire_msg(obj: u32, opcode: u16, args: &[u8]) -> Vec<u8> {
        let size = u32::try_from(8 + args.len()).expect("message size fits u32");
        let mut buf = Vec::with_capacity(size as usize);
        buf.extend_from_slice(&obj.to_ne_bytes());
        buf.extend_from_slice(&((size << 16) | u32::from(opcode)).to_ne_bytes());
        buf.extend_from_slice(args);
        buf
    }

    fn u32s(v: &[u32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_ne_bytes()).collect()
    }

    /// Read exactly `len` bytes (looping short reads), collecting any
    /// descriptors delivered with those bytes.
    fn recv_exact(fd: RawFd, len: usize) -> (Vec<u8>, Vec<OwnedFd>) {
        use std::io::IoSliceMut;
        let mut out = Vec::new();
        let mut fds = Vec::new();
        let mut cmsg_buf = vec![0u8; 4096];
        while out.len() < len {
            let mut chunk = vec![0u8; len - out.len()];
            let n = {
                let mut iov = [IoSliceMut::new(&mut chunk)];
                let msg =
                    recvmsg::<()>(fd, &mut iov, Some(&mut cmsg_buf), MsgFlags::empty()).unwrap();
                assert!(msg.bytes > 0, "unexpected EOF");
                if let Ok(cmsgs) = msg.cmsgs() {
                    for cmsg in cmsgs {
                        if let ControlMessageOwned::ScmRights(rcvd) = cmsg {
                            fds.extend(rcvd.into_iter().map(crate::process::adopt_scm_fd));
                        }
                    }
                }
                msg.bytes
            };
            out.extend_from_slice(&chunk[..n]);
        }
        (out, fds)
    }

    #[test]
    fn create_pool_with_trailing_buffer_keeps_its_fd() {
        let (bridge_in, client) = UnixStream::pair().unwrap();
        let (bridge_out, host) = UnixStream::pair().unwrap();
        let host_fd = host.as_raw_fd();

        let state = Arc::new(Mutex::new(FirewallState::new(vec![])));
        let done = Arc::new(AtomicBool::new(false));
        let worker = std::thread::spawn(move || {
            bridge_loop(bridge_in, bridge_out, state, &done, true).unwrap();
        });

        // Teach the tracker: registry id 2, wl_shm bound as id 9.
        let get_registry = wire_msg(1, 1, &u32s(&[2]));
        sendmsg::<()>(
            client.as_raw_fd(),
            &[IoSlice::new(&get_registry)],
            &[],
            MsgFlags::empty(),
            None,
        )
        .unwrap();
        let mut bind_args = u32s(&[5]);
        bind_args.extend_from_slice(&(7u32.to_ne_bytes())); // "wl_shm" len incl NUL
        bind_args.extend_from_slice(b"wl_shm\0\0"); // padded to 8
        bind_args.extend_from_slice(&u32s(&[1, 9])); // version, new_id
        let bind = wire_msg(2, 0, &bind_args);
        sendmsg::<()>(
            client.as_raw_fd(),
            &[IoSlice::new(&bind)],
            &[],
            MsgFlags::empty(),
            None,
        )
        .unwrap();

        // The failing flush: create_pool(new 46, fd, size) + create_buffer,
        // one sendmsg, one descriptor — exactly as libwayland emits it.
        let pool = wire_msg(9, 0, &u32s(&[46, 4096]));
        let buffer = wire_msg(46, 0, &u32s(&[39, 0, 0, 0, 0, 0]));
        let mut blob = pool.clone();
        blob.extend_from_slice(&buffer);
        let (pread, _pwrite) = nix::unistd::pipe().unwrap();
        let cmsg = ControlMessage::ScmRights(&[pread.as_raw_fd()]);
        sendmsg::<()>(
            client.as_raw_fd(),
            &[IoSlice::new(&blob)],
            &[cmsg],
            MsgFlags::empty(),
            None,
        )
        .unwrap();
        drop(pread);

        // Drain in exact message sizes: 12, 32, 16, 32.
        let (m1, f1) = recv_exact(host_fd, 12);
        assert_eq!(m1, get_registry);
        assert!(f1.is_empty());
        let (m2, f2) = recv_exact(host_fd, 32);
        assert_eq!(m2, bind);
        assert!(f2.is_empty());
        let (m3, f3) = recv_exact(host_fd, 16);
        assert_eq!(m3, pool, "pool bytes must arrive intact");
        assert_eq!(f3.len(), 1, "pool must carry exactly one fd");
        let (m4, f4) = recv_exact(host_fd, 32);
        assert_eq!(m4, buffer);
        assert!(f4.is_empty(), "buffer must carry no fd");

        drop(client);
        worker.join().unwrap();
    }

    #[test]
    fn keymap_event_keeps_its_fd_across_directions() {
        // Full-duplex version of the foot failure: the client binds wl_seat
        // and gets a keyboard on the c2h path; the host's keymap event must
        // arrive with its descriptor on the h2c path. Both bridge threads
        // share one Tracker — this test fails if either direction's
        // observation is lost.
        use std::sync::Mutex;

        let (bridge_c, script_c) = UnixStream::pair().unwrap();
        let (bridge_h, script_h) = UnixStream::pair().unwrap();

        let state = Arc::new(Mutex::new(FirewallState::new(vec![])));
        let done = Arc::new(AtomicBool::new(false));

        let c2h = {
            let (in_s, out_s) = (bridge_c.try_clone().unwrap(), bridge_h.try_clone().unwrap());
            let (st, dn) = (Arc::clone(&state), Arc::clone(&done));
            std::thread::spawn(move || {
                bridge_loop(in_s, out_s, st, &dn, true).unwrap();
            })
        };
        let h2c = {
            let (st, dn) = (Arc::clone(&state), Arc::clone(&done));
            std::thread::spawn(move || {
                bridge_loop(bridge_h, bridge_c, st, &dn, false).unwrap();
            })
        };

        // Client: registry 2, bind wl_seat as 4, get_keyboard as 28.
        let get_registry = wire_msg(1, 1, &u32s(&[2]));
        sendmsg::<()>(
            script_c.as_raw_fd(),
            &[IoSlice::new(&get_registry)],
            &[],
            MsgFlags::empty(),
            None,
        )
        .unwrap();
        let mut bind_args = u32s(&[1]);
        bind_args.extend_from_slice(&8u32.to_ne_bytes()); // "wl_seat" len incl NUL
        bind_args.extend_from_slice(b"wl_seat\0"); // already 8 with NUL
        bind_args.extend_from_slice(&u32s(&[8, 4])); // version, new_id
        assert_eq!(bind_args.len(), 24);
        let bind = wire_msg(2, 0, &bind_args);
        sendmsg::<()>(
            script_c.as_raw_fd(),
            &[IoSlice::new(&bind)],
            &[],
            MsgFlags::empty(),
            None,
        )
        .unwrap();
        let get_keyboard = wire_msg(4, 1, &u32s(&[28]));
        sendmsg::<()>(
            script_c.as_raw_fd(),
            &[IoSlice::new(&get_keyboard)],
            &[],
            MsgFlags::empty(),
            None,
        )
        .unwrap();

        // Host: keymap event on 28 with one descriptor, single sendmsg.
        let keymap = wire_msg(28, 0, &u32s(&[1, 35522]));
        let (pread, _pwrite) = nix::unistd::pipe().unwrap();
        let cmsg = ControlMessage::ScmRights(&[pread.as_raw_fd()]);
        sendmsg::<()>(
            script_h.as_raw_fd(),
            &[IoSlice::new(&keymap)],
            &[cmsg],
            MsgFlags::empty(),
            None,
        )
        .unwrap();
        drop(pread);

        // Client must receive the keymap bytes with exactly one descriptor.
        // Drain order on script_c: its own requests are NOT echoed (bridge
        // is not a loopback), so the first 16 bytes here are the keymap.
        // (Draining script_h first also synchronizes with the c2h thread:
        // without it the join below can race socket teardown.)
        let (echoed, echo_fds) = recv_exact(script_h.as_raw_fd(), 12 + 32 + 12);
        assert_eq!(
            echoed,
            [get_registry, bind, get_keyboard].concat(),
            "client requests must forward intact"
        );
        assert!(echo_fds.is_empty());
        let (got, fds) = recv_exact(script_c.as_raw_fd(), 16);
        assert_eq!(got, keymap);
        assert_eq!(fds.len(), 1, "keymap must carry exactly one fd");

        drop(script_c);
        drop(script_h);
        c2h.join().unwrap();
        h2c.join().unwrap();
    }
}
