use std::collections::HashSet;
use std::os::fd::{AsRawFd, BorrowedFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use nix::poll::{poll, PollFd, PollFlags, PollTimeout};

use crate::error::GuestError;
use crate::protocol::HostMessage;
use crate::socket;

pub fn run() -> Result<(), GuestError> {
    let container_name = std::env::var("PODMGR_CONTAINER")
        .map_err(|_| GuestError::ContainerNameMissing)?;

    let xdg_runtime = std::env::var("XDG_RUNTIME_DIR")
        .unwrap_or_else(|_| format!("/run/user/{}", nix::unistd::getuid()));

    let host_socket_path =
        PathBuf::from(&xdg_runtime).join("podmgr").join(format!("{}.sock", container_name));

    let bin_dir = PathBuf::from("/run/podmgr/bin");

    // 1. Create /run/podmgr/bin/
    std::fs::create_dir_all(&bin_dir)?;

    // 2. Connect to host socket with retry
    eprintln!("podmgr-guest: connecting to host socket...");
    let mut host_stream = socket::connect_to_host(&host_socket_path)?;

    // 3. Handshake
    let all_caps = vec![
        "notify".to_string(),
        "xdg_open".to_string(),
        "clipboard".to_string(),
    ];
    let accepted = socket::handshake(&mut host_stream, &container_name, &all_caps)?;
    let accepted_set: HashSet<String> = accepted.iter().cloned().collect();
    eprintln!("podmgr-guest: accepted capabilities: {:?}", accepted);

    // 4. Install interceptor symlinks for accepted capabilities
    install_interceptors(&accepted_set, &bin_dir)?;

    // 5. Write PATH injection
    write_path_injection(&bin_dir)?;

    // 6. Enter event loop (listen for host messages)
    event_loop(&mut host_stream)?;

    Ok(())
}

fn install_interceptors(accepted: &HashSet<String>, bin_dir: &std::path::Path) -> std::io::Result<()> {
    let self_path = std::env::current_exe()?;
    let self_path_str = self_path.to_string_lossy();

    let symlinks = vec![
        ("notify", "notify-send"),
        ("xdg_open", "xdg-open"),
        ("clipboard", "podmgr-clipboard"),
    ];

    for (cap, name) in symlinks {
        if accepted.contains(cap) {
            let link = bin_dir.join(name);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(self_path_str.as_ref(), &link)?;
        }
    }

    Ok(())
}

fn write_path_injection(bin_dir: &std::path::Path) -> std::io::Result<()> {
    let conf_dir = std::path::PathBuf::from("/etc/profile.d");
    std::fs::create_dir_all(&conf_dir)?;
    let conf_path = conf_dir.join("podmgr.sh");
    let content = format!(
        "export PATH={}:$PATH\n",
        bin_dir.to_string_lossy()
    );
    std::fs::write(conf_path, content)?;
    Ok(())
}

fn event_loop(host_stream: &mut UnixStream) -> Result<(), GuestError> {
    let idle_timeout = PollTimeout::try_from(Duration::from_millis(300_000))
        .expect("5-minute timeout fits in i32");

    loop {
        let mut fds = [
            PollFd::new(unsafe { BorrowedFd::borrow_raw(host_stream.as_raw_fd()) }, PollFlags::POLLIN),
        ];

        match poll(&mut fds, idle_timeout) {
            Ok(0) => {
                eprintln!("podmgr-guest: idle timeout, exiting.");
                return Ok(());
            }
            Ok(_) => {
                let revents = fds[0].revents().unwrap_or(PollFlags::empty());
                if revents.contains(PollFlags::POLLHUP) || revents.contains(PollFlags::POLLERR) {
                    eprintln!("podmgr-guest: host socket hung up.");
                    return Ok(());
                }
                if revents.contains(PollFlags::POLLIN) {
                    match socket::read_host_message(host_stream) {
                        Ok(Some(HostMessage::Shutdown)) => {
                            eprintln!("podmgr-guest: received shutdown, exiting.");
                            return Ok(());
                        }
                        Ok(Some(HostMessage::Ping)) => {}
                        Ok(Some(HostMessage::HelloAck { .. })) => {}
                        Ok(Some(HostMessage::ClipboardData { .. })) => {}
                        Ok(None) => {
                            eprintln!("podmgr-guest: host disconnected.");
                            return Ok(());
                        }
                        Err(e) => {
                            if !e.to_string().contains("WouldBlock") {
                                return Err(e);
                            }
                        }
                    }
                }
            }
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => return Err(GuestError::IO(std::io::Error::from_raw_os_error(e as i32))),
        }
    }
}
