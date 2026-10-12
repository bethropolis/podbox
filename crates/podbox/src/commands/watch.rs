use std::collections::BTreeMap;
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
    interval_secs: u64,
    output: OutputFormat,
    dry_run: bool,
) -> Result<()> {
    if matches!(output, OutputFormat::Json) {
        anyhow::bail!("`podbox watch` streams ndjson or text, not pretty json");
    }
    let interval = Duration::from_secs(interval_secs.max(1));
    let only = name.clone();
    let targets = current_names(only.as_deref());
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
        let current = current_names(only.as_deref());
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

/// Managed container names: just `name` when filtering, else every config
/// on disk (re-enumerated each tick so new boxes join the stream).
fn current_names(only: Option<&str>) -> Vec<String> {
    if let Some(name) = only {
        return vec![name.to_string()];
    }
    podbox::config::list_configs()
        .iter()
        .filter_map(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .collect()
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
