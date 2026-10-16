use std::io::{Read, Write};

use crate::interceptors::{send_to_host, send_to_host_and_read_response};
use crate::protocol::{CAP_CLIPBOARD, GuestMessage, HostMessage, MAX_CLIPBOARD_BYTES};

/// Run clipboard operations: `podbox-clipboard set` or `podbox-clipboard get`.
pub fn run(args: &[String]) {
    if args.len() < 2 {
        eprintln!("podbox-clipboard: expected 'set' or 'get'");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "set" => {
            let text = match read_stdin_text(&mut std::io::stdin()) {
                Ok(text) => text,
                Err(e) => {
                    eprintln!("clipboard set: failed to read stdin: {e}");
                    std::process::exit(1);
                }
            };
            let msg = match prepare_set(text) {
                Ok(msg) => msg,
                Err(e) => {
                    eprintln!("clipboard set: {e}");
                    std::process::exit(1);
                }
            };
            if let Err(e) = send_to_host(CAP_CLIPBOARD, &msg) {
                eprintln!("clipboard set: failed: {e}");
                std::process::exit(1);
            }
        }
        "get" => {
            let msg = GuestMessage::ClipboardGet;
            match send_to_host_and_read_response(CAP_CLIPBOARD, &msg) {
                Ok(HostMessage::ClipboardData { text }) => {
                    let _ = std::io::stdout().write_all(text.as_bytes());
                }
                Ok(HostMessage::Error { reason }) => {
                    eprintln!("clipboard get: {reason}");
                    std::process::exit(1);
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

/// Read stdin as text. The content passes through exactly: no trimming,
/// so indentation and trailing newlines survive the round trip.
fn read_stdin_text(reader: &mut impl Read) -> std::io::Result<String> {
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    Ok(text)
}

/// Build the set message, refusing content the host would have to reject
/// with a clear local error instead of a failed frame.
fn prepare_set(text: String) -> Result<GuestMessage, String> {
    if text.len() > MAX_CLIPBOARD_BYTES {
        return Err(format!(
            "content exceeds the {} MiB limit",
            MAX_CLIPBOARD_BYTES / (1024 * 1024)
        ));
    }
    Ok(GuestMessage::ClipboardSet { text })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_preserves_whitespace_exactly() {
        match prepare_set("  indented\n\n".to_string()) {
            Ok(GuestMessage::ClipboardSet { text }) => assert_eq!(text, "  indented\n\n"),
            other => panic!("expected a set message, got {other:?}"),
        }
    }

    #[test]
    fn set_refuses_oversized_content_locally() {
        let big = "x".repeat(MAX_CLIPBOARD_BYTES + 1);
        assert!(prepare_set(big).is_err());
        assert!(prepare_set("x".repeat(MAX_CLIPBOARD_BYTES)).is_ok());
    }

    #[test]
    fn stdin_rejects_non_utf8() {
        let mut bad: &[u8] = &[0xff, 0xfe];
        assert!(read_stdin_text(&mut bad).is_err());
    }
}
