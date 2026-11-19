//! Registry probe: read-only access to an OCI registry for base digest and
//! build-key label lookups.
//!
//! I/O shell: shells out to `skopeo inspect --raw` for the manifest and
//! `skopeo inspect --config` for the labels, with a `podman manifest inspect`
//! fallback. Behind a trait so tests can fake it.
//!
//! A tag ref resolves to an index on multi-arch images. The index has no
//! top-level `config` descriptor, so the digest falls back to `sha256` of the
//! manifest bytes just fetched — which is what the registry itself reports as
//! the manifest digest.

use std::collections::BTreeMap;
use std::time::Duration;

use sha2::{Digest, Sha256};

use crate::error::PodboxError;

/// Registry probes must not hang a publish run.
const PROBE_TIMEOUT: Duration = Duration::from_secs(60);

/// Opaque handle to a registry image manifest.
#[derive(Debug, Clone)]
pub struct RemoteImage {
    pub ref_str: String,
    /// Raw manifest JSON (as returned by the registry).
    pub manifest_json: String,
    /// Resolved image digest (`sha256:...`): the config digest for a single
    /// manifest, the manifest digest for an index.
    pub digest: String,
    /// Labels from the image config.
    pub labels: BTreeMap<String, String>,
}

/// Registry read operations.
pub trait Registry: Send + Sync {
    fn name(&self) -> &str;
    /// Inspect a remote image ref, returning its manifest and config labels.
    ///
    /// `ref_str` is a registry reference **without** the `docker://` scheme;
    /// each backend adds the transport prefix itself.
    ///
    /// Returns `None` when the image does not exist in the registry.
    fn inspect(&self, ref_str: &str) -> Result<Option<RemoteImage>, PodboxError>;
}

/// Real registry backend: `skopeo inspect` with a `podman` fallback.
pub struct SkopeoRegistry;

/// Outcome of one probe backend, so "image missing" and "tool missing" stay
/// distinguishable (the former falls back to a local image, the latter does
/// not).
enum Probe {
    Found(RemoteImage),
    Missing,
    ToolMissing,
}

impl Registry for SkopeoRegistry {
    fn name(&self) -> &'static str {
        "skopeo"
    }

    fn inspect(&self, ref_str: &str) -> Result<Option<RemoteImage>, PodboxError> {
        match inspect_skopeo(ref_str)? {
            Probe::Found(img) => Ok(Some(img)),
            skopeo => match (skopeo, inspect_podman(ref_str)?) {
                (_, Probe::Found(img)) => Ok(Some(img)),
                (Probe::ToolMissing, Probe::ToolMissing) => {
                    Err(PodboxError::RegistryProbeUnavailable)
                }
                _ => Ok(None),
            },
        }
    }
}

/// Registry refs are transport-less. Strip any `docker://` a caller supplied
/// so the constructed argv can never carry the scheme twice.
fn registry_ref(ref_str: &str) -> &str {
    let mut rest = ref_str;
    while let Some(stripped) = rest.strip_prefix("docker://") {
        rest = stripped;
    }
    rest
}

/// argv for `skopeo inspect --raw`: the manifest.
fn skopeo_inspect_argv(ref_str: &str) -> Vec<String> {
    vec![
        "inspect".to_owned(),
        "--raw".to_owned(),
        format!("docker://{}", registry_ref(ref_str)),
    ]
}

/// argv for `skopeo inspect --config`: the image config (labels).
fn skopeo_config_argv(ref_str: &str) -> Vec<String> {
    vec![
        "inspect".to_owned(),
        "--config".to_owned(),
        format!("docker://{}", registry_ref(ref_str)),
    ]
}

/// `sha256:<hex>` of the raw manifest bytes; the registry's manifest digest.
fn manifest_digest(raw: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw);
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

/// Base identity for a manifest: its config digest when it has one, else the
/// manifest digest. A multi-arch index has no top-level `config` descriptor,
/// so falling back keeps a base update invalidating the build key.
fn resolved_digest(manifest: &serde_json::Value, raw: &[u8]) -> String {
    manifest
        .get("config")
        .and_then(|c| c.get("digest"))
        .and_then(|d| d.as_str())
        .map_or_else(|| manifest_digest(raw), str::to_owned)
}

/// True when skopeo/podman stderr means "this image is not in the registry"
/// rather than "the probe broke".
fn is_missing_image(stderr: &str) -> bool {
    let stderr = stderr.to_ascii_lowercase();
    [
        "not found",
        "unknown",
        "404",
        "manifest unknown",
        "name unknown",
    ]
    .iter()
    .any(|needle| stderr.contains(needle))
}

