//! Prebuilt-image path: pull (or re-tag) a published image, optionally
//! layer packages on top plus the host's embedded guest binary, and record
//! the lock file.
//!
//! Extracted verbatim from `build.rs`.

use std::time::Instant;

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

use crate::cli::OutputFormat;
use crate::codegen::distros::DistroFamily;
use crate::config::Config;
use crate::env::build_context_dir;
use crate::error::PodboxError;
use crate::ui;

use super::{open_log, run_podman_logged};

pub(crate) fn run_prebuilt(
    config: &Config,
    dry_run: bool,
    rebuild: bool,
    output: OutputFormat,
) -> Result<()> {
    let events = matches!(output, OutputFormat::Ndjson);
    let image_ref = match config.image.source() {
        crate::config::ImageSource::Prebuilt { ref_str } => ref_str,
        _ => config.image.base.clone(),
    };
    let local_tag = config.local_image_tag();
    let context_dir = build_context_dir(&config.container.name);
    let lock_path = context_dir.join(".podbox.lock");
    let has_packages = !config.image.packages.install.is_empty();

    // Acquire exclusive build lock (auto-releases on panic/crash via kernel flock)
    let _build_lock = if !dry_run {
        std::fs::create_dir_all(&context_dir)?;
        let file = std::fs::File::create(context_dir.join(".build.lock"))?;
        Some(Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, e)| e)?)
    } else {
        None
    };

    // The image key covers the rendered overlay recipe (or the bare FROM
    // ref), the guest binary and the podbox version — everything that ends
    // up in the image. Quadlet-only toggles like `integration.audio` leave
    // it alone; a podbox upgrade busts it.
    let recipe = crate::codegen::containerfile::generate_prebuilt_overlay(
        config,
        &image_ref,
        crate::guest::PODBOX_GUEST.is_some(),
    )
    .unwrap_or_else(|| format!("FROM {image_ref}\n"));
    let image_key = super::image_cache_key(&recipe, crate::guest::PODBOX_GUEST, crate::VERSION);

    if !rebuild {
        let lock = crate::lock::read(&lock_path)?;
        if super::cache_hit(
            lock.as_ref(),
            &image_key,
            crate::podman::image_exists(&local_tag)?,
        ) {
            if events {
                crate::contract::emit_event(serde_json::json!({
                    "event": "done", "ok": true, "cached": true,
                }));
            } else {
                println!("Prebuilt image already present as {local_tag}. Skipping pull.");
                println!("Use --rebuild to re-pull.");
            }
            return Ok(());
        }
    }

    if dry_run {
        println!("Would pull: {image_ref}");
        if crate::codegen::containerfile::prebuilt_overlay_needed(config) {
            if has_packages {
                println!(
                    "Would install packages on top: {}",
                    config.image.packages.install.join(", ")
                );
            }
            if !config.image.run.commands.is_empty() {
                println!(
                    "Would run {} command(s) on top",
                    config.image.run.commands.len()
                );
            }
            if crate::guest::PODBOX_GUEST.is_some() {
                println!("Would layer host guest binary on top (version match)");
            }
        }
        println!("Would tag as: {local_tag}");
        println!("Would write lock file at: {}", lock_path.display());
        return Ok(());
    }

    // Warn on version mismatch from labels (best-effort, image may not exist yet)
    if let Ok(labels) = crate::podman::image_labels(&image_ref) {
        if let Some(guest_ver) = labels
            .get("podbox.guest_version")
            .or_else(|| labels.get("podmgr.guest_version"))
        {
            let guest_clean = guest_ver.trim_start_matches('v');
            let host_clean = crate::VERSION.trim_start_matches('v');
            if guest_clean != host_clean {
                eprintln!(
                    "Warning: image guest version (v{guest_clean}) differs from host (v{host_clean}). \
                     Protocol compatibility will be validated at runtime."
                );
            }
        }
    }

    if events {
        crate::contract::emit_event(serde_json::json!({
            "event": "step", "index": 1, "total": 2,
            "text": format!("Pulling {image_ref}"),
        }));
    } else {
        println!("Pulling {image_ref}...");
    }
    let status = std::process::Command::new("podman")
        .args(["pull", &image_ref])
        .status()?;
    if !status.success() {
        return Err(PodboxError::PullFailed {
            image: image_ref.clone(),
        }
        .into());
    }

    if let Some(overlay) = crate::codegen::containerfile::generate_prebuilt_overlay(
        config,
        &image_ref,
        crate::guest::PODBOX_GUEST.is_some(),
    ) {
        // Layer the config's packages and RUN steps on top of the prebuilt
        // image, plus the host's embedded guest binary so
        // `/run/podbox/guest-version` matches the host. The COPY is
        // content-hashed, so it only busts podman cache when the guest
        // actually changes; the heavy registry base layers stay cached.
        // Builds without an embedded guest (published-crate installs) skip
        // the layer and keep the registry-baked guest.
        std::fs::create_dir_all(&context_dir)
            .with_context(|| format!("failed to create context dir '{}'", context_dir.display()))?;

        let mut containerfile = overlay;
        if !containerfile.ends_with('\n') {
            containerfile.push('\n');
        }
        if let Some(guest_bin) = crate::guest::PODBOX_GUEST {
            std::fs::write(context_dir.join("podbox-guest"), guest_bin).with_context(|| {
                format!(
                    "failed to write guest binary to '{}'",
                    context_dir.join("podbox-guest").display()
                )
            })?;
        }

        let containerfile_path = context_dir.join("Containerfile");
        std::fs::write(&containerfile_path, &containerfile).with_context(|| {
            format!(
                "failed to write Containerfile to '{}'",
                containerfile_path.display()
            )
        })?;

        if events {
            crate::contract::emit_event(serde_json::json!({
                "event": "step", "index": 2, "total": 2,
                "text": format!("Installing packages on top of {local_tag}"),
            }));
        } else {
            println!("Installing packages on top of prebuilt image...");
        }
        let args: Vec<std::ffi::OsString> = vec![
            "build".into(),
            "-t".into(),
            local_tag.clone().into(),
            "-f".into(),
            containerfile_path.clone().into(),
            context_dir.clone().into(),
        ];
        let mut log = open_log(
            &config.container.name,
            &format!(
                "podman build -t {local_tag} -f {} {}",
                containerfile_path.display(),
                context_dir.display()
            ),
        )?;
        let start = Instant::now();
        run_podman_logged(&args, &config.container.name, "overlay build", &mut log)?;
        ui::ok(&format!(
            "Image {local_tag} ready with packages installed ({:.1}s)",
            start.elapsed().as_secs_f32()
        ));
    } else {
        if events {
            crate::contract::emit_event(serde_json::json!({
                "event": "step", "index": 2, "total": 2,
                "text": format!("Tagging {local_tag}"),
            }));
        } else {
            println!("Tagging as {local_tag}...");
        }
        let status = std::process::Command::new("podman")
            .args(["tag", &image_ref, &local_tag])
            .status()?;
        if !status.success() {
            return Err(PodboxError::TagFailed {
                image: local_tag.clone(),
            }
            .into());
        }
        if !events {
            println!("Image {local_tag} ready.");
        }
    }

    std::fs::create_dir_all(&config.container.home).with_context(|| {
        format!(
            "failed to create home dir '{}'",
            config.container.home.display()
        )
    })?;
    let digest = crate::podman::image_digest(&local_tag)?;
    let lock = crate::lock::LockFile {
        config_checksum: image_key,
        image_digest: digest,
    };
    crate::lock::write(&lock_path, &lock)?;

    if events {
        crate::contract::emit_event(serde_json::json!({
            "event": "done", "ok": true, "cached": false,
        }));
    }
    Ok(())
}

