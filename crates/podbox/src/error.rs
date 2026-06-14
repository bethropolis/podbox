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
}
