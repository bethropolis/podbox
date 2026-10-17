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
    let cli = Cli::parse();
    let format = command_output_format(&cli.command);
    if let Err(e) = run(cli) {
        // Contract: in JSON mode failures also print a structured error on
        // stdout (same exit code). Diagnostics stay on stderr either way.
        // Skipped when the command already printed its JSON report and the
        // error only carries the exit status (doctor failures, --check).
        // NDJSON streams get the compact one-line form so line parsers keep
        // working; the envelope terminates the stream.
        if matches!(
            format,
            Some(podbox::cli::OutputFormat::Json | podbox::cli::OutputFormat::Ndjson)
        ) {
            let reported = e
                .downcast_ref::<PodboxError>()
                .is_some_and(PodboxError::json_reported);
            if !reported {
                let code = e
                    .downcast_ref::<PodboxError>()
                    .map_or("error", PodboxError::code);
                let payload = podbox::contract::error_payload(code, &format!("{e:#}"));
                let text = match format {
                    Some(podbox::cli::OutputFormat::Ndjson) => serde_json::to_string(&payload),
                    _ => serde_json::to_string_pretty(&payload),
                };
                if let Ok(text) = text {
                    println!("{text}");
                }
            }
        }
        if !matches!(
            e.downcast_ref::<PodboxError>(),
            Some(PodboxError::ChildExit(_))
        ) {
            ui::error(&format!("{e:#}"));
        }
        exit_code_for_error(&e)
    } else {
        ExitCode::SUCCESS
    }
}

fn exit_code_for_error(err: &anyhow::Error) -> ExitCode {
    if let Some(podbox_err) = err.downcast_ref::<PodboxError>() {
        match podbox_err {
            // 2: the definition file (or the explicit `--config` file) is missing,
            // unreadable or unparseable. A missing `--config` is a hard error here.
            PodboxError::DefinitionNotFound { .. }
            | PodboxError::ConfigFileMissing { .. }
            | PodboxError::DefinitionReadFailed(_)
            | PodboxError::DefinitionParseFailed(_) => ExitCode::from(2),
            PodboxError::ContainerMissing(_) => ExitCode::from(3),
            PodboxError::BuildFailed(_) | PodboxError::PodmanInspectFailed { .. } => {
                ExitCode::from(4)
            }
            PodboxError::PodmanNotFound => ExitCode::from(5),
            // 64 is EX_USAGE: a name that cannot be used. Not 2, which is the
            // definition-file code above.
            PodboxError::InvalidName { .. } => ExitCode::from(64),
            PodboxError::ChildExit(code) => ExitCode::from(u8::try_from(*code).unwrap_or(1)),
            PodboxError::PullFailed { .. } | PodboxError::TagFailed { .. } => ExitCode::from(6),
            _ => ExitCode::FAILURE,
        }
    } else {
        ExitCode::FAILURE
    }
}

/// The `--output` format the invoked command was asked for, including the
/// nested `provision status` / `snapshot list` / `config` / `export list`
/// readers. Used once from `main` so machine-mode failures can print the
/// structured error envelope (pretty for `json`, one line for `ndjson`).
fn command_output_format(cmd: &Command) -> Option<podbox::cli::OutputFormat> {
    use podbox::cli::{
        ConfigCommand, ExportCommand, OutputFormat, ProvisionCommand, SnapshotCommand,
    };
    let is_machine =
        |output: &OutputFormat| matches!(output, OutputFormat::Json | OutputFormat::Ndjson);
    let format = match cmd {
        Command::Status { output, .. }
        | Command::Stats { output, .. }
        | Command::Inspect { output, .. }
        | Command::List { output, .. }
        | Command::Info { output, .. }
        | Command::Doctor { output, .. }
        | Command::Diff { output, .. }
        | Command::History { output, .. }
        | Command::Build { output, .. }
        | Command::Watch { output, .. }
        | Command::Export {
            export_cmd: ExportCommand::List { output },
        }
        | Command::Provision {
            provision_cmd: ProvisionCommand::Status { output, .. },
        }
        | Command::Config {
            config_cmd:
                ConfigCommand::Get { output, .. }
                | ConfigCommand::Set { output, .. }
                | ConfigCommand::Validate { output, .. },
        }
        | Command::Snapshot {
            snapshot_cmd: SnapshotCommand::List { output, .. },
        } => *output,
        _ => return None,
    };
    is_machine(&format).then_some(format)
}

