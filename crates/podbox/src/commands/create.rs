//! Container creation flow and source-selection facade.

mod finish;
mod import;
mod source;

pub use super::create_init::run_init;
pub use import::stage_import;
pub(super) use source::{derive_container_name, detect_package_manager, read_profile_content};

use std::path::Path;

use anyhow::Result;

use podbox::config::{self, Config};
use podbox::error::PodboxError;

use finish::finish_create;
use source::{edit_config_file, ensure_runtime_name_free};

/// Create a container: pull profile/image, build, install Quadlet, and start.
///
/// Never prompts: with `--yes` an omitted `--name` is derived from the image
/// reference instead of stopping with a suggestion.
#[allow(clippy::too_many_arguments)]
pub fn run_create(
    dry_run: bool,
    image: Option<&str>,
    import: Option<&Path>,
    name: Option<&str>,
    packages: Option<&str>,
    no_start: bool,
    edit: bool,
    yes: bool,
) -> Result<()> {
    // `--import` stages the file into profiles/, then the normal lookup
    // runs: the staged user config wins over any bundled profile.
    if let Some(src) = import {
        let staged = stage_import(src, name, packages, dry_run)?;
        if dry_run {
            return Ok(());
        }
        return run_create(
            dry_run,
            Some(&staged),
            None,
            name,
            None,
            no_start,
            edit,
            yes,
        );
    }
    let Some(image) = image else {
        anyhow::bail!("Specify a profile or image, or pass `--import <PATH>`.");
    };
    let is_profile = !image.contains('/') && !image.contains('\\');

    if is_profile && podbox::profiles::find(image).is_some() {
        let profile_content = read_profile_content(image)?;
        let template = Config::parse(&profile_content)?;
        let container_name = name.unwrap_or(&template.container.name).to_string();

        // An on-disk config for the resolved name wins over the profile
        // template, so `create dev` after pasting a Studio TOML builds your
        // file instead of the bundled template.
        if let Some(existing) = config::find_config_path(&container_name) {
            eprintln!(
                "Config already exists at '{}'. Reusing existing config.",
                existing.display()
            );
            if packages.is_some() {
                eprintln!("Note: --packages is ignored when reusing an existing config.");
            }
            if dry_run {
                let cfg = Config::load(&existing)?;
                let toml_str = toml::to_string_pretty(&cfg)?;
                println!("Would reuse existing config: {}", existing.display());
                println!("---\n{toml_str}");
                return finish_create(&cfg, &container_name, dry_run, no_start);
            }
            let mut cfg = Config::load(&existing)?;
            if edit {
                edit_config_file(&existing)?;
                cfg = Config::load(&existing)?;
            }
            return finish_create(&cfg, &container_name, dry_run, no_start);
        }

        let shell_info = podbox::wizard::detect_host_shell();
        if !shell_info.detected {
            eprintln!("Note: $SHELL not set or unrecognized, defaulting to bash.");
        }

        let mut cfg = template;
        podbox::wizard::apply_shell_defaults(&mut cfg, &shell_info);
        if let Some(pkgs) = packages {
            for pkg in podbox::config::parse_package_list(pkgs) {
                if !cfg.image.packages.install.contains(&pkg) {
                    cfg.image.packages.install.push(pkg);
                }
            }
        }
        let container_name = name.unwrap_or(&cfg.container.name).to_string();
        cfg.container.name.clone_from(&container_name);
        cfg.image.name.clone_from(&container_name);
        ensure_runtime_name_free(&config::list_configs(), &container_name)?;
        let config_dir = config::profiles_dir();
        let config_path = config_dir.join(format!("{container_name}.toml"));

        // Any on-disk config returned early above, so this always writes.
        if dry_run {
            let config_toml = toml::to_string_pretty(&cfg)?;
            println!("Would create config: {}", config_path.display());
            println!("---\n{config_toml}");
        } else {
            let config_toml = toml::to_string_pretty(&cfg)?;
            std::fs::create_dir_all(&config_dir)?;
            podbox::fsutil::write_atomic(&config_path, config_toml.as_bytes())?;
            println!("Created config: {}", config_path.display());
        }

        if edit && !dry_run {
            edit_config_file(&config_path)?;
            cfg = Config::load(&config_path)?;
        }

        return finish_create(&cfg, &container_name, dry_run, no_start);
    }

    if is_profile {
        eprintln!(
            "Note: '{}' is not a known profile (available: {}). Treating it as an image reference to pull.",
            image,
            podbox::profiles::list_names().join(", ")
        );
    }

    // Non-interactive mode: derive the container name up front so the whole
    // raw-image path (conflict check, dry-run plan, config write) runs.
    let derived;
    let name = match name {
        Some(n) => Some(n),
        None if yes => {
            derived = derive_container_name(image, None);
            println!("Using derived container name '{derived}' (--yes).");
            Some(derived.as_str())
        }
        None => None,
    };

    // A bare name that resolves to an on-disk config but no known profile
    // stops here: without `--name` there is nothing to derive. (The `--name`
    // case is handled in-branch below, reusing the existing config.)
    if name.is_none() && config::find_config_path(image).is_some() {
        let existing_path = config::find_config_path(image).unwrap();
        let stem = existing_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        anyhow::bail!(
            "Config '{stem}' already exists at {}.\n\
             Use `podbox build -C {stem}` to build, or `podbox start -C {stem}` to start.",
            existing_path.display(),
        );
    }

    if dry_run {
        println!("podman pull {image}");
        if name.is_none() {
            let suggested = derive_container_name(image, None);
            println!(
                "Run `podbox init {image} --name <name>` to create a config (e.g. --name {suggested})."
            );
        }
        return Ok(());
    }

    println!("Pulling image...");
    let status = std::process::Command::new("podman")
        .args(["pull", image])
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .status()
        .map_err(|_| PodboxError::PullFailed {
            image: image.into(),
        })?;

    if !status.success() {
        return Err(PodboxError::PullFailed {
            image: image.into(),
        }
        .into());
    }

    if let Some(n) = name {
        let container_name = n.to_string();
        // An on-disk config for `--name` wins over the derived image config.
        if let Some(existing) = config::find_config_path(&container_name) {
            eprintln!(
                "Config already exists at '{}'. Reusing existing config.",
                existing.display()
            );
            if packages.is_some() {
                eprintln!("Note: --packages is ignored when reusing an existing config.");
            }
            let mut cfg = Config::load(&existing)?;
            if edit && !dry_run {
                edit_config_file(&existing)?;
                cfg = Config::load(&existing)?;
            }
            println!("Image '{image}' pulled and configured.");
            return finish_create(&cfg, &container_name, dry_run, no_start);
        }
        let shell_info = podbox::wizard::detect_host_shell();
        let mut cfg = Config::embedded();
        cfg.image.base = image.to_string();
        cfg.image.name.clone_from(&container_name);
        cfg.container.name.clone_from(&container_name);
        cfg.container.home = podbox::config::default_home(&container_name);
        cfg.image.packages.manager = Some(detect_package_manager(image));
        cfg.container.shell.clear();
        podbox::wizard::apply_shell_defaults(&mut cfg, &shell_info);
        if let Some(pkgs) = packages {
            for pkg in podbox::config::parse_package_list(pkgs) {
                if !cfg.image.packages.install.contains(&pkg) {
                    cfg.image.packages.install.push(pkg);
                }
            }
        }
        cfg.validate()?;

        ensure_runtime_name_free(&config::list_configs(), &container_name)?;
        let config_dir = config::profiles_dir();
        let config_path = config_dir.join(format!("{container_name}.toml"));
        // Any on-disk config returned early above, so this always writes.
        if dry_run {
            let toml_str = toml::to_string_pretty(&cfg)?;
            println!("Would create config: {}", config_path.display());
            println!("---\n{toml_str}");
        } else {
            std::fs::create_dir_all(&config_dir)?;
            let toml_str = toml::to_string_pretty(&cfg)?;
            podbox::fsutil::write_atomic(&config_path, toml_str.as_bytes())?;
            println!("Created config: {}", config_path.display());
        }

        if edit && !dry_run {
            edit_config_file(&config_path)?;
            cfg = Config::load(&config_path)?;
        }

        println!("Image '{image}' pulled and configured.");
        return finish_create(&cfg, &container_name, dry_run, no_start);
    }

    println!("Image '{image}' pulled.");
    let suggested = derive_container_name(image, None);
    println!(
        "Run `podbox init {image} --name <name>` to create a config (e.g. --name {suggested})."
    );
    Ok(())
}
