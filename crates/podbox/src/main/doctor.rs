use anyhow::Result;

use super::super::ctx::Ctx;
use super::super::resolve::{self, ConfigResolveError};
use crate::commands;
use podbox::cli::{Cli, OutputFormat};
use podbox::ui;

/// `doctor` resolves the definition itself: a broken one must reach the
/// doctor's `--fix` offer instead of aborting the dispatch in shared
/// resolution. This is the plain-`resolve_config` flow under those rules.
pub(super) fn dispatch_doctor(
    ctx: &Ctx,
    cli: &Cli,
    target: Option<String>,
    fix: bool,
    yes: bool,
    output: OutputFormat,
) -> Result<()> {
    // `--fix` writes config and units; under --dry-run it only reports.
    let apply_fix = fix && !ctx.dry_run;
    if fix && ctx.dry_run {
        ui::warn("--fix is skipped under --dry-run; reporting only");
    }
    match resolve::resolve_config(cli, target.clone()) {
        Ok((config, _)) => {
            let env = podbox::env::resolve()?;
            commands::runtime::run_doctor(&config, &env, apply_fix, yes, output)
        }
        Err(err) => {
            // The only resolution failure doctor can repair in place is a
            // bare `container.memory` (typed match, not message text).
            if apply_fix
                && err.is_bare_memory()
                && commands::runtime::try_fix_bare_memory_for_target(
                    target.as_deref(),
                    cli.config.as_deref(),
                    apply_fix,
                    yes,
                )?
            {
                let (config, _) = resolve::resolve_config(cli, target)
                    .map_err(ConfigResolveError::into_report)?;
                let env = podbox::env::resolve()?;
                return commands::runtime::run_doctor(&config, &env, apply_fix, yes, output);
            }
            Err(err.into_report())
        }
    }
}
