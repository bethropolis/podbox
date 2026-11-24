use serde::{Deserialize, Serialize};

use crate::config::defaults::{
    default_pull_retry, default_pull_retry_delay, is_default_packages, is_default_pull_retry,
    is_default_pull_retry_delay, is_default_run,
};
use crate::config::enums::{ImageSource, PackageManager};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ImageConfig {
    pub base: String,
    pub name: String,
    #[serde(rename = "image", default)]
    pub image_ref: Option<String>,
    #[serde(
        default = "default_pull_retry",
        skip_serializing_if = "is_default_pull_retry"
    )]
    pub pull_retry: u32,
    #[serde(
        default = "default_pull_retry_delay",
        skip_serializing_if = "is_default_pull_retry_delay"
    )]
    pub pull_retry_delay: String,
    #[serde(default, skip_serializing_if = "is_default_packages")]
    pub packages: PackageConfig,
    #[serde(default, skip_serializing_if = "is_default_run")]
    pub run: RunConfig,
}

impl ImageConfig {
    pub fn source(&self) -> ImageSource {
        match &self.image_ref {
            Some(ref_str) => ImageSource::Prebuilt {
                ref_str: ref_str.clone(),
            },
            None => ImageSource::Build {
                base: self.base.clone(),
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct PackageConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub install: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    /// Explicit override. `None` (omitted) means auto-detect from the image
    /// name at use time — see `resolve_prebuilt_distro` and diff's
    /// `resolve_manager`. This is why the field can stay out of the TOML.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manager: Option<PackageManager>,
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RunConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commands: Vec<String>,
}
