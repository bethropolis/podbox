//! Guest-side daemon lifecycle: signal handling, interceptor install, pidfd
//! process tracking, and the poll event loop. One orchestration flow whose
//! helpers are tightly coupled to it (documented exemption, per the
//! modularization guide 1/8).
use std::collections::HashSet;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

/// Register SIGTERM/SIGINT handlers that set `shutdown`.
///
/// The event loop polls the flag (and tolerates `EINTR`), so no
/// `SA_RESTART` coordination is needed.
fn setup_signal_handler(shutdown: &Arc<AtomicBool>) -> std::io::Result<()> {
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT] {
        signal_hook::flag::register(sig, Arc::clone(shutdown))?;
    }
    Ok(())
}

use crate::error::GuestError;
use crate::protocol::{GuestMessage, HostMessage, write_frame};
use crate::socket;

mod services;

const EXCLUDED_COMMS: &[&str] = &[
    "podbox-guest",
    "podmgr-guest",
    "podman-init",
    "catatonit",
    "tini",
];

/// Open a pidfd for a given PID (Linux 5.3+).
///
/// Returns `Err` on old kernels or when the PID does not exist.
fn open_pidfd(pid: i32) -> std::io::Result<OwnedFd> {
    let pid = rustix::process::Pid::from_raw(pid)
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "invalid PID"))?;
    rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())
        .map_err(std::io::Error::from)
}

struct TrackedProcess {
    _pid: i32,
    fd: OwnedFd,
}

/// Read a process's parent PID from `/proc/<pid>/stat`.
///
/// The second field is the executable name in parentheses and may itself
/// contain spaces or parentheses, so the fields after the final `)` are
/// split instead: `state`, then `ppid`.
fn ppid_of(pid: i32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, tail) = stat.rsplit_once(')')?;
    tail.split_whitespace().nth(1)?.parse().ok()
}

/// Walk from `start` up to PID 1, collecting every PID on the way.
///
/// Stops on a cycle, on a missing parent, or after a depth bound.
fn collect_ancestry<F: FnMut(i32) -> Option<i32>>(start: i32, mut parent: F) -> HashSet<i32> {
    let mut chain = HashSet::new();
    let mut pid = start;
    // A PID tree is shallow; the bound only guards a corrupt /proc cycle.
    for _ in 0..64 {
        if pid <= 0 || !chain.insert(pid) {
            break;
        }
        match parent(pid) {
            Some(next) if next > 0 => pid = next,
            _ => break,
        }
    }
    chain
}

/// The guest daemon's own PID and that of each of its ancestors.
///
/// Excluding the daemon by process *name* is not enough: on musl-based
/// images the kernel reports the dynamic loader as the command name
/// (`ld-musl-x86_64.so.1`) rather than `podbox-guest`, so a name-based scan
/// counts the daemon itself as a user task and the container never goes
/// idle. Ancestry is independent of how the binary is named.
fn own_ancestry() -> &'static HashSet<i32> {
    static ANCESTRY: OnceLock<HashSet<i32>> = OnceLock::new();
    ANCESTRY.get_or_init(|| {
        let pid = i32::try_from(std::process::id()).unwrap_or(1);
        collect_ancestry(pid, ppid_of)
    })
}

