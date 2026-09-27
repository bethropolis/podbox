use anyhow::{Context, Result};

const PREFIX: &str = "podbox-cache-";

fn volume_names() -> Result<Vec<String>> {
    let output = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&[
            "volume",
            "ls",
            "--filter",
            "name=podbox-cache-",
            "--format",
            "{{.Name}}",
        ]),
    )?;
    if !output.status.success() {
        anyhow::bail!("podman volume ls failed");
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|name| name.starts_with(PREFIX))
        .map(str::to_string)
        .collect())
}

fn volume_bytes(path: &str) -> u64 {
    fn walk(path: &std::path::Path) -> u64 {
        let Ok(meta) = std::fs::symlink_metadata(path) else {
            return 0;
        };
        if meta.is_file() {
            return meta.len();
        }
        if !meta.is_dir() {
            return 0;
        }
        std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| walk(&entry.path()))
            .sum()
    }
    walk(std::path::Path::new(path))
}

fn human_size(bytes: u64) -> String {
    let mut size = bytes as f64;
    let mut unit = "B";
    for next in ["KiB", "MiB", "GiB", "TiB"] {
        if size < 1024.0 {
            break;
        }
        size /= 1024.0;
        unit = next;
    }
    format!("{size:.1} {unit}")
}

fn attachments(volume: &str) -> Vec<String> {
    let args = podbox::process::args(&[
        "ps",
        "-a",
        "--filter",
        &format!("volume={volume}"),
        "--format",
        "{{.Names}}",
    ]);
    podbox::process::run_piped("podman", &args)
        .ok()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub fn run(command: &podbox::cli::CacheCommand, dry_run: bool) -> Result<()> {
    match command {
        podbox::cli::CacheCommand::List => {
            let names = volume_names()?;
            if names.is_empty() {
                println!("No shared cache volumes have been provisioned.");
                return Ok(());
            }
            println!("{:<26} {:>12}  ATTACHED CONTAINERS", "CACHE", "SIZE");
            for volume in names {
                let args = podbox::process::args(&[
                    "volume",
                    "inspect",
                    "--format",
                    "{{.Mountpoint}}",
                    &volume,
                ]);
                let path = podbox::process::run_piped("podman", &args)
                    .ok()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                    .unwrap_or_default();
                let size = human_size(volume_bytes(&path));
                let attached = attachments(&volume);
                println!(
                    "{:<26} {:>12}  {}",
                    volume.trim_start_matches(PREFIX),
                    size,
                    if attached.is_empty() {
                        "—".into()
                    } else {
                        attached.join(", ")
                    }
                );
            }
        }
        podbox::cli::CacheCommand::Prune { name } => {
            let volumes = if let Some(name) = name {
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                {
                    anyhow::bail!("invalid cache name '{name}'");
                }
                let volume = format!("{PREFIX}{name}");
                if !volume_names()?.contains(&volume) {
                    anyhow::bail!("shared cache '{name}' does not exist");
                }
                vec![volume]
            } else {
                let volumes = volume_names()?;
                if volumes.is_empty() {
                    println!("No shared cache volumes to prune.");
                    return Ok(());
                }
                if !dry_run {
                    if !podbox::codegen::distros::is_tty() {
                        anyhow::bail!(
                            "pruning all shared caches requires confirmation in a terminal; specify a cache name to prune one"
                        );
                    }
                    let yes = dialoguer::Confirm::new()
                        .with_prompt(format!(
                            "Remove all {} shared cache volume(s)?",
                            volumes.len()
                        ))
                        .default(false)
                        .interact_opt()?
                        .unwrap_or(false);
                    if !yes {
                        println!("Cancelled.");
                        return Ok(());
                    }
                }
                volumes
            };
            for volume in volumes {
                if dry_run {
                    println!("podman volume rm {volume}");
                    continue;
                }
                let args = podbox::process::args(&["volume", "rm", &volume]);
                let output = podbox::process::run_piped("podman", &args)
                    .with_context(|| format!("removing cache volume '{volume}'"))?;
                if !output.status.success() {
                    anyhow::bail!(
                        "could not remove cache volume '{volume}'; stop or detach its containers first"
                    );
                }
                println!("Removed {volume}.");
            }
        }
    }
    Ok(())
}
