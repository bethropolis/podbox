use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::time::Duration;

use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;

use crate::config::{Config, HostExecCwd};
use crate::process::FdCapturingReader;
use crate::protocol::{FrameRead, FrameReader, GuestMessage, HostMessage, write_frame};
use crate::socket_host::{CwdMap, PING_INTERVAL};

use super::{
    allowlisted_exec_env, exit_code_and_signal, host_exec_failed, resolve_host_exec_request,
};

/// Exactly how many descriptors a `HostExecV2` request carries: the child
/// reads fd 0 and writes fds 1 and 2. A peer that sends a different count
/// is refused rather than guessed at.
const EXEC_FD_COUNT: usize = 3;

/// How long the host-exec relay loop waits for a peer frame before it
/// reaps the child instead. Short enough for interactive Ctrl-C forwarding
/// when the host is idle.
const SIGNAL_RELAY_POLL: Duration = Duration::from_millis(100);

/// The parsed fields of a `HostExecV2` request, without its descriptors.
pub(in crate::socket_host) struct HostExecV2Request {
    pub(super) cmd: String,
    pub(super) args: Vec<String>,
    pub(super) env: Vec<(String, String)>,
    pub(super) cwd: Option<String>,
    pub(super) tty: bool,
}

impl HostExecV2Request {
    pub(in crate::socket_host) fn from_message(
        cmd: String,
        args: Vec<String>,
        env: Vec<(String, String)>,
        cwd: Option<String>,
        tty: bool,
    ) -> Self {
        Self {
            cmd,
            args,
            env,
            cwd,
            tty,
        }
    }
}

/// What a guest cwd request resolves to on the host, after the config's
/// `integration.host_exec.cwd` policy and the container's bind mappings.
enum GuestCwd {
    /// Start the child in the mapped host directory.
    Mapped(String),
    /// Start in the host home: either the policy ignores the guest path
    /// (the default) or a `map` guest path had no mapping and the policy
    /// falls back. `warn` is the stderr note the guest sees through its
    /// own descriptor in the fallback case.
    HostHome { warn: Option<String> },
    /// Run nothing: the policy refuses an unmappable guest path.
    Refused,
}

/// Resolve the configured cwd behaviour for a V2 request.
fn resolve_guest_cwd(cwd: &Option<String>, policy: HostExecCwd, cwd_map: &CwdMap) -> GuestCwd {
    let Some(guest_path) = cwd else {
        // No path carried: nothing to map or refuse, historical default.
        return GuestCwd::HostHome { warn: None };
    };
    match policy {
        HostExecCwd::Home => GuestCwd::HostHome { warn: None },
        HostExecCwd::Map => match cwd_map.translate(guest_path) {
            Some(host) => GuestCwd::Mapped(host),
            None => GuestCwd::HostHome {
                warn: Some(format!(
                    "host-exec: guest working directory '{guest_path}' has no mapping to the \
                     host; starting in the host home\n"
                )),
            },
        },
        HostExecCwd::Refuse => match cwd_map.translate(guest_path) {
            Some(host) => GuestCwd::Mapped(host),
            None => GuestCwd::Refused,
        },
    }
}

