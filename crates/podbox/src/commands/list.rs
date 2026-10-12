//! `podbox list`: managed-container table with status, autostart and
//! active-context columns (JSON and human forms).
//!
//! Extracted verbatim from `commands/definition.rs`; re-exported from
//! `definition` so `commands::definition::run_list` keeps resolving.

use anyhow::Result;
use owo_colors::{OwoColorize, Stream};

use podbox::cli::OutputFormat;
use podbox::config;

/// List all podbox-managed containers with status, autostart, and active context.
///
/// Display names are runtime names (`container.name`), matching `status`,
/// units, and exports. The file stem is only the lookup key for `-C` and
/// may differ; unloadable configs fall back to their stem so they stay
/// visible instead of vanishing.
pub fn run_list(output: OutputFormat) -> Result<()> {
    if matches!(output, OutputFormat::Ndjson) {
        return Err(podbox::contract::bail_ndjson("list"));
    }
    let configs = config::list_configs();
    let active_inner = config::read_active_context().and_then(|stem| {
        config::find_config_path(&stem)
            .and_then(|path| config::Config::load(&path).ok())
            .map(|cfg| cfg.container.name)
    });

    if matches!(output, OutputFormat::Json) {
        let entries: Vec<serde_json::Value> = configs
            .iter()
            .map(|cp| {
                let stem = cp
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let loaded = config::Config::load(cp).ok();
                let name = loaded
                    .as_ref()
                    .map(|cfg| cfg.container.name.clone())
                    .unwrap_or_else(|| stem.clone());
                let state_label = podbox::podman::state_label(&name);
                let autostart = loaded.is_some_and(|cfg| cfg.lifecycle.autostart);
                serde_json::json!({
                    "name": name,
                    "status": state_label,
                    "autostart": autostart,
                    "active": active_inner.as_deref() == Some(&name),
                })
            })
            .collect();
        podbox::contract::print_json(serde_json::json!({"containers": entries}))?;
        return Ok(());
    }

    if configs.is_empty() {
        println!("No containers found. Create your first container with `podbox init -i`.");
        return Ok(());
    }

    println!(
        "{:<20} {:<17} {:<10} {}",
        "CONTAINER".if_supports_color(Stream::Stdout, |s| s.bold()),
        "STATUS".if_supports_color(Stream::Stdout, |s| s.bold()),
        "AUTOSTART".if_supports_color(Stream::Stdout, |s| s.bold()),
        "ACTIVE CONTEXT".if_supports_color(Stream::Stdout, |s| s.bold()),
    );
    println!("{}", "─".repeat(64));

    for config_path in configs {
        let stem = config_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let name = config::Config::load(&config_path)
            .map(|cfg| cfg.container.name)
            .unwrap_or_else(|_| stem.clone());

        // Colored cells are padded around their *plain* text (see
        // [`pad_around`]) so ANSI escapes never skew the column widths.
        let (dot, label) = status_parts(&name);
        let (auto_plain, auto_cell) = autostart_parts(&config_path);
        let (active_plain, active_cell) = active_parts(&name, &active_inner);

        let mut row = format!(
            "{name:<20} {dot} {label:<15} {}",
            pad_around(&auto_plain, &auto_cell, 10),
        );
        if !active_plain.is_empty() {
            row.push(' ');
            row.push_str(&active_cell);
        }
        println!("{}", row.trim_end());
    }

    Ok(())
}

/// Pad a cell whose rendered form contains ANSI escapes: `plain` supplies the
/// visible text for width math, `rendered` is what actually prints.
fn pad_around(plain: &str, rendered: &str, width: usize) -> String {
    let mut s = String::from(rendered);
    let visible = plain.chars().count();
    if visible < width {
        s.extend(std::iter::repeat_n(' ', width - visible));
    }
    s
}

/// Status cell split into a pre-colored dot and a plain label so the label
/// can be width-padded safely.
fn status_parts(name: &str) -> (String, &'static str) {
    match podbox::podman::query_state(name) {
        Ok(podbox::podman::ContainerState::Running) => (
            "●"
                .if_supports_color(Stream::Stdout, |s| s.green())
                .to_string(),
            "running",
        ),
        Ok(podbox::podman::ContainerState::Stopped) => {
            if podbox::systemd::is_unit_failed(name) {
                (
                    "⚠"
                        .if_supports_color(Stream::Stdout, |s| s.red())
                        .to_string(),
                    "failed",
                )
            } else {
                (
                    "○"
                        .if_supports_color(Stream::Stdout, |s| s.bright_black())
                        .to_string(),
                    "stopped",
                )
            }
        }
        Ok(podbox::podman::ContainerState::Missing) => (
            "○"
                .if_supports_color(Stream::Stdout, |s| s.yellow())
                .to_string(),
            "unbuilt",
        ),
        Err(_) => (
            "?".if_supports_color(Stream::Stdout, |s| s.red())
                .to_string(),
            "unknown",
        ),
    }
}

/// Autostart cell as (plain text, rendered form).
fn autostart_parts(config_path: &std::path::Path) -> (String, String) {
    match config::Config::load(config_path) {
        Ok(cfg) if cfg.lifecycle.autostart => (
            "yes".into(),
            "yes"
                .if_supports_color(Stream::Stdout, |s| s.green())
                .to_string(),
        ),
        Ok(_) => ("no".into(), "no".into()),
        Err(_) => (
            "err".into(),
            "err"
                .if_supports_color(Stream::Stdout, |s| s.red())
                .to_string(),
        ),
    }
}

/// Active-context marker as (plain text, rendered form).
fn active_parts(name: &str, active_ctx: &Option<String>) -> (String, String) {
    if active_ctx.as_deref() == Some(name) {
        (
            "★ active".into(),
            "★ active"
                .if_supports_color(Stream::Stdout, |s| s.yellow())
                .to_string(),
        )
    } else {
        (String::new(), String::new())
    }
}
