use std::process::Command;
use std::sync::Mutex;

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

static PODMAN_VERSION: Mutex<Option<PodmanVersion>> = Mutex::new(None);

fn parse_version_string(s: &str) -> PodmanVersion {
    let s = s.trim();
    // Strip anything after a space (e.g. "5.3.0 (ok)" -> "5.3.0")
    let s = s.split_whitespace().next().unwrap_or(s);
    // Strip Debian-style epoch prefix (e.g. "100:5.3.0" -> "5.3.0")
    let s = s.rsplit_once(':').map(|(_, after)| after).unwrap_or(s);
    let mut parts = s.splitn(3, '.');
    PodmanVersion {
        major: parts.next().and_then(|p| p.parse().ok()).unwrap_or(0),
        minor: parts.next().and_then(|p| p.parse().ok()).unwrap_or(0),
        patch: parts.next().and_then(|p| p.parse().ok()).unwrap_or(0),
    }
}

pub fn podman_version() -> anyhow::Result<PodmanVersion> {
    let mut cache = PODMAN_VERSION.lock().unwrap();
    if let Some(ref ver) = *cache {
        return Ok(ver.clone());
    }

    // Prefer structured output from `podman version -f` (more reliable
    // across distro packaging, e.g. Debian epoch suffixes).
    let structured = Command::new("podman")
        .args(["version", "-f", "{{.Client.Version}}"])
        .output()
        .ok()
        .filter(|o| o.status.success());
    let version_str = match structured {
        Some(ref output) => String::from_utf8_lossy(&output.stdout).trim().to_string(),
        None => {
            // Fallback: parse `podman --version` (e.g. "podman version 5.3.0")
            let output = Command::new("podman").args(["--version"]).output()?;
            let stdout = String::from_utf8_lossy(&output.stdout);
            stdout.split_whitespace().last().unwrap_or("").to_string()
        }
    };
    let ver = parse_version_string(&version_str);
    *cache = Some(ver.clone());
    Ok(ver)
}

