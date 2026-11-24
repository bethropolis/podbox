use std::io::Read as _;
use std::path::Path;

use anyhow::{Context, Result};

use podbox::config::{self, Config};

/// Validate a TOML file and stage it into profiles/ as a user profile.
///
/// Returns the staged container name. With `dry_run` nothing is written.
/// The `--name` override (else the file stem) becomes the lookup key and is
/// stamped into `[container] name` / `[image] name`; otherwise the file
/// bytes land verbatim, comments and formatting intact.
pub fn stage_import(
    src: &Path,
    name_override: Option<&str>,
    packages: Option<&str>,
    dry_run: bool,
) -> Result<String> {
    let from_stdin = src.as_os_str() == "-";
    let raw = if from_stdin {
        if name_override.is_none() {
            anyhow::bail!("`--import -` (stdin) requires `--name`.");
        }
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        buf
    } else {
        std::fs::read_to_string(src)
            .with_context(|| format!("failed to read import '{}'", src.display()))?
    };
    let mut cfg = Config::parse(&raw)?;
    cfg.validate()?;
    let stem = if from_stdin {
        String::new()
    } else {
        src.file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let effective = name_override.unwrap_or(&stem).to_string();
    if effective.is_empty() {
        anyhow::bail!(
            "Cannot derive a container name from '{}'. Pass `--name`.",
            src.display()
        );
    }
    let target = config::profiles_dir().join(format!("{effective}.toml"));
    if target.is_file() && !dry_run {
        anyhow::bail!(
            "Config '{effective}' already exists at {}. Run `podbox create {effective}` to use it.",
            target.display()
        );
    }
    if let Some(pkgs) = packages {
        for pkg in podbox::config::parse_package_list(pkgs) {
            if !cfg.image.packages.install.contains(&pkg) {
                cfg.image.packages.install.push(pkg);
            }
        }
    }
    let mut out = raw;
    if cfg.container.name != effective || cfg.image.name != effective {
        // Stamping only when names disagree keeps untouched files byte-identical.
        if name_override.is_some() || cfg.container.name.is_empty() {
            cfg.container.name.clone_from(&effective);
            cfg.image.name.clone_from(&effective);
            out = toml::to_string_pretty(&cfg)?;
        }
    }
    if dry_run {
        println!("Would import {} -> {}", src.display(), target.display());
        println!("---\n{out}");
        return Ok(effective);
    }
    std::fs::create_dir_all(config::profiles_dir())?;
    podbox::fsutil::write_atomic(&target, out.as_bytes())?;
    println!("Imported {} -> {}", src.display(), target.display());
    Ok(effective)
}
