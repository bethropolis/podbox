//! systemd integration for podbox: unit control, status queries, and
//! diagnostics. `systemctl` helpers live in [`units`], status parsing and
//! diagnostics in [`status`].

mod status;
pub mod units;
pub use status::UnitStatus;
pub use units::{
    daemon_reload, enable_linger, enable_now_socket, is_available, is_unit_enabled, is_unit_failed,
    reset_failed, restart_unit, start_unit, stop_compositor, stop_socket_and_host, stop_unit,
};

use std::time::{Duration, Instant};

use anyhow::Result;

use crate::podman::{ContainerState, query_state};

use status::{diagnostic_card, journal_tail, query_unit_status};
use units::heal_missing_guest_socket;

const POLL_INTERVAL_MS: u64 = 300;

/// Start a container with friendly diagnostics on failure.
///
/// Checks for `NeedDaemonReload` and auto-fixes it. If the start fails,
/// queries systemd and journalctl to build a diagnostic card for the user.
pub fn start_unit_friendly(name: &str, timeout_secs: u64) -> Result<()> {
    if !is_available() {
        anyhow::bail!("systemctl not available");
    }

    // Check if daemon-reload is needed first
    match query_unit_status(name) {
        Ok(status) if status.need_daemon_reload => {
            tracing::info!("systemd needs reload — running daemon-reload...");
            daemon_reload()?;
        }
        Ok(_) => {}
        Err(_) => {
            // Unit might not exist yet — that's fine, we're about to try starting.
        }
    }

    // Clear any previous failure so a unit that landed in `failed` (e.g. from
    // an idle stop or a transient error) can be started again without the
    // user having to run `systemctl --user reset-failed` manually.
    reset_failed(name)?;

    // Self-heal: if the guest socket file vanished while its unit stayed
    // active, rebind it before starting — otherwise podman fails with
    // `statfs .../podbox/<name>.sock: no such file or directory`.
    let _ = heal_missing_guest_socket(name);

    let attempt = || -> Result<()> {
        start_unit(name)?;
        wait_for_running(name, timeout_secs)
    };

    let mut start_result = attempt();

    if start_result.is_err() {
        // One retry: a socket that went missing mid-start gets rebound first.
        if let Ok(true) = heal_missing_guest_socket(name) {
            eprintln!("Retrying start after socket rebind...");
            reset_failed(name)?;
            start_result = attempt();
        }
    }

    match start_result {
        Ok(()) => Ok(()),
        Err(_) => {
            // Gather diagnostics
            let status = query_unit_status(name).unwrap_or_default();
            let journal = journal_tail(name, 10).ok();
            let card = diagnostic_card(name, &status, journal.as_deref());
            eprintln!("{card}");
            anyhow::bail!("container '{name}' failed to start");
        }
    }
}

/// Poll until the container reaches Running state or timeout.
fn wait_for_running(name: &str, timeout_secs: u64) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match query_state(name)? {
            ContainerState::Running => return Ok(()),
            _ if Instant::now() >= deadline => {
                let state = query_state(name)?;
                anyhow::bail!(
                    "container '{name}' did not become ready within {timeout_secs}s (final state: {state:?})",
                );
            }
            _ => {
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
        }
    }
}
