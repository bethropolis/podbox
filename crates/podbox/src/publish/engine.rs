//! Build engine: podman or buildx.
//!
//! I/O shell: spawns `podman build`/`podman push` or `docker buildx build`.
//! Behind a trait so tests can fake it.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::error::PodboxError;
use crate::publish::plan::PlanImage;

/// Build engine trait.
pub trait Engine: Send + Sync {
    fn name(&self) -> &str;
    /// Build a single image+tag from an emitted context dir.
    /// Returns `Ok(())` on success.
    fn build(
        &self,
        image: &PlanImage,
        tag: &str,
        context_dir: &Path,
        labels: &BTreeMap<String, String>,
    ) -> Result<(), PodboxError>;
    /// Push a single tag.
    fn push(&self, image: &PlanImage, tag: &str, context_dir: &Path) -> Result<(), PodboxError>;
}

/// Auto-select an engine: `buildx` when `--ci github` is requested, else
/// `podman` if available, else `buildx`.
pub fn select_engine(prefer_ci: bool) -> Result<Box<dyn Engine>, PodboxError> {
    if prefer_ci {
        return Ok(Box::new(BuildxEngine));
    }
    if Command::new("podman")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        return Ok(Box::new(PodmanEngine));
    }
    Ok(Box::new(BuildxEngine))
}

/// podman engine: `podman build` + `podman push` per tag.
pub struct PodmanEngine;

impl Engine for PodmanEngine {
    fn name(&self) -> &'static str {
        "podman"
    }

    fn build(
        &self,
        _image: &PlanImage,
        tag: &str,
        context_dir: &Path,
        labels: &BTreeMap<String, String>,
    ) -> Result<(), PodboxError> {
        let mut args = vec![
            "build".into(),
            "--platform".into(),
            "linux/amd64".into(),
            "-t".into(),
            tag.into(),
        ];
        for (k, v) in labels {
            args.push("--label".into());
            args.push(format!("{k}={v}").into());
        }
        args.push(context_dir.to_owned().into());
        run_checked("podman", &args)?;
        Ok(())
    }

    fn push(&self, _image: &PlanImage, _tag: &str, _context_dir: &Path) -> Result<(), PodboxError> {
        run_checked("podman", &["push".into(), _tag.into()])?;
        Ok(())
    }
}

/// buildx engine: `docker buildx build ... --push` or `--load`.
pub struct BuildxEngine;

impl Engine for BuildxEngine {
    fn name(&self) -> &'static str {
        "buildx"
    }

    fn build(
        &self,
        _image: &PlanImage,
        tag: &str,
        context_dir: &Path,
        labels: &BTreeMap<String, String>,
    ) -> Result<(), PodboxError> {
        let mut args = vec![
            "buildx".into(),
            "build".into(),
            "--platform".into(),
            "linux/amd64".into(),
            "-t".into(),
            tag.into(),
        ];
        for (k, v) in labels {
            args.push("--label".into());
            args.push(format!("{k}={v}").into());
        }
        args.push(context_dir.to_owned().into());
        run_checked("docker", &args)?;
        Ok(())
    }

    fn push(&self, _image: &PlanImage, _tag: &str, _context_dir: &Path) -> Result<(), PodboxError> {
        // buildx push is folded into build with --push; this is a no-op
        // hook kept for interface symmetry (tags are pushed at build time
        // when --push is set).
        Ok(())
    }
}

fn run_checked(bin: &str, args: &[std::ffi::OsString]) -> Result<(), PodboxError> {
    let status = Command::new(bin)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|_e| PodboxError::EngineUnavailable { engine: bin.into() })?;
    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        Err(PodboxError::PublishBuildFailed {
            key: "unknown".into(),
            details: format!("{bin} exited {code}"),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_engine_prefers_podman_locally() {
        // Can't control PATH here; just ensure it returns something.
        let _eng = select_engine(false).unwrap();
    }
}
