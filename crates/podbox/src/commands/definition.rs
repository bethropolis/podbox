use anyhow::Result;
use owo_colors::{OwoColorize, Stream};

use podbox::cli::OutputFormat;
use podbox::config;

/// Print the path to the definition file.
pub fn run_find_definition(name: Option<&str>) -> Result<()> {
    match name {
        Some(n) => {
            let path = config::config_dir().join(format!("{n}.toml"));
            if path.exists() {
                println!("{}", path.display());
            } else {
                println!("(no config found for '{n}')");
            }
        }
        None => match config::find_definition() {
            Some(path) => println!("{}", path.display()),
            None => println!("(embedded default)"),
        },
    }
    Ok(())
}

/// Generate shell completions.
pub fn run_completions(shell: clap_complete::shells::Shell) -> Result<()> {
    let mut cmd = <podbox::cli::Cli as clap::CommandFactory>::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    Ok(())
}

/// List all podbox-managed containers with status, autostart, and active context.
pub fn run_list(output: OutputFormat) -> Result<()> {
    let configs = config::list_configs();
    let active_ctx = config::read_active_context();

    if matches!(output, OutputFormat::Json) {
        let entries: Vec<serde_json::Value> = configs
            .iter()
            .map(|cp| {
                let name = cp
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let state_label = match podbox::podman::query_state(&name) {
                    Ok(podbox::podman::ContainerState::Running) => "running",
                    Ok(podbox::podman::ContainerState::Stopped)
                        if podbox::systemd::is_unit_failed(&name) =>
                    {
                        "failed"
                    }
                    Ok(podbox::podman::ContainerState::Stopped) => "stopped",
                    Ok(podbox::podman::ContainerState::Missing) => "unbuilt",
                    Err(_) => "unknown",
                };
                let autostart = config::Config::load(cp).is_ok_and(|c| c.lifecycle.autostart);
                serde_json::json!({
                    "name": name,
                    "status": state_label,
                    "autostart": autostart,
                    "active": active_ctx.as_deref() == Some(&name),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"containers": entries}))?
        );
        return Ok(());
    }

    if configs.is_empty() {
        println!("No containers found. Create your first container with `podbox init -i`.");
        return Ok(());
    }

    println!(
        "{:<20} {:<24} {:<10} {}",
        "CONTAINER".if_supports_color(Stream::Stdout, |s| s.bold()),
        "STATUS".if_supports_color(Stream::Stdout, |s| s.bold()),
        "AUTOSTART".if_supports_color(Stream::Stdout, |s| s.bold()),
        "ACTIVE CONTEXT".if_supports_color(Stream::Stdout, |s| s.bold()),
    );
    println!("{}", "─".repeat(75));

    for config_path in configs {
        let name = config_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let status = format_status(&name);
        let autostart = format_autostart(&config_path);
        let active = format_active(&name, &active_ctx);

        println!("{name:<20} {status:<24} {autostart:<10} {active}");
    }

    Ok(())
}

/// Format the container status with owo-colors styling.
fn format_status(name: &str) -> String {
    let (dot, label) = match podbox::podman::query_state(name) {
        Ok(podbox::podman::ContainerState::Running) => {
            ("●".if_supports_color(Stream::Stdout, |s| s.green()).to_string(), "running")
        }
        Ok(podbox::podman::ContainerState::Stopped) => {
            if podbox::systemd::is_unit_failed(name) {
                ("⚠".if_supports_color(Stream::Stdout, |s| s.red()).to_string(), "failed")
            } else {
                ("○".if_supports_color(Stream::Stdout, |s| s.bright_black()).to_string(), "stopped")
            }
        }
        Ok(podbox::podman::ContainerState::Missing) => {
            ("○".if_supports_color(Stream::Stdout, |s| s.yellow()).to_string(), "unbuilt")
        }
        Err(_) => {
            ("?".if_supports_color(Stream::Stdout, |s| s.red()).to_string(), "unknown")
        }
    };
    format!("{dot} {label}")
}

/// Format the autostart column, reading from the TOML config.
fn format_autostart(config_path: &std::path::Path) -> String {
    let autostart = match config::Config::load(config_path) {
        Ok(cfg) => cfg.lifecycle.autostart,
        Err(_) => return "err".if_supports_color(Stream::Stdout, |s| s.red()).to_string(),
    };
    if autostart {
        "yes".if_supports_color(Stream::Stdout, |s| s.green()).to_string()
    } else {
        "no".to_string()
    }
}

/// Format the active-context marker.
fn format_active(name: &str, active_ctx: &Option<String>) -> String {
    if active_ctx.as_deref() == Some(name) {
        "★ active".if_supports_color(Stream::Stdout, |s| s.yellow()).to_string()
    } else {
        String::new()
    }
}