fn inspect_skopeo(ref_str: &str) -> Result<Probe, PodboxError> {
    let ref_str = registry_ref(ref_str);
    if which::which("skopeo").is_err() {
        // skopeo not installed; let the caller try podman.
        return Ok(Probe::ToolMissing);
    }
    let args = crate::process::args(&skopeo_inspect_argv(ref_str));
    let output =
        crate::process::run_piped_timeout("skopeo", &args, PROBE_TIMEOUT).map_err(|e| {
            PodboxError::RegistryProbeFailed {
                reference: ref_str.into(),
                details: format!("skopeo inspect --raw failed: {e}"),
            }
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if is_missing_image(&stderr) {
            return Ok(Probe::Missing);
        }
        return Err(PodboxError::RegistryProbeFailed {
            reference: ref_str.into(),
            details: format!("skopeo inspect --raw: {}", stderr.trim()),
        });
    }

    let raw = output.stdout;
    let manifest: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|e| PodboxError::RegistryProbeFailed {
            reference: ref_str.into(),
            details: format!("could not parse the manifest returned by skopeo: {e}"),
        })?;

    let digest = resolved_digest(&manifest, &raw);

    let labels = match config_labels_skopeo(ref_str) {
        Ok(labels) => labels,
        Err(e) => {
            // Non-fatal: probing still produced a usable base digest.
            tracing::debug!("could not read image labels for '{ref_str}': {e}");
            BTreeMap::new()
        }
    };

    Ok(Probe::Found(RemoteImage {
        ref_str: ref_str.into(),
        manifest_json: String::from_utf8_lossy(&raw).into_owned(),
        digest,
        labels,
    }))
}

