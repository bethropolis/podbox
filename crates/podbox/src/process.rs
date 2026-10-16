use std::ffi::OsString;
use std::io::{self, BufRead, IoSlice, IoSliceMut, Write};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::process::{Command, ExitStatus, Output};
use std::time::Duration;

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::signal::{Signal, kill};
use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, recvmsg, sendmsg};
use nix::unistd::Pid;
/// Build a `Vec<OsString>` from a slice of `&str`/`&String` literals.
pub fn args<S: AsRef<str>>(items: &[S]) -> Vec<OsString> {
    items.iter().map(|s| OsString::from(s.as_ref())).collect()
}

/// Replace the current process with the given binary and arguments.
///
/// Uses `CommandExt::exec()` so the shell gets a real TTY.
/// On success this function never returns; on failure it returns an error.
pub fn exec_replace(bin: &str, args: &[OsString]) -> anyhow::Error {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    let err = cmd.exec();
    anyhow::Error::from(err).context(format!("failed to exec {bin}"))
}

/// Run a command, capturing stdout and stderr.
pub fn run_piped(bin: &str, args: &[OsString]) -> anyhow::Result<Output> {
    let output = Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?
        .wait_with_output()?;
    Ok(output)
}

/// Spawn a command attached to the current terminal.
///
/// Returns the exit status without judging it. Mutating callers should use
/// [`spawn_interactive_checked`]; this stays for callers that pass the status
/// on (for example `enter`, which forwards the shell's exit code).
pub fn spawn_interactive(bin: &str, args: &[OsString]) -> anyhow::Result<ExitStatus> {
    let status = Command::new(bin).args(args).status()?;
    Ok(status)
}

/// Run a command and fail if it exits non-zero.
///
/// The error names the program, its shell-quoted arguments, and the trimmed
/// stderr, so a failed `podman` call is visible in the message rather than
/// reported as success.
pub fn run_checked(bin: &str, args: &[OsString]) -> anyhow::Result<Output> {
    let output = run_piped(bin, args)?;
    if !output.status.success() {
        anyhow::bail!(
            "{} failed ({}): {}",
            describe(bin, args),
            status_text(output.status),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(output)
}

/// Spawn an interactive command and fail if it exits non-zero.
///
/// Stdio is inherited, so the child's own diagnostics are already on the
/// terminal. The returned error is [`PodboxError::ChildExit`], which carries
/// the child's code to the process exit status without adding a second message.
pub fn spawn_interactive_checked(bin: &str, args: &[OsString]) -> anyhow::Result<()> {
    let status = spawn_interactive(bin, args)?;
    if status.success() {
        return Ok(());
    }
    Err(crate::error::PodboxError::ChildExit(exit_code_of(status)).into())
}

/// Exit code to report for a finished child.
///
/// A normal exit gives its code. A signal gives the shell convention `128 + n`.
pub fn exit_code_of(status: ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt;
    status
        .code()
        .or_else(|| status.signal().map(|s| 128 + s))
        .unwrap_or(1)
}

/// Shell-quoted `bin arg...` for error messages.
fn describe(bin: &str, args: &[OsString]) -> String {
    std::iter::once(bin.to_string())
        .chain(args.iter().map(|a| shell_quote(&a.to_string_lossy())))
        .collect::<Vec<_>>()
        .join(" ")
}

fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./:=@%+,".contains(c))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

fn status_text(status: ExitStatus) -> String {
    match status.code() {
        Some(code) => format!("exit {code}"),
        None => format!("killed by signal {}", exit_code_of(status) - 128),
    }
}

/// Run a command while teeing stdout+stderr into a log file.
///
/// Every child line is appended to `log` as produced. When `mirror` is true
/// (verbose mode) lines are also echoed to our stdout so long operations
/// stream live; otherwise output is captured silently and the caller shows a
/// tail from the log on failure.
pub fn run_with_log(
    bin: &str,
    args: &[OsString],
    log: &mut std::fs::File,
    mirror: bool,
) -> anyhow::Result<ExitStatus> {
    let mut child = Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| anyhow::Error::new(e).context(format!("failed to execute {bin}")))?;

    let mut out = child.stdout.take().expect("stdout piped");
    let mut err = child.stderr.take().expect("stderr piped");
    let mut log_out = log.try_clone()?;
    let mut log_err = log.try_clone()?;

    let t_out = std::thread::spawn(move || tee_stream(&mut out, &mut log_out, false, mirror));
    let t_err = std::thread::spawn(move || tee_stream(&mut err, &mut log_err, true, mirror));

    let status = child.wait()?;
    let _ = t_out.join();
    let _ = t_err.join();
    let _ = log.flush();
    Ok(status)
}

/// Copy one child pipe into the log until EOF; optionally mirror to stderr.
fn tee_stream<R: io::Read, W: io::Write>(src: &mut R, dst: &mut W, to_stderr: bool, mirror: bool) {
    let mut reader = io::BufReader::new(src);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                if dst.write_all(line.as_bytes()).is_err() {
                    break;
                }
                if mirror {
                    let _ = if to_stderr {
                        io::stderr().write_all(line.as_bytes())
                    } else {
                        io::stdout().write_all(line.as_bytes())
                    };
                }
            }
        }
    }
}

