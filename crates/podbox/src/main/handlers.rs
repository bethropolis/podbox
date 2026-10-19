//! Shared command handlers for the `podbox` binary.
//!
//! Positional-name classification, the leading-name promotion for
//! `exec`/`run`, and a few small multi-branch command flows. Config
//! resolution itself lives in `resolve`; the dispatch in `dispatch`. Items
//! here are `pub(crate)` because only the binary crate consumes them.
//
use anyhow::{Context, Result};

use podbox::cli::Command;
use podbox::config::Config;
use podbox::editor;
use podbox::error::PodboxError;

use crate::commands;

#[allow(clippy::unnested_or_patterns)]
pub(crate) fn extract_positional_name(cmd: &Command) -> Option<String> {
    match cmd {
        Command::Build { name, .. }
        | Command::Enable { name, .. }
        | Command::Disable { name, .. }
        | Command::Start { name, .. }
        | Command::Stop { name, .. }
        | Command::Restart { name, .. }
        | Command::Watch { name, .. }
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
        | Command::Rollback { name }
        | Command::Inspect { name, .. }
        | Command::FindDefinition { name }
        | Command::Recover { name, .. }
        | Command::Edit { name, .. }
        | Command::Doctor { name, .. }
        | Command::Stats { name, .. } => name.clone(),
        // Always named; dispatched before generic resolution (see `run`), but
        // listed here so `validate_cli_names` checks the name too.
        Command::Serve { name } | Command::Compositor { name } => Some(name.clone()),
        Command::Dotfiles { dotfiles_cmd } => match dotfiles_cmd {
            podbox::cli::DotfilesCommand::Sync { name }
            | podbox::cli::DotfilesCommand::Status { name } => name.clone(),
        },
        Command::Provision { provision_cmd } => match provision_cmd {
            podbox::cli::ProvisionCommand::Sync { name, .. }
            | podbox::cli::ProvisionCommand::Status { name, .. } => name.clone(),
        },
        _ => None,
    }
}

/// True when a config for `name` exists (canonical or legacy), i.e. `name`
/// is a managed container. Used to disambiguate the optional leading
/// container name on `exec` / `run`.
fn is_known_config(name: &str) -> bool {
    !name.is_empty() && podbox::config::find_config_path(name).is_some()
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
        Command::Run { app, app_args, .. } if !app_args.is_empty() && is_known_config(app) => {
            let name = std::mem::take(app);
            *app = app_args.remove(0);
            *explicit_container = Some(name);
        }
        _ => {}
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
    // The config was already resolved from any positional name, so the
    // runtime name it carries is authoritative. The raw argument may be a
    // file name that differs from `container.name`.
    let name = resolved_name;
    match cmd {
        podbox::cli::SnapshotCommand::Create { tag, .. } => {
            commands::lifecycle::run_snapshot(config, name, tag.as_deref(), dry_run)?;
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

pub(crate) fn run_profile_command(cmd: &podbox::cli::ProfileCommand, dry_run: bool) -> Result<()> {
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
        podbox::cli::ProfileCommand::Export { name, output } => {
            let p = podbox::profiles::find(name)
                .ok_or_else(|| anyhow::anyhow!("Profile '{name}' not found"))?;
            if let Some(path) = output {
                std::fs::write(path, &p.toml)
                    .with_context(|| format!("failed to write export '{}'", path.display()))?;
                println!("Exported profile '{name}' -> {}", path.display());
            } else {
                print!("{}", p.toml);
            }
        }
        podbox::cli::ProfileCommand::Import { path, name, force } => {
            // Import stages and, with `--force`, deletes the target first. Both
            // are real writes; `stage_import` honors dry_run, but the delete
            // below does not, so `--force` under dry-run would lose the profile.
            if dry_run && *force {
                anyhow::bail!(
                    "--dry-run is not supported for `podbox profile import --force` (it deletes the existing profile first)"
                );
            }
            if *force {
                // `--force` stages over an existing file: remove first so the
                // shared staging helper's clobber guard stays unconditional.
                let stem = name.clone().unwrap_or_else(|| {
                    path.file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                let target = podbox::config::profiles_dir().join(format!("{stem}.toml"));
                if target.is_file() {
                    std::fs::remove_file(&target)?;
                }
            }
            commands::create::stage_import(path, name.as_deref(), None, dry_run)?;
        }
    }
    Ok(())
}

/// Open the config in the user's editor, detect [image] changes, and offer to rebuild.
pub(crate) fn run_edit(dry_run: bool, container: Option<&str>, rebuild_after: bool) -> Result<()> {
    let config_path = super::resolve::resolve_config_path(container)
        .map_err(super::resolve::ConfigResolveError::into_report)?;

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
            commands::lifecycle::run_build(
                &config,
                &env,
                &xdg,
                false,
                false,
                false,
                podbox::cli::OutputFormat::Text,
            )?;
        } else if podbox::codegen::distros::is_tty() {
            let ctx = podbox::ui::Ctx::new(false, false, dry_run);
            match podbox::ui::confirm(&ctx, "Rebuild now?", true) {
                Ok(()) => {
                    let config = Config::load(&config_path)?;
                    let env = podbox::env::resolve()?;
                    let xdg = podbox::xdg::resolve(&config.integration.xdg_dirs)?;
                    commands::lifecycle::run_build(
                        &config,
                        &env,
                        &xdg,
                        false,
                        false,
                        false,
                        podbox::cli::OutputFormat::Text,
                    )?;
                }
                Err(PodboxError::ConfirmationRequired) => {
                    eprintln!("Run `podbox build` to apply changes.");
                }
                // Declined on purpose; stay quiet like before.
                Err(_) => {}
            }
        } else {
            eprintln!("Run `podbox build` to apply changes.");
        }
    }

    Ok(())
}
