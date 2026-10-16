//! Guest sandbox entrypoint: the single `run()` flow plus `setup_user`.
//! One cohesive concern (documented exemption, per the modularization guide 1/8).
use std::ffi::CString;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use nix::unistd::{Gid, Uid, initgroups, setgid, setuid};

/// Spawn a detached daemon process, then exec the user command.
///
/// At the start, if running as root and host user info is available,
/// a matching system user is created and privileges are dropped.
///
/// This process: spawns `podbox-guest --daemon` in the background (as
/// root, before dropping privileges — the daemon needs root to install
/// interceptors under /run/podbox/bin), then execs the user shell or
/// command (replacing this process), or loops with a sleep in background
/// mode (no TTY, no cmd). The spawned daemon survives the exec and is
/// reparented when this process eventually exits.
#[allow(clippy::similar_names)]
pub fn run(cmd: &[String]) -> ! {
    let host_user = std::env::var("HOST_USER").ok();
    let host_uid = std::env::var("HOST_UID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    let host_gid = std::env::var("HOST_GID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());

    // If running as root and host info is available, create the user and drop privileges.
    let drop = if let (Some(user), Some(uid), Some(gid)) = (&host_user, host_uid, host_gid) {
        let is_root = nix::unistd::getuid().is_root();
        if is_root {
            setup_user(user, uid, gid);
            Some((uid, gid, user.clone()))
        } else {
            None
        }
    } else {
        None
    };

    // Spawn the daemon detached: fresh fork+exec via std, null stdio.
    // Must happen before the privilege drop below — the daemon runs as
    // root by design (interceptor installation under /run/podbox/bin).
    let self_exe = std::env::current_exe().unwrap_or_else(|_| "/usr/local/bin/podbox-guest".into());
    if let Err(e) = Command::new(&self_exe)
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        eprintln!(
            "podbox-guest: failed to spawn daemon {}: {e}",
            self_exe.display()
        );
        std::process::exit(1);
    }

    if let Some((uid, gid)) = drop.as_ref().map(|(u, g, _)| (*u, *g)) {
        let user = drop.as_ref().map(|(_, _, u)| u.clone()).unwrap_or_default();
        if let Err(e) = drop_privileges(&user, uid, gid) {
            eprintln!("podbox-guest: refusing to run the user shell: {e}");
            std::process::exit(1);
        }
    }

    let is_tty = nix::unistd::isatty(std::io::stdin()).unwrap_or(false);

    // Environment for the exec'd shell/command. Applied via the spawned
    // process instead of process-global `env::set_var`, which is unsafe
    // and unnecessary here — nothing reads the environment after this
    // point in this process.
    let user_env: Option<(String, String, String)> = drop
        .as_ref()
        .map(|(_, _, user)| (format!("/home/{user}"), user.clone(), user.clone()));

    // Expose guest version for host doctor (file); env for shell is set per-Command below.
    let _ = std::fs::create_dir_all("/run/podbox");
    let _ = std::fs::write("/run/podbox/guest-version", crate::VERSION);

    let exec_shell_cmd = |program: &str, args: &[String], arg0: Option<String>| -> ! {
        let mut c = Command::new(program);
        c.args(args);
        if let Some(a0) = arg0 {
            let _ = c.arg0(a0);
        }
        c.env("PODBOX_GUEST_VERSION", crate::VERSION);
        if let Some((ref home, ref user, ref logname)) = user_env {
            c.env("HOME", home)
                .env("USER", user)
                .env("LOGNAME", logname);
        }
        // `exec` replaces this process on success and never returns;
        // the return value is the error that prevented it.
        let e: std::io::Error = c.exec();
        eprintln!("podbox-guest: failed to execute command: {e}");
        std::process::exit(1);
    };

    if is_tty && !cmd.is_empty() {
        // Interactive: exec the requested command
        exec_shell_cmd(&cmd[0], &cmd[1..], None);
    } else if is_tty {
        // Interactive with no explicit CMD: start a login shell. Prefer the
        // shell recorded in passwd (chosen during user setup, after any
        // reconciliation) over the inherited `$SHELL`, which may point at
        // a host path that does not exist in the container.
        let shell = drop
            .as_ref()
            .and_then(|(_, _, user)| passwd_shell(user))
            .or_else(|| std::env::var("SHELL").ok())
            .unwrap_or_else(|| "/bin/bash".into());
        let arg0 = format!("-{shell}");
        exec_shell_cmd(&shell, &[], Some(arg0));
    } else {
        // Background (e.g. systemd): keep PID 1 alive
        loop {
            std::thread::sleep(std::time::Duration::from_hours(1));
        }
    }
}

