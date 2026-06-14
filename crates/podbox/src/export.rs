use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use anyhow::Result;

use crate::error::PodboxError;

/// Standard XDG application directories searched inside the container,
/// in priority order.  Many apps install to `~/.local/share/applications/`
/// (per-user), `/usr/local/share/applications/`, or `/opt/<app>/share/applications/`.
const DESKTOP_SEARCH_PATHS: &[&str] = &[
    "/usr/share/applications",
    "/usr/local/share/applications",
    "/usr/share/applications/kde",
    "/usr/share/applications/gnome",
    "/opt",
];

/// Export an application as a .desktop file on the host.
pub fn export_app(container_name: &str, app: &str) -> Result<()> {
    // 1. Locate .desktop file in container, searching XDG directories.
    let (container_path, desktop_content) = find_desktop_file(container_name, app)?;

    // 2. Rewrite Name= and Exec= lines
    let rewritten = rewrite_desktop_file(&desktop_content, container_name, app);

    // 3. Write host .desktop file
    let apps_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join("applications");
    std::fs::create_dir_all(&apps_dir)?;

    let host_path = apps_dir.join(format!("podbox-{}-{}.desktop", container_name, app));
    std::fs::write(&host_path, rewritten)?;

    // 4. Try to extract icon
    if let Some(icon_name) = extract_icon_name(&desktop_content) {
        if let Err(e) = copy_icon_from_container(container_name, &icon_name, container_name) {
            eprintln!("Warning: failed to copy icon '{}': {}", icon_name, e);
        }
    }

    // 5. Update desktop database
    if let Err(e) = std::process::Command::new("update-desktop-database")
        .arg(&apps_dir)
        .output()
        .map(|_| ())
    {
        eprintln!("Warning: update-desktop-database failed: {}", e);
    }

    println!(
        "Exported app '{}'.desktop (from {}) -> {}",
        app,
        container_path,
        host_path.display()
    );
    Ok(())
}

/// Find a `.desktop` file in the container by searching XDG dirs,
/// falling back to user-installed locations.
fn find_desktop_file(container_name: &str, app: &str) -> Result<(String, String)> {
    let filename = format!("{}.desktop", app);

    // First: search well-known system locations.
    for dir in DESKTOP_SEARCH_PATHS {
        if *dir == "/opt" {
            // /opt is a prefix — search one level deep for share/applications.
            continue;
        }
        let candidate = format!("{}/{}", dir, filename);
        if let Some(content) = try_cat(container_name, &candidate)? {
            return Ok((candidate, content));
        }
    }

    // Second: per-user installs.
    let user_dirs = ["/root/.local/share/applications", "/home"];
    for dir in user_dirs {
        if let Some(content) = try_cat(container_name, &format!("{}/{}", dir, filename))? {
            return Ok((format!("{}/{}", dir, filename), content));
        }
    }

    // Third: /opt — search for any /opt/*/share/applications/<app>.desktop.
    if let Some((path, content)) = find_desktop_in_opt(container_name, app)? {
        return Ok((path, content));
    }

    Err(PodboxError::ExportFailed {
        details: format!(
            "app {} not found in container (searched: {})",
            app,
            DESKTOP_SEARCH_PATHS.join(", ")
        ),
    }
    .into())
}

/// `podman exec <container> cat <path>` — returns `Some(content)` if the
/// file exists, `None` if `cat` reports missing, error on other failures.
fn try_cat(container_name: &str, path: &str) -> Result<Option<String>> {
    let args: Vec<OsString> = vec![
        "exec".into(),
        container_name.into(),
        "cat".into(),
        path.into(),
    ];
    let output = crate::process::run_piped("podman", &args)?;
    if output.status.success() {
        Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned()))
    } else {
        Ok(None)
    }
}

/// Search /opt/*/share/applications/ for a matching .desktop file.
fn find_desktop_in_opt(container_name: &str, app: &str) -> Result<Option<(String, String)>> {
    let args: Vec<OsString> = vec![
        "exec".into(),
        container_name.into(),
        "sh".into(),
        "-c".into(),
        format!(
            "for d in /opt/*/share/applications; do \
               [ -f \"$d/{app}.desktop\" ] && echo \"$d/{app}.desktop\"; \
             done"
        )
        .into(),
    ];
    let output = crate::process::run_piped("podman", &args)?;
    if !output.status.success() {
        return Ok(None);
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(content) = try_cat(container_name, line)? {
            return Ok(Some((line.to_string(), content)));
        }
    }
    Ok(None)
}

