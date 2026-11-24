use std::os::unix::net::UnixStream;

use super::super::actions::HostActions;
use crate::protocol::{HostMessage, write_frame};

/// Handle a `Notify` message from the guest.
///
/// Without actions the notification is fire-and-forget. With actions the
/// chosen key (or an empty key when the user picked none, or the notification
/// could not be shown) goes back to the guest.
pub(in crate::socket_host) fn handle_notify(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
    summary: String,
    body: String,
    urgency: String,
    app_name: String,
    choices: Vec<crate::protocol::NotifyAction>,
) -> anyhow::Result<()> {
    if choices.is_empty() {
        if let Err(e) = actions.notify(&summary, &body, &urgency, &app_name, &choices) {
            tracing::debug!("notification not shown: {e:#}");
        }
        return Ok(());
    }
    let action_key = actions
        .notify(&summary, &body, &urgency, &app_name, &choices)
        .unwrap_or_else(|e| {
            tracing::warn!("notification with actions not shown: {e:#}");
            String::new()
        });
    write_frame(
        stream,
        &HostMessage::NotifyActionResult {
            notification_id: 0,
            action_key,
        },
    )?;
    Ok(())
}

/// Longest URI accepted from the guest.
pub(crate) const MAX_URI_LEN: usize = 8192;

/// Why a guest-supplied URI was refused.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UriError {
    Empty,
    TooLong,
    ControlCharacter,
    Malformed,
    /// `scheme` is refused because it is not in `allowed`.
    SchemeNotAllowed {
        scheme: String,
        allowed: Vec<String>,
    },
    MissingHost,
}

impl std::fmt::Display for UriError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "empty URI"),
            Self::TooLong => write!(f, "URI longer than {MAX_URI_LEN} bytes"),
            Self::ControlCharacter => write!(f, "URI contains a control character"),
            Self::Malformed => write!(f, "not a URI"),
            Self::SchemeNotAllowed { scheme, allowed } => write!(
                f,
                "scheme '{scheme}' is not allowed (allowed: {})",
                allowed.join(", ")
            ),
            Self::MissingHost => write!(f, "web URL has no host"),
        }
    }
}

/// Check a guest-supplied URI and return the text to open.
///
/// A bare host such as `example.com` is opened over https. Paths and anything
/// that is not a URI are refused.
pub(crate) fn validate_uri(uri: &str, allowed: &[String]) -> Result<String, UriError> {
    let s = uri.trim();
    if s.is_empty() {
        return Err(UriError::Empty);
    }
    if s.len() > MAX_URI_LEN {
        return Err(UriError::TooLong);
    }
    if s.chars().any(char::is_control) {
        return Err(UriError::ControlCharacter);
    }
    if s.starts_with('/') || s.starts_with('.') {
        return Err(UriError::Malformed);
    }

    let (text, parsed) = match url::Url::parse(s) {
        Ok(parsed) => (s.to_string(), parsed),
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            let wrapped = format!("https://{s}");
            let parsed = url::Url::parse(&wrapped).map_err(|_| UriError::Malformed)?;
            (wrapped, parsed)
        }
        Err(_) => return Err(UriError::Malformed),
    };

    let scheme = parsed.scheme().to_ascii_lowercase();
    if !allowed.iter().any(|a| a.eq_ignore_ascii_case(&scheme)) {
        return Err(UriError::SchemeNotAllowed {
            scheme,
            allowed: allowed.to_vec(),
        });
    }
    let web = scheme == "http" || scheme == "https";
    if web && parsed.host_str().is_none_or(str::is_empty) {
        return Err(UriError::MissingHost);
    }
    Ok(text)
}

/// Handle an `XdgOpen` message from the guest.
///
/// A refused URI gets an `Error` frame with the reason. Accepted or refused,
/// the caller closes the connection afterwards: the guest's `xdg-open` waits
/// for that close, and treats an `Error` frame as failure.
pub(in crate::socket_host) fn handle_xdg_open(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
    allowed: &[String],
    uri: String,
) -> anyhow::Result<()> {
    match validate_uri(&uri, allowed) {
        Ok(validated) => actions.open_uri(&validated)?,
        Err(e) => {
            tracing::warn!("refused xdg-open request: {e}");
            write_frame(
                stream,
                &HostMessage::Error {
                    reason: format!("xdg-open refused: {e}"),
                    code: None,
                },
            )?;
        }
    }
    Ok(())
}

/// Handle a `ClipboardSet` message from the guest.
///
/// Oversized content gets an `Error` reply and the connection stays open:
/// dropping it would punish a client that can simply send less.
pub(in crate::socket_host) fn handle_clipboard_set(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
    text: String,
) -> anyhow::Result<()> {
    if text.len() > crate::protocol::MAX_CLIPBOARD_BYTES {
        write_frame(
            stream,
            &HostMessage::Error {
                reason: format!(
                    "clipboard content exceeds the {} MiB limit",
                    crate::protocol::MAX_CLIPBOARD_BYTES / (1024 * 1024)
                ),
                code: None,
            },
        )?;
        return Ok(());
    }
    actions.clipboard_set(&text)
}

/// Handle a `ClipboardGet` message from the guest.
///
/// The bytes pass through exactly as the backend returned them: no
/// trimming, so trailing newlines and indentation survive the round trip.
pub(in crate::socket_host) fn handle_clipboard_get(
    stream: &mut UnixStream,
    actions: &dyn HostActions,
) -> anyhow::Result<()> {
    let text = actions.clipboard_get()?;
    if text.len() > crate::protocol::MAX_CLIPBOARD_BYTES {
        write_frame(
            stream,
            &HostMessage::Error {
                reason: format!(
                    "clipboard content exceeds the {} MiB limit",
                    crate::protocol::MAX_CLIPBOARD_BYTES / (1024 * 1024)
                ),
                code: None,
            },
        )?;
        return Ok(());
    }
    let response = HostMessage::ClipboardData { text };
    write_frame(stream, &response)?;
    Ok(())
}
