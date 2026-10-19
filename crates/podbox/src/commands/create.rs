use std::io::Read as _;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use podbox::codegen::distros;
use podbox::config::{self, Config};
use podbox::editor;
use podbox::error::PodboxError;

/// Build image, install Quadlet, and start the container.
fn finish_create(cfg: &Config, _caller_name: &str, dry_run: bool, no_start: bool) -> Result<()> {
    // The caller's name can be a file name or `--name`; the box itself is
    // installed under `container.name`, so that is what starts and exports.
    let container_name = cfg.runtime_name();
    if dry_run {
        println!("podbox build");
    } else {
        let local_tag = cfg.local_image_tag();
        if podbox::podman::image_exists(&local_tag).unwrap_or(false) {
            println!("Image already exists, skipping build.");
        } else {
            let env = podbox::env::resolve()?;
            let xdg = podbox::xdg::resolve(&cfg.integration.xdg_dirs)?;
            podbox::build::run(
                cfg,
                &env,
                &xdg,
                false,
                false,
                podbox::cli::OutputFormat::Text,
            )?;
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

    let dotfiles_pending =
        cfg.dotfiles.is_some() && !crate::commands::dotfiles::is_provisioned(cfg);
    let dotfiles_staged = if !dotfiles_pending {
        true
    } else if dry_run {
        crate::commands::dotfiles::stage(cfg, &podbox::env::resolve()?, true).is_ok()
    } else {
        match crate::commands::dotfiles::stage(cfg, &podbox::env::resolve()?, false) {
            Ok(()) => true,
            Err(err) => {
                eprintln!("Warning: dotfiles source setup failed: {err:#}");
                eprintln!(
                    "Fix the source issue and run `podbox dotfiles sync {container_name}` to retry."
                );
                false
            }
        }
    };

    if no_start {
        println!("Container created but not started (--no-start).");
        println!("Run `podbox enter {container_name}` to start and enter it.");
    } else if dry_run {
        println!("podman start {container_name}");
        if dotfiles_pending && dotfiles_staged {
            crate::commands::dotfiles::provision(cfg, &podbox::env::resolve()?, true)?;
        }
        crate::commands::provision::run_for_create(cfg, &podbox::env::resolve()?, true)?;
    } else {
        podbox::ui::step("Starting container...");
        // The Quadlet unit is freshly installed, so whether systemd owns the
        // start depends only on systemctl being present (see
        // `runner::Runtime::for_system`).
        let ctx = crate::ctx::Ctx { dry_run };
        crate::commands::runner::Runtime::for_system().start(&ctx, cfg)?;
        podbox::ui::ok(&format!("Container '{container_name}' is running"));
        // `create` starts via systemd/podman directly, bypassing `run_start`,
        // so exports happen here rather than riding along with it.
        podbox::export::ensure_exports(cfg, container_name);
        if dotfiles_pending && dotfiles_staged {
            if let Err(err) =
                crate::commands::dotfiles::provision(cfg, &podbox::env::resolve()?, false)
            {
                eprintln!("Warning: dotfiles provisioning failed: {err:#}");
                eprintln!(
                    "Fix the issue and run `podbox dotfiles sync {container_name}` to retry."
                );
            }
        }
        // Provision steps run once, after dotfiles, in declared order.
        if let Err(err) =
            crate::commands::provision::run_for_create(cfg, &podbox::env::resolve()?, false)
        {
            eprintln!("Warning: guest provisioning failed: {err:#}");
            eprintln!(
                "Run `podbox provision status {container_name}`, then `podbox provision sync {container_name}` to retry."
            );
        }
        println!("Run `podbox enter` to enter.");
    }
    if no_start && dotfiles_pending {
        println!(
            "Dotfiles provisioning will run after the container is started; use `podbox dotfiles sync {container_name}` to run it explicitly."
        );
    }
    if no_start && !cfg.provision.is_empty() {
        println!(
            "Provisioning steps will run after the container is started; use `podbox provision status {container_name}` to check."
        );
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

/// Open a config file in the user's editor. Callers re-load the file after
/// this returns, so edits (or pasted Studio TOML) take effect.
fn edit_config_file(path: &Path) -> Result<()> {
    let ed = editor::resolve()?;
    editor::open(&ed, path)?;
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

/// Refuse a new config whose runtime name another config already claims.
///
/// Lookups by name use the file stem, so `work.toml` with `container.name =
/// "dev"` would not be found by `create --name dev`. Writing `dev.toml` then
/// would give two configs for one container. Checking the runtime name here
/// stops that.
fn ensure_runtime_name_free(existing: &[PathBuf], container_name: &str) -> Result<()> {
    let taken = existing
        .iter()
        .find(|path| Config::load(path).is_ok_and(|cfg| cfg.container.name == container_name));
    if let Some(path) = taken {
        anyhow::bail!(
            "container '{container_name}' is already defined by '{}'. Choose another name or edit that config.",
            path.display()
        );
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_config(dir: &Path, file: &str, runtime: &str) -> PathBuf {
        let mut cfg = Config::embedded();
        cfg.container.name = runtime.to_string();
        cfg.image.name = runtime.to_string();
        let path = dir.join(file);
        std::fs::write(&path, toml::to_string_pretty(&cfg).unwrap()).unwrap();
        path
    }

    #[test]
    fn new_config_refused_when_runtime_name_is_claimed_by_another_file() {
        let dir = std::env::temp_dir().join(format!("podbox-create-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let work = write_config(&dir, "work.toml", "dev");
        let other = write_config(&dir, "other.toml", "other");
        let existing = vec![work.clone(), other];

        let err = ensure_runtime_name_free(&existing, "dev").unwrap_err();
        assert!(err.to_string().contains("already defined by"), "{err}");
        assert!(err.to_string().contains(&work.display().to_string()));
        assert!(ensure_runtime_name_free(&existing, "fresh").is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
