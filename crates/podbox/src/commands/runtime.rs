use std::ffi::OsString;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use serde::Serialize;

use podbox::cli::OutputFormat;
use podbox::codegen::distros;
use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::podman::{ContainerState, query_state};
use podbox::protocol::{GuestMessage, write_frame};
use podbox::xdg::ResolvedXdgDirs;

/// Try to register a terminal session with the host's `socket_host`.
///
/// Opens a pidfd for the current process, connects to the host socket,
/// sends `RegisterSession` with the pidfd via `SCM_RIGHTS`, then closes
/// the connection.  Silently skips on old kernels or when `serve` is
/// not running.
fn register_session(name: &str, xdg_runtime_dir: &Path) {
    let pidfd = match podbox::process::open_pidfd(std::process::id().cast_signed()) {
        Ok(fd) => fd,
        _ => return,
    };
    let socket_path = xdg_runtime_dir.join("podbox").join(format!("{name}.sock"));
    let mut stream = match UnixStream::connect(&socket_path) {
        Ok(s) => s,
        _ => return,
    };
    if let Err(e) = write_frame(&mut stream, &GuestMessage::RegisterSession) {
        tracing::warn!("failed to register session: {e}");
        return;
    }
    if let Err(e) = podbox::process::send_fd(&stream, pidfd.as_raw_fd()) {
        tracing::warn!("failed to send pidfd to host: {e}");
    }
}

