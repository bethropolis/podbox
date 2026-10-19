use anyhow::Result;

use podbox::podman::{ContainerState, query_state};

pub mod cache;
pub mod clone;
pub mod config;
pub mod context;
pub mod create;
pub mod create_init;
pub mod definition;
pub mod diff;
pub mod dotfiles;
pub mod export;
pub mod history;
pub mod info;
pub mod inspect;
pub mod lifecycle;
pub mod list;
pub mod migrate;
pub mod provision;
pub mod pull;
pub mod recover;
pub mod runner;
pub mod runtime;
pub mod serve;
pub mod stats;
pub mod translate;
pub mod watch;

pub const DEFAULT_START_TIMEOUT_SECS: u64 = 30;

/// Start a container if it isn't already running.
///
/// Delegates the "systemd if available, else podman" decision to
/// [`runner::Runtime`]: friendly systemd diagnostics on failure, `podman
/// start` plus a wait loop for standalone containers.
pub fn ensure_running(name: &str, dry_run: bool, timeout_secs: u64) -> Result<()> {
    match query_state(name)? {
        ContainerState::Running => Ok(()),
        ContainerState::Stopped | ContainerState::Missing => {
            let ctx = crate::ctx::Ctx { dry_run };
            runner::Runtime::for_system().start_and_wait(&ctx, name, timeout_secs)
        }
    }
}