/// Scan /proc for user processes (anything not in `EXCLUDED_COMMS`, not the
/// daemon or its ancestors, and not a supervised service; plus the container
/// init at PID 1 which is never a user task).
fn scan_user_processes() -> Vec<i32> {
    let mut pids = Vec::new();
    let own = own_ancestry();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.chars().all(|c| c.is_ascii_digit())
            && let Ok(pid) = name_str.parse::<i32>()
        {
            if pid == 1 {
                // The container init (reaper) is never a user task, whatever
                // its comm (catatonit/tini, but also e.g. `systemd`).
                continue;
            }
            if own.contains(&pid) {
                // The guest daemon and its ancestors. See `own_ancestry`.
                continue;
            }
            if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
                let comm_trimmed = comm.trim();
                if !EXCLUDED_COMMS.contains(&comm_trimmed) && !services::is_service_process(pid) {
                    pids.push(pid);
                }
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
    services::start_service_supervisor();
    let shutdown = Arc::new(AtomicBool::new(false));
    setup_signal_handler(&shutdown)?;

    let host_socket_path = socket::host_socket_path()?;
    let container_name = socket::container_name()?;
    let bin_dir = PathBuf::from("/run/podbox/bin");

    // 1. Create /run/podbox/bin/ and expose guest version for fastfetch/host
    std::fs::create_dir_all(&bin_dir)?;
    let _ = std::fs::write("/run/podbox/guest-version", crate::VERSION);

    // 2. Connect to host socket with retry
    tracing::info!("guest: connecting to host socket...");
    let mut host_stream = socket::connect_to_host(&host_socket_path)?;

    // 3. Handshake
    let all_caps: Vec<String> = crate::protocol::ALL_CAPABILITIES
        .iter()
        .map(|&s| s.to_string())
        .collect();
    let (accepted, idle_timeout_secs, host_exec_shims) =
        socket::handshake(&mut host_stream, &container_name, &all_caps)?;
    let accepted_set: HashSet<String> = accepted.iter().cloned().collect();
    tracing::info!("guest: accepted capabilities: {accepted:?}");

    // 4. Check version drift
    check_version_drift(&accepted_set, &mut host_stream, &container_name);

    // 5. Install interceptor symlinks for accepted capabilities
    install_interceptors(&accepted_set, &bin_dir, &host_exec_shims)?;

    // 6. Write PATH injection
    write_path_injection(&bin_dir)?;

    // 7. Resolve and export the user's full PATH for host-side consumption
    resolve_user_path();

    // 8. Handle connections and self-heal on host disconnects/restarts.
    // Only a dropped connection reconnects: a shutdown (signal or host
    // message) exits the process, and a version mismatch is fatal.
    let mut backoff_failures = 0u32;
    loop {
        match event_loop(&mut host_stream, idle_timeout_secs, &shutdown) {
            Exit::Shutdown => {
                tracing::info!("guest: shutdown requested, exiting.");
                return Ok(());
            }
            Exit::Disconnected => {
                if shutdown.load(Ordering::Relaxed) {
                    return Ok(());
                }
                tracing::warn!("guest: host disconnected. Retrying connection...");
                backoff_failures = backoff_failures.saturating_add(1);
                std::thread::sleep(backoff_delay(backoff_failures));
                let stream = match socket::connect_to_host(&host_socket_path) {
                    Ok(stream) => stream,
                    Err(e) => {
                        tracing::error!("guest: reconnect failed: {e}");
                        continue;
                    }
                };
                host_stream = stream;
                match socket::handshake(&mut host_stream, &container_name, &all_caps) {
                    Ok((caps, _, shims)) => {
                        tracing::info!(
                            "guest: re-established connection and handshook successfully."
                        );
                        backoff_failures = 0;
                        let caps_set: HashSet<String> = caps.iter().cloned().collect();
                        let _ = install_interceptors(&caps_set, &bin_dir, &shims);
                        let _ = write_path_injection(&bin_dir);
                    }
                    Err(GuestError::VersionMismatch(reason)) => {
                        tracing::error!("guest: {reason}; not reconnecting.");
                        return Err(GuestError::VersionMismatch(reason));
                    }
                    Err(e) => {
                        tracing::error!("guest: re-handshake failed: {e}");
                    }
                }
            }
        }
    }
}

/// How the daemon's connection to the host ended.
#[derive(Debug, PartialEq, Eq)]
enum Exit {
    /// SIGTERM/SIGINT arrived, or the host sent `Shutdown`: exit the
    /// process instead of reconnecting.
    Shutdown,
    /// The connection dropped (EOF, hangup, I/O error): reconnect.
    Disconnected,
}

/// Reconnect wait after `failures` consecutive failed sessions: 1 s
/// doubling up to a 60 s cap. Reset to zero after a successful session.
fn backoff_delay(failures: u32) -> std::time::Duration {
    let secs = 1u64 << failures.saturating_sub(1).min(6);
    std::time::Duration::from_secs(secs.min(60))
}

fn install_interceptors(
    accepted: &HashSet<String>,
    bin_dir: &std::path::Path,
    host_exec_shims: &[String],
) -> std::io::Result<()> {
    let self_path = std::env::current_exe()?;
    let self_path_str = self_path.to_string_lossy();

    for (name, cap) in crate::protocol::INTERCEPTORS {
        if accepted.contains(*cap) {
            let link = bin_dir.join(name);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(self_path_str.as_ref(), &link)?;
        }
    }

    // Custom host-exec shims (e.g. `git` -> podbox-guest shim)
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
            let link = bin_dir.join(shim);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(self_path_str.as_ref(), &link)?;
            tracing::info!("installed host-exec shim: /run/podbox/bin/{shim}");
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
        let _ = crate::socket::connect_and_send_oneshot(&msg, crate::protocol::CAP_NOTIFY);
    } else {
        tracing::warn!(
            "image is outdated (built with {baked_host_version}, host is now {guest_version}). Run `podbox build --rebuild`."
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

fn run_command_with_timeout(
    mut cmd: Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    let child = cmd.spawn().ok()?;
    let child_id = child.id();

    // Spawn a monitor thread to enforce the timeout
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if rx.recv_timeout(timeout).is_err() {
            // Timeout reached; terminate the child process safely
            #[cfg(unix)]
            {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(child_id.cast_signed()),
                    nix::sys::signal::Signal::SIGKILL,
                );
            }
        }
    });

    let output = child.wait_with_output().ok();
    let _ = tx.send(()); // Signal the monitor thread to exit
    output
}

