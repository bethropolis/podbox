//! Checkpoint the current image before rebuilding.

use anyhow::Result;

use podbox::config::Config;

pub(super) fn checkpoint_previous(config: &Config, name: &str, dry_run: bool) -> Result<()> {
    let local_latest = config.local_image_tag();
    let fallback =
        if config.image.source().is_prebuilt() && config.image.packages.install.is_empty() {
            config.image.image_ref.as_deref().unwrap_or(&local_latest)
        } else {
            &local_latest
        };
    let previous = format!("{}:checkpoint-prev", config.image_repo());
    if dry_run {
        println!("podman tag \"$(podman inspect --format '{{{{.Image}}}}' {name})\" {previous}");
        return Ok(());
    }
    let inspect = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["inspect", "--format", "{{.Image}}", name]),
    );
    let current = inspect
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|image| !image.is_empty())
        .or_else(|| {
            podbox::podman::image_exists(fallback)
                .ok()
                .filter(|exists| *exists)
                .map(|_| fallback.to_string())
        });
    let Some(current) = current else {
        return Ok(());
    };
    let output = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["tag", &current, &previous]),
    )?;
    if !output.status.success() {
        anyhow::bail!("failed to checkpoint current image '{current}'");
    }
    Ok(())
}
