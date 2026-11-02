use std::io::Write;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};
use signal_hook::iterator::Signals;

use crate::protocol::{
    CAP_HOST_EXEC, CAP_HOST_EXEC_FDS, CAP_HOST_EXEC_STREAM, FrameRead, FrameReader, GuestMessage,
    HostMessage, read_frame, write_frame,
};
use crate::socket::negotiate_at_with_optional;

/// The signals the shim traps while a fd-passing exec runs and relays to
/// the host as `HostExecSignal` frames. The host forwards them to the
/// child's own process group; nothing here reaches the shim's caller.
const FORWARDED_SIGNALS: &[i32] = &[SIGHUP, SIGINT, SIGTERM, SIGWINCH];

/// How long a fd-passing wait loop sleeps before forwarding signals.
/// Interactive Ctrl-C wants the relay fast, but the loop must also reap
/// the child scheduler-subscribed polling.
const SIGNAL_RELAY_POLL: Duration = Duration::from_millis(100);

/// The same interval as a `poll(2)` timeout. Kept separate because
/// `PollTimeout` takes integer milliseconds, not a `Duration`.
const SIGNAL_RELAY_POLL_MS: u16 = 100;

/// How long the relay waits for the last pty bytes after the host reports
/// the child's exit. The child is already reaped by then; this only lets
/// the kernel hand over bytes it buffered before the slave closed.
const TTY_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// Env keys the guest may ask the host to export to a child. Sent only if
/// actually set somewhere; the host enforces the same fixed list again.
const EXEC_ENV_KEYS: &[&str] = &["TERM", "LANG", "COLORTERM"];

pub fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("host-exec: usage: host-exec <command> [args...]");
        std::process::exit(1);
    }
    run_as_command(&args[1], &args[2..]);
}

pub fn run_as_command(cmd: &str, args: &[String]) {
    // Negotiation happens before the request is written; a refused capability
    // is reported here rather than by the host dropping the connection.
    // Streaming output is requested opportunistically: old hosts refuse the
    // extra capability and the host answers with buffered frames, which the
    // same response loop below already understands.
    let socket_path = crate::socket::host_socket_path().unwrap_or_else(|e| {
        eprintln!("host-exec: {e}");
        std::process::exit(1);
    });
    let container = crate::socket::container_name().unwrap_or_else(|e| {
        eprintln!("host-exec: {e}");
        std::process::exit(1);
    });
    let (mut stream, accepted) = negotiate_at_with_optional(
        &socket_path,
        &container,
        &[CAP_HOST_EXEC],
        &[CAP_HOST_EXEC_STREAM, CAP_HOST_EXEC_FDS],
    )
    .unwrap_or_else(|e| {
        eprintln!("host-exec: {e}");
        std::process::exit(1);
    });

    // Old hosts do not know the fd capability (it lands in the rejected
    // list), so they keep serving the historical request shape untouched.
    let code = if accepted.contains(&CAP_HOST_EXEC_FDS.to_string()) {
        run_with_fds(&mut stream, cmd, args)
    } else {
        let msg = GuestMessage::HostExec {
            cmd: cmd.to_string(),
            args: args.to_vec(),
        };
        if write_frame(&mut stream, &msg).is_err() {
            std::process::exit(1);
        }
        let stdout = std::io::stdout();
        let stderr = std::io::stderr();
        pump_responses(&mut stream, &mut stdout.lock(), &mut stderr.lock())
    };
    std::process::exit(code);
}

/// Run the request with the stdio flowing directly between the guest and
/// the host child: our own fds 0..2 (temperately substituted by /dev/null
/// where closed) are attached to the frame, and the child's exit comes
/// back as one `HostExecDone`. Nothing is buffered here — output bytes
/// take the guest's own pipes, not the protocol.
///
/// With our stdout on a terminal the request instead allocates a pty and
/// hands the host its slave end: the child keeps the terminal interaction
/// a shell session expects, and this shim relays bytes between the
/// caller's terminal and the master until the exit report arrives.
fn run_with_fds(stream: &mut UnixStream, cmd: &str, args: &[String]) -> i32 {
    let tty = {
        use std::io::IsTerminal;
        std::io::stdout().is_terminal()
    };
    let (triple, master) = if tty {
        // The pty's slave carries the child's stdio; the master stays on
        // this side of the session until the exit report arrives.
        match tty_triple() {
            Ok((triple, master)) => (triple, Some(master)),
            Err(e) => {
                eprintln!("host-exec: {e}");
                return 1;
            }
        }
    } else {
        match stdio_triple() {
            Ok(t) => (t, None),
            Err(e) => {
                eprintln!("host-exec: {e}");
                return 1;
            }
        }
    };
    let msg = GuestMessage::HostExecV2 {
        cmd: cmd.to_string(),
        args: args.to_vec(),
        env: allowlisted_own_env(),
        // Carried for the host phase that translates a container directory
        // through the volume mappings; sending our cwd costs nothing.
        cwd: std::env::current_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
        tty,
    };
    let mut wire = Vec::new();
    if write_frame(&mut wire, &msg).is_err() {
        return 1;
    }
    let raws: Vec<i32> = triple.iter().map(AsRawFd::as_raw_fd).collect();
    match crate::protocol::sysutil::send_bytes_with_fds(stream, &[&wire], &raws) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("host-exec: {e}");
            return 1;
        }
    }
    // The host now owns the copies that matter. Close ours so a pty master
    // sees EIO when the child exits and a pipe's reader sees EOF without
    // waiting for this process to return.
    drop(triple);
    match master {
        Some(master) => relay_tty_session(stream, &master),
        None => wait_for_done_with_signals(stream),
    }
}

