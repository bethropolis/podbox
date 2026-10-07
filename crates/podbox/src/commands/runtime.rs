use std::ffi::OsString;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;

use podbox::codegen::distros;
use podbox::config::Config;
use podbox::podman::{ContainerState, query_state};
use podbox::protocol::{GuestMessage, write_frame};
use podbox::xdg::ResolvedXdgDirs;

mod context;
pub mod doctor;

use doctor::is_systemd_managed;
pub use doctor::run_doctor;
pub use doctor::try_fix_bare_memory_for_target;

pub(crate) use context::RunContext;
use context::resolve_container_workdir;

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

fn append_env_args(args: &mut Vec<OsString>, env: &[String]) -> Result<()> {
    for entry in env {
        let (key, value) = entry.split_once('=').ok_or_else(|| {
            anyhow::anyhow!("invalid environment override '{entry}': expected KEY=VALUE")
        })?;
        if key.is_empty()
            || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            || key.as_bytes()[0].is_ascii_digit()
            || value.chars().any(|c| matches!(c, '\0' | '\n' | '\r'))
        {
            anyhow::bail!("invalid environment override '{entry}': invalid key or value");
        }
        args.push(format!("--env={entry}").into());
    }
    Ok(())
}

fn append_forwarded_env(args: &mut Vec<OsString>, config: &Config, explicit: &[String]) {
    let explicit_keys: std::collections::HashSet<&str> = explicit
        .iter()
        .filter_map(|entry| entry.split_once('=').map(|(key, _)| key))
        .collect();
    for (key, value) in std::env::vars() {
        if config.container.env.values.contains_key(&key) || explicit_keys.contains(key.as_str()) {
            continue;
        }
        let forwarded = config.container.env.forward.iter().any(|pattern| {
            if let Some(prefix) = pattern.strip_suffix('*') {
                key.starts_with(prefix)
            } else {
                key == *pattern
            }
        });
        if forwarded {
            args.push(format!("--env={key}={value}").into());
        }
    }
}

