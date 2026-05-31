use crate::interceptors::send_to_host;
use crate::protocol::GuestMessage;

/// Parse xdg-open argv and send an open request to the host.
pub fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("xdg-open: missing URI argument");
        std::process::exit(1);
    }

    let uri = args[1].clone();

    let msg = GuestMessage::XdgOpen { uri };

    if let Err(e) = send_to_host(&msg) {
        eprintln!("xdg-open interceptor: failed to send: {}", e);
    }
}
