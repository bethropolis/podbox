//! Binary entrypoint. Keeps the slim `main()`/`run()` dispatch, tracing init,
//! and exit-code mapping; command handlers live in the `handlers` module.
//! This large dispatch function is a single cohesive concern (documented
//! exemption to the ~300 LOC guideline, per the modularization guide 1/8).
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use podbox::cli::{Cli, Command};
use podbox::config;
use podbox::editor;
use podbox::error::PodboxError;

mod commands;

#[path = "main/handlers.rs"]
mod handlers;

use handlers::{
    extract_positional_name, promote_leading_container_name, resolve_config, resolve_config_path,
    run_edit, run_profile_command, run_snapshot_command,
};

use podbox::ui;

fn init_tracing(verbosity: u8) {
    use tracing_subscriber::prelude::*;
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let level = match verbosity {
            0 => "info",
            1 => "debug",
            _ => "trace",
        };
        tracing_subscriber::EnvFilter::new(level)
    });
    if let Ok(layer) = tracing_journald::layer() {
        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(layer)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .try_init();
    }
}

fn main() -> ExitCode {
    let result = run();
    if let Err(e) = result {
        ui::error(&format!("{e:#}"));
        exit_code_for_error(&e)
    } else {
        ExitCode::SUCCESS
    }
}

fn exit_code_for_error(err: &anyhow::Error) -> ExitCode {
    if let Some(podbox_err) = err.downcast_ref::<PodboxError>() {
        match podbox_err {
            PodboxError::DefinitionNotFound { .. }
            | PodboxError::DefinitionReadFailed(_)
            | PodboxError::DefinitionParseFailed(_) => ExitCode::from(2),
            PodboxError::ContainerMissing(_) => ExitCode::from(3),
            PodboxError::BuildFailed(_) | PodboxError::PodmanInspectFailed { .. } => {
                ExitCode::from(4)
            }
            PodboxError::PodmanNotFound => ExitCode::from(5),
            PodboxError::PullFailed { .. } | PodboxError::TagFailed { .. } => ExitCode::from(6),
            _ => ExitCode::FAILURE,
        }
    } else {
        ExitCode::FAILURE
    }
}

