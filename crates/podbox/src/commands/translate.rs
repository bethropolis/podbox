use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;

use podbox::config::Config;
use podbox::xdg::ResolvedXdgDirs;

/// The XDG directories podbox shares with the container, in match order.
const SHARED_DIRS: [(
    &str,
    fn(&ResolvedXdgDirs) -> Option<&podbox::xdg::ResolvedXdgDir>,
); 7] = [
    ("Documents", |xdg| xdg.documents.as_ref()),
    ("Downloads", |xdg| xdg.downloads.as_ref()),
    ("Pictures", |xdg| xdg.pictures.as_ref()),
    ("Music", |xdg| xdg.music.as_ref()),
    ("Videos", |xdg| xdg.videos.as_ref()),
    ("Desktop", |xdg| xdg.desktop.as_ref()),
    ("Projects", |xdg| xdg.projects.as_ref()),
];

/// Convert a host path to a container path (or vice versa).
pub fn run_translate_path(
    config: &Config,
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
    let home_in_container = container_home_name(&host_home);
    let host_path = if Path::new(path_str).is_relative() {
        std::env::current_dir().map_or_else(|_| PathBuf::from(path_str), |d| d.join(path_str))
    } else {
        PathBuf::from(path_str)
    };

    if to_container {
        let translated = to_container_path(
            &host_path,
            xdg,
            &config.container.home,
            &host_home,
            &home_in_container,
        )
        .unwrap_or_else(|| path_str.to_owned());
        write_path(&translated)?;
    }

    if to_host {
        let translated = to_host_path(path_str, xdg, &config.container.home, &home_in_container)
            .unwrap_or_else(|| path_str.to_owned());
        write_path(&translated)?;
    }

    Ok(())
}

/// The container-side home path for the host user.
fn container_home_name(host_home: &Path) -> String {
    host_home
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| *name != "root")
        .map_or_else(|| "/root".to_string(), |name| format!("/home/{name}"))
}

/// Host path → container path. Shared XDG directories win; the container's
/// home is mapped to `/home/<user>` rather than left under the host's
/// `~/containers/<name>/` prefix.
fn to_container_path(
    host_path: &Path,
    xdg: &ResolvedXdgDirs,
    container_home: &Path,
    host_home: &Path,
    home_in_container: &str,
) -> Option<String> {
    for (dir_name, dir) in SHARED_DIRS {
        if let Some(resolved) = dir(xdg)
            && let Ok(relative) = host_path.strip_prefix(&resolved.path)
        {
            return Some(format!(
                "{home_in_container}/{dir_name}/{}",
                relative.display()
            ));
        }
    }
    if let Ok(relative) = host_path.strip_prefix(container_home) {
        return Some(format!("{home_in_container}/{}", relative.display()));
    }
    if let Ok(relative) = host_path.strip_prefix(host_home) {
        return Some(format!("{home_in_container}/{}", relative.display()));
    }
    None
}

/// Container path → host path, the inverse mapping (including `Projects`).
fn to_host_path(
    path_str: &str,
    xdg: &ResolvedXdgDirs,
    container_home: &Path,
    home_in_container: &str,
) -> Option<String> {
    for (dir_name, dir) in SHARED_DIRS {
        if let Some(resolved) = dir(xdg) {
            let prefix = format!("{home_in_container}/{dir_name}/");
            if let Some(relative) = path_str.strip_prefix(&prefix) {
                return Some(resolved.path.join(relative).display().to_string());
            }
        }
    }
    let prefix = format!("{home_in_container}/");
    if let Some(relative) = path_str.strip_prefix(&prefix) {
        return Some(container_home.join(relative).display().to_string());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use podbox::xdg::ResolvedXdgDir;

    fn xdg() -> ResolvedXdgDirs {
        let mut dirs = ResolvedXdgDirs::mock_preview();
        let entry = |path: &str| {
            Some(ResolvedXdgDir {
                path: PathBuf::from(path),
                read_write: true,
            })
        };
        dirs.documents = entry("/host/Documents");
        dirs.projects = entry("/host/Projects");
        dirs
    }

    #[test]
    fn maps_container_home_to_the_container_home() {
        let translated = to_container_path(
            Path::new("/home/user/containers/dev/notes.txt"),
            &xdg(),
            Path::new("/home/user/containers/dev"),
            Path::new("/home/user"),
            "/home/user",
        );
        assert_eq!(translated.as_deref(), Some("/home/user/notes.txt"));
    }

    #[test]
    fn maps_projects_both_ways() {
        let translated = to_container_path(
            Path::new("/host/Projects/podbox"),
            &xdg(),
            Path::new("/home/user/containers/dev"),
            Path::new("/home/user"),
            "/home/user",
        );
        assert_eq!(translated.as_deref(), Some("/home/user/Projects/podbox"));

        let back = to_host_path(
            "/home/user/Projects/podbox",
            &xdg(),
            Path::new("/home/user/containers/dev"),
            "/home/user",
        );
        assert_eq!(back.as_deref(), Some("/host/Projects/podbox"));
    }

    #[test]
    fn maps_container_home_back_to_the_host_path() {
        let back = to_host_path(
            "/home/user/src/main.rs",
            &xdg(),
            Path::new("/home/user/containers/dev"),
            "/home/user",
        );
        assert_eq!(
            back.as_deref(),
            Some("/home/user/containers/dev/src/main.rs")
        );
    }
}