/// Marker-delimited probe output: interactive shells print MOTD/greetings
/// to stdout (fastfetch art, color codes), which would otherwise be captured
/// as part of `$PATH`. The markers isolate the actual value.
const PATH_MARKER_START: &str = "__podbox_path__";
const PATH_MARKER_END: &str = "__podbox_path_end__";

/// Extract the probed PATH from shell output: the text between the markers.
/// Falls back to the whole trimmed output when markers are absent.
fn extract_probed_path(output: &str) -> &str {
    if let Some((_, rest)) = output.split_once(PATH_MARKER_START)
        && let Some((path, _)) = rest.split_once(PATH_MARKER_END)
    {
        return path.trim();
    }
    output.trim()
}

/// Persistent PATH cache: freshness window and sanity cap.
const PATH_CACHE_TTL_SECS: u64 = 24 * 3600;
const PATH_CACHE_MAX_LEN: usize = 16 * 1024;

fn path_cache_file(home: &str) -> PathBuf {
    PathBuf::from(home).join(".cache/podbox/path")
}

fn unix_now_secs() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Read a cached PATH: accepted only on guest-version match, age under TTL,
/// and sane content. Anything else returns `None` (re-probe).
fn read_path_cache(home: &str, now_secs: u64) -> Option<String> {
    let content = std::fs::read_to_string(path_cache_file(home)).ok()?;
    let mut parts = content.splitn(3, '\n');
    let version = parts.next()?;
    let stamp: u64 = parts.next()?.trim().parse().ok()?;
    let path = parts.next()?.trim_end_matches('\n');
    if version != crate::VERSION {
        return None;
    }
    if now_secs.saturating_sub(stamp) > PATH_CACHE_TTL_SECS {
        return None;
    }
    if path.is_empty()
        || path.len() > PATH_CACHE_MAX_LEN
        || path.bytes().any(|b| b == 0 || b == b'\n' || b == b'\r')
    {
        return None;
    }
    Some(path.to_string())
}

/// Persist a probed PATH for future boots. Best-effort: failures are silent
/// (worst case is today's behavior — probe every boot).
fn write_path_cache(home: &str, uid: u32, gid: u32, path: &str, now_secs: u64) {
    let file = path_cache_file(home);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = format!("{}\n{}\n{}", crate::VERSION, now_secs, path);
    if std::fs::write(&file, &content).is_ok() {
        let _ = std::os::unix::fs::chown(&file, Some(uid), Some(gid));
    }
}