fn run() -> Result<()> {
    let mut cli = Cli::parse();

    ui::set_quiet(cli.quiet);
    ui::set_verbose(cli.verbose);
    init_tracing(cli.verbose);

    // `exec` / `run` accept an optional leading container name (podman-style).
    // Must run before config resolution so the promoted name wins over
    // PODBOX_CONTAINER / active context, but never overrides an explicit -C.
    promote_leading_container_name(&mut cli.command, &mut cli.container);

    // Exit early if podman is not installed — clean error instead of a cryptic
    // spawn failure deep in the stack.
    if !matches!(
        &cli.command,
        Command::Completions { .. }
            | Command::Profile { .. }
            | Command::Serve { .. }
            | Command::Compositor { .. }
            | Command::CompleteNames
            | Command::History { .. }
            | Command::InternalStdinWatchdog { .. }
            | Command::Migrate { .. }
    ) && which::which("podman").is_err()
    {
        return Err(PodboxError::PodmanNotFound.into());
    }

    match &cli.command {
        Command::InternalStdinWatchdog { parent_pid } => {
            return commands::runtime::run_stdin_watchdog(*parent_pid);
        }

        Command::Completions { shell, abbrs } => {
            return commands::definition::run_completions((*shell).into(), *abbrs);
        }

        Command::CompleteNames => {
            return commands::definition::run_complete_names();
        }

        Command::History {
            name,
            limit,
            output,
        } => {
            return commands::history::run_history(name.clone(), *limit, *output);
        }

        Command::Init {
            image,
            name,
            interactive,
            profile,
        } => {
            return commands::create::run_init(
                cli.dry_run,
                image.as_deref(),
                name.as_deref(),
                *interactive,
                profile.as_deref(),
            );
        }

        Command::Create {
            image,
            name,
            packages,
            no_start,
            edit,
        } => {
            return commands::create::run_create(
                cli.dry_run,
                image,
                name.as_deref(),
                packages.as_deref(),
                *no_start,
                *edit,
            );
        }

        Command::Profile { profile_cmd } => {
            return run_profile_command(profile_cmd);
        }

        Command::List { output } => {
            return commands::definition::run_list(*output);
        }

        Command::Clone {
            src,
            dst,
            copy_home,
        } => {
            return commands::clone::run_clone(src, dst, *copy_home, cli.dry_run);
        }

        Command::Use { name, clear } => {
            return commands::context::run_use(name.clone(), *clear, cli.dry_run);
        }

        Command::Migrate { force } => {
            return commands::migrate::run_migrate(commands::migrate::MigrateOpts {
                dry_run: cli.dry_run,
                force: *force,
            });
        }

        Command::Edit { name, rebuild } => {
            let container_name = name
                .clone()
                .or_else(|| cli.container.clone())
                .or_else(|| std::env::var("PODBOX_CONTAINER").ok())
                .or_else(config::read_active_context);
            return run_edit(cli.dry_run, container_name.as_deref(), *rebuild);
        }

        _ => {}
    }

    // Resolution chain: positional -> -C -> PODBOX_CONTAINER env -> .active
    let cmd_name = extract_positional_name(&cli.command);
    let target_name = cmd_name
        .or_else(|| cli.container.clone())
        .or_else(|| std::env::var("PODBOX_CONTAINER").ok())
        .or_else(config::read_active_context);

    // Short-circuit for commands that don't need a full config load
    if let Command::FindDefinition { name } = &cli.command {
        let lookup = name.clone().or_else(|| target_name.clone());
        return commands::definition::run_find_definition(lookup.as_deref());
    }

    if let Command::Disable { force: true, .. } = &cli.command {
        let n = target_name.clone()
            .context("--force requires a container name (positional, -C, PODBOX_CONTAINER env, or active context)")?;
        return commands::lifecycle::run_disable(&n);
    }

    if let Command::Remove {
        stale: true, force, ..
    } = &cli.command
    {
        return commands::lifecycle::run_remove_stale(cli.dry_run, *force);
    }

    let (config, name) = resolve_config(&cli, target_name)?;

    let env = podbox::env::resolve()?;
    let xdg = podbox::xdg::resolve(&config.integration.xdg_dirs)?;

    match &cli.command {
        Command::InternalStdinWatchdog { .. } | Command::CompleteNames => {
            unreachable!("handled before config resolution")
        }

        Command::Build {
            name: _,
            rebuild,
            no_diff,
            edit,
        } => {
            if *edit {
                let config_path = resolve_config_path(cli.container.as_deref())?;
                let ed = editor::resolve()?;
                editor::open(&ed, &config_path)?;
            }
            commands::lifecycle::run_build(&config, &env, &xdg, cli.dry_run, *rebuild, *no_diff)?;
        }

        Command::Enable { name: _ } => {
            commands::lifecycle::run_enable(&config, &env, &xdg, cli.dry_run)?;
        }

        Command::Disable { name: _, .. } => {
            commands::lifecycle::run_disable(&name)?;
        }

        Command::Start {
            name: _,
            timeout,
            edit,
        } => {
            if *edit {
                let config_path = resolve_config_path(cli.container.as_deref())?;
                let ed = editor::resolve()?;
                editor::open(&ed, &config_path)?;
            }
            commands::lifecycle::run_start(&config, &env, &xdg, &name, cli.dry_run, *timeout)?;
        }

        Command::Stop { name: _ } => {
            commands::lifecycle::run_stop(&config, &name, cli.dry_run)?;
        }

        Command::Enter { name: _, edit } => {
            if *edit {
                let config_path = resolve_config_path(cli.container.as_deref())?;
                let ed = editor::resolve()?;
                editor::open(&ed, &config_path)?;
            }
            commands::runtime::run_shell_enter(&env, &config, &name, cli.dry_run, &xdg)?;
        }

        Command::Exec {
            args: cmd_args,
            root,
        } => {
            commands::runtime::run_exec(&env, &name, cmd_args, cli.dry_run, *root)?;
        }

        Command::Run { app, app_args } => {
            commands::runtime::run_run(&env, &name, app, app_args, cli.dry_run)?;
        }

        Command::Status { name: _, output } => {
            commands::runtime::run_status(&name, cli.dry_run, *output)?;
        }

        Command::Stats {
            no_stream, output, ..
        } => {
            commands::stats::run_stats(&name, *no_stream, *output)?;
        }

        Command::Logs {
            name: _,
            follow,
            tail,
            since,
        } => {
            commands::runtime::run_logs(&name, *follow, *tail, since.clone(), cli.dry_run)?;
        }

        Command::Diff { apply, output, .. } => {
            commands::diff::run_diff(&config, &name, &env.username, *apply, *output)?;
        }

        Command::Snapshot { snapshot_cmd } => {
            run_snapshot_command(snapshot_cmd, &config, &name, cli.dry_run)?;
        }

        Command::Restore { tag, .. } => {
            commands::lifecycle::run_restore(&config, &name, tag)?;
        }

        Command::Inspect {
            config: show_config,
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
                *output,
            )?;
        }

        Command::Export { export_cmd } => {
            commands::export::run_export(&name, Some(&config), export_cmd)?;
        }

        Command::Remove {
            name: _,
            all,
            force,
            config: remove_config,
            ..
        } => {
            commands::lifecycle::run_remove(
                &config,
                &name,
                cli.dry_run,
                *all,
                *force,
                *remove_config,
            )?;
        }

        Command::Serve { name: serve_name } => {
            commands::serve::run_serve(cli.config.as_ref(), serve_name, cli.dry_run)?;
        }

        Command::Compositor { name: comp_name } => {
            let config_path = podbox::config::find_config_path(comp_name).ok_or_else(|| {
                anyhow::anyhow!(
                    "no config found for container '{}' at '{}/{{profiles/,}}{}.toml'",
                    comp_name,
                    podbox::config::config_dir().display(),
                    comp_name
                )
            })?;
            let config = podbox::config::Config::load(&config_path)?;
            podbox::compositor::run_compositor(&config, comp_name)?;
        }

        Command::Update { no_restart, .. } => {
            commands::lifecycle::run_update(&config, &env, &xdg, &name, cli.dry_run, *no_restart)?;
        }

        Command::Pull { image } => {
            commands::pull::run_pull(&config, image, cli.dry_run)?;
        }

        Command::Doctor {
            name: _,
            fix,
            output,
        } => {
            commands::runtime::run_doctor(&config, &env, *fix, *output)?;
        }

        Command::Recover { name: _, yes } => {
            commands::recover::run_recover(
                &config,
                &env,
                &xdg,
                &name,
                commands::recover::RecoverOpts {
                    yes: *yes,
                    dry_run: cli.dry_run,
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

        Command::FindDefinition { .. }
        | Command::History { .. }
        | Command::Completions { .. }
        | Command::Profile { .. }
        | Command::Init { .. }
        | Command::Create { .. }
        | Command::Clone { .. }
        | Command::List { .. }
        | Command::Use { .. }
        | Command::Migrate { .. }
        | Command::Edit { .. } => unreachable!(),
    }

    Ok(())
}
