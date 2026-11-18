//! Discovery rules: find `images.toml` or `envs/*.toml` or a lone
//! `podbox.toml` under `--dir`.
//!
//! Pure module: given a file listing, produce a discovery result.

use std::path::{Path, PathBuf};

use crate::publish::manifest::ImageEntry;

const MANIFEST_NAME: &str = "images.toml";
const ENV_DIR: &str = "envs";
const PODBOX_TOML: &str = "podbox.toml";

/// Where the manifest comes from.
#[derive(Debug, Clone)]
pub enum Discovery {
    /// `images.toml` exists at this path; its image entries are parsed from
    /// file content by the caller.
    ImagesToml(PathBuf),
    /// `envs/*.toml`: one image per file, key = file stem, sorted.
    EnvsDir {
        dir: PathBuf,
        images: std::collections::BTreeMap<String, ImageEntry>,
    },
    /// A lone `podbox.toml`: one image, key = `podbox`, lone ref rule.
    LonePodboxToml(PathBuf),
}

/// Error: nothing discovered.
#[derive(Debug, thiserror::Error)]
#[error("No images.toml, {env_dir}/*.toml or {podbox_toml} found under '{dir}'")]
pub struct ManifestMissing {
    pub env_dir: &'static str,
    pub podbox_toml: &'static str,
    pub dir: String,
}

/// Given the entries (relative paths) under `--dir`, discover a manifest
/// source. Evaluated in order, first match wins:
///   1. `images.toml` in `--dir`.
///   2. `envs/*.toml`: one image per file, key = file stem, sorted by name.
///   3. `podbox.toml`: one image, key = `podbox` (lone-podbox-toml ref rule).
///   4. Otherwise error `manifest_missing`.
pub fn discover(entries: &[PathBuf], dir: &Path) -> Result<Discovery, ManifestMissing> {
    let entry_names: Vec<String> = entries
        .iter()
        .filter_map(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
        .collect();

    // 1. images.toml
    if entry_names.iter().any(|n| n == MANIFEST_NAME) {
        return Ok(Discovery::ImagesToml(dir.join(MANIFEST_NAME)));
    }

    // 2. envs/*.toml
    let env_files: Vec<&PathBuf> = entries
        .iter()
        .filter(|p| {
            p.parent()
                .is_some_and(|parent| parent.file_name().is_some_and(|n| n == ENV_DIR))
                && p.extension().is_some_and(|e| e == "toml")
        })
        .collect();
    if !env_files.is_empty() {
        let mut images = std::collections::BTreeMap::new();
        for f in env_files {
            let stem = f
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let config = format!("{ENV_DIR}/{stem}.toml");
            images.insert(
                stem.clone(),
                ImageEntry {
                    config,
                    tags: Vec::new(),
                    platforms: Vec::new(),
                    name: None,
                    containerfile: None,
                },
            );
        }
        return Ok(Discovery::EnvsDir {
            dir: dir.join(ENV_DIR),
            images,
        });
    }

    // 3. lone podbox.toml
    if entry_names.iter().any(|n| n == PODBOX_TOML) {
        return Ok(Discovery::LonePodboxToml(dir.join(PODBOX_TOML)));
    }

    // 4. nothing found
    Err(ManifestMissing {
        env_dir: ENV_DIR,
        podbox_toml: PODBOX_TOML,
        dir: dir.display().to_string(),
    })
}

/// Read the entries under `--dir` for discovery: the files directly in it
/// plus the TOML files of an `envs/` subdirectory (one level deep).
pub fn list_dir(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut out = Vec::new();
    if let Ok(read_dir) = std::fs::read_dir(dir) {
        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.is_file() {
                out.push(path);
            } else if path.is_dir() && path.file_name().is_some_and(|n| n == ENV_DIR) {
                let Ok(envs) = std::fs::read_dir(&path) else {
                    continue;
                };
                for env in envs.flatten() {
                    let env_path = env.path();
                    if env_path.is_file() && env_path.extension().is_some_and(|e| e == "toml") {
                        out.push(env_path);
                    }
                }
            }
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_envs_dir() {
        let entries = vec![
            PathBuf::from("envs/dev.toml"),
            PathBuf::from("envs/gaming.toml"),
        ];
        let d = discover(&entries, Path::new(".")).unwrap();
        assert!(matches!(d, Discovery::EnvsDir { .. }));
        if let Discovery::EnvsDir { images, .. } = &d {
            assert_eq!(images.len(), 2);
            assert!(images.contains_key("dev"));
            assert!(images.contains_key("gaming"));
        }
    }

    #[test]
    fn discovers_lone_podbox_toml() {
        let entries = vec![PathBuf::from("podbox.toml")];
        let d = discover(&entries, Path::new(".")).unwrap();
        assert!(matches!(d, Discovery::LonePodboxToml(_)));
    }

    #[test]
    fn discovers_images_toml() {
        let entries = vec![PathBuf::from("images.toml")];
        let d = discover(&entries, Path::new(".")).unwrap();
        assert!(matches!(d, Discovery::ImagesToml(_)));
    }

    #[test]
    fn list_dir_sees_envs_toml_files() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("envs")).unwrap();
        std::fs::write(dir.path().join("envs/dev.toml"), "").unwrap();
        std::fs::write(dir.path().join("envs/notes.txt"), "").unwrap();

        let entries = list_dir(dir.path()).unwrap();
        let discovered = discover(&entries, dir.path()).unwrap();
        assert!(matches!(discovered, Discovery::EnvsDir { .. }));
        if let Discovery::EnvsDir { images, .. } = &discovered {
            assert_eq!(images.len(), 1);
            assert_eq!(images["dev"].config, "envs/dev.toml");
        }
    }

    #[test]
    fn errors_when_nothing_found() {
        let entries = vec![PathBuf::from("README.md")];
        let err = discover(&entries, Path::new(".")).unwrap_err();
        assert!(err.to_string().contains("No images.toml"));
    }
}