/// Resolve the user's full PATH by spawning their configured shell in
/// interactive mode and capturing `$PATH`.  Writes the result to
/// `/run/podbox/path` for consumption by the host-side `read_user_path()`.
///
/// Probing is skipped when a fresh persistent cache exists: the home volume
/// survives container recreation, so only the first-ever boot (and any boot
/// a day after the last probe) pays for shell spawns.
///
/// Silently skips on any error (no file = host falls back to Quadlet default).
#[allow(clippy::similar_names)]
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
    let home = format!("/home/{user}");

    // Fast path: a fresh persistent cache (home volume survives recreation).
    if let Some(now) = unix_now_secs()
        && let Some(cached) = read_path_cache(&home, now)
    {
        let _ = std::fs::write("/run/podbox/path", &cached);
        return;
    }

    // Determine the best shell for PATH resolution.
    // The user's interactive shell (e.g. fish) adds bun, cargo,
    // mise, etc. to PATH via its config — the passwd shell may be /bin/sh
    // which won't source those.  Try fish first, then passwd, then bash.
    let passwd_shell = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|p| {
            p.lines()
                .find(|l| l.starts_with(&format!("{user}:")))
                .and_then(|l| l.split(':').nth(6))
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "/bin/sh".to_string());

    let mut candidates = vec![
        PathBuf::from("/usr/bin/fish"),
        PathBuf::from(&passwd_shell),
        PathBuf::from("/bin/bash"),
        PathBuf::from("/bin/sh"),
    ];
    candidates.sort();
    candidates.dedup();

    let mut best_path = String::new();

    for shell in &candidates {
        if !shell.exists() {
            continue;
        }
        let probe = format!("echo \"{PATH_MARKER_START}$PATH{PATH_MARKER_END}\"");
        let mut cmd = Command::new(shell);
        cmd.args(["-ic", &probe])
            .uid(uid)
            .gid(gid)
            .env("HOME", format!("/home/{user}"))
            .env("USER", user)
            .env("LOGNAME", user)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        if let Some(output) = run_command_with_timeout(cmd, std::time::Duration::from_secs(2))
            && output.status.success()
        {
            let resolved = String::from_utf8_lossy(&output.stdout);
            let trimmed = extract_probed_path(&resolved);
            if trimmed.len() > best_path.len() {
                best_path = trimmed.to_string();
            }
        }
    }

    if best_path.is_empty() {
        return;
    }

    let _ = std::fs::write(PathBuf::from("/run/podbox/path"), &best_path);
    if let Some(now) = unix_now_secs() {
        write_path_cache(&home, uid, gid, &best_path, now);
    }
}

/// Max poll interval in ms (`PollTimeout` caps at `u16::MAX` = 65535).
const MAX_POLL_MS: i64 = 60_000;

/// Grace period between the final idle scan and firing `IdleTimeout`. A task
/// that spawns during this window is caught by the confirm scan, closing the
/// TOCTOU race where a freshly-started process would be killed by shutdown.
const IDLE_CONFIRM_MS: u64 = 1_000;