/// Read the user's resolved PATH from the container's `/run/podbox/path`.
///
/// Returns `None` if the container is not running or the daemon hasn't
/// written the file yet (graceful fallback to Quadlet default PATH).
fn read_user_path(name: &str) -> Option<String> {
    let args = podbox::process::args(&["exec", name, "cat", "/run/podbox/path"]);
    let output =
        podbox::process::run_piped_timeout("podman", &args, Duration::from_secs(10)).ok()?;
    if !output.status.success() {
        return None;
    }
    let resolved = String::from_utf8(output.stdout).ok()?;
    let trimmed = resolved.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

/// Resolve the working directory inside the container from the host CWD.
///
/// Builds a map of host→container mount paths from the config, canonicalizes
/// the host CWD, and picks the longest-prefix match. Falls back to
/// `/home/<username>` when nothing matches.
fn resolve_container_workdir(config: &Config, env: &HostEnv, xdg: &ResolvedXdgDirs) -> String {
    let home = format!("/home/{}", env.username);

    let host_cwd = match std::env::current_dir() {
        Ok(p) => match std::fs::canonicalize(&p) {
            Ok(c) => c,
            Err(_) => p,
        },
        Err(_) => return home.clone(),
    };

    // (canonicalized host path, container path)
    let mut mounts: Vec<(PathBuf, PathBuf)> = Vec::new();

    // Home dir
    if let Ok(h) = std::fs::canonicalize(&config.container.home) {
        mounts.push((h, PathBuf::from(&home)));
    }

    // XDG dirs
    let xdg_map: &[(&Option<podbox::xdg::ResolvedXdgDir>, &str)] = &[
        (&xdg.documents, "Documents"),
        (&xdg.downloads, "Downloads"),
        (&xdg.pictures, "Pictures"),
        (&xdg.music, "Music"),
        (&xdg.videos, "Videos"),
        (&xdg.desktop, "Desktop"),
        (&xdg.projects, "Projects"),
    ];
    for (dir, name) in xdg_map {
        if let Some(resolved) = dir {
            if let Ok(h) = std::fs::canonicalize(&resolved.path) {
                mounts.push((h, PathBuf::from(format!("{home}/{name}"))));
            }
        }
    }

    // Extra mounts: "host:container[:opts]"
    for mount in &config.container.mounts.extra {
        let parts: Vec<&str> = mount.split(':').collect();
        if parts.len() < 2 {
            continue;
        }
        if let Ok(h) = std::fs::canonicalize(parts[0]) {
            mounts.push((h, PathBuf::from(parts[1])));
        }
    }

    // Find longest host prefix match
    let mut best: Option<(PathBuf, PathBuf)> = None;
    for (host_path, container_path) in mounts {
        if host_cwd == host_path || host_cwd.starts_with(&host_path) {
            match &best {
                Some((best_host, _))
                    if best_host.components().count() >= host_path.components().count() => {}
                _ => best = Some((host_path, container_path)),
            }
        }
    }

    match best {
        Some((host_path, container_path)) => match host_cwd.strip_prefix(&host_path) {
            Ok(rel) if !rel.as_os_str().is_empty() => {
                container_path.join(rel).to_string_lossy().to_string()
            }
            _ => container_path.to_string_lossy().to_string(),
        },
        None => home,
    }
}

/// Spawn a background watchdog that terminates the `podman exec` client when the
/// user's terminal hangs up.
///
/// Rootless `podman exec -it` relays the user's terminal into a freshly
/// allocated container-side pty. When the user's terminal closes, the client
/// keeps the container-side pty master open, so the shell inside the container
/// never sees a hangup and leaks as an orphaned PPID-0 process that also blocks
/// idle shutdown. The watchdog polls stdin for hangup and on detection SIGTERMs
/// (then SIGKILLs) the exec client, forcing podman to tear down the
/// container-side session.
///
/// The watchdog is a detached child process (a fresh exec of this binary
/// running the hidden `internal-stdin-watchdog` subcommand) that watches the
/// CLI's pid — the pid the CLI then execve's into podman. It ignores
/// SIGHUP/SIGINT and exits as soon as its watched pid exits, so clean
/// sessions leave no residue.
///
/// Only interactive sessions are guarded: `-it` requires a controlling TTY, and
/// non-TTY stdin is relayed as EOF by podman itself.
fn spawn_stdin_watchdog() {
    if !distros::is_tty() {
        return;
    }

    // The watchdog is a fresh exec of this binary running the hidden
    // `internal-stdin-watchdog` subcommand. Spawning (fork+exec) keeps the
    // child free of fork()-in-multi-threaded-process hazards: it starts from a
    // clean single-threaded process with no inherited locks or allocator state.
    let Ok(exe) = std::env::current_exe() else {
        return;
    };

    let _ = std::process::Command::new(exe)
        .arg("internal-stdin-watchdog")
        .arg(std::process::id().to_string())
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Body of the `podbox internal-stdin-watchdog <pid>` subcommand.
///
/// Runs in a freshly exec'd, single-threaded process. Polls stdin for hangup
/// and the pidfd of `parent_pid`; exits when the parent goes away, SIGTERMs
/// (then SIGKILLs after 2s) the parent when stdin hangs up. Never returns
/// normally — always terminates via `std::process::exit`.
pub fn run_stdin_watchdog(parent_pid: u32) -> Result<()> {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, kill, sigaction};
    use nix::unistd::Pid;

    // Ignore SIGHUP/SIGINT: the terminal hangup that triggers us may also be
    // delivered here; we must survive long enough to relay SIGTERM.
    let ignore = SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty());
    // SAFETY: `SigIgn` installs no handler function — asking the kernel to
    // ignore two signals is async-signal-safe and races nothing in this
    // freshly exec'd single-threaded process. Neither std nor rustix offers
    // a safe signal-disposition API.
    #[allow(unsafe_code)]
    unsafe {
        let _ = sigaction(Signal::SIGHUP, &ignore);
        let _ = sigaction(Signal::SIGINT, &ignore);
    }

    let Ok(raw_pid) = i32::try_from(parent_pid) else {
        std::process::exit(1);
    };
    let parent_pid = Pid::from_raw(raw_pid);

    let Ok(parent_fd) = podbox::process::open_pidfd(parent_pid.as_raw()) else {
        std::process::exit(1);
    };

    let stdin = std::io::stdin();
    let mut fds = [
        nix::poll::PollFd::new(stdin.as_fd(), PollFlags::POLLHUP | PollFlags::POLLERR),
        PollFd::new(parent_fd.as_fd(), PollFlags::POLLIN),
    ];

    loop {
        match poll(&mut fds, PollTimeout::from(Some(30_000u16))) {
            Ok(0) => {}
            Err(nix::errno::Errno::EINTR) => {}
            Err(_) => std::process::exit(1),
            Ok(_) => {
                let parent_events = fds[1].revents().unwrap_or(PollFlags::empty());
                if parent_events.contains(PollFlags::POLLIN)
                    || parent_events.contains(PollFlags::POLLHUP)
                    || parent_events.contains(PollFlags::POLLERR)
                {
                    // Parent exited — nothing left to watch.
                    std::process::exit(0);
                }

                let stdin_events = fds[0].revents().unwrap_or(PollFlags::empty());
                if stdin_events.contains(PollFlags::POLLHUP)
                    || stdin_events.contains(PollFlags::POLLERR)
                {
                    let _ = kill(parent_pid, Signal::SIGTERM);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let _ = kill(parent_pid, Signal::SIGKILL);
                    std::process::exit(0);
                }
            }
        }
    }
}

