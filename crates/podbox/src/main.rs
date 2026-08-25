use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;

use podbox::cli::{Cli, Command};
use podbox::config::{self, Config};
use podbox::editor;
use podbox::error::PodboxError;

mod commands;

use podbox::ui;

/// Commands that need image label defaults applied to the config.
/// These generate Quadlet files or build the image — the rest can skip
/// the ~100ms `podman inspect` fork.
fn needs_image_labels(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Build { .. } | Command::Enable { .. } | Command::Update { .. }
    )
}

#[allow(clippy::unnested_or_patterns)]
fn extract_positional_name(cmd: &Command) -> Option<String> {
    match cmd {
        Command::Build { name, .. }
        | Command::Enable { name }
        | Command::Disable { name, .. }
        | Command::Start { name, .. }
        | Command::Stop { name }
        | Command::Enter { name, .. }
        | Command::Status { name, .. }
        | Command::Remove { name, .. }
        | Command::Logs { name, .. }
        | Command::Update { name, .. }
        | Command::Diff { name, .. }
        | Command::Snapshot {
            snapshot_cmd: podbox::cli::SnapshotCommand::Create { name, .. },
        }
        | Command::Snapshot {
            snapshot_cmd: podbox::cli::SnapshotCommand::List { name, .. },
        }
        | Command::Snapshot {
            snapshot_cmd: podbox::cli::SnapshotCommand::Prune { name, .. },
        }
        | Command::Restore { name, .. }
        | Command::Inspect { name, .. }
        | Command::FindDefinition { name }
        | Command::Edit { name, .. } => name.clone(),
        _ => None,
    }
}

/// True when `<config_dir>/<name>.toml` exists, i.e. `name` is a managed
/// container. Used to disambiguate the optional leading container name on
/// `exec` / `run`.
fn is_known_config(name: &str) -> bool {
    !name.is_empty()
        && podbox::config::config_dir()
            .join(format!("{name}.toml"))
            .is_file()
}

/// Promote an optional leading container name on `exec` / `run`, matching
/// `podman exec [CONTAINER] COMMAND`. The name is only taken when it refers to
/// a known config AND more arguments follow (`podbox exec fedora ls`), so a
/// bare `podbox exec fedora` still runs the `fedora` binary in the resolved
/// container. An explicit `-C` always wins.
fn promote_leading_container_name(command: &mut Command, explicit_container: &mut Option<String>) {
    if explicit_container.is_some() {
        return;
    }
    match command {
        Command::Exec { args, .. } if args.len() > 1 && is_known_config(&args[0]) => {
            *explicit_container = Some(args.remove(0));
        }
        Command::Run { app, app_args } if !app_args.is_empty() && is_known_config(app) => {
            let name = std::mem::take(app);
            *app = app_args.remove(0);
            *explicit_container = Some(name);
        }
        _ => {}
    }
}

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
            | Command::InternalStdinWatchdog { .. }
    ) && which::which("podman").is_err()
    {
        return Err(PodboxError::PodmanNotFound.into());
    }

    match &cli.command {
        Command::InternalStdinWatchdog { parent_pid } => {
            return commands::runtime::run_stdin_watchdog(*parent_pid);
        }

        Command::Completions { shell } => {
            return commands::definition::run_completions((*shell).into());
        }

        Command::CompleteNames => {
            return commands::definition::run_complete_names();
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
            let config_dir = podbox::config::config_dir();
            let config_path = config_dir.join(format!("{comp_name}.toml"));
            let config = podbox::config::Config::load(&config_path)?;
            podbox::compositor::run_compositor(&config, comp_name)?;
        }

        Command::Update { no_restart, .. } => {
            commands::lifecycle::run_update(&config, &env, &xdg, &name, cli.dry_run, *no_restart)?;
        }

        Command::Pull { image } => {
            commands::pull::run_pull(&config, image, cli.dry_run)?;
        }

        Command::Doctor { fix, output } => {
            commands::runtime::run_doctor(&config, &env, *fix, *output)?;
        }

        Command::TranslatePath {
            to_container,
            to_host,
            path,
        } => {
            commands::translate::run_translate_path(&config, &xdg, *to_container, *to_host, path)?;
        }

        Command::FindDefinition { .. }
        | Command::Completions { .. }
        | Command::Profile { .. }
        | Command::Init { .. }
        | Command::Create { .. }
        | Command::Clone { .. }
        | Command::List { .. }
        | Command::Use { .. }
        | Command::Edit { .. } => unreachable!(),
    }

    Ok(())
}