/// Resolve the distro family for package installation on a prebuilt image.
///
/// An explicit `manager` always wins. An omitted one is detected from the
/// image reference (falling back to the base name), so prebuilt images from
/// any distro family build without pinning a manager.
pub(crate) fn resolve_prebuilt_distro(config: &Config) -> DistroFamily {
    crate::codegen::distros::family_for_config(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn config_with(image_base: &str, image_ref: Option<&str>, manager: Option<&str>) -> Config {
        let manager_line = manager
            .map(|m| format!("manager = \"{m}\"\n"))
            .unwrap_or_default();
        let image_line = image_ref
            .map(|r| format!("image = \"{r}\"\n"))
            .unwrap_or_default();
        Config::parse(&format!(
            r#"
[image]
base = "{image_base}"
name = "env"
{image_line}
[image.packages]
{manager_line}
[container]
name = "env"
home = "~/env"
"#
        ))
        .unwrap()
    }

    #[test]
    fn omitted_manager_detects_from_the_image_reference() {
        let cfg = config_with(
            "ubuntu-latest",
            Some("ghcr.io/bethropolis/podbox:ubuntu-latest"),
            None,
        );
        assert_eq!(resolve_prebuilt_distro(&cfg), DistroFamily::DebianLike);

        let cfg = config_with("cachyos/cachyos:latest", None, None);
        assert_eq!(resolve_prebuilt_distro(&cfg), DistroFamily::ArchLike);

        // Unknown names stay Unknown (whose manager resolves to dnf),
        // rather than guessing a family.
        let cfg = config_with("unknown:latest", None, None);
        assert_eq!(resolve_prebuilt_distro(&cfg), DistroFamily::Unknown);
        assert_eq!(
            resolve_prebuilt_distro(&cfg).manager(),
            crate::config::PackageManager::Dnf
        );
    }

    #[test]
    fn explicit_manager_wins_over_detection() {
        let cfg = config_with(
            "ubuntu-latest",
            Some("ghcr.io/bethropolis/podbox:ubuntu-latest"),
            Some("dnf"),
        );
        assert_eq!(resolve_prebuilt_distro(&cfg), DistroFamily::FedoraLike);
    }
}
