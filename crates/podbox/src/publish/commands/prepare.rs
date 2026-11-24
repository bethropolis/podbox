use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;

use crate::error::PodboxError;
use crate::publish::context::{PublishContext, RefType};
use crate::publish::discover::{Discovery, discover, list_dir};
use crate::publish::manifest::Manifest;

use super::PublishOpts;

/// Synthesize a manifest for a lone `podbox.toml`, which is a container
/// definition rather than an `images.toml` manifest.
fn lone_podbox_manifest(config: String) -> Manifest {
    let mut images = BTreeMap::new();
    images.insert(
        "podbox".to_owned(),
        crate::publish::manifest::ImageEntry {
            config,
            tags: Vec::new(),
            platforms: Vec::new(),
            name: None,
            containerfile: None,
        },
    );
    Manifest {
        schema_version: 1,
        registry: "ghcr.io".into(),
        platforms: Vec::new(),
        tags: vec!["latest".into(), "sha".into()],
        containerfile: None,
        images,
    }
}

/// Load manifest: explicit --manifest or discovery.
pub(super) fn load_manifest(
    manifest_path: &Path,
    explicit: bool,
    dir: &Path,
) -> Result<Manifest, PodboxError> {
    // A `podbox.toml` is a definition, never a manifest. Handle it before the
    // "path exists" branch so it is not parsed with the manifest schema.
    if manifest_path.file_name().and_then(|n| n.to_str()) == Some("podbox.toml") {
        let relative = manifest_path
            .strip_prefix(dir)
            .unwrap_or(Path::new("podbox.toml"));
        return Ok(lone_podbox_manifest(
            relative.to_string_lossy().into_owned(),
        ));
    }
    if manifest_path.exists() {
        let content =
            std::fs::read_to_string(manifest_path).map_err(|e| PodboxError::ManifestInvalid {
                details: format!("failed to read manifest '{}': {e}", manifest_path.display()),
            })?;
        return Manifest::parse(&content).map_err(|e| PodboxError::ManifestInvalid {
            details: e.to_string(),
        });
    }
    if explicit {
        return Err(PodboxError::ManifestMissing {
            dir: manifest_path.display().to_string(),
        });
    }
    // Discovery.
    let entries = list_dir(dir).map_err(|e| PodboxError::ManifestInvalid {
        details: format!("failed to list directory '{}': {e}", dir.display()),
    })?;
    match discover(&entries, dir) {
        Ok(Discovery::ImagesToml(path)) => {
            let content =
                std::fs::read_to_string(&path).map_err(|e| PodboxError::ManifestInvalid {
                    details: format!("failed to read manifest '{}': {e}", path.display()),
                })?;
            Manifest::parse(&content).map_err(|e| PodboxError::ManifestInvalid {
                details: e.to_string(),
            })
        }
        Ok(Discovery::EnvsDir { images, .. }) => Ok(Manifest {
            schema_version: 1,
            registry: "ghcr.io".into(),
            platforms: Vec::new(),
            tags: vec!["latest".into(), "sha".into()],
            containerfile: None,
            images,
        }),
        Ok(Discovery::LonePodboxToml(_path)) => Ok(lone_podbox_manifest("podbox.toml".into())),
        Err(e) => Err(PodboxError::ManifestMissing { dir: e.dir }),
    }
}

pub(super) fn parse_ref_type(value: &str) -> Result<RefType, PodboxError> {
    match value {
        "branch" => Ok(RefType::Branch),
        "tag" => Ok(RefType::Tag),
        other => Err(PodboxError::InvalidOptionValue {
            option: "--ref-type",
            value: other.into(),
            expected: "'branch' or 'tag'",
        }),
    }
}

/// Apply explicit CLI context flags on top of `--ci github` values.
pub(super) fn apply_context_overrides(
    context: &mut PublishContext,
    opts: &PublishOpts,
) -> Result<(), PodboxError> {
    if let Some(value) = &opts.owner {
        context.owner.clone_from(value);
    }
    if let Some(value) = &opts.repo {
        context.repo.clone_from(value);
    }
    if let Some(value) = &opts.sha {
        context.sha.clone_from(value);
    }
    if let Some(value) = &opts.ref_name {
        context.ref_name.clone_from(value);
    }
    if let Some(value) = &opts.event {
        context.event.clone_from(value);
    }
    if let Some(value) = &opts.default_branch {
        context.default_branch = Some(value.clone());
    }
    if let Some(value) = &opts.ref_type {
        context.ref_type = parse_ref_type(value)?;
    }
    Ok(())
}

/// True if the manifest source is a lone podbox.toml (affects ref derivation).
pub(super) fn manifest_source_is_lone_podbox_toml(
    opts: &PublishOpts,
    manifest_path: &Path,
) -> Result<bool, PodboxError> {
    if let Some(m) = &opts.manifest {
        return Ok(m.file_name().and_then(|n| n.to_str()) == Some("podbox.toml"));
    }
    let entries = list_dir(manifest_path.parent().unwrap_or(Path::new("."))).map_err(|e| {
        PodboxError::ManifestInvalid {
            details: e.to_string(),
        }
    })?;
    match discover(&entries, manifest_path.parent().unwrap_or(Path::new("."))) {
        Ok(Discovery::LonePodboxToml(_)) => Ok(true),
        _ => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lone_podbox_toml_is_a_definition_not_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("podbox.toml");
        std::fs::write(&path, "[image]\nbase = \"alpine:latest\"\n").unwrap();

        let manifest = load_manifest(&path, false, dir.path()).unwrap();
        assert_eq!(manifest.images.len(), 1);
        assert_eq!(manifest.images["podbox"].config, "podbox.toml");
    }
}