fn podman_exec_output(name: &str, user: &str, command: &[&str]) -> Option<String> {
    let mut args: Vec<OsString> = vec!["exec".into(), "-u".into(), user.into(), name.into()];
    args.extend(command.iter().map(OsString::from));
    let output = podbox::process::run_piped("podman", &args).ok()?;
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_mount_paths(
    config: &Config,
    xdg: &ResolvedXdgDirs,
    here: Option<&str>,
    name: &str,
) -> Vec<String> {
    let home = format!("/home/{name}");
    let mut paths = Vec::new();
    for mount in &config.container.mounts.extra {
        if let Some(path) = mount.split(':').nth(1) {
            paths.push(path.to_string());
        }
    }
    for (dir, child) in [
        (&xdg.documents, "Documents"),
        (&xdg.downloads, "Downloads"),
        (&xdg.pictures, "Pictures"),
        (&xdg.music, "Music"),
        (&xdg.videos, "Videos"),
        (&xdg.desktop, "Desktop"),
        (&xdg.projects, "Projects"),
    ] {
        if dir.is_some() {
            paths.push(format!("{home}/{child}"));
        }
    }
    if let Some(path) = here {
        paths.push(path.to_string());
    }
    paths.sort();
    paths.dedup();
    paths
}

/// Container-side git state, gathered in a single `podman exec`.
///
/// Every field used to cost its own round-trip: `git config --get-all
/// safe.directory`, `git config --get user.name`, `git config --get
/// user.email`, and a `printenv` per GIT_* key. At ~0.45s per podman call
/// that made a plain `podbox exec true` spend most of its time asking git
/// questions it already knew the answer to.
struct GitBridgeState {
    safe_directories: Vec<String>,
    has_name: bool,
    has_email: bool,
    env_set: std::collections::HashSet<String>,
}

/// One exec that prints the container's whole git identity state.
///
/// Sections are delimited by `@@key` markers so values containing newlines
/// stay unambiguous. `sh` is present in every base image podbox builds for.
const GIT_PROBE: &str = r#"printf '@@safe\n'; git config --global --get-all safe.directory 2>/dev/null; printf '@@name\n'; git config --global --get user.name 2>/dev/null; printf '@@email\n'; git config --global --get user.email 2>/dev/null; printf '@@env\n'; env | grep -E '^GIT_(AUTHOR|COMMITTER)_(NAME|EMAIL)='"#;

/// Parse [`GIT_PROBE`] output.
fn parse_git_probe(stdout: &str) -> GitBridgeState {
    let mut safe_directories = Vec::new();
    let mut has_name = false;
    let mut has_email = false;
    let mut env_set = std::collections::HashSet::new();
    let mut section = String::new();

    for line in stdout.lines() {
        match line.trim_end() {
            "@@safe" => section = "safe".into(),
            "@@name" => section = "name".into(),
            "@@email" => section = "email".into(),
            "@@env" => section = "env".into(),
            _ => match section.as_str() {
                "safe" => safe_directories.push(line.to_string()),
                "name" => has_name = !line.trim().is_empty(),
                "email" => has_email = !line.trim().is_empty(),
                "env" => {
                    if let Some((key, value)) = line.split_once('=') {
                        if !value.trim().is_empty() {
                            env_set.insert(key.to_string());
                        }
                    }
                }
                _ => {}
            },
        }
    }

    GitBridgeState {
        safe_directories,
        has_name,
        has_email,
        env_set,
    }
}

fn probe_git_bridge(name: &str, git_user: &str) -> Option<GitBridgeState> {
    let command = podman_exec_output(name, git_user, &["sh", "-c", GIT_PROBE])?;
    Some(parse_git_probe(&command))
}

/// Add every missing `safe.directory` in one exec rather than one per path.
///
/// `git config --add` runs in a loop inside the container, so a first-time
/// `podbox enter` in a project directory costs a single round-trip instead of
/// one per mount plus the working directory.
fn add_safe_directories(name: &str, git_user: &str, paths: &[String]) {
    if paths.is_empty() {
        return;
    }
    let script = "for p in \"$@\"; do git config --global --add safe.directory \"$p\"; done";
    let mut command: Vec<&str> = vec!["sh", "-c", script, "podbox-safe"];
    command.extend(paths.iter().map(String::as_str));
    let _ = podman_exec_output(name, git_user, &command);
}

/// What the last bridge sync decided, so a repeat exec can reuse it.
#[derive(serde::Deserialize, serde::Serialize, Default)]
struct GitBridgeStamp {
    /// Container id the sync ran against; a recreate invalidates it.
    container_id: String,
    /// `git_mount_paths` the sync covered.
    paths: Vec<String>,
    /// `--env=KEY=VALUE` arguments the sync decided to inject.
    identity_env: Vec<String>,
}

/// `~/.local/state/podbox/<name>/git-bridge.json`
fn git_bridge_stamp_path(name: &str) -> PathBuf {
    dirs::state_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/state"))
        .join("podbox")
        .join(name)
        .join("git-bridge.json")
}

/// True when a previous run already synced exactly this container and these
/// paths, so every probe would return the same answer.
fn git_bridge_is_current(
    stamp: &GitBridgeStamp,
    container_id: Option<&str>,
    paths: &[String],
) -> bool {
    let Some(id) = container_id else {
        return false;
    };
    !id.is_empty() && stamp.container_id == id && stamp.paths == paths
}

fn read_git_bridge_stamp(name: &str) -> Option<GitBridgeStamp> {
    let raw = std::fs::read_to_string(git_bridge_stamp_path(name)).ok()?;
    serde_json::from_str(&raw).ok()
}

fn write_git_bridge_stamp(name: &str, stamp: &GitBridgeStamp) {
    let path = git_bridge_stamp_path(name);
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    if let Ok(json) = serde_json::to_string(stamp) {
        let _ = std::fs::write(&path, json);
    }
}

fn prepare_git_bridge(
    ctx: RunContext<'_>,
    git_user: &str,
    here: Option<&str>,
    args: &mut Vec<OsString>,
) {
    let RunContext {
        config,
        env,
        name,
        xdg,
        env_overrides: explicit,
        ..
    } = ctx;
    if !config.integration.git_identity {
        return;
    }
    let paths = git_mount_paths(config, xdg, here, &env.username);
    let container_id = podbox::podman::seen_container_id(name);

    // Fast path: the container has not been recreated and the mount set is
    // unchanged, so the recorded decision is still valid. This is what keeps a
    // warm `podbox exec` down to the exec itself.
    let identity_env: Vec<String> = match read_git_bridge_stamp(name) {
        Some(stamp) if git_bridge_is_current(&stamp, container_id.as_deref(), &paths) => {
            stamp.identity_env
        }
        _ => {
            let identity_env = sync_git_bridge(name, git_user, &paths, config, explicit);
            write_git_bridge_stamp(
                name,
                &GitBridgeStamp {
                    container_id: container_id.clone().unwrap_or_default(),
                    paths: paths.clone(),
                    identity_env: identity_env.clone(),
                },
            );
            identity_env
        }
    };

    if identity_env.is_empty() {
        return;
    }
    let identity_args: Vec<OsString> = identity_env.iter().map(OsString::from).collect();
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.splice(index..index, identity_args);
    }
}

