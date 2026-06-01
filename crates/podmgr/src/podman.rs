use std::process::Command;

use crate::error::PodmgrError;

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
        return Err(PodmgrError::PodmanInspectFailed {
            name: name.into(),
            stderr: stderr.to_string(),
        }
        .into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_lowercase();
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
        return Err(PodmgrError::PodmanInspectFailed {
            name: tag.into(),
            stderr: stderr.to_string(),
        }
        .into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