fn resolve_config(cli: &Cli, target_name: Option<String>) -> Result<(Config, String)> {
    let mut config = if let Some(ref path) = cli.config {
        match Config::load(path) {
            Ok(cfg) => cfg,
            Err(e)
                if e.downcast_ref::<PodboxError>()
                    .is_some_and(|pe| matches!(pe, PodboxError::DefinitionNotFound { .. })) =>
            {
                ui::warn(&format!(
                    "config file not found at '{}', using embedded default.",
                    path.display()
                ));
                Config::embedded()
            }
            Err(e) => return Err(e),
        }
    } else if let Some(ref container_name) = target_name {
        let config_dir = config::config_dir();
        let config_path = config_dir.join(format!("{container_name}.toml"));
        Config::load(&config_path).map_err(|e| {
            anyhow::anyhow!(
                "{}\n\nHint: Use `--config <PATH>` to specify a config file, or `-C <NAME>` to use a config from {}",
                e,
                config_dir.display()
            )
        })?
    } else {
        let config_list = config::list_configs();
        let tty = podbox::codegen::distros::is_tty();

        // No configs at all — welcome the user and offer the wizard (TTY only).
        if config_list.is_empty() && config::find_definition().is_none() {
            if !tty {
                anyhow::bail!(
                    "No container configs found.\n\n\
                     Hint: Run `podbox init -i` to create one interactively, or\n\
                           `podbox create <profile>` for a one-shot setup, or\n\
                           point at a file with `--config <PATH>`."
                );
            }
            eprintln!("Welcome to podbox! It looks like you don't have any containers set up yet.");
            let launch =
                dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                    .with_prompt("Would you like to run the interactive setup wizard?")
                    .default(true)
                    .interact()
                    .unwrap_or(false);
            if launch {
                commands::create::run_init(cli.dry_run, None, None, true, None)?;
                return Ok((Config::embedded(), String::new()));
            }
        }

        // Multiple configs without an explicit name: prompt on a TTY, fail
        // with a hint otherwise so scripts never hang or guess.
        match config_list.len().cmp(&1) {
            std::cmp::Ordering::Greater => {
                if tty {
                    let items: Vec<String> = config_list
                        .iter()
                        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                        .collect();
                    let selection =
                        dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                            .with_prompt("Multiple containers found")
                            .items(&items)
                            .default(0)
                            .interact()
                            .map_err(|e| anyhow::anyhow!("selection failed: {e}"))?;
                    Config::load(&config_list[selection])?
                } else {
                    let names: Vec<String> = config_list
                        .iter()
                        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                        .collect();
                    anyhow::bail!(
                        "Multiple container configs found ({}).\n\n\
                         Hint: Pass `-C <NAME>`, set $PODBOX_CONTAINER, or pin one with\n\
                               `podbox use <NAME>` for non-interactive use.",
                        names.join(", ")
                    );
                }
            }
            std::cmp::Ordering::Equal => Config::load(&config_list[0])?,
            std::cmp::Ordering::Less => match config::find_definition() {
                Some(path) => Config::load(&path)?,
                None => {
                    anyhow::bail!(
                        "No container configs found. Create one with `podbox init --interactive` \
                         or specify a config with `--config <PATH>` / `-C <NAME>`."
                    );
                }
            },
        }
    };

    let name = config.container.name.clone();

    if needs_image_labels(&cli.command) {
        let local_tag = format!("localhost/podbox-{}:latest", config.image.name);
        if let Ok(true) = podbox::podman::image_exists(&local_tag)
            && let Ok(labels) = podbox::labels::fetch(&local_tag)
        {
            podbox::labels::apply_defaults(&mut config, &labels);
        }
    }

    Ok((config, name))
}