/// Enter a shell inside the container.
pub fn run_shell_enter(
    env: &HostEnv,
    config: &Config,
    name: &str,
    dry_run: bool,
    xdg: &ResolvedXdgDirs,
) -> Result<()> {
    let tty_flag = if distros::is_tty() { "-it" } else { "-i" };
    let workdir = resolve_container_workdir(config, env, xdg);

    let mut exec_args: Vec<OsString> = vec![
        "exec".into(),
        tty_flag.into(),
        "-u".into(),
        env.username.as_str().into(),
        "--workdir".into(),
        workdir.into(),
    ];
    if let Some(ref path) = read_user_path(name) {
        exec_args.push(format!("--env=PATH={path}").into());
    }
    exec_args.push(name.into());
    exec_args.push(config.container.shell.as_str().into());

    if dry_run {
        println!("podman {}", args_to_string(&exec_args));
        return Ok(());
    }
    crate::commands::ensure_running(name, dry_run, crate::commands::DEFAULT_START_TIMEOUT_SECS)?;
    register_session(name, &env.xdg_runtime_dir);
    spawn_stdin_watchdog();
    let err = podbox::process::exec_replace("podman", &exec_args);
    Err(err)
}

/// Execute an arbitrary command inside the container.
pub fn run_exec(
    env: &HostEnv,
    name: &str,
    cmd_args: &[String],
    dry_run: bool,
    root: bool,
) -> Result<()> {
    let tty_flag = if distros::is_tty() { "-it" } else { "-i" };

    let mut exec_args: Vec<OsString> = vec!["exec".into(), tty_flag.into()];
    if !root {
        exec_args.push("-u".into());
        exec_args.push(env.username.as_str().into());
        if let Some(ref path) = read_user_path(name) {
            exec_args.push(format!("--env=PATH={path}").into());
        }
    }
    exec_args.push(name.into());
    for a in cmd_args {
        exec_args.push(a.into());
    }

    if dry_run {
        println!("podman {}", args_to_string(&exec_args));
        return Ok(());
    }
    crate::commands::ensure_running(name, dry_run, crate::commands::DEFAULT_START_TIMEOUT_SECS)?;
    register_session(name, &env.xdg_runtime_dir);
    spawn_stdin_watchdog();
    let err = podbox::process::exec_replace("podman", &exec_args);
    Err(err)
}

/// Run an app in the background inside the container.
pub fn run_run(
    env: &HostEnv,
    name: &str,
    app: &str,
    app_args: &[String],
    dry_run: bool,
) -> Result<()> {
    let mut exec_args: Vec<OsString> = vec![
        "exec".into(),
        "-d".into(),
        "-u".into(),
        env.username.as_str().into(),
    ];
    if let Some(ref path) = read_user_path(name) {
        exec_args.push(format!("--env=PATH={path}").into());
    }
    exec_args.push(name.into());
    exec_args.push(app.into());
    for a in app_args {
        exec_args.push(a.into());
    }

    if dry_run {
        println!("podman {}", args_to_string(&exec_args));
        return Ok(());
    }
    crate::commands::ensure_running(name, dry_run, crate::commands::DEFAULT_START_TIMEOUT_SECS)?;
    register_session(name, &env.xdg_runtime_dir);
    podbox::process::spawn_interactive("podman", &exec_args).map(|_| ())
}

fn quadlet_installed(name: &str) -> bool {
    podbox::quadlet_install::is_installed(name)
}

