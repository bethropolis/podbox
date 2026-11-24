//! V2 fd-passing and TTY host-exec protocol tests.

#[path = "support/socket_protocol.rs"]
mod socket_protocol;

use std::os::fd::{AsRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use podbox::config::{Config, HostExecEntry};
use podbox::protocol::{CAP_HOST_EXEC, CAP_HOST_EXEC_FDS, GuestMessage, HostMessage, write_frame};
use socket_protocol::{enabled_config, finish, hello, recv, send, start};

/// Hello with `host_exec_fds` granted.
fn hello_fds() -> GuestMessage {
    hello(&[CAP_HOST_EXEC, CAP_HOST_EXEC_FDS])
}

/// Serialize a `HostExecV2` frame and send it with descriptors attached,
/// exactly the way the guest shim does.
fn send_v2_with_fds(c: &mut UnixStream, msg: &GuestMessage, fds: &[OwnedFd]) {
    let mut wire = Vec::new();
    write_frame(&mut wire, msg).unwrap();
    let raws: Vec<RawFd> = fds.iter().map(AsRawFd::as_raw_fd).collect();
    podbox::protocol::sysutil::send_bytes_with_fds(c, &[&wire], &raws).unwrap();
}

/// A `HostExecV2` request with plain bodies.
fn v2(cmd: &str, args: Vec<String>) -> GuestMessage {
    GuestMessage::HostExecV2 {
        cmd: cmd.into(),
        args,
        env: vec![],
        cwd: None,
        tty: false,
    }
}

/// A pipe whose read end the child will take as one of its stdio fds.
/// Sending the descriptors over SCM_RIGHTS transfers the *same pipe*, so
/// the bytes the guest writes are the bytes the child reads — with no
/// protocol frame carrying them at all.
fn pipe_pair() -> (OwnedFd, OwnedFd) {
    let (read, write) = std::io::pipe().unwrap();
    (read.into(), write.into())
}

/// A config that also allowlists `sleep` for the relay test.
fn fds_config() -> Config {
    let mut cfg = enabled_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "sleep".to_string(),
            HostExecEntry::Simple("/usr/bin/sleep".into()),
        );
    cfg
}

/// A V2 request with the tty flag set.
fn v2_tty(cmd: &str, args: Vec<String>) -> GuestMessage {
    GuestMessage::HostExecV2 {
        cmd: cmd.into(),
        args,
        env: vec![],
        cwd: None,
        tty: true,
    }
}

/// Open a pty pair the way the guest shim does: a master, and the slave
/// end opened peer to it, in owned duplicate form for the request's
/// three stdio slots.
fn pty_pair() -> (OwnedFd, [OwnedFd; 3]) {
    use rustix::pty::{OpenptFlags, ioctl_tiocgptpeer, openpt, unlockpt};
    let master = openpt(OpenptFlags::RDWR | OpenptFlags::NOCTTY).unwrap();
    unlockpt(&master).unwrap();
    let slave = ioctl_tiocgptpeer(&master, OpenptFlags::RDWR).unwrap();
    let slave2 = nix::unistd::dup(&slave).unwrap();
    let slave3 = nix::unistd::dup(&slave).unwrap();
    (master, [slave, slave2, slave3])
}