/// Resolve the config file path for the given container name (or auto-detect).
fn resolve_config_path(container: Option<&str>) -> Result<PathBuf> {
    if let Some(name) = container {
        let path = config::config_dir().join(format!("{name}.toml"));
        if !path.exists() {
            anyhow::bail!(
                "no config found for container '{}' at '{}'",
                name,
                path.display()
            );
        }
        return Ok(path);
    }

    let configs = config::list_configs();
    match configs.len() {
        0 => {
            let local = config::find_definition();
            match local {
                Some(p) => Ok(p),
                None => anyhow::bail!("no config found. Run `podbox init` to create one."),
            }
        }
        1 => Ok(configs.into_iter().next().unwrap()),
        _ => {
            if podbox::codegen::distros::is_tty() {
                let items: Vec<String> = configs
                    .iter()
                    .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                    .collect();
                let idx =
                    dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
                        .with_prompt("Select container")
                        .items(&items)
                        .default(0)
                        .interact()?;
                Ok(configs[idx].clone())
            } else {
                anyhow::bail!("multiple configs found — specify one with --container <name>")
            }
        }
    }
}

/// Hash the `[image]` section of a config file — used to detect changes.
fn hash_image_section(path: &std::path::Path) -> Result<String> {
    let raw = std::fs::read_to_string(path)?;
    let table: toml::Value = raw.parse()?;
    let image_str = table
        .get("image")
        .map(std::string::ToString::to_string)
        .unwrap_or_default();
    use sha2::{Digest, Sha256};
    Ok(hex::encode(Sha256::digest(image_str.as_bytes())))
}

fn run_snapshot_command(
    cmd: &podbox::cli::SnapshotCommand,
    config: &Config,
    resolved_name: &str,
    dry_run: bool,
) -> Result<()> {
    let name = match cmd {
        podbox::cli::SnapshotCommand::Create { name: n, .. }
        | podbox::cli::SnapshotCommand::List { name: n, .. }
        | podbox::cli::SnapshotCommand::Prune { name: n, .. } => {
            n.as_deref().unwrap_or(resolved_name)
        }
    };
    match cmd {
        podbox::cli::SnapshotCommand::Create { tag, .. } => {
            commands::lifecycle::run_snapshot(config, name, tag.as_deref())?;
        }
        podbox::cli::SnapshotCommand::List { output, .. } => {
            commands::lifecycle::run_snapshot_list(name, *output)?;
        }
        podbox::cli::SnapshotCommand::Prune { keep, .. } => {
            commands::lifecycle::run_snapshot_prune(name, *keep, dry_run)?;
        }
    }
    Ok(())
}

fn run_profile_command(cmd: &podbox::cli::ProfileCommand) -> Result<()> {
    match cmd {
        podbox::cli::ProfileCommand::List => {
            let list = podbox::profiles::all();
            println!("{:<10} {:<15} DESCRIPTION", "PROFILE", "LABEL");
            println!("{}", "─".repeat(70));
            for p in &list {
                println!("{:<10} {:<15} {}", p.name, p.label, p.description);
            }
        }
        podbox::cli::ProfileCommand::Show { name } => {
            let p = podbox::profiles::find(name)
                .ok_or_else(|| anyhow::anyhow!("Profile '{name}' not found"))?;
            println!("{}", p.toml);
        }
    }
    Ok(())
}

/// Open the config in the user's editor, detect [image] changes, and offer to rebuild.
fn run_edit(dry_run: bool, container: Option<&str>, rebuild_after: bool) -> Result<()> {
    let config_path = resolve_config_path(container)?;

    if dry_run {
        println!("Would open: {}", config_path.display());
        return Ok(());
    }

    let pre_hash = hash_image_section(&config_path)?;

    let ed = editor::resolve()?;
    editor::open(&ed, &config_path)?;

    let post_hash = hash_image_section(&config_path)?;
    let image_changed = pre_hash != post_hash;

    if image_changed {
        println!("Image config changed.");
        if rebuild_after {
            let config = Config::load(&config_path)?;
            let env = podbox::env::resolve()?;
            let xdg = podbox::xdg::resolve(&config.integration.xdg_dirs)?;
            commands::lifecycle::run_build(&config, &env, &xdg, false, false, false)?;
        } else if podbox::codegen::distros::is_tty() {
            let yes = dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                .with_prompt("Rebuild now?")
                .default(true)
                .interact()?;
            if yes {
                let config = Config::load(&config_path)?;
                let env = podbox::env::resolve()?;
                let xdg = podbox::xdg::resolve(&config.integration.xdg_dirs)?;
                commands::lifecycle::run_build(&config, &env, &xdg, false, false, false)?;
            }
        } else {
            eprintln!("Run `podbox build` to apply changes.");
        }
    }

    Ok(())
}