#[allow(clippy::too_many_lines)]
fn event_loop(host_stream: &mut UnixStream, idle_timeout_secs: u64, shutdown: &AtomicBool) -> Exit {
    let idle_limit_ms = (idle_timeout_secs.saturating_mul(1000)).cast_signed();
    let mut tracked: Vec<TrackedProcess> = Vec::new();
    let mut remaining_ms = idle_limit_ms;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            tracing::info!("guest: received shutdown signal, exiting.");
            return Exit::Shutdown;
        }

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
                            let mut active = scan_user_processes();
                            if active.is_empty() {
                                // Confirm we're still idle after a short grace
                                // before killing the box, so a task started in
                                // the window since the last scan is not lost.
                                std::thread::sleep(std::time::Duration::from_millis(
                                    IDLE_CONFIRM_MS,
                                ));
                                active = scan_user_processes();
                            }
                            if active.is_empty() {
                                let _ = write_frame(host_stream, &GuestMessage::IdleTimeout);
                                // Our own idle timer fired: the host will
                                // stop the unit. Exit instead of
                                // reconnecting to a container going down.
                                return Exit::Shutdown;
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
                Err(nix::errno::Errno::EINTR) => {
                    if shutdown.load(Ordering::Relaxed) {
                        tracing::info!("guest: received shutdown signal, exiting.");
                        return Exit::Shutdown;
                    }
                    continue;
                }
                Err(_) => return Exit::Disconnected,
            }
        }

        // ── Host socket events ──
        // POLLIN first: a peer that writes and then closes delivers its
        // last message alongside POLLHUP. That message (e.g. `Shutdown`)
        // must be processed, not discarded as a hangup.
        if host_revents.contains(PollFlags::POLLIN) {
            match socket::read_host_message(host_stream) {
                Ok(Some(HostMessage::Shutdown)) => {
                    tracing::info!("guest: received shutdown, exiting.");
                    return Exit::Shutdown;
                }
                Ok(Some(
                    HostMessage::Ping
                    | HostMessage::HelloAck { .. }
                    | HostMessage::ClipboardData { .. }
                    | HostMessage::HostExecStdout { .. }
                    | HostMessage::HostExecStderr { .. }
                    | HostMessage::HostExecOutput { .. }
                    | HostMessage::HostExecDone { .. }
                    | HostMessage::NotifyActionResult { .. }
                    | HostMessage::Error { .. },
                )) => {}
                Ok(Some(HostMessage::GetInfo)) => {
                    let _ = write_frame(
                        host_stream,
                        &GuestMessage::Info {
                            guest_version: crate::VERSION.to_string(),
                            protocol_version: crate::protocol::PROTOCOL_VERSION,
                        },
                    );
                }
                Ok(Some(HostMessage::CheckIdle)) => {
                    let mut active = scan_user_processes();
                    if active.is_empty() {
                        std::thread::sleep(std::time::Duration::from_millis(IDLE_CONFIRM_MS));
                        active = scan_user_processes();
                    }
                    if active.is_empty() {
                        let _ = write_frame(host_stream, &GuestMessage::IdleTimeout);
                    } else {
                        tracked = track_processes(&active);
                        remaining_ms = idle_limit_ms;
                        let _ = write_frame(host_stream, &GuestMessage::Busy);
                    }
                }
                Ok(None) => {
                    tracing::warn!("guest: host disconnected.");
                    return Exit::Disconnected;
                }
                Err(GuestError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    // Spurious poll wakeup: POLLIN was reported but there is
                    // nothing to read yet. Fall through to the hangup check.
                }
                Err(_) => return Exit::Disconnected,
            }
        }

        if host_revents.contains(PollFlags::POLLHUP) || host_revents.contains(PollFlags::POLLERR) {
            tracing::warn!("guest: host socket hung up.");
            return Exit::Disconnected;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn extract_probed_path_strips_motd_noise() {
        let noisy = "\x1b[100m   \x1b[m\nfastfetch art here\n__podbox_path__/usr/bin:/bin__podbox_path_end__\n";
        assert_eq!(extract_probed_path(noisy), "/usr/bin:/bin");
    }

    #[test]
    fn extract_probed_path_falls_back_without_markers() {
        assert_eq!(extract_probed_path("  /usr/bin:/bin\n"), "/usr/bin:/bin");
    }

    #[test]
    fn extract_probed_path_empty_when_markers_empty() {
        assert_eq!(
            extract_probed_path("noise__podbox_path____podbox_path_end__tail"),
            ""
        );
    }

    fn cache_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("podbox-pathcache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn path_cache_roundtrip_when_fresh() {
        let home = cache_dir("fresh");
        let home_str = home.to_string_lossy().to_string();
        write_path_cache(&home_str, 1000, 1000, "/usr/bin:/bin", 1_700_000_000);
        assert_eq!(
            read_path_cache(&home_str, 1_700_000_000 + 3600),
            Some("/usr/bin:/bin".to_string())
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn path_cache_rejects_stale_version_and_age() {
        let home = cache_dir("stale");
        let home_str = home.to_string_lossy().to_string();
        write_path_cache(&home_str, 1000, 1000, "/usr/bin:/bin", 1_700_000_000);
        // Wrong version line.
        let file = home.join(".cache/podbox/path");
        let bad = std::fs::read_to_string(&file)
            .unwrap()
            .replacen(crate::VERSION, "v0.0.0", 1);
        std::fs::write(&file, bad).unwrap();
        assert_eq!(read_path_cache(&home_str, 1_700_000_000 + 1), None);
        // Expired.
        write_path_cache(&home_str, 1000, 1000, "/usr/bin:/bin", 1_700_000_000);
        assert_eq!(
            read_path_cache(&home_str, 1_700_000_000 + PATH_CACHE_TTL_SECS + 1),
            None
        );
        // Empty and overlong content.
        write_path_cache(&home_str, 1000, 1000, "", 1_700_000_000);
        assert_eq!(read_path_cache(&home_str, 1_700_000_000 + 1), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ancestry_includes_self_and_every_parent() {
        // 8 → 4 → 2 → 1, with 1 being the container init.
        let tree = HashMap::from([(8, Some(4)), (4, Some(2)), (2, Some(1)), (1, None)]);
        let chain = collect_ancestry(8, |pid| tree.get(&pid).copied().flatten());
        assert_eq!(chain, HashSet::from([8, 4, 2, 1]));
    }

    #[test]
    fn ancestry_stops_on_a_cycle() {
        // A corrupt or unusual /proc must not spin the walk.
        let tree = HashMap::from([(10, Some(11)), (11, Some(10))]);
        let chain = collect_ancestry(10, |pid| tree.get(&pid).copied().flatten());
        assert_eq!(chain, HashSet::from([10, 11]));
    }

    #[test]
    fn ancestry_stops_at_a_missing_parent() {
        let chain = collect_ancestry(42, |_| None);
        assert_eq!(chain, HashSet::from([42]));
    }

    #[test]
    fn ancestry_of_pid_one_is_just_pid_one() {
        let chain = collect_ancestry(1, |pid| if pid == 1 { Some(0) } else { None });
        assert_eq!(chain, HashSet::from([1]));
    }

    /// The regression this guards: on musl images the guest's `comm` is the
    /// loader, so name-based exclusion missed the daemon and the container
    /// could never idle. Ancestry excludes it regardless of the name.
    #[test]
    fn own_ancestry_covers_the_live_daemon_whatever_its_comm() {
        let own = own_ancestry();
        let self_pid = i32::try_from(std::process::id()).expect("pid fits in i32");
        assert!(own.contains(&self_pid), "the daemon must exclude itself");
        let comm = std::fs::read_to_string("/proc/self/comm").unwrap_or_default();
        let comm = comm.trim().to_string();
        // Whether or not the name-based list happens to cover this process,
        // ancestry must: that is what makes musl images work.
        if !EXCLUDED_COMMS.contains(&comm.as_str()) {
            assert!(
                own.contains(&self_pid),
                "daemon named {comm} must be excluded by ancestry"
            );
        }
    }

    // ── shutdown / reconnect behavior ──

    /// Drive `event_loop` on one end of a socket pair. The result comes
    /// back over a channel with a deadline, so a loop that never returns
    /// fails the test instead of hanging the suite.
    fn run_event_loop_once(
        server: std::os::unix::net::UnixStream,
        idle_timeout_secs: u64,
        preset_shutdown: bool,
    ) -> Exit {
        let shutdown = AtomicBool::new(preset_shutdown);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut stream = server;
            let _ = tx.send(event_loop(&mut stream, idle_timeout_secs, &shutdown));
        });
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("event loop did not return")
    }

    #[test]
    fn backoff_delay_doubles_up_to_a_cap() {
        let secs: Vec<u64> = (0..=8).map(|f| backoff_delay(f).as_secs()).collect();
        assert_eq!(secs, vec![1, 1, 2, 4, 8, 16, 32, 60, 60]);
        assert_eq!(backoff_delay(u32::MAX).as_secs(), 60);
    }

    #[test]
    fn shutdown_message_makes_the_event_loop_exit() {
        let (server, mut mock) = std::os::unix::net::UnixStream::pair().unwrap();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(200));
            let _ = write_frame(&mut mock, &HostMessage::Shutdown);
        });
        assert_eq!(run_event_loop_once(server, 0, false), Exit::Shutdown);
    }

    #[test]
    fn closed_host_connection_is_a_disconnect_not_a_shutdown() {
        let (server, mock) = std::os::unix::net::UnixStream::pair().unwrap();
        drop(mock);
        assert_eq!(run_event_loop_once(server, 0, false), Exit::Disconnected);
    }

    #[test]
    fn preset_shutdown_flag_exits_without_reading() {
        let (server, _mock) = std::os::unix::net::UnixStream::pair().unwrap();
        assert_eq!(run_event_loop_once(server, 0, true), Exit::Shutdown);
    }
}
