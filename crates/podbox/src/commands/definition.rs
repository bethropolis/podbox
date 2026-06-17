use anyhow::Result;

use podbox::cli::OutputFormat;
use podbox::config;

/// Print the path to the definition file.
pub fn run_find_definition(name: Option<&str>) -> Result<()> {
    match name {
        Some(n) => {
            let path = config::config_dir().join(format!("{}.toml", n));
            if path.exists() {
                println!("{}", path.display());
            } else {
                println!("(no config found for '{}')", n);
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
    let use_color = podbox::codegen::distros::is_tty();

    if matches!(output, OutputFormat::Json) {
        let entries: Vec<serde_json::Value> = configs
            .iter()
            .map(|cp| {
                let name = cp.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let state_label = match podbox::podman::query_state(&name) {
                    Ok(podbox::podman::ContainerState::Running) => "running",
                    Ok(podbox::podman::ContainerState::Stopped)
                        if podbox::systemd::is_unit_failed(&name) => "failed",
                    Ok(podbox::podman::ContainerState::Stopped) => "stopped",
                    Ok(podbox::podman::ContainerState::Missing) => "unbuilt",
                    Err(_) => "unknown",
                };
                let autostart = config::Config::load(cp)
                    .map(|c| c.lifecycle.autostart)
                    .unwrap_or(false);
                serde_json::json!({
                    "name": name,
                    "status": state_label,
                    "autostart": autostart,
                    "active": active_ctx.as_deref() == Some(&name),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&serde_json::json!({"containers": entries}))?);
        return Ok(());
    }

    if configs.is_empty() {
        println!("No containers found. Create your first container with `podbox init -i`.");
        return Ok(());
    }

    println!(
        "{:<20} {:<24} {:<10} {}",
        color_if("CONTAINER", use_color, "\x1b[1m", "\x1b[0m"),
        color_if("STATUS", use_color, "\x1b[1m", "\x1b[0m"),
        color_if("AUTOSTART", use_color, "\x1b[1m", "\x1b[0m"),
        color_if("ACTIVE CONTEXT", use_color, "\x1b[1m", "\x1b[0m"),
    );
    println!("{}", "─".repeat(75));

    for config_path in configs {
        let name = config_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let status = format_status(&name, use_color);
        let autostart = format_autostart(&config_path, use_color);
        let active = format_active(&name, &active_ctx, use_color);

        println!("{:<20} {:<24} {:<10} {}", name, status, autostart, active);
    }

    Ok(())
}

/// Format the container status with optional ANSI color.
fn format_status(name: &str, color: bool) -> String {
    let (dot, label) = match podbox::podman::query_state(name) {
        Ok(podbox::podman::ContainerState::Running) => {
            if color { ("\x1b[32m●\x1b[0m", "running") } else { ("●", "running") }
        }
        Ok(podbox::podman::ContainerState::Stopped) => {
            if podbox::systemd::is_unit_failed(name) {
                if color { ("\x1b[31m⚠\x1b[0m", "failed") } else { ("⚠", "failed") }
            } else {
                if color { ("\x1b[90m○\x1b[0m", "stopped") } else { ("○", "stopped") }
            }
        }
        Ok(podbox::podman::ContainerState::Missing) => {
            if color { ("\x1b[33m○\x1b[0m", "unbuilt") } else { ("○", "unbuilt") }
        }
        Err(_) => {
            if color { ("\x1b[31m?\x1b[0m", "unknown") } else { ("?", "unknown") }
        }
    };
    format!("{} {}", dot, label)
}

/// Format the autostart column, reading from the TOML config.
fn format_autostart(config_path: &std::path::Path, color: bool) -> String {
    let autostart = match config::Config::load(config_path) {
        Ok(cfg) => cfg.lifecycle.autostart,
        Err(_) => return color_if("err", color, "\x1b[31m", "\x1b[0m"),
    };
    if autostart {
        color_if("yes", color, "\x1b[32m", "\x1b[0m")
    } else {
        "no".to_string()
    }
}

/// Format the active-context marker.
fn format_active(name: &str, active_ctx: &Option<String>, color: bool) -> String {
    if active_ctx.as_deref() == Some(name) {
        if color {
            "\x1b[33m★ active\x1b[0m".to_string()
        } else {
            "active".to_string()
        }
    } else {
        String::new()
    }
}

/// Wrap text in ANSI color/reset if color is enabled.
fn color_if(text: &str, color: bool, ansi_on: &str, ansi_off: &str) -> String {
    if color {
        format!("{}{}{}", ansi_on, text, ansi_off)
    } else {
        text.to_string()
    }
}
