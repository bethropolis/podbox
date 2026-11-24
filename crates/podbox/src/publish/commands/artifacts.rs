use std::path::PathBuf;

use anyhow::Result;

use crate::config::Config;
use crate::error::PodboxError;
use crate::publish::build_context;

/// A build context emitted to a temp dir.
#[derive(Debug)]
pub(super) struct BuildContext {
    /// Owns the directory: it must outlive the build, otherwise the
    /// Containerfile is deleted before the engine reads it.
    pub(super) dir: tempfile::TempDir,
    /// Absolute path to the generated build file (the `--file` argument).
    pub(super) containerfile: PathBuf,
}

/// Emit the build context (Containerfile + guest binary) to a temp dir.
pub(super) fn emit_context(containerfile: &str, name: &str) -> Result<BuildContext, PodboxError> {
    // Defense in depth: names were already validated during resolution.
    build_context::validate_name(name)?;
    let dir = tempfile::Builder::new()
        .prefix("podbox-publish-")
        .tempdir()
        .map_err(publish_build_failed)?;
    let path = dir.path().join(name);
    std::fs::write(&path, containerfile).map_err(publish_build_failed)?;
    if let Some(guest) = crate::guest::PODBOX_GUEST {
        std::fs::write(dir.path().join("podbox-guest"), guest).map_err(publish_build_failed)?;
    }
    Ok(BuildContext {
        dir,
        containerfile: path,
    })
}

fn publish_build_failed(error: std::io::Error) -> PodboxError {
    PodboxError::PublishBuildFailed {
        key: "unknown".into(),
        details: error.to_string(),
    }
}

/// Generate the Containerfile for a definition.
pub(super) fn generate_containerfile(config: &Config) -> Result<String, PodboxError> {
    let env = crate::env::HostEnv::mock_preview();
    crate::codegen::containerfile::generate(config, "podbox-guest", &env)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitted_context_survives_until_the_builder_is_done() {
        let ctx = emit_context("FROM alpine:latest\n", "Containerfile").unwrap();
        assert!(ctx.dir.path().join("Containerfile").is_file());
        assert!(ctx.dir.path().join("podbox-guest").is_file());
        assert_eq!(ctx.containerfile, ctx.dir.path().join("Containerfile"));
    }

    #[test]
    fn emitted_context_uses_the_configured_name() {
        let ctx = emit_context("FROM alpine:latest\n", "Dockerfile.custom").unwrap();
        assert!(ctx.containerfile.is_file());
        assert!(ctx.containerfile.ends_with("Dockerfile.custom"));
        assert!(!ctx.dir.path().join("Containerfile").exists());
        assert!(ctx.dir.path().join("podbox-guest").is_file());
    }

    #[test]
    fn invalid_containerfile_name_fails_before_io() {
        let err = emit_context("FROM x\n", "../escape").unwrap_err();
        assert!(matches!(err, PodboxError::ContainerfileNameInvalid { .. }));
    }
}
