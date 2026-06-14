use anyhow::Result;

use podbox::podman::{query_state, ContainerState};
use podbox::systemd;

pub mod clone;
pub mod context;
pub mod create;
pub mod definition;
pub mod diff;
pub mod export;
pub mod inspect;
pub mod lifecycle;
pub mod pull;
pub mod runtime;
pub mod serve;
pub mod stats;
pub mod translate;

pub const DEFAULT_START_TIMEOUT_SECS: u64 = 30;

/// Start a container if it isn't already running.
///
/// Uses `start_unit_friendly` from the systemd module when systemd is
/// available (friendly diagnostics on failure), falling back to
/// `podman start` for standalone containers.
pub fn ensure_running(name: &str, dry_run: bool, timeout_secs: u64) -> Result<()> {
    match query_state(name)? {
        ContainerState::Running => Ok(()),
        ContainerState::Stopped | ContainerState::Missing => {
            if dry_run {
                println!("podman start {}", name);
                return Ok(());
            }
            if systemd::is_available() {
                systemd::start_unit_friendly(name, timeout_secs)
            } else {
                let args = podbox::process::args(&["start", name]);
                podbox::process::spawn_interactive("podman", &args)?;
                wait_for_running(name, timeout_secs)
            }
        }
    }
}

/// Poll until the container reaches Running state or timeout.
fn wait_for_running(name: &str, timeout_secs: u64) -> Result<()> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match query_state(name)? {
            ContainerState::Running => return Ok(()),
            _ if Instant::now() >= deadline => {
                let state = query_state(name)?;
                anyhow::bail!(
                    "Container '{}' did not become ready within {}s \
                     (final state: {:?})",
                    name,
                    timeout_secs,
                    state,
                );
            }
            _ => {
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
}
