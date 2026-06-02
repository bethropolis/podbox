use crate::interceptors::send_to_host;
use crate::protocol::GuestMessage;

/// Parse notify-send argv and send a notification request to the host.
pub fn run(args: &[String]) {
    let (summary, body, urgency) = parse_args(args);

    let msg = GuestMessage::Notify {
        summary,
        body,
        urgency,
    };

    if let Err(e) = send_to_host(&msg) {
        eprintln!("notify-send interceptor: failed to send: {}", e);
    }
}

fn parse_args(args: &[String]) -> (String, String, String) {
    let mut summary = String::new();
    let mut body = String::new();
    let mut urgency = "normal".to_string();

    let mut i = 1; // skip arg0
    while i < args.len() {
        match args[i].as_str() {
            "-u" | "--urgency" => {
                if i + 1 < args.len() {
                    urgency = args[i + 1].clone();
                    i += 2;
                } else {
                    i += 1;
                }
            }
            s if summary.is_empty() => {
                summary = s.to_string();
                i += 1;
            }
            s => {
                if !body.is_empty() {
                    body.push(' ');
                }
                body.push_str(s);
                i += 1;
            }
        }
    }

    (summary, body, urgency)
}
