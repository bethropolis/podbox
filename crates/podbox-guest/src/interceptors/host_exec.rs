use std::io::Write;

use crate::protocol::{
    CAP_HOST_EXEC, CAP_HOST_EXEC_STREAM, GuestMessage, HostMessage, write_frame,
};
use crate::socket::negotiate_at_with_optional;

pub fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("host-exec: usage: host-exec <command> [args...]");
        std::process::exit(1);
    }
    run_as_command(&args[1], &args[2..]);
}

pub fn run_as_command(cmd: &str, args: &[String]) {
    let msg = GuestMessage::HostExec {
        cmd: cmd.to_string(),
        args: args.to_vec(),
    };

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
    let (mut stream, _accepted) = negotiate_at_with_optional(
        &socket_path,
        &container,
        &[CAP_HOST_EXEC],
        &[CAP_HOST_EXEC_STREAM],
    )
    .unwrap_or_else(|e| {
        eprintln!("host-exec: {e}");
        std::process::exit(1);
    });
    if write_frame(&mut stream, &msg).is_err() {
        std::process::exit(1);
    }

    let stdout = std::io::stdout();
    let stderr = std::io::stderr();
    let code = pump_responses(&mut stream, &mut stdout.lock(), &mut stderr.lock());
    std::process::exit(code);
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