/// Which of the child's stdio descriptors this process should hand over:
/// the guest's own stdin/stdout/stderr duplicated into owned handles, or
/// a `/dev/null` opened read-write where the owning fd is closed entirely
/// (`0<&-` style shells), so the request always carries the same three.
fn stdio_triple() -> std::io::Result<[OwnedFd; 3]> {
    Ok([
        dup_or_devnull(std::io::stdin().as_fd())?,
        dup_or_devnull(std::io::stdout().as_fd())?,
        dup_or_devnull(std::io::stderr().as_fd())?,
    ])
}

/// Allocate the pty a tty session runs in, and return its slave end
/// three times (the child's stdin, stdout and stderr) plus the master
/// the caller must keep open until the session ends. The caller's own
/// terminal — this shim is only asked for a pty when its stdout is one —
/// supplies the initial termios and window size, so the child starts
/// with the interaction state and rows/columns the caller already sees.
fn tty_triple() -> std::io::Result<([OwnedFd; 3], OwnedFd)> {
    let mut master = None;
    let triple = tty_triple_for(std::io::stdout(), &mut master)?;
    let master = master.expect("the allocator always sets the master");
    Ok((triple, master))
}

/// The allocator proper, parameterised over the terminal whose state the
/// pty starts with and a slot capturing the master. `tty_triple` fixes
/// them to our stdout; tests allocate with a pty of their own.
///
/// `TIOCGPTPEER` opens the slave relative to the master, so there is no
/// naming race between `unlockpt` and the slave's open.
fn tty_triple_for(
    source: impl std::os::fd::AsFd,
    master_slot: &mut Option<OwnedFd>,
) -> std::io::Result<[OwnedFd; 3]> {
    use rustix::pty::{OpenptFlags, ioctl_tiocgptpeer, openpt, unlockpt};

    let master = openpt(
        // NOCTTY: no terminal should be claimed for *this* process — only
        // the host child, via its own session, gets the slave as ctty.
        // CLOEXEC: the master belongs to the shim, not anything it runs.
        OpenptFlags::RDWR | OpenptFlags::NOCTTY | OpenptFlags::CLOEXEC,
    )?;
    unlockpt(&master)?;
    let slave = ioctl_tiocgptpeer(&master, OpenptFlags::RDWR)?;

    // Termios and window size come from the source descriptor (a
    // terminal on the interactive path by construction). Missing values
    // SKIP silently: a pty with the driver defaults is usable, while
    // failing the whole session over a row count is not. `tcgetattr`
    // copies the termios wholesale from the source — INTR keys,
    // processing state, echo — without reconstructing any of it here.
    if let Ok(t) = rustix::termios::tcgetattr(source.as_fd()) {
        rustix::termios::tcsetattr(&slave, rustix::termios::OptionalActions::Now, &t)?;
    }
    if let Ok(ws) = rustix::termios::tcgetwinsize(source.as_fd()) {
        rustix::termios::tcsetwinsize(&slave, ws)?;
    }

    // One slave per stdio slot, as every request carries three fds.
    let slave2 = nix::unistd::dup(&slave).map_err(std::io::Error::from)?;
    let slave3 = nix::unistd::dup(&slave).map_err(std::io::Error::from)?;
    *master_slot = Some(master);
    Ok([slave, slave2, slave3])
}

fn dup_or_devnull(fd: BorrowedFd<'_>) -> std::io::Result<OwnedFd> {
    match nix::fcntl::fcntl(fd, nix::fcntl::FcntlArg::F_GETFD) {
        Ok(_) => {
            // Owned descriptor: safe to duplicate — the original stays
            // with this process untouched, and the copy outlives the
            // request frame being sent.
            nix::unistd::dup(fd).map_err(std::io::Error::from)
        }
        Err(nix::errno::Errno::EBADF) => std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/null")
            .map(std::os::fd::OwnedFd::from),
        Err(e) => Err(std::io::Error::from(e)),
    }
}