/// Print the container's running state.
pub fn run_status(name: &str, dry_run: bool, output: podbox::cli::OutputFormat) -> Result<()> {
    if matches!(output, podbox::cli::OutputFormat::Json) {
        let state = query_state(name)?;
        let status = match state {
            ContainerState::Running => "running",
            ContainerState::Stopped if podbox::systemd::is_unit_failed(name) => "failed",
            ContainerState::Stopped => "stopped",
            ContainerState::Missing if quadlet_installed(name) => "not built",
            ContainerState::Missing => "not installed",
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": name,
                "status": status,
            }))?
        );
        return Ok(());
    }

    if dry_run {
        println!("podman inspect --format {{{{.State.Status}}}} {name}");
        return Ok(());
    }
    let state = query_state(name)?;
    match state {
        ContainerState::Running => println!("{name} [running]"),
        ContainerState::Stopped => println!("{name} [stopped]"),
        ContainerState::Missing => {
            if quadlet_installed(name) {
                println!("{name} [not built]");
            } else {
                println!("{name} [not installed]");
            }
        }
    }
    Ok(())
}

/// Check whether a container is managed by systemd (Quadlet).
fn is_systemd_managed(name: &str) -> bool {
    podbox::systemd::is_unit_enabled(name)
}

/// Show container logs, routing through journalctl for systemd-managed
/// containers and falling back to `podman logs` for standalone ones.
pub fn run_logs(
    name: &str,
    follow: bool,
    tail: Option<u32>,
    since: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let lines = tail.unwrap_or(50);

    if is_systemd_managed(name) {
        let mut args: Vec<OsString> = vec![
            "--user".into(),
            "-u".into(),
            format!("{name}.service").into(),
        ];
        if follow {
            args.push("-f".into());
        }
        args.push("-n".into());
        args.push(lines.to_string().into());
        if let Some(s) = &since {
            args.push("--since".into());
            args.push(s.into());
        }
        if dry_run {
            println!("journalctl {}", args_to_string(&args));
            return Ok(());
        }
        println!("Showing logs for: {name}.service");
        podbox::process::spawn_interactive("journalctl", &args).map(|_| ())
    } else {
        let mut args: Vec<OsString> = vec!["logs".into()];
        if follow {
            args.push("-f".into());
        }
        args.push("--tail".into());
        args.push(lines.to_string().into());
        if let Some(s) = &since {
            args.push("--since".into());
            args.push(s.into());
        }
        args.push(name.into());
        if dry_run {
            println!("podman {}", args_to_string(&args));
            return Ok(());
        }
        podbox::process::spawn_interactive("podman", &args).map(|_| ())
    }
}

#[derive(Serialize)]
struct DoctorEntry {
    name: String,
    status: String,
    message: String,
}

