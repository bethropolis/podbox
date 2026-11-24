use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::time::Duration;

use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;

use crate::config::IntegrationConfig;
use crate::protocol::{ExecStream, HostMessage, write_frame};

use super::{exit_code_and_signal, host_exec_failed, resolve_host_exec_request};

/// How long a legacy (non fd-passing) host-exec child may run before it
/// is killed. The fd-passing session instead honours the allowlist
/// entry's own `timeout`, configured per command.
const HOST_EXEC_TIMEOUT: Duration = Duration::from_mins(1);

/// Total output (stdout + stderr) a streaming host-exec child may produce
/// before it is killed. Bounds host memory and keeps every frame far under
/// the frame cap. Per-entry configuration would need a config key.
const MAX_HOST_EXEC_OUTPUT: u64 = 64 * 1024 * 1024;

/// One read from a streaming child's pipe. Small enough that the JSON
/// encoding of a chunk stays far under the frame cap.
const EXEC_CHUNK_LEN: usize = 16 * 1024;

/// Handle a `HostExec` message from the guest.
///
/// When the guest negotiated `host_exec_stream`, the child's pipes are
/// forwarded in raw-byte chunks as the command runs, so output larger
/// than a frame (or not valid UTF-8) survives. Otherwise the whole output
/// is buffered and sent as one frame per pipe, exactly as before, for
/// guests that predate streaming.
pub(in crate::socket_host) fn handle_host_exec(
    stream: &mut UnixStream,
    config: &IntegrationConfig,
    cmd: String,
    args: Vec<String>,
    streaming: bool,
) -> anyhow::Result<()> {
    let canonical_path = match resolve_host_exec_request(config, &cmd, &args) {
        Ok(p) => p,
        Err(msg) => return host_exec_failed(stream, &msg),
    };

    match Command::new(&canonical_path)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => {
            if streaming {
                stream_child_output(stream, child, HOST_EXEC_TIMEOUT, MAX_HOST_EXEC_OUTPUT)
            } else {
                buffered_child_output(stream, child)
            }
        }
        Err(e) => {
            let msg = if e.kind() == std::io::ErrorKind::NotFound {
                format!("host-exec: '{cmd}' not found in allowlist path or host $PATH")
            } else {
                format!("host-exec: failed to execute '{cmd}': {e}")
            };
            host_exec_failed(stream, &msg)
        }
    }
}

/// Wait for the child, then send its whole stdout and stderr as one frame
/// each. Preserves the historical framing for guests that did not
/// negotiate `host_exec_stream`.
fn buffered_child_output(
    stream: &mut UnixStream,
    child: std::process::Child,
) -> anyhow::Result<()> {
    let output = crate::process::wait_child_timeout(child, HOST_EXEC_TIMEOUT)?;
    if !output.stdout.is_empty() {
        write_frame(
            stream,
            &HostMessage::HostExecStdout {
                data: String::from_utf8_lossy(&output.stdout).to_string(),
            },
        )?;
    }
    if !output.stderr.is_empty() {
        write_frame(
            stream,
            &HostMessage::HostExecStderr {
                data: String::from_utf8_lossy(&output.stderr).to_string(),
            },
        )?;
    }
    let (exit_code, signal) = exit_code_and_signal(&output.status);
    write_frame(stream, &HostMessage::HostExecDone { exit_code, signal })?;
    Ok(())
}

/// One item moving from a streaming child's pipe readers to the sender.
enum ExecChunk {
    Out(Vec<u8>),
    Err(Vec<u8>),
    OutEof,
    ErrEof,
}

/// Forward a streaming child's pipes as raw-byte chunks until both hit
/// EOF, then report the exit. Kills the child when `timeout` elapses or
/// `max_bytes` of total output is exceeded.
pub(in crate::socket_host) fn stream_child_output(
    stream: &mut UnixStream,
    mut child: std::process::Child,
    timeout: Duration,
    max_bytes: u64,
) -> anyhow::Result<()> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, rx) = std::sync::mpsc::channel::<ExecChunk>();
    if let Some(pipe) = stdout {
        pump_pipe(pipe, tx.clone(), ExecChunk::Out, ExecChunk::OutEof);
    }
    if let Some(pipe) = stderr {
        pump_pipe(pipe, tx.clone(), ExecChunk::Err, ExecChunk::ErrEof);
    }
    drop(tx);

    // Same killer as the buffered path: SIGKILL past the timeout. The done
    // signal disarms it once the child has been reaped.
    let pid = Pid::from_raw(child.id().cast_signed());
    let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        if done_rx.recv_timeout(timeout).is_err() {
            let _ = kill(pid, Signal::SIGKILL);
        }
    });

    let mut eofs = 0;
    let mut total: u64 = 0;
    while eofs < 2 {
        let (payload, wire_stream) = match rx.recv() {
            Ok(ExecChunk::Out(data)) => (data, ExecStream::Stdout),
            Ok(ExecChunk::Err(data)) => (data, ExecStream::Stderr),
            Ok(ExecChunk::OutEof | ExecChunk::ErrEof) => {
                eofs += 1;
                continue;
            }
            // Readers only exit after sending their EOF; a dead channel
            // here means both are gone.
            Err(_) => break,
        };
        total = total.saturating_add(payload.len() as u64);
        if total > max_bytes {
            tracing::warn!("host-exec: output exceeded {max_bytes} bytes, killing child");
            let _ = kill(Pid::from_raw(child.id().cast_signed()), Signal::SIGKILL);
            write_frame(
                stream,
                &HostMessage::HostExecStderr {
                    data: format!("host-exec: output exceeded {max_bytes} bytes, truncated"),
                },
            )?;
            // Reap the killed child; unread pipe data dies with the
            // readers, which exit on EOF by themselves.
            let _ = child.wait()?;
            write_frame(
                stream,
                &HostMessage::HostExecDone {
                    exit_code: 1,
                    signal: None,
                },
            )?;
            let _ = done_tx.send(());
            return Ok(());
        }
        write_frame(
            stream,
            &HostMessage::HostExecOutput {
                stream: wire_stream,
                data: payload,
            },
        )?;
    }
    let status = child.wait()?;
    let _ = done_tx.send(());
    let (exit_code, signal) = exit_code_and_signal(&status);
    write_frame(stream, &HostMessage::HostExecDone { exit_code, signal })?;
    Ok(())
}

/// Copy one child pipe into the channel in 16 KiB chunks until EOF, then
/// report EOF. A broken pipe (child killed) ends the stream like an EOF.
fn pump_pipe<R: std::io::Read + Send + 'static>(
    mut pipe: R,
    tx: std::sync::mpsc::Sender<ExecChunk>,
    wrap: fn(Vec<u8>) -> ExecChunk,
    eof: ExecChunk,
) {
    std::thread::spawn(move || {
        let mut buf = [0u8; EXEC_CHUNK_LEN];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) => {
                    let _ = tx.send(eof);
                    return;
                }
                Ok(n) => {
                    if tx.send(wrap(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
                Err(_) => {
                    let _ = tx.send(eof);
                    return;
                }
            }
        }
    });
}
