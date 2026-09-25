//! Host-side Wayland compositor bridge: socket setup and the accept loop
//! that hands each accepted connection to [`bridge`]. Interface filtering
//! and rate limiting live in [`firewall`]; the per-connection byte-stream
//! bridge and its `SCM_RIGHTS` fd-attribution rules live in [`bridge`].

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Context, Result};

/// Register SIGTERM/SIGINT handlers that set `shutdown`.
fn setup_signal_handler(shutdown: Arc<AtomicBool>) -> Result<()> {
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(sig, Arc::clone(&shutdown))?;
    }
    Ok(())
}

use crate::config::Config;

const MAX_CONNECTIONS: usize = 128;

mod bridge;
mod firewall;

use bridge::bridge_loop;
use firewall::FirewallState;

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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Instant;

    use super::firewall::{FirewallState, is_blocked_global, rate_allow};

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
}