/// The original per-exec logic, with the round-trips collapsed: one probe
/// exec, at most one exec to write the missing `safe.directory` entries.
fn sync_git_bridge(
    name: &str,
    git_user: &str,
    paths: &[String],
    config: &Config,
    explicit: &[String],
) -> Vec<String> {
    let mut identity_env = Vec::new();
    let Some(state) = probe_git_bridge(name, git_user) else {
        return identity_env;
    };

    let missing: Vec<String> = paths
        .iter()
        .filter(|path| !state.safe_directories.contains(path))
        .cloned()
        .collect();
    add_safe_directories(name, git_user, &missing);

    let explicit_keys: std::collections::HashSet<&str> = explicit
        .iter()
        .filter_map(|entry| entry.split_once('=').map(|(key, _)| key))
        .collect();
    for (configured, git_key, env_keys) in [
        (
            state.has_name,
            "user.name",
            ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"],
        ),
        (
            state.has_email,
            "user.email",
            ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"],
        ),
    ] {
        if configured {
            continue;
        }
        let Ok(output) = std::process::Command::new("git")
            .args(["config", "--global", "--get", git_key])
            .output()
        else {
            continue;
        };
        if !output.status.success() {
            continue;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            continue;
        }
        for env_key in env_keys {
            let configured_env = state.env_set.contains(env_key);
            let forwarded = std::env::var_os(env_key).is_some()
                && config.container.env.forward.iter().any(|pattern| {
                    pattern
                        .strip_suffix('*')
                        .map_or(pattern == env_key, |prefix| env_key.starts_with(prefix))
                });
            if forwarded
                || configured_env
                || explicit_keys.contains(env_key)
                || config.container.env.values.contains_key(env_key)
            {
                continue;
            }
            identity_env.push(format!("--env={env_key}={value}"));
        }
    }
    identity_env
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
pub fn run_shell_enter(ctx: RunContext<'_>, here: bool) -> Result<()> {
    let RunContext {
        env,
        config,
        name,
        xdg,
        env_overrides,
        dry_run,
    } = ctx;
    let tty_flag = if distros::is_tty() { "-it" } else { "-i" };
    let workdir = resolve_container_workdir(config, env, xdg, here)?;

    let mut exec_args: Vec<OsString> = vec![
        "exec".into(),
        tty_flag.into(),
        "-u".into(),
        env.username.as_str().into(),
        "-w".into(),
        workdir.clone().into(),
    ];
    if let Some(ref path) = read_user_path(name) {
        exec_args.push(format!("--env=PATH={path}").into());
    }
    append_forwarded_env(&mut exec_args, config, env_overrides);
    append_env_args(&mut exec_args, env_overrides)?;
    exec_args.push(name.into());
    exec_args.push(config.container.shell.as_str().into());

    if dry_run {
        println!("podman {}", args_to_string(&exec_args));
        return Ok(());
    }
    crate::commands::ensure_running(name, dry_run, crate::commands::DEFAULT_START_TIMEOUT_SECS)?;
    prepare_git_bridge(ctx, &env.username, here.then_some(&workdir), &mut exec_args);
    register_session(name, &env.xdg_runtime_dir);
    spawn_stdin_watchdog();
    let err = podbox::process::exec_replace("podman", &exec_args);
    Err(err)
}

/// Execute an arbitrary command inside the container.
pub fn run_exec(ctx: RunContext<'_>, cmd_args: &[String], root: bool, here: bool) -> Result<()> {
    let RunContext {
        env,
        config,
        name,
        xdg,
        env_overrides,
        dry_run,
    } = ctx;
    let tty_flag = if distros::is_tty() { "-it" } else { "-i" };

    let mut exec_args: Vec<OsString> = vec!["exec".into(), tty_flag.into()];
    if !root {
        exec_args.push("-u".into());
        exec_args.push(env.username.as_str().into());
        if let Some(ref path) = read_user_path(name) {
            exec_args.push(format!("--env=PATH={path}").into());
        }
    }
    if here {
        exec_args.push("-w".into());
        exec_args.push(resolve_container_workdir(config, env, xdg, true)?.into());
    }
    append_forwarded_env(&mut exec_args, config, env_overrides);
    append_env_args(&mut exec_args, env_overrides)?;
    exec_args.push(name.into());
    for a in cmd_args {
        exec_args.push(a.into());
    }

    if dry_run {
        println!("podman {}", args_to_string(&exec_args));
        return Ok(());
    }
    crate::commands::ensure_running(name, dry_run, crate::commands::DEFAULT_START_TIMEOUT_SECS)?;
    let here_path = if here {
        exec_args
            .windows(2)
            .find(|pair| pair[0] == "-w")
            .and_then(|pair| pair[1].to_str())
            .map(str::to_string)
    } else {
        None
    };
    prepare_git_bridge(
        ctx,
        if root { "root" } else { &env.username },
        here_path.as_deref(),
        &mut exec_args,
    );
    register_session(name, &env.xdg_runtime_dir);
    spawn_stdin_watchdog();
    let err = podbox::process::exec_replace("podman", &exec_args);
    Err(err)
}

/// Run an app in the background inside the container.
pub fn run_run(ctx: RunContext<'_>, app: &str, app_args: &[String]) -> Result<()> {
    let RunContext {
        env,
        name,
        config,
        env_overrides,
        dry_run,
        ..
    } = ctx;
    let mut exec_args: Vec<OsString> = vec![
        "exec".into(),
        "-d".into(),
        "-u".into(),
        env.username.as_str().into(),
    ];
    if let Some(ref path) = read_user_path(name) {
        exec_args.push(format!("--env=PATH={path}").into());
    }
    append_forwarded_env(&mut exec_args, config, env_overrides);
    append_env_args(&mut exec_args, env_overrides)?;
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
    prepare_git_bridge(ctx, &env.username, None, &mut exec_args);
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
        // Canonical vocabulary shared with `podbox list --output json`:
        // running | stopped | failed | unbuilt. The Quadlet distinction is
        // preserved as a separate boolean rather than a second status word.
        let (status, installed) = match state {
            ContainerState::Running => ("running", true),
            ContainerState::Stopped if podbox::systemd::is_unit_failed(name) => ("failed", true),
            ContainerState::Stopped => ("stopped", true),
            ContainerState::Missing => ("unbuilt", quadlet_installed(name)),
        };
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "name": name,
                "status": status,
                "installed": installed,
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
        ContainerState::Stopped if podbox::systemd::is_unit_failed(name) => {
            println!("{name} [failed]");
        }
        ContainerState::Stopped => println!("{name} [stopped]"),
        ContainerState::Missing => println!("{name} [unbuilt]"),
    }
    Ok(())
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

fn args_to_string(args: &[OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod workdir_tests {
    use super::append_env_args;

    #[test]
    fn validates_explicit_environment_overrides() {
        let mut args = Vec::new();
        assert!(append_env_args(&mut args, &["TOKEN=a=b".into()]).is_ok());
        assert_eq!(args[0].to_string_lossy(), "--env=TOKEN=a=b");
        assert!(append_env_args(&mut Vec::new(), &["BAD-NAME=x".into()]).is_err());
        assert!(append_env_args(&mut Vec::new(), &["MISSING_VALUE".into()]).is_err());
    }
}

#[cfg(test)]
mod git_bridge_tests {
    use super::{GitBridgeStamp, git_bridge_is_current, parse_git_probe};

    #[test]
    fn probe_reads_sections() {
        let state = parse_git_probe(
            "@@safe\n/home/user\n/home/user/Projects\n@@name\nbet\n@@email\n\n@@env\n\
             GIT_AUTHOR_NAME=bet\nGIT_COMMITTER_NAME=\nGIT_AUTHOR_EMAIL=bet@host\n",
        );
        assert_eq!(
            state.safe_directories,
            ["/home/user", "/home/user/Projects"]
        );
        assert!(state.has_name);
        assert!(!state.has_email, "empty user.email counts as unset");
        assert!(state.env_set.contains("GIT_AUTHOR_NAME"));
        assert!(!state.env_set.contains("GIT_COMMITTER_NAME"));
        assert!(state.env_set.contains("GIT_AUTHOR_EMAIL"));
    }

    #[test]
    fn probe_handles_empty_output() {
        // A container without git configured still has to parse.
        let state = parse_git_probe("");
        assert!(state.safe_directories.is_empty());
        assert!(!state.has_name);
        assert!(!state.has_email);
        assert!(state.env_set.is_empty());
    }

    #[test]
    fn probe_keeps_paths_with_spaces() {
        let state = parse_git_probe("@@safe\n/home/user/My Projects\n@@name\n");
        assert_eq!(state.safe_directories, ["/home/user/My Projects"]);
    }

    fn stamp(id: &str, paths: &[&str]) -> GitBridgeStamp {
        GitBridgeStamp {
            container_id: id.to_string(),
            paths: paths.iter().map(|p| (*p).to_string()).collect(),
            identity_env: vec!["--env=GIT_AUTHOR_NAME=bet".to_string()],
        }
    }

    #[test]
    fn stamp_is_reused_for_the_same_container_and_paths() {
        let s = stamp("abc123", &["/home/user"]);
        assert!(git_bridge_is_current(
            &s,
            Some("abc123"),
            &["/home/user".into()]
        ));
    }

    #[test]
    fn stamp_is_invalidated_by_recreate_and_new_paths() {
        let s = stamp("abc123", &["/home/user"]);
        // Recreated container: same name, new id.
        assert!(!git_bridge_is_current(
            &s,
            Some("def456"),
            &["/home/user".into()]
        ));
        // Entering a new working directory adds a safe.directory path.
        assert!(!git_bridge_is_current(
            &s,
            Some("abc123"),
            &["/home/user".into(), "/home/user/Projects".into()]
        ));
        // No id recorded (podman could not be queried): must re-probe.
        assert!(!git_bridge_is_current(&s, None, &["/home/user".into()]));
        assert!(!git_bridge_is_current(&s, Some(""), &["/home/user".into()]));
    }
}
