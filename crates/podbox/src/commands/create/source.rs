use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use podbox::codegen::distros;
use podbox::config::Config;
use podbox::editor;

/// Open a config file in the user's editor. Callers re-load the file after
/// this returns, so edits (or pasted Studio TOML) take effect.
pub(super) fn edit_config_file(path: &Path) -> Result<()> {
    let ed = editor::resolve()?;
    editor::open(&ed, path)?;
    Ok(())
}

pub(in crate::commands) fn read_profile_content(profile: &str) -> Result<String> {
    if profile.contains('/') || profile.contains('\\') {
        std::fs::read_to_string(Path::new(profile))
            .with_context(|| format!("failed to read profile '{profile}'"))
    } else {
        let found = podbox::profiles::find(profile).ok_or_else(|| {
            let names = podbox::profiles::list_names();
            anyhow::anyhow!(
                "Unknown profile '{}'. Available profiles: {}",
                profile,
                names.join(", ")
            )
        })?;
        Ok(found.toml)
    }
}

/// Refuse a new config whose runtime name another config already claims.
///
/// Lookups by name use the file stem, so `work.toml` with `container.name =
/// "dev"` would not be found by `create --name dev`. Writing `dev.toml` then
/// would give two configs for one container. Checking the runtime name here
/// stops that.
pub(super) fn ensure_runtime_name_free(existing: &[PathBuf], container_name: &str) -> Result<()> {
    let taken = existing
        .iter()
        .find(|path| Config::load(path).is_ok_and(|cfg| cfg.container.name == container_name));
    if let Some(path) = taken {
        anyhow::bail!(
            "container '{container_name}' is already defined by '{}'. Choose another name or edit that config.",
            path.display()
        );
    }
    Ok(())
}

pub(in crate::commands) fn derive_container_name(image: &str, custom_name: Option<&str>) -> String {
    if let Some(name) = custom_name {
        return name.to_string();
    }
    let image_part = image.split_once(':').map_or(image, |(n, _)| n);
    let short = image_part.split('/').next_back().unwrap_or(image_part);
    let tag = image.split_once(':').map_or("latest", |(_, t)| t);
    if tag == "latest" || tag.is_empty() {
        short.to_string()
    } else {
        format!("{}-{}", short, tag.replace('.', "-"))
    }
}

pub(in crate::commands) fn detect_package_manager(image: &str) -> podbox::config::PackageManager {
    distros::detect_package_manager(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(dir: &Path, file: &str, runtime: &str) -> PathBuf {
        let mut cfg = Config::embedded();
        cfg.container.name = runtime.to_string();
        cfg.image.name = runtime.to_string();
        let path = dir.join(file);
        std::fs::write(&path, toml::to_string_pretty(&cfg).unwrap()).unwrap();
        path
    }

    #[test]
    fn new_config_refused_when_runtime_name_is_claimed_by_another_file() {
        let dir = std::env::temp_dir().join(format!("podbox-create-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let work = write_config(&dir, "work.toml", "dev");
        let other = write_config(&dir, "other.toml", "other");
        let existing = vec![work.clone(), other];

        let err = ensure_runtime_name_free(&existing, "dev").unwrap_err();
        assert!(err.to_string().contains("already defined by"), "{err}");
        assert!(err.to_string().contains(&work.display().to_string()));
        assert!(ensure_runtime_name_free(&existing, "fresh").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
