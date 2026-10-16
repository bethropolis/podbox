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

    // The clone's home sits next to the source home, whatever it is: the
    // source home comes from its config, not from a hardcoded guess.
    let src_home = cfg.container.home.clone();
    let new_home = src_home.parent().map(|p| p.join(dst)).unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("~"))
            .join("containers")
            .join(dst)
    });
    cfg.image.name = dst.to_string();
    cfg.container.name = dst.to_string();
    cfg.container.home.clone_from(&new_home);

    let new_content = toml::to_string_pretty(&cfg)?;
    if let Some(parent) = dst_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    podbox::fsutil::write_atomic(&dst_path, new_content.as_bytes())?;
    println!("Created config: {}", dst_path.display());

    if copy_home {
        if src_home.exists() {
            let entries = std::fs::read_dir(&src_home).map_or(0, std::iter::Iterator::count);
            if entries > 500 {
                eprintln!("Warning: source home has {entries} items. Copying may take a while.");
            }
            std::fs::create_dir_all(&new_home)?;
            // Copy the *contents*: `cp -a src dst` nests (`dst/src`) when
            // dst exists and fails when its parent is missing.
            let status = podbox::process::run_checked(
                "cp",
                &podbox::process::args(&[
                    "-a",
                    &src_home.join(".").to_string_lossy(),
                    &new_home.to_string_lossy(),
                ]),
            );
            if let Err(e) = status {
                anyhow::bail!("Failed to copy home directory contents: {e:#}");
            }
            // A fresh box must re-provision dotfiles; the copied stamp
            // would claim the source's provisioning covers the clone.
            let _ = std::fs::remove_file(new_home.join(".podbox-dotfiles.done"));
            println!("Home directory copied.");
        } else {
            eprintln!(
                "Warning: source home '{}' does not exist, skipping copy.",
                src_home.display()
            );
        }
    }

    println!();
    println!("Cloned '{src}' → '{dst}'");
    println!("Run `podbox build {dst}` to build and start.");
    Ok(())
}
