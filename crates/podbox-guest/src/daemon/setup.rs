use std::collections::HashSet;
use std::os::unix::net::UnixStream;

pub(super) fn install_interceptors(
    accepted: &HashSet<String>,
    bin_dir: &std::path::Path,
    host_exec_shims: &[String],
) -> std::io::Result<()> {
    let self_path = std::env::current_exe()?;

    // Everything this daemon should expose. Links for anything not in this
    // set are stale and removed below, so revoked capabilities/shims stop
    // resolving.
    let mut desired: HashSet<String> = HashSet::new();
    for (name, cap) in crate::protocol::INTERCEPTORS {
        if accepted.contains(*cap) {
            desired.insert((*name).to_string());
        }
    }
    if accepted.contains(crate::protocol::CAP_HOST_EXEC) {
        for shim in host_exec_shims {
            if shim == "podbox-guest"
                || shim == "podmgr-guest"
                || shim.contains('/')
                || shim.contains("..")
                || shim.contains('\0')
                || shim.is_empty()
            {
                tracing::warn!("skipping invalid host-exec shim name: {shim:?}");
                continue;
            }
            // Guard against overwriting core interceptors (plus the legacy
            // `podmgr-*` alias for the clipboard shim).
            let is_core = crate::protocol::INTERCEPTORS
                .iter()
                .any(|(name, _)| *name == shim)
                || shim == "podmgr-clipboard";
            if is_core {
                continue;
            }
            desired.insert(shim.clone());
        }
    }

    for name in &desired {
        let link = bin_dir.join(name);
        let _ = std::fs::remove_file(&link);
        std::os::unix::fs::symlink(&self_path, &link)?;
    }
    prune_stale_links(bin_dir, &self_path, &desired);

    for name in &desired {
        if crate::protocol::INTERCEPTORS.iter().any(|(n, _)| n == name) {
            tracing::debug!("installed interceptor: /run/podbox/bin/{name}");
        } else {
            tracing::info!("installed host-exec shim: /run/podbox/bin/{name}");
        }
    }
    Ok(())
}

/// Remove symlinks in `bin_dir` that point at this binary but are no longer
/// in `desired` (a capability or host-exec shim was revoked on reconnect).
fn prune_stale_links(
    bin_dir: &std::path::Path,
    self_path: &std::path::Path,
    desired: &HashSet<String>,
) {
    let Ok(entries) = std::fs::read_dir(bin_dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if desired.contains(&name) {
            continue;
        }
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.file_type().is_symlink() {
            continue;
        }
        let Ok(target) = std::fs::read_link(&path) else {
            continue;
        };
        if target == self_path {
            let _ = std::fs::remove_file(&path);
        }
    }
}

pub(super) fn check_version_drift(
    accepted: &HashSet<String>,
    _host_stream: &mut UnixStream,
    container_name: &str,
    live_host_version: &str,
) {
    let Ok(baked_host_version) =
        std::env::var("PODBOX_HOST_VERSION").or_else(|_| std::env::var("PODMGR_HOST_VERSION"))
    else {
        return;
    };

    // Hosts older than the `host_version` handshake field send an empty
    // value; that is not drift.
    if live_host_version.is_empty() {
        return;
    }

    if baked_host_version == live_host_version {
        return;
    }

    let summary = "podbox: container image is outdated";
    let body = format!(
        "Container '{container_name}' was built with podbox {baked_host_version} but host is now {live_host_version}. Run `podbox build --rebuild`."
    );

    if accepted.contains(crate::protocol::CAP_NOTIFY) {
        let msg = crate::protocol::GuestMessage::Notify {
            summary: summary.to_string(),
            body,
            urgency: "normal".to_string(),
            actions: vec![],
            app_name: "podbox".to_string(),
        };
        let _ = crate::socket::connect_and_send_oneshot(&msg, crate::protocol::CAP_NOTIFY);
    } else {
        tracing::warn!(
            "image is outdated (built with {baked_host_version}, host is now {live_host_version}). Run `podbox build --rebuild`."
        );
    }
}

pub(super) fn write_path_injection(bin_dir: &std::path::Path) -> std::io::Result<()> {
    let conf_dir = std::path::PathBuf::from("/etc/profile.d");
    std::fs::create_dir_all(&conf_dir)?;
    let conf_path = conf_dir.join("podbox.sh");
    let content = format!("export PATH={}:$PATH\n", bin_dir.to_string_lossy());
    std::fs::write(conf_path, content)?;

    let fish_dir = std::path::PathBuf::from("/etc/fish/conf.d");
    if fish_dir.is_dir() || std::fs::create_dir_all(&fish_dir).is_ok() {
        let fish_path = fish_dir.join("podbox.fish");
        let fish_content = format!("fish_add_path -m {}\n", bin_dir.to_string_lossy());
        let _ = std::fs::write(fish_path, fish_content);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn prune_stale_links_removes_only_links_to_this_binary() {
        let dir = std::env::temp_dir().join(format!("podbox-prune-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let self_path = std::path::Path::new("/tmp/podbox-self-target");
        std::os::unix::fs::symlink(self_path, dir.join("stale")).unwrap();
        std::os::unix::fs::symlink("/bin/false", dir.join("other")).unwrap();
        std::os::unix::fs::symlink("/bin/true", dir.join("keep")).unwrap();
        let desired = HashSet::from(["keep".to_string()]);

        prune_stale_links(&dir, self_path, &desired);

        assert!(dir.join("keep").exists(), "desired link was removed");
        assert!(!dir.join("stale").exists(), "stale link survived");
        assert!(dir.join("other").exists(), "unrelated link was removed");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
