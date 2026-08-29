use anyhow::Result;
use owo_colors::{OwoColorize, Stream};

use podbox::cli::OutputFormat;
use podbox::history;

/// Print the recorded lifecycle history for a container (or all containers).
///
/// Read-only: never creates the log, never prompts, works on a machine
/// contract. Missing log / no matching entries => empty output, exit 0.
pub fn run_history(name: Option<String>, limit: usize, output: OutputFormat) -> Result<()> {
    let entries = history::load().unwrap_or_default();
    let filtered: Vec<&history::HistoryEntry> = entries
        .iter()
        .filter(|e| name.as_deref().is_none_or(|n| e.name == n))
        .take(if limit == 0 { usize::MAX } else { limit })
        .collect();

    if matches!(output, OutputFormat::Json) {
        let json = serde_json::json!({ "history": filtered });
        println!("{}", serde_json::to_string_pretty(&json)?);
        return Ok(());
    }

    if filtered.is_empty() {
        return Ok(());
    }

    println!(
        "{:<20} {:<22} {:<14} {}",
        "TIME".if_supports_color(Stream::Stdout, |s| s.bold()),
        "CONTAINER".if_supports_color(Stream::Stdout, |s| s.bold()),
        "ACTION".if_supports_color(Stream::Stdout, |s| s.bold()),
        "DETAIL".if_supports_color(Stream::Stdout, |s| s.bold()),
    );
    println!("{}", "─".repeat(70));

    for e in filtered {
        let action = match e.action.as_str() {
            "start" | "enable" | "recover" => e
                .action
                .if_supports_color(Stream::Stdout, |s| s.green())
                .to_string(),
            "stop" | "disable" | "remove" => e
                .action
                .if_supports_color(Stream::Stdout, |s| s.red())
                .to_string(),
            _ => e
                .action
                .if_supports_color(Stream::Stdout, |s| s.cyan())
                .to_string(),
        };
        println!(
            "{:<20} {:<22} {:<14} {}",
            e.timestamp, e.name, action, e.detail
        );
    }

    Ok(())
}
