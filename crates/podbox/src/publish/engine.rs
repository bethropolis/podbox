//! Build engine: podman or buildx.
//!
//! I/O shell: spawns `podman build`/`podman push` or `docker buildx build`.
//! Behind a trait so tests can fake it.

use std::collections::BTreeMap;
use std::ffi::OsString;
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
    #[allow(clippy::too_many_arguments)] // one full build job, threaded through
    fn build(
        &self,
        image: &PlanImage,
        tags: &[String],
        platform: &str,
        context_dir: &Path,
        containerfile: &Path,
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

    #[allow(clippy::too_many_arguments)] // mirrors the trait's build signature
    fn build(
        &self,
        _image: &PlanImage,
        tags: &[String],
        platform: &str,
        context_dir: &Path,
        containerfile: &Path,
        labels: &BTreeMap<String, String>,
        _push: bool,
    ) -> Result<(), PodboxError> {
        let args = podman_build_args(tags, platform, context_dir, containerfile, labels);
        run_checked("podman", &args, Some(containerfile))?;
        Ok(())
    }

    fn push(&self, _image: &PlanImage, tag: &str, _context_dir: &Path) -> Result<(), PodboxError> {
        run_checked_push(
            "podman",
            tag,
            &[OsString::from("push"), OsString::from(tag)],
        )
    }
}

/// buildx engine: `docker buildx build ... --push` or `--load`.
pub struct BuildxEngine;

impl Engine for BuildxEngine {
    fn name(&self) -> &'static str {
        "buildx"
    }

    #[allow(clippy::too_many_arguments)] // mirrors the trait's build signature
    fn build(
        &self,
        _image: &PlanImage,
        tags: &[String],
        platform: &str,
        context_dir: &Path,
        containerfile: &Path,
        labels: &BTreeMap<String, String>,
        push: bool,
    ) -> Result<(), PodboxError> {
        let args = buildx_build_args(tags, platform, context_dir, containerfile, labels, push);
        run_checked("docker", &args, Some(containerfile))?;
        Ok(())
    }

    fn push(&self, _image: &PlanImage, _tag: &str, _context_dir: &Path) -> Result<(), PodboxError> {
        // Folded into `build --push`.
        Ok(())
    }
}

/// `podman build` argv. `--file` is explicit: no engine default is relied on.
fn podman_build_args(
    tags: &[String],
    platform: &str,
    context_dir: &Path,
    containerfile: &Path,
    labels: &BTreeMap<String, String>,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "build".into(),
        "--file".into(),
        containerfile.as_os_str().to_owned(),
        "--platform".into(),
        platform.into(),
    ];
    push_tags(&mut args, tags);
    push_labels(&mut args, labels);
    args.push(context_dir.as_os_str().to_owned());
    args
}

/// `docker buildx build` argv. Folds the push step into the build.
fn buildx_build_args(
    tags: &[String],
    platform: &str,
    context_dir: &Path,
    containerfile: &Path,
    labels: &BTreeMap<String, String>,
    push: bool,
) -> Vec<OsString> {
    let mut args: Vec<OsString> = vec![
        "buildx".into(),
        "build".into(),
        "--file".into(),
        containerfile.as_os_str().to_owned(),
        "--platform".into(),
        platform.into(),
    ];
    push_tags(&mut args, tags);
    // buildx has no separate push step: publish at build time, or load the
    // result into the local image store for `--no-push`.
    args.push(if push { "--push" } else { "--load" }.into());
    push_labels(&mut args, labels);
    args.push(context_dir.as_os_str().to_owned());
    args
}

/// One `-t` per planned tag, so a single build produces every tag.
fn push_tags(args: &mut Vec<OsString>, tags: &[String]) {
    for tag in tags {
        args.push("-t".into());
        args.push(tag.into());
    }
}

fn push_labels(args: &mut Vec<OsString>, labels: &BTreeMap<String, String>) {
    for (k, v) in labels {
        args.push("--label".into());
        args.push(format!("{k}={v}").into());
    }
}

fn run_checked_push(bin: &str, tag: &str, args: &[OsString]) -> Result<(), PodboxError> {
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
        Err(PodboxError::PublishPushFailed {
            key: tag.into(),
            details: format!("{bin} push exited {code}"),
        })
    }
}

fn run_checked(bin: &str, args: &[OsString], file: Option<&Path>) -> Result<(), PodboxError> {
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
        let details = match file {
            Some(path) => format!("{bin} exited {code} (--file {})", path.display()),
            None => format!("{bin} exited {code}"),
        };
        Err(PodboxError::PublishBuildFailed {
            key: "unknown".into(),
            details,
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

    fn rendered(args: &[OsString]) -> Vec<String> {
        args.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect()
    }

    fn labels() -> BTreeMap<String, String> {
        BTreeMap::from([("podbox.build-key".to_string(), "k".to_string())])
    }

    #[test]
    fn podman_build_passes_an_explicit_file_before_platform() {
        let tags = vec!["img:latest".to_string(), "img:1".to_string()];
        let args = rendered(&podman_build_args(
            &tags,
            "linux/amd64",
            Path::new("/ctx"),
            Path::new("/ctx/Dockerfile"),
            &labels(),
        ));
        assert_eq!(&args[..3], &["build", "--file", "/ctx/Dockerfile"]);
        let file = args.iter().position(|a| a == "--file").unwrap();
        let platform = args.iter().position(|a| a == "--platform").unwrap();
        assert!(file < platform, "--file must precede --platform");
        assert_eq!(args.iter().filter(|a| *a == "--file").count(), 1);
        assert_eq!(args.iter().filter(|a| *a == "-t").count(), tags.len());
        assert_eq!(
            args.iter().filter(|a| *a == "--label").count(),
            labels().len()
        );
        assert_eq!(args.last().unwrap(), "/ctx");
    }

    #[test]
    fn buildx_build_passes_an_explicit_file_before_platform() {
        for push in [false, true] {
            let tags = vec!["img:latest".to_string(), "img:1".to_string()];
            let args = rendered(&buildx_build_args(
                &tags,
                "linux/amd64",
                Path::new("/ctx"),
                Path::new("/ctx/Containerfile"),
                &labels(),
                push,
            ));
            assert_eq!(&args[..3], &["buildx", "build", "--file"]);
            assert_eq!(args[3], "/ctx/Containerfile");
            let file = args.iter().position(|a| a == "--file").unwrap();
            let platform = args.iter().position(|a| a == "--platform").unwrap();
            assert!(file < platform, "--file must precede --platform");
            assert_eq!(args.iter().filter(|a| *a == "--file").count(), 1);
            assert_eq!(args.iter().filter(|a| *a == "-t").count(), tags.len());
            assert!(args.contains(&if push { "--push" } else { "--load" }.to_string()));
            assert_eq!(args.last().unwrap(), "/ctx");
        }
    }
}
