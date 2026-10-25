use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;

use podbox::config::Config;
use podbox::xdg::ResolvedXdgDirs;

/// Convert a host path to a container path (or vice versa).
pub fn run_translate_path(
    _config: &Config,
    xdg: &ResolvedXdgDirs,
    to_container: bool,
    to_host: bool,
    path_str: &str,
) -> Result<()> {
    let write_path = |path: &str| -> Result<()> {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(path.as_bytes())?;
        stdout.write_all(b"\n")?;
        Ok(())
    };
    let host_home = dirs::home_dir().unwrap_or_default();
    let home_in_container = host_home
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| *name != "root")
        .map_or_else(|| "/root".to_string(), |name| format!("/home/{name}"));
    let host_path = if Path::new(path_str).is_relative() {
        std::env::current_dir().map_or_else(|_| PathBuf::from(path_str), |d| d.join(path_str))
    } else {
        PathBuf::from(path_str)
    };

    if to_container {
        let host_to_container: Vec<(&str, &PathBuf)> = vec![
            ("Documents", &xdg.documents),
            ("Downloads", &xdg.downloads),
            ("Pictures", &xdg.pictures),
            ("Music", &xdg.music),
            ("Videos", &xdg.videos),
            ("Desktop", &xdg.desktop),
            ("Projects", &xdg.projects),
        ]
        .into_iter()
        .filter_map(|(name, opt)| opt.as_ref().map(|r| (name, &r.path)))
        .collect();

        for (dir_name, host_dir) in &host_to_container {
            if let Ok(relative) = host_path.strip_prefix(host_dir) {
                let container_path =
                    format!("{}/{}/{}", home_in_container, dir_name, relative.display());
                write_path(&container_path)?;
                return Ok(());
            }
        }

        if let Ok(relative) = host_path.strip_prefix(&host_home) {
            let container_path = format!("{}/{}", home_in_container, relative.display());
            write_path(&container_path)?;
            return Ok(());
        }

        write_path(path_str)?;
    }

    if to_host {
        for (dir_name, host_dir) in [
            ("Documents", &xdg.documents),
            ("Downloads", &xdg.downloads),
            ("Pictures", &xdg.pictures),
            ("Music", &xdg.music),
            ("Videos", &xdg.videos),
            ("Desktop", &xdg.desktop),
        ] {
            if let Some(resolved) = host_dir {
                let container_prefix = format!("{home_in_container}/{dir_name}/");
                if path_str.starts_with(&container_prefix) {
                    let relative = path_str.strip_prefix(&container_prefix).unwrap_or("");
                    let host_path = resolved.path.join(relative);
                    println!("{}", host_path.display());
                    return Ok(());
                }
            }
        }

        println!("{path_str}");
    }

    Ok(())
}
