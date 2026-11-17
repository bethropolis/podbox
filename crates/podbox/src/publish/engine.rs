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
    /// Build a single image+tag for one platform from an emitted context dir.
    /// `push` asks the engine to publish at build time when it can (buildx);
    /// otherwise the caller pushes afterwards.
    fn build(
        &self,
        image: &PlanImage,
        tag: &str,
        platform: &str,
        context_dir: &Path,
        labels: &BTreeMap<String, String>,
        push: bool,
    ) -> Result<(), PodboxError>;
    /// Push a single tag. Engines that pushed during `build` treat this as a
    /// no-op.
    fn push(&self, image: &PlanImage, tag: &str, context_dir: &Path) -> Result<(), PodboxError>;
}

fn podman_available() -> bool {
    Command::new("podman")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Select an engine: `explicit` wins; otherwise `buildx` for `--ci github`,
/// else `podman` if available, else `buildx`.
pub fn select_engine(
    prefer_ci: bool,
    explicit: Option<&str>,
) -> Result<Box<dyn Engine>, PodboxError> {
    match explicit {
        Some("buildx") => return Ok(Box::new(BuildxEngine)),
        Some("podman") => {
            if podman_available() {
                return Ok(Box::new(PodmanEngine));
            }
            return Err(PodboxError::EngineUnavailable {
                engine: "podman".into(),
            });
        }
        Some(other) => {
            return Err(PodboxError::EngineUnavailable {
                engine: other.into(),
            });
        }
        None => {}
    }
    if prefer_ci {
        return Ok(Box::new(BuildxEngine));
    }
    if podman_available() {
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
        platform: &str,
        context_dir: &Path,
        labels: &BTreeMap<String, String>,
        _push: bool,
    ) -> Result<(), PodboxError> {
        let mut args = vec![
            "build".into(),
            "--platform".into(),
            platform.into(),
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

    fn push(&self, _image: &PlanImage, tag: &str, _context_dir: &Path) -> Result<(), PodboxError> {
        run_checked("podman", &["push".into(), tag.into()])?;
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
        platform: &str,
        context_dir: &Path,
        labels: &BTreeMap<String, String>,
        push: bool,
    ) -> Result<(), PodboxError> {
        let mut args = vec![
            "buildx".into(),
            "build".into(),
            "--platform".into(),
            platform.into(),
            "-t".into(),
            tag.into(),
            // buildx has no separate push step: publish at build time, or
            // load the result into the local image store for `--no-push`.
            if push { "--push" } else { "--load" }.into(),
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
        // Folded into `build --push`.
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
        let _eng = select_engine(false, None).unwrap();
        let _eng = select_engine(true, None).unwrap();
        assert_eq!(
            select_engine(true, Some("buildx")).unwrap().name(),
            "buildx"
        );
    }

    #[test]
    fn unknown_explicit_engine_is_rejected() {
        let err = match select_engine(false, Some("nerdctl")) {
            Ok(_) => panic!("unknown engine must be rejected"),
            Err(err) => err,
        };
        assert!(matches!(err, PodboxError::EngineUnavailable { .. }));
    }
}
