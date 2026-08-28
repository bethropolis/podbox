//! Command handlers and config-resolution helpers for the `podbox` binary.
//!
//! Extracted from `main.rs`, which keeps `main`, the `run` dispatch, tracing
//! init, and exit-code mapping. Items here are `pub(crate)` because only the
//! binary crate consumes them.

use std::path::PathBuf;

use anyhow::Result;

use podbox::cli::{Cli, Command};
use podbox::config::{self, Config};
use podbox::editor;
use podbox::error::PodboxError;
use podbox::ui;

use crate::commands;

/// Commands that need image label defaults applied to the config.
/// These generate Quadlet files or build the image — the rest can skip
/// the ~100ms `podman inspect` fork.
pub(crate) fn needs_image_labels(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Build { .. } | Command::Enable { .. } | Command::Update { .. }
    )
}

#[allow(clippy::unnested_or_patterns)]
pub(crate) fn extract_positional_name(cmd: &Command) -> Option<String> {
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
        | Command::Recover { name, .. }
        | Command::Edit { name, .. }
        | Command::Doctor { name, .. } => name.clone(),
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
pub(crate) fn promote_leading_container_name(
    command: &mut Command,
    explicit_container: &mut Option<String>,
) {
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

pub(crate) fn resolve_config(cli: &Cli, target_name: Option<String>) -> Result<(Config, String)> {
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
pub(crate) fn resolve_config_path(container: Option<&str>) -> Result<PathBuf> {
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

pub(crate) fn run_snapshot_command(
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

pub(crate) fn run_profile_command(cmd: &podbox::cli::ProfileCommand) -> Result<()> {
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
pub(crate) fn run_edit(dry_run: bool, container: Option<&str>, rebuild_after: bool) -> Result<()> {
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