/// Export a binary shim to ~/.local/bin.
pub fn export_bin(container_name: &str, bin: &str) -> Result<()> {
    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
    std::fs::create_dir_all(&bin_dir)?;

    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "podbox".to_string());
    let shim = format!(
        "#!/bin/sh\nexec {} --container \"{}\" run \"{}\" \"$@\"\n",
        exe,
        container_name.replace('"', "\\\""),
        bin.replace('"', "\\\"")
    );

    let shim_path = bin_dir.join(bin);
    std::fs::write(&shim_path, shim)?;
    #[allow(clippy::print_literal)]
    {
        let _ = std::fs::set_permissions(&shim_path, std::fs::Permissions::from_mode(0o755));
    }

    println!("Exported bin shim '{}' -> {}", bin, shim_path.display());
    Ok(())
}

/// Remove all exports for a container.
pub fn unexport_all(container_name: &str) -> Result<()> {
    let apps_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join("applications");
    let prefix = format!("podbox-{}", container_name);

    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with(&prefix) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    let icons_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join(format!("icons/podbox/{}", container_name));
    // Also remove legacy icons path
    let old_icons_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join(format!("icons/podmgr/{}", container_name));
    let _ = std::fs::remove_dir_all(&icons_dir);
    if old_icons_dir.exists() {
        let _ = std::fs::remove_dir_all(&old_icons_dir);
    }

    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));

    // Remove shims that reference this container
    let marker = format!("--container \"{}\"", container_name);
    if let Ok(entries) = std::fs::read_dir(&bin_dir) {
        for entry in entries.flatten() {
            if let Ok(content) = std::fs::read_to_string(entry.path()) {
                if content.contains(&marker) {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }

    println!("Unexported all apps and bins for '{}'.", container_name);
    Ok(())
}

fn rewrite_desktop_file(content: &str, container_name: &str, _app: &str) -> String {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "podbox".to_string());
    let suffix = format!("({})", container_name);
    content
        .lines()
        .map(|line| {
            if let Some(original) = line.strip_prefix("Exec=") {
                format!(
                    "                    Exec={} --container \"{}\" exec -- {}",
                    exe,
                    container_name.replace('"', "\\\""),
                    original
                )
            } else if let Some((key, val)) = line.split_once('=') {
                if (key == "Name" || key.starts_with("Name[")) && !val.contains(&suffix) {
                    format!("{}={} ({})", key, val, container_name)
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_icon_name(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.strip_prefix("Icon=").map(|s| s.to_string()))
}

fn copy_icon_from_container(container_name: &str, icon_name: &str, _profile: &str) -> Result<()> {
    // Sanitize icon name: refuse path separators to prevent traversal
    if icon_name.contains('/') || icon_name.contains("..") {
        return Err(anyhow::anyhow!(
            "icon name contains path separators, refusing: {}",
            icon_name
        ));
    }

    let icons_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join(format!("icons/podbox/{}", container_name));
    std::fs::create_dir_all(&icons_dir)?;

    let icon_paths: Vec<String> = vec![
        format!("/usr/share/icons/hicolor/48x48/apps/{}.png", icon_name),
        format!("/usr/share/icons/hicolor/scalable/apps/{}.svg", icon_name),
        format!("/usr/share/icons/hicolor/64x64/apps/{}.png", icon_name),
        format!("/usr/share/icons/hicolor/128x128/apps/{}.png", icon_name),
        format!("/usr/share/icons/hicolor/256x256/apps/{}.png", icon_name),
        format!("/usr/share/icons/hicolor/48x48/apps/{}.svg", icon_name),
    ];

    for path in &icon_paths {
        let ext = std::path::Path::new(path)
            .extension()
            .map(|e| e.to_string_lossy())
            .unwrap_or_default();
        let args: Vec<OsString> = vec![
            "exec".into(),
            container_name.into(),
            "cat".into(),
            path.into(),
        ];
        let output = crate::process::run_piped("podman", &args)?;
        if output.status.success() {
            let dest = icons_dir.join(format!("{}.{}", icon_name, ext));
            std::fs::write(dest, &output.stdout)?;
            break;
        }
    }

    Ok(())
}
