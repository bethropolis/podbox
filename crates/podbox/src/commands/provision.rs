//! One-shot guest provisioning steps (`[[provision]]`).

mod runner;
mod state;
mod status;

pub use state::{ProvisionState, classify, plan};
// Retain the original facade surface for command and doctor callers.
#[allow(unused_imports)]
pub use state::{provision_dir, step_hash};
#[allow(unused_imports)]
pub use status::StatusRow;
pub use status::status;

use anyhow::Result;
use podbox::config::Config;
use podbox::env::HostEnv;

use runner::{execute, orphan_names, prune_orphans, run_step};

/// `provision sync`: ensure running, then run pending/stale (or forced) steps.
pub fn sync(
    config: &Config,
    env: &HostEnv,
    only: &[String],
    force: bool,
    dry_run: bool,
) -> Result<()> {
    if config.provision.is_empty() {
        anyhow::bail!(
            "no [[provision]] steps configured for '{}'",
            config.container.name
        );
    }
    if dry_run {
        let states = classify(config);
        let orphans = orphan_names(&states);
        if !orphans.is_empty() {
            println!(
                "Would prune {} orphaned stamp(s): {}",
                orphans.len(),
                orphans.join(", ")
            );
        }
        let steps = plan(config, &states, only, force)?;
        if steps.is_empty() {
            if orphans.is_empty() {
                println!("Nothing to do: all steps applied.");
            }
            return Ok(());
        }
        println!("Would run {} step(s):", steps.len());
        for step in &steps {
            // Print the exec line via run_step dry-run for fidelity.
            run_step(config, env, step, true)?;
        }
        return Ok(());
    }
    crate::commands::ensure_running(
        &config.container.name,
        false,
        crate::commands::DEFAULT_START_TIMEOUT_SECS,
    )?;
    let states = classify(config);
    let pruned = prune_orphans(config, &states);
    if pruned > 0 {
        println!("Pruned {pruned} orphaned provision stamp(s).");
    }
    let steps = plan(config, &states, only, force)?;
    if steps.is_empty() {
        if pruned == 0 {
            println!("Nothing to do: all steps applied.");
        }
        return Ok(());
    }
    execute(config, env, &steps, false, true)
}

/// Steps for `create`: dotfiles first (handled by caller), then these.
/// Lenient on `warn` (warning + continue), strict on `abort`.
pub fn run_for_create(config: &Config, env: &HostEnv, dry_run: bool) -> Result<()> {
    if config.provision.is_empty() {
        return Ok(());
    }
    let states = classify(config);
    let steps = plan(config, &states, &[], false)?;
    if steps.is_empty() {
        return Ok(());
    }
    if dry_run {
        for step in &steps {
            run_step(config, env, step, true)?;
        }
        return Ok(());
    }
    execute(config, env, &steps, false, false)
}
