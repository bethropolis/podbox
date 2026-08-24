use std::collections::{HashSet, VecDeque};
use std::io::{IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, recvmsg, sendmsg};

/// Register SIGTERM/SIGINT handlers that set `shutdown`.
fn setup_signal_handler(shutdown: Arc<AtomicBool>) -> Result<()> {
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(sig, Arc::clone(&shutdown))?;
    }
    Ok(())
}

use crate::config::Config;

const MAX_CONNECTIONS: usize = 128;

struct FirewallState {
    blocked_interfaces: HashSet<String>,
}

impl FirewallState {
    fn new(blocked_interfaces: Vec<String>) -> Self {
        Self {
            blocked_interfaces: blocked_interfaces.into_iter().collect(),
        }
    }
}

/// Run the Wayland firewall proxy for a container.
///
/// Listens on `$XDG_RUNTIME_DIR/podbox/{name}-wayland.sock`, accepts
/// connections from the container, bridges each to the host compositor's
/// Wayland socket, and filters blocked interfaces from `wl_registry::global`
/// events on the host→client path.
pub fn run_compositor(config: &Config, name: &str) -> Result<()> {
    let xdg_runtime = std::env::var("XDG_RUNTIME_DIR")
        .or_else(|_| {
            let uid = nix::unistd::getuid().as_raw();
            Ok::<_, std::env::VarError>(format!("/run/user/{uid}"))
        })
        .context("XDG_RUNTIME_DIR not set")?;

    let wayland_display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    let host_socket = Path::new(&xdg_runtime).join(&wayland_display);

    if !host_socket.exists() {
        anyhow::bail!(
            "Host Wayland socket not found at {} (WAYLAND_DISPLAY={})",
            host_socket.display(),
            wayland_display
        );
    }

    let socket_path = Path::new(&xdg_runtime)
        .join("podbox")
        .join(format!("{name}-wayland.sock"));

    let _ = std::fs::remove_file(&socket_path);
    std::fs::create_dir_all(socket_path.parent().context("socket path has no parent")?)?;

    let shutdown = Arc::new(AtomicBool::new(false));
    setup_signal_handler(Arc::clone(&shutdown))?;

    let listener = UnixListener::bind(&socket_path).with_context(|| {
        format!(
            "Failed to bind Wayland proxy socket at {}",
            socket_path.display()
        )
    })?;
    // Non-blocking + periodic tick so SIGTERM/SIGINT ends the accept loop
    // promptly instead of blocking in accept(2) until systemd's
    // TimeoutStopSec SIGKILL (90s stall on every container stop).
    listener.set_nonblocking(true)?;

    let blocked = config.wayland.blocked_interfaces.clone();

    let mut connections = 0;
    loop {
        if shutdown.load(Ordering::Relaxed) || connections >= MAX_CONNECTIONS {
            break;
        }

        let stream = match listener.accept() {
            Ok((s, _)) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => {
                tracing::error!("compositor: accept failed: {e}");
                break;
            }
        };
        connections += 1;

        stream.set_nonblocking(false)?;

        let host_conn = match UnixStream::connect(&host_socket) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("compositor: failed to connect to host Wayland socket: {e}");
                continue;
            }
        };

        let state = Arc::new(Mutex::new(FirewallState::new(blocked.clone())));
        let done = Arc::new(AtomicBool::new(false));

        let client_clone = stream.try_clone()?;
        let host_clone = host_conn.try_clone()?;
        let state_c2h = Arc::clone(&state);
        let done_c2h = Arc::clone(&done);

        std::thread::spawn(move || {
            if let Err(e) = bridge_loop(stream, host_clone, state_c2h, &done_c2h, true) {
                tracing::error!("compositor: client→host bridge error: {e}");
            }
            done_c2h.store(true, Ordering::Relaxed);
        });

        let state_h2c = state;
        let done_h2c = done;

        std::thread::spawn(move || {
            if let Err(e) = bridge_loop(host_conn, client_clone, state_h2c, &done_h2c, false) {
                tracing::error!("compositor: host→client bridge error: {e}");
            }
            done_h2c.store(true, Ordering::Relaxed);
        });
    }

    Ok(())
}

