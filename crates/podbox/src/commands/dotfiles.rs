use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use podbox::config::{Config, DotfilesCloneOn};
use podbox::env::HostEnv;

struct DotfilesPaths {
    host_target: PathBuf,
    guest_home: String,
    guest_target: String,
}

fn target_relative(target: &str) -> &str {
    if target == "~" {
        ""
    } else {
        target.strip_prefix("~/").unwrap_or(target)
    }
}

fn paths(config: &Config, env: &HostEnv) -> Result<Option<DotfilesPaths>> {
    let Some(dotfiles) = &config.dotfiles else {
        return Ok(None);
    };
    let relative = target_relative(&dotfiles.target);
    let host_target = config.container.home.join(relative);
    let guest_home = format!("/home/{}", env.username);
    let guest_target = Path::new(&guest_home)
        .join(relative)
        .to_string_lossy()
        .into_owned();
    Ok(Some(DotfilesPaths {
        host_target,
        guest_home,
        guest_target,
    }))
}

fn run_host(command: &mut Command, description: &str) -> Result<()> {
    let status = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to start {description}"))?;
    if !status.success() {
        anyhow::bail!("{description} failed with {status}");
    }
    Ok(())
}

fn source_is_host(source: &str) -> bool {
    source.starts_with("host:")
}

/// Stage host-readable sources before starting the container.
pub fn stage(config: &Config, env: &HostEnv, dry_run: bool) -> Result<()> {
    let Some(dotfiles) = &config.dotfiles else {
        return Ok(());
    };
    let Some(paths) = paths(config, env)? else {
        return Ok(());
    };

    if !source_is_host(&dotfiles.source) && dotfiles.clone_on == DotfilesCloneOn::Container {
        return Ok(());
    }

    if dry_run {
        if source_is_host(&dotfiles.source) {
            println!(
                "cp -a host:{} {}/",
                dotfiles.source.trim_start_matches("host:"),
                paths.host_target.display()
            );
        } else {
            println!(
                "git clone {} {}",
                dotfiles.source,
                paths.host_target.display()
            );
        }
        return Ok(());
    }

    std::fs::create_dir_all(&config.container.home)?;
    if let Some(source) = dotfiles.source.strip_prefix("host:") {
        let source = podbox::config::expand_tilde(source);
        if !source.is_dir() {
            anyhow::bail!(
                "dotfiles host source '{}' is not a directory",
                source.display()
            );
        }
        let source_canonical = std::fs::canonicalize(&source)?;
        let target_canonical = std::fs::canonicalize(&paths.host_target).ok();
        if target_canonical.as_ref().is_some_and(|target| {
            target == &source_canonical || target.starts_with(&source_canonical)
        }) || paths.host_target.starts_with(&source_canonical)
        {
            anyhow::bail!("dotfiles source and target resolve to the same directory");
        }
        std::fs::create_dir_all(&paths.host_target)?;
        run_host(
            Command::new("cp")
                .arg("-a")
                .arg("--")
                .arg(source.join("."))
                .arg(&paths.host_target),
            "copying host dotfiles",
        )?;
    } else if paths.host_target.join(".git").is_dir() {
        run_host(
            Command::new("git")
                .arg("-C")
                .arg(&paths.host_target)
                .args(["pull", "--ff-only"]),
            "updating host dotfiles repository",
        )?;
    } else {
        if paths.host_target.exists() {
            anyhow::bail!(
                "dotfiles target '{}' exists and is not a Git repository; move it or choose another target",
                paths.host_target.display()
            );
        }
        if let Some(parent) = paths.host_target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        run_host(
            Command::new("git")
                .args(["clone", &dotfiles.source])
                .arg(&paths.host_target),
            "cloning dotfiles repository on host",
        )?;
    }
    Ok(())
}

fn distro(config: &Config) -> String {
    let image = config
        .image
        .image_ref
        .as_deref()
        .unwrap_or(&config.image.base)
        .to_ascii_lowercase();
    ["fedora", "debian", "ubuntu", "arch", "alpine", "cachy"]
        .into_iter()
        .find(|name| image.contains(name))
        .unwrap_or("unknown")
        .to_string()
}

