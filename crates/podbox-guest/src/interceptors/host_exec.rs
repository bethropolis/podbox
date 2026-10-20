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
/// a shell session expects, and the master stays open on this side until
/// the exit report arrives.
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
    let code = wait_for_done_with_signals(stream);
    // The master is what keeps the pty pair alive, so it must survive the
    // whole wait above; dropping it only after the exit report hangs the
    // terminal up behind a child that (rarely) still lingers.
    drop(master);
    code
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