/// Token-bucket rate-limit check.
/// Returns `true` if the message is allowed through.
fn rate_allow(bucket: &mut f64, last_refill: &mut Instant) -> bool {
    const RATE: f64 = 10_000.0;
    let now = Instant::now();
    let elapsed = now.duration_since(*last_refill).as_secs_f64();
    *bucket = (*bucket + elapsed * RATE).min(RATE);
    *last_refill = now;
    if *bucket >= 1.0 {
        *bucket -= 1.0;
        true
    } else {
        false
    }
}

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
fn bridge_loop(
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
                            // SAFETY: fds received via SCM_RIGHTS are owned by the receiver.
                            let owned = unsafe { OwnedFd::from_raw_fd(fd) };
                            read_fds.push(owned);
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

/// Check whether a host→client message is a `wl_registry::global` event
/// announcing a blocked interface.
fn is_blocked_global(message_bytes: &[u8], opcode: u16, state: &Mutex<FirewallState>) -> bool {
    // wl_registry::global (opcode 0) format:
    //   8 bytes header (object_id, size+opcode=0)
    //   4 bytes name (u32)
    //   4 bytes interface string length (u32, includes NUL)
    //   N bytes interface string (padded to 4 bytes)
    //   4 bytes version (u32)
    if opcode != 0 || message_bytes.len() < 16 {
        return false;
    }

    let str_len = u32::from_ne_bytes(message_bytes[12..16].try_into().unwrap()) as usize;

    // Guard against integer overflow on 32-bit platforms
    if message_bytes
        .len()
        .checked_sub(16)
        .is_none_or(|rem| rem < str_len)
    {
        return false;
    }

    if str_len < 2 {
        return false;
    }

    // Exclude the null terminator at the end.
    let interface_bytes = &message_bytes[16..16 + str_len - 1];
    let interface_name = match std::str::from_utf8(interface_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let guard = state.lock().unwrap_or_else(|e| e.into_inner());
    guard.blocked_interfaces.contains(interface_name)
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

    fn make_global(object_id: u32, name: u32, interface: &str, version: u32) -> Vec<u8> {
        let raw = interface.as_bytes();
        let str_len = raw.len().checked_add(1).unwrap();
        let padded_len = str_len.next_multiple_of(4);
        let msg_size = u32::try_from(8 + 4 + 4 + padded_len + 4).unwrap();

        let mut buf = Vec::with_capacity(msg_size as usize);
        buf.extend_from_slice(&object_id.to_ne_bytes());
        buf.extend_from_slice(&(msg_size << 16).to_ne_bytes());
        buf.extend_from_slice(&name.to_ne_bytes());
        buf.extend_from_slice(&u32::try_from(str_len).unwrap().to_ne_bytes());
        buf.extend_from_slice(raw);
        buf.push(0);
        while buf.len() < (8 + 4 + 4 + padded_len) {
            buf.push(0);
        }
        buf.extend_from_slice(&version.to_ne_bytes());
        buf
    }

    fn make_message(object_id: u32, size: u32, opcode: u16) -> Vec<u8> {
        let mut buf = Vec::with_capacity(size as usize);
        buf.extend_from_slice(&object_id.to_ne_bytes());
        buf.extend_from_slice(&((size << 16) | u32::from(opcode)).to_ne_bytes());
        while buf.len() < size as usize {
            buf.push(0);
        }
        buf
    }

    fn blocked_state() -> Mutex<FirewallState> {
        Mutex::new(FirewallState::new(vec![
            "zwlr_screencopy_manager_v1".into(),
            "ext_foreign_toplevel_list_v1".into(),
        ]))
    }

    fn empty_state() -> Mutex<FirewallState> {
        Mutex::new(FirewallState::new(vec![]))
    }

    #[test]
    fn blocks_screencopy_interface() {
        let data = make_global(2, 42, "zwlr_screencopy_manager_v1", 1);
        assert!(is_blocked_global(&data, 0, &blocked_state()));
    }

    #[test]
    fn blocks_foreign_toplevel() {
        let data = make_global(2, 43, "ext_foreign_toplevel_list_v1", 1);
        assert!(is_blocked_global(&data, 0, &blocked_state()));
    }

    #[test]
    fn allows_safe_interface() {
        let data = make_global(2, 44, "wl_compositor", 6);
        assert!(!is_blocked_global(&data, 0, &blocked_state()));
    }

    #[test]
    fn allows_wl_shm() {
        let data = make_global(2, 1, "wl_shm", 1);
        assert!(!is_blocked_global(&data, 0, &blocked_state()));
    }

    #[test]
    fn blocks_nothing_when_empty_blocklist() {
        let data = make_global(2, 42, "zwlr_screencopy_manager_v1", 1);
        assert!(!is_blocked_global(&data, 0, &empty_state()));
    }

    #[test]
    fn ignores_non_registry_opcode() {
        let data = make_message(2, 16, 1);
        assert!(!is_blocked_global(&data, 1, &blocked_state()));
    }

    #[test]
    fn ignores_short_payload() {
        let data = make_message(2, 12, 0);
        assert!(!is_blocked_global(&data, 0, &blocked_state()));
    }

    #[test]
    fn ignores_empty_interface_string() {
        let mut data = make_message(2, 16, 0);
        data[12..16].copy_from_slice(&0u32.to_ne_bytes());
        assert!(!is_blocked_global(&data, 0, &blocked_state()));
    }

    #[test]
    fn allows_partial_name_prefix_match() {
        let data = make_global(2, 42, "zwlr_screencopy", 1);
        assert!(!is_blocked_global(&data, 0, &blocked_state()));
    }

    #[test]
    fn allows_similar_but_not_blocked() {
        let data = make_global(2, 99, "zwlr_layer_shell_v1", 1);
        assert!(!is_blocked_global(&data, 0, &blocked_state()));
    }

    #[test]
    fn rate_allow_accepts_first_message() {
        let mut bucket = 10_000.0;
        let mut last = Instant::now();
        assert!(rate_allow(&mut bucket, &mut last));
    }

    #[test]
    fn rate_allow_drains_bucket() {
        let mut bucket = 2.0;
        let mut last = Instant::now();
        assert!(rate_allow(&mut bucket, &mut last));
        assert!(rate_allow(&mut bucket, &mut last));
        assert!(!rate_allow(&mut bucket, &mut last));
    }

    #[test]
    fn rate_allow_refills_over_time() {
        let mut bucket = 0.0;
        let mut last = Instant::now();
        assert!(!rate_allow(&mut bucket, &mut last));
        // Simulate a small delay
        std::thread::sleep(std::time::Duration::from_millis(1));
        let mut bucket = 0.0;
        let mut last_refill = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        assert!(rate_allow(&mut bucket, &mut last_refill));
    }

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
