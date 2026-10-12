use std::io;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum PodboxError {
    #[error("container '{0}' not found -- run `podbox build` and `podbox enable` first")]
    ContainerMissing(String),

    #[error("definition file not found at {path}")]
    DefinitionNotFound { path: PathBuf },

    #[error("failed to read definition file: {0}")]
    DefinitionReadFailed(#[from] io::Error),

    #[error("failed to parse definition file: {0}")]
    DefinitionParseFailed(#[from] toml::de::Error),

    #[error("podman not found in PATH")]
    PodmanNotFound,

    #[error("home directory '{path}' could not be created: {source}")]
    HomeCreateFailed { path: PathBuf, source: io::Error },

    #[error("podman inspect failed for '{name}': {stderr}")]
    PodmanInspectFailed { name: String, stderr: String },

    #[error("build failed: {0}")]
    BuildFailed(String),

    #[error("export failed: {details}")]
    ExportFailed { details: String },

    #[error("xdg-user-dir not found in PATH -- install xdg-user-dirs")]
    XdgUserDirNotFound,

    #[error("failed to pull image '{image}'")]
    PullFailed { image: String },

    #[error("failed to tag image as '{image}'")]
    TagFailed { image: String },

    #[error("protocol version mismatch: host speaks v{expected}, guest speaks v{got}")]
    ProtocolMismatch { expected: u32, got: u32 },

    #[error("config validation failed:\n{details}")]
    ConfigValidationFailed { details: String },

    #[error("provision steps pending or stale")]
    StepsPending,

    #[error("{0} check(s) failed")]
    ChecksFailed(u32),

    #[error(
        "this build of podbox has no embedded guest binary and can only manage prebuilt images -- use a release binary or a source build with the podbox-guest workspace present to build custom images"
    )]
    GuestBinaryUnavailable,
}

impl PodboxError {
    /// Stable lowercase identifier for the client contract
    /// (`docs/clients.md`). Messages are for people; codes are for parsers.
    /// Only extended, never renamed.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ContainerMissing(_) => "container_missing",
            Self::DefinitionNotFound { .. } => "definition_missing",
            Self::DefinitionReadFailed(_) => "definition_unreadable",
            Self::DefinitionParseFailed(_) | Self::ConfigValidationFailed { .. } => {
                "definition_invalid"
            }
            Self::PodmanNotFound => "podman_missing",
            Self::HomeCreateFailed { .. } => "home_create_failed",
            Self::PodmanInspectFailed { .. } => "inspect_failed",
            Self::BuildFailed(_) => "build_failed",
            Self::ExportFailed { .. } => "export_failed",
            Self::XdgUserDirNotFound => "xdg_user_dir_missing",
            Self::PullFailed { .. } => "image_pull_failed",
            Self::TagFailed { .. } => "image_tag_failed",
            Self::StepsPending => "steps_pending",
            Self::ChecksFailed(_) => "checks_failed",
            Self::ProtocolMismatch { .. } => "protocol_mismatch",
            Self::GuestBinaryUnavailable => "guest_binary_unavailable",
        }
    }

    /// True when the failure's structured JSON report was already printed
    /// on stdout and the error only carries the exit status. `main` skips
    /// the error envelope for these so stdout stays a single JSON document.
    pub fn json_reported(&self) -> bool {
        matches!(self, Self::StepsPending | Self::ChecksFailed(_))
    }
}
