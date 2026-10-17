//! Host-side Wayland compositor bridge: socket setup and the accept loop
//! that hands each accepted connection to [`bridge`]. Interface filtering
//! and rate limiting live in [`firewall`]; the per-connection byte-stream
//! bridge and its `SCM_RIGHTS` fd-attribution rules live in [`bridge`].

use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// Register SIGTERM/SIGINT handlers via the shared sysutil dead-simple flag.
fn setup_signal_handler() -> Result<Arc<AtomicBool>> {
    Ok(crate::protocol::sysutil::install_shutdown_signal_handler())
}

use crate::config::Config;

const MAX_CONNECTIONS: usize = 128;

mod bridge;
mod firewall;
mod tracker;

use bridge::bridge_loop;
use firewall::FirewallState;

/// Run the Wayland firewall proxy for a container.
///
/// Listens on `$XDG_RUNTIME_DIR/podbox/{name}-wayland.sock`, accepts
/// connections from the container, bridges each to the host compositor's
/// Wayland socket, and filters blocked interfaces from `wl_registry::global`
/// events on the host→client path.
pub fn run_compositor(config: &Config, name: &str) -> Result<()> {
    // The old code had a `.context("XDG_RUNTIME_DIR not set")` here, but the
    // `or_else` always supplied the uid fallback, so the context was
    // unreachable; the shared helper is exactly that behaviour.
    let xdg_runtime = crate::protocol::sysutil::xdg_runtime_dir();

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

    let shutdown = setup_signal_handler()?;

    let listener = UnixListener::bind(&socket_path).with_context(|| {
        format!(
            "Failed to bind Wayland proxy socket at {}",
            socket_path.display()
        )
    })?;

    serve_forever(
        listener,
        &host_socket,
        config.wayland.blocked_interfaces.clone(),
        &shutdown,
        MAX_CONNECTIONS,
    )
}

