//! Which layer of the host starts and stops a container.
//!
//! Quadlet containers are systemd user units: lifecycle verbs go through
//! `systemctl --user` so systemd tracks the transitions. Standalone
//! containers are driven with plain `podman`. That decision — "systemctl
//! if Quadlet is enabled and systemd is available, else podman" — used to
//! sit next to every verb with slightly different dry-run text and unit
//! naming at each site; [`Runtime`] owns it once.

use anyhow::Result;

use podbox::config::Config;
use podbox::process;
use podbox::systemd;

use crate::ctx::Ctx;

/// The layer that actually performs a start/stop/restart.
pub enum Runtime {
    /// The box is Quadlet-managed and systemctl exists: verbs run against
    /// `<name>.service` in the user session.
    Systemd,
    /// Standalone box: verbs call `podman` directly.
    Podman,
}

impl Runtime {
    /// Decide for a loaded config: through systemd when Quadlet is enabled
    /// for the box and systemctl exists, plain podman otherwise.
    pub fn for_config(cfg: &Config) -> Self {
        if cfg.lifecycle.quadlet && systemd::is_available() {
            Self::Systemd
        } else {
            Self::Podman
        }
    }

    /// Decide without a Quadlet config in play (`create` just staged the
    /// unit; whether systemd owns it depends only on systemctl being there).
    pub fn for_system() -> Self {
        if systemd::is_available() {
            Self::Systemd
        } else {
            Self::Podman
        }
    }

    /// Decide for a container whose config is gone: by what is installed.
    pub fn for_installed(name: &str) -> Self {
        if podbox::quadlet_install::is_installed(name) {
            Self::Systemd
        } else {
            Self::Podman
        }
    }

    /// Start the container of a freshly installed unit (the `create`
    /// finish-up; podbox just installed the unit, no wait loop needed).
    /// Like [`start_and_wait`](Self::start_and_wait), the legacy dry-run
    /// line is `podman start` even on the systemd side; under `--dry-run`
    /// `create` prints its own staged-flow lines and returns earlier.
    pub fn start(&self, ctx: &Ctx, cfg: &Config) -> Result<()> {
        if ctx.dry_run {
            println!("podman start {}", cfg.runtime_name());
            return Ok(());
        }
        match self {
            Self::Systemd => systemd::start_unit(cfg.runtime_name()),
            Self::Podman => Self::podman_verb("start", cfg.runtime_name()),
        }
    }

    /// Start a stopping/missing container and wait until it reports running.
    ///
    /// Systemd gets the friendly variant: its failure diagnostics name the
    /// journal, podman's come from the caller's own wait loop below.
    /// The legacy dry-run line stays `podman start` — the callers that can
    /// reach it with `ctx.dry_run` already print more context.
    pub fn start_and_wait(&self, ctx: &Ctx, name: &str, timeout_secs: u64) -> Result<()> {
        if ctx.dry_run {
            println!("podman start {name}");
            return Ok(());
        }
        match self {
            Self::Systemd => systemd::start_unit_friendly(name, timeout_secs),
            Self::Podman => {
                Self::podman_verb("start", name)?;
                wait_for_running(name, timeout_secs)
            }
        }
    }

    /// Stop the container (`podbox stop`).
    ///
    /// Uses `systemctl --user stop` when systemd manages the box so systemd
    /// tracks the transition (a plain podman stop would leave systemd
    /// reporting a stale state), `podman stop` otherwise.
    pub fn stop(&self, ctx: &Ctx, cfg: &Config) -> Result<()> {
        self.stop_named(ctx, cfg.runtime_name())
    }

    /// The body of [`stop`](Self::stop) for a box known only by name (its
    /// config failed to load, `stop --all` falls back).
    pub fn stop_named(&self, ctx: &Ctx, name: &str) -> Result<()> {
        if ctx.dry_run {
            println!("{}", self.describe("stop", name));
            return Ok(());
        }
        match self {
            Self::Systemd => systemd::stop_unit(name),
            Self::Podman => Self::podman_verb("stop", name),
        }
    }

    /// Restart the container (`podbox restart`, and the restart step of
    /// `update`).
    pub fn restart(&self, ctx: &Ctx, cfg: &Config) -> Result<()> {
        let name = cfg.runtime_name();
        if ctx.dry_run {
            println!("{}", self.describe_restart(name));
            return Ok(());
        }
        match self {
            Self::Systemd => {
                // reset-failed first so a previously failed unit actually
                // restarts instead of systemd refusing to start it again.
                systemd::reset_failed(name)?;
                systemd::restart_unit(name)
            }
            Self::Podman => Self::podman_verb("restart", name),
        }
    }

    /// The dry-run line for [`describe`-style verbs](Self::describe):
    /// `systemctl --user <action> <name>` or `podman <action> <name>`.
    pub fn describe(&self, action: &str, name: &str) -> String {
        match self {
            Self::Systemd => format!("systemctl --user {action} {name}"),
            Self::Podman => format!("podman {action} {name}"),
        }
    }

    fn describe_restart(&self, name: &str) -> String {
        // systemd restarts the unit, podman the container, so only one side
        // carries the `.service` suffix.
        match self {
            Self::Systemd => format!("systemctl --user restart {name}.service"),
            Self::Podman => format!("podman restart {name}"),
        }
    }

    /// Run a checked mutating `podman <verb> <name>`.
    fn podman_verb(verb: &str, name: &str) -> Result<()> {
        let args = process::args(&[verb, name]);
        process::spawn_interactive_checked("podman", &args)
    }
}

/// Poll until the container reaches `Running` state or the timeout expires.
fn wait_for_running(name: &str, timeout_secs: u64) -> Result<()> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match podbox::podman::query_state(name)? {
            podbox::podman::ContainerState::Running => return Ok(()),
            _ if Instant::now() >= deadline => {
                let state = podbox::podman::query_state(name)?;
                anyhow::bail!(
                    "Container '{name}' did not become ready within {timeout_secs}s \
                     (final state: {state:?})",
                );
            }
            _ => {
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
}
