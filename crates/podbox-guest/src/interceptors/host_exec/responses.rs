use std::io::Write;
use std::os::unix::net::UnixStream;

use signal_hook::iterator::Signals;

use crate::protocol::{FrameRead, FrameReader, GuestMessage, HostMessage, read_frame, write_frame};

use super::{FORWARDED_SIGNALS, SIGNAL_RELAY_POLL};

/// Relay host responses to our own stdio until the host reports the exit.
/// Returns the child's exit code, or 1 when the stream ends first.
///
/// Output is written as raw bytes (never through a lossy conversion), and
/// a closed downstream (`| head`) ends the relay cleanly instead of
/// panicking: there is nothing left to show the output to.
pub(super) fn pump_responses(
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

/// Wait for a fd-passing session's `HostExecDone`, relaying trapped
/// terminal signals meanwhile. Returns the child's exit code, or 1 when
/// the host closed the session without reporting (output is truncated
/// through the guest fds, so success would be a lie).
///
/// Signals are polled between frame reads, not in a handler: writing a
/// protocol frame from signal context is the bug this structure avoids,
/// and `signal_hook`'s iterator funnels arrivals safely into the loop.
pub(super) fn wait_for_done_with_signals(stream: &mut UnixStream) -> i32 {
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
