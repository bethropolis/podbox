//! Session surface: `enter`, `exec` and `run` — everything that builds a
//! `podman exec` line and replaces the CLI process with it. The git bridge
//! lives in [`git_bridge`], the stdin watchdog in [`watchdog`], doctor in
//! [`doctor`], status in [`status`], and logs in [`logs`].

use std::ffi::OsString;
use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use anyhow::Result;

use podbox::codegen::distros;
use podbox::config::Config;
use podbox::protocol::{GuestMessage, write_frame};

pub(crate) use context::RunContext;
use context::resolve_container_workdir;

mod context;
mod doctor;
mod git_bridge;
mod logs;
mod status;
mod watchdog;

pub use logs::run_logs;
pub use status::run_status;
pub use watchdog::run_stdin_watchdog;

pub use doctor::run_doctor;
pub use doctor::try_fix_bare_memory_for_target;
use watchdog::spawn_stdin_watchdog;

/// Try to register a terminal session with the host's `socket_host`.
///
/// Opens a pidfd for the current process, connects to the host socket,
/// sends `RegisterSession` with the pidfd via `SCM_RIGHTS`, then closes
/// the connection.  Silently skips on old kernels or when `serve` is
/// not running.
fn register_session(name: &str, xdg_runtime_dir: &std::path::Path) {
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

/// Insert `--env=PATH=<container path>` before the container-name slot.
///
/// Read after the box is running: the guest daemon only writes
/// `/run/podbox/path` once it is up, so reading before `ensure_running`
/// misses it on every cold start and the session falls back to the Quadlet
/// default PATH. Returns the (possibly shifted) name slot.
fn insert_user_path(name: &str, args: &mut Vec<OsString>, name_index: usize) -> usize {
    let Some(path) = read_user_path(name) else {
        return name_index;
    };
    let index = name_index.min(args.len());
    args.splice(index..index, [OsString::from(format!("--env=PATH={path}"))]);
    name_index + 1
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

/// Render a `podman`/`journalctl` argv for dry-run output.
pub(super) fn args_to_string(args: &[OsString]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(" ")
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
    append_forwarded_env(&mut exec_args, config, env_overrides);
    append_env_args(&mut exec_args, env_overrides)?;
    let name_index = exec_args.len();
    exec_args.push(name.into());
    exec_args.push(config.container.shell.as_str().into());

    if dry_run {
        println!("podman {}", args_to_string(&exec_args));
        return Ok(());
    }
    crate::commands::ensure_running(name, false, crate::commands::DEFAULT_START_TIMEOUT_SECS)?;
    // The daemon writes /run/podbox/path on startup, so the PATH is only
    // readable once the box is running.
    let name_index = insert_user_path(name, &mut exec_args, name_index);
    git_bridge::prepare(
        ctx,
        &env.username,
        here.then_some(&workdir),
        name_index,
        &mut exec_args,
    );
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
    }
    if here {
        exec_args.push("-w".into());
        exec_args.push(resolve_container_workdir(config, env, xdg, true)?.into());
    }
    append_forwarded_env(&mut exec_args, config, env_overrides);
    append_env_args(&mut exec_args, env_overrides)?;
    let name_index = exec_args.len();
    exec_args.push(name.into());
    for a in cmd_args {
        exec_args.push(a.into());
    }

    if dry_run {
        println!("podman {}", args_to_string(&exec_args));
        return Ok(());
    }
    crate::commands::ensure_running(name, false, crate::commands::DEFAULT_START_TIMEOUT_SECS)?;
    let here_path = if here {
        exec_args
            .windows(2)
            .find(|pair| pair[0] == "-w")
            .and_then(|pair| pair[1].to_str())
            .map(str::to_string)
    } else {
        None
    };
    // The daemon writes /run/podbox/path on startup, so the PATH is only
    // readable once the box is running. Root sessions keep the image PATH.
    let name_index = if root {
        name_index
    } else {
        insert_user_path(name, &mut exec_args, name_index)
    };
    git_bridge::prepare(
        ctx,
        if root { "root" } else { &env.username },
        here_path.as_deref(),
        name_index,
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
    append_forwarded_env(&mut exec_args, config, env_overrides);
    append_env_args(&mut exec_args, env_overrides)?;
    let name_index = exec_args.len();
    exec_args.push(name.into());
    exec_args.push(app.into());
    for a in app_args {
        exec_args.push(a.into());
    }

    if dry_run {
        println!("podman {}", args_to_string(&exec_args));
        return Ok(());
    }
    crate::commands::ensure_running(name, false, crate::commands::DEFAULT_START_TIMEOUT_SECS)?;
    // The daemon writes /run/podbox/path on startup, so the PATH is only
    // readable once the box is running.
    let name_index = insert_user_path(name, &mut exec_args, name_index);
    git_bridge::prepare(ctx, &env.username, None, name_index, &mut exec_args);
    register_session(name, &env.xdg_runtime_dir);
    podbox::process::spawn_interactive_checked("podman", &exec_args)
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