/// Handle a `HostExecV2` message: run the allowlisted command with the
/// guest's descriptors as the child's stdio, relay signals to it while it
/// runs, and report its exit.
pub(in crate::socket_host) fn handle_host_exec_v2(
    stream: &mut UnixStream,
    config: &Config,
    cwd_map: &CwdMap,
    request: HostExecV2Request,
    fds: Vec<OwnedFd>,
) -> anyhow::Result<()> {
    let HostExecV2Request {
        cmd,
        args,
        env,
        cwd,
        tty,
    } = request;
    // Descriptors are consumed by every path below: moved into the child's
    // stdio, or dropped here — dropping closes them, so a refused request
    // never leaves a stray fd describing guest state in the host process.
    // A tty request carries the slave end of the guest's own pty; the
    // descriptors still flow the same way.
    // The cwd policy is applied before anything runs: a refusal must not
    // execute a command whose working directory the admin did not allow.
    let guest_cwd = resolve_guest_cwd(&cwd, config.integration.host_exec.cwd, cwd_map);
    if let GuestCwd::Refused = &guest_cwd {
        let reason = match &cwd {
            Some(path) => format!(
                "host-exec: refusing to execute with guest working directory '{path}': \
                 no known mapping to the host (integration.host_exec.cwd = \"refuse\")"
            ),
            None => "host-exec: refusing the request".to_string(),
        };
        return host_exec_v2_refused(stream, &reason, "not_allowed");
    }
    let canonical_path = match resolve_host_exec_request(&config.integration, &cmd, &args) {
        Ok(p) => p,
        Err(msg) => return host_exec_failed(stream, &msg),
    };
    let ok_fds = match <Vec<OwnedFd> as TryInto<[OwnedFd; EXEC_FD_COUNT]>>::try_into(fds) {
        Ok(f) => f,
        Err(_) => {
            return host_exec_v2_refused(
                stream,
                &format!(
                    "host-exec: the request must carry exactly {EXEC_FD_COUNT} descriptors (stdin, stdout, stderr)"
                ),
                "bad_request",
            );
        }
    };

    // Security model: these descriptors arrived from the guest over the
    // host connection and are guest-side pipes, files and terminals —
    // plumbing between the guest's own processes. With them as its stdio
    // the host child reads and writes what the guest asked it to, and the
    // host gains no access it did not already have for running the
    // allowlisted command. The delivery copies are consumed by the child's
    // stdio and dropped right after spawn, so fds are closed here rather
    // than leaked into later requests.
    let [child_stdin, child_stdout, child_stderr] = ok_fds;

    // Only the allowlisted keys reach through; the child otherwise runs in
    // the host daemon's own environment. The cwd policy decides where the
    // child starts: `home` (the default) starts in the host home; `map`
    // starts it in the host directory the guest cwd is bound from, with a
    // stderr warning (through the guest's own descriptor, so it reaches
    // attached-stdio pipes and ptys alike) when the guest path has no
    // mapping; `refuse` never got this far.
    let mut command = Command::new(&canonical_path);
    command.args(&args);
    for (key, value) in allowlisted_exec_env(&env) {
        command.env(key, value);
    }
    match guest_cwd {
        GuestCwd::Mapped(host_dir) => {
            tracing::info!("host-exec: starting child in mapped host cwd {host_dir:?}");
            command.current_dir(host_dir);
        }
        GuestCwd::HostHome { warn } => {
            // The fallback warning goes through the guest's own stderr
            // descriptor, before any child output: no protocol frame is
            // needed and both the pipe and pty session shapes deliver it.
            if let Some(text) = warn {
                // Writing through nix keeps the fd semantics explicit and
                // avoids depending on `OwnedFd`'s io traits age.
                nix::unistd::write(&child_stderr, text.as_bytes()).ok();
            }
            command.current_dir(cwd_map.host_home());
        }
        // Unreachable: a refusal returned above, and there is no path
        // that resolves to `Refused` twice.
        GuestCwd::Refused => {}
    }
    // Own process group for pipe sessions: a relayed SIGINT/SIGTERM reaches
    // the child and nothing above it, unlike a child sharing the daemon's
    // group. A tty child gets its own session instead (ties the terminal
    // to it and gives it a group of its pid anyway), and must NOT be made
    // a group leader here, or the session creation fails.
    if tty {
        give_the_child_its_tty(&mut command, &child_stdin);
    } else {
        command.process_group(0);
    }
    let child = command
        .stdin(Stdio::from(child_stdin))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr))
        .spawn();

    let mut child = match child {
        Ok(c) => c,
        Err(e) => {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!("host-exec: '{cmd}' not found in allowlist path or host $PATH")
            } else {
                format!("host-exec: failed to execute '{cmd}': {e}")
            };
            return host_exec_failed(stream, &msg);
        }
    };

    // Spawn succeeded: the stdio descriptors are in the child now, and the
    // host's own copies are closed by `Stdio`'s drop after `spawn` — no
    // leaked fd holds the guest's pipes open past the child's lifetime.
    // The per-entry timeout kills only this child's group, and is reported
    // as a signal death like any other kill; a session with no configured
    // timeout is never time-limited here, and neither is a tty session —
    // an interactive child outliving its request is correct behaviour.
    let timeout = if tty {
        None
    } else {
        config
            .integration
            .host_exec
            .resolve(&cmd)
            .and_then(|entry| entry.timeout_secs())
            .map(Duration::from_secs)
    };
    run_child_with_signal_relay(stream, &mut child, timeout)
}