/// Run a command with a timeout, capturing stdout and stderr.
///
/// The child process receives SIGKILL after `timeout` if it has not exited.
pub fn run_piped_timeout(
    bin: &str,
    args: &[OsString],
    timeout: Duration,
) -> anyhow::Result<Output> {
    let child = Command::new(bin)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    wait_child_timeout(child, timeout)
}

/// Run an interactive command with a timeout (SIGTERM, then SIGKILL).
///
/// Unlike `run_piped_timeout`, this sends SIGTERM first with a 5-second
/// grace period before SIGKILL, giving well-behaved processes a chance
/// to clean up.
pub fn spawn_interactive_timeout(
    bin: &str,
    args: &[OsString],
    timeout: Duration,
) -> anyhow::Result<ExitStatus> {
    let mut cmd = Command::new(bin);
    cmd.args(args);
    spawn_interactive_timeout_cmd(&mut cmd, timeout)
}

/// Same as [`spawn_interactive_timeout`] but takes a pre-built command so
/// callers can set stdin/stdout (e.g. null stdin for one-shot exec).
pub fn spawn_interactive_timeout_with_stdio(
    cmd: &mut Command,
    timeout: Duration,
) -> anyhow::Result<ExitStatus> {
    spawn_interactive_timeout_cmd(cmd, timeout)
}

fn spawn_interactive_timeout_cmd(
    cmd: &mut Command,
    timeout: Duration,
) -> anyhow::Result<ExitStatus> {
    let mut child = cmd.spawn()?;
    let pid = Pid::from_raw(child.id().cast_signed());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if rx.recv_timeout(timeout).is_err() {
            let _ = kill(pid, Signal::SIGTERM);
            std::thread::sleep(Duration::from_secs(5));
            let _ = kill(pid, Signal::SIGKILL);
        }
    });
    let status = child.wait()?;
    let _ = tx.send(());
    Ok(status)
}

/// Wait for a child process to complete, enforcing a timeout via SIGKILL.
pub fn wait_child_timeout(child: std::process::Child, timeout: Duration) -> anyhow::Result<Output> {
    let pid = Pid::from_raw(child.id().cast_signed());
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        if rx.recv_timeout(timeout).is_err() {
            let _ = kill(pid, Signal::SIGKILL);
        }
    });
    let output = child.wait_with_output()?;
    let _ = tx.send(());
    Ok(output)
}

/// Open a pidfd for a given PID (Linux 5.3+).
///
/// Returns `Err` on old kernels or when the PID does not exist.
pub fn open_pidfd(pid: i32) -> io::Result<OwnedFd> {
    let pid = rustix::process::Pid::from_raw(pid)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid PID"))?;
    rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty()).map_err(io::Error::from)
}

