//! `podbox logs`: journalctl for systemd-managed containers, `podman logs`
//! for standalone ones.
//!
//! Extracted from `runtime.rs` with the rest of the session surface.

use std::ffi::OsString;

use anyhow::Result;

use super::doctor::is_systemd_managed;

/// Show container logs, routing through journalctl for systemd-managed
/// containers and falling back to `podman logs` for standalone ones.
pub fn run_logs(
    name: &str,
    follow: bool,
    tail: Option<u32>,
    since: Option<String>,
    dry_run: bool,
) -> Result<()> {
    let lines = tail.unwrap_or(50);

    if is_systemd_managed(name) {
        let mut args: Vec<OsString> = vec![
            "--user".into(),
            "-u".into(),
            format!("{name}.service").into(),
        ];
        if follow {
            args.push("-f".into());
        }
        args.push("-n".into());
        args.push(lines.to_string().into());
        if let Some(s) = &since {
            args.push("--since".into());
            args.push(s.into());
        }
        if dry_run {
            println!("journalctl {}", super::args_to_string(&args));
            return Ok(());
        }
        println!("Showing logs for: {name}.service");
        podbox::process::spawn_interactive_checked("journalctl", &args)
    } else {
        let mut args: Vec<OsString> = vec!["logs".into()];
        if follow {
            args.push("-f".into());
        }
        args.push("--tail".into());
        args.push(lines.to_string().into());
        if let Some(s) = &since {
            args.push("--since".into());
            args.push(s.into());
        }
        args.push(name.into());
        if dry_run {
            println!("podman {}", super::args_to_string(&args));
            return Ok(());
        }
        podbox::process::spawn_interactive_checked("podman", &args)
    }
}
