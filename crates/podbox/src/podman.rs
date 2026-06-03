use std::process::Command;
use std::sync::OnceLock;

use crate::error::PodboxError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PodmanVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl PodmanVersion {
    pub fn at_least(&self, major: u32, minor: u32) -> bool {
        (self.major, self.minor) >= (major, minor)
    }
}

static PODMAN_VERSION: OnceLock<anyhow::Result<PodmanVersion>> = OnceLock::new();

pub fn podman_version() -> anyhow::Result<&'static PodmanVersion> {
    let res = PODMAN_VERSION.get_or_init(|| {
        let output = std::process::Command::new("podman")
            .args(["--version"])
            .output()?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let version_str = stdout.split_whitespace().last().unwrap_or("");
        let parts: Vec<&str> = version_str.split('.').collect();
        Ok(PodmanVersion {
            major: parts.first().unwrap_or(&"0").parse().unwrap_or(0),
            minor: parts.get(1).unwrap_or(&"0").parse().unwrap_or(0),
            patch: parts.get(2).unwrap_or(&"0").parse().unwrap_or(0),
        })
    });
    res.as_ref().map_err(|e| anyhow::anyhow!("{}", e))
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn set_test_version(ver: PodmanVersion) {
    PODMAN_VERSION.set(Ok(ver)).ok();
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ContainerState {
    Running,
    Stopped,
    Missing,
}

/// Check whether a local image tag exists.
pub fn image_exists(tag: &str) -> anyhow::Result<bool> {
    let output = std::process::Command::new("podman")
        .args(["image", "exists", tag])
        .output()?;
    Ok(output.status.success())
}

/// Fetch OCI labels for a local image.
pub fn image_labels(tag: &str) -> anyhow::Result<std::collections::HashMap<String, String>> {
    let output = std::process::Command::new("podman")
        .args(["inspect", "--format", "{{json .Labels}}", tag])
        .output()?;
    if !output.status.success() {
        return Ok(std::collections::HashMap::new());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let map: std::collections::HashMap<String, String> =
        serde_json::from_str(stdout.trim()).unwrap_or_default();
    Ok(map)
}

/// Query the state of a container.
pub fn query_state(name: &str) -> anyhow::Result<ContainerState> {
    let output = Command::new("podman")
        .args(["inspect", "--format", "{{.State.Status}}", name])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no such container") || stderr.contains("no such object") {
            return Ok(ContainerState::Missing);
        }
        return Err(PodboxError::PodmanInspectFailed {
            name: name.into(),
            stderr: stderr.to_string(),
        }
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout)
        .trim()
        .to_lowercase();
    match stdout.as_str() {
        "running" => Ok(ContainerState::Running),
        "stopped" | "exited" => Ok(ContainerState::Stopped),
        _ => Ok(ContainerState::Stopped),
    }
}

/// Get the digest of a built image.
pub fn image_digest(tag: &str) -> anyhow::Result<String> {
    let output = Command::new("podman")
        .args(["inspect", "--format", "{{.Digest}}", tag])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PodboxError::PodmanInspectFailed {
            name: tag.into(),
            stderr: stderr.to_string(),
        }
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