/// Relay a tty session between the caller's terminal and the child's pty.
///
/// The host child writes into the pty slave, so somebody has to read the
/// master and put those bytes on the caller's stdout; likewise the caller's
/// stdin has to be written into the master. The caller's terminal is put in
/// raw mode while the relay runs: the child's pty already carries the
/// original termios and therefore owns echo and line editing. Leaving the
/// caller's terminal cooked would echo every byte a second time.
fn relay_tty_session(stream: &mut UnixStream, master: &OwnedFd) -> i32 {
    let input = match dup_or_devnull(std::io::stdin().as_fd()) {
        Ok(fd) => fd,
        Err(e) => {
            eprintln!("host-exec: {e}");
            return 1;
        }
    };
    let output = match dup_or_devnull(std::io::stdout().as_fd()) {
        Ok(fd) => fd,
        Err(e) => {
            eprintln!("host-exec: {e}");
            return 1;
        }
    };
    let mut stderr = std::io::stderr();
    relay_tty(stream, master, &input, &output, &mut stderr)
}

/// The relay proper, parameterised over the local input/output descriptors
/// and the error sink so tests can stand in for the shim's terminal.
//
// One event loop is the whole point: stdin, the pty master and the
// protocol socket must be serviced together, with signals between ticks.
#[allow(clippy::too_many_lines)]
fn relay_tty(
    stream: &mut UnixStream,
    master: &OwnedFd,
    input: &OwnedFd,
    output: &OwnedFd,
    stderr: &mut dyn Write,
) -> i32 {
    use nix::errno::Errno;
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use rustix::termios::{
        OptionalActions, SpecialCodeIndex, tcgetattr, tcgetwinsize, tcsetattr, tcsetwinsize,
    };

    let saved = tcgetattr(input).ok();
    let veof = saved.as_ref().map_or(0x04, |termios| {
        termios.special_codes[SpecialCodeIndex::VEOF]
    });
    let mut signals = match Signals::new(FORWARDED_SIGNALS.iter().copied()) {
        Ok(s) => Some(s),
        Err(e) => {
            // The pty's own line discipline still delivers keyboard
            // signals; only externally-sent ones are lost. Keep relaying.
            eprintln!("host-exec: signal tracking unavailable: {e}");
            None
        }
    };
    let restore = TerminalRestore {
        fd: input,
        saved: saved.clone(),
    };
    if let Some(termios) = &saved {
        let mut raw = termios.clone();
        raw.make_raw();
        if tcsetattr(input, OptionalActions::Now, &raw).is_err() {
            drop(restore);
            eprintln!("host-exec: failed to put the terminal in raw mode");
            return 1;
        }
    }
    if stream.set_read_timeout(Some(SIGNAL_RELAY_POLL)).is_err() {
        drop(restore);
        eprintln!("host-exec: failed to set the session socket timeout");
        return 1;
    }

    let poll_timeout = PollTimeout::from(SIGNAL_RELAY_POLL_MS);
    let mut reader = FrameReader::new();
    let mut input_open = true;
    let mut master_open = true;
    let mut output_open = true;
    let mut host_stdout = Vec::new();
    let mut host_stderr = Vec::new();
    let mut exit: Option<i32> = None;
    let mut disconnected = false;

    while exit.is_none() && !disconnected {
        let mut poll_error = None;
        let (input_ready, master_ready, stream_ready) = {
            let mut pollfds = [
                PollFd::new(
                    input.as_fd(),
                    if input_open {
                        PollFlags::POLLIN
                    } else {
                        PollFlags::empty()
                    },
                ),
                PollFd::new(
                    master.as_fd(),
                    if master_open {
                        PollFlags::POLLIN
                    } else {
                        PollFlags::empty()
                    },
                ),
                PollFd::new(stream.as_fd(), PollFlags::POLLIN),
            ];
            match poll(&mut pollfds, poll_timeout) {
                Ok(_) | Err(Errno::EINTR) => {}
                Err(e) => poll_error = Some(e),
            }
            (
                poll_ready(&pollfds[0]),
                poll_ready(&pollfds[1]),
                poll_ready(&pollfds[2]),
            )
        };
        if let Some(e) = poll_error {
            eprintln!("host-exec: terminal relay poll failed: {e}");
            disconnected = true;
            break;
        }

        if let Some(signals) = &mut signals {
            for sig in signals.pending() {
                if sig == SIGWINCH
                    && let Ok(ws) = tcgetwinsize(output)
                {
                    let _ = tcsetwinsize(master, ws);
                }
                if write_frame(stream, &GuestMessage::HostExecSignal { sig }).is_err() {
                    disconnected = true;
                    break;
                }
            }
        }
        if disconnected {
            break;
        }

        if input_open && input_ready {
            let mut buf = [0u8; 8192];
            match read_fd(input, &mut buf) {
                Ok(0) => {
                    // A piped stdin has no VEOF byte to type; send the
                    // child pty's own EOF character so it sees end-of-file.
                    let _ = write_all_fd(master, &[veof]);
                    input_open = false;
                }
                Ok(n) => {
                    if write_all_fd(master, &buf[..n]).is_err() {
                        master_open = false;
                        input_open = false;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => input_open = false,
            }
        }

        if master_open && master_ready {
            let mut buf = [0u8; 8192];
            match read_fd(master, &mut buf) {
                Ok(0) => master_open = false,
                Ok(n) => {
                    if output_open && write_all_fd(output, &buf[..n]).is_err() {
                        output_open = false;
                    }
                }
                // EIO means every slave copy is closed: the child is gone
                // and no more bytes will arrive.
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
                Err(_) => master_open = false,
            }
        }

        if stream_ready {
            match reader.read(stream) {
                Ok(FrameRead::Frame(bytes)) => {
                    match serde_json::from_slice::<HostMessage>(&bytes) {
                        Ok(HostMessage::HostExecDone { exit_code, .. }) => {
                            exit = Some(exit_code);
                        }
                        Ok(HostMessage::Error { reason, .. }) => {
                            host_stderr
                                .extend_from_slice(format!("host-exec: {reason}\n").as_bytes());
                            exit = Some(1);
                        }
                        Ok(HostMessage::HostExecStdout { data }) => {
                            host_stdout.extend_from_slice(data.as_bytes());
                        }
                        Ok(HostMessage::HostExecStderr { data }) => {
                            host_stderr.extend_from_slice(data.as_bytes());
                            if !data.ends_with('\n') {
                                host_stderr.push(b'\n');
                            }
                        }
                        Ok(_) => {} // Pings and other session-free traffic
                        Err(e) => {
                            host_stderr.extend_from_slice(
                                format!("host-exec: malformed host frame: {e}\n").as_bytes(),
                            );
                            exit = Some(1);
                        }
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut => {}
                Ok(FrameRead::Eof) | Err(_) => {
                    disconnected = true;
                    break;
                }
            }
        }
    }

    if exit.is_some() {
        drain_master(master, output, output_open);
    }
    // Restore before printing host diagnostics: raw mode disables the
    // output processing that puts a newline at column zero.
    drop(restore);
    if !host_stdout.is_empty() && output_open {
        let _ = write_all_fd(output, &host_stdout);
    }
    if !host_stderr.is_empty() {
        let _ = stderr.write_all(&host_stderr);
        let _ = stderr.flush();
    }
    if disconnected {
        return 1;
    }
    exit.unwrap_or(1)
}

/// Restores the caller's termios however the relay exits.
struct TerminalRestore<'a> {
    fd: &'a OwnedFd,
    saved: Option<rustix::termios::Termios>,
}

impl Drop for TerminalRestore<'_> {
    fn drop(&mut self) {
        if let Some(saved) = &self.saved {
            let _ =
                rustix::termios::tcsetattr(self.fd, rustix::termios::OptionalActions::Now, saved);
        }
    }
}

/// Drain bytes the child wrote just before exiting. The host reports the
/// exit only after reaping the child, so the slave copies are closed and
/// this reaches EIO once the kernel buffer is empty.
fn drain_master(master: &OwnedFd, output: &OwnedFd, output_open: bool) {
    use nix::errno::Errno;
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

    let deadline = std::time::Instant::now() + TTY_DRAIN_TIMEOUT;
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return;
        }
        let mut pollfds = [PollFd::new(master.as_fd(), PollFlags::POLLIN)];
        match poll(
            &mut pollfds,
            PollTimeout::try_from(remaining).unwrap_or(PollTimeout::ZERO),
        ) {
            Ok(0) => return,
            Ok(_) | Err(Errno::EINTR) => {}
            Err(_) => return,
        }
        if !poll_ready(&pollfds[0]) {
            return;
        }
        let mut buf = [0u8; 8192];
        match read_fd(master, &mut buf) {
            Ok(0) => return,
            Ok(n) => {
                if output_open {
                    let _ = write_all_fd(output, &buf[..n]);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return, // EIO: nothing left behind the closed slave
        }
    }
}

/// True when a polled descriptor has data or a terminal hangup to read.
fn poll_ready(fd: &nix::poll::PollFd<'_>) -> bool {
    use nix::poll::PollFlags;
    fd.revents().is_some_and(|events| {
        events.intersects(
            PollFlags::POLLIN | PollFlags::POLLHUP | PollFlags::POLLERR | PollFlags::POLLNVAL,
        )
    })
}

/// Read once, retrying the signal-interrupted case.
fn read_fd(fd: &OwnedFd, buf: &mut [u8]) -> std::io::Result<usize> {
    loop {
        match nix::unistd::read(fd.as_fd(), buf) {
            Ok(n) => return Ok(n),
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => return Err(std::io::Error::from(e)),
        }
    }
}

/// Write all bytes to a descriptor, retrying short writes and EINTR.
fn write_all_fd(fd: &OwnedFd, data: &[u8]) -> std::io::Result<()> {
    let mut rest = data;
    while !rest.is_empty() {
        match nix::unistd::write(fd.as_fd(), rest) {
            Ok(0) => return Err(std::io::Error::from(std::io::ErrorKind::WriteZero)),
            Ok(n) => rest = &rest[n..],
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => return Err(std::io::Error::from(e)),
        }
    }
    Ok(())
}

/// Relay host responses to our own stdio until the host reports the exit.
/// Returns the child's exit code, or 1 when the stream ends first.
///
/// Output is written as raw bytes (never through a lossy conversion), and
/// a closed downstream (`| head`) ends the relay cleanly instead of
/// panicking: there is nothing left to show the output to.
fn pump_responses(
    stream: &mut std::os::unix::net::UnixStream,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    use crate::protocol::read_frame;
    while let Ok(Some(bytes)) = read_frame(stream) {
        let msg: Result<HostMessage, _> = serde_json::from_slice(&bytes);
        match msg {
            Ok(HostMessage::HostExecOutput { stream, data }) => {
                let clean = match stream {
                    crate::protocol::ExecStream::Stdout => emit(&mut *stdout, &data),
                    crate::protocol::ExecStream::Stderr => emit(&mut *stderr, &data),
                };
                if clean {
                    let _ = stdout.flush();
                    let _ = stderr.flush();
                } else {
                    return 0;
                }
            }
            Ok(HostMessage::HostExecStdout { data }) => {
                if emit(&mut *stdout, data.as_bytes()) {
                    let _ = stdout.flush();
                } else {
                    return 0;
                }
            }
            Ok(HostMessage::HostExecStderr { data }) => {
                if emit(&mut *stderr, data.as_bytes()) {
                    let _ = stderr.flush();
                } else {
                    return 0;
                }
            }
            Ok(HostMessage::HostExecDone { exit_code, .. }) => {
                let _ = stdout.flush();
                let _ = stderr.flush();
                return exit_code;
            }
            // The host closed a request this shim no longer tracks, or
            // sent something else for another client: stop waiting.
            _ => break,
        }
    }
    // No `HostExecDone` arrived (host died mid-stream): the output is
    // truncated, so success would be a lie.
    1
}

/// Write a chunk; false means the downstream closed (EPIPE).
fn emit(out: &mut dyn Write, data: &[u8]) -> bool {
    match out.write_all(data) {
        Ok(()) => true,
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => false,
        Err(_) => false,
    }
}

/// The env pairs this process may offer the child, restricted to the same
/// fixed list the host re-checks. Sending only these keeps the request
/// honest even against a host whose filter is older than the shim.
fn allowlisted_own_env() -> Vec<(String, String)> {
    EXEC_ENV_KEYS
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_string(), v)))
        .collect()
}

