//! Registry probe: read-only access to an OCI registry for base digest and
//! build-key label lookups.
//!
//! I/O shell: shells out to `skopeo inspect --raw` (preferred, gives the
//! raw manifest incl. labels) with a `podman manifest inspect` fallback.
//! Behind a trait so tests can fake it.

use std::collections::BTreeMap;
use std::process::Command;

use crate::error::PodboxError;

/// Opaque handle to a registry image manifest.
#[derive(Debug, Clone)]
pub struct RemoteImage {
    pub ref_str: String,
    /// Raw manifest JSON (as returned by the registry).
    pub manifest_json: String,
    /// Config digest (`sha256:...`) extracted from the manifest.
    pub config_digest: String,
    /// Labels from the image config.
    pub labels: BTreeMap<String, String>,
}

/// Registry read operations.
pub trait Registry: Send + Sync {
    fn name(&self) -> &str;
    /// Inspect a remote image ref, returning its manifest and config labels.
    /// Returns `None` when the ref does not exist.
    fn inspect(&self, ref_str: &str) -> Result<Option<RemoteImage>, PodboxError>;
}

/// Real registry backend: `skopeo inspect --raw` with `podman` fallback.
pub struct SkopeoRegistry;

impl Registry for SkopeoRegistry {
    fn name(&self) -> &str {
        "skopeo"
    }

    fn inspect(&self, ref_str: &str) -> Result<Option<RemoteImage>, PodboxError> {
        if let Some(img) = inspect_skopeo(ref_str)? {
            return Ok(Some(img));
        }
        inspect_podman(ref_str)
    }
}

fn inspect_skopeo(ref_str: &str) -> Result<Option<RemoteImage>, PodboxError> {
    let output = Command::new("skopeo")
        .args(["inspect", "--raw", &format!("docker://{ref_str}")])
        .output();

    let Ok(output) = output else {
        // skopeo not installed; fall through to podman.
        return Ok(None);
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("not found") || stderr.contains("unknown") || stderr.contains("404") {
            return Ok(None);
        }
        return Err(PodboxError::BaseUnresolved {
            image: ref_str.into(),
        });
    }

    let manifest_json = String::from_utf8_lossy(&output.stdout).into_owned();
    let manifest: serde_json::Value =
        serde_json::from_str(&manifest_json).map_err(|_| PodboxError::BaseUnresolved {
            image: ref_str.into(),
        })?;

    // The config descriptor carries the config digest.
    let config_digest = manifest
        .get("config")
        .and_then(|c| c.get("digest"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
        .unwrap_or_default();

    // Fetch config JSON to read labels.
    let config_digest_owned = config_digest.clone();
    let labels = if !config_digest_owned.is_empty() {
        config_labels_skopeo(ref_str, &config_digest_owned)?
    } else {
        BTreeMap::new()
    };

    Ok(Some(RemoteImage {
        ref_str: ref_str.into(),
        manifest_json,
        config_digest,
        labels,
    }))
}

/// Fetch raw config JSON for a digest and extract labels.
fn config_labels_skopeo(
    ref_str: &str,
    digest: &str,
) -> Result<BTreeMap<String, String>, PodboxError> {
    // Split ref into repo:tag for the manifest endpoint; config is content-addressable.
    // skopeo can inspect the config directly via `docker://ref@digest`.
    let output = Command::new("skopeo")
        .args(["inspect", "--raw", &format!("docker://{ref_str}@{digest}")])
        .output();
    let Ok(output) = output else {
        return Ok(BTreeMap::new());
    };
    if !output.status.success() {
        return Ok(BTreeMap::new());
    }
    let config_json = String::from_utf8_lossy(&output.stdout);
    let config: serde_json::Value = serde_json::from_str(&config_json).unwrap_or_default();
    let mut labels = BTreeMap::new();
    if let Some(cfg) = config.get("config") {
        if let Some(lbls) = cfg.get("Labels").and_then(|l| l.as_object()) {
            for (k, v) in lbls {
                if let Some(v) = v.as_str() {
                    labels.insert(k.clone(), v.to_string());
                }
            }
        }
    }
    Ok(labels)
}

fn inspect_podman(ref_str: &str) -> Result<Option<RemoteImage>, PodboxError> {
    let output = Command::new("podman")
        .args([
            "manifest",
            "inspect",
            "--format",
            "{{json .}}",
            &format!("docker://{ref_str}"),
        ])
        .output();
    let Ok(output) = output else {
        return Err(PodboxError::BaseUnresolved {
            image: ref_str.into(),
        });
    };
    if !output.status.success() {
        return Ok(None);
    }
    let manifest_json = String::from_utf8_lossy(&output.stdout).into_owned();
    let manifest: serde_json::Value = serde_json::from_str(&manifest_json).unwrap_or_default();

    // podman manifest inspect wraps the manifest. Single-arch image: read
    // .config.digest; multi-arch: read .manifests[0].digest.
    let config_digest = manifest
        .get("manifests")
        .and_then(|m| m.get(0))
        .and_then(|m| m.get("digest"))
        .and_then(|d| d.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            // Single image: .config.digest
            manifest
                .get("config")
                .and_then(|c| c.get("digest"))
                .and_then(|d| d.as_str())
                .map(|s| s.to_string())
                .unwrap_or_default()
        });

    let mut labels = BTreeMap::new();
    if !config_digest.is_empty() {
        if let Some(cfg) = manifest.get("config").and_then(|c| c.get("config")) {
            if let Some(lbls) = cfg.get("Labels").and_then(|l| l.as_object()) {
                for (k, v) in lbls {
                    if let Some(v) = v.as_str() {
                        labels.insert(k.clone(), v.to_string());
                    }
                }
            }
        }
    }

    Ok(Some(RemoteImage {
        ref_str: ref_str.into(),
        manifest_json,
        config_digest,
        labels,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_name() {
        assert_eq!(SkopeoRegistry.name(), "skopeo");
    }
}