#[cfg(test)]
#[allow(dead_code)]
pub(crate) fn set_test_version(ver: PodmanVersion) {
    let mut cache = PODMAN_VERSION.lock().unwrap();
    *cache = Some(ver);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_version() {
        let v = parse_version_string("5.3.0");
        assert_eq!(
            v,
            PodmanVersion {
                major: 5,
                minor: 3,
                patch: 0
            }
        );
    }

    #[test]
    fn parses_version_with_epoch() {
        let v = parse_version_string("100:5.3.0");
        assert_eq!(
            v,
            PodmanVersion {
                major: 5,
                minor: 3,
                patch: 0
            }
        );
    }

    #[test]
    fn parses_version_with_parenthetical() {
        let v = parse_version_string("5.3.0 (ok)");
        assert_eq!(
            v,
            PodmanVersion {
                major: 5,
                minor: 3,
                patch: 0
            }
        );
    }

    #[test]
    fn parses_version_with_leading_trailing_whitespace() {
        let v = parse_version_string("  5.3.0  ");
        assert_eq!(
            v,
            PodmanVersion {
                major: 5,
                minor: 3,
                patch: 0
            }
        );
    }

    #[test]
    fn handles_single_component() {
        let v = parse_version_string("5");
        assert_eq!(
            v,
            PodmanVersion {
                major: 5,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn handles_two_components() {
        let v = parse_version_string("5.3");
        assert_eq!(
            v,
            PodmanVersion {
                major: 5,
                minor: 3,
                patch: 0
            }
        );
    }

    #[test]
    fn handles_empty_string() {
        let v = parse_version_string("");
        assert_eq!(
            v,
            PodmanVersion {
                major: 0,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn handles_non_numeric() {
        let v = parse_version_string("abc");
        assert_eq!(
            v,
            PodmanVersion {
                major: 0,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn handles_partial_numeric() {
        let v = parse_version_string("5.abc.0");
        assert_eq!(
            v,
            PodmanVersion {
                major: 5,
                minor: 0,
                patch: 0
            }
        );
    }

    #[test]
    fn version_at_least_works() {
        let v = PodmanVersion {
            major: 5,
            minor: 6,
            patch: 0,
        };
        assert!(v.at_least(5, 6));
        assert!(v.at_least(4, 0));
        assert!(v.at_least(5, 5));
        assert!(!v.at_least(5, 7));
        assert!(!v.at_least(6, 0));
    }

    #[test]
    fn parse_size_bytes_handles_podman_units() {
        assert_eq!(super::parse_size_bytes("12B"), Some(12));
        assert_eq!(super::parse_size_bytes("1.5KiB"), Some(1536));
        assert_eq!(super::parse_size_bytes("12.3MiB"), Some(12_897_484));
        assert_eq!(super::parse_size_bytes("2GiB"), Some(2_147_483_648));
        assert_eq!(super::parse_size_bytes("16.61GB"), Some(16_610_000_000));
        assert_eq!(super::parse_size_bytes("65.54kB"), Some(65540));
        assert_eq!(super::parse_size_bytes("512"), Some(512));
        assert_eq!(super::parse_size_bytes("bogus"), None);
        assert_eq!(super::parse_size_bytes("10XB"), None);
    }

    #[test]
    fn split_mem_usage_splits_used_and_limit() {
        assert_eq!(
            super::split_mem_usage("12.3MiB / 2GiB"),
            Some((12_897_484, 2_147_483_648))
        );
        assert_eq!(super::split_mem_usage("n/a"), None);
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ContainerState {
    Running,
    Stopped,
    Missing,
}

/// Container ids seen by the most recent [`query_state`] call, keyed by name.
///
/// `query_state` already runs `podman inspect`, so widening its format to
/// also return `.Id` costs nothing while giving callers a cheap way to tell
/// "same container" from "recreated since last time" without another
/// round-trip. Only the runtime's git bridge reads this.
static SEEN_CONTAINER_IDS: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, String>>,
> = std::sync::OnceLock::new();

fn seen_container_ids() -> &'static std::sync::Mutex<std::collections::HashMap<String, String>> {
    SEEN_CONTAINER_IDS.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Container id recorded by the last successful [`query_state`] for `name`.
///
/// `None` when `query_state` has not run for this container in this process
/// (or podman could not be queried), so callers must treat it as "unknown"
/// rather than "unchanged".
pub fn seen_container_id(name: &str) -> Option<String> {
    seen_container_ids().lock().ok()?.get(name).cloned()
}

fn record_container_id(name: &str, id: &str) {
    if let Ok(mut map) = seen_container_ids().lock() {
        map.insert(name.to_string(), id.to_string());
    }
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
        .args([
            "inspect",
            "--type",
            "image",
            "--format",
            "{{json .Labels}}",
            tag,
        ])
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
///
/// Checks `podman inspect` first.  If the container is unknown to podman,
/// falls back to `systemctl --user is-active` (quadlet-managed containers)
/// and returns `Stopped` when the unit exists but is inactive.
pub fn query_state(name: &str) -> anyhow::Result<ContainerState> {
    let output = Command::new("podman")
        .args([
            "inspect",
            "--type",
            "container",
            "--format",
            "{{.State.Status}} {{.Id}}",
            name,
        ])
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if stderr.contains("no such container") || stderr.contains("no such object") {
            // Fallback: check systemd for Quadlet-managed containers
            if let Ok(sys) = Command::new("systemctl")
                .args(["--user", "is-active", &format!("{name}.service")])
                .output()
            {
                let state = String::from_utf8_lossy(&sys.stdout).trim().to_string();
                match state.as_str() {
                    "active" => {
                        // Systemd says active, but verify the podman container
                        // actually exists. Quadlet's OnStop=remove can delete
                        // the container while the unit is restarting. If the
                        // container is truly gone, return Stopped so that
                        // ensure_running() tries to start it.
                        if let Ok(out) = Command::new("podman")
                            .args(["ps", "-q", "--filter", &format!("name=^{name}$")])
                            .output()
                        {
                            if out.status.success()
                                && !String::from_utf8_lossy(&out.stdout).trim().is_empty()
                            {
                                return Ok(ContainerState::Running);
                            }
                        }
                        return Ok(ContainerState::Stopped);
                    }
                    "inactive" | "failed" => return Ok(ContainerState::Stopped),
                    _ => {
                        // "unknown" — unit not loaded.  Check whether quadlet
                        // files exist; if so the container was built but is
                        // stopped (OnStop=remove may have removed the podman
                        // object while leaving the quadlet infrastructure).
                        if crate::quadlet_install::is_installed(name) {
                            return Ok(ContainerState::Stopped);
                        }
                        return Ok(ContainerState::Missing);
                    }
                };
            }
            // systemctl not available — check quadlet files as last resort
            if crate::quadlet_install::is_installed(name) {
                return Ok(ContainerState::Stopped);
            }
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
    // `{{.State.Status}} {{.Id}}` — keep the id for `seen_container_id`.
    let mut fields = stdout.split_whitespace();
    let status = fields.next().unwrap_or_default().to_string();
    if let Some(id) = fields.next() {
        if !id.is_empty() && id != "unknown" {
            record_container_id(name, id);
        }
    }
    match status.as_str() {
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

/// Last-start timestamp for a container, as reported by podman.
///
/// `None` when the container is unknown or was never started (podman
/// reports the zero time). Best-effort: every failure maps to `None` so
/// `status --output json` simply omits the field.
pub fn started_at(name: &str) -> Option<String> {
    let args = crate::process::args(&[
        "inspect",
        "--type",
        "container",
        "--format",
        "{{json .State.StartedAt}}",
        name,
    ]);
    let output =
        crate::process::run_piped_timeout("podman", &args, std::time::Duration::from_secs(10))
            .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout)
        .trim()
        .trim_matches('"')
        .to_string();
    if text.is_empty() || text.starts_with("0001-01-01") {
        return None;
    }
    Some(text)
}

/// Point-in-time resource sample for a running container.
#[derive(Debug, PartialEq)]
pub struct ContainerStats {
    pub cpu_percent: f64,
    pub memory_bytes: u64,
    pub memory_limit_bytes: u64,
    pub pids: u32,
}

/// One `podman stats --no-stream` sample.
///
/// `None` when the container is not running or stats cannot be parsed.
/// Best-effort by design: `status --output json` omits what it cannot read.
pub fn stats_sample(name: &str) -> Option<ContainerStats> {
    let args = crate::process::args(&["stats", "--no-stream", "--format", "json", name]);
    let output =
        crate::process::run_piped_timeout("podman", &args, std::time::Duration::from_secs(10))
            .ok()?;
    if !output.status.success() {
        return None;
    }
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).ok()?;
    let row = rows.into_iter().next()?;
    let cpu = row
        .get("CPU")
        .and_then(|v| v.as_str())
        .and_then(|s| s.trim().trim_end_matches('%').parse::<f64>().ok())?;
    let (used, limit) = row
        .get("MemUsage")
        .and_then(|v| v.as_str())
        .and_then(split_mem_usage)?;
    let pids = row.get("PIDs").and_then(|v| {
        v.as_str()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .or_else(|| v.as_u64().and_then(|n| u32::try_from(n).ok()))
    })?;
    Some(ContainerStats {
        cpu_percent: cpu,
        memory_bytes: used,
        memory_limit_bytes: limit,
        pids,
    })
}

/// Split podman's `MemUsage` (`"12.3MiB / 2GiB"`) into used/limit bytes.
fn split_mem_usage(s: &str) -> Option<(u64, u64)> {
    let (used, limit) = s.split_once('/')?;
    Some((parse_size_bytes(used)?, parse_size_bytes(limit)?))
}

/// Parse a human size (`"12.3MiB"`, `"2GB"`, `"512"`) into bytes.
/// SI units are 1000-based, IEC (`iB`) 1024-based, matching podman output.
fn parse_size_bytes(s: &str) -> Option<u64> {
    let s = s.trim();
    let cut = s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len());
    let (num, unit) = s.split_at(cut);
    let num: f64 = num.trim().parse().ok()?;
    let mult = match unit.trim().to_uppercase().as_str() {
        "" | "B" => 1.0,
        "K" | "KB" => 1e3,
        "KIB" => 1024.0,
        "M" | "MB" => 1e6,
        "MIB" => 1024.0 * 1024.0,
        "G" | "GB" => 1e9,
        "GIB" => 1024.0 * 1024.0 * 1024.0,
        "T" | "TB" => 1e12,
        "TIB" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return None,
    };
    // Podman sizes are non-negative magnitudes; saturate the float fallbacks.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some((num * mult) as u64)
}
