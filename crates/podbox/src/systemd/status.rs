//! Unit status query and diagnostics: `systemctl show` parsing, journal
//! tails, actionable hint extraction, and the diagnostic card.
//!
//! Extracted verbatim from `systemd.rs`.

use std::process::Command;

use anyhow::{Context, Result};

/// Parsed status of a systemd unit.
#[derive(Debug, Default)]
pub struct UnitStatus {
    pub load_state: String,
    pub active_state: String,
    pub sub_state: String,
    pub load_error: String,
    pub need_daemon_reload: bool,
}

/// Query systemd unit properties via `systemctl --user show`.
pub(crate) fn query_unit_status(name: &str) -> Result<UnitStatus> {
    let mut cmd = Command::new("systemctl");
    cmd.args([
        "--user",
        "show",
        &format!("{name}.service"),
        "--property=LoadState,ActiveState,SubState,LoadError,NeedDaemonReload",
    ]);
    let output = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn systemctl show")?
        .wait_with_output()
        .context("systemctl show failed")?;

    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("unit '{}' not found by systemd: {}", name, stderr.trim());
    }

    Ok(parse_unit_show(&String::from_utf8_lossy(&output.stdout)))
}

fn parse_unit_show(raw: &str) -> UnitStatus {
    let mut status = UnitStatus::default();
    for line in raw.lines() {
        let (key, value) = match line.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        match key {
            "LoadState" => status.load_state = value.to_string(),
            "ActiveState" => status.active_state = value.to_string(),
            "SubState" => status.sub_state = value.to_string(),
            "LoadError" => status.load_error = value.to_string(),
            "NeedDaemonReload" => status.need_daemon_reload = value == "yes",
            _ => {}
        }
    }
    status
}

/// Tail journal logs for a container's service units.
pub(crate) fn journal_tail(name: &str, n: u32) -> Result<String> {
    if which::which("journalctl").is_err() {
        anyhow::bail!("journalctl not available");
    }
    let mut cmd = Command::new("journalctl");
    cmd.args([
        "--user",
        "-u",
        &format!("{name}.service"),
        "-n",
        &n.to_string(),
        "--no-pager",
        "--output=short",
    ]);
    let output = cmd
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("failed to spawn journalctl")?
        .wait_with_output()
        .context("journalctl failed")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("journalctl failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if stdout.trim().is_empty() {
        anyhow::bail!("no journal entries found");
    }
    Ok(stdout)
}

/// Build an actionable hint string from unit status and journal output.
fn diagnose(status: &UnitStatus, journal: Option<&str>) -> (String, String) {
    let load_error = &status.load_error;

    let error_msg = if !load_error.is_empty() {
        load_error.clone()
    } else {
        format!(
            "ActiveState={}, SubState={}",
            status.active_state, status.sub_state
        )
    };

    let hint = if load_error.contains("Invalid environment")
        || load_error.contains("bad setting")
        || load_error.contains("Bad message")
    {
        "Check your config environment variables. \
         Environment keys must not contain newlines or '=' characters, \
         and values must be valid UTF-8."
            .to_string()
    } else if load_error.contains("port") || load_error.contains("address") {
        "A port specified in [network]ports may already be in use on the host. \
         Ensure the port is available and not bound by another service."
            .to_string()
    } else if load_error.contains("permission") || load_error.contains("Permission") {
        "systemd reported a permission error. \
         Check that your home and mount directories are accessible."
            .to_string()
    } else if load_error.contains("mount")
        || load_error.contains("volume")
        || load_error.contains("Volume")
    {
        "A mount directory specified in your config may not exist. \
         Verify your XDG and custom mount paths are correct."
            .to_string()
    } else if let Some(journal) = journal {
        extract_hint_from_journal(journal)
    } else {
        "Run `podbox build --rebuild` to regenerate Quadlet files, \
         then `podbox enable` to reinstall them."
            .to_string()
    };

    (error_msg, hint)
}

fn extract_hint_from_journal(journal: &str) -> String {
    for line in journal.lines() {
        let lower = line.to_lowercase();
        if lower.contains("oci runtime") || lower.contains("container create failed") {
            return "An OCI runtime error occurred. \
                     Check that your container image has all required dependencies \
                     and that your mount paths are correct."
                .to_string();
        }
        if lower.contains("permission denied") {
            return "A permission error occurred. \
                     Check that your home and mount directories have the correct permissions."
                .to_string();
        }
        if lower.contains("port already in use")
            || lower.contains("address already in use")
            || lower.contains("listen failed")
            || lower.contains("couldn't listen")
        {
            return "A mapped port is already in use on the host. \
                     Change the host port in your config's [network]ports section."
                .to_string();
        }
        if lower.contains("no such file") || lower.contains("not found") {
            return "A file or directory referenced in the config was not found. \
                     Verify all mount paths and the container image name."
                .to_string();
        }
    }
    "Run `podbox build --rebuild` to regenerate Quadlet files, \
     then `podbox enable` to reinstall them."
        .to_string()
}

