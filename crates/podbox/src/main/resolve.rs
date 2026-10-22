//! The single config-resolution chain.
//!
//! Everything that wants a definition file comes through here in one shape:
//! an explicit `--config` path first, otherwise a container name (positional
//! → `-C` → `$PODBOX_CONTAINER` → active context), otherwise the single
//! on-disk config (with an interactive choice, or a refusal for scripts, when
//! there are several). Failures are typed ([`ConfigResolveError`]) instead of
//! matched from strings, so a caller that can act on a specific failure (the
//! doctor's bare-`memory` offer) matches the variant, and one conversion
//! ([`ConfigResolveError::into_report`]) pins the dispatch contract's exit
//! codes and error-text wording in exactly one place.

use std::path::PathBuf;

use podbox::cli::{Cli, Command};
use podbox::config::{self, Config};
use podbox::error::PodboxError;
use podbox::ui;

/// Why the chain refused to produce a definition.
#[derive(Debug)]
pub(crate) enum ConfigResolveError {
    /// No definition file for the container a command named.
    ///
    /// Converted to [`PodboxError::ContainerMissing`]: exit code 3 and the
    /// `container_missing` JSON code for clients.
    ContainerMissing(String),

    /// Several definition files could serve the command, and there is no
    /// interactive session to pick one in.
    Ambiguous { names: Vec<String> },

    /// No config exists and there is no fallback definition to load.
    NotFound,

    /// A definition file was found, but loading or validating it failed.
    ///
    /// The wrapped error keeps its typed source ([`PodboxError`] variants),
    /// so exit codes survive the conversion; `hint` marks the named-lookup
    /// path, whose report carries the `--config`/`-C` pointer the old chain
    /// appended. Auto-detected configs report the raw error unchanged. The
    /// `path` is carried for a future caller that wants to point at the
    /// offending file (the reports deliberately keep the old wording).
    Invalid {
        /// Carried so a future caller can point at the offending file; the
        /// reports deliberately keep the old wording without it.
        #[allow(dead_code)]
        path: PathBuf,
        err: anyhow::Error,
        hint: bool,
    },

    /// An already-final message built while resolving (the first-run wizard
    /// offers and its fallbacks). Printed as-is.
    Report(anyhow::Error),
}

impl ConfigResolveError {
    /// The dispatch contract's final report for this failure.
    ///
    /// `main`/`exit_code_for_error` downcasts the returned chain, so the
    /// typed variants below keep their exit codes and JSON error codes.
    pub(crate) fn into_report(self) -> anyhow::Error {
        match self {
            Self::ContainerMissing(name) => PodboxError::ContainerMissing(name).into(),
            Self::Report(err) => err,
            Self::NotFound => anyhow::anyhow!(
                "No container configs found. Create one with `podbox init --interactive` \
                 or specify a config with `--config <PATH>` / `-C <NAME>`."
            ),
            Self::Ambiguous { names } => anyhow::anyhow!(
                "Multiple container configs found ({}).\n\n\
                 Hint: Pass `-C <NAME>`, set $PODBOX_CONTAINER, or pin one with\n\
                `podbox use <NAME>` for non-interactive use.",
                names.join(", ")
            ),
            Self::Invalid {
                err, hint: false, ..
            } => err,
            Self::Invalid { err, .. } => anyhow::anyhow!(
                "{err}\n\nHint: Use `--config <PATH>` to specify a config file, or `-C <NAME>` to use a config from {}",
                config::config_dir().display()
            ),
        }
    }

    /// True when a config failed `container.memory` validation — the only
    /// resolution failure `doctor --fix` can repair in place. Matched on the
    /// typed [`PodboxError::ConfigValidationFailed`] entry list, not on
    /// whole-message text.
    pub(crate) fn is_bare_memory(&self) -> bool {
        match self {
            Self::Invalid { err, .. } => {
                err.downcast_ref::<PodboxError>().is_some_and(|e| {
                    matches!(e, PodboxError::ConfigValidationFailed { details }
                    if details.lines().any(|line| {
                        // Validation joins later entries with an indented
                        // bullet (`"  - "`), which strips off with the
                        // surrounding whitespace.
                        line.trim_start()
                            .trim_start_matches("- ")
                            .starts_with("container.memory:")
                    }))
                })
            }
            _ => false,
        }
    }
}