/// Run diagnostics on the container and host environment.
pub fn run_doctor(config: &Config, env: &HostEnv, fix: bool, output: OutputFormat) -> Result<()> {
    let mut entries: Vec<DoctorEntry> = Vec::new();
    let mut passes = 0u32;
    let mut failures = 0u32;

    macro_rules! check {
        ($name:expr, $status:expr, $msg:expr $(,)?) => {{
            entries.push(DoctorEntry {
                name: $name.to_string(),
                status: $status.to_string(),
                message: $msg.to_string(),
            });
            match $status {
                "pass" => passes += 1,
                "fail" => failures += 1,
                _ => {}
            }
        }};
    }

    match podbox::podman::podman_version() {
        Ok(ver) if ver.at_least(6, 0) => {
            check!(
                "podman",
                "pass",
                format!(
                    "{}.{}.{} (6.x file-based Quadlet install)",
                    ver.major, ver.minor, ver.patch
                )
            );
        }
        Ok(ver) if ver.at_least(5, 6) => {
            check!(
                "podman",
                "pass",
                format!("{}.{}.{} (>= 5.6)", ver.major, ver.minor, ver.patch)
            );
        }
        Ok(ver) if ver.at_least(5, 5) => {
            check!(
                "podman",
                "warn",
                format!("{}.{}.{} (< 5.6)", ver.major, ver.minor, ver.patch)
            );
        }
        Ok(ver) => {
            check!(
                "podman",
                "fail",
                format!("{}.{}.{} (< 5.5)", ver.major, ver.minor, ver.patch)
            );
        }
        Err(_) => {
            check!("podman", "fail", "not found in PATH".to_string());
        }
    }

    if config.integration.wayland {
        if let Some(ref socket) = env.wayland_socket {
            check!("Wayland socket", "pass", "found");
            match socket.metadata() {
                Ok(meta) => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::MetadataExt;
                        let owner = meta.uid();
                        if owner == env.uid {
                            check!("Wayland socket owner", "pass", "correct");
                        } else if fix {
                            match fix_wayland_socket_ownership(socket) {
                                Ok(()) => {
                                    check!("Wayland socket owner", "pass", "fixed via --fix");
                                }
                                Err(e) => {
                                    check!(
                                        "Wayland socket owner",
                                        "fail",
                                        format!("fix failed: {e}")
                                    );
                                }
                            }
                        } else {
                            check!(
                                "Wayland socket owner",
                                "warn",
                                format!("owner {} != host UID {}", owner, env.uid)
                            );
                        }
                    }
                }
                Err(e) => {
                    check!("Wayland socket", "warn", format!("could not stat: {e}"));
                }
            }
        } else {
            check!(
                "Wayland socket",
                "warn",
                "not found (WAYLAND_DISPLAY may not be set)"
            );
        }
    }

    match which::which("xdg-user-dir") {
        Ok(_) => check!("xdg-user-dir", "pass", "found"),
        Err(_) => check!("xdg-user-dir", "warn", "not found"),
    }

    match std::fs::read_to_string("/etc/subuid") {
        Ok(content) => {
            let username = &env.username;
            if content.lines().any(|l| l.starts_with(username)) {
                check!(
                    "/etc/subuid",
                    "pass",
                    format!("user '{username}' has sub-UID allocations")
                );
            } else {
                check!(
                    "/etc/subuid",
                    "fail",
                    format!("user '{username}' missing from /etc/subuid")
                );
            }
        }
        Err(_) => {
            check!("/etc/subuid", "warn", "could not read /etc/subuid");
        }
    }

    match std::fs::read_to_string("/etc/subgid") {
        Ok(content) => {
            let username = &env.username;
            if content.lines().any(|l| l.starts_with(username)) {
                check!(
                    "/etc/subgid",
                    "pass",
                    format!("user '{username}' has sub-GID allocations")
                );
            } else {
                check!(
                    "/etc/subgid",
                    "fail",
                    format!("user '{username}' missing from /etc/subgid")
                );
            }
        }
        Err(_) => {
            check!("/etc/subgid", "warn", "could not read /etc/subgid");
        }
    }

    match podbox::guest::PODBOX_GUEST {
        Some(bytes) => {
            check!(
                "embedded guest binary",
                "pass",
                format!("{} bytes", bytes.len())
            );
        }
        None => {
            check!(
                "embedded guest binary",
                "warn",
                "no embedded guest — prebuilt-image build; custom `podbox build` unsupported (use a release binary or source build)"
            );
        }
    }

    if config.lifecycle.autostart {
        if which::which("loginctl").is_ok() {
            let username = std::env::var("USER").unwrap_or_default();
            if !username.is_empty()
                && let Ok(output) = podbox::process::run_piped(
                    "loginctl",
                    &[
                        "show-user".into(),
                        username.into(),
                        "--property=Linger".into(),
                    ],
                )
            {
                let out = String::from_utf8_lossy(&output.stdout);
                if out.contains("yes") {
                    check!("loginctl linger", "pass", "enabled");
                } else {
                    check!("loginctl linger", "warn", "not enabled");
                }
            }
        }
    }

    if config.lifecycle.quadlet {
        if podbox::quadlet_install::is_installed(&config.container.name) {
            check!("Quadlet files", "pass", "installed");
        } else {
            check!("Quadlet files", "warn", "not found");
        }
    }

    // ── wl-copy / wl-paste / xdg-dbus-proxy ──
    for &(bin, desc) in &[
        ("wl-copy", "clipboard copy from container"),
        ("wl-paste", "clipboard paste to container"),
        ("xdg-dbus-proxy", "D-Bus proxy"),
    ] {
        match which::which(bin) {
            Ok(_) => check!(bin, "pass", "found"),
            Err(_) => check!(bin, "warn", format!("not found — {desc} will fail")),
        }
    }

    // ── Stale sockets ──
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let sock_dir = Path::new(&runtime_dir).join("podbox");
        if let Ok(entries) = std::fs::read_dir(&sock_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "sock") {
                    if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                        if !name.is_empty()
                            && !podbox::config::config_dir()
                                .join(format!("{name}.toml"))
                                .exists()
                        {
                            check!(
                                "stale socket",
                                "warn",
                                format!("{} (no config)", path.display())
                            );
                        }
                    }
                }
            }
        }
    }

    // ── Orphaned snapshot images ──
    if let Ok(output) = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&[
            "images",
            "--filter",
            "reference=localhost/podbox-*:snapshot-*",
            "--format",
            "{{.Repository}}:{{.Tag}}",
        ]),
    ) {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|l| !l.is_empty()) {
            if let Some((repo, full_tag)) = line.rsplit_once(':') {
                if let Some(box_name) = repo.strip_prefix("localhost/podbox-") {
                    if let Some(tag) = full_tag.strip_prefix("snapshot-") {
                        let meta_path = podbox::config::config_dir()
                            .join("snapshots")
                            .join(box_name)
                            .join(format!("{tag}.toml"));
                        if !meta_path.exists() {
                            check!("orphaned snapshot", "warn", format!("{line} (no metadata)"));
                        }
                    }
                }
            }
        }
    }

    // ── Dead export shims (desktop files) ──
    if let Some(apps_dir) = dirs::data_dir().map(|d| d.join("applications")) {
        if let Ok(entries) = std::fs::read_dir(&apps_dir) {
            for entry in entries.flatten() {
                let fname = entry.file_name();
                let fname_str = fname.to_string_lossy();
                if (fname_str.starts_with("podbox-") || fname_str.starts_with("podmgr-"))
                    && fname_str.ends_with(".desktop")
                {
                    if let Ok(content) = std::fs::read_to_string(entry.path()) {
                        if let Some(exec_line) = content.lines().find(|l| l.starts_with("Exec=")) {
                            let rest = exec_line.strip_prefix("Exec=").unwrap_or("");
                            let args = shell_words::split(rest).unwrap_or_default();
                            let pos = args.iter().position(|a| a == "-C" || a == "--container");
                            if let Some(name) = pos.and_then(|p| args.get(p + 1)) {
                                if !podbox::config::config_dir()
                                    .join(format!("{name}.toml"))
                                    .exists()
                                {
                                    check!(
                                        "dead export",
                                        "warn",
                                        format!(
                                            "{} (container '{name}' missing)",
                                            entry.path().display()
                                        )
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Dead export shims (bin shims) ──
    if let Some(bin_dir) = dirs::home_dir().map(|h| h.join(".local/bin")) {
        if let Ok(entries) = std::fs::read_dir(&bin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    continue;
                }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains("--container") || content.contains("-C ") {
                        let args = shell_words::split(&content).unwrap_or_default();
                        let pos = args.iter().position(|a| a == "-C" || a == "--container");
                        if let Some(name) = pos.and_then(|p| args.get(p + 1)) {
                            if !podbox::config::config_dir()
                                .join(format!("{name}.toml"))
                                .exists()
                            {
                                check!(
                                    "dead export",
                                    "warn",
                                    format!("{} (container '{name}' missing)", path.display())
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    match output {
        OutputFormat::Json => {
            let report = serde_json::json!({
                "checks": entries,
                "summary": {
                    "passes": passes,
                    "failures": failures,
                    "total": entries.len(),
                }
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Text => {
            for entry in &entries {
                let tag = match entry.status.as_str() {
                    "pass" => "PASS",
                    "warn" => "WARN",
                    "fail" => "FAIL",
                    _ => &entry.status,
                };
                println!("[{tag}] {}: {}", entry.name, entry.message);
            }
            println!("\n{passes} / {} checks passed", entries.len());
        }
    }

    if failures > 0 {
        Err(anyhow::anyhow!("{failures} check(s) failed"))
    } else {
        Ok(())
    }
}

fn fix_wayland_socket_ownership(socket: &Path) -> Result<()> {
    let runtime_dir = socket
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine runtime directory from socket path"))?;

    let output = std::process::Command::new("podman")
        .args(["unshare", "chown", "0:0"])
        .arg(socket)
        .arg(runtime_dir)
        .output()
        .map_err(|e| anyhow::anyhow!("Failed to run podman unshare: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("podman unshare chown failed: {stderr}");
    }
    Ok(())
}

fn args_to_string(args: &[OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
