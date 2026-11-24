#[cfg(feature = "cli")]
use std::path::PathBuf;

#[cfg(feature = "cli")]
use anyhow::Result;

use super::paths::config_dir;

/// Parse a comma-separated package list into trimmed, non-empty entries.
pub fn parse_package_list(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Error for a container name that resolves to no definition file. Built in
/// exactly one place so the text (and what scripts can match on) never
/// drifts between the three dispatch paths that reach it.
pub fn missing_config_error(name: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "no config found for container '{}' at '{}/{{profiles/,}}{}.toml'",
        name,
        config_dir().display(),
        name
    )
}

/// Interactive selection between several definition files, on the file
/// stems (`-C` names, `podbox use` targets). Callers keep their own
/// non-TTY refusals — those messages are genuinely site-specific.
#[cfg(feature = "cli")]
pub fn select_config_interactive(configs: &[PathBuf], prompt: &str) -> Result<PathBuf> {
    let items: Vec<String> = configs
        .iter()
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().into_owned()))
        .collect();
    let selection = dialoguer::Select::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt(prompt)
        .items(&items)
        .default(0)
        .interact()
        .map_err(|e| anyhow::anyhow!("selection failed: {e}"))?;
    Ok(configs[selection].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One error text for every "no config found" call site: name, the
    /// config dir, and the profiles search all spelled out.
    #[test]
    fn missing_config_error_names_the_searched_locations() {
        let err = missing_config_error("dev").to_string();
        assert!(
            err.contains("no config found for container 'dev'")
                && err.contains("{profiles/,}dev.toml"),
            "{err}"
        );
    }
}
