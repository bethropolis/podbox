use std::io::{self, IoSlice, IoSliceMut};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::net::UnixStream;

use nix::fcntl::{FcntlArg, FdFlag, fcntl};
use nix::sys::socket::{ControlMessage, ControlMessageOwned, MsgFlags, recvmsg, sendmsg};

/// Open a pidfd for a given PID (Linux 5.3+).
///
/// Returns `Err` on old kernels or when the PID does not exist.
pub fn open_pidfd(pid: i32) -> io::Result<OwnedFd> {
    crate::protocol::sysutil::open_pidfd(pid)
}

/// Send a raw file descriptor over a connected Unix stream via `SCM_RIGHTS`.
///
/// Sends one dummy byte alongside the descriptor so the receiver can detect EOF.
/// Retries on `EINTR` to prevent spurious session drops.
pub fn send_fd(stream: &UnixStream, fd: RawFd) -> io::Result<()> {
    let raw_fd = stream.as_raw_fd();
    let cmsg = ControlMessage::ScmRights(&[fd]);
    let iov = [IoSlice::new(&[0u8])];
    loop {
        match sendmsg::<()>(raw_fd, &iov, &[cmsg], MsgFlags::empty(), None) {
            Ok(_) => return Ok(()),
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => return Err(io::Error::from(e)),
        }
    }
}

/// Mark a descriptor close-on-exec so children spawned later (host-exec
/// commands, `xdg-open`, `wl-copy`) never inherit it.
///
/// Use this right after adopting a descriptor from outside the process,
/// such as systemd's socket-activation fd 3: without the flag a
/// long-running child would hold the listening socket open.
pub fn set_cloexec(fd: BorrowedFd) -> io::Result<()> {
    fcntl(fd, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC))
        .map(|_| ())
        .map_err(io::Error::from)
}

/// Adopt a raw descriptor received via `SCM_RIGHTS` into an owned handle.
///
/// The kernel duplicates ancillary-data descriptors into the receiving
/// process on delivery, transferring their single reference — whoever
/// calls this must adopt them (here) or close them (leak). This is the
/// single audit point for that ownership transfer in this crate.
pub fn adopt_scm_fd(raw: RawFd) -> OwnedFd {
    // SAFETY: `raw` arrived via SCM_RIGHTS over a Unix socket; the kernel
    // duplicated it into this process on delivery and we now hold exactly
    // one reference to a valid open descriptor. No safe wrapper exists for
    // adopting externally-sourced fds.
    #[allow(unsafe_code)]
    unsafe {
        OwnedFd::from_raw_fd(raw)
    }
}

/// Receive a file descriptor from a connected Unix stream via `SCM_RIGHTS`.
///
/// Returns `None` when the sender has closed the connection (EOF), or when
/// the message carries no descriptor. Every descriptor the kernel delivers
/// is adopted into an `OwnedFd` straight away: the first is returned and
/// any extras are dropped on the spot, so a peer that sends more than
/// asked-for cannot leak descriptors into this process. Delivery requests
/// `MSG_CMSG_CLOEXEC`, so the adopted descriptors also stay out of spawned
/// children. Retries on `EINTR` to prevent spurious session drops.
pub fn recv_fd(stream: &UnixStream) -> io::Result<Option<OwnedFd>> {
    let raw_fd = stream.as_raw_fd();
    let mut buf = [0u8; 1];
    let mut iov = [IoSliceMut::new(&mut buf)];
    let mut cmsg_buf = vec![0u8; 256];
    let msg = loop {
        match recvmsg::<()>(
            raw_fd,
            &mut iov,
            Some(&mut cmsg_buf),
            MsgFlags::MSG_CMSG_CLOEXEC,
        ) {
            Ok(m) => break m,
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => return Err(io::Error::from(e)),
        }
    };

    if msg.bytes == 0 {
        return Ok(None);
    }

    let mut owned: Vec<OwnedFd> = Vec::new();
    if let Ok(cmsgs) = msg.cmsgs() {
        for cmsg in cmsgs {
            if let ControlMessageOwned::ScmRights(fds) = cmsg {
                owned.extend(fds.into_iter().map(adopt_scm_fd));
            }
        }
    }
    let mut owned = owned.into_iter();
    Ok(owned.next())
}

