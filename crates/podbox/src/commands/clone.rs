use anyhow::Result;

use podbox::config::{self, Config};

/// Clone an existing container config to a new name.
pub fn run_clone(src: &str, dst: &str, copy_home: bool, dry_run: bool) -> Result<()> {
    let src_path = config::find_config_path(src).ok_or_else(|| {
        anyhow::anyhow!(
            "Source config '{}' not found at {}/{{profiles/,}}{}.toml",
            src,
            config::config_dir().display(),
            src
        )
    })?;
    let dst_path = config::profiles_dir().join(format!("{dst}.toml"));

    if config::find_config_path(dst).is_some() {
        anyhow::bail!(
            "Destination config '{}' already exists at {}",
            dst,
            dst_path.display()
        );
    }

    if dry_run {
        println!("Would clone '{src}' to '{dst}'");
        if copy_home {
            println!("Would also copy home directory contents.");
        }
        return Ok(());
    }

    let content = std::fs::read_to_string(&src_path)?;
    let mut cfg: Config = toml::from_str(&content)
        .map_err(|e| anyhow::anyhow!("Failed to parse '{}': {}", src_path.display(), e))?;

    let new_home = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("~"))
        .join("containers")
        .join(dst);
    cfg.image.name = dst.to_string();
    cfg.container.name = dst.to_string();
    cfg.container.home.clone_from(&new_home);

    let new_content = toml::to_string_pretty(&cfg)?;
    if let Some(parent) = dst_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&dst_path, &new_content)?;
    println!("Created config: {}", dst_path.display());

    if copy_home {
        let home = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("~"))
            .join("containers")
            .join(src);
        if home.exists() {
            let entries = std::fs::read_dir(&home).map_or(0, std::iter::Iterator::count);
            if entries > 500 {
                eprintln!("Warning: source home has {entries} items. Copying may take a while.");
            }
            let status = std::process::Command::new("cp")
                .args(["-a", &home.to_string_lossy(), &new_home.to_string_lossy()])
                .status()?;
            if !status.success() {
                anyhow::bail!("Failed to copy home directory contents.");
            }
            println!("Home directory copied.");
        } else {
            eprintln!(
                "Warning: source home '{}' does not exist, skipping copy.",
                home.display()
            );
        }
    }

    println!();
    println!("Cloned '{src}' → '{dst}'");
    println!("Run `podbox build {dst}` to build and start.");
    Ok(())
}
