use crate::config::Config;

use super::super::values::{is_lowercase_image_name, is_valid_image_ref, validate_container_name};

pub fn validate_image(config: &Config, errors: &mut Vec<String>) {
    if config.image.base.trim().is_empty() {
        errors.push("image.base: must not be empty".into());
    } else if !is_valid_image_ref(&config.image.base) {
        errors.push(format!(
            "image.base: '{}' is not a valid image reference (expected [registry/]name[:tag], no spaces or shell characters)",
            config.image.base
        ));
    }
    if config.image.name.trim().is_empty() {
        errors.push("image.name: must not be empty".into());
    } else if let Err(e) = validate_container_name("image", &config.image.name) {
        errors.push(format!("image.name: {e}"));
    } else if !is_lowercase_image_name(&config.image.name) {
        errors.push(format!(
            "image.name: '{}' must be lowercase (podman rejects uppercase image repository names)",
            config.image.name
        ));
    }
    if let Some(ref r) = config.image.image_ref {
        if r.trim().is_empty() {
            errors.push("image.image: must not be empty when set".into());
        } else if !is_valid_image_ref(r) {
            errors.push(format!(
                "image.image: '{r}' is not a valid image reference (expected [registry/]name[:tag][@sha256:<digest>], no spaces or shell characters)"
            ));
        }
    }
}

pub fn validate_packages(config: &Config, errors: &mut Vec<String>) {
    // Packages need a package manager: an undetectable image family
    // with install entries would otherwise silently emit the wrong
    // install command (dnf on Arch). Fail with the fix attached.
    if !config.image.packages.install.is_empty()
        && crate::codegen::distros::family_for_config(config)
            == crate::codegen::distros::DistroFamily::Unknown
    {
        let image = config
            .image
            .image_ref
            .as_deref()
            .unwrap_or(&config.image.base);
        errors.push(format!(
            "image.packages: cannot determine package manager for '{image}'; set image.packages.manager explicitly (\"apt\", \"dnf\", \"pacman\", \"apk\" or \"zypper\")"
        ));
    }
}
