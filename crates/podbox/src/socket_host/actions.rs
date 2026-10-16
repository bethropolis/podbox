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
        read_host_clipboard("wl-paste")
    }

    fn stop_unit(&self, name: &str) -> anyhow::Result<()> {
        crate::systemd::stop_unit(name)
    }
}

/// Read the host clipboard, preserving the bytes exactly.
///
/// `wl-paste --no-newline` (wl-clipboard 2.1+) emits the selection
/// without the trailing newline plain `wl-paste` appends. Older
/// `wl-paste` rejects the flag with a failure status, so fall back to a
/// plain paste and strip the single appended newline instead. Either way
/// the guest receives the selection unchanged.
fn read_host_clipboard(paste_bin: &str) -> anyhow::Result<String> {
    let flagged = run_wl_paste(paste_bin, &["--no-newline"])?;
    if flagged.status.success() {
        return Ok(String::from_utf8_lossy(&flagged.stdout).into_owned());
    }
    let plain = run_wl_paste(paste_bin, &[])?;
    Ok(strip_single_trailing_newline(&String::from_utf8_lossy(
        &plain.stdout,
    )))
}

/// Run `wl-paste` with the given extra arguments, killing it after
/// `CLIPBOARD_GET_WAIT` so a wedged compositor cannot pin the connection.
fn run_wl_paste(paste_bin: &str, extra_args: &[&str]) -> anyhow::Result<std::process::Output> {
    let child = std::process::Command::new(paste_bin)
        .args(extra_args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    // Killed after CLIPBOARD_GET_WAIT, so a wedged compositor cannot pin
    // the connection's slot.
    crate::process::wait_child_timeout(child, CLIPBOARD_GET_WAIT)
}

/// Drop at most the one trailing `\n` a plain `wl-paste` appends. Content
/// is otherwise untouched: `"a\n\n"` keeps one newline.
fn strip_single_trailing_newline(text: &str) -> String {
    text.strip_suffix('\n').unwrap_or(text).to_string()
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

    // ── clipboard fidelity ──

    #[test]
    fn strip_removes_at_most_one_trailing_newline() {
        assert_eq!(strip_single_trailing_newline("x\n"), "x");
        assert_eq!(strip_single_trailing_newline("x\n\n"), "x\n");
        assert_eq!(strip_single_trailing_newline("x"), "x");
        assert_eq!(strip_single_trailing_newline(""), "");
    }

    /// Write an executable fake `wl-paste`. It prints `stdout_text` and
    /// exits `flag_status` when called with `--no-newline`, 0 otherwise.
    fn fake_wl_paste(dir: &std::path::Path, stdout_text: &str, flag_status: i32) -> String {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("wl-paste");
        // Quote the payload for the shell: no single quotes appear in the
        // test strings below.
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"--no-newline\" ]; then printf '{}'; exit {flag_status}; fi\nprintf '{}'\n",
            stdout_text.replace('\n', "\\n"),
            stdout_text.replace('\n', "\\n"),
        );
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn modern_wl_paste_comes_through_exactly() {
        let dir = tempfile::tempdir().unwrap();
        let paste = fake_wl_paste(dir.path(), "  spaced\\n\\n", 0);
        assert_eq!(
            read_host_clipboard(&paste).unwrap(),
            "  spaced\n\n".to_string()
        );
    }

    #[test]
    fn ancient_wl_paste_falls_back_and_strips_one_newline() {
        let dir = tempfile::tempdir().unwrap();
        // Rejects `--no-newline` (exit 1); the plain paste appends `\n`.
        let paste = fake_wl_paste(dir.path(), "line\\n\\n", 1);
        assert_eq!(read_host_clipboard(&paste).unwrap(), "line\n");
    }
}
