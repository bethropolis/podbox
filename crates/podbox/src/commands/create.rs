use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use podbox::codegen::distros;
use podbox::config::{self, Config};
use podbox::editor;
use podbox::error::PodboxError;
use podbox::systemd;

/// Build image, install Quadlet, and start the container.
fn finish_create(cfg: &Config, container_name: &str, dry_run: bool, no_start: bool) -> Result<()> {
    if dry_run {
        println!("podbox build");
    } else {
        let local_tag = format!("localhost/podbox-{}:latest", cfg.image.name);
        if podbox::podman::image_exists(&local_tag).unwrap_or(false) {
            println!("Image already exists, skipping build.");
        } else {
            let env = podbox::env::resolve()?;
            let xdg = podbox::xdg::resolve(&cfg.integration.xdg_dirs)?;
            podbox::build::run(cfg, &env, &xdg, false, false)?;
        }
    }

    if dry_run {
        println!("podbox enable");
    } else {
        println!("Installing Quadlet files...");
        let env = podbox::env::resolve()?;
        let xdg = podbox::xdg::resolve(&cfg.integration.xdg_dirs)?;
        podbox::quadlet_install::install(cfg, &env, &xdg, false)?;
    }

    if no_start {
        println!("Container created but not started (--no-start).");
        println!("Run `podbox enter {container_name}` to start and enter it.");
    } else if dry_run {
        println!("podman start {container_name}");
    } else {
        podbox::ui::step("Starting container...");
        if systemd::is_available() {
            systemd::start_unit(container_name)?;
        } else {
            let args = podbox::process::args(&["start", container_name]);
            podbox::process::spawn_interactive("podman", &args)?;
        }
        podbox::ui::ok(&format!("Container '{container_name}' is running"));
        println!("Run `podbox enter` to enter.");
    }

    if !dry_run {
        let _ = config::write_active_context(container_name);
        let _ = podbox::history::record(
            container_name,
            "create",
            &format!("image {}", cfg.image.name),
        );
    }

    Ok(())
}

pub(super) fn read_profile_content(profile: &str) -> Result<String> {
    if profile.contains('/') || profile.contains('\\') {
        std::fs::read_to_string(Path::new(profile))
            .with_context(|| format!("failed to read profile '{profile}'"))
    } else {
        let found = podbox::profiles::find(profile).ok_or_else(|| {
            let names = podbox::profiles::list_names();
            anyhow::anyhow!(
                "Unknown profile '{}'. Available profiles: {}",
                profile,
                names.join(", ")
            )
        })?;
        Ok(found.toml)
    }
}

pub(super) fn derive_container_name(image: &str, custom_name: Option<&str>) -> String {
    if let Some(name) = custom_name {
        return name.to_string();
    }
    let image_part = image.split_once(':').map_or(image, |(n, _)| n);
    let short = image_part.split('/').next_back().unwrap_or(image_part);
    let tag = image.split_once(':').map_or("latest", |(_, t)| t);
    if tag == "latest" || tag.is_empty() {
        short.to_string()
    } else {
        format!("{}-{}", short, tag.replace('.', "-"))
    }
}

pub(super) fn detect_package_manager(image: &str) -> podbox::config::PackageManager {
    distros::detect_package_manager(image)
}

pub use super::create_init::run_init;

/// Create a container: pull profile/image, build, install Quadlet, and start.
pub fn run_create(
    dry_run: bool,
    image: &str,
    name: Option<&str>,
    packages: Option<&str>,
    no_start: bool,
    edit: bool,
) -> Result<()> {
    let is_profile = !image.contains('/') && !image.contains('\\');

    if is_profile && podbox::profiles::find(image).is_some() {
        let profile_content = read_profile_content(image)?;

        let shell_info = podbox::wizard::detect_host_shell();
        if !shell_info.detected {
            eprintln!("Note: $SHELL not set or unrecognized, defaulting to fish.");
        }

        let mut cfg = Config::parse(&profile_content)?;
        podbox::wizard::apply_shell_defaults(&mut cfg, &shell_info);
        if let Some(pkgs) = packages {
            for pkg in pkgs.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if !cfg.image.packages.install.contains(&pkg.to_string()) {
                    cfg.image.packages.install.push(pkg.to_string());
                }
            }
        }
        let container_name = name.unwrap_or(&cfg.container.name).to_string();
        cfg.container.name.clone_from(&container_name);
        cfg.image.name.clone_from(&container_name);
        let config_dir = config::profiles_dir();
        let config_path = config_dir.join(format!("{container_name}.toml"));

        if config::find_config_path(&container_name).is_some() && !dry_run {
            eprintln!(
                "Config already exists at '{}'. Reusing existing config.",
                config_path.display()
            );
        } else {
            let config_toml = toml::to_string_pretty(&cfg)?;
            if dry_run {
                println!("Would create config: {}", config_path.display());
                println!("---\n{config_toml}");
            } else {
                std::fs::create_dir_all(&config_dir)?;
                std::fs::write(&config_path, &config_toml)?;
                println!("Created config: {}", config_path.display());
            }
        }

        if edit && !dry_run {
            let ed = editor::resolve()?;
            editor::open(&ed, &config_path)?;
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

    let existing_name = match name {
        Some(n) => Some(n.to_string()),
        None => Some(image.to_string()),
    };
    if let Some(ref check) = existing_name
        && config::find_config_path(check).is_some()
    {
        let existing_path = config::find_config_path(check).unwrap();
        let stem = existing_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy();
        anyhow::bail!(
            "Config '{}' already exists at {}.\n\
             Use `podbox build -C {}` to build, or `podbox start -C {}` to start.",
            stem,
            existing_path.display(),
            stem,
            stem
        );
    }

    if dry_run {
        println!("podman pull {image}");
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
        let shell_info = podbox::wizard::detect_host_shell();
        let mut cfg = Config::embedded();
        cfg.image.base = image.to_string();
        cfg.image.name.clone_from(&container_name);
        cfg.container.name.clone_from(&container_name);
        cfg.container.home = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join("containers")
            .join(&container_name);
        cfg.image.packages.manager = detect_package_manager(image);
        cfg.container.shell.clear();
        podbox::wizard::apply_shell_defaults(&mut cfg, &shell_info);
        if let Some(pkgs) = packages {
            for pkg in pkgs.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if !cfg.image.packages.install.contains(&pkg.to_string()) {
                    cfg.image.packages.install.push(pkg.to_string());
                }
            }
        }
        cfg.validate()?;

        let config_dir = config::profiles_dir();
        let config_path = config_dir.join(format!("{container_name}.toml"));
        if config::find_config_path(&container_name).is_some() {
            eprintln!(
                "Config already exists at '{}'. Reusing existing config.",
                config_path.display()
            );
        } else {
            std::fs::create_dir_all(&config_dir)?;
            let toml_str = toml::to_string_pretty(&cfg)?;
            std::fs::write(&config_path, &toml_str)?;
            println!("Created config: {}", config_path.display());
        }

        if edit && !dry_run {
            let ed = editor::resolve()?;
            editor::open(&ed, &config_path)?;
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
