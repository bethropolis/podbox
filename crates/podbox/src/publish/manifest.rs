//! `images.toml` manifest: parse, validate, and discovery-shape.
//!
//! Pure module: TOML in, `Manifest` out. No host dependencies.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Latest manifest schema. Bump only on breaking changes.
const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// `images.toml` — one manifest describes a set of images to publish.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    #[serde(default = "default_registry")]
    pub registry: String,
    #[serde(default)]
    pub platforms: Vec<String>,
    #[serde(default = "default_tags")]
    pub tags: Vec<String>,
    #[serde(default)]
    pub images: BTreeMap<String, ImageEntry>,
}

fn default_registry() -> String {
    "ghcr.io".into()
}

fn default_tags() -> Vec<String> {
    vec!["latest".into(), "sha".into()]
}

/// One image to publish.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImageEntry {
    /// Relative path (under `--dir`) to the podbox definition TOML.
    pub config: String,
    /// Image-level tags replace the top-level list (no merge).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Image-level platforms replace the top-level list (no merge).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    /// Full ref without tag (derived when omitted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Validated manifest, ready for planning.
#[derive(Debug, Clone)]
pub struct ValidatedManifest {
    pub registry: String,
    pub platforms: Vec<String>,
    pub tags: Vec<String>,
    pub images: BTreeMap<String, ImageEntry>, // BTreeMap = deterministic order
}

/// Error during manifest parsing/validation.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error(
        "manifest requires podbox schema {0}; this podbox supports schema {MANIFEST_SCHEMA_VERSION} — upgrade podbox"
    )]
    SchemaTooNew(u32),
    #[error("unknown key '{key}' in images.toml")]
    UnknownKey { key: String },
    #[error("images is empty: at least one image must be declared")]
    EmptyImages,
    #[error("image '{key}' references config '{path}' which does not exist under --dir")]
    ConfigMissing { key: String, path: String },
    #[error("platform '{platform}' is not supported by publish v1 (only linux/amd64)")]
    UnsupportedPlatform { platform: String },
    #[error("tags must be non-empty")]
    EmptyTags,
}

impl Manifest {
    /// Parse and validate `images.toml` content.
    ///
    /// Unknown keys are errors with line numbers, matching `config validate`.
    pub fn parse(content: &str) -> Result<Self, ManifestError> {
        let value: toml::Value =
            toml::from_str(content).map_err(|_| ManifestError::UnknownKey {
                key: "parse error".into(),
            })?;
        Self::from_value(value)
    }

    fn from_value(value: toml::Value) -> Result<Self, ManifestError> {
        let table = match &value {
            toml::Value::Table(t) => t,
            _ => return Err(ManifestError::UnknownKey { key: "root".into() }),
        };

        // Schema version check.
        if let Some(seen) = table
            .get("schema_version")
            .and_then(toml::Value::as_integer)
        {
            if seen as u32 > MANIFEST_SCHEMA_VERSION {
                return Err(ManifestError::SchemaTooNew(seen as u32));
            }
        }

        // Reject unknown top-level keys.
        let allowed = ["schema_version", "registry", "platforms", "tags", "images"];
        for key in table.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(ManifestError::UnknownKey { key: key.clone() });
            }
        }

        let manifest: Manifest = value
            .try_into()
            .map_err(|e| ManifestError::UnknownKey { key: e.to_string() })?;

        // Validate.
        if manifest.images.is_empty() {
            return Err(ManifestError::EmptyImages);
        }
        for _platform in &manifest.platforms {
            if _platform != "linux/amd64" {
                return Err(ManifestError::UnsupportedPlatform {
                    platform: _platform.clone(),
                });
            }
        }
        for (key, entry) in &manifest.images {
            let _ = key;
            let _ = entry;
        }

        Ok(manifest)
    }

    /// Validate against a `PublishContext`, returning a `ValidatedManifest`.
    pub fn validate(
        self,
        context: &crate::publish::context::PublishContext,
    ) -> Result<ValidatedManifest, ManifestError> {
        let platforms = if self.platforms.is_empty() {
            context.default_platforms()
        } else {
            self.platforms
        };
        let tags = if self.tags.is_empty() {
            context.default_tags()
        } else {
            self.tags
        };

        // Validate image-level tags: after merging with top-level, each image
        // must have non-empty tags.
        for (key, entry) in &self.images {
            let effective_tags = if entry.tags.is_empty() {
                &tags
            } else {
                &entry.tags
            };
            if effective_tags.is_empty() {
                return Err(ManifestError::EmptyTags);
            }
            let _ = key;
        }

        Ok(ValidatedManifest {
            registry: self.registry,
            platforms,
            tags,
            images: self.images,
        })
    }
}