/// Wait for a fd-passing session's `HostExecDone`, relaying trapped
/// terminal signals meanwhile. Returns the child's exit code, or 1 when
/// the host closed the session without reporting (output is truncated
/// through the guest fds, so success would be a lie).
///
/// Signals are polled between frame reads, not in a handler: writing a
/// protocol frame from signal context is the bug this structure avoids,
/// and `signal_hook`'s iterator funnels arrivals safely into the loop.
fn wait_for_done_with_signals(stream: &mut UnixStream) -> i32 {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    wait_for_done_with_signals_to(stream, &mut stdout, &mut stderr)
}

/// The writer-parameterised wait loop, so tests can capture what a
/// host-side failure frame would show the caller.
fn wait_for_done_with_signals_to(
    stream: &mut UnixStream,
    stdout: &mut dyn Write,
    stderr: &mut dyn Write,
) -> i32 {
    let mut signals = match Signals::new(FORWARDED_SIGNALS.iter().copied()) {
        Ok(s) => s,
        // Without the trap a forwarded signal would kill the shim and
        // strand the child: keep the session alive and degrade to waiting.
        Err(e) => {
            eprintln!("host-exec: signal tracking unavailable: {e}");
            return wait_for_done(stream);
        }
    };
    let mut reader = FrameReader::new();
    if stream.set_read_timeout(Some(SIGNAL_RELAY_POLL)).is_err() {
        return 1;
    }
    loop {
        match reader.read(stream) {
            Ok(FrameRead::Frame(bytes)) => match serde_json::from_slice::<HostMessage>(&bytes) {
                Ok(HostMessage::HostExecDone { exit_code, .. }) => {
                    let _ = stream.flush();
                    return exit_code;
                }
                // A typed refusal (unknown command, unsupported option):
                // surface it like a shell diagnostic.
                Ok(HostMessage::Error { reason, .. }) => {
                    eprintln!("host-exec: {reason}");
                    return 1;
                }
                // Host-side failures (a missing allowlisted binary, a
                // failed spawn) use these frames even on an fd session.
                // The child never ran, so nothing else will print them.
                Ok(HostMessage::HostExecStdout { data }) => {
                    if emit(&mut *stdout, data.as_bytes()) {
                        let _ = stdout.flush();
                    }
                }
                Ok(HostMessage::HostExecStderr { data }) => {
                    if emit(&mut *stderr, data.as_bytes()) {
                        if !data.ends_with('\n') {
                            let _ = stderr.write_all(b"\n");
                        }
                        let _ = stderr.flush();
                    }
                }
                Ok(_) => {} // nothing this shim owns (e.g. Pings)
                Err(e) => {
                    eprintln!("host-exec: malformed host frame: {e}");
                    return 1;
                }
            },
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                for sig in signals.pending() {
                    let _ = write_frame(stream, &GuestMessage::HostExecSignal { sig });
                }
            }
            // The host closed the session without reporting, or the read
            // failed: output through the guest fds is truncated already,
            // so success would be a lie.
            Ok(FrameRead::Eof) | Err(_) => return 1,
        }
    }
}

