use std::io::Write;
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use signal_hook::consts::SIGWINCH;
use signal_hook::iterator::Signals;

use crate::protocol::{FrameRead, FrameReader, GuestMessage, HostMessage, write_frame};

use super::stdio::dup_or_devnull;
use super::{FORWARDED_SIGNALS, SIGNAL_RELAY_POLL, SIGNAL_RELAY_POLL_MS};

/// How long the relay waits for the last pty bytes after the host reports
/// the child's exit.
const TTY_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

/// Relay a tty session between the caller's terminal and the child's pty.
///
/// The host child writes into the pty slave, so somebody has to read the
/// master and put those bytes on the caller's stdout; likewise the caller's
/// stdin has to be written into the master. The caller's terminal is put in
/// raw mode while the relay runs: the child's pty already carries the
/// original termios and therefore owns echo and line editing. Leaving the
/// caller's terminal cooked would echo every byte a second time.
pub(super) fn relay_tty_session(stream: &mut UnixStream, master: &OwnedFd) -> i32 {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A pty pair built the same way the allocator builds one, so tests
    /// can stand in for the caller's terminal without reaching for the
    /// shim's real stdout.
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
}