/// Accept loop behind [`run_compositor`], split out so tests can drive it
/// with a fake host compositor and a small connection limit.
///
/// `live` counts connections whose bridge threads are still running.
/// Reaching `max` refuses the newcomer (accept-and-close with a
/// rate-limited log line) instead of stopping the loop: short-lived
/// clients must not exhaust a lifetime budget.
fn serve_forever(
    listener: UnixListener,
    host_socket: &Path,
    blocked: Vec<String>,
    shutdown: &AtomicBool,
    max: usize,
) -> Result<()> {
    // Non-blocking + periodic tick so SIGTERM/SIGINT ends the accept loop
    // promptly instead of blocking in accept(2) until systemd's
    // TimeoutStopSec SIGKILL (90s stall on every container stop).
    listener.set_nonblocking(true)?;

    let live = Arc::new(AtomicUsize::new(0));
    let mut last_refused_log: Option<Instant> = None;

    loop {
        if shutdown.load(Ordering::Relaxed) {
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

        if live.load(Ordering::SeqCst) >= max {
            // Refuse, don't stop: drop `stream` to close the connection.
            if last_refused_log.is_none_or(|t| t.elapsed() >= Duration::from_secs(5)) {
                tracing::warn!("compositor: refusing connection, {max} live already");
                last_refused_log = Some(Instant::now());
            }
            continue;
        }

        stream.set_nonblocking(false)?;

        let host_conn = match UnixStream::connect(host_socket) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("compositor: failed to connect to host Wayland socket: {e}");
                continue;
            }
        };

        let state = Arc::new(Mutex::new(FirewallState::new(blocked.clone())));
        let done = Arc::new(AtomicBool::new(false));

        // Reserve the slot before spawning: a bridge thread that finishes
        // instantly must never decrement a counter that was not incremented
        // yet (that would underflow it and refuse everything after).
        live.fetch_add(1, Ordering::SeqCst);
        let client_clone = match stream.try_clone() {
            Ok(clone) => clone,
            Err(e) => {
                live.fetch_sub(1, Ordering::SeqCst);
                return Err(e.into());
            }
        };
        let host_clone = match host_conn.try_clone() {
            Ok(clone) => clone,
            Err(e) => {
                live.fetch_sub(1, Ordering::SeqCst);
                return Err(e.into());
            }
        };

        // Decremented by whichever bridge direction finishes last, so the
        // slot frees exactly when the connection is fully torn down.
        let remaining = Arc::new(AtomicUsize::new(2));
        let live_c2h = Arc::clone(&live);
        let remaining_c2h = Arc::clone(&remaining);
        let state_c2h = Arc::clone(&state);
        let done_c2h = Arc::clone(&done);

        std::thread::spawn(move || {
            if let Err(e) = bridge_loop(stream, host_clone, state_c2h, &done_c2h, true) {
                tracing::error!("compositor: client→host bridge error: {e}");
            }
            done_c2h.store(true, Ordering::Relaxed);
            if remaining_c2h.fetch_sub(1, Ordering::SeqCst) == 1 {
                live_c2h.fetch_sub(1, Ordering::SeqCst);
            }
        });

        let state_h2c = state;
        let done_h2c = done;
        let live_h2c = Arc::clone(&live);

        std::thread::spawn(move || {
            if let Err(e) = bridge_loop(host_conn, client_clone, state_h2c, &done_h2c, false) {
                tracing::error!("compositor: host→client bridge error: {e}");
            }
            done_h2c.store(true, Ordering::Relaxed);
            if remaining.fetch_sub(1, Ordering::SeqCst) == 1 {
                live_h2c.fetch_sub(1, Ordering::SeqCst);
            }
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Instant;

    use super::firewall::{
        FirewallState, forbidden_bind, is_blocked_global, rate_allow, track_client_request,
    };

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

    /// `wl_display.get_registry(new_id)`, the client message that creates a
    /// registry the firewall will watch.
    fn get_registry_msg(new_id: u32) -> Vec<u8> {
        let mut m = make_message(1, 12, 1);
        m[8..12].copy_from_slice(&new_id.to_ne_bytes());
        m
    }

    /// `wl_registry.bind(name, interface, version, new_id)` from client object `obj`.
    fn bind_msg(obj: u32, name: u32, iface: &str, new_id: u32) -> Vec<u8> {
        let mut args = Vec::new();
        args.extend_from_slice(&name.to_ne_bytes());
        let raw = iface.as_bytes();
        args.extend_from_slice(&u32::try_from(raw.len() + 1).unwrap().to_ne_bytes());
        args.extend_from_slice(raw);
        args.push(0);
        while args.len() % 4 != 0 {
            args.push(0);
        }
        args.extend_from_slice(&1u32.to_ne_bytes());
        args.extend_from_slice(&new_id.to_ne_bytes());
        let size = u32::try_from(8 + args.len()).unwrap();
        let mut buf = obj.to_ne_bytes().to_vec();
        buf.extend_from_slice(&(size << 16).to_ne_bytes());
        buf.extend_from_slice(&args);
        buf
    }

    /// A firewall with `blocked` interfaces and registry object 2, created
    /// through the real `get_registry` path.
    fn state_with(blocked: Vec<String>) -> Mutex<FirewallState> {
        let st = Mutex::new(FirewallState::new(blocked));
        track_client_request(&st, 1, 1, &get_registry_msg(2));
        st
    }

    fn blocked_state() -> Mutex<FirewallState> {
        state_with(vec![
            "zwlr_screencopy_manager_v1".into(),
            "ext_foreign_toplevel_list_v1".into(),
        ])
    }

    fn empty_state() -> Mutex<FirewallState> {
        state_with(vec![])
    }

    #[test]
    fn blocks_screencopy_interface() {
        let data = make_global(2, 42, "zwlr_screencopy_manager_v1", 1);
        assert!(is_blocked_global(&data, 2, 0, &blocked_state()));
    }

    #[test]
    fn blocks_foreign_toplevel() {
        let data = make_global(2, 43, "ext_foreign_toplevel_list_v1", 1);
        assert!(is_blocked_global(&data, 2, 0, &blocked_state()));
    }

    #[test]
    fn allows_safe_interface() {
        let data = make_global(2, 44, "wl_compositor", 6);
        assert!(!is_blocked_global(&data, 2, 0, &blocked_state()));
    }

    #[test]
    fn allows_wl_shm() {
        let data = make_global(2, 1, "wl_shm", 1);
        assert!(!is_blocked_global(&data, 2, 0, &blocked_state()));
    }

    #[test]
    fn blocks_nothing_when_empty_blocklist() {
        let data = make_global(2, 42, "zwlr_screencopy_manager_v1", 1);
        assert!(!is_blocked_global(&data, 2, 0, &empty_state()));
    }

    #[test]
    fn ignores_non_registry_opcode() {
        let data = make_message(2, 16, 1);
        assert!(!is_blocked_global(&data, 2, 1, &blocked_state()));
    }

    #[test]
    fn ignores_short_payload() {
        let data = make_message(2, 12, 0);
        assert!(!is_blocked_global(&data, 2, 0, &blocked_state()));
    }

    #[test]
    fn ignores_empty_interface_string() {
        let mut data = make_message(2, 16, 0);
        data[12..16].copy_from_slice(&0u32.to_ne_bytes());
        assert!(!is_blocked_global(&data, 2, 0, &blocked_state()));
    }

    #[test]
    fn allows_partial_name_prefix_match() {
        let data = make_global(2, 42, "zwlr_screencopy", 1);
        assert!(!is_blocked_global(&data, 2, 0, &blocked_state()));
    }

    #[test]
    fn allows_similar_but_not_blocked() {
        let data = make_global(2, 99, "zwlr_layer_shell_v1", 1);
        assert!(!is_blocked_global(&data, 2, 0, &blocked_state()));
    }

    #[test]
    fn opcode_zero_on_a_non_registry_object_is_not_a_global() {
        let data = make_global(9, 42, "zwlr_screencopy_manager_v1", 1);
        assert!(!is_blocked_global(&data, 9, 0, &blocked_state()));
    }

    #[test]
    fn bind_to_a_hidden_global_name_is_refused() {
        let st = blocked_state();
        let global = make_global(2, 42, "zwlr_screencopy_manager_v1", 1);
        assert!(is_blocked_global(&global, 2, 0, &st));
        // The client guesses the name and binds anyway.
        let bind = bind_msg(2, 42, "zwlr_screencopy_manager_v1", 9);
        assert!(forbidden_bind(&bind, 2, 0, &st).is_some());
    }

    #[test]
    fn allowed_name_with_a_blocked_interface_string_is_refused() {
        let st = blocked_state();
        // Name 3 was never hidden, but the interface it carries is blocked.
        let bind = bind_msg(2, 3, "zwlr_screencopy_manager_v1", 9);
        assert!(forbidden_bind(&bind, 2, 0, &st).is_some());
    }

    #[test]
    fn allowed_bind_is_not_refused() {
        let st = blocked_state();
        let bind = bind_msg(2, 1, "wl_shm", 9);
        assert!(forbidden_bind(&bind, 2, 0, &st).is_none());
    }

    #[test]
    fn bind_on_a_non_registry_object_is_not_checked() {
        let st = blocked_state();
        let bind = bind_msg(9, 42, "zwlr_screencopy_manager_v1", 10);
        assert!(forbidden_bind(&bind, 9, 0, &st).is_none());
    }

    #[test]
    fn global_remove_for_a_hidden_name_is_hidden_too() {
        let st = blocked_state();
        assert!(is_blocked_global(
            &make_global(2, 42, "zwlr_screencopy_manager_v1", 1),
            2,
            0,
            &st
        ));
        let mut remove = make_message(2, 12, 1);
        remove[8..12].copy_from_slice(&42u32.to_ne_bytes());
        assert!(is_blocked_global(&remove, 2, 1, &st));
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

    // ── serve_forever accept-loop tests ──

    use std::io::{Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};
    use std::time::Duration;

    /// One `wl_display.get_registry` message: complete enough for the
    /// bridge to forward, small enough to stay in one read.
    fn registry_request_msg() -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&1u32.to_ne_bytes());
        buf.extend_from_slice(&((12u32 << 16) | 1).to_ne_bytes());
        buf.extend_from_slice(&0u32.to_ne_bytes());
        buf
    }

    /// 500 sequential short-lived connections must all be served. With a
    /// lifetime (never decremented) counter the loop would stop after 128:
    /// later connects would be refused and this test would fail fast on
    /// the refused `connect`.
    ///
    /// Connections go out in waves paced by the fake host's receive
    /// counter, so the kernel backlog never fills; each client sends one
    /// message and closes, and the closing handshake tears the slot down.
    /// A final synchronized round trip proves the loop is still alive.
    #[test]
    fn sequential_short_lived_connections_never_exhaust_the_budget() {
        const N: usize = 500;
        const WAVE: usize = 50;
        let dir = tempfile::tempdir().unwrap();
        let proxy_path = dir.path().join("proxy.sock");
        let host_path = dir.path().join("host.sock");
        let proxy_listener = UnixListener::bind(&proxy_path).unwrap();
        let host_listener = UnixListener::bind(&host_path).unwrap();

        let forwarded = Arc::new(AtomicUsize::new(0));
        // Fake host: read one message per client, then close. The close
        // propagates EOF back through both bridge directions, tearing the
        // slot down without any per-connection wait here. It serves the N
        // wave connections plus the final probe. The handle is dropped
        // (detached) on purpose: if the test fails halfway, no join can
        // block on this thread's exact-count blocking `accept`.
        let forwarded_host = Arc::clone(&forwarded);
        drop(std::thread::spawn(move || {
            for _ in 0..=N {
                let Ok((mut conn, _)) = host_listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 12];
                let _ = conn.read_exact(&mut buf);
                forwarded_host.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }));
        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_serve = Arc::clone(&shutdown);
        let serve = std::thread::spawn(move || {
            super::serve_forever(
                proxy_listener,
                &host_path,
                vec![],
                &shutdown_serve,
                super::MAX_CONNECTIONS,
            )
            .unwrap();
        });

        let msg = registry_request_msg();
        let mut sent = 0;
        while sent < N {
            let wave = WAVE.min(N - sent);
            for _ in 0..wave {
                let mut client = UnixStream::connect(&proxy_path).unwrap();
                client.write_all(&msg).unwrap();
            }
            sent += wave;
            // Pace the next wave on the server's actual throughput.
            let began = Instant::now();
            while forwarded.load(AtomicOrdering::SeqCst) < sent {
                assert!(
                    began.elapsed() < Duration::from_secs(30),
                    "server stalled after {} of {N} connections",
                    forwarded.load(AtomicOrdering::SeqCst),
                );
                std::thread::sleep(Duration::from_millis(20));
            }
        }

        // The loop must still be alive after all 500: a full round
        // trip with an EOF close from the fake host.
        let mut probe = UnixStream::connect(&proxy_path).unwrap();
        probe
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        probe.write_all(&msg).unwrap();
        let mut buf = [0u8; 64];
        loop {
            match probe.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) => panic!("final probe: {e}"),
            }
        }
        shutdown.store(true, AtomicOrdering::Relaxed);
        serve.join().expect("accept loop panicked");
        assert_eq!(forwarded.load(AtomicOrdering::SeqCst), N + 1);
    }

    /// While `max` connections are live, the next one is refused (prompt
    /// EOF, not a hang and not a stopped loop); after one closes, a new
    /// connection is admitted again.
    #[test]
    fn connection_over_the_live_limit_is_refused_and_slots_are_reused() {
        let dir = tempfile::tempdir().unwrap();
        let proxy_path = dir.path().join("proxy.sock");
        let host_path = dir.path().join("host.sock");
        let proxy_listener = UnixListener::bind(&proxy_path).unwrap();
        let host_listener = UnixListener::bind(&host_path).unwrap();
        host_listener.set_nonblocking(true).unwrap();

        // Fake host: hold every proxied connection open until the client
        // goes away, and count accepted connections so the test can wait
        // for both slots to be live.
        let accepted = Arc::new(AtomicUsize::new(0));
        let host_done = Arc::new(AtomicBool::new(false));
        let host_worker = {
            let accepted = Arc::clone(&accepted);
            let host_done = Arc::clone(&host_done);
            std::thread::spawn(move || {
                while !host_done.load(AtomicOrdering::Relaxed) {
                    match host_listener.accept() {
                        Ok((mut conn, _)) => {
                            accepted.fetch_add(1, AtomicOrdering::SeqCst);
                            std::thread::spawn(move || {
                                let mut buf = [0u8; 1024];
                                while matches!(conn.read(&mut buf), Ok(n) if n > 0) {}
                            });
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(_) => break,
                    }
                }
            })
        };

        let shutdown = Arc::new(AtomicBool::new(false));
        let shutdown_serve = Arc::clone(&shutdown);
        let serve_worker = std::thread::spawn(move || {
            super::serve_forever(proxy_listener, &host_path, vec![], &shutdown_serve, 2).unwrap();
        });

        let held1 = UnixStream::connect(&proxy_path).unwrap();
        let held2 = UnixStream::connect(&proxy_path).unwrap();
        let began = Instant::now();
        while accepted.load(AtomicOrdering::SeqCst) < 2 {
            assert!(
                began.elapsed() < Duration::from_secs(10),
                "proxy never connected both clients upstream"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        // The slot increment lands microseconds after the upstream
        // connect; let the bridge threads settle before probing.
        std::thread::sleep(Duration::from_millis(200));

        // Two live: the third connection is refused with a prompt EOF.
        let mut refused = UnixStream::connect(&proxy_path).unwrap();
        refused
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buf = [0u8; 1];
        assert_eq!(
            refused.read(&mut buf).unwrap(),
            0,
            "over-limit connection was not refused"
        );
        drop(refused);

        // Free one slot; a new connection is admitted again (poll: bridge
        // teardown after the client EOF is asynchronous).
        drop(held1);
        let began = Instant::now();
        let replacement = loop {
            let mut candidate = UnixStream::connect(&proxy_path).unwrap();
            candidate
                .set_read_timeout(Some(Duration::from_millis(300)))
                .unwrap();
            match candidate.read(&mut buf) {
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break candidate;
                }
                _ => {
                    assert!(
                        began.elapsed() < Duration::from_secs(10),
                        "freed slot was never reused"
                    );
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        };

        drop(held2);
        drop(replacement);
        shutdown.store(true, AtomicOrdering::Relaxed);
        serve_worker.join().unwrap();
        host_done.store(true, AtomicOrdering::Relaxed);
        host_worker.join().unwrap();
    }
}
