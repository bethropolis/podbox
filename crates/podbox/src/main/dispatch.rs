//! The command dispatch, split by what a command needs to run.
//!
//! [`needs_config`] classifies every [`Command`] variant exhaustively, so a
//! new command cannot compile until it declares how it reaches a definition:
//! it is self-contained ([`ConfigNeed::None`]), targets one named container
//! ([`ConfigNeed::Named`]), or has no name of its own and relies on the
//! global `-C` / `$PODBOX_CONTAINER` / active-context chain
//! ([`ConfigNeed::All`]). The classification decides which dispatch function
//! runs it:
//!
//! - [`dispatch_no_config`]: the handler either needs no definition at all
//!   or loads one itself because the shared resolution would abort the
//!   command's own error handling (`config set`, `doctor`, `disable --force`.
//! - [`dispatch_with_config`]: one definition is resolved up front through
//!   the single chain in [`resolve`] and handed to the handler.
//!
//! Every `match` here is exhaustive — no `_` arms — so variants must be
//! spelled out when they are added.

use anyhow::{Context, Result};

use super::ctx::Ctx;
use super::handlers;
use super::resolve::{self, ConfigResolveError};
use crate::commands;
use podbox::cli::{
    CacheCommand, Cli, Command, ConfigCommand, DotfilesCommand, ExportCommand, OutputFormat,
    ProvisionCommand, SnapshotCommand,
};
use podbox::config;
use podbox::ui;

/// How a command reaches its definition.
///
/// The payload of [`ConfigNeed::Named`] is the command's own positional
/// container name, if any; it seeds the global name chain the same way the
/// positional name always did.
#[derive(Debug)]
pub(crate) enum ConfigNeed {
    /// Self-contained: no shared definition, either because the command
    /// needs none or because it resolves (and can repair) the file itself.
    None,
    /// Needs exactly one container's definition. `Some(name)` when the
    /// command takes a positional name, `None` when the name lives in a
    /// subcommand or comes only from the global chain. The payload documents
    /// the shape of each classified command (and is how `Named` with a name
    /// is distinguished in the exhaustive match); `target_name` keeps
    /// gathering the full global chain for every command, so the value
    /// itself is not read again.
    #[allow(dead_code)]
    Named(Option<String>),
    /// Needs a definition but has no name of its own: the global chain
    /// (with its machine-level auto-detection fallback) is the only source.
    All,
}

/// Classify how a command reaches a definition file.
///
/// Exhaustive on purpose: adding a `Command` variant breaks this match and
/// forces the command to declare its config needs (014: the missing
/// classification is how `serve`/`compositor`/`stats` slipped past name
/// validation once).
pub(crate) fn needs_config(cmd: &Command) -> ConfigNeed {
    match cmd {
        // Handlers that load (or repair) a definition themselves for
        // special error handling, or that need no definition at all.
        Command::Cache { .. }
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
        | Command::History { .. }
        | Command::Edit { .. }
        | Command::Serve { .. }
        | Command::Compositor { .. }
        | Command::FindDefinition { .. }
        | Command::Doctor { .. } => ConfigNeed::None,

        // Arg-flagged modes that must not depend on shared resolution:
        // `--force` disables by name only, `--stale` removes containers
        // whose config is already gone, `--all` needs no target at all.
        Command::Disable { force: true, .. }
        | Command::Remove { stale: true, .. }
        | Command::Stop { all: true, .. } => ConfigNeed::None,
        Command::Config {
            config_cmd: ConfigCommand::Set { .. } | ConfigCommand::Validate { .. },
        } => ConfigNeed::None,

        // One definition, whose name comes from the command line
        // (positional or subcommand) or the global chain.
        Command::Build { name, .. }
        | Command::Enable { name, .. }
        | Command::Start { name, .. }
        | Command::Restart { name, .. }
        | Command::Status { name, .. }
        | Command::Logs { name, .. }
        | Command::Update { name, .. }
        | Command::Diff { name, .. }
        | Command::Restore { name, .. }
        | Command::Rollback { name }
        | Command::Inspect { name, .. }
        | Command::Recover { name, .. }
        | Command::Stats { name, .. }
        | Command::Enter { name, .. } => ConfigNeed::Named(name.clone()),
        Command::Disable {
            name, force: false, ..
        } => ConfigNeed::Named(name.clone()),
        Command::Stop {
            name, all: false, ..
        } => ConfigNeed::Named(name.clone()),
        Command::Remove {
            name, stale: false, ..
        } => ConfigNeed::Named(name.clone()),
        Command::Config {
            config_cmd: ConfigCommand::Get { .. },
        }
        | Command::Snapshot { .. }
        | Command::Export { .. }
        | Command::Dotfiles { .. }
        | Command::Provision { .. }
        | Command::Pull { .. } => ConfigNeed::Named(None),

        // No name of any kind in the command: only the global chain.
        Command::Exec { .. } | Command::Run { .. } | Command::TranslatePath { .. } => {
            ConfigNeed::All
        }
    }
}

