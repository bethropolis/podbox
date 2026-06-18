use std::collections::HashSet;
use std::io::{IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};
use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, recvmsg, sendmsg};

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

    let listener = UnixListener::bind(&socket_path).with_context(|| {
        format!(
            "Failed to bind Wayland proxy socket at {}",
            socket_path.display()
        )
    })?;

    let blocked = config.wayland.blocked_interfaces.clone();

    for stream in listener.incoming().flatten().take(MAX_CONNECTIONS) {
        let host_conn = match UnixStream::connect(&host_socket) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("podbox-compositor: failed to connect to host Wayland socket: {e}");
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
                eprintln!("podbox-compositor: client→host bridge error: {e}");
            }
            done_c2h.store(true, Ordering::Relaxed);
        });

        let state_h2c = state;
        let done_h2c = done;

        std::thread::spawn(move || {
            if let Err(e) = bridge_loop(host_conn, client_clone, state_h2c, &done_h2c, false) {
                eprintln!("podbox-compositor: host→client bridge error: {e}");
            }
            done_h2c.store(true, Ordering::Relaxed);
        });
    }

    Ok(())
}

/// Bidirectional byte-stream bridge between two Unix sockets.
///
/// For the host→client direction, `is_client_to_host = false`, and the
/// bridge intercepts `wl_registry::global` events (opcode 0, string
/// payload at offset 12) to filter interfaces on the blocklist.
///
/// File descriptors received via `SCM_RIGHTS` are forwarded with the
/// first Wayland message from the same `recvmsg` batch.
fn bridge_loop(
    in_socket: UnixStream,
    out_socket: UnixStream,
    state: Arc<Mutex<FirewallState>>,
    done: &AtomicBool,
    is_client_to_host: bool,
) -> Result<()> {
    // Set a read timeout so that recvmsg periodically unblocks and checks
    // the `done` flag. Without this, a thread blocked on recvmsg would
    // never notice the opposite direction has failed.
    in_socket.set_read_timeout(Some(Duration::from_millis(100)))?;

    let mut read_buf = [0u8; 16384];
    let mut cmsg_buffer = vec![0u8; 4096];
    let mut bytes_cache = Vec::with_capacity(32768);
    let mut pending_fds: Vec<OwnedFd> = Vec::new();

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
                Err(e) if e == nix::errno::Errno::EAGAIN => {
                    if done.load(Ordering::Relaxed) {
                        break;
                    }
                    continue;
                }
                Err(e) => return Err(e.into()),
            };

            let bytes = msg.bytes;
            if bytes == 0 {
                break;
            }

            if let Ok(cmsgs) = msg.cmsgs() {
                for cmsg in cmsgs {
                    if let ControlMessageOwned::ScmRights(fds) = cmsg {
                        for fd in fds {
                            // SAFETY: fds received via SCM_RIGHTS are owned by the receiver.
                            let owned = unsafe { OwnedFd::from_raw_fd(fd) };
                            pending_fds.push(owned);
                        }
                    }
                }
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

            if msg_size < 8 || consumed + msg_size > bytes_cache.len() {
                break;
            }

            let message_bytes = &bytes_cache[consumed..consumed + msg_size];
            let should_drop =
                !is_client_to_host && is_blocked_global(message_bytes, opcode, &state);

            if !should_drop {
                forward_message(&out_socket, message_bytes, &mut pending_fds)?;
            } else {
                // Close fds belonging to the dropped message.
                pending_fds.clear();
            }

            consumed += msg_size;
        }

        bytes_cache.drain(..consumed);
    }

    Ok(())
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
    if str_len < 2 || message_bytes.len() < 16 + str_len {
        return false;
    }

    // Exclude the null terminator at the end.
    let interface_bytes = &message_bytes[16..16 + str_len - 1];
    let interface_name = match std::str::from_utf8(interface_bytes) {
        Ok(s) => s,
        Err(_) => return false,
    };

    let guard = state.lock().unwrap();
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
        let msg_size = (8 + 4 + 4 + padded_len + 4) as u32;

        let mut buf = Vec::with_capacity(msg_size as usize);
        buf.extend_from_slice(&object_id.to_ne_bytes());
        buf.extend_from_slice(&((msg_size << 16) | 0).to_ne_bytes());
        buf.extend_from_slice(&name.to_ne_bytes());
        buf.extend_from_slice(&(str_len as u32).to_ne_bytes());
        buf.extend_from_slice(raw);
        buf.push(0);
        while buf.len() < (8 + 4 + 4 + padded_len) as usize {
            buf.push(0);
        }
        buf.extend_from_slice(&version.to_ne_bytes());
        buf
    }

    fn make_message(object_id: u32, size: u32, opcode: u16) -> Vec<u8> {
        let mut buf = Vec::with_capacity(size as usize);
        buf.extend_from_slice(&object_id.to_ne_bytes());
        buf.extend_from_slice(&((size << 16) | opcode as u32).to_ne_bytes());
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
}