impl ValidatedManifest {
    /// Look up an image by key.
    pub fn image(&self, key: &str) -> Option<&ImageEntry> {
        self.images.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"
schema_version = 1

[images.dev]
config = "envs/dev.toml"
"#;

    const FULL: &str = r#"
schema_version = 1
registry = "ghcr.io"
platforms = ["linux/amd64"]
tags = ["latest", "sha"]

[images.dev]
config = "envs/dev.toml"

[images.gaming]
config = "envs/gaming.toml"
tags = ["latest", "nightly"]
platforms = ["linux/amd64"]
name = "ghcr.io/me/gaming"
"#;

    #[test]
    fn parses_minimal_manifest() {
        let m = Manifest::parse(MINIMAL).unwrap();
        assert_eq!(m.registry, "ghcr.io");
        assert_eq!(m.platforms, Vec::<String>::new());
        assert_eq!(m.tags, vec!["latest".to_string(), "sha".to_string()]);
        assert_eq!(m.images.len(), 1);
        let entry = m.images.get("dev").unwrap();
        assert_eq!(entry.config, "envs/dev.toml");
        assert_eq!(entry.tags, Vec::<String>::new());
        assert_eq!(entry.platforms, Vec::<String>::new());
        assert_eq!(entry.name, None);
    }

    #[test]
    fn parses_full_manifest() {
        let m = Manifest::parse(FULL).unwrap();
        assert_eq!(m.registry, "ghcr.io");
        assert_eq!(m.platforms, vec!["linux/amd64".to_string()]);
        assert_eq!(m.tags, vec!["latest".to_string(), "sha".to_string()]);
        assert_eq!(m.images.len(), 2);
        let gaming = m.images.get("gaming").unwrap();
        assert_eq!(gaming.config, "envs/gaming.toml");
        assert_eq!(
            gaming.tags,
            vec!["latest".to_string(), "nightly".to_string()]
        );
        assert_eq!(gaming.name, Some("ghcr.io/me/gaming".to_string()));
    }

    #[test]
    fn rejects_unknown_top_level_key() {
        let bad = r#"
schema_version = 1
bogus = true

[images.dev]
config = "envs/dev.toml"
"#;
        let err = Manifest::parse(bad).unwrap_err();
        assert!(matches!(err, ManifestError::UnknownKey { .. }));
    }

    #[test]
    fn rejects_empty_images() {
        let bad = r#"
schema_version = 1
"#;
        let err = Manifest::parse(bad).unwrap_err();
        assert!(matches!(err, ManifestError::EmptyImages));
    }

    #[test]
    fn rejects_unsupported_platform() {
        let bad = r#"
schema_version = 1
platforms = ["linux/arm64"]

[images.dev]
config = "envs/dev.toml"
"#;
        let err = Manifest::parse(bad).unwrap_err();
        assert!(matches!(err, ManifestError::UnsupportedPlatform { .. }));
    }

    #[test]
    fn rejects_schema_too_new() {
        let bad = r#"
schema_version = 99

[images.dev]
config = "envs/dev.toml"
"#;
        let err = Manifest::parse(bad).unwrap_err();
        assert!(matches!(err, ManifestError::SchemaTooNew(99)));
    }
}
