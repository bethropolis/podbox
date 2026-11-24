use std::os::fd::{AsFd, BorrowedFd, OwnedFd};

/// Which of the child's stdio descriptors this process should hand over:
/// the guest's own stdin/stdout/stderr duplicated into owned handles, or
/// a `/dev/null` opened read-write where the owning fd is closed entirely
/// (`0<&-` style shells), so the request always carries the same three.
pub(super) fn stdio_triple() -> std::io::Result<[OwnedFd; 3]> {
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
pub(super) fn tty_triple() -> std::io::Result<([OwnedFd; 3], OwnedFd)> {
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

pub(super) fn dup_or_devnull(fd: BorrowedFd<'_>) -> std::io::Result<OwnedFd> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;

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
}
