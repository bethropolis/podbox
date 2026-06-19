use std::collections::HashSet;
use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

use crate::error::GuestError;
use crate::protocol::{GuestMessage, HostMessage, write_frame};
use crate::socket;

const EXCLUDED_COMMS: &[&str] = &[
    "podbox-guest",
    "podmgr-guest",
    "podman-init",
    "catatonit",
    "tini",
];

/// Open a pidfd for a given PID (Linux 5.3+).
///
/// # Safety
///
/// The caller must ensure `pid` refers to a valid process. The kernel
/// validates the PID and returns either a valid fd or -errno.
fn open_pidfd(pid: i32) -> std::io::Result<OwnedFd> {
    let ret = unsafe { nix::libc::syscall(nix::libc::SYS_pidfd_open, pid, 0) };
    if ret < 0 {
        Err(std::io::Error::last_os_error())
    } else {
        // SAFETY: ret is a non-negative fd returned by the kernel.
        let fd = i32::try_from(ret).map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "pidfd_open returned invalid fd",
            )
        })?;
        // SAFETY: fd is a non-negative fd returned by the kernel.
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}

struct TrackedProcess {
    _pid: i32,
    fd: OwnedFd,
}

/// Scan /proc for user processes (anything not in `EXCLUDED_COMMS`).
fn scan_user_processes() -> Vec<i32> {
    let mut pids = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.chars().all(|c| c.is_ascii_digit())
            && let Ok(comm) = std::fs::read_to_string(entry.path().join("comm"))
        {
            let comm_trimmed = comm.trim();
            if !EXCLUDED_COMMS.contains(&comm_trimmed)
                && let Ok(pid) = name_str.parse::<i32>()
            {
                pids.push(pid);
            }
        }
    }
    pids
}

/// Open pidfds for a list of PIDs.
fn track_processes(pids: &[i32]) -> Vec<TrackedProcess> {
    pids.iter()
        .filter_map(|&pid| {
            open_pidfd(pid)
                .ok()
                .map(|fd| TrackedProcess { _pid: pid, fd })
        })
        .collect()
}

/// Check whether any poll events indicate fd readiness.
fn has_event(revents: PollFlags) -> bool {
    revents.contains(PollFlags::POLLIN)
        || revents.contains(PollFlags::POLLHUP)
        || revents.contains(PollFlags::POLLERR)
}

pub fn run() -> Result<(), GuestError> {
    let host_socket_path = socket::host_socket_path()?;
    let container_name = socket::container_name()?;
    let bin_dir = PathBuf::from("/run/podbox/bin");

    // 1. Create /run/podbox/bin/
    std::fs::create_dir_all(&bin_dir)?;

    // 2. Connect to host socket with retry
    eprintln!("podbox-guest: connecting to host socket...");
    let mut host_stream = socket::connect_to_host(&host_socket_path)?;

    // 3. Handshake
    let all_caps: Vec<String> = crate::protocol::ALL_CAPABILITIES
        .iter()
        .map(|&s| s.to_string())
        .collect();
    let (accepted, idle_timeout_secs) =
        socket::handshake(&mut host_stream, &container_name, &all_caps)?;
    let accepted_set: HashSet<String> = accepted.iter().cloned().collect();
    eprintln!("podbox-guest: accepted capabilities: {accepted:?}");

    // 4. Check version drift
    check_version_drift(&accepted_set, &mut host_stream, &container_name);

    // 5. Install interceptor symlinks for accepted capabilities
    install_interceptors(&accepted_set, &bin_dir)?;

    // 6. Write PATH injection
    write_path_injection(&bin_dir)?;

    // 7. Resolve and export the user's full PATH for host-side consumption
    resolve_user_path();

    // 8. Enter event loop (listen for host messages)
    event_loop(&mut host_stream, idle_timeout_secs)?;

    Ok(())
}

