use anyhow::Result;

use super::super::ctx::Ctx;
use super::super::handlers;
use super::super::resolve::{self, ConfigResolveError};
use crate::commands;
use podbox::cli::{Cli, Command, ConfigCommand, DotfilesCommand, ProvisionCommand};

use super::edit::edit_config_for_command;

/// Whether the invoked command asked to open the definition in `$EDITOR`.
fn wants_edit(command: &Command) -> bool {
    matches!(
        command,
        Command::Build { edit: true, .. }
            | Command::Start { edit: true, .. }
            | Command::Enter { edit: true, .. }
    )
}

/// Run a command that needs one merged definition.
///
/// Resolution happens exactly once through [`resolve::resolve_config`] — the
/// single chain, with typed errors so `main`'s exit mapping keeps its codes.
pub(crate) fn dispatch_with_config(ctx: &Ctx, cli: &Cli, target: Option<String>) -> Result<()> {
    let dry = ctx.dry_run;
    // Apply `--edit` before resolving: the command must run against what the
    // user just saved, not the pre-edit snapshot.
    if wants_edit(&cli.command) {
        edit_config_for_command(ctx, cli)?;
    }
    let (config, name) =
        resolve::resolve_config(cli, target.clone()).map_err(ConfigResolveError::into_report)?;

    let env = podbox::env::resolve()?;
    let xdg = podbox::xdg::resolve(&config.integration.xdg_dirs)?;

    match &cli.command {
        Command::Build {
            name: _,
            rebuild,
            no_diff,
            output,
            ..
        } => {
            commands::lifecycle::run_build(
                &config,
                &env,
                &xdg,
                dry,
                *rebuild,
                *no_diff,
                (*output).into(),
            )?;
        }

        Command::Enable { name: _, yes } => {
            commands::lifecycle::run_enable(&config, &env, &xdg, dry, *yes)?;
        }

        Command::Disable { force: false, .. } => {
            commands::lifecycle::run_disable(&name, dry)?;
        }

        Command::Start {
            name: _, timeout, ..
        } => {
            commands::lifecycle::run_start(&config, &env, &xdg, &name, dry, *timeout)?;
        }

        Command::Stop { all: false, .. } => {
            commands::lifecycle::run_stop(&config, &name, dry)?;
        }

        Command::Restart { .. } => {
            commands::lifecycle::run_restart(&config, &name, dry)?;
        }

        Command::Enter {
            name: _,
            here,
            env: env_overrides,
            ..
        } => {
            let ctx = commands::runtime::RunContext {
                env: &env,
                config: &config,
                xdg: &xdg,
                name: &name,
                env_overrides,
                dry_run: dry,
            };
            commands::runtime::run_shell_enter(ctx, *here)?;
        }

        Command::Exec {
            args: cmd_args,
            root,
            env: env_overrides,
            here,
        } => {
            let ctx = commands::runtime::RunContext {
                env: &env,
                config: &config,
                xdg: &xdg,
                name: &name,
                env_overrides,
                dry_run: dry,
            };
            commands::runtime::run_exec(ctx, cmd_args, *root, *here)?;
        }

        Command::Run {
            app,
            app_args,
            env: env_overrides,
        } => {
            let ctx = commands::runtime::RunContext {
                env: &env,
                config: &config,
                xdg: &xdg,
                name: &name,
                env_overrides,
                dry_run: dry,
            };
            commands::runtime::run_run(ctx, app, app_args)?;
        }

        Command::Status { name: _, output } => {
            commands::runtime::run_status(&config, &name, dry, (*output).into())?;
        }

        Command::Config {
            config_cmd: ConfigCommand::Get { key, output, .. },
        } => {
            commands::config::run_get(&config, key, (*output).into())?;
        }

        Command::Stats {
            no_stream, output, ..
        } => {
            commands::stats::run_stats(&name, *no_stream, (*output).into())?;
        }

        Command::Logs {
            name: _,
            follow,
            tail,
            since,
        } => {
            commands::runtime::run_logs(&name, *follow, *tail, since.clone(), dry)?;
        }

        Command::Diff { apply, output, .. } => {
            commands::diff::run_diff(&config, &name, &env.username, *apply, dry, (*output).into())?;
        }

        Command::Snapshot { snapshot_cmd } => {
            handlers::run_snapshot_command(snapshot_cmd, &config, &name, dry)?;
        }

        Command::Restore { tag, .. } => {
            commands::lifecycle::run_restore(&config, &name, tag, dry)?;
        }

        Command::Rollback { .. } => {
            commands::lifecycle::run_rollback(&config, &env, &xdg, &name, dry)?;
        }

        Command::Inspect {
            toml: show_config,
            quadlet: show_quadlet,
            env: show_env,
            output,
            ..
        } => {
            commands::inspect::run_inspect(
                &config,
                &name,
                &env,
                &xdg,
                *show_config,
                *show_quadlet,
                *show_env,
                (*output).into(),
            )?;
        }

        Command::Export { export_cmd } => {
            commands::export::run_export(&name, Some(&config), export_cmd, dry)?;
        }

        Command::Update { no_restart, .. } => {
            commands::lifecycle::run_update(&config, &env, &xdg, &name, dry, *no_restart)?;
        }

        Command::Pull { image } => {
            commands::pull::run_pull(&config, image, dry)?;
        }

        Command::Dotfiles { dotfiles_cmd } => match dotfiles_cmd {
            DotfilesCommand::Sync { .. } => {
                commands::dotfiles::sync(&config, &env, dry)?;
            }
            DotfilesCommand::Status { .. } => {
                commands::dotfiles::status(&config, &env)?;
            }
        },

        Command::Provision { provision_cmd } => match provision_cmd {
            ProvisionCommand::Sync { steps, force, .. } => {
                commands::provision::sync(&config, &env, steps, *force, dry)?;
            }
            ProvisionCommand::Status { output, check, .. } => {
                commands::provision::status(&config, (*output).into(), *check)?;
            }
        },

        Command::Recover { name: _, yes } => {
            commands::recover::run_recover(
                &config,
                &env,
                &xdg,
                &name,
                commands::recover::RecoverOpts {
                    yes: *yes,
                    dry_run: dry,
                },
            )?;
        }

        Command::TranslatePath {
            to_container,
            to_host,
            path,
        } => {
            commands::translate::run_translate_path(&config, &xdg, *to_container, *to_host, path)?;
        }

        // Mirror arms: these (arg-flagged or self-resolving) modes bypass
        // shared resolution and are dispatched by `dispatch_no_config`, see
        // `needs_config`.
        Command::Doctor { .. }
        | Command::Edit { .. }
        | Command::History { .. }
        | Command::Cache { .. }
        | Command::InternalStdinWatchdog { .. }
        | Command::Completions { .. }
        | Command::CompleteNames
        | Command::Init { .. }
        | Command::Create { .. }
        | Command::Profile { .. }
        | Command::List { .. }
        | Command::Info { .. }
        | Command::Watch { .. }
        | Command::Clone { .. }
        | Command::Use { .. }
        | Command::Migrate { .. }
        | Command::Serve { .. }
        | Command::Compositor { .. }
        | Command::FindDefinition { .. }
        | Command::Disable { force: true, .. }
        | Command::Remove { .. }
        | Command::Stop { all: true, .. }
        | Command::Config {
            config_cmd: ConfigCommand::Set { .. } | ConfigCommand::Validate { .. },
        }
        | Command::Publish { .. } => {}
    }

    Ok(())
}
