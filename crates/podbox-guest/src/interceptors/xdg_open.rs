use crate::protocol::{CAP_XDG_OPEN, GuestMessage, HostMessage, write_frame};
use crate::socket;

/// Parse xdg-open argv and send an open request to the host.
///
/// The host answers a refused URI with an `Error` frame, and closes the
/// connection after every request. Success is that close; an `Error` frame
/// is a failure, so the exit status matches what happened.
pub fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("xdg-open: missing URI argument");
        std::process::exit(1);
    }

    let uri = args[1].clone();
    let msg = GuestMessage::XdgOpen { uri };

    let mut stream = match socket::connect_negotiated(&[CAP_XDG_OPEN]) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("xdg-open interceptor: failed to send: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = write_frame(&mut stream, &msg) {
        eprintln!("xdg-open interceptor: failed to send: {e}");
        std::process::exit(1);
    }

    loop {
        match socket::read_host_message(&mut stream) {
            // Host closed the connection: the request was handled.
            Ok(None) => return,
            Ok(Some(HostMessage::Error { reason, .. })) => {
                eprintln!("xdg-open: {reason}");
                std::process::exit(1);
            }
            // Keepalive or other traffic: keep waiting for the close.
            Ok(Some(_)) => {}
            Err(e) => {
                eprintln!("xdg-open interceptor: no reply from host: {e}");
                std::process::exit(1);
            }
        }
    }
}