/// Register the tty child's session setup on the command: `setsid` makes
/// the child a session leader — its own process group, so the relay's
/// `killpg` target is exactly this child — and `TIOCSCTTY` on the stdin
/// it inherits makes the guest's pty its controlling terminal, so the
/// kernel exercises the terminal's own Ctrl-C, job control and
/// window-size accounting directly on the child.
///
/// Must not be combined with `process_group(0)`: a group leader cannot
/// `setsid`, and the session creation would fail before the exec.
fn give_the_child_its_tty(command: &mut Command, ctty: &OwnedFd) {
    let ctty_fd = ctty.as_raw_fd();
    // SAFETY: `pre_exec` runs the closure between fork and exec, where
    // only async-signal-safe code may touch the world. `setsid` and
    // `ioctl` are raw syscalls (nix and libc wrap them in direct calls,
    // no allocation, no locks), the descriptor used is the slave the
    // child inherits as stdin, and a failure aborts the spawn with an
    // error instead of leaving a half-configured child behind.
    //
    // The workspace denies `unsafe` wholesale; this session-leader
    // kernel setup has no safe wrapper, as documented above.
    #[allow(unsafe_code)]
    // SAFETY: see above.
    unsafe {
        command.pre_exec(move || {
            nix::unistd::setsid().map_err(std::io::Error::from)?;
            if nix::libc::ioctl(ctty_fd, nix::libc::TIOCSCTTY, 0) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

/// Refuse a `HostExecV2` request before anything runs: a typed `Error`
/// frame, with the request's descriptors (if any) dropped by the caller of
/// this handler not being run.
fn host_exec_v2_refused(stream: &mut UnixStream, reason: &str, code: &str) -> anyhow::Result<()> {
    write_frame(
        stream,
        &HostMessage::Error {
            reason: reason.to_string(),
            code: Some(code.to_string()),
        },
    )?;
    Ok(())
}

/// While a V2 child runs, the connection carries relay messages only:
/// poll the peer for `HostExecSignal` frames with a short timeout and
/// reap the child in the gaps. On any disconnect or foreign frame with
/// attached descriptors, stray descriptors are closed, and the child is
/// killed and reaped so nothing outlives its session.
///
/// `timeout` is the allowlist entry's configured execution timeout: once
/// it expires the child's process group is SIGKILLed and the death is
/// reported like any signal death (`128 + 9`, signal 9). `None` — the
/// default for entries without `timeout` — never time-limits the child.
fn run_child_with_signal_relay(
    stream: &mut UnixStream,
    child: &mut std::process::Child,
    timeout: Option<Duration>,
) -> anyhow::Result<()> {
    let pgid = Pid::from_raw(child.id().cast_signed());
    let mut reader = FrameReader::new();
    stream.set_read_timeout(Some(SIGNAL_RELAY_POLL))?;
    let deadline = timeout.map(|t| std::time::Instant::now() + t);

    let mut exited: Option<std::process::ExitStatus> = None;
    while exited.is_none() {
        // A peer that attaches descriptors to a relay frame misuses the
        // session: capture and close them instead of leaving them for a
        // later (different) request.
        let mut stray: Vec<OwnedFd> = Vec::new();
        let read = reader.read(&mut FdCapturingReader::new(stream, &mut stray));
        drop(stray);
        match read {
            Ok(FrameRead::Frame(bytes)) => {
                if let Ok(GuestMessage::HostExecSignal { sig }) =
                    serde_json::from_slice::<GuestMessage>(&bytes)
                {
                    relay_signal(pgid, sig);
                } else {
                    tracing::debug!("host-exec: ignoring non-relay frame while child runs");
                }
            }
            Ok(FrameRead::Eof) => {
                // The shim died without reporting: nothing is left to
                // write output to, so the child should not keep running.
                tracing::warn!("host-exec: peer closed the connection; killing the child");
                let _ = kill(pgid, Signal::SIGKILL);
                child.wait()?;
                stream.set_read_timeout(Some(PING_INTERVAL))?;
                return Ok(());
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                match child.try_wait() {
                    Ok(Some(status)) => exited = Some(status),
                    Ok(None) => {
                        // A child that exits inside the poll gap is
                        // reaped normally: the deadline never kills a
                        // command that finished on its own. Only a live
                        // child past its deadline gets the kill.
                        if deadline.is_some_and(|d| std::time::Instant::now() >= d) {
                            tracing::warn!(
                                "host-exec: allowlist entry timeout expired; killing the child"
                            );
                            let _ = killpg(pgid, Signal::SIGKILL);
                        }
                    }
                    // A reap failure means the child is gone and unknown:
                    // fail loudly, the caller treats it as host drift.
                    Err(e) => {
                        stream.set_read_timeout(Some(PING_INTERVAL))?;
                        return Err(anyhow::Error::from(e));
                    }
                }
            }
            Err(e) => {
                tracing::warn!("host-exec: relay read failed: {e}");
                let _ = kill(pgid, Signal::SIGKILL);
                let _ = child.wait();
                stream.set_read_timeout(Some(PING_INTERVAL))?;
                return Err(anyhow::Error::from(e));
            }
        }
    }

    stream.set_read_timeout(Some(PING_INTERVAL))?;
    let (exit_code, signal) = exit_code_and_signal(exited.as_ref().unwrap());
    write_frame(stream, &HostMessage::HostExecDone { exit_code, signal })?;
    Ok(())
}

/// Map a guest-relayed signal number to the child's process group. Only
/// the shim's known set is relayed (SIGHUP, SIGINT, SIGTERM, SIGWINCH):
/// `HostExecSignal` is a relay channel for terminal signals, not an
/// arbitrary host API, so any other number is dropped with a log. Relay
/// errors are debug-logged, since a child that has just exited races any
/// late `HostExecSignal` with an unavoidable `ESRCH`.
fn relay_signal(pgid: Pid, sig: i32) {
    let signal = match sig {
        1 => Signal::SIGHUP,
        2 => Signal::SIGINT,
        15 => Signal::SIGTERM,
        28 => Signal::SIGWINCH,
        other => {
            tracing::debug!("host-exec: ignoring unforwardable signal {other}");
            return;
        }
    };
    // The child leads its own process group (`process_group(0)` at
    // spawn), so the relay reaches only it — not the daemon's group —
    // however the guest shim was itself signalled.
    match killpg(pgid, signal) {
        Err(nix::errno::Errno::ESRCH) => {}
        Err(e) => {
            tracing::debug!("host-exec: relay of signal {} failed: {e}", signal as i32)
        }
        Ok(()) => {
            tracing::debug!(
                "host-exec: relayed signal {} to the child's group",
                signal as i32
            )
        }
    }
}