fn guest_exec(
    config: &Config,
    env: &HostEnv,
    working_dir: &str,
    args: &[String],
    dry_run: bool,
) -> Result<()> {
    let mut command = Command::new("podman");
    command.args(["exec", "--user"]).arg(env.uid.to_string());
    let mut variables = vec![
        ("PODBOX", "1".to_string()),
        ("PODBOX_CONTAINER", config.container.name.clone()),
        ("PODBOX_DISTRO", distro(config)),
        ("PODBOX_HOME", format!("/home/{}", env.username)),
    ];
    if podbox::profiles::find(&config.image.name).is_some() {
        variables.push(("PODBOX_PROFILE", config.image.name.clone()));
    }
    if let Some(paths) = paths(config, env)? {
        variables.push(("PODBOX_DOTFILES_DIR", paths.guest_target));
    }
    for (key, value) in variables {
        command.arg("--env").arg(format!("{key}={value}"));
    }
    command
        .arg("--workdir")
        .arg(working_dir)
        .arg(&config.container.name)
        .args(args);
    if dry_run {
        println!("{command:?}");
        Ok(())
    } else {
        run_host(&mut command, "running dotfiles command in container")
    }
}

/// Run the container-side clone/install phase and record successful completion.
pub fn provision(config: &Config, env: &HostEnv, dry_run: bool) -> Result<()> {
    let Some(dotfiles) = &config.dotfiles else {
        return Ok(());
    };
    let path = paths(config, env)?.context("dotfiles paths are unavailable")?;
    if !source_is_host(&dotfiles.source) && dotfiles.clone_on == DotfilesCloneOn::Container {
        let guest_target_exists = path.host_target.join(".git").is_dir();
        if guest_target_exists {
            guest_exec(
                config,
                env,
                &path.guest_home,
                &[
                    "git".into(),
                    "-C".into(),
                    path.guest_target.clone(),
                    "pull".into(),
                    "--ff-only".into(),
                ],
                dry_run,
            )?;
        } else {
            if path.host_target.exists() {
                anyhow::bail!(
                    "dotfiles target '{}' exists and is not a Git repository; move it or choose another target",
                    path.host_target.display()
                );
            }
            let parent = Path::new(&path.guest_target)
                .parent()
                .context("dotfiles target has no parent")?
                .to_string_lossy()
                .into_owned();
            guest_exec(
                config,
                env,
                &path.guest_home,
                &["mkdir".into(), "-p".into(), parent],
                dry_run,
            )?;
            guest_exec(
                config,
                env,
                &path.guest_home,
                &[
                    "git".into(),
                    "clone".into(),
                    dotfiles.source.clone(),
                    path.guest_target.clone(),
                ],
                dry_run,
            )?;
        }
    }
    if let Some(install) = &dotfiles.install {
        guest_exec(
            config,
            env,
            &path.guest_target,
            &["sh".into(), "-lc".into(), install.clone()],
            dry_run,
        )?;
    }
    if dry_run {
        println!("touch {}/.podbox-dotfiles.done", path.guest_home);
    } else {
        std::fs::write(
            config.container.home.join(".podbox-dotfiles.done"),
            "done\n",
        )
        .context("writing dotfiles provisioning stamp")?;
        println!("Dotfiles provisioned for '{}'", config.container.name);
    }
    Ok(())
}

pub fn is_provisioned(config: &Config) -> bool {
    config
        .container
        .home
        .join(".podbox-dotfiles.done")
        .is_file()
}

pub fn sync(config: &Config, env: &HostEnv, dry_run: bool) -> Result<()> {
    if config.dotfiles.is_none() {
        anyhow::bail!(
            "no [dotfiles] configuration for '{}'",
            config.container.name
        );
    }
    stage(config, env, dry_run)?;
    if dry_run {
        println!("podman start {} (if stopped)", config.container.name);
        return provision(config, env, true);
    }
    crate::commands::ensure_running(
        &config.container.name,
        false,
        crate::commands::DEFAULT_START_TIMEOUT_SECS,
    )?;
    provision(config, env, false)
}

pub fn status(config: &Config, env: &HostEnv) -> Result<()> {
    let Some(dotfiles) = &config.dotfiles else {
        anyhow::bail!(
            "no [dotfiles] configuration for '{}'",
            config.container.name
        );
    };
    let path = paths(config, env)?.context("dotfiles paths are unavailable")?;
    let stamp = config.container.home.join(".podbox-dotfiles.done");
    println!("Container: {}", config.container.name);
    println!("Source: {}", dotfiles.source);
    println!("Target: {}", path.guest_target);
    println!("Files present: {}", path.host_target.is_dir());
    println!("Provisioned: {}", stamp.is_file());
    if path.host_target.join(".git").is_dir() {
        let output = Command::new("git")
            .args(["-C"])
            .arg(&path.host_target)
            .args(["status", "--short", "--branch"])
            .output()
            .context("checking dotfiles repository status")?;
        if output.status.success() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
    }
    Ok(())
}