/// Send a raw file descriptor over a connected Unix stream via `SCM_RIGHTS`.
///
/// Sends one dummy byte alongside the descriptor so the receiver can detect EOF.
/// Retries on `EINTR` to prevent spurious session drops.
pub fn send_fd(stream: &UnixStream, fd: RawFd) -> io::Result<()> {
    let raw_fd = stream.as_raw_fd();
    let cmsg = ControlMessage::ScmRights(&[fd]);
    let iov = [IoSlice::new(&[0u8])];
    loop {
        match sendmsg::<()>(raw_fd, &iov, &[cmsg], MsgFlags::empty(), None) {
            Ok(_) => return Ok(()),
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => return Err(io::Error::from(e)),
        }
    }
}

/// Mark a descriptor close-on-exec so children spawned later (host-exec
/// commands, `xdg-open`, `wl-copy`) never inherit it.
///
/// Use this right after adopting a descriptor from outside the process,
/// such as systemd's socket-activation fd 3: without the flag a
/// long-running child would hold the listening socket open.
pub fn set_cloexec(fd: BorrowedFd) -> io::Result<()> {
    fcntl(fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))
        .map(|_| ())
        .map_err(io::Error::from)
}

/// Adopt a raw descriptor received via `SCM_RIGHTS` into an owned handle.
///
/// The kernel duplicates ancillary-data descriptors into the receiving
/// process on delivery, transferring their single reference — whoever
/// calls this must adopt them (here) or close them (leak). This is the
/// single audit point for that ownership transfer in this crate.
pub fn adopt_scm_fd(raw: RawFd) -> OwnedFd {
    // SAFETY: `raw` arrived via SCM_RIGHTS over a Unix socket; the kernel
    // duplicated it into this process on delivery and we now hold exactly
    // one reference to a valid open descriptor. No safe wrapper exists for
    // adopting externally-sourced fds.
    #[allow(unsafe_code)]
    unsafe {
        OwnedFd::from_raw_fd(raw)
    }
}

