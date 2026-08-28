use std::path::PathBuf;

use anyhow::Result;

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("podbox"))
        .unwrap_or_else(|| PathBuf::from("~/.config/podbox"))
}

/// Canonical profiles directory: ~/.config/podbox/profiles/
pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

/// Resolve a container configuration path by name.
///
/// Resolution order:
/// 1. Canonical path: ~/.config/podbox/profiles/<name>.toml
/// 2. Legacy root path: ~/.config/podbox/<name>.toml (deprecated)
pub fn find_config_path(name: &str) -> Option<PathBuf> {
    let canonical = profiles_dir().join(format!("{name}.toml"));
    if canonical.is_file() {
        return Some(canonical);
    }

    let legacy = config_dir().join(format!("{name}.toml"));
    if legacy.is_file() {
        tracing::debug!(
            "Using legacy config path '{}'. Run `podbox migrate` to move to profiles/.",
            legacy.display()
        );
        return Some(legacy);
    }

    None
}

/// List legacy root-level configs (those not yet migrated to profiles/).
pub fn find_legacy_root_configs() -> Vec<PathBuf> {
    let root = config_dir();
    let profiles = profiles_dir();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return vec![];
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "toml") {
            // Skip files that also exist in profiles/ (canonical wins)
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if profiles.join(format!("{stem}.toml")).is_file() {
                    continue;
                }
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

pub fn find_definition() -> Option<PathBuf> {
    let new_local = PathBuf::from(".podbox.toml");
    if new_local.exists() {
        return Some(new_local);
    }

    let old_local = PathBuf::from(".podmgr.toml");
    if old_local.exists() {
        eprintln!(
            "Warning: '.podmgr.toml' found. Rename it to '.podbox.toml' to silence this warning."
        );
        return Some(old_local);
    }

    // Fall back to any config in config_dir or profiles_dir
    let configs = list_configs();
    if !configs.is_empty() {
        if configs.len() > 1 {
            eprintln!(
                "Warning: multiple configuration files found in {}. Selecting '{}' alphabetically. Use --config to specify a different file.",
                config_dir().display(),
                configs[0].display()
            );
        }
        return Some(configs.into_iter().next().unwrap());
    }

    None
}

pub fn list_configs() -> Vec<PathBuf> {
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, PathBuf> = BTreeMap::new();

    // 1. Scan legacy root (lower priority)
    let root = config_dir();
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "toml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    map.insert(stem.to_string(), path);
                }
            }
        }
    }

    // 2. Scan canonical profiles/ directory (overwrites legacy on collision)
    let pdir = profiles_dir();
    if let Ok(entries) = std::fs::read_dir(&pdir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().is_some_and(|ext| ext == "toml") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    map.insert(stem.to_string(), path);
                }
            }
        }
    }

    let mut out: Vec<PathBuf> = map.into_values().collect();
    out.sort();
    out
}

pub fn active_context_path() -> PathBuf {
    config_dir().join(".active")
}

pub fn read_active_context() -> Option<String> {
    let path = active_context_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let name = content.trim().to_string();
    if name.is_empty() {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    if find_config_path(&name).is_some() {
        Some(name)
    } else {
        let _ = std::fs::remove_file(&path);
        None
    }
}

pub fn write_active_context(name: &str) -> Result<()> {
    let path = active_context_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, name)?;
    Ok(())
}

pub fn clear_active_context() -> Result<()> {
    let path = active_context_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~/foo"), home.join("foo"));
        assert_eq!(expand_tilde("~"), home.clone());
        assert_eq!(expand_tilde("/foo/bar"), PathBuf::from("/foo/bar"));
    }
}
