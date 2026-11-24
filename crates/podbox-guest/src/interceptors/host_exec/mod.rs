use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM, SIGWINCH};

use crate::protocol::{
    CAP_HOST_EXEC, CAP_HOST_EXEC_FDS, CAP_HOST_EXEC_STREAM, GuestMessage, write_frame,
};
use crate::socket::negotiate_at_with_optional;

mod responses;
mod stdio;
mod tty;

use responses::{pump_responses, wait_for_done_with_signals};
use stdio::{stdio_triple, tty_triple};
use tty::relay_tty_session;

/// The signals the shim traps while a fd-passing exec runs and relays to
/// the host as `HostExecSignal` frames. The host forwards them to the
/// child's own process group; nothing here reaches the shim's caller.
pub(super) const FORWARDED_SIGNALS: &[i32] = &[SIGHUP, SIGINT, SIGTERM, SIGWINCH];

/// How long a fd-passing wait loop sleeps before forwarding signals.
pub(super) const SIGNAL_RELAY_POLL: Duration = Duration::from_millis(100);

/// The same interval as a `poll(2)` timeout.
pub(super) const SIGNAL_RELAY_POLL_MS: u16 = 100;

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

/// The env pairs this process may offer the child, restricted to the same
/// list the host re-checks.
fn allowlisted_own_env() -> Vec<(String, String)> {
    std::env::vars()
        .filter(|(key, _)| crate::protocol::is_host_exec_env(key))
        .collect()
}
