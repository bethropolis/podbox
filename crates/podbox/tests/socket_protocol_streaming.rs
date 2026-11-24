//! Streaming host-exec capability, output framing, and compatibility tests.

#[path = "support/socket_protocol.rs"]
mod socket_protocol;

use std::sync::Arc;

use podbox::config::HostExecEntry;
use podbox::protocol::{
    CAP_HOST_EXEC, CAP_HOST_EXEC_STREAM, ExecStream, GuestMessage, HostMessage,
};
use socket_protocol::{enabled_config, finish, hello, recv, send, start};

/// Collected streaming output: stdout/stderr chunks plus the exit.
type StreamedOutput = (Vec<(ExecStream, Vec<u8>)>, i32, Option<i32>);

/// Hello with the streaming capability and run one command, collecting
/// every frame until `HostExecDone`.
fn run_streamed(
    c: &mut std::os::unix::net::UnixStream,
    cmd: &str,
    args: Vec<String>,
) -> StreamedOutput {
    send(
        c,
        &GuestMessage::HostExec {
            cmd: cmd.into(),
            args,
        },
    );
    let mut chunks = Vec::new();
    loop {
        match recv(c) {
            HostMessage::HostExecOutput { stream, data } => chunks.push((stream, data)),
            HostMessage::HostExecDone { exit_code, signal } => {
                return (chunks, exit_code, signal);
            }
            other => panic!("expected streamed output or done, got {other:?}"),
        }
    }
}

fn hello_streaming() -> GuestMessage {
    hello(&[CAP_HOST_EXEC, CAP_HOST_EXEC_STREAM])
}

#[test]
fn stream_capability_is_granted_alongside_host_exec() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello_streaming());
    match recv(&mut c) {
        HostMessage::HelloAck { accepted, .. } => {
            assert!(accepted.contains(&CAP_HOST_EXEC.to_string()));
            assert!(accepted.contains(&CAP_HOST_EXEC_STREAM.to_string()));
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn stream_capability_is_rejected_when_host_exec_is_disabled() {
    let mut cfg = enabled_config();
    cfg.integration.host_exec.enabled = false;
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_streaming());
    match recv(&mut c) {
        HostMessage::HelloAck { accepted, .. } => {
            assert!(!accepted.contains(&CAP_HOST_EXEC_STREAM.to_string()));
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
    finish(c, h).unwrap();
}

#[test]
fn streamed_binary_output_arrives_byte_identical() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello_streaming());
    let _ = recv(&mut c);
    // The shell binary's first KiB is not valid UTF-8: a lossy conversion
    // would corrupt these bytes.
    let expected = std::fs::read("/bin/sh").unwrap()[..1024].to_vec();
    let (chunks, code, signal) = run_streamed(
        &mut c,
        "sh",
        vec!["-c".into(), "head -c 1024 /bin/sh".into()],
    );
    let stdout: Vec<u8> = chunks
        .iter()
        .filter(|(s, _)| *s == ExecStream::Stdout)
        .flat_map(|(_, d)| d.clone())
        .collect();
    assert_eq!(stdout, expected);
    assert_eq!(code, 0);
    assert_eq!(signal, None);
    finish(c, h).unwrap();
}

#[test]
fn streamed_large_output_uses_many_frames_and_stays_complete() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello_streaming());
    let _ = recv(&mut c);
    let (chunks, code, _) = run_streamed(
        &mut c,
        "sh",
        vec!["-c".into(), "head -c 100000 /dev/urandom".into()],
    );
    let total: usize = chunks.iter().map(|(_, d)| d.len()).sum();
    assert_eq!(total, 100_000);
    assert!(
        chunks.len() > 1,
        "expected several chunks, got {}",
        chunks.len()
    );
    assert_eq!(code, 0);
    finish(c, h).unwrap();
}

#[test]
fn streamed_signal_death_reports_the_shell_convention_code() {
    // A script that kills itself: `$$` inside the file is not an argument,
    // so the host-exec argument filter does not apply to it.
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("selfterm.sh");
    std::fs::write(&script, "#!/bin/sh\nkill -TERM $$\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let mut cfg = enabled_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "selfterm".to_string(),
            HostExecEntry::Simple(script.to_string_lossy().into_owned()),
        );
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_streaming());
    let _ = recv(&mut c);
    let (_, code, signal) = run_streamed(&mut c, "selfterm", vec![]);
    assert_eq!(code, 143);
    assert_eq!(signal, Some(15));
    finish(c, h).unwrap();
}

#[test]
fn unstreamed_guest_still_gets_buffered_frames() {
    // A guest from before streaming (no `host_exec_stream` capability)
    // gets exactly the historical framing: one stdout frame, one done.
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["legacy".into()],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStdout { data } => assert!(data.contains("legacy")),
        other => panic!("expected HostExecStdout, got {other:?}"),
    }
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}
