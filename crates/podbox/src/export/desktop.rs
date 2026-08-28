//! `.desktop` export plumbing: locate a container app's desktop file,
//! rewrite Exec/Name, and copy its icon, when exporting to the host.
//!
//! Extracted verbatim from `export.rs`.

use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::Result;

use crate::error::PodboxError;

const DESKTOP_SEARCH_PATHS: &[&str] = &[
    "/usr/share/applications",
    "/usr/local/share/applications",
    "/usr/share/applications/kde",
    "/usr/share/applications/gnome",
    "/opt",
];

pub(crate) fn is_valid_app_name(app: &str) -> bool {
    !app.is_empty()
        && app
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

pub(crate) fn find_desktop_file(container_name: &str, app: &str) -> Result<(String, String)> {
    if !is_valid_app_name(app) {
        return Err(PodboxError::ExportFailed {
            details: format!("invalid app name: '{app}'"),
        }
        .into());
    }
    let filename = format!("{app}.desktop");

    // First: search well-known system locations.
    for dir in DESKTOP_SEARCH_PATHS {
        if *dir == "/opt" {
            // /opt is a prefix — search one level deep for share/applications.
            continue;
        }
        let candidate = format!("{dir}/{filename}");
        if let Some(content) = try_cat(container_name, &candidate)? {
            return Ok((candidate, content));
        }
    }

    // Second: per-user installs.
    let user_dirs = ["/root/.local/share/applications", "/home"];
    for dir in user_dirs {
        if let Some(content) = try_cat(container_name, &format!("{dir}/{filename}"))? {
            return Ok((format!("{dir}/{filename}"), content));
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
pub(crate) fn try_cat(container_name: &str, path: &str) -> Result<Option<String>> {
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
pub(crate) fn find_desktop_in_opt(
    container_name: &str,
    app: &str,
) -> Result<Option<(String, String)>> {
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
    for line in stdout.lines().take(500) {
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
pub(crate) fn rewrite_desktop_file(content: &str, container_name: &str, _app: &str) -> String {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "podbox".to_string());
    let suffix = format!("({container_name})");
    content
        .lines()
        .map(|line| {
            if let Some(original) = line.strip_prefix("Exec=") {
                format!(
                    "Exec={} --container \"{}\" exec -- {}",
                    exe,
                    container_name.replace('"', "\\\""),
                    original
                )
            } else if let Some((key, val)) = line.split_once('=') {
                if (key == "Name" || key.starts_with("Name[")) && !val.contains(&suffix) {
                    format!("{key}={val} ({container_name})")
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

pub(crate) fn extract_icon_name(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.strip_prefix("Icon=").map(|s| s.to_string()))
}

pub(crate) fn copy_icon_from_container(
    container_name: &str,
    icon_name: &str,
    _profile: &str,
) -> Result<()> {
    // Sanitize icon name: refuse path separators to prevent traversal
    if icon_name.contains('/') || icon_name.contains("..") {
        return Err(anyhow::anyhow!(
            "icon name contains path separators, refusing: {icon_name}"
        ));
    }

    let icons_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join(format!("icons/podbox/{container_name}"));
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
            let dest = icons_dir.join(format!("{icon_name}.{ext}"));
            std::fs::write(dest, &output.stdout)?;
            break;
        }
    }

    Ok(())
}