/// Receive bytes from a connected Unix stream together with any
/// descriptors the peer attached via `SCM_RIGHTS`.
///
/// Plain `read(2)` silently discards descriptors queued by a peer's
/// `sendmsg`, so every read that may carry attached fds goes through
/// here. Every descriptor the kernel delivers is adopted immediately
/// (`MSG_CMSG_CLOEXEC`, so nothing added later inherits them).
///
/// # Security model for guest-supplied descriptors
///
/// The fds this returns originate on the guest side of the host
/// connection — pipes, regular files and terminals the guest process
/// already had open. Handing them to a spawned child as its stdio is what
/// the guest asked for and gives the host **no new access**: the host
/// only holds an extra handle to the pipe between the guest's two
/// processes. Descriptors not consumed by a host-exec request are
/// dropped by the caller, which is their only closing step.
pub fn recv_bytes_with_fds(
    stream: &UnixStream,
    buf: &mut [u8],
) -> io::Result<(usize, Vec<OwnedFd>)> {
    /// Buffer covering several delivered descriptors; a peer that sends
    /// more in one message is refused (see the `MSG_CTRUNC` check) rather
    /// than partially honoured with the rest silently closed.
    const CMSG_BUF_LEN: usize = 1024;

    let raw_fd = stream.as_raw_fd();
    let mut iov = [IoSliceMut::new(buf)];
    let mut cmsg_buf = vec![0u8; CMSG_BUF_LEN];
    let msg = loop {
        match recvmsg::<()>(
            raw_fd,
            &mut iov,
            Some(&mut cmsg_buf),
            MsgFlags::MSG_CMSG_CLOEXEC,
        ) {
            Ok(m) => break m,
            Err(nix::errno::Errno::EINTR) => {}
            Err(e) => return Err(io::Error::from(e)),
        }
    };

    let mut fds: Vec<OwnedFd> = Vec::new();
    if let Ok(cmsgs) = msg.cmsgs() {
        for cmsg in cmsgs {
            if let ControlMessageOwned::ScmRights(arrived) = cmsg {
                fds.extend(arrived.into_iter().map(adopt_scm_fd));
            }
        }
    }
    if msg.flags.contains(MsgFlags::MSG_CTRUNC) {
        // The kernel dropped descriptors we had no room for. Refuse the
        // whole message — every adopted descriptor is closed here — so a
        // truncated request can never be executed as if it were whole.
        drop(fds);
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "too many attached descriptors in one message",
        ));
    }
    Ok((msg.bytes, fds))
}

/// A [`std::io::Read`] adapter that keeps [`recv_bytes_with_fds`]'s descriptors
/// alongside the frame bytes a [`crate::protocol::FrameReader`] consumes.
///
/// The protocol reader requests one length prefix or body slice per read;
/// descriptors attached to a frame ride the *first* read of its bytes and
/// are parked in `pending` until the frame is complete, so a frame and
/// its attached fds are consumed as one unit no matter how many recv
/// calls the frame needed.
pub struct FdCapturingReader<'a> {
    stream: &'a UnixStream,
    /// Adopted descriptors waiting for the frame they arrived with. The
    /// owner of this vec decides what to do with them once a frame is
    /// complete (use or drop-closes).
    pending: &'a mut Vec<OwnedFd>,
}

impl<'a> FdCapturingReader<'a> {
    pub fn new(stream: &'a UnixStream, pending: &'a mut Vec<OwnedFd>) -> Self {
        Self { stream, pending }
    }
}