/// Drop root privileges to the container user, failing closed.
///
/// Order matters: `initgroups` first (replaces root's supplementary groups
/// with the user's own, so device access granted by group works and nothing
/// of root's group set survives), then `setgid`, then `setuid`. Any failure
/// aborts the entrypoint instead of running the user's shell as root, and
/// the trailing check catches a partial drop.
fn drop_privileges(user: &str, uid: u32, gid: u32) -> Result<(), String> {
    let name = CString::new(user).map_err(|e| format!("invalid username {user:?}: {e}"))?;
    initgroups(&name, Gid::from_raw(gid)).map_err(|e| format!("initgroups({user}): {e}"))?;
    setgid(Gid::from_raw(gid)).map_err(|e| format!("setgid({gid}): {e}"))?;
    setuid(Uid::from_raw(uid)).map_err(|e| format!("setuid({uid}): {e}"))?;
    let have = (
        nix::unistd::getuid().as_raw(),
        nix::unistd::geteuid().as_raw(),
        nix::unistd::getgid().as_raw(),
    );
    if have != (uid, uid, gid) {
        return Err(format!(
            "drop incomplete: uid={} euid={} gid={}, want {uid}:{gid}",
            have.0, have.1, have.2
        ));
    }
    Ok(())
}

/// A parsed `/etc/passwd` line: the numeric ids plus the login shell.
struct PasswdIds {
    uid: u32,
    gid: u32,
    shell: String,
}

/// Look `user` up in passwd-format text. Returns `None` for an unknown
/// user, a malformed line, or an empty shell field.
fn passwd_entry(passwd_content: &str, user: &str) -> Option<PasswdIds> {
    let line = passwd_content
        .lines()
        .find(|l| l.split(':').next().is_some_and(|n| n == user))?;
    let mut fields = line.split(':');
    let uid: u32 = fields.nth(2)?.parse().ok()?;
    let gid: u32 = fields.next()?.parse().ok()?;
    let shell = fields.nth(2)?.to_string();
    if shell.is_empty() {
        return None;
    }
    Some(PasswdIds { uid, gid, shell })
}

/// Login shell recorded for `user` in `/etc/passwd`, if any.
fn passwd_shell(user: &str) -> Option<String> {
    let content = std::fs::read_to_string("/etc/passwd").ok()?;
    passwd_entry(&content, user).map(|e| e.shell)
}

/// First shell binary present on this image, for fresh accounts.
fn best_shell() -> &'static str {
    ["/bin/bash", "/bin/zsh", "/bin/fish", "/bin/sh"]
        .into_iter()
        .find(|s| std::path::Path::new(s).exists())
        .unwrap_or("/bin/sh")
}

/// Create a fresh account for the host user (useradd, or adduser on
/// minimal images). Best-effort like the rest of setup: failures are
/// reported by the tools themselves, and the fail-closed privilege drop
/// in `run` still refuses to continue as root.
fn create_user(user: &str, uid: u32, gid: u32, home_dir: &Path) {
    let shell = best_shell();
    let status = std::process::Command::new("useradd")
        .args([
            "-u",
            &uid.to_string(),
            "-g",
            &gid.to_string(),
            "-d",
            &home_dir.to_string_lossy(),
            "-s",
            shell,
            "-m",
            user,
        ])
        .status();
    if status.is_err() || !status.unwrap().success() {
        let _ = std::process::Command::new("adduser")
            .args([
                "-u",
                &uid.to_string(),
                "-D",
                "-h",
                &home_dir.to_string_lossy(),
                "-s",
                shell,
                user,
            ])
            .status();
    }
}