/// Resolve the definition file path for `target` (or auto-detect one).
///
/// The path-only stage of the chain: commands that re-read the file
/// themselves (`config set`/`validate`, the editor commands) stop here and
/// never see a merged config, so a broken file still gets its precise
/// report instead of dying in shared resolution.
pub(crate) fn resolve_config_path(
    target: Option<&str>,
) -> std::result::Result<PathBuf, ConfigResolveError> {
    if let Some(name) = target {
        let Some(path) = config::find_config_path(name) else {
            // Human detail stays on stderr; the typed error carries exit 3
            // and the `container_missing` code for JSON-mode clients.
            ui::hint(&[&format!(
                "no config for '{name}' at '{}/{{profiles/,}}{name}.toml' — use `--config <PATH>` or `-C <NAME>` from {}",
                config::config_dir().display(),
                config::config_dir().display(),
            )]);
            return Err(ConfigResolveError::ContainerMissing(name.to_string()));
        };
        return Ok(path);
    }

    let configs = config::list_configs();
    match configs.as_slice() {
        [] => config::find_definition().ok_or(ConfigResolveError::NotFound),
        [only] => Ok(only.clone()),
        many => {
            if podbox::codegen::distros::is_tty() {
                let selection = config::select_config_interactive(many, "Select container")
                    .map_err(ConfigResolveError::Report)?;
                Ok(selection)
            } else {
                let names: Vec<String> = many
                    .iter()
                    .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
                    .collect();
                Err(ConfigResolveError::Ambiguous { names })
            }
        }
    }
}

/// Resolve and load the definition for a command.
///
/// Returns the merged config together with its runtime name (the name podman
/// and systemd know; a config file name may differ). A named target carries
/// the `--config`/`-C` hint on load failures; auto-detected ones keep the
/// raw typed error.
pub(crate) fn resolve_config(
    cli: &Cli,
    target_name: Option<String>,
) -> std::result::Result<(Config, String), ConfigResolveError> {
    let config = match &cli.config {
        // A missing explicit path is an error, never the embedded default:
        // `--config typo.toml rm --force` must not act on some other box.
        Some(path) => {
            if !path.is_file() {
                return Err(ConfigResolveError::Invalid {
                    path: path.clone(),
                    err: PodboxError::ConfigFileMissing { path: path.clone() }.into(),
                    hint: false,
                });
            }
            Config::load(path).map_err(|err| ConfigResolveError::Invalid {
                path: path.clone(),
                err,
                hint: false,
            })?
        }
        None => load_by_lookup(cli, target_name)?,
    };

    let name = config.container.name.clone();
    let mut config = config;

    apply_image_labels(&cli.command, &mut config);
    Ok((config, name))
}

/// [`resolve_config`] for cleanup commands (`remove` the container): resolve
/// the file the same way, but load it leniently so a definition the
/// validator rejects — a legacy file the D-Bus rule now flags, say — can
/// still be deleted. Structural failures refuse exactly as before.
pub(crate) fn resolve_config_lenient(
    cli: &Cli,
    target_name: Option<String>,
) -> std::result::Result<(Config, String), ConfigResolveError> {
    let config = match &cli.config {
        Some(path) => {
            if !path.is_file() {
                return Err(ConfigResolveError::Invalid {
                    path: path.clone(),
                    err: PodboxError::ConfigFileMissing { path: path.clone() }.into(),
                    hint: false,
                });
            }
            Config::load_lenient(path).map_err(|err| ConfigResolveError::Invalid {
                path: path.clone(),
                err,
                hint: false,
            })?
        }
        None => load_lenient_by_lookup(cli, target_name)?,
    };

    let name = config.container.name.clone();
    Ok((config, name))
}

/// Resolve the file a name chain points to, walking a fresh user through
/// the first-run wizard when nothing exists yet. Both load strategies
/// ([`load_by_lookup`], [`load_lenient_by_lookup`]) share this.
fn lookup_path_with_wizard(
    cli: &Cli,
    target_name: Option<String>,
) -> std::result::Result<PathBuf, ConfigResolveError> {
    let mut retried_after_wizard = false;
    loop {
        match resolve_config_path(target_name.as_deref()) {
            Ok(path) => return Ok(path),
            Err(err @ ConfigResolveError::NotFound) if !retried_after_wizard => {
                // No configs at all — welcome the user and offer the wizard
                // (TTY only; scripts get the typed refusal instead of a
                // prompt they could never answer).
                if !podbox::codegen::distros::is_tty() {
                    return Err(ConfigResolveError::Report(anyhow::anyhow!(
                        "No container configs found.\n\n\
                         Hint: Run `podbox init -i` to create one interactively, or\n\
                        `podbox create <profile>` for a one-shot setup, or\n\
                        point at a file with `--config <PATH>`."
                    )));
                }
                eprintln!(
                    "Welcome to podbox! It looks like you don't have any containers set up yet."
                );
                let launch =
                    dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
                        .with_prompt("Would you like to run the interactive setup wizard?")
                        .default(true)
                        .interact()
                        .unwrap_or(false);
                if !launch {
                    // The chain's own refusal wording covers the declined
                    // wizard; nothing special left to add here.
                    return Err(err);
                }
                crate::commands::create::run_init(cli.dry_run, None, None, true, None)
                    .map_err(ConfigResolveError::Report)?;
                // Continue with the config the wizard wrote, not the embedded
                // default. The retry sees a non-empty list, so it cannot loop.
                if config::list_configs().is_empty() {
                    return Err(ConfigResolveError::Report(anyhow::anyhow!(
                        "Setup finished without creating a config. Run `podbox init` to create one."
                    )));
                }
                retried_after_wizard = true;
            }
            Err(err) => return Err(err),
        }
    }
}