/// Receive a file descriptor from a connected Unix stream via `SCM_RIGHTS`.
///
/// Returns `None` when the sender has closed the connection (EOF), or when
/// the message carries no descriptor. Every descriptor the kernel delivers
/// is adopted into an `OwnedFd` straight away: the first is returned and
/// any extras are dropped on the spot, so a peer that sends more than
/// asked-for cannot leak descriptors into this process. Delivery requests
/// `MSG_CMSG_CLOEXEC`, so the adopted descriptors also stay out of spawned
/// children. Retries on `EINTR` to prevent spurious session drops.
pub fn recv_fd(stream: &UnixStream) -> io::Result<Option<OwnedFd>> {
    let raw_fd = stream.as_raw_fd();
    let mut buf = [0u8; 1];
    let mut iov = [IoSliceMut::new(&mut buf)];
    let mut cmsg_buf = vec![0u8; 256];
    let msg = loop {
        match recvmsg::<()>(
            raw_fd,
            &mut iov,
            Some(&mut cmsg_buf),
            MsgFlags::MSG_CMSG_CLOEXEC,
        ) {
            Ok(m) => break m,
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => return Err(io::Error::from(e)),
        }
    };

    if msg.bytes == 0 {
        return Ok(None);
    }

    let mut owned: Vec<OwnedFd> = Vec::new();
    if let Ok(cmsgs) = msg.cmsgs() {
        for cmsg in cmsgs {
            if let ControlMessageOwned::ScmRights(fds) = cmsg {
                owned.extend(fds.into_iter().map(adopt_scm_fd));
            }
        }
    }
    let mut owned = owned.into_iter();
    Ok(owned.next())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsFd;

    #[test]
    fn args_builds_osstring_vec() {
        let v = args(&["foo", "bar", "baz"]);
        assert_eq!(v.len(), 3);
        assert_eq!(v[0], "foo");
        assert_eq!(v[1], "bar");
        assert_eq!(v[2], "baz");
    }

    #[test]
    fn run_checked_fails_on_nonzero_and_names_stderr() {
        let err = run_checked("sh", &args(&["-c", "echo boom >&2; exit 3"])).unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("exit 3"), "missing status: {text}");
        assert!(text.contains("boom"), "missing stderr: {text}");
    }

    #[test]
    fn run_checked_succeeds_on_zero() {
        assert!(run_checked("true", &[]).is_ok());
    }

    #[test]
    fn spawn_interactive_checked_reports_child_exit_code() {
        let err = spawn_interactive_checked("sh", &args(&["-c", "exit 3"])).unwrap_err();
        let code = err
            .downcast_ref::<crate::error::PodboxError>()
            .map(|e| matches!(e, crate::error::PodboxError::ChildExit(3)));
        assert_eq!(code, Some(true), "expected ChildExit(3), got {err:#}");
    }

    #[test]
    fn spawn_interactive_checked_passes_success_through() {
        assert!(spawn_interactive_checked("true", &[]).is_ok());
    }

    #[test]
    fn signal_exit_maps_to_shell_convention() {
        let status = Command::new("sh")
            .args(["-c", "kill -TERM $$"])
            .status()
            .unwrap();
        assert_eq!(exit_code_of(status), 143);
    }

    #[test]
    fn shell_quote_leaves_plain_words_and_quotes_the_rest() {
        assert_eq!(shell_quote("dev-box_1"), "dev-box_1");
        assert_eq!(shell_quote("a b"), "'a b'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn args_accepts_mixed_types() {
        let s = String::from("hello");
        let v = args(&["a", &s, "c"]);
        assert_eq!(v[1], "hello");
    }

    fn fd_targets() -> Vec<std::path::PathBuf> {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| std::fs::read_link(e.path()).ok())
            .collect()
    }

    fn has_cloexec(fd: BorrowedFd) -> bool {
        let flags = fcntl(fd, FcntlArg::F_GETFD).unwrap();
        FdFlag::from_bits_retain(flags).contains(FdFlag::FD_CLOEXEC)
    }

    #[test]
    fn set_cloexec_marks_the_descriptor_close_on_exec() {
        let (a, _b) = UnixStream::pair().unwrap();
        fcntl(a.as_fd(), FcntlArg::F_SETFD(FdFlag::empty())).unwrap();
        assert!(!has_cloexec(a.as_fd()));
        set_cloexec(a.as_fd()).unwrap();
        assert!(has_cloexec(a.as_fd()));
    }

    #[test]
    fn recv_fd_returns_one_and_closes_the_extras() {
        // Spares live in a unique temp file, so counting descriptors that
        // point at it is unaffected by whatever other tests have open.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let spares: Vec<std::fs::File> = (0..3).map(|_| tmp.reopen().unwrap()).collect();
        let raws: Vec<RawFd> = spares.iter().map(|f| f.as_raw_fd()).collect();
        let target = tmp.path().to_path_buf();
        let count_for = |p: &std::path::Path| fd_targets().iter().filter(|t| *t == p).count();
        // The temp handle plus its three reopens.
        assert_eq!(count_for(&target), 4);

        let (tx, rx) = UnixStream::pair().unwrap();
        sendmsg::<()>(
            tx.as_raw_fd(),
            &[IoSlice::new(&[0u8])],
            &[ControlMessage::ScmRights(&raws)],
            MsgFlags::empty(),
            None,
        )
        .unwrap();
        let got = recv_fd(&rx).unwrap().expect("expected a descriptor");
        // Delivered close-on-exec, so children never inherit it.
        assert!(has_cloexec(got.as_fd()));
        // Three sent, one returned: the two extras are already closed.
        assert_eq!(count_for(&target), 5);
        drop(got);
        assert_eq!(count_for(&target), 4);
    }
}