/// `--edit` on build, start and enter: open the config in the editor, or only
/// say so under `--dry-run` (an editor session is not a dry-run action).
///
/// Edits the file the command actually runs against: the explicit `--config`
/// path when given, otherwise the config for the positional name (which wins
/// over `-C`, mirroring resolution) or `-C` itself.
fn edit_config_for_command(cli: &Cli) -> Result<()> {
    let config_path = match &cli.config {
        Some(path) => path.clone(),
        None => {
            let positional = extract_positional_name(&cli.command);
            let name = positional.as_deref().or(cli.container.as_deref());
            resolve_config_path(name)?
        }
    };
    if cli.dry_run {
        ui::would(&format!("open '{}' in $EDITOR", config_path.display()));
        return Ok(());
    }
    let ed = editor::resolve()?;
    editor::open(&ed, &config_path)
}

/// Reject unsafe names before any command touches the filesystem or podman.
///
/// Names feed file paths (`quadlet_dir().join(name)`), unit names and podman
/// arguments, so `.`, `..`, a leading `-` or a `/` must never get past here.
/// Covers every place a name can enter: the global `-C`, positional names,
/// `create --name`, `clone` (both names), profile names, and snapshot tags.
fn validate_cli_names(cli: &Cli) -> Result<()> {
    use podbox::cli::{ProfileCommand, SnapshotCommand};
    use podbox::config::validate_container_name as check;

    if let Some(name) = &cli.container {
        check("container", name)?;
    }
    if let Some(name) = handlers::extract_positional_name(&cli.command) {
        check("container", &name)?;
    }
    match &cli.command {
        Command::Create {
            name: Some(name), ..
        } => check("container", name)?,
        Command::Clone { src, dst, .. } => {
            check("container", src)?;
            check("container", dst)?;
        }
        Command::Profile { profile_cmd } => match profile_cmd {
            ProfileCommand::Show { name } => check("profile", name)?,
            ProfileCommand::Import {
                name: Some(name), ..
            } => check("profile", name)?,
            _ => {}
        },
        Command::Snapshot {
            snapshot_cmd: SnapshotCommand::Create { tag: Some(tag), .. },
        } => check("snapshot tag", tag)?,
        _ => {}
    }
    Ok(())
}

