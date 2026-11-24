//! Legacy buffered host-exec protocol and allowlist tests.

#[path = "support/socket_protocol.rs"]
mod socket_protocol;

use std::sync::Arc;

use podbox::config::HostExecEntry;
use podbox::protocol::{CAP_HOST_EXEC, GuestMessage, HostMessage};
use socket_protocol::{enabled_config, finish, hello, recv, send, start};

fn expect_error(msg: HostMessage, needle: &str) {
    match msg {
        HostMessage::Error { reason, .. } => {
            assert!(
                reason.contains(needle),
                "reason {reason:?} lacks {needle:?}"
            );
        }
        other => panic!("expected Error containing {needle:?}, got {other:?}"),
    }
}

#[test]
fn host_exec_runs_an_allowlisted_command_and_reports_its_exit() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["hello from test".into()],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStdout { data } => {
            assert!(data.contains("hello from test"), "{data:?}")
        }
        other => panic!("expected HostExecStdout, got {other:?}"),
    }
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "sh".into(),
            args: vec!["-c".into(), "exit 42".into()],
        },
    );
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 42, .. }
    ));
    finish(c, h).unwrap();
}

#[test]
fn host_exec_refuses_commands_outside_the_allowlist() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    for cmd in ["rm", "/bin/rm", "echo/../rm"] {
        send(
            &mut c,
            &GuestMessage::HostExec {
                cmd: cmd.into(),
                args: vec!["-rf".into(), "/tmp/x".into()],
            },
        );
        match recv(&mut c) {
            HostMessage::HostExecStderr { data } => {
                assert!(data.contains("Permission denied"), "{cmd}: {data:?}");
            }
            other => panic!("{cmd}: expected stderr refusal, got {other:?}"),
        }
        assert!(matches!(
            recv(&mut c),
            HostMessage::HostExecDone { exit_code: 1, .. }
        ));
    }
    finish(c, h).unwrap();
}

#[test]
fn host_exec_refuses_argument_injection_patterns() {
    let mut cfg = enabled_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "echo".to_string(),
            HostExecEntry::Detailed {
                path: "/bin/echo".into(),
                filter: true,
                shim: true,
                timeout: None,
            },
        );
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    for args in [
        vec!["a;id".to_string()],
        vec!["$(id)".to_string()],
        vec!["--config=/tmp/evil".to_string()],
        vec!["x > /etc/passwd".to_string()],
    ] {
        send(
            &mut c,
            &GuestMessage::HostExec {
                cmd: "echo".into(),
                args: args.clone(),
            },
        );
        match recv(&mut c) {
            HostMessage::HostExecStderr { data } => {
                assert!(data.contains("Security violation"), "{args:?}: {data:?}");
            }
            other => panic!("{args:?}: expected refusal, got {other:?}"),
        }
        assert!(matches!(
            recv(&mut c),
            HostMessage::HostExecDone { exit_code: 1, .. }
        ));
    }
    finish(c, h).unwrap();
}

#[test]
fn host_exec_is_refused_when_disabled_by_config() {
    let mut cfg = enabled_config();
    cfg.integration.host_exec.enabled = false;
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    match recv(&mut c) {
        HostMessage::HelloAck { accepted, .. } => assert!(accepted.is_empty()),
        other => panic!("expected HelloAck, got {other:?}"),
    }
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec![],
        },
    );
    expect_error(recv(&mut c), "'host_exec' not accepted");
    finish(c, h).unwrap();
}

/// The old request path is untouched for a guest that negotiated only
/// the pre-fd capabilities, even on a host that also serves V2.
#[test]
fn old_host_exec_path_regression() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);
    // A clean exit…
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["plain".into()],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStdout { data } => assert!(data.contains("plain")),
        other => panic!("expected legacy stdout frame, got {other:?}"),
    }
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    // …a refusal…
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "rm".into(),
            args: vec![],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStderr { data } => {
            assert!(data.contains("Permission denied"), "{data:?}");
        }
        other => panic!("expected legacy refusal, got {other:?}"),
    }
    // The refusal also reports its exit in a Done frame.
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 1, .. }
    ));
    // …and the recorded backend invocations reaching the same shape.
    finish(c, h).unwrap();
}
