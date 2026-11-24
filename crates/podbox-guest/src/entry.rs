//! Guest sandbox entrypoint: spawn the daemon, drop privileges, and exec.

use std::ffi::CString;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};

use nix::unistd::{Gid, Uid, initgroups, setgid, setuid};

mod user_setup;

use user_setup::{passwd_shell, setup_user};

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
            let _ = e;
            eprintln!("podbox-guest: refusing to run the user shell");
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

#[cfg(test)]
mod tests {
    use super::*;

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
