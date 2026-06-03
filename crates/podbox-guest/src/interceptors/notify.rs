use crate::interceptors::{send_to_host, send_to_host_and_read_response};
use crate::protocol::{GuestMessage, HostMessage, NotifyAction};

pub fn run(args: &[String]) {
    let (summary, body, urgency, actions) = parse_args(args);

    let has_actions = !actions.is_empty();

    let msg = GuestMessage::Notify {
        summary,
        body,
        urgency,
        actions,
        app_name: String::new(),
    };

    if !has_actions {
        if let Err(e) = send_to_host(&msg) {
            eprintln!("notify-send interceptor: failed to send: {}", e);
        }
    } else {
        match send_to_host_and_read_response(&msg) {
            Ok(HostMessage::NotifyActionResult { action_key, .. }) => {
                if !action_key.is_empty() {
                    println!("{}", action_key);
                }
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("notify-send interceptor: failed: {}", e);
            }
        }
    }
}

fn parse_args(args: &[String]) -> (String, String, String, Vec<NotifyAction>) {
    let mut summary = String::new();
    let mut body = String::new();
    let mut urgency = "normal".to_string();
    let mut actions: Vec<NotifyAction> = Vec::new();

    let mut i = 1;
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
            "-A" | "--action" => {
                if i + 1 < args.len() {
                    let val = &args[i + 1];
                    if let Some((key, label)) = val.split_once(':') {
                        actions.push(NotifyAction {
                            key: key.to_string(),
                            label: label.to_string(),
                        });
                    }
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

    (summary, body, urgency, actions)
}
