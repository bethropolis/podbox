use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;

use podbox::cli::OutputFormat;

/// Stream container status changes until stopped.
///
/// Prints the current state once, then one line per change. Foreground
/// only: the client owns this process and stops it (Ctrl-C exits cleanly
/// with no cleanup needed). Nothing runs in the background, so the
/// no-daemon promise holds.
///
/// `ndjson` prints one compact object per line (`schema`, `event`,
/// `name`, `status`); `text` prints human transitions. Pretty `json` is
/// rejected: a multi-line document per event would break line parsers.
pub fn run_watch(
    name: Option<String>,
    config_path: Option<PathBuf>,
    interval_secs: u64,
    output: OutputFormat,
    dry_run: bool,
) -> Result<()> {
    if matches!(output, OutputFormat::Json) {
        anyhow::bail!("`podbox watch` streams ndjson or text, not pretty json");
    }
    let interval = Duration::from_secs(interval_secs.max(1));
    let targets = resolve_targets(name.clone(), config_path.clone())?;
    if dry_run {
        println!(
            "would poll {} container(s) every {}s",
            targets.len(),
            interval.as_secs()
        );
        return Ok(());
    }

    let mut last: BTreeMap<String, String> = BTreeMap::new();
    for target in &targets {
        let status = podbox::podman::state_label(target).to_string();
        emit(target, None, &status, output);
        last.insert(target.clone(), status);
    }

    loop {
        std::thread::sleep(interval);
        // Re-resolve each tick so new boxes join the stream. A transient
        // failure (half-written file) keeps the previous set quietly; the
        // next tick recovers instead of emitting spurious transitions.
        let current = resolve_targets(name.clone(), config_path.clone())
            .unwrap_or_else(|_| last.keys().cloned().collect());
        for target in &current {
            let status = podbox::podman::state_label(target).to_string();
            if last.get(target).is_none_or(|prev| prev != &status) {
                emit(target, last.get(target), &status, output);
                last.insert(target.clone(), status);
            }
        }
        last.retain(|known, _| current.iter().any(|c| c == known));
    }
}

/// Resolve watch targets to runtime names (`container.name`).
///
/// A `--config` path wins; otherwise a name resolves through the config
/// lookup (unknown names fail fast instead of streaming `unbuilt` forever);
/// with no filter every config on disk is watched by runtime name.
fn resolve_targets(only: Option<String>, config_path: Option<PathBuf>) -> Result<Vec<String>> {
    if let Some(path) = config_path {
        let cfg = podbox::config::Config::load(&path)?;
        return Ok(vec![cfg.container.name]);
    }
    match only {
        Some(name) => {
            let path = podbox::config::find_config_path(&name)
                .ok_or_else(|| podbox::error::PodboxError::ContainerMissing(name.clone()))?;
            let cfg = podbox::config::Config::load(&path)?;
            Ok(vec![cfg.container.name])
        }
        None => Ok(podbox::config::list_configs()
            .iter()
            .map(|p| {
                let stem = p
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                podbox::config::Config::load(p)
                    .map(|cfg| cfg.container.name)
                    .unwrap_or(stem)
            })
            .collect()),
    }
}

fn emit(name: &str, previous: Option<&String>, status: &str, output: OutputFormat) {
    match output {
        OutputFormat::Ndjson => {
            podbox::contract::emit_event(serde_json::json!({
                "event": "status",
                "name": name,
                "status": status,
            }));
        }
        _ => match previous {
            Some(prev) => println!("{name}: {prev} -> {status}"),
            None => println!("{name}: {status}"),
        },
    }
}