fn load_by_lookup(
    cli: &Cli,
    target_name: Option<String>,
) -> std::result::Result<Config, ConfigResolveError> {
    let hinted = target_name.is_some();
    let path = lookup_path_with_wizard(cli, target_name)?;
    Config::load(&path).map_err(|err| ConfigResolveError::Invalid {
        path,
        err,
        hint: hinted,
    })
}

/// [`load_by_lookup`] through [`Config::load_lenient`].
fn load_lenient_by_lookup(
    cli: &Cli,
    target_name: Option<String>,
) -> std::result::Result<Config, ConfigResolveError> {
    let hinted = target_name.is_some();
    let path = lookup_path_with_wizard(cli, target_name)?;
    Config::load_lenient(&path).map_err(|err| ConfigResolveError::Invalid {
        path,
        err,
        hint: hinted,
    })
}

/// Commands that need image label defaults applied to the config.
///
/// These generate Quadlet files or build the image — the rest can skip
/// the ~100ms `podman inspect` fork.
fn needs_image_labels(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::Build { .. } | Command::Enable { .. } | Command::Update { .. }
    )
}

/// Apply the built image's stored defaults (packages, settings the image
/// was built with) onto the command's view of the config.
fn apply_image_labels(cmd: &Command, config: &mut Config) {
    if !needs_image_labels(cmd) {
        return;
    }
    let local_tag = config.local_image_tag();
    if let Ok(true) = podbox::podman::image_exists(&local_tag)
        && let Ok(labels) = podbox::labels::fetch(&local_tag)
    {
        podbox::labels::apply_defaults(config, &labels);
    }
}

#[cfg(test)]
mod tests {
    use super::ConfigResolveError;
    use podbox::error::PodboxError;

    fn invalid_validation(details: &str) -> ConfigResolveError {
        invalid_validation_hint(details, true)
    }

    /// `hint` mirrors the named-lookup path: `true` wraps the report with the
    /// `--config`/`-C` pointer, `false` is the auto-detected case.
    fn invalid_validation_hint(details: &str, hint: bool) -> ConfigResolveError {
        ConfigResolveError::Invalid {
            path: std::path::PathBuf::from("dev.toml"),
            err: PodboxError::ConfigValidationFailed {
                details: details.to_string(),
            }
            .into(),
            hint,
        }
    }

    #[test]
    fn bare_memory_recognizes_the_typed_validation_variant() {
        let err = invalid_validation("container.memory: '2' is not a valid memory limit");
        assert!(err.is_bare_memory());
    }

    #[test]
    fn bare_memory_recognizes_memory_among_several_failures() {
        // Validation joins later entries with an indented bullet.
        let err = invalid_validation(
            "container.cpus: 'x' is not a valid CPU count\n  \
             - container.memory: '2' is not a valid memory limit",
        );
        assert!(err.is_bare_memory());
    }

    #[test]
    fn other_validation_failures_are_not_the_memory_offer() {
        let err = invalid_validation("container.cpus: 'x' is not a valid CPU count (e.g. '2.0')");
        assert!(!err.is_bare_memory());
    }

    #[test]
    fn the_mention_of_the_key_in_unrelated_text_does_not_count() {
        // The old string match on the flattened message would hit this; the
        // per-validation-entry check must not.
        let err = invalid_validation(
            "container.mounts.extra[0]: '~/container.memory:notes' missing ':' separator",
        );
        assert!(!err.is_bare_memory());
    }

    #[test]
    fn missing_container_keeps_typedness_through_the_conversion() {
        let report = ConfigResolveError::ContainerMissing("dev".into()).into_report();
        let typed = report
            .downcast_ref::<PodboxError>()
            .expect("container-missing must stay a PodboxError");
        assert_eq!(typed.code(), "container_missing");
    }

    #[test]
    fn invalid_auto_reports_stay_raw_so_exit_codes_survive() {
        let report = invalid_validation_hint("some other invalid key: value", false).into_report();
        assert!(
            report
                .downcast_ref::<PodboxError>()
                .is_some_and(|e| matches!(e, PodboxError::ConfigValidationFailed { .. })),
            "auto-detected configs keep their typed error"
        );
    }

    #[test]
    fn invalid_named_reports_carry_the_config_hint() {
        let report = invalid_validation("container.memory: '2' is not a valid memory limit")
            .into_report()
            .to_string();
        assert!(
            report.contains("Hint: Use `--config <PATH>`"),
            "named lookups keep the pointer: {report}"
        );
    }
}