impl std::io::Read for FdCapturingReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let (n, fds) = recv_bytes_with_fds(self.stream, buf)?;
        self.pending.extend(fds);
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsFd;

    fn fd_targets() -> Vec<std::path::PathBuf> {
        std::fs::read_dir("/proc/self/fd")
            .unwrap()
            .filter_map(|e| e.ok())
            .filter_map(|e| std::fs::read_link(e.path()).ok())
            .collect()
    }

    fn has_cloexec(fd: BorrowedFd) -> bool {
        let flags = fcntl(fd, FcntlArg::F_GETFD).unwrap();
        FdFlag::from_bits_retain(flags).contains(FdFlag::FD_CLOEXEC)
    }

    #[test]
    fn set_cloexec_marks_the_descriptor_close_on_exec() {
        let (a, _b) = UnixStream::pair().unwrap();
        fcntl(a.as_fd(), FcntlArg::F_SETFD(FdFlag::empty())).unwrap();
        assert!(!has_cloexec(a.as_fd()));
        set_cloexec(a.as_fd()).unwrap();
        assert!(has_cloexec(a.as_fd()));
    }

    #[test]
    fn recv_fd_returns_one_and_closes_the_extras() {
        // Spares live in a unique temp file, so counting descriptors that
        // point at it is unaffected by whatever other tests have open.
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let spares: Vec<std::fs::File> = (0..3).map(|_| tmp.reopen().unwrap()).collect();
        let raws: Vec<RawFd> = spares.iter().map(|f| f.as_raw_fd()).collect();
        let target = tmp.path().to_path_buf();
        let count_for = |p: &std::path::Path| fd_targets().iter().filter(|t| *t == p).count();
        // The temp handle plus its three reopens.
        assert_eq!(count_for(&target), 4);

        let (tx, rx) = UnixStream::pair().unwrap();
        sendmsg::<()>(
            tx.as_raw_fd(),
            &[IoSlice::new(&[0u8])],
            &[ControlMessage::ScmRights(&raws)],
            MsgFlags::empty(),
            None,
        )
        .unwrap();
        let got = recv_fd(&rx).unwrap().expect("expected a descriptor");
        // Delivered close-on-exec, so children never inherit it.
        assert!(has_cloexec(got.as_fd()));
        // Three sent, one returned: the two extras are already closed.
        assert_eq!(count_for(&target), 5);
        drop(got);
        assert_eq!(count_for(&target), 4);
    }

    /// Frame bytes and the descriptors attached to them arrive through
    /// one recvmsg: a frame read via [`FdCapturingReader`] parks the
    /// guest's stdio pipes in `pending` while [`crate::protocol::FrameReader`] assembles
    /// the payload, none of them lost to the prefix/body slicing the
    /// protocol reader does, and a descriptor-free frame leaves the
    /// pending list untouched.
    #[test]
    fn frame_bytes_and_attached_descriptors_arrive_together() {
        use crate::protocol::{FrameRead, FrameReader, GuestMessage, write_frame};

        let (mut tx, rx) = UnixStream::pair().unwrap();
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let spares: Vec<std::fs::File> = (0..3).map(|_| tmp.reopen().unwrap()).collect();
        let raws: Vec<RawFd> = spares.iter().map(|f| f.as_raw_fd()).collect();

        let v2 = |cmd: &str| GuestMessage::HostExecV2 {
            cmd: cmd.into(),
            args: vec![],
            env: vec![],
            cwd: None,
            tty: false,
        };
        let mut wire = Vec::new();
        write_frame(&mut wire, &v2("cat")).unwrap();
        crate::protocol::sysutil::send_bytes_with_fds(&tx, &[&wire], &raws).unwrap();

        let mut pending: Vec<OwnedFd> = Vec::new();
        let mut reader = FrameReader::new();
        let mut fd_in = FdCapturingReader::new(&rx, &mut pending);
        let frame = match reader.read(&mut fd_in).unwrap() {
            FrameRead::Frame(b) => b,
            other => panic!("expected a frame, got {other:?}"),
        };

        let msg: GuestMessage = serde_json::from_slice(&frame).unwrap();
        assert!(matches!(msg, GuestMessage::HostExecV2 { cmd, .. } if cmd == "cat"));
        assert_eq!(pending.len(), 3);

        // A later frame that carries no descriptors leaves the pending
        // list unchanged instead of reporting anything extra.
        write_frame(&mut tx, &v2("true")).unwrap();
        let mut pending2: Vec<OwnedFd> = Vec::new();
        let mut fd_in = FdCapturingReader::new(&rx, &mut pending2);
        match reader.read(&mut fd_in).unwrap() {
            FrameRead::Frame(b) => {
                let msg: GuestMessage = serde_json::from_slice(&b).unwrap();
                assert!(matches!(msg, GuestMessage::HostExecV2 { cmd, .. } if cmd == "true"));
            }
            other => panic!("expected a frame, got {other:?}"),
        }
        assert!(pending2.is_empty(), "no descriptors were sent");
        assert_eq!(pending.len(), 3);
    }
}
