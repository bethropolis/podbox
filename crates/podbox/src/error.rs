use std::io;
use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum PodboxError {
    #[error("container '{0}' not found -- run `podbox build` and `podbox enable` first")]
    ContainerMissing(String),

    /// An explicit `--config <PATH>` that does not exist. Never falls back to
    /// the embedded default: a typo must not act on some other box.
    #[error("config file not found: {}", path.display())]
    ConfigFileMissing { path: PathBuf },

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

    /// A name (container, image, profile, tag) that would be unsafe as a file
    /// name, unit name or podman argument.
    #[error(
        "invalid {kind} name '{name}': use 1-63 letters, digits, '.', '_' or '-', starting with a letter or digit"
    )]
    InvalidName { kind: &'static str, name: String },

    /// A child process exited non-zero. Its own diagnostics were already shown
    /// (stdio is inherited), so `main` exits with this code without adding a line.
    #[error("command exited with status {0}")]
    ChildExit(i32),

    /// The user declined a confirmation prompt.
    #[error("cancelled")]
    Cancelled,

    /// A destructive command needs confirmation but stdin is not a TTY and
    /// `--yes`/`--force` was not given.
    #[error("confirmation required: re-run with --yes (or --force)")]
    ConfirmationRequired,

    #[error("config validation failed:\n{details}")]
    ConfigValidationFailed { details: String },

    #[error("provision steps pending or stale")]
    StepsPending,

    #[error("{0} check(s) failed")]
    ChecksFailed(u32),

    #[error("configuration invalid: {0} error(s)")]
    ConfigInvalid(usize),

    #[error(
        "this build of podbox has no embedded guest binary and can only manage prebuilt images -- use a release binary or a source build with the podbox-guest workspace present to build custom images"
    )]
    GuestBinaryUnavailable,

    /// Publish: no manifest found.
    #[error("no images.toml, envs/*.toml or podbox.toml found in '{dir}'")]
    ManifestMissing { dir: String },

    /// Publish: manifest parse/validation error.
    #[error("manifest invalid: {details}")]
    ManifestInvalid { details: String },

    /// Publish: definition loading error.
    #[error("publish definition load failed for '{key}': {details}")]
    PublishDefinitionLoad { key: String, details: String },

    /// Publish: image key not found in manifest.
    #[error("image '{key}' not found in manifest")]
    ImageMissing { key: String },

    /// Publish: tags expand to nothing for this context.
    #[error("image '{key}' expands to no tags in this context ({context})")]
    NoTags { key: String, context: String },

    /// Publish: prebuilt definition with nothing to layer.
    #[error("image '{key}' is not buildable: prebuilt image with no packages or run commands")]
    NotBuildable { key: String },

    /// Publish: engine not available.
    #[error("publish engine '{engine}' not available")]
    EngineUnavailable { engine: String },

    /// Publish: base image digest could not be resolved.
    #[error("cannot resolve base image digest for '{image}'")]
    BaseUnresolved { image: String },

    /// Publish: build failed.
    #[error("publish build failed for '{key}': {details}")]
    PublishBuildFailed { key: String, details: String },

    /// Publish: registry rejected credentials.
    #[error("registry authentication failed for '{registry}'")]
    AuthFailed { registry: String },

    /// Publish: push failed for another reason.
    #[error("publish push failed for '{key}': {details}")]
    PublishPushFailed { key: String, details: String },

    /// Publish: missing context field.
    #[error("publish context missing field '{0}'")]
    MissingContextField(&'static str),

    /// Publish: missing environment variable.
    #[error("publish context missing environment variable '{0}'")]
    MissingEnv(&'static str),

    /// Publish: invalid environment variable.
    #[error("publish context invalid environment variable '{0}': {1}")]
    InvalidEnv(&'static str, &'static str),
}

impl PodboxError {
    /// Stable lowercase identifier for the client contract
    /// (`docs/clients.md`). Messages are for people; codes are for parsers.
    /// Only extended, never renamed.
    pub fn code(&self) -> &'static str {
        match self {
            Self::ContainerMissing(_) => "container_missing",
            Self::ConfigFileMissing { .. } => "config_missing",
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
            Self::ConfigInvalid(_) => "config_invalid",
            Self::ProtocolMismatch { .. } => "protocol_mismatch",
            Self::ChildExit(_) => "child_exit",
            Self::Cancelled => "cancelled",
            Self::ConfirmationRequired => "confirmation_required",
            Self::InvalidName { .. } => "invalid_name",
            Self::GuestBinaryUnavailable => "guest_binary_unavailable",
            Self::ManifestMissing { .. } => "manifest_missing",
            Self::ManifestInvalid { .. } => "manifest_invalid",
            Self::PublishDefinitionLoad { .. } => "publish_definition_load",
            Self::ImageMissing { .. } => "image_missing",
            Self::NoTags { .. } => "no_tags",
            Self::NotBuildable { .. } => "not_buildable",
            Self::EngineUnavailable { .. } => "engine_unavailable",
            Self::BaseUnresolved { .. } => "base_unresolved",
            Self::PublishBuildFailed { .. } => "build_failed",
            Self::AuthFailed { .. } => "auth_failed",
            Self::PublishPushFailed { .. } => "push_failed",
            Self::MissingContextField { .. } => "publish_context_missing",
            Self::MissingEnv { .. } => "publish_env_missing",
            Self::InvalidEnv { .. } => "publish_env_invalid",
        }
    }

    /// True when the failure's structured JSON report was already printed
    /// on stdout and the error only carries the exit status. `main` skips
    /// the error envelope for these so stdout stays a single JSON document.
    pub fn json_reported(&self) -> bool {
        matches!(
            self,
            Self::StepsPending | Self::ChecksFailed(_) | Self::ConfigInvalid(_)
        )
    }
}