/// Read the pty master until the slave side is gone (kernel EIO / EOF).
/// Non-blocking so an unexpected child that never exits fails the test
/// in the assert below instead of hanging it.
fn drain_master(master: OwnedFd) -> String {
    use std::fs::File;
    use std::io::Read;
    nix::fcntl::fcntl(
        &master,
        nix::fcntl::FcntlArg::F_SETFL(nix::fcntl::OFlag::O_NONBLOCK),
    )
    .unwrap();
    let mut master_file = File::from(master);
    let mut out = Vec::new();
    let mut chunk = [0u8; 256];
    loop {
        match master_file.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => out.extend_from_slice(&chunk[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::Interrupted
                    || e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                // The child may still be writing; a clean poll keeps the
                // loop from spinning.
                std::thread::sleep(Duration::from_millis(20));
            }
            // EIO on Linux: every slave copy is closed, nothing more
            // will arrive.
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn fds_capability_is_granted_alongside_host_exec() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello_fds());
    match recv(&mut c) {
        HostMessage::HelloAck { accepted, .. } => {
            assert!(accepted.contains(&CAP_HOST_EXEC_FDS.to_string()));
        }
        other => panic!("expected HelloAck, got {other:?}"),
    }
    finish(c, h).unwrap();
}

/// `echo hi | host-exec cat` in miniature: the request carries the
/// read end of a pipe as the child's stdin and the write end of another
/// pipe as its stdout, both created on the "guest" side of this test.
/// The bytes flow through the pipes themselves — the protocol carries
/// only the request and one `HostExecDone`.
#[test]
fn attached_stdio_round_trips_without_protocol_copying() {
    let mut cfg = fds_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert("cat".to_string(), HostExecEntry::Simple("/bin/cat".into()));
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    let (stdin_read, stdin_write) = pipe_pair();
    let (our_read, child_stdout_w) = pipe_pair();
    // Exactly three descriptors in child-fd order: the stderr slot is
    // present and unread (no child error expected from `cat`).
    let (_, stderr_write) = pipe_pair();

    send_v2_with_fds(
        &mut c,
        &v2("cat", vec![]),
        &[stdin_read, child_stdout_w, stderr_write],
    );

    // Write the payload into the guest's pipe and close it, so the child
    // sees EOF and exits on its own.
    use std::io::{Read, Write};
    let mut stdin = std::io::BufWriter::new(std::fs::File::from(stdin_write));
    stdin.write_all(b"attached-stdio payload\n").unwrap();
    stdin.flush().unwrap();
    drop(stdin);

    // The result comes out of the pipe the "guest" wrote into — no
    // `HostExecOutput` frame ever carries byte data on this path.
    let mut out = String::new();
    std::fs::File::from(our_read)
        .read_to_string(&mut out)
        .unwrap();
    assert!(
        out.contains("attached-stdio payload"),
        "round-trip payload missing: {out:?}"
    );

    // Only the exit report rides the protocol.
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}

/// A frame forwarded while the child runs: `sleep 100` is relayed SIGINT
/// into its own process group, dies with signal 2, and the guest sees
/// the shell-convention `HostExecDone` exit 130.
#[test]
fn host_exec_signal_relay_reports_the_shell_convention_exit() {
    let (mut c, h) = start(fds_config(), Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    // Allowlisted `sleep` runs directly (no shell to intercept the
    // relay); stdin is an already-empty pipe.
    send_v2_with_fds(
        &mut c,
        &v2("sleep", vec!["100".into()]),
        &[pipe_pair().0, pipe_pair().1, pipe_pair().1],
    );

    // Let the child spawn, then forward SIGINT like the shim does. The
    // host relay loop polls frames every 100 ms.
    std::thread::sleep(Duration::from_millis(400));
    send(&mut c, &GuestMessage::HostExecSignal { sig: 2 });
    match recv(&mut c) {
        HostMessage::HostExecDone { exit_code, signal } => {
            assert_eq!(exit_code, 130, "SIGINT death reports 128+2");
            assert_eq!(signal, Some(2));
        }
        other => panic!("expected HostExecDone, got {other:?}"),
    }
    finish(c, h).unwrap();
}

/// A detailed entry's `timeout` bounds the child: a command still alive
/// at expiry is SIGKILLed in its own process group, and the guest sees
/// the same signal-death report a relayed kill produces.
#[test]
fn host_exec_v2_per_entry_timeout_kills_and_reports_signal_death() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("slow.sh");
    std::fs::write(&script, "#!/bin/sh\nsleep 100\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut cfg = fds_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "slow".to_string(),
            HostExecEntry::Detailed {
                path: script.to_string_lossy().into_owned(),
                filter: true,
                shim: false,
                timeout: Some(1),
            },
        );
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    send_v2_with_fds(
        &mut c,
        &v2("slow", vec![]),
        &[pipe_pair().0, pipe_pair().1, pipe_pair().1],
    );
    match recv(&mut c) {
        HostMessage::HostExecDone { exit_code, signal } => {
            assert_eq!(exit_code, 137, "SIGKILL death reports 128+9");
            assert_eq!(signal, Some(9));
        }
        other => panic!("expected HostExecDone, got {other:?}"),
    }
    finish(c, h).unwrap();
}

/// A command that finishes inside its `timeout` is reaped normally: the
/// deadline is checked in the reap poll only, so a fast command is never
/// raced into a kill.
#[test]
fn host_exec_v2_per_entry_timeout_spares_a_command_that_finishes_in_time() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fast.sh");
    std::fs::write(&script, "#!/bin/sh\ntrue\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut cfg = fds_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "fast".to_string(),
            HostExecEntry::Detailed {
                path: script.to_string_lossy().into_owned(),
                filter: true,
                shim: false,
                timeout: Some(5),
            },
        );
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    send_v2_with_fds(
        &mut c,
        &v2("fast", vec![]),
        &[pipe_pair().0, pipe_pair().1, pipe_pair().1],
    );
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}