/// Whether the command forks `podman` (directly or through the guest/tooling).
///
/// Straight-edged default: a command not listed here requires `podman`, so
/// new commands get the early clean error rather than a cryptic spawn
/// failure deep in the stack.
pub(crate) fn needs_podman(cmd: &Command) -> bool {
    !matches!(
        cmd,
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
                dotfiles_cmd: DotfilesCommand::Status { .. }
            }
            | Command::Provision {
                provision_cmd: ProvisionCommand::Status { .. }
            }
    )
}

/// The `--output` format the invoked command was asked for, including the
/// nested `provision status` / `snapshot list` / `config` / `export list`
/// readers. Used once so machine-mode failures can print the structured
/// error envelope (pretty for `json`, one line for `ndjson`). Typed the
/// same way as [`needs_config`]: adding a variant means deciding whether it
/// speaks machine mode.
pub(crate) fn command_output_format(cmd: &Command) -> Option<OutputFormat> {
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
        | Command::Config {
            config_cmd:
                ConfigCommand::Get { output, .. }
                | ConfigCommand::Set { output, .. }
                | ConfigCommand::Validate { output },
        }
        | Command::Export {
            export_cmd: ExportCommand::List { output },
        }
        | Command::Provision {
            provision_cmd: ProvisionCommand::Status { output, .. },
        }
        | Command::Snapshot {
            snapshot_cmd: SnapshotCommand::List { output, .. },
        } => *output,

        // Text-only commands. Grouped per variant so a new output format on
        // any of them is a deliberate decision, not a default.
        Command::Cache { .. }
        | Command::InternalStdinWatchdog { .. }
        | Command::Enable { .. }
        | Command::Disable { .. }
        | Command::Start { .. }
        | Command::Stop { .. }
        | Command::Restart { .. }
        | Command::Exec { .. }
        | Command::Run { .. }
        | Command::Logs { .. }
        | Command::Remove { .. }
        | Command::Serve { .. }
        | Command::Compositor { .. }
        | Command::Enter { .. }
        | Command::Create { .. }
        | Command::Edit { .. }
        | Command::Clone { .. }
        | Command::Init { .. }
        | Command::Update { .. }
        | Command::Pull { .. }
        | Command::Profile { .. }
        | Command::Dotfiles {
            dotfiles_cmd: DotfilesCommand::Sync { .. } | DotfilesCommand::Status { .. },
        }
        | Command::Provision {
            provision_cmd: ProvisionCommand::Sync { .. },
        }
        | Command::Completions { .. }
        | Command::Snapshot {
            snapshot_cmd: SnapshotCommand::Create { .. } | SnapshotCommand::Prune { .. },
        }
        | Command::Restore { .. }
        | Command::Rollback { .. }
        | Command::Use { .. }
        | Command::FindDefinition { .. }
        | Command::Recover { .. }
        | Command::TranslatePath { .. }
        | Command::Migrate { .. }
        | Command::CompleteNames
        | Command::Export {
            export_cmd:
                ExportCommand::App { .. }
                | ExportCommand::Bin { .. }
                | ExportCommand::Clean
                | ExportCommand::Remove { .. },
        } => return None,
    };
    is_machine(&format).then_some(format)
}

