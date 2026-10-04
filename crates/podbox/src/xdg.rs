use std::path::PathBuf;

#[cfg(feature = "cli")]
use std::process::Command;

#[cfg(feature = "cli")]
use anyhow::Result;

#[cfg(feature = "cli")]
use crate::config::{XdgDirConfig, XdgDirValue};

pub struct ResolvedXdgDir {
    pub path: PathBuf,
    pub read_write: bool,
}

pub struct ResolvedXdgDirs {
    pub documents: Option<ResolvedXdgDir>,
    pub downloads: Option<ResolvedXdgDir>,
    pub pictures: Option<ResolvedXdgDir>,
    pub music: Option<ResolvedXdgDir>,
    pub videos: Option<ResolvedXdgDir>,
    pub desktop: Option<ResolvedXdgDir>,
    pub projects: Option<ResolvedXdgDir>,
}

impl ResolvedXdgDirs {
    /// Empty stand-in for previews (Studio/wasm): no host directories are
    /// shared, so generated units show only the config-driven mounts.
    pub fn mock_preview() -> Self {
        Self {
            documents: None,
            downloads: None,
            pictures: None,
            music: None,
            videos: None,
            desktop: None,
            projects: None,
        }
    }
}

/// Resolve XDG user directories from the host.
///
/// For each enabled dir, tries in order:
/// 1. Parse `~/.config/user-dirs.dirs`
/// 2. Call `xdg-user-dir <NAME>`
/// 3. Fall back to `~/DirName`
/// 4. If path doesn't exist on disk: `None`
///
/// Host-only (`cli` feature): shells out to `xdg-user-dir` and probes the
/// filesystem, neither of which exists on wasm.
#[cfg(feature = "cli")]
pub fn resolve(config: &XdgDirConfig) -> Result<ResolvedXdgDirs> {
    Ok(ResolvedXdgDirs {
        documents: resolve_dir(&config.documents, "DOCUMENTS", "Documents"),
        downloads: resolve_dir(&config.downloads, "DOWNLOADS", "Downloads"),
        pictures: resolve_dir(&config.pictures, "PICTURES", "Pictures"),
        music: resolve_dir(&config.music, "MUSIC", "Music"),
        videos: resolve_dir(&config.videos, "VIDEOS", "Videos"),
        desktop: resolve_dir(&config.desktop, "DESKTOP", "Desktop"),
        projects: resolve_dir(&config.projects, "PROJECTS", "Projects"),
    })
}

#[cfg(feature = "cli")]
fn resolve_dir(value: &XdgDirValue, xdg_name: &str, fallback_name: &str) -> Option<ResolvedXdgDir> {
    if !value.is_enabled() {
        return None;
    }
    let read_write = value.is_read_write();

    // Try xdg-user-dir
    if let Ok(output) = Command::new("xdg-user-dir").arg(xdg_name).output() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !path.is_empty() && path != home_dir_str() {
            let p = PathBuf::from(path);
            if p.exists() {
                return Some(ResolvedXdgDir {
                    path: p,
                    read_write,
                });
            }
        }
    }

    // Fallback to ~/DirName
    if let Some(home) = dirs::home_dir() {
        let p = home.join(fallback_name);
        if p.exists() {
            return Some(ResolvedXdgDir {
                path: p,
                read_write,
            });
        }
    }

    None
}

#[cfg(feature = "cli")]
fn home_dir_str() -> String {
    dirs::home_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default()
}
