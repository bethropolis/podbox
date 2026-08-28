//! Prebuilt-image path: pull (or re-tag) a published image, optionally
//! layer packages on top, and record the lock file.
//!
//! Extracted verbatim from `build.rs`.

use std::time::Instant;

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

use crate::codegen::distros::DistroFamily;
use crate::config::Config;
use crate::error::PodboxError;
use crate::ui;

use super::{build_context_dir, checksum, open_log, run_podman_logged};

pub(crate) fn run_prebuilt(config: &Config, dry_run: bool, rebuild: bool) -> Result<()> {
    let image_ref = match config.image.source() {
        crate::config::ImageSource::Prebuilt { ref_str } => ref_str,
        _ => config.image.base.clone(),
    };
    let local_tag = format!("localhost/podbox-{}:latest", config.image.name);
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

    // Checksum covers both the image ref and the install list so that
    // changing either triggers a rebuild.
    let definition_toml = toml::to_string(config)
        .with_context(|| "failed to serialize definition config".to_string())?;
    let config_checksum = checksum(&definition_toml);

    if !rebuild {
        if let Some(lock) = crate::lock::read(&lock_path)? {
            if lock.config_checksum == config_checksum && crate::podman::image_exists(&local_tag)? {
                println!("Prebuilt image already present as {local_tag}. Skipping pull.");
                println!("Use --rebuild to re-pull.");
                return Ok(());
            }
        }
    }

    if dry_run {
        println!("Would pull: {image_ref}");
        if has_packages {
            println!(
                "Would install packages on top: {}",
                config.image.packages.install.join(", ")
            );
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

    println!("Pulling {image_ref}...");
    let status = std::process::Command::new("podman")
        .args(["pull", &image_ref])
        .status()?;
    if !status.success() {
        return Err(PodboxError::PullFailed {
            image: image_ref.clone(),
        }
        .into());
    }

    if has_packages {
        // Layer the config's packages on top of the prebuilt image.
        let distro = resolve_prebuilt_distro(config);
        let install_cmd = distro.install_cmd();
        let clean_cmd = distro.clean_cmd();

        let packages = config.image.packages.install.join(" ");
        let run_line = if clean_cmd.is_empty() {
            format!("RUN {install_cmd} {packages}")
        } else {
            format!("RUN {install_cmd} {packages} && {clean_cmd}")
        };

        let containerfile = format!("FROM {image_ref}\n{run_line}\n");

        std::fs::create_dir_all(&context_dir)
            .with_context(|| format!("failed to create context dir '{}'", context_dir.display()))?;

        let containerfile_path = context_dir.join("Containerfile");
        std::fs::write(&containerfile_path, &containerfile).with_context(|| {
            format!(
                "failed to write Containerfile to '{}'",
                containerfile_path.display()
            )
        })?;

        println!("Installing packages on top of prebuilt image...");
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
        println!("Tagging as {local_tag}...");
        let status = std::process::Command::new("podman")
            .args(["tag", &image_ref, &local_tag])
            .status()?;
        if !status.success() {
            return Err(PodboxError::TagFailed {
                image: local_tag.clone(),
            }
            .into());
        }
        println!("Image {local_tag} ready.");
    }

    std::fs::create_dir_all(&config.container.home).with_context(|| {
        format!(
            "failed to create home dir '{}'",
            config.container.home.display()
        )
    })?;
    let digest = crate::podman::image_digest(&local_tag)?;
    let lock = crate::lock::LockFile {
        config_checksum,
        image_digest: digest,
    };
    crate::lock::write(&lock_path, &lock)?;

    Ok(())
}

/// Resolve the distro family for package installation on a prebuilt image.
/// Respects the explicit `manager` field in the config, falling back to
/// name-based detection via `DistroFamily`.
pub(crate) fn resolve_prebuilt_distro(config: &Config) -> DistroFamily {
    match config.image.packages.manager {
        crate::config::PackageManager::Apt => DistroFamily::DebianLike,
        crate::config::PackageManager::Dnf => DistroFamily::FedoraLike,
        crate::config::PackageManager::Pacman => DistroFamily::ArchLike,
        crate::config::PackageManager::Apk => DistroFamily::AlpineLike,
        crate::config::PackageManager::Zypper => DistroFamily::SuseLike,
    }
}