/// Reject unsafe names before any command touches the filesystem or podman.
///
/// Names feed file paths (`quadlet_dir().join(name)`), unit names and podman
/// arguments, so `.`, `..`, a leading `-` or a `/` must never get past here.
/// Covers every place a name can enter: the global `-C`, positional names,
/// `create --name`, `clone` (both names), profile names, and snapshot tags.
pub(crate) fn validate_cli_names(cli: &Cli) -> Result<()> {
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

/// The container name a command targets: its positional name first, then
/// `-C`, `$PODBOX_CONTAINER`, and the active context. The single seed for
/// both the path-only lookups and full definition resolution.
pub(crate) fn target_name(command: &Command, container: Option<&str>) -> Option<String> {
    handlers::extract_positional_name(command)
        .or_else(|| container.map(str::to_string))
        .or_else(|| std::env::var("PODBOX_CONTAINER").ok())
        .or_else(config::read_active_context)
}

/// `--edit` on build, start and enter: open the config in the editor, or only
/// say so under `--dry-run` (an editor session is not a dry-run action).
///
/// Edits the file the command actually runs against: the explicit `--config`
/// path when given, otherwise the config for the positional name (which wins
/// over `-C`, mirroring resolution) or `-C` itself.
fn edit_config_for_command(ctx: &Ctx, cli: &Cli) -> Result<()> {
    let config_path = match &cli.config {
        Some(path) => path.clone(),
        None => {
            let name =
                handlers::extract_positional_name(&cli.command).or_else(|| cli.container.clone());
            resolve::resolve_config_path(name.as_deref())
                .map_err(ConfigResolveError::into_report)?
        }
    };
    if ctx.dry_run {
        ui::would(&format!("open '{}' in $EDITOR", config_path.display()));
        return Ok(());
    }
    let ed = podbox::editor::resolve()?;
    podbox::editor::open(&ed, &config_path)
}

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
        } => commands::history::run_history(name.clone(), *limit, *output),

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

        Command::List { output } => commands::definition::run_list(*output),

        Command::Info { output } => commands::info::run_info(*output),

        Command::Watch {
            name,
            interval,
            output,
        } => commands::watch::run_watch(
            name.clone().or_else(|| cli.container.clone()),
            cli.config.clone(),
            *interval,
            *output,
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
            commands::config::run_set(&path, key, value, dry, *output)
        }
        Command::Config {
            config_cmd: ConfigCommand::Validate { output },
        } => {
            let path = match &cli.config {
                Some(p) => p.clone(),
                None => resolve::resolve_config_path(cli.container.as_deref())
                    .map_err(ConfigResolveError::into_report)?,
            };
            commands::config::run_validate(&path, *output)
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
            let config_path = podbox::config::find_config_path(name)
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

        // `stop --all` needs no target: it enumerates running containers itself.
        Command::Stop { all: true, .. } => commands::lifecycle::run_stop_all(dry),

        Command::Doctor {
            name: _,
            fix,
            output,
        } => dispatch_doctor(ctx, cli, target, *fix, *output),

        // Mirror arms: these (arg-flagged) modes go through shared
        // resolution instead, see `needs_config`.
        Command::Disable { force: false, .. }
        | Command::Remove { stale: false, .. }
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

/// `doctor` resolves the definition itself: a broken one must reach the
/// doctor's `--fix` offer instead of aborting the dispatch in shared
/// resolution. This is the plain-`resolve_config` flow under those rules.
fn dispatch_doctor(
    ctx: &Ctx,
    cli: &Cli,
    target: Option<String>,
    fix: bool,
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
            commands::runtime::run_doctor(&config, &env, apply_fix, output)
        }
        Err(err) => {
            // The only resolution failure doctor can repair in place is a
            // bare `container.memory` (typed match, not message text).
            if apply_fix
                && err.is_bare_memory()
                && commands::runtime::try_fix_bare_memory_for_target(target.as_deref(), apply_fix)?
            {
                let (config, _) = resolve::resolve_config(cli, target)
                    .map_err(ConfigResolveError::into_report)?;
                let env = podbox::env::resolve()?;
                return commands::runtime::run_doctor(&config, &env, apply_fix, output);
            }
            Err(err.into_report())
        }
    }
}

/// Run a command that needs one merged definition.
///
/// Resolution happens exactly once through [`resolve::resolve_config`] — the
/// single chain, with typed errors so `main`'s exit mapping keeps its codes.
pub(crate) fn dispatch_with_config(ctx: &Ctx, cli: &Cli, target: Option<String>) -> Result<()> {
    let dry = ctx.dry_run;
    let (config, name) =
        resolve::resolve_config(cli, target.clone()).map_err(ConfigResolveError::into_report)?;

    let env = podbox::env::resolve()?;
    let xdg = podbox::xdg::resolve(&config.integration.xdg_dirs)?;

    match &cli.command {
        Command::Build {
            name: _,
            rebuild,
            no_diff,
            edit,
            output,
        } => {
            if matches!(output, OutputFormat::Json) {
                anyhow::bail!("`podbox build` supports text and ndjson output, not pretty json");
            }
            if *edit {
                edit_config_for_command(ctx, cli)?;
            }
            commands::lifecycle::run_build(&config, &env, &xdg, dry, *rebuild, *no_diff, *output)?;
        }

        Command::Enable { name: _, yes } => {
            commands::lifecycle::run_enable(&config, &env, &xdg, dry, *yes)?;
        }

        Command::Disable { force: false, .. } => {
            commands::lifecycle::run_disable(&name, dry)?;
        }

        Command::Start {
            name: _,
            timeout,
            edit,
        } => {
            if *edit {
                edit_config_for_command(ctx, cli)?;
            }
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
            edit,
            here,
            env: env_overrides,
        } => {
            if *edit {
                edit_config_for_command(ctx, cli)?;
            }
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
            commands::runtime::run_status(&config, &name, dry, *output)?;
        }

        Command::Config {
            config_cmd: ConfigCommand::Get { key, output, .. },
        } => {
            commands::config::run_get(&config, key, *output)?;
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
            commands::runtime::run_logs(&name, *follow, *tail, since.clone(), dry)?;
        }

        Command::Diff { apply, output, .. } => {
            commands::diff::run_diff(&config, &name, &env.username, *apply, dry, *output)?;
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
                *output,
            )?;
        }

        Command::Export { export_cmd } => {
            commands::export::run_export(&name, Some(&config), export_cmd, dry)?;
        }

        Command::Remove {
            name: _,
            all,
            force,
            remove_config,
            stale: false,
        } => {
            commands::lifecycle::run_remove(&config, &name, dry, *all, *force, *remove_config)?;
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
                commands::provision::status(&config, *output, *check)?;
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
        | Command::Remove { stale: true, .. }
        | Command::Stop { all: true, .. }
        | Command::Config {
            config_cmd: ConfigCommand::Set { .. } | ConfigCommand::Validate { .. },
        } => {}
    }

    Ok(())
}
