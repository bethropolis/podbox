use anyhow::{Context, Result};

use super::super::ctx::Ctx;
use super::super::handlers;
use super::super::resolve::{self, ConfigResolveError};
use crate::commands;
use podbox::cli::{CacheCommand, Cli, Command, ConfigCommand};

use super::doctor::dispatch_doctor;
use super::publish;

/// Run a command that does not go through shared definition resolution.
///
/// Handlers here either need no definition, or resolve the file themselves
/// because their error handling must survive a broken/unreadable config
/// (`config set`, `doctor`), or are flagged modes that must not depend on a
/// target that could be gone (`disable --force`, `remove --stale`,
/// `stop --all`).
pub(crate) fn dispatch_no_config(ctx: &Ctx, cli: &Cli, target: Option<String>) -> Result<()> {
    let dry = ctx.dry_run;
    match &cli.command {
        Command::InternalStdinWatchdog { parent_pid } => {
            commands::runtime::run_stdin_watchdog(*parent_pid)
        }

        Command::Completions { shell, abbrs } => {
            commands::definition::run_completions((*shell).into(), *abbrs)
        }

        Command::CompleteNames => commands::definition::run_complete_names(),

        Command::History {
            name,
            limit,
            output,
        } => commands::history::run_history(name.clone(), *limit, (*output).into()),

        Command::Cache { cache_cmd } => {
            let yes = matches!(cache_cmd, CacheCommand::Prune { yes, .. } if *yes);
            commands::cache::run(cache_cmd, dry, yes)
        }

        Command::Init {
            image,
            name,
            interactive,
            profile,
        } => commands::create::run_init(
            dry,
            image.as_deref(),
            name.as_deref(),
            *interactive,
            profile.as_deref(),
        ),

        Command::Create {
            image,
            import,
            name,
            packages,
            no_start,
            edit,
            yes,
        } => commands::create::run_create(
            dry,
            image.as_deref(),
            import.as_deref(),
            name.as_deref(),
            packages.as_deref(),
            *no_start,
            *edit,
            *yes,
        ),

        Command::Profile { profile_cmd } => handlers::run_profile_command(profile_cmd, dry),

        Command::List { output } => commands::definition::run_list((*output).into()),

        Command::Info { output } => commands::info::run_info((*output).into()),

        Command::Watch {
            name,
            interval,
            output,
        } => commands::watch::run_watch(
            name.clone().or_else(|| cli.container.clone()),
            cli.config.clone(),
            *interval,
            (*output).into(),
            dry,
        ),

        // Set/validate work on the file and do their own loading, so a
        // broken config still gets a precise report (or a fix) instead of
        // dying in shared resolution. `--config` wins when given, otherwise
        // `-C` / active context / auto-detect.
        Command::Config {
            config_cmd: ConfigCommand::Set { key, value, output },
        } => {
            let path = match &cli.config {
                Some(p) => p.clone(),
                None => resolve::resolve_config_path(cli.container.as_deref())
                    .map_err(ConfigResolveError::into_report)?,
            };
            commands::config::run_set(&path, key, value, dry, (*output).into())
        }
        Command::Config {
            config_cmd: ConfigCommand::Validate { name, output },
        } => {
            let target = name.as_deref().or(cli.container.as_deref());
            let path = match &cli.config {
                Some(p) => p.clone(),
                None => {
                    resolve::resolve_config_path(target).map_err(ConfigResolveError::into_report)?
                }
            };
            commands::config::run_validate(&path, (*output).into())
        }

        Command::Clone {
            src,
            dst,
            copy_home,
        } => commands::clone::run_clone(src, dst, *copy_home, dry),

        Command::Use { name, clear } => commands::context::run_use(name.clone(), *clear, dry),

        Command::Migrate { force } => {
            commands::migrate::run_migrate(commands::migrate::MigrateOpts {
                dry_run: dry,
                force: *force,
            })
        }

        Command::Edit { name: _, rebuild } => handlers::run_edit(dry, target.as_deref(), *rebuild),

        // Systemd runs these with the box name in the unit. They load that
        // config directly: generic resolution would ignore the name and
        // could fail on other boxes' configs (several configs, no `.active`).
        Command::Serve { name } => commands::serve::run_serve(cli.config.as_ref(), name, dry),

        Command::Compositor { name } => {
            let config_path = podbox::config::find_by_runtime_or_stem(name)
                .ok_or_else(|| podbox::config::missing_config_error(name))?;
            let config = podbox::config::Config::load(&config_path)?;
            podbox::compositor::run_compositor(&config, name)
        }

        Command::FindDefinition { .. } => {
            commands::definition::run_find_definition(target.as_deref())
        }

        Command::Disable { force: true, .. } => {
            let n = target
                .as_deref()
                .context("--force requires a container name (positional, -C, PODBOX_CONTAINER env, or active context)")?
                .to_string();
            commands::lifecycle::run_disable(&n, dry)
        }

        Command::Remove {
            stale: true, force, ..
        } => commands::lifecycle::run_remove_stale(dry, *force),

        // A plain remove needs only the facts a definition carries (home,
        // quadlet flag), not a valid definition: load leniently so legacy
        // definitions that fail strict validation can still be deleted.
        Command::Remove {
            name: _,
            home,
            force,
            remove_config,
            stale: false,
        } => {
            let (config, name) = resolve::resolve_config_lenient(cli, target)
                .map_err(ConfigResolveError::into_report)?;
            commands::lifecycle::run_remove(&config, &name, dry, *home, *force, *remove_config)
        }

        // `stop --all` needs no target: it enumerates running containers itself.
        Command::Stop { all: true, .. } => commands::lifecycle::run_stop_all(dry),

        Command::Doctor {
            name: _,
            fix,
            yes,
            output,
        } => dispatch_doctor(ctx, cli, target, *fix, *yes, (*output).into()),

        Command::Publish { .. } => publish::run_publish(cli, dry),

        // Mirror arms: these (arg-flagged) modes go through shared
        // resolution instead, see `needs_config`.
        Command::Disable { force: false, .. }
        | Command::Stop { all: false, .. }
        | Command::Config {
            config_cmd: ConfigCommand::Get { .. },
        }
        | Command::Build { .. }
        | Command::Enable { .. }
        | Command::Start { .. }
        | Command::Restart { .. }
        | Command::Exec { .. }
        | Command::Run { .. }
        | Command::Status { .. }
        | Command::Stats { .. }
        | Command::Logs { .. }
        | Command::Diff { .. }
        | Command::Snapshot { .. }
        | Command::Restore { .. }
        | Command::Rollback { .. }
        | Command::Inspect { .. }
        | Command::Export { .. }
        | Command::Update { .. }
        | Command::Pull { .. }
        | Command::Enter { .. }
        | Command::Dotfiles { .. }
        | Command::Provision { .. }
        | Command::Recover { .. }
        | Command::TranslatePath { .. } => Ok(()),
    }
}
