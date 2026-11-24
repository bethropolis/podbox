use anyhow::{Context, Result};

use crate::error::PodboxError;

use super::Config;

impl Config {
    /// Parse with `extends` resolution anchored at `path`.
    ///
    /// Relative / sibling / profile extends are resolved; the merged TOML is
    /// deserialized as a single `Config`. Source-less `parse` keeps its old
    /// behaviour for tests/embedded.
    pub fn parse_with_source(path: &std::path::Path, content: &str) -> Result<Config> {
        let merged = crate::config::extends::resolve_extends_chain(path, content)?;
        let config = Config::from_merged(merged)?;
        config.validate()?;
        Ok(config)
    }

    /// Load for cleanup commands (`remove`): definition-neutral bookkeeping
    /// that must work on a legacy definition the validator rejects. Only
    /// *validation* failures are demoted to printed warnings — structural
    /// failures (unreadable file, unknown keys, newer schema) still refuse,
    /// because working around those could delete more than the container
    /// this command names.
    pub fn load_lenient(path: &std::path::Path) -> Result<Config> {
        if !path.exists() {
            return Err(PodboxError::DefinitionNotFound {
                path: path.to_path_buf(),
            }
            .into());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read definition file '{}'", path.display()))?;
        let merged = crate::config::extends::resolve_extends_chain(path, &content)?;
        let config = Config::from_merged(merged)?;
        let (warnings, result) = config.validate_with_warnings();
        // Nothing printed yet (this path never calls the auto-printing
        // `validate`), so surface every diagnostic here: the parse warnings,
        // and a demoted error verdict when the definition would not load
        // under the strict rules. Printing is deliberate: a cleanup running
        // against a knowingly-invalid definition must say so.
        #[cfg(feature = "cli")]
        for line in warnings {
            crate::config::validation::note_parse_warning(line);
        }
        // wasm has no stderr anyway; the verdict simply stops being fatal.
        #[cfg(not(feature = "cli"))]
        let _ = warnings;
        if let Err(err) = result {
            #[cfg(feature = "cli")]
            crate::config::validation::note_parse_warning(format!(
                "not applying strict validation rules for removal: {err:#}"
            ));
            #[cfg(not(feature = "cli"))]
            let _ = err;
        }
        Ok(config)
    }

    pub fn load(path: &std::path::Path) -> Result<Config> {
        if !path.exists() {
            return Err(PodboxError::DefinitionNotFound {
                path: path.to_path_buf(),
            }
            .into());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read definition file '{}'", path.display()))?;
        Self::parse_with_source(path, &content)
    }

    /// Load without emitting diagnostics; used when scanning for a runtime name.
    pub(crate) fn load_for_lookup(path: &std::path::Path) -> Result<Config> {
        if !path.exists() {
            return Err(PodboxError::DefinitionNotFound {
                path: path.to_path_buf(),
            }
            .into());
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read definition file '{}'", path.display()))?;
        let merged = crate::config::extends::resolve_extends_chain(path, &content)?;
        let config = Config::from_merged(merged)?;
        let (_, result) = config.validate_with_warnings();
        result.map(|()| config)
    }
}