fn run(mut cli: Cli) -> Result<()> {
    ui::set_quiet(cli.quiet);
    ui::set_verbose(cli.verbose);
    init_tracing(cli.verbose);

    // `exec` / `run` accept an optional leading container name (podman-style).
    // Must run before config resolution so the promoted name wins over
    // PODBOX_CONTAINER / active context, but never overrides an explicit -C.
    promote_leading_container_name(&mut cli.command, &mut cli.container);
    validate_cli_names(&cli)?;

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
            | Command::Info { .. }
            | Command::Watch { .. }
            | Command::Cache { .. }
            | Command::InternalStdinWatchdog { .. }
            | Command::Migrate { .. }
            | Command::Dotfiles {
                dotfiles_cmd: podbox::cli::DotfilesCommand::Status { .. }
            }
            | Command::Provision {
                provision_cmd: podbox::cli::ProvisionCommand::Status { .. }
            }
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

        Command::Cache { cache_cmd } => {
            let yes = matches!(
                cache_cmd,
                podbox::cli::CacheCommand::Prune { yes, .. } if *yes
            );
            return commands::cache::run(cache_cmd, cli.dry_run, yes);
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
            import,
            name,
            packages,
            no_start,
            edit,
            yes,
        } => {
            return commands::create::run_create(
                cli.dry_run,
                image.as_deref(),
                import.as_deref(),
                name.as_deref(),
                packages.as_deref(),
                *no_start,
                *edit,
                *yes,
            );
        }

        Command::Profile { profile_cmd } => {
            return run_profile_command(profile_cmd, cli.dry_run);
        }

        Command::List { output } => {
            return commands::definition::run_list(*output);
        }

        Command::Info { output } => {
            return commands::info::run_info(*output);
        }

        Command::Watch {
            name,
            interval,
            output,
        } => {
            return commands::watch::run_watch(
                name.clone().or_else(|| cli.container.clone()),
                cli.config.clone(),
                *interval,
                *output,
                cli.dry_run,
            );
        }

        Command::Config { config_cmd } => match config_cmd {
            // Set/validate work on the file and do their own loading, so a
            // broken config still gets a precise report (or a fix) instead
            // of dying in shared resolution. `--config` wins when given,
            // otherwise `-C` / active context / auto-detect.
            podbox::cli::ConfigCommand::Set { key, value, output } => {
                let path = match &cli.config {
                    Some(p) => p.clone(),
                    None => resolve_config_path(cli.container.as_deref())?,
                };
                return commands::config::run_set(&path, key, value, cli.dry_run, *output);
            }
            podbox::cli::ConfigCommand::Validate { output } => {
                let path = match &cli.config {
                    Some(p) => p.clone(),
                    None => resolve_config_path(cli.container.as_deref())?,
                };
                return commands::config::run_validate(&path, *output);
            }
            podbox::cli::ConfigCommand::Get { .. } => {}
        },

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

        // Systemd runs these with the box name in the unit. They load that
        // config directly: generic resolution would ignore the name and could
        // fail on other boxes' configs (several configs, no `.active`).
        Command::Serve { name } => {
            return commands::serve::run_serve(cli.config.as_ref(), name, cli.dry_run);
        }

        Command::Compositor { name } => {
            let config_path = podbox::config::find_config_path(name)
                .ok_or_else(|| podbox::config::missing_config_error(name))?;
            let config = podbox::config::Config::load(&config_path)?;
            return podbox::compositor::run_compositor(&config, name);
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
        return commands::lifecycle::run_disable(&n, cli.dry_run);
    }

    if let Command::Remove {
        stale: true, force, ..
    } = &cli.command
    {
        return commands::lifecycle::run_remove_stale(cli.dry_run, *force);
    }

    // `stop --all` needs no target: it enumerates running containers itself.
    if let Command::Stop { all: true, .. } = &cli.command {
        return commands::lifecycle::run_stop_all(cli.dry_run);
    }

    // Doctor needs special handling: bare `memory = "2"` now fails validation,
    // so `resolve_config` would bail before `run_doctor` can offer --fix.
    if let Command::Doctor { fix, output, .. } = &cli.command {
        // `--fix` writes config and units; under --dry-run it only reports.
        let apply_fix = *fix && !cli.dry_run;
        if *fix && cli.dry_run {
            ui::warn("--fix is skipped under --dry-run; reporting only");
        }
        let target_for_doctor = target_name.clone();
        match resolve_config(&cli, target_for_doctor.clone()) {
            Ok((config, _)) => {
                let env = podbox::env::resolve()?;
                return commands::runtime::run_doctor(&config, &env, apply_fix, *output);
            }
            Err(err) => {
                let err_str = format!("{err:#}");
                if err_str.contains("container.memory") && apply_fix {
                    if commands::runtime::try_fix_bare_memory_for_target(
                        target_for_doctor.as_deref(),
                        apply_fix,
                    )? {
                        let (config2, _) = resolve_config(&cli, target_for_doctor)?;
                        let env2 = podbox::env::resolve()?;
                        return commands::runtime::run_doctor(&config2, &env2, apply_fix, *output);
                    }
                }
                return Err(err);
            }
        }
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
            output,
        } => {
            if matches!(output, podbox::cli::OutputFormat::Json) {
                return Err(anyhow::anyhow!(
                    "`podbox build` supports text and ndjson output, not pretty json"
                ));
            }
            if *edit {
                edit_config_for_command(&cli)?;
            }
            commands::lifecycle::run_build(
                &config,
                &env,
                &xdg,
                cli.dry_run,
                *rebuild,
                *no_diff,
                *output,
            )?;
        }

        Command::Enable { name: _, yes } => {
            commands::lifecycle::run_enable(&config, &env, &xdg, cli.dry_run, *yes)?;
        }

        Command::Disable { .. } => {
            commands::lifecycle::run_disable(&name, cli.dry_run)?;
        }

        Command::Start {
            name: _,
            timeout,
            edit,
        } => {
            if *edit {
                edit_config_for_command(&cli)?;
            }
            commands::lifecycle::run_start(&config, &env, &xdg, &name, cli.dry_run, *timeout)?;
        }

        Command::Stop { .. } => {
            commands::lifecycle::run_stop(&config, &name, cli.dry_run)?;
        }

        Command::Restart { .. } => {
            commands::lifecycle::run_restart(&config, &name, cli.dry_run)?;
        }

        Command::Enter {
            name: _,
            edit,
            here,
            env: env_overrides,
        } => {
            if *edit {
                edit_config_for_command(&cli)?;
            }
            let ctx = commands::runtime::RunContext {
                env: &env,
                config: &config,
                xdg: &xdg,
                name: &name,
                env_overrides,
                dry_run: cli.dry_run,
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
                dry_run: cli.dry_run,
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
                dry_run: cli.dry_run,
            };
            commands::runtime::run_run(ctx, app, app_args)?;
        }

        Command::Status { name: _, output } => {
            commands::runtime::run_status(&config, &name, cli.dry_run, *output)?;
        }

        Command::Config { config_cmd } => match config_cmd {
            podbox::cli::ConfigCommand::Get { key, output, .. } => {
                commands::config::run_get(&config, key, *output)?;
            }
            _ => unreachable!("config set/validate are handled before config resolution"),
        },

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
            commands::diff::run_diff(&config, &name, &env.username, *apply, cli.dry_run, *output)?;
        }

        Command::Snapshot { snapshot_cmd } => {
            run_snapshot_command(snapshot_cmd, &config, &name, cli.dry_run)?;
        }

        Command::Restore { tag, .. } => {
            commands::lifecycle::run_restore(&config, &name, tag, cli.dry_run)?;
        }

        Command::Rollback { .. } => {
            commands::lifecycle::run_rollback(&config, &env, &xdg, &name, cli.dry_run)?;
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
                *output,
            )?;
        }

        Command::Export { export_cmd } => {
            commands::export::run_export(&name, Some(&config), export_cmd, cli.dry_run)?;
        }

        Command::Remove {
            name: _,
            all,
            force,
            remove_config,
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

        Command::Update { no_restart, .. } => {
            commands::lifecycle::run_update(&config, &env, &xdg, &name, cli.dry_run, *no_restart)?;
        }

        Command::Pull { image } => {
            commands::pull::run_pull(&config, image, cli.dry_run)?;
        }

        Command::Dotfiles { dotfiles_cmd } => match dotfiles_cmd {
            podbox::cli::DotfilesCommand::Sync { .. } => {
                commands::dotfiles::sync(&config, &env, cli.dry_run)?;
            }
            podbox::cli::DotfilesCommand::Status { .. } => {
                commands::dotfiles::status(&config, &env)?;
            }
        },

        Command::Provision { provision_cmd } => match provision_cmd {
            podbox::cli::ProvisionCommand::Sync { steps, force, .. } => {
                commands::provision::sync(&config, &env, steps, *force, cli.dry_run)?;
            }
            podbox::cli::ProvisionCommand::Status { output, check, .. } => {
                commands::provision::status(&config, *output, *check)?;
            }
        },

        Command::Doctor {
            name: _,
            fix,
            output,
        } => {
            commands::runtime::run_doctor(&config, &env, *fix && !cli.dry_run, *output)?;
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
        | Command::Info { .. }
        | Command::Watch { .. }
        | Command::Use { .. }
        | Command::Migrate { .. }
        | Command::Edit { .. }
        | Command::Serve { .. }
        | Command::Compositor { .. } => unreachable!(),
        Command::Cache { .. } => unreachable!(),
    }

    Ok(())
}