fn install_interceptors(
    accepted: &HashSet<String>,
    bin_dir: &std::path::Path,
) -> std::io::Result<()> {
    let self_path = std::env::current_exe()?;
    let self_path_str = self_path.to_string_lossy();

    let symlinks = vec![
        (crate::protocol::CAP_NOTIFY, "notify-send"),
        (crate::protocol::CAP_XDG_OPEN, "xdg-open"),
        (crate::protocol::CAP_CLIPBOARD, "podbox-clipboard"),
        (crate::protocol::CAP_HOST_EXEC, "host-exec"),
    ];

    for (cap, name) in symlinks {
        if accepted.contains(cap) {
            let link = bin_dir.join(name);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(self_path_str.as_ref(), &link)?;
        }
    }

    Ok(())
}

fn check_version_drift(
    accepted: &HashSet<String>,
    _host_stream: &mut UnixStream,
    container_name: &str,
) {
    let Ok(baked_host_version) =
        std::env::var("PODBOX_HOST_VERSION").or_else(|_| std::env::var("PODMGR_HOST_VERSION"))
    else {
        return;
    };

    let guest_version = crate::VERSION;

    if baked_host_version == guest_version {
        return;
    }

    let summary = "podbox: container image is outdated";
    let body = format!(
        "Container '{container_name}' was built with podbox {baked_host_version} but host is now {guest_version}. Run `podbox build --rebuild`."
    );

    if accepted.contains(crate::protocol::CAP_NOTIFY) {
        let msg = crate::protocol::GuestMessage::Notify {
            summary: summary.to_string(),
            body,
            urgency: "normal".to_string(),
            actions: vec![],
            app_name: "podbox".to_string(),
        };
        let _ = crate::socket::connect_and_send_oneshot(&msg);
    } else {
        eprintln!(
            "podbox-guest: image is outdated (built with {baked_host_version}, host is now {guest_version}). Run `podbox build --rebuild`."
        );
    }
}