/// Fetch the image config through skopeo and extract its labels.
///
/// `skopeo inspect --config docker://<ref>` accepts both tag and digest refs,
/// unlike appending `@<digest>` to an existing reference (which produces an
/// invalid `repo:tag@digest` or `repo@digest@digest` reference).
fn config_labels_skopeo(ref_str: &str) -> Result<BTreeMap<String, String>, PodboxError> {
    let args = crate::process::args(&skopeo_config_argv(ref_str));
    let output =
        crate::process::run_piped_timeout("skopeo", &args, PROBE_TIMEOUT).map_err(|e| {
            PodboxError::RegistryProbeFailed {
                reference: ref_str.into(),
                details: format!("skopeo inspect --config could not run: {e}"),
            }
        })?;
    if !output.status.success() {
        return Err(PodboxError::RegistryProbeFailed {
            reference: ref_str.into(),
            details: format!(
                "skopeo inspect --config: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    let config: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| PodboxError::RegistryProbeFailed {
            reference: ref_str.into(),
            details: format!("could not parse the image config returned by skopeo: {e}"),
        })?;
    Ok(parse_labels(&config))
}

/// Extract the string-valued entries of `.config.Labels`. Non-string values
/// are ignored: OCI labels are strings, and a malformed one must not fail the
/// probe.
fn parse_labels(config: &serde_json::Value) -> BTreeMap<String, String> {
    let mut labels = BTreeMap::new();
    let Some(lbls) = config
        .get("config")
        .and_then(|c| c.get("Labels"))
        .and_then(|l| l.as_object())
    else {
        return labels;
    };
    for (k, v) in lbls {
        if let Some(v) = v.as_str() {
            labels.insert(k.clone(), v.to_owned());
        }
    }
    labels
}

fn inspect_podman(ref_str: &str) -> Result<Probe, PodboxError> {
    let ref_str = registry_ref(ref_str);
    if which::which("podman").is_err() {
        return Ok(Probe::ToolMissing);
    }
    let args = crate::process::args(&[
        "manifest",
        "inspect",
        "--format",
        "{{json .}}",
        &format!("docker://{ref_str}"),
    ]);
    let output =
        crate::process::run_piped_timeout("podman", &args, PROBE_TIMEOUT).map_err(|e| {
            PodboxError::RegistryProbeFailed {
                reference: ref_str.into(),
                details: format!("podman manifest inspect failed: {e}"),
            }
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.trim().is_empty() || is_missing_image(&stderr) {
            return Ok(Probe::Missing);
        }
        return Err(PodboxError::RegistryProbeFailed {
            reference: ref_str.into(),
            details: format!("podman manifest inspect: {}", stderr.trim()),
        });
    }

    let raw = output.stdout;
    let manifest: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|e| PodboxError::RegistryProbeFailed {
            reference: ref_str.into(),
            details: format!("could not parse the manifest returned by podman: {e}"),
        })?;

    // Single-arch image: read .config.digest; multi-arch: read
    // .manifests[0].digest; fall back to the manifest bytes.
    let digest = manifest
        .get("manifests")
        .and_then(|m| m.get(0))
        .and_then(|m| m.get("digest"))
        .and_then(|d| d.as_str())
        .or_else(|| {
            manifest
                .get("config")
                .and_then(|c| c.get("digest"))
                .and_then(|d| d.as_str())
        })
        .map_or_else(|| manifest_digest(&raw), str::to_owned);

    Ok(Probe::Found(RemoteImage {
        ref_str: ref_str.into(),
        manifest_json: String::from_utf8_lossy(&raw).into_owned(),
        digest,
        labels: parse_labels(&manifest),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn registry_name() {
        assert_eq!(SkopeoRegistry.name(), "skopeo");
    }

    #[test]
    fn inspect_argv_uses_raw_and_one_scheme() {
        assert_eq!(
            skopeo_inspect_argv("alpine:latest"),
            ["inspect", "--raw", "docker://alpine:latest"]
        );
    }

    #[test]
    fn config_argv_uses_config_and_one_scheme() {
        assert_eq!(
            skopeo_config_argv("alpine@sha256:c2661275"),
            ["inspect", "--config", "docker://alpine@sha256:c2661275"]
        );
    }

    #[test]
    fn caller_supplied_scheme_is_not_duplicated() {
        for ref_str in [
            "alpine:latest",
            "ghcr.io/org/base:2026.1",
            "ghcr.io:443/o/r:t",
            "alpine@sha256:c2661275",
            "ghcr.io/o/r:latest@sha256:c2661275",
        ] {
            for argv in [
                skopeo_inspect_argv(&format!("docker://{ref_str}")),
                skopeo_config_argv(&format!("docker://docker://{ref_str}")),
            ] {
                let reference = argv.last().expect("reference argument");
                assert_eq!(reference, &format!("docker://{ref_str}"));
                assert!(!reference.contains("docker://docker://"));
            }
        }
    }

    #[test]
    fn manifest_digest_is_sha256_of_the_raw_bytes() {
        // `sha256("")` and `sha256("abc")` are well-known vectors.
        assert_eq!(
            manifest_digest(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            manifest_digest(b"abc"),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn single_manifest_uses_its_config_digest() {
        let manifest = json!({
            "schemaVersion": 2,
            "config": {"digest": "sha256:b9284176"},
            "layers": []
        });
        assert_eq!(resolved_digest(&manifest, b"{}"), "sha256:b9284176");
    }

    #[test]
    fn index_uses_the_manifest_digest() {
        let raw = br#"{"schemaVersion":2,"manifests":[]}"#;
        let manifest: serde_json::Value = serde_json::from_slice(raw).expect("valid index");
        let digest = resolved_digest(&manifest, raw);
        assert_eq!(digest, manifest_digest(raw));
        assert!(digest.starts_with("sha256:"));
    }

    #[test]
    fn labels_ignore_non_string_values() {
        let config = json!({
            "config": {
                "Labels": {
                    "podbox.build-key": "abc123",
                    "org.opencontainers.image.title": "podbox",
                    "bad.number": 7,
                    "bad.null": null
                }
            }
        });
        let labels = parse_labels(&config);
        assert_eq!(
            labels.get("podbox.build-key").map(String::as_str),
            Some("abc123")
        );
        assert_eq!(
            labels
                .get("org.opencontainers.image.title")
                .map(String::as_str),
            Some("podbox")
        );
        assert_eq!(labels.len(), 2);
    }

    #[test]
    fn labels_absent_is_empty() {
        assert!(parse_labels(&json!({"config": {}})).is_empty());
        assert!(parse_labels(&json!({})).is_empty());
    }

    #[test]
    fn missing_image_phrasings_are_recognised() {
        for stderr in [
            "Error: manifest unknown",
            "Error: reading manifest latest in docker.io/library/nope: not found",
            "requested access to the resource is denied: 404",
        ] {
            assert!(is_missing_image(stderr), "should be missing: {stderr}");
        }
        assert!(!is_missing_image("invalid reference format"));
    }
}