/// The env allowlist is the whole boundary: TERM reaches the child; an
/// arbitrary variable and a forced `HOME` do not (the child keeps the
/// host daemon's own `HOME`).
#[test]
fn host_exec_v2_env_is_allowlisted() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("showenv.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf 'TERM=[%s]\\nEVIL=[%s]\\nHOME=[%s]\\n' \"$TERM\" \"$PODBOX_EVIL\" \"$HOME\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut cfg = fds_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "showenv".to_string(),
            HostExecEntry::Simple(script.to_string_lossy().into_owned()),
        );

    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    let (out_r, out_w) = pipe_pair();
    let home_before = std::env::var("HOME").unwrap_or_default();
    let msg = GuestMessage::HostExecV2 {
        cmd: "showenv".into(),
        args: vec![],
        env: vec![
            ("TERM".into(), "xterm-forwarded".into()),
            ("PODBOX_EVIL".into(), "smuggled".into()),
            ("HOME".into(), "/container/fake".into()),
        ],
        cwd: None,
        tty: false,
    };
    send_v2_with_fds(&mut c, &msg, &[pipe_pair().0, out_w, pipe_pair().1]);

    // The child's stdout comes through the guest pipe, not the protocol.
    // (The moved write ends drop when the temporary fd array does, so the
    // pipe's last writer is the child and EOF is meaningful.)
    let mut out = String::new();
    use std::io::Read;
    std::fs::File::from(out_r).read_to_string(&mut out).unwrap();
    assert!(
        out.contains("TERM=[xterm-forwarded]"),
        "TERM forwarded: {out:?}"
    );
    assert!(
        !out.contains("EVIL=[smuggled]"),
        "unfiltered env must not pass through: {out:?}"
    );
    assert!(
        !out.contains("HOME=[/container/fake]"),
        "HOME from the container must not pass through: {out:?}"
    );
    if !home_before.is_empty() {
        assert!(
            out.contains(&format!("HOME=[{home_before}]")),
            "HOME stays the host's own: {out:?} (host home {home_before:?})"
        );
    }
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}