fn write_path_injection(bin_dir: &std::path::Path) -> std::io::Result<()> {
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

/// Resolve the user's full PATH by spawning their configured shell in
/// interactive mode and capturing `$PATH`.  Writes the result to
/// `/run/podbox/path` for consumption by the host-side `read_user_path()`.
///
/// Silently skips on any error (no file = host falls back to Quadlet default).
fn resolve_user_path() {
    let host_user = std::env::var("HOST_USER").ok();
    let host_uid = std::env::var("HOST_UID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    let host_gid = std::env::var("HOST_GID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());

    let (Some(ref user), Some(uid), Some(gid)) = (host_user.as_ref(), host_uid, host_gid) else {
        return;
    };

    // Determine the best shell for PATH resolution.
    // The user's actual interactive shell (e.g. fish) adds bun, cargo,
    // mise, etc. to PATH via its config — the passwd shell may be /bin/sh
    // which won't source those.  Try fish first, then passwd, then bash.
    let passwd_shell = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|p| {
            p.lines()
                .find(|l| l.starts_with(&format!("{user}:")))
                .and_then(|l| l.split(':').nth(6))
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "/bin/sh".to_string());

    let mut candidates = vec![
        PathBuf::from("/usr/bin/fish"),
        PathBuf::from(&passwd_shell),
        PathBuf::from("/bin/bash"),
        PathBuf::from("/bin/sh"),
    ];
    candidates.dedup();

    let mut best_path = String::new();

    for shell in &candidates {
        if !shell.exists() {
            continue;
        }
        let mut cmd = Command::new(shell);
        cmd.args(["-ic", "echo \"$PATH\""])
            .uid(uid)
            .gid(gid)
            .env("HOME", format!("/home/{user}"))
            .env("USER", user)
            .env("LOGNAME", user)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        if let Ok(output) = cmd.output() {
            if output.status.success() {
                let resolved = String::from_utf8_lossy(&output.stdout);
                let trimmed = resolved.trim();
                if trimmed.len() > best_path.len() {
                    best_path = trimmed.to_string();
                }
            }
        }
    }

    if best_path.is_empty() {
        return;
    }

    let _ = std::fs::write(PathBuf::from("/run/podbox/path"), &best_path);
}

/// Max poll interval in ms (`PollTimeout` caps at `u16::MAX` = 65535).
const MAX_POLL_MS: i64 = 60_000;

#[allow(clippy::too_many_lines)]
fn event_loop(host_stream: &mut UnixStream, idle_timeout_secs: u64) -> Result<(), GuestError> {
    let idle_limit_ms = (idle_timeout_secs.saturating_mul(1000)).cast_signed();
    let mut tracked: Vec<TrackedProcess> = Vec::new();
    let mut remaining_ms = idle_limit_ms;

    loop {
        let host_revents: PollFlags;
        let pid_revents: Vec<PollFlags>;

        {
            let mut fds: Vec<PollFd> = Vec::with_capacity(1 + tracked.len());
            fds.push(PollFd::new(host_stream.as_fd(), PollFlags::POLLIN));
            for proc in &tracked {
                fds.push(PollFd::new(proc.fd.as_fd(), PollFlags::POLLIN));
            }

            let timeout = if tracked.is_empty() && remaining_ms > 0 {
                let poll_ms = remaining_ms.min(MAX_POLL_MS);
                PollTimeout::from(Some(u16::try_from(poll_ms).unwrap_or(u16::MAX)))
            } else {
                PollTimeout::from(None::<u16>)
            };

            match poll(&mut fds, timeout) {
                Ok(0) => {
                    if tracked.is_empty() && remaining_ms > 0 {
                        remaining_ms -= MAX_POLL_MS;
                        if remaining_ms <= 0 {
                            let active = scan_user_processes();
                            if active.is_empty() {
                                let _ = write_frame(host_stream, &GuestMessage::IdleTimeout);
                                return Ok(());
                            }
                            tracked = track_processes(&active);
                            remaining_ms = idle_limit_ms;
                        }
                        continue;
                    }
                    continue;
                }
                Ok(_) => {
                    host_revents = fds[0].revents().unwrap_or(PollFlags::empty());
                    pid_revents = fds[1..]
                        .iter()
                        .map(|f| f.revents().unwrap_or(PollFlags::empty()))
                        .collect();
                }
                Err(nix::errno::Errno::EINTR) => continue,
                Err(e) => return Err(GuestError::Io(e.into())),
            }
        }

        // ── Host socket events ──
        if host_revents.contains(PollFlags::POLLHUP) || host_revents.contains(PollFlags::POLLERR) {
            eprintln!("podbox-guest: host socket hung up.");
            return Ok(());
        }

        if host_revents.contains(PollFlags::POLLIN) {
            match socket::read_host_message(host_stream) {
                Ok(Some(HostMessage::Shutdown)) => {
                    eprintln!("podbox-guest: received shutdown, exiting.");
                    return Ok(());
                }
                Ok(Some(
                    HostMessage::Ping
                    | HostMessage::HelloAck { .. }
                    | HostMessage::ClipboardData { .. }
                    | HostMessage::HostExecStdout { .. }
                    | HostMessage::HostExecStderr { .. }
                    | HostMessage::HostExecDone { .. }
                    | HostMessage::NotifyActionResult { .. },
                )) => {}
                Ok(Some(HostMessage::CheckIdle)) => {
                    let active = scan_user_processes();
                    if active.is_empty() {
                        let _ = write_frame(host_stream, &GuestMessage::IdleTimeout);
                    } else {
                        tracked = track_processes(&active);
                        remaining_ms = idle_limit_ms;
                        let _ = write_frame(host_stream, &GuestMessage::Busy);
                    }
                }
                Ok(None) => {
                    eprintln!("podbox-guest: host disconnected.");
                    return Ok(());
                }
                Err(e) => {
                    if !e.to_string().contains("WouldBlock") {
                        return Err(e);
                    }
                }
            }
        }

        // ── pidfd events (tracked process exits) ──
        let mut exited: Vec<usize> = Vec::new();
        for (i, rev) in pid_revents.iter().enumerate() {
            if has_event(*rev) {
                exited.push(i);
            }
        }

        for &i in exited.iter().rev() {
            tracked.remove(i);
        }

        if !exited.is_empty() && tracked.is_empty() {
            let active = scan_user_processes();
            if !active.is_empty() {
                tracked = track_processes(&active);
                remaining_ms = idle_limit_ms;
            }
            // No processes found: idle timer started naturally on next poll iteration
            // (poll timeout when tracked.is_empty() && remaining_ms > 0).
        }
    }
}
