//! Host-side effects of the guest socket protocol.
//!
//! The message loop in [`super::conn`] calls these instead of running
//! `notify-send`, `xdg-open`, `wl-copy`, `wl-paste` or `systemctl` directly.
//! That lets the protocol tests drive the real loop with a recording fake.
//! `host-exec` is not behind this trait: its allowlisted commands run for
//! real, and its tests use commands that exist on any Linux host.

use std::io::Write;
use std::sync::mpsc;
use std::time::Duration;

use crate::protocol::NotifyAction;

/// How long a notification with actions may wait for the user to pick one.
/// Past this, the guest gets an empty key and the connection slot is freed.
const ACTION_WAIT: Duration = Duration::from_mins(1);

/// How long `wl-paste` may run before it is killed.
const CLIPBOARD_GET_WAIT: Duration = Duration::from_secs(10);

/// Run `wait` on its own thread and return its key, or an empty string if it
/// has not finished within `timeout`. The thread is left to finish on its own
/// (it cannot be cancelled), but the caller's connection is released.
pub(crate) fn key_within<F>(timeout: Duration, wait: F) -> String
where
    F: FnOnce() -> String + Send + 'static,
{
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(wait());
    });
    rx.recv_timeout(timeout).unwrap_or_default()
}

/// Side effects a connection performs on the host.
pub trait HostActions: Sync {
    /// Show a notification. With `actions` empty this is fire-and-forget.
    /// Otherwise returns the key of the action the user picked, or an empty
    /// string when none was picked. An error means the notification could
    /// not be shown.
    fn notify(&self, summary: &str, body: &str, actions: &[NotifyAction])
    -> anyhow::Result<String>;

    /// Open a URI already validated by the caller, with the host's `xdg-open`.
    fn open_uri(&self, uri: &str) -> anyhow::Result<()>;

    /// Copy text to the host clipboard.
    fn clipboard_set(&self, text: &str) -> anyhow::Result<()>;

    /// Read the host clipboard as text.
    fn clipboard_get(&self) -> anyhow::Result<String>;

    /// Stop the container's systemd unit (idle shutdown).
    fn stop_unit(&self, name: &str) -> anyhow::Result<()>;
}

/// The real host: notifications over D-Bus, `xdg-open`, `wl-copy`/`wl-paste`,
/// and `systemctl`.
pub struct SystemActions;

impl HostActions for SystemActions {
    fn notify(
        &self,
        summary: &str,
        body: &str,
        actions: &[NotifyAction],
    ) -> anyhow::Result<String> {
        if actions.is_empty() {
            notify_rust::Notification::new()
                .summary(summary)
                .body(body)
                .show()?;
            return Ok(String::new());
        }
        let mut notif = notify_rust::Notification::new();
        notif.summary(summary).body(body);
        for action in actions {
            notif.action(&action.key, &action.label);
        }
        let handle = notif.show()?;
        Ok(key_within(ACTION_WAIT, move || {
            let mut chosen_key = String::new();
            handle.wait_for_action(|action| {
                chosen_key = action.to_string();
            });
            chosen_key
        }))
    }

    fn open_uri(&self, uri: &str) -> anyhow::Result<()> {
        let args = [uri.into()];
        match crate::process::spawn_interactive_timeout("xdg-open", &args, Duration::from_secs(30))
        {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::warn!("xdg-open exited with {status} for host open request"),
            Err(e) => tracing::warn!("failed to run xdg-open: {e:#}"),
        }
        Ok(())
    }

    fn clipboard_set(&self, text: &str) -> anyhow::Result<()> {
        let mut child = std::process::Command::new("wl-copy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(text.as_bytes());
        }
        drop(child.stdin.take());
        let _ = crate::process::wait_child_timeout(child, Duration::from_secs(10))?;
        Ok(())
    }

    fn clipboard_get(&self) -> anyhow::Result<String> {
        let child = std::process::Command::new("wl-paste")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()?;
        // Killed after CLIPBOARD_GET_WAIT, so a wedged compositor cannot pin
        // the connection's slot.
        let output = crate::process::wait_child_timeout(child, CLIPBOARD_GET_WAIT)?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn stop_unit(&self, name: &str) -> anyhow::Result<()> {
        crate::systemd::stop_unit(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wait_that_outlasts_the_timeout_yields_no_key() {
        let started = std::time::Instant::now();
        let key = key_within(Duration::from_millis(100), || {
            std::thread::sleep(Duration::from_secs(3));
            "late".into()
        });
        assert_eq!(key, "");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_wait_that_finishes_in_time_returns_its_key() {
        assert_eq!(key_within(Duration::from_secs(5), || "open".into()), "open");
    }
}
