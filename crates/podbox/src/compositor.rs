use std::collections::HashSet;
use std::io::{IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use nix::sys::socket::{recvmsg, sendmsg, ControlMessage, ControlMessageOwned, MsgFlags};

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
            let should_drop = !is_client_to_host
                && is_blocked_global(message_bytes, opcode, &state);

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
fn is_blocked_global(
    message_bytes: &[u8],
    opcode: u16,
    state: &Mutex<FirewallState>,
) -> bool {
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
        sendmsg::<()>(out_socket.as_raw_fd(), &iov, &[cmsg], MsgFlags::empty(), None)?;
        pending_fds.clear();
    }

    Ok(())
}
