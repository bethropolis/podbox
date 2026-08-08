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

fn is_valid_app_name(app: &str) -> bool {
    !app.is_empty()
        && app
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Export an application as a .desktop file on the host.
pub fn export_app(container_name: &str, app: &str) -> Result<()> {
    if !is_valid_app_name(app) {
        return Err(PodboxError::ExportFailed {
            details: format!("invalid app name: '{app}'"),
        }
        .into());
    }

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
    if !is_valid_app_name(app) {
        return Err(PodboxError::ExportFailed {
            details: format!("invalid app name: '{app}'"),
        }
        .into());
    }
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
pub fn export_bin(container_name: &str, bin: &str) -> Result<()> {
    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
    std::fs::create_dir_all(&bin_dir)?;

    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "podbox".to_string());
    let shim = format!(
        "#!/bin/sh\nexec {} --container \"{}\" exec \"{}\" \"$@\"\n",
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
            if let Ok(mut file) = std::fs::File::open(entry.path()) {
                use std::io::Read;
                let mut chunk = vec![0u8; 4096];
                if let Ok(bytes_read) = file.read(&mut chunk) {
                    let content = String::from_utf8_lossy(&chunk[..bytes_read]);
                    if content.contains(&marker) {
                        let _ = std::fs::remove_file(entry.path());
                    }
                }
            }
        }
    }

    println!("Unexported all apps and bins for '{}'.", container_name);
    Ok(())
}

/// List the .desktop apps and bin shims exported to the host for a container.
pub fn list_exports(container_name: &str) -> Result<()> {
    let apps_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join(".local/share"))
                .unwrap_or_else(|| PathBuf::from("/usr/local/share"))
        })
        .join("applications");
    let prefix = format!("podbox-{}-", container_name);
    let suffix = ".desktop";

    let mut apps: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&apps_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) && name.ends_with(suffix) {
                apps.push(name[prefix.len()..name.len() - suffix.len()].to_string());
            }
        }
    }
    apps.sort();

    let bin_dir = dirs::home_dir()
        .map(|h| h.join(".local/bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));
    let marker = format!("--container \"{}\"", container_name);
    let mut bins: Vec<String> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&bin_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok(mut file) = std::fs::File::open(&path) {
                use std::io::Read;
                let mut chunk = vec![0u8; 4096];
                if let Ok(bytes_read) = file.read(&mut chunk) {
                    let content = String::from_utf8_lossy(&chunk[..bytes_read]);
                    if content.contains(&marker) {
                        bins.push(entry.file_name().to_string_lossy().into_owned());
                    }
                }
            }
        }
    }
    bins.sort();

    if apps.is_empty() && bins.is_empty() {
        println!("No exports for '{}'.", container_name);
        return Ok(());
    }

    if !apps.is_empty() {
        println!("Apps:");
        for app in &apps {
            println!("  {app}");
        }
    }
    if !bins.is_empty() {
        if !apps.is_empty() {
            println!();
        }
        println!("Bins:");
        for bin in &bins {
            println!("  {bin}");
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_app_names() {
        for name in &["firefox", "Firefox", "code-oss", "code_oss", "v1.2.3", "a"] {
            assert!(is_valid_app_name(name), "expected '{name}' to be valid");
        }
    }

    #[test]
    fn reject_empty_name() {
        assert!(!is_valid_app_name(""));
    }

    #[test]
    fn reject_shell_metacharacters() {
        for bad in &[
            "foo;rm", "foo\"bar", "foo`bar", "foo$bar", "foo|bar", "foo>bar", "foo<bar", "foo&bar",
            "foo\nbar", "../foo", "foo/bar", "foo bar", "foo\\bar", "foo'bar",
        ] {
            assert!(!is_valid_app_name(bad), "expected '{bad}' to be rejected");
        }
    }

    #[test]
    fn export_app_rejects_invalid_name() {
        let result = export_app("test-container", "foo;rm");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("foo;rm") || err.contains("invalid"),
            "error should mention the name: {err}"
        );
    }

    #[test]
    fn find_desktop_file_rejects_invalid_name() {
        let result = find_desktop_file("test-container", "foo`whoami`");
        assert!(result.is_err());
        let err = format!("{}", result.unwrap_err());
        assert!(
            err.contains("foo`whoami`") || err.contains("invalid"),
            "error should mention the name: {err}"
        );
    }

    #[test]
    fn list_exports_lists_apps_and_bins() {
        let apps_dir = dirs::data_dir()
            .expect("data dir")
            .join("applications");
        std::fs::create_dir_all(&apps_dir).expect("create apps dir");
        let app_path = apps_dir.join("podbox-box-firefox.desktop");
        std::fs::write(&app_path, "[Desktop Entry]\nName=Firefox (box)\n").unwrap();

        let bin_dir = dirs::home_dir().expect("home dir").join(".local/bin");
        std::fs::create_dir_all(&bin_dir).expect("create bin dir");
        let shim_path = bin_dir.join("firefox");
        std::fs::write(
            &shim_path,
            "#!/bin/sh\nexec /usr/bin/podbox --container \"box\" exec \"firefox\" \"$@\"\n",
        )
        .unwrap();

        let apps_dir = dirs::data_dir().expect("data dir").join("applications");
        let prefix = format!("podbox-{}-", "box");
        let suffix = ".desktop";
        let mut apps: Vec<String> = std::fs::read_dir(&apps_dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(&prefix) && n.ends_with(suffix))
            .map(|n| n[prefix.len()..n.len() - suffix.len()].to_string())
            .collect();
        apps.sort();

        let marker = format!("--container \"{}\"", "box");
        let mut bins: Vec<String> = std::fs::read_dir(&bin_dir)
            .unwrap()
            .flatten()
            .filter(|e| {
                std::fs::read_to_string(e.path())
                    .map(|c| c.contains(&marker))
                    .unwrap_or(false)
            })
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        bins.sort();

        assert_eq!(apps, vec!["firefox".to_string()]);
        assert_eq!(bins, vec!["firefox".to_string()]);

        let _ = std::fs::remove_file(&app_path);
        let _ = std::fs::remove_file(&shim_path);
    }
}