/// Format a diagnostic card as a string.
pub(crate) fn diagnostic_card(name: &str, status: &UnitStatus, journal: Option<&str>) -> String {
    let (error_msg, hint) = diagnose(status, journal);

    let error_line = format!("   LoadError: {error_msg}");

    let unit_line = format!("  Unit:         {name}.service");
    let load_line = format!("  LoadState:    {}", status.load_state);
    let active_line = format!("  ActiveState:  {}", status.active_state);
    let sub_line = format!("  SubState:     {}", status.sub_state);
    let error_label = if error_msg.is_empty() {
        String::new()
    } else {
        format!("\n  {error_line}")
    };
    let reload_line = if status.need_daemon_reload {
        "\n  Note: systemd indicated NeedDaemonReload=yes. \
         A daemon-reload was triggered.\n"
            .to_string()
    } else {
        String::new()
    };

    let journal_section = match journal {
        Some(j) if !j.trim().is_empty() => {
            let lines: Vec<&str> = j.lines().collect();
            let tail = if lines.len() > 10 {
                &lines[lines.len() - 10..]
            } else {
                &lines
            };
            let body = tail
                .iter()
                .map(|l| format!("    {l}"))
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n  Journal (last {} lines):\n{}", tail.len(), body)
        }
        _ => String::new(),
    };

    format!(
        "\nError: Container '{name}' failed to start.\n\
         \n\
         Diagnostics:\n\
         {unit_line}\n\
         {load_line}\n\
         {active_line}\n\
         {sub_line}{error_label}{reload_line}\
         \n\
         Hint: {hint}\
         {journal_section}\n\
         \n\
         Run `podbox build --rebuild` and `podbox enable` to regenerate and \
         reinstall Quadlet files, then try again.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_show_output() -> &'static str {
        "LoadState=loaded\nActiveState=active\nSubState=running\nLoadError=\nNeedDaemonReload=no\n"
    }

    fn sample_show_bad_env() -> &'static str {
        "LoadState=bad-setting\nActiveState=failed\nSubState=failed\nLoadError=Invalid environment assignment on line 23.\nNeedDaemonReload=no\n"
    }

    #[test]
    fn parse_loaded_unit() {
        let s = parse_unit_show(sample_show_output());
        assert_eq!(s.load_state, "loaded");
        assert_eq!(s.active_state, "active");
        assert_eq!(s.sub_state, "running");
        assert!(s.load_error.is_empty());
        assert!(!s.need_daemon_reload);
    }

    #[test]
    fn parse_bad_setting() {
        let s = parse_unit_show(sample_show_bad_env());
        assert_eq!(s.load_state, "bad-setting");
        assert_eq!(s.active_state, "failed");
        assert!(!s.load_error.is_empty());
        assert!(s.load_error.contains("Invalid environment"));
    }

    #[test]
    fn parse_with_daemon_reload() {
        let raw = "LoadState=loaded\nActiveState=inactive\nSubState=dead\nLoadError=\nNeedDaemonReload=yes\n";
        let s = parse_unit_show(raw);
        assert!(s.need_daemon_reload);
    }

    #[test]
    fn parse_empty_output() {
        let s = parse_unit_show("");
        assert!(s.load_state.is_empty());
        assert!(!s.need_daemon_reload);
    }

    #[test]
    fn diagnose_bad_environment() {
        let s = parse_unit_show(sample_show_bad_env());
        let (err, _hint) = diagnose(&s, None);
        assert!(err.contains("Invalid environment"));
    }

    #[test]
    fn diagnose_healthy_unit() {
        let s = parse_unit_show(sample_show_output());
        let (err, _hint) = diagnose(&s, None);
        assert!(err.contains("ActiveState=active"));
    }

    #[test]
    fn diagnostic_card_renders() {
        let s = parse_unit_show(sample_show_bad_env());
        let card = diagnostic_card("dev", &s, Some("test journal line\nanother line\n"));
        assert!(card.contains("dev"));
        assert!(card.contains("bad-setting"));
        assert!(card.contains("Invalid environment"));
        assert!(card.contains("Hint:"));
    }

    #[test]
    fn diagnostic_card_with_journal() {
        let s = UnitStatus::default();
        let journal = "Jun 15 10:00:00 systemd[1]: podbox-dev.service: Failed with result exit-code.\nJun 15 10:00:00 systemd[1]: podbox-dev.service: Main process exited, code=exited, status=1/FAILURE\n";
        let card = diagnostic_card("test", &s, Some(journal));
        assert!(card.contains("Journal"));
        assert!(card.contains("test"));
    }
}