/// Signal-free fallback used when trap registration fails: keeps the
/// session watch intact without the relay.
fn wait_for_done(stream: &mut UnixStream) -> i32 {
    loop {
        match read_frame(stream) {
            Ok(Some(bytes)) => match serde_json::from_slice::<HostMessage>(&bytes) {
                Ok(HostMessage::HostExecDone { exit_code, .. }) => return exit_code,
                Ok(HostMessage::Error { reason, .. }) => {
                    eprintln!("host-exec: {reason}");
                    return 1;
                }
                Ok(HostMessage::HostExecStdout { data }) => {
                    let mut out = std::io::stdout();
                    let _ = out.write_all(data.as_bytes());
                    let _ = out.flush();
                }
                Ok(HostMessage::HostExecStderr { data }) => {
                    let mut err = std::io::stderr();
                    let _ = err.write_all(data.as_bytes());
                    if !data.ends_with('\n') {
                        let _ = err.write_all(b"\n");
                    }
                    let _ = err.flush();
                }
                _ => {}
            },
            _ => return 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ExecStream;

    /// A writer that fails like a closed pipe.
    struct ClosedPipe;
    impl Write for ClosedPipe {
        fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::from(std::io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// Feed the pump scripted host frames; capture what it relays.
    fn pump_scripted(frames: &[HostMessage]) -> (i32, Vec<u8>, Vec<u8>) {
        let (mut client, mut server) = std::os::unix::net::UnixStream::pair().unwrap();
        for frame in frames {
            write_frame(&mut server, frame).unwrap();
        }
        drop(server);
        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = pump_responses(&mut client, &mut out, &mut err);
        (code, out, err)
    }

    /// Drive the fd-passing wait loop over a socket pair. The `server`
    /// end reads frames until it sees a relayed signal with `want_sig`,
    /// then reports `done_code`, mimicking how the host relays a
    /// forwarded signal to the child and relays its death back.
    fn wait_scripted(want_sig: i32, done_code: i32) -> i32 {
        use nix::sys::signal::{Signal, kill};
        use nix::unistd::Pid;

        let (mut client, mut server) = std::os::unix::net::UnixStream::pair().unwrap();
        std::thread::spawn(move || {
            let mut saw_signal = false;
            for _ in 0..50 {
                let Ok(Some(bytes)) = read_frame(&mut server) else {
                    return;
                };
                let msg: GuestMessage = serde_json::from_slice(&bytes).unwrap();
                match msg {
                    GuestMessage::HostExecSignal { sig } if sig == want_sig => saw_signal = true,
                    _ => {}
                }
                if saw_signal {
                    write_frame(
                        &mut server,
                        &HostMessage::HostExecDone {
                            exit_code: done_code,
                            signal: Some(want_sig % 128),
                        },
                    )
                    .unwrap();
                    return;
                }
            }
        });
        // Deliver the real signal to this process after the shim has
        // registered its trap — the iterator consumes it without killing
        // the test, exactly as the trap works inside the shim.
        let handle = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            let _ = kill(Pid::this(), Signal::SIGWINCH);
        });
        let code = wait_for_done_with_signals(&mut client);
        let _ = handle.join();
        code
    }

    /// A SIGWINCH delivered while waiting is relayed as a protocol
    /// message, and the host's shell-convention code comes back through
    /// unchanged.
    #[test]
    fn fd_session_relays_a_trapped_signal_and_reports_the_done_code() {
        assert_eq!(wait_scripted(28, 130), 130);
    }

    /// A plain `HostExecDone` passes through without any signal relay.
    #[test]
    fn fd_session_reports_a_plain_exit() {
        let (mut client, mut server) = std::os::unix::net::UnixStream::pair().unwrap();
        write_frame(
            &mut server,
            &HostMessage::HostExecDone {
                exit_code: 42,
                signal: None,
            },
        )
        .unwrap();
        assert_eq!(wait_for_done_with_signals(&mut client), 42);
    }

    /// A typed refusal from the host prints to the guest shell's stderr
    /// and exits 1.
    #[test]
    fn fd_session_surfaces_a_host_refusal() {
        let (mut client, mut server) = std::os::unix::net::UnixStream::pair().unwrap();
        write_frame(
            &mut server,
            &HostMessage::Error {
                reason: "host-exec: TTY sessions are not supported yet".into(),
                code: Some("not_supported".into()),
            },
        )
        .unwrap();
        assert_eq!(wait_for_done_with_signals(&mut client), 1);
    }

    /// The stdio triple always beats the same three owned descriptors,
    /// duplicating the live ones rather than transferring them (so the
    /// shim's real stdout survives the request).
    #[test]
    fn stdio_triple_dups_the_live_descriptors() {
        let triple = stdio_triple().unwrap();
        let raws: Vec<i32> = triple.iter().map(AsRawFd::as_raw_fd).collect();
        assert_eq!(raws.len(), 3);
        // Fresh numbers distinct from each other off-the-dup path.
        for n in raws {
            assert!(n >= 0);
        }
    }

    /// A pty pair built the same way the allocator builds one, so tests
    /// can stand in for the caller's terminal without reaching for the
    /// shim's real stdout.
    #[cfg(test)]
    fn test_pty() -> (OwnedFd, OwnedFd) {
        use rustix::pty::{OpenptFlags, ioctl_tiocgptpeer, openpt, unlockpt};
        let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
        unlockpt(&master).unwrap();
        let slave = ioctl_tiocgptpeer(&master, OpenptFlags::RDWR).unwrap();
        (master, slave)
    }

    /// Read a pty until `want` appears, polling so a broken relay fails
    /// the test instead of hanging it.
    fn read_until(fd: &OwnedFd, want: &[u8]) {
        use nix::poll::{PollFd, PollFlags, PollTimeout, poll};

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut got = Vec::new();
        while std::time::Instant::now() < deadline {
            let mut pollfds = [PollFd::new(fd.as_fd(), PollFlags::POLLIN)];
            let ready = poll(&mut pollfds, PollTimeout::from(50_u16)).unwrap();
            if ready == 0 || !poll_ready(&pollfds[0]) {
                continue;
            }
            let mut buf = [0u8; 128];
            match nix::unistd::read(fd.as_fd(), &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    got.extend_from_slice(&buf[..n]);
                    if got.windows(want.len()).any(|w| w == want) {
                        return;
                    }
                }
                Err(nix::errno::Errno::EINTR | nix::errno::Errno::EAGAIN) => {}
                Err(e) => panic!("pty read failed: {e}"),
            }
        }
        panic!(
            "did not see {:?}; got {:?}",
            String::from_utf8_lossy(want),
            String::from_utf8_lossy(&got)
        );
    }

    /// The relay copies both directions and puts the caller's termios back
    /// when the session ends. This is the piece the host-side pty test
    /// cannot see: it drains the master itself instead of using the shim.
    #[test]
    fn tty_relay_copies_both_directions_and_restores_the_terminal() {
        let (local_master, local_slave) = test_pty();
        let before = rustix::termios::tcgetattr(&local_slave).unwrap();

        let input = nix::unistd::dup(&local_slave).unwrap();
        let output = nix::unistd::dup(&local_slave).unwrap();
        let (child_master, child_slave) = test_pty();
        let mut child_termios = rustix::termios::tcgetattr(&child_slave).unwrap();
        child_termios.make_raw();
        rustix::termios::tcsetattr(
            &child_slave,
            rustix::termios::OptionalActions::Now,
            &child_termios,
        )
        .unwrap();

        let (mut client, mut server) = std::os::unix::net::UnixStream::pair().unwrap();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut err = Vec::new();
            let code = relay_tty(&mut client, &child_master, &input, &output, &mut err);
            let _ = done_tx.send((code, err));
        });

        // Caller → child, then child → caller.
        write_all_fd(&local_master, b"ping\n").unwrap();
        read_until(&child_slave, b"ping\n");
        write_all_fd(&child_slave, b"pong\n").unwrap();
        read_until(&local_master, b"pong\n");

        // Closing the child side makes the master reach EIO, as it does
        // when the host reaps the child, then the exit report arrives.
        drop(child_slave);
        write_frame(
            &mut server,
            &HostMessage::HostExecDone {
                exit_code: 7,
                signal: None,
            },
        )
        .unwrap();
        let (code, err) = done_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .expect("the tty relay did not finish");
        assert_eq!(code, 7);
        assert!(err.is_empty(), "unexpected relay stderr: {err:?}");

        let after = rustix::termios::tcgetattr(&local_slave).unwrap();
        assert_eq!(before.input_modes.bits(), after.input_modes.bits());
        assert_eq!(before.output_modes.bits(), after.output_modes.bits());
        assert_eq!(before.control_modes.bits(), after.control_modes.bits());
        assert_eq!(before.local_modes.bits(), after.local_modes.bits());
    }

    /// A host-side failure on an fd session uses `HostExecStderr` frames
    /// (no child exists to write through the descriptors); the guest must
    /// print them instead of exiting silently.
    #[test]
    fn fd_session_surfaces_host_side_stderr_frames() {
        let (mut client, mut server) = std::os::unix::net::UnixStream::pair().unwrap();
        write_frame(
            &mut server,
            &HostMessage::HostExecStderr {
                data: "host-exec: no such command".into(),
            },
        )
        .unwrap();
        write_frame(
            &mut server,
            &HostMessage::HostExecDone {
                exit_code: 1,
                signal: None,
            },
        )
        .unwrap();

        let mut out = Vec::new();
        let mut err = Vec::new();
        let code = wait_for_done_with_signals_to(&mut client, &mut out, &mut err);
        assert_eq!(code, 1);
        assert!(out.is_empty());
        assert!(
            String::from_utf8_lossy(&err).contains("no such command"),
            "missing host diagnostic: {err:?}"
        );
    }

    /// The tty triple is three handles to one pty device — the request
    /// may only ever carry one stdio device — and that device inherits
    /// the caller's terminal geometry, since the drain/winsize copy is
    /// how rows and columns reach the host child.
    #[test]
    fn tty_session_triple_is_one_device_with_the_caller_size() {
        let (_source_master, source_slave) = test_pty();
        let want = rustix::termios::Winsize {
            ws_row: 80,
            ws_col: 24,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        rustix::termios::tcsetwinsize(&source_slave, want).unwrap();

        let mut master_slot = None;
        let triple = tty_triple_for(&source_slave, &mut master_slot).unwrap();
        assert!(master_slot.is_some(), "the allocator keeps the master");

        // One device across the three stdio slots…
        let rdevs: Vec<u64> = triple
            .iter()
            .map(|fd| nix::sys::stat::fstat(fd).unwrap().st_rdev)
            .collect();
        assert!(
            rdevs.windows(2).all(|w| w[0] == w[1]),
            "the slaves are not one device: {rdevs:?}"
        );
        // …carrying the caller's window size.
        let got = rustix::termios::tcgetwinsize(&triple[0]).unwrap();
        assert_eq!((got.ws_row, got.ws_col), (80, 24));
    }

    #[test]
    fn streaming_chunks_arrive_byte_identical() {
        let frames = vec![
            HostMessage::HostExecOutput {
                stream: ExecStream::Stdout,
                data: vec![0xff, 0xfe, b'A'],
            },
            HostMessage::HostExecOutput {
                stream: ExecStream::Stderr,
                data: b"warn\n".to_vec(),
            },
            HostMessage::HostExecDone {
                exit_code: 3,
                signal: None,
            },
        ];
        let (code, out, err) = pump_scripted(&frames);
        assert_eq!(code, 3);
        assert_eq!(out, vec![0xff, 0xfe, b'A']);
        assert_eq!(err, b"warn\n");
    }

    #[test]
    fn signal_death_reports_the_shell_convention_code() {
        let frames = vec![HostMessage::HostExecDone {
            exit_code: 143,
            signal: Some(15),
        }];
        let (code, _, _) = pump_scripted(&frames);
        assert_eq!(code, 143);
    }

    #[test]
    fn buffered_frames_from_an_old_host_still_work() {
        let frames = vec![
            HostMessage::HostExecStdout {
                data: "hello".into(),
            },
            HostMessage::HostExecDone {
                exit_code: 0,
                signal: None,
            },
        ];
        let (code, out, _) = pump_scripted(&frames);
        assert_eq!(code, 0);
        assert_eq!(out, b"hello");
    }

    #[test]
    fn truncated_stream_is_a_failure_not_a_success() {
        let frames = vec![HostMessage::HostExecOutput {
            stream: ExecStream::Stdout,
            data: b"partial".to_vec(),
        }];
        let (code, out, _) = pump_scripted(&frames);
        assert_eq!(code, 1);
        assert_eq!(out, b"partial");
    }

    #[test]
    fn closed_downstream_exits_cleanly_instead_of_panicking() {
        let (mut client, mut server) = std::os::unix::net::UnixStream::pair().unwrap();
        write_frame(
            &mut server,
            &HostMessage::HostExecOutput {
                stream: ExecStream::Stdout,
                data: b"x".repeat(1024),
            },
        )
        .unwrap();
        drop(server);
        let mut closed = ClosedPipe;
        let mut err = Vec::new();
        assert_eq!(pump_responses(&mut client, &mut closed, &mut err), 0);
    }
}
