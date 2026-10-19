//! `podbox status`: one container's state line, human or machine-readable.
//!
//! Extracted from `runtime.rs` with the rest of the status surface.

use podbox::config::Config;
use podbox::podman::{ContainerState, query_state};

/// Quadlet installed for `name`? Used to explain an `unbuilt` container.
fn quadlet_installed(name: &str) -> bool {
    podbox::quadlet_install::is_installed(name)
}

/// Print the container's running state.
pub fn run_status(
    config: &Config,
    name: &str,
    dry_run: bool,
    output: podbox::cli::OutputFormat,
) -> anyhow::Result<()> {
    if matches!(output, podbox::cli::OutputFormat::Ndjson) {
        return Err(podbox::contract::bail_ndjson("status"));
    }
    if matches!(output, podbox::cli::OutputFormat::Json) {
        let state = query_state(name)?;
        // Canonical vocabulary shared with `podbox list --output json`:
        // running | stopped | failed | unbuilt. The Quadlet distinction is
        // preserved as a separate boolean rather than a second status word.
        let (status, installed) = match state {
            ContainerState::Running => ("running", true),
            ContainerState::Stopped if podbox::systemd::is_unit_failed(name) => ("failed", true),
            ContainerState::Stopped => ("stopped", true),
            ContainerState::Missing => ("unbuilt", quadlet_installed(name)),
        };
        let mut obj = serde_json::Map::new();
        obj.insert("name".into(), serde_json::json!(name));
        obj.insert("status".into(), serde_json::json!(status));
        obj.insert("installed".into(), serde_json::json!(installed));
        let base_image = match config.image.source() {
            podbox::config::ImageSource::Build { base } => base.clone(),
            podbox::config::ImageSource::Prebuilt { ref_str } => ref_str.clone(),
        };
        obj.insert("base_image".into(), serde_json::json!(base_image));
        // Optional enrichment: present when readable, absent otherwise, so
        // a client needs one call instead of four.
        if let Some(started) = podbox::podman::started_at(name) {
            obj.insert("started_at".into(), serde_json::json!(started));
        }
        if status == "running" {
            if let Some(stats) = podbox::podman::stats_sample(name) {
                obj.insert("cpu_percent".into(), serde_json::json!(stats.cpu_percent));
                obj.insert("memory_bytes".into(), serde_json::json!(stats.memory_bytes));
                obj.insert(
                    "memory_limit_bytes".into(),
                    serde_json::json!(stats.memory_limit_bytes),
                );
                obj.insert("pids".into(), serde_json::json!(stats.pids));
            }
        }
        podbox::contract::print_json(serde_json::Value::Object(obj))?;
        return Ok(());
    }

    if dry_run {
        println!("podman inspect --format {{{{.State.Status}}}} {name}");
        return Ok(());
    }
    let state = query_state(name)?;
    match state {
        ContainerState::Running => println!("{name} [running]"),
        ContainerState::Stopped if podbox::systemd::is_unit_failed(name) => {
            println!("{name} [failed]");
        }
        ContainerState::Stopped => println!("{name} [stopped]"),
        ContainerState::Missing => println!("{name} [unbuilt]"),
    }
    Ok(())
}
