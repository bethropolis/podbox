//! `podbox init`: interactively or non-interactively scaffold a new
//! container config (wizard, profile, or base-image forms).
//!
//! Extracted verbatim from `commands/create.rs`; re-exported from
//! `create` so `commands::create::run_init` keeps resolving.

use anyhow::Result;

use podbox::codegen::distros;
use podbox::config::{self, Config};

use std::path::PathBuf;

use super::create::{derive_container_name, detect_package_manager, read_profile_content};

/// Initialize a new container config.
pub fn run_init(
    dry_run: bool,
    image: Option<&str>,
    name: Option<&str>,
    interactive: bool,
    profile: Option<&str>,
) -> Result<()> {
    let shell_info = podbox::wizard::detect_host_shell();
    if !shell_info.detected && !interactive {
        eprintln!("Note: $SHELL not set or unrecognized, defaulting to fish.");
    }

    if interactive {
        if !distros::is_tty() {
            anyhow::bail!("--interactive requires a TTY (stdin is not a terminal)");
        }
        let profiles = podbox::profiles::all();
        let result = podbox::wizard::run_wizard(&profiles, &shell_info)?;
        if !result.confirmed {
            let toml = toml::to_string_pretty(&result.config)?;
            println!("{toml}");
            return Ok(());
        }
        let config_dir = config::profiles_dir();
        let config_path = config_dir.join(format!("{}.toml", result.name));
        if config::find_config_path(&result.name).is_some() && !dry_run {
            anyhow::bail!(
                "Config already exists at '{}'. Remove it first.",
                config_path.display()
            );
        }
        if dry_run {
            let toml = toml::to_string_pretty(&result.config)?;
            println!("Would write to: {}", config_path.display());
            println!("---\n{toml}");
            return Ok(());
        }
        std::fs::create_dir_all(&config_dir)?;
        result.config.validate()?;
        let toml = toml::to_string_pretty(&result.config)?;
        std::fs::write(&config_path, &toml)?;
        println!("Created: {}", config_path.display());
        println!(
            "Run `podbox start -C {}` to build, enable, and start.",
            result.name
        );
        return Ok(());
    }

    if let Some(p) = profile {
        let profile_content = read_profile_content(p)?;
        let mut cfg = Config::parse(&profile_content)?;
        podbox::wizard::apply_shell_defaults(&mut cfg, &shell_info);
        let container_name = name.unwrap_or(&cfg.container.name).to_string();
        cfg.container.name.clone_from(&container_name);
        cfg.image.name.clone_from(&container_name);
        let toml_str = toml::to_string_pretty(&cfg)?;
        let config_dir = config::profiles_dir();
        let config_path = config_dir.join(format!("{container_name}.toml"));

        if config::find_config_path(&container_name).is_some() && !dry_run {
            anyhow::bail!(
                "Config already exists at '{}'. Remove it first or use a different name.",
                config_path.display()
            );
        }

        if dry_run {
            println!("Would create: {}", config_path.display());
            println!("---\n{toml_str}");
            return Ok(());
        }

        std::fs::create_dir_all(&config_dir)?;
        std::fs::write(&config_path, &toml_str)?;
        println!("Created config: {}", config_path.display());
        println!();
        println!(
            "Profile created! Run `podbox create {container_name}` or `podbox start` to spin it up."
        );
        return Ok(());
    }

    if image.is_none() {
        let profiles = podbox::profiles::all();
        println!("Available profiles:");
        for p in &profiles {
            println!("  {:<8} {}  —  {}", p.name, p.label, p.description);
        }
        println!();
        println!("Usage:");
        println!("  podbox init <image>         Create a custom container (e.g. fedora:44)");
        println!("  podbox init --profile <name>  Create from a prebuilt profile");
        println!("  podbox init -i               Interactive wizard");
        anyhow::bail!("Specify a base image or use --profile.");
    }

    let base = image.unwrap();
    let container_name = derive_container_name(base, name);

    let mut cfg = Config::embedded();
    cfg.image.base = base.to_string();
    cfg.image.name.clone_from(&container_name);
    cfg.container.name.clone_from(&container_name);
    cfg.container.home = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("containers")
        .join(&container_name);
    cfg.image.packages.manager = detect_package_manager(base);

    cfg.container.shell.clear();
    podbox::wizard::apply_shell_defaults(&mut cfg, &shell_info);
    cfg.validate()?;
    let toml_str = toml::to_string_pretty(&cfg)?;
    let config_dir = config::profiles_dir();
    let config_path = config_dir.join(format!("{container_name}.toml"));

    if config::find_config_path(&container_name).is_some() && !dry_run {
        let alt = format!("{container_name}-alt");
        anyhow::bail!(
            "Config already exists at '{}'.\n\
             Use --name to specify a different name (e.g. --name {}).",
            config_path.display(),
            alt
        );
    }

    if dry_run {
        println!("Would create: {}", config_path.display());
        println!("---\n{toml_str}");
        return Ok(());
    }

    std::fs::create_dir_all(&config_dir)?;
    std::fs::write(&config_path, &toml_str)?;
    println!("Created config: {}", config_path.display());
    println!();
    println!(
        "Container created! Run `podbox create {container_name}` or `podbox start` to spin it up."
    );

    Ok(())
}
