use std::ffi::OsString;
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;

use podbox::codegen::distros;
use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::podman::{ContainerState, query_state};
use podbox::protocol::{GuestMessage, write_frame};
use podbox::xdg::ResolvedXdgDirs;

mod context;
pub mod doctor;

use doctor::is_systemd_managed;
pub use doctor::run_doctor;
pub use doctor::try_fix_bare_memory_for_target;

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

fn prepare_git_bridge(
    config: &Config,
    env: &HostEnv,
    name: &str,
    git_user: &str,
    xdg: &ResolvedXdgDirs,
    here: Option<&str>,
    args: &mut Vec<OsString>,
    explicit: &[String],
) {
    if !config.integration.git_identity {
        return;
    }
    let mut identity_args = Vec::new();
    if let Some(existing) = podman_exec_output(
        name,
        git_user,
        &["git", "config", "--global", "--get-all", "safe.directory"],
    ) {
        for path in git_mount_paths(config, xdg, here, &env.username) {
            if !existing.lines().any(|entry| entry == path) {
                let _ = podman_exec_output(
                    name,
                    git_user,
                    &[
                        "git",
                        "config",
                        "--global",
                        "--add",
                        "safe.directory",
                        &path,
                    ],
                );
            }
        }
    }
    let explicit_keys: std::collections::HashSet<&str> = explicit
        .iter()
        .filter_map(|entry| entry.split_once('=').map(|(key, _)| key))
        .collect();
    for (git_key, env_keys) in [
        ("user.name", ["GIT_AUTHOR_NAME", "GIT_COMMITTER_NAME"]),
        ("user.email", ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"]),
    ] {
        let configured = podman_exec_output(name, git_user, &["git", "config", "--get", git_key])
            .is_some_and(|v| !v.is_empty());
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
            let configured_env = podman_exec_output(name, git_user, &["printenv", env_key])
                .is_some_and(|value| !value.is_empty());
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
            identity_args.push(OsString::from(format!("--env={env_key}={value}")));
        }
    }
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.splice(index..index, identity_args);
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
    here: bool,
    env_overrides: &[String],
) -> Result<()> {
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
    prepare_git_bridge(
        config,
        env,
        name,
        &env.username,
        xdg,
        here.then_some(&workdir),
        &mut exec_args,
        env_overrides,
    );
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
    config: &Config,
    xdg: &ResolvedXdgDirs,
    here: bool,
    env_overrides: &[String],
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
        config,
        env,
        name,
        if root { "root" } else { &env.username },
        xdg,
        here_path.as_deref(),
        &mut exec_args,
        env_overrides,
    );
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
    config: &Config,
    xdg: &ResolvedXdgDirs,
    env_overrides: &[String],
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
    prepare_git_bridge(
        config,
        env,
        name,
        &env.username,
        xdg,
        None,
        &mut exec_args,
        env_overrides,
    );
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