/// The capability gate: a guest that never negotiated `host_exec_fds`
/// gets a typed error, nothing executes, and the old session shape
/// still works afterwards.
#[test]
fn vfs_refused_without_the_fds_capability() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    // No `host_exec_stream`: the legacy request below must arrive in the
    // buffered session shape the oldest guests know.
    send(&mut c, &hello(&[CAP_HOST_EXEC]));
    let _ = recv(&mut c);

    send_v2_with_fds(
        &mut c,
        &v2("echo", vec!["should-not-run".into()]),
        &[pipe_pair().0, pipe_pair().1, pipe_pair().1],
    );
    match recv(&mut c) {
        HostMessage::Error { reason, .. } => {
            assert!(reason.contains("host_exec_fds"), "reason {reason:?}");
        }
        other => panic!("expected typed Error, got {other:?}"),
    }
    // Nothing ran: no exit report — and the old-style request works in
    // the same session afterwards.
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["old-path-after-refusal".into()],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStdout { data } => {
            assert!(data.contains("old-path-after-refusal"))
        }
        other => panic!("expected old path to survive, got {other:?}"),
    }
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}

/// A tty request runs the child on the guest's pty: the slave end
/// arrives as the request's three descriptors, `setsid`+`TIOCSCTTY`
/// attach it as the child's controlling terminal (a failure there
/// aborts the whole spawn, so reaching the exit report proves it), and
/// the child's output flows out of the master — no protocol copy.
#[test]
fn host_exec_v2_tty_runs_the_child_on_a_pty() {
    let mut cfg = fds_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "tty".to_string(),
            HostExecEntry::Simple("/usr/bin/tty".into()),
        );
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    let (master, slave_triple) = pty_pair();
    send_v2_with_fds(&mut c, &v2_tty("tty", vec![]), &slave_triple);
    // Our owned slave copies were handed over by descriptor; dropping
    // them is what closes the last slave open and lets the master drain
    // to its EIO end below.
    drop(slave_triple);

    // `/usr/bin/tty` prints its stdin's device path only when it is a
    // controlling-terminal-capable terminal, and exits 0.
    let out = drain_master(master);
    assert!(
        out.contains("/dev/pts/"),
        "the child did not see the pty as its stdin: {out:?}"
    );
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}

/// The per-entry timeout never applies to a tty session: an interactive
/// child that outlives its allowlist entry's `timeout` reports its own
/// natural exit instead of a deadline kill.
#[test]
fn host_exec_v2_tty_sessions_are_never_time_limited() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("linger.sh");
    std::fs::write(&script, "#!/bin/sh\nsleep 2\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();

    let mut cfg = fds_config();
    cfg.integration
        .host_exec
        .allowlist
        .as_mut()
        .unwrap()
        .insert(
            "linger".to_string(),
            HostExecEntry::Detailed {
                path: script.to_string_lossy().into_owned(),
                filter: true,
                shim: false,
                // Smaller than the child's actual runtime: a deadline
                // kill would report 137 + SIGKILL instead of this exit.
                timeout: Some(1),
            },
        );
    let (mut c, h) = start(cfg, Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);

    let (_master, slave_triple) = pty_pair();
    send_v2_with_fds(&mut c, &v2_tty("linger", vec![]), &slave_triple);
    drop(slave_triple);
    assert!(matches!(
        recv(&mut c),
        HostMessage::HostExecDone { exit_code: 0, .. }
    ));
    finish(c, h).unwrap();
}

/// An out-of-session relay frame is quietly ignored, and the session
/// keeps working for an old-style request afterwards.
#[test]
fn out_of_session_signal_is_ignored_harmlessly() {
    let (mut c, h) = start(enabled_config(), Arc::default());
    send(&mut c, &hello_fds());
    let _ = recv(&mut c);
    send(&mut c, &GuestMessage::HostExecSignal { sig: 9 });
    send(
        &mut c,
        &GuestMessage::HostExec {
            cmd: "echo".into(),
            args: vec!["still-alive".into()],
        },
    );
    match recv(&mut c) {
        HostMessage::HostExecStdout { data } => assert!(data.contains("still-alive")),
        other => panic!("expected stdout, got {other:?}"),
    }
    assert!(matches!(recv(&mut c), HostMessage::HostExecDone { .. }));
    finish(c, h).unwrap();
}
