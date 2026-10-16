use crate::interceptors::{send_to_host, send_to_host_and_read_response};
use crate::protocol::{CAP_NOTIFY, GuestMessage, HostMessage, NotifyAction};

pub fn run(args: &[String]) {
    let (summary, body, urgency, app_name, actions) = parse_args(args);

    let has_actions = !actions.is_empty();

    let msg = GuestMessage::Notify {
        summary,
        body,
        urgency,
        actions,
        app_name,
    };

    if has_actions {
        match send_to_host_and_read_response(CAP_NOTIFY, &msg) {
            Ok(HostMessage::NotifyActionResult { action_key, .. }) => {
                if !action_key.is_empty() {
                    println!("{action_key}");
                }
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("notify-send interceptor: failed: {e}");
                std::process::exit(1);
            }
        }
    } else if let Err(e) = send_to_host(CAP_NOTIFY, &msg) {
        eprintln!("notify-send interceptor: failed to send: {e}");
        std::process::exit(1);
    }
}

/// Outcome of probing one flag at the cursor.
enum FlagHit {
    /// The cursor is not on this flag; it did not move.
    Missing,
    /// The flag was consumed but carried no value.
    Bare,
    /// The flag and its value were consumed.
    Value(String),
}

/// Fetch the value of `-f value`, `--flag value` or `--flag=value`.
/// Advances past the flag (and its value, when present) on a hit.
fn take_flag(args: &[String], i: &mut usize, short: &str, long: &str) -> FlagHit {
    let Some(token) = args.get(*i) else {
        return FlagHit::Missing;
    };
    if token == short || token == long {
        if let Some(value) = args.get(*i + 1) {
            *i += 2;
            return FlagHit::Value(value.clone());
        }
        *i += 1;
        return FlagHit::Bare;
    }
    if let Some((name, value)) = token.split_once('=')
        && name == long
    {
        *i += 1;
        return FlagHit::Value(value.to_string());
    }
    FlagHit::Missing
}

/// Split `notify-send` arguments into summary, body, urgency, app name and
/// actions. Only urgency and the app name travel over the protocol; the icon
/// and the expiry/category/hint/transient flags are accepted (so they never
/// leak into the text) and dropped, matching what the host can display.
fn parse_args(args: &[String]) -> (String, String, String, String, Vec<NotifyAction>) {
    let mut summary = String::new();
    let mut body = String::new();
    let mut urgency = "normal".to_string();
    let mut app_name = String::new();
    let mut actions: Vec<NotifyAction> = Vec::new();

    let mut i = 1;
    while i < args.len() {
        // Value flags forwarded over the protocol.
        match take_flag(args, &mut i, "-u", "--urgency") {
            FlagHit::Value(value) => {
                urgency = value;
                continue;
            }
            FlagHit::Bare => continue,
            FlagHit::Missing => {}
        }
        match take_flag(args, &mut i, "-a", "--app-name") {
            FlagHit::Value(value) => {
                app_name = value;
                continue;
            }
            FlagHit::Bare => continue,
            FlagHit::Missing => {}
        }
        // Accepted and dropped: no protocol field carries them.
        if !matches!(take_flag(args, &mut i, "-i", "--icon"), FlagHit::Missing)
            || !matches!(
                take_flag(args, &mut i, "-t", "--expire-time"),
                FlagHit::Missing
            )
            || !matches!(
                take_flag(args, &mut i, "-c", "--category"),
                FlagHit::Missing
            )
            || !matches!(take_flag(args, &mut i, "-h", "--hint"), FlagHit::Missing)
        {
            continue;
        }
        if args[i] == "-e" || args[i] == "--transient" {
            i += 1;
            continue;
        }
        match args[i].as_str() {
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

    (summary, body, urgency, app_name, actions)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_string()).collect()
    }

    #[test]
    fn flags_never_leak_into_summary_or_body() {
        let (summary, body, urgency, app_name, actions) = parse_args(&argv(&[
            "notify-send",
            "-u",
            "critical",
            "-t",
            "1000",
            "Title",
            "Body",
        ]));
        assert_eq!(summary, "Title");
        assert_eq!(body, "Body");
        assert_eq!(urgency, "critical");
        assert!(app_name.is_empty());
        assert!(actions.is_empty());
    }

    #[test]
    fn app_name_and_long_forms_come_through() {
        let (summary, body, urgency, app_name, _) = parse_args(&argv(&[
            "notify-send",
            "--app-name=Files",
            "-i",
            "folder",
            "-a",
            "Other",
            "--urgency=low",
            "Title",
        ]));
        assert_eq!(summary, "Title");
        assert!(body.is_empty());
        assert_eq!(urgency, "low");
        // Last app-name wins, like repeated flags elsewhere.
        assert_eq!(app_name, "Other");
    }

    #[test]
    fn transient_category_hint_and_actions_parse() {
        let (summary, body, _, _, actions) = parse_args(&argv(&[
            "notify-send",
            "-e",
            "-c",
            "transfer",
            "-h",
            "x:y",
            "-A",
            "open:Open it",
            "Title",
            "Body words",
        ]));
        assert_eq!(summary, "Title");
        assert_eq!(body, "Body words");
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].key, "open");
    }
}