/// Create a system user matching the host UID/GID, set up passwordless sudo,
/// and ensure runtime directories are owned by the user.
///
/// When the home directory already exists (e.g. bind-mounted), its actual
/// owner UID from the filesystem is used instead of `HOST_UID`, because
/// UserNS=keep-id idmapped mounts shift UIDs.  The chown step is skipped
/// entirely for pre-existing directories to avoid corrupting host ownership.
///
/// All operations are idempotent — safe to call on every container start.
#[allow(clippy::too_many_lines)]
fn setup_user(user: &str, uid: u32, gid: u32) {
    let home_dir = Path::new("/home").join(user);

    // /run is tmpfs, so this marker is recreated on every container
    // boot, not on every exec into a running container.  Skipping
    // setup_user on subsequent execs removes a noticeable amount of
    // per-exec latency (useradd/groupadd/chmod on every shell entry).
    let marker = Path::new("/run/podbox/.setup_done");
    if marker.exists() {
        return;
    }

    // Advisory file locking via fd-lock (flock) to prevent concurrent
    // setup_user calls from corrupting /etc/passwd and /etc/group.
    // flock auto-releases if the process crashes — no stale locks.
    let lock_path = Path::new("/run/podbox/setup.lock");
    let _ = std::fs::create_dir_all(lock_path.parent().unwrap());
    let Ok(lock_file) = std::fs::File::create(lock_path) else {
        return;
    };
    let mut lock = fd_lock::RwLock::new(lock_file);
    let Ok(_guard) = lock.write() else { return };
    if marker.exists() {
        return;
    }

    let passwd = || std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let group_file = || std::fs::read_to_string("/etc/group").unwrap_or_default();

    // If a user with the target UID already exists under a different name,
    // remove it so we can create ours with the correct UID.
    if let Some(existing) = passwd()
        .lines()
        .find(|l| l.split(':').nth(2).is_some_and(|u| u == uid.to_string()))
        .and_then(|l| l.split(':').next())
        && existing != user
    {
        // Reuse the numeric id: drop the squatter so the useradd below can
        // claim the UID. Logged, since this deletes an image-shipped account.
        eprintln!("podbox-guest: uid {uid} belongs to {existing}; removing it to reuse the id");
        let _ = std::process::Command::new("userdel")
            .arg("-r")
            .arg(existing)
            .status();
    }

    // 1. Group
    let group_exists = group_file()
        .lines()
        .any(|l| l.starts_with(&format!("{user}:")));
    if !group_exists {
        let status = std::process::Command::new("groupadd")
            .args(["-g", &gid.to_string(), user])
            .status();
        if status.is_err() || !status.unwrap().success() {
            let _ = std::process::Command::new("addgroup")
                .args(["-g", &gid.to_string(), user])
                .status();
        }
    }

    // 2. User: create it, or reconcile an existing name onto our ids. An
    // image that already ships `user` with a different UID/GID would
    // otherwise leave `setuid` on an id with no passwd entry ("I have no
    // name!") and a broken `$HOME`.
    match passwd_entry(&passwd(), user) {
        None => create_user(user, uid, gid, &home_dir),
        Some(existing) if existing.uid != uid || existing.gid != gid => {
            let status = std::process::Command::new("usermod")
                .args([
                    "-u",
                    &uid.to_string(),
                    "-g",
                    &gid.to_string(),
                    "-d",
                    &home_dir.to_string_lossy(),
                    "-s",
                    best_shell(),
                    user,
                ])
                .status();
            if status.is_err() || !status.unwrap().success() {
                eprintln!(
                    "podbox-guest: cannot move {user} from {}:{} to {uid}:{gid}; \
                     the shell may show 'I have no name!'",
                    existing.uid, existing.gid
                );
            }
        }
        Some(_) => {}
    }

    // Make the home directory accessible (read+execute) to other users
    // and writable by the dynamic user.  755 (not 777) limits damage if
    // an untrusted process runs inside the container — it can read config
    // but cannot clobber the home directory.  chmod is safe because it
    // does NOT change ownership through the idmapped mount.
    let _ = std::process::Command::new("chmod")
        .args(["755", &home_dir.to_string_lossy()])
        .status();

    // 3. Supplementary groups — try common group names portably
    let supp_groups = ["wheel", "sudo", "video", "audio", "render", "input"];
    let existing_groups: Vec<&str> = supp_groups
        .iter()
        .filter(|g| {
            group_file()
                .lines()
                .any(|l| l.starts_with(&format!("{g}:")))
        })
        .copied()
        .collect();
    if !existing_groups.is_empty() {
        // Prefer usermod for transactional updates that keep /etc/gshadow in sync
        let mut usermod_ok = false;
        let groups_arg = existing_groups.join(",");
        if let Ok(status) = std::process::Command::new("usermod")
            .args(["-aG", &groups_arg, user])
            .status()
        {
            usermod_ok = status.success();
        }
        // Fall back to manual /etc/group patching only if usermod is unavailable or failed
        if !usermod_ok {
            let group_content = std::fs::read_to_string("/etc/group").unwrap_or_default();
            let mut modified = false;
            let patched: String = group_content
                .lines()
                .map(|line| {
                    let parts: Vec<&str> = line.splitn(4, ':').collect();
                    if parts.len() == 4 && existing_groups.contains(&parts[0]) {
                        let members = parts[3];
                        if members.split(',').any(|m| m == user) {
                            return line.to_string();
                        }
                        modified = true;
                        if members.is_empty() {
                            format!("{}:{}:{}:{}", parts[0], parts[1], parts[2], user)
                        } else {
                            format!(
                                "{}:{}:{}:{},{}",
                                parts[0], parts[1], parts[2], members, user
                            )
                        }
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            if modified {
                let _ = std::fs::write("/etc/group", patched + "\n");
            }
        }
    }

    // 4. Passwordless sudo, one file per user so removing a container
    // user cannot revoke another's access. The empty password also lets
    // `su` work on images without sudo.
    let sudoers_dir = Path::new("/etc/sudoers.d");
    if sudoers_dir.exists() {
        let _ = std::fs::remove_file(sudoers_dir.join("podbox"));
        let sudo_file = sudoers_dir.join(format!("podbox-{user}"));
        let _ = std::fs::write(&sudo_file, format!("{user} ALL=(ALL) NOPASSWD: ALL\n"));
        let _ = std::fs::set_permissions(&sudo_file, PermissionsExt::from_mode(0o440));
        let _ = std::process::Command::new("passwd")
            .args(["-d", user])
            .status();
    }

    // 5. Make runtime directory writable by all
    let runtime_dir =
        std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| format!("/run/user/{uid}"));
    let _ = std::process::Command::new("chmod")
        .args(["777", &runtime_dir])
        .status();

    // 6. Flatpak-info compatibility symlink (points to read-only host mount)
    let flatpak_info_symlink = Path::new(&runtime_dir).join("flatpak-info");
    let _ = std::fs::remove_file(&flatpak_info_symlink);
    let _ = std::os::unix::fs::symlink("//.flatpak-info", &flatpak_info_symlink);

    // 7. dconf subdirectory
    let dconf_dir = Path::new(&runtime_dir).join("dconf");
    let _ = std::fs::create_dir_all(&dconf_dir);
    let owner = format!("{uid}:{gid}");
    let _ = std::process::Command::new("chown")
        .args([&owner, dconf_dir.to_str().unwrap_or_default()])
        .status();
    let _ = std::process::Command::new("chmod")
        .args(["700", &dconf_dir.to_string_lossy()])
        .status();

    // 8. XDG data dirs must be owned by the container user.  Distro image
    //    skeletons or a root-run first shell (e.g. fish) leave
    //    ~/.local/share/fish root-owned, which breaks user writes such as
    //    fish history.  Chown non-recursively and only when mis-owned:
    //    subdirs like icons/themes/fonts are read-only host bind mounts and
    //    must never be touched.
    let local_dir = home_dir.join(".local");
    let share_dir = local_dir.join("share");
    for dir in [&local_dir, &share_dir, &share_dir.join("fish")] {
        let _ = std::fs::create_dir_all(dir);
    }
    for dir in [&local_dir, &share_dir, &share_dir.join("fish")] {
        let needs_fix = match std::fs::metadata(dir) {
            Ok(m) => m.uid() != uid || m.gid() != gid,
            Err(_) => true,
        };
        if needs_fix {
            let _ = std::process::Command::new("chown")
                .args([&owner, dir.to_str().unwrap_or_default()])
                .status();
        }
    }

    // 9. Mark setup complete so subsequent execs in the same container
    // skip this whole block.  /run is tmpfs — re-runs once per boot.
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(marker, b"");

    // fd-lock guard drops → flock released automatically
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASSWD: &str = "root:x:0:0:root:/root:/bin/bash\n\
         alice:x:1000:1000:Alice:/home/alice:/bin/zsh\n\
         noshell:x:1001:1001::/home/noshell:\n";

    #[test]
    fn passwd_entry_parses_ids_and_shell() {
        let entry = passwd_entry(PASSWD, "alice").expect("alice must parse");
        assert_eq!((entry.uid, entry.gid), (1000, 1000));
        assert_eq!(entry.shell, "/bin/zsh");
    }

    #[test]
    fn passwd_entry_rejects_unknown_users_and_empty_shells() {
        assert!(passwd_entry(PASSWD, "nobody").is_none());
        assert!(passwd_entry(PASSWD, "noshell").is_none());
        assert!(passwd_entry("garbage-without-colons", "garbage-without-colons").is_none());
    }

    #[test]
    fn mismatched_ids_are_detected_for_reconciliation() {
        // Image ships `alice` as 1001:1001 while the host is 1000:1000.
        let squatter = passwd_entry("alice:x:1001:1001::/home/alice:/bin/sh\n", "alice").unwrap();
        assert!(squatter.uid != 1000 || squatter.gid != 1000);
    }

    #[test]
    fn failed_privilege_drop_is_an_error_not_a_silent_shell() {
        if nix::unistd::getuid().is_root() {
            // As root the drop would succeed and shed our own privileges.
            return;
        }
        // Non-root cannot assume uid 0: the drop must report, never proceed.
        assert!(drop_privileges("root", 0, 0).is_err());
    }
}
