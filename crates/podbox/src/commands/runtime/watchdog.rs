//! Stdin watchdog for interactive `enter` / `exec` sessions.
//!
//! Rootless `podman exec -it` relays the user's terminal into a freshly
//! allocated container-side pty. When the user's terminal closes, the client
//! keeps the container-side pty master open, so the shell inside the container
//! never sees a hangup and leaks as an orphaned PPID-0 process that also blocks
//! idle shutdown. The watchdog polls stdin for hangup and on detection SIGTERMs
//! (then SIGKILLs) the exec client, forcing podman to tear down the
//! container-side session.
//!
//! The watchdog is a detached child process (a fresh exec of this binary
//! running the hidden `internal-stdin-watchdog` subcommand) that watches the
//! CLI's pid — the pid the CLI then execve's into podman. It ignores
//! SIGHUP/SIGINT and exits as soon as its watched pid exits, so clean
//! sessions leave no residue.
//!
//! Only interactive sessions are guarded: `-it` requires a controlling TTY, and
//! non-TTY stdin is relayed as EOF by podman itself.
//!
//! Extracted from `runtime.rs` alongside the exec paths that spawn it.

use std::os::fd::AsFd;

use anyhow::Result;

use podbox::codegen::distros;

/// Spawn a background watchdog that terminates the `podman exec` client when the
/// user's terminal hangs up.
pub(super) fn spawn_stdin_watchdog() {
    if !distros::is_tty() {
        return;
    }

    // The watchdog is a fresh exec of this binary running the hidden
    // `internal-stdin-watchdog` subcommand. Spawning (fork+exec) keeps the
    // child free of fork()-in-multi-threaded-process hazards: it starts from a
    // clean single-threaded process with no inherited locks or allocator state.
    let Ok(exe) = std::env::current_exe() else {
        return;
    };

    let _ = std::process::Command::new(exe)
        .arg("internal-stdin-watchdog")
        .arg(std::process::id().to_string())
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// Body of the `podbox internal-stdin-watchdog <pid>` subcommand.
///
/// Runs in a freshly exec'd, single-threaded process. Polls stdin for hangup
/// and the pidfd of `parent_pid`; exits when the parent goes away, SIGTERMs
/// (then SIGKILLs after 2s) the parent when stdin hangs up. Never returns
/// normally — always terminates via `std::process::exit`.
///
/// Public because `main` dispatches the hidden subcommand to it directly.
pub fn run_stdin_watchdog(parent_pid: u32) -> Result<()> {
    use nix::poll::{PollFd, PollFlags, PollTimeout, poll};
    use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, kill, sigaction};
    use nix::unistd::Pid;

    // Ignore SIGHUP/SIGINT: the terminal hangup that triggers us may also be
    // delivered here; we must survive long enough to relay SIGTERM.
    let ignore = SigAction::new(SigHandler::SigIgn, SaFlags::empty(), SigSet::empty());
    // SAFETY: `SigIgn` installs no handler function — asking the kernel to
    // ignore two signals is async-signal-safe and races nothing in this
    // freshly exec'd single-threaded process. Neither std nor rustix offers
    // a safe signal-disposition API.
    #[allow(unsafe_code)]
    unsafe {
        let _ = sigaction(Signal::SIGHUP, &ignore);
        let _ = sigaction(Signal::SIGINT, &ignore);
    }

    let Ok(raw_pid) = i32::try_from(parent_pid) else {
        std::process::exit(1);
    };
    let parent_pid = Pid::from_raw(raw_pid);

    let Ok(parent_fd) = podbox::process::open_pidfd(parent_pid.as_raw()) else {
        std::process::exit(1);
    };

    let stdin = std::io::stdin();
    let mut fds = [
        nix::poll::PollFd::new(stdin.as_fd(), PollFlags::POLLHUP | PollFlags::POLLERR),
        PollFd::new(parent_fd.as_fd(), PollFlags::POLLIN),
    ];

    loop {
        match poll(&mut fds, PollTimeout::from(Some(30_000u16))) {
            Ok(0) => {}
            Err(nix::errno::Errno::EINTR) => {}
            Err(_) => std::process::exit(1),
            Ok(_) => {
                let parent_events = fds[1].revents().unwrap_or(PollFlags::empty());
                if parent_events.contains(PollFlags::POLLIN)
                    || parent_events.contains(PollFlags::POLLHUP)
                    || parent_events.contains(PollFlags::POLLERR)
                {
                    // Parent exited — nothing left to watch.
                    std::process::exit(0);
                }

                let stdin_events = fds[0].revents().unwrap_or(PollFlags::empty());
                if stdin_events.contains(PollFlags::POLLHUP)
                    || stdin_events.contains(PollFlags::POLLERR)
                {
                    let _ = kill(parent_pid, Signal::SIGTERM);
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let _ = kill(parent_pid, Signal::SIGKILL);
                    std::process::exit(0);
                }
            }
        }
    }
}
