//! Guest sandbox entrypoint: the single `run()` flow plus `setup_user`.
//! One cohesive concern (documented exemption, per the modularization guide 1/8).
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

use nix::unistd::{Gid, Uid, setgid, setuid};

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
        let _ = setgid(Gid::from_raw(gid));
        let _ = setuid(Uid::from_raw(uid));
    }

    let is_tty = nix::unistd::isatty(std::io::stdin()).unwrap_or(false);

    // Environment for the exec'd shell/command. Applied via the spawned
    // process instead of process-global `env::set_var`, which is unsafe
    // and unnecessary here — nothing reads the environment after this
    // point in this process.
    let user_env: Option<(String, String, String)> = drop
        .as_ref()
        .map(|(_, _, user)| (format!("/home/{user}"), user.clone(), user.clone()));

    let exec_shell_cmd = |program: &str, args: &[String], arg0: Option<String>| -> ! {
        let mut c = Command::new(program);
        c.args(args);
        if let Some(a0) = arg0 {
            let _ = c.arg0(a0);
        }
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
        // Interactive with no explicit CMD: start a login shell
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/usr/bin/fish".into());
        let arg0 = format!("-{shell}");
        exec_shell_cmd(&shell, &[], Some(arg0));
    } else {
        // Background (e.g. systemd): keep PID 1 alive
        loop {
            std::thread::sleep(std::time::Duration::from_hours(1));
        }
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

    // 2. User
    let user_exists = passwd().lines().any(|l| l.starts_with(&format!("{user}:")));

    if !user_exists {
        // Detect the best available shell
        let shell = ["/bin/bash", "/bin/zsh", "/bin/fish", "/bin/sh"]
            .iter()
            .find(|s| std::path::Path::new(s).exists())
            .copied()
            .unwrap_or("/bin/sh");

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

    // Make the home directory accessible (read+execute) to other users
    // and writable by the dynamic user.  755 (not 777) limits damage if
    // an untrusted process runs inside the container — it can read config
    // but cannot clobber the home directory.  chmod is safe because it
    // does NOT change ownership through the idmapped mount.
    let _ = std::process::Command::new("chmod")
        .args(["755", &home_dir.to_string_lossy()])
        .status();

    // 3. Supplementary groups — try common group names portably
    let supp_groups = ["wheel", "sudo", "video", "audio", "render"];
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

    // 4. Passwordless sudo
    let sudoers_dir = Path::new("/etc/sudoers.d");
    if sudoers_dir.exists() {
        let sudo_file = sudoers_dir.join("podbox");
        let _ = std::fs::write(&sudo_file, format!("{user} ALL=(ALL) NOPASSWD: ALL\n"));
        let _ = std::fs::set_permissions(&sudo_file, PermissionsExt::from_mode(0o440));
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
