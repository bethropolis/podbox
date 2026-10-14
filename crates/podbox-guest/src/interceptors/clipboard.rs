use std::io::{Read, Write};

use crate::interceptors::{send_to_host, send_to_host_and_read_response};
use crate::protocol::{CAP_CLIPBOARD, GuestMessage, HostMessage};

/// Run clipboard operations: `podbox-clipboard set` or `podbox-clipboard get`.
pub fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("podbox-clipboard: expected 'set' or 'get'");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "set" => {
            let mut text = String::new();
            if std::io::stdin().read_to_string(&mut text).is_ok() {
                let msg = GuestMessage::ClipboardSet {
                    text: text.trim().to_string(),
                };
                if let Err(e) = send_to_host(CAP_CLIPBOARD, &msg) {
                    eprintln!("clipboard set: failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        "get" => {
            let msg = GuestMessage::ClipboardGet;
            match send_to_host_and_read_response(CAP_CLIPBOARD, &msg) {
                Ok(HostMessage::ClipboardData { text }) => {
                    let _ = std::io::stdout().write_all(text.as_bytes());
                }
                Ok(_) => {
                    eprintln!("clipboard get: unexpected response");
                    std::process::exit(1);
                }
                Err(e) => {
                    eprintln!("clipboard get: failed: {e}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            eprintln!("podbox-clipboard: unknown command '{}'", args[1]);
            std::process::exit(1);
        }
    }
}
