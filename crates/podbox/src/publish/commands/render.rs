use anyhow::Result;
use serde_json::{Value, json};

use crate::contract;
use crate::publish::events::{done_document, done_event, image_event, plan_entry};
use crate::publish::plan::Plan;

use super::OutputFormat;
use super::pipeline::PublishSummary;

/// Print the plan for `--list`.
pub(super) fn print_list(plan: &Plan, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => {
            let entries: Vec<Value> = plan.images.iter().map(plan_entry).collect();
            contract::print_json(json!({"images": entries}))?;
        }
        OutputFormat::Ndjson => {
            for image in &plan.images {
                contract::emit_event(plan_entry(image));
            }
        }
        OutputFormat::Text => {
            for image in &plan.images {
                println!("{}  {}  {}", image.key, image.ref_str, image.tags.join(","));
            }
        }
    }
    Ok(())
}

/// Emit final image/done events after the pipeline has completed.
pub(super) fn render_summary(summary: &PublishSummary, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Ndjson => {
            for event in &summary.events {
                contract::emit_event(image_event(event));
            }
            let ok = summary.failed == 0;
            contract::emit_event(done_event(
                ok,
                summary.events.len(),
                summary.pushed,
                summary.skipped,
                summary.failed,
            ));
        }
        OutputFormat::Json => {
            let mut json_events = Vec::new();
            for event in &summary.events {
                json_events.push(image_event(event));
            }
            let ok = summary.failed == 0;
            contract::print_json(done_document(
                ok,
                json_events,
                summary.pushed,
                summary.skipped,
                summary.failed,
            ))?;
        }
        OutputFormat::Text => {
            if summary.failed > 0 {
                eprintln!("{} image(s) failed", summary.failed);
            }
        }
    }
    Ok(())
}
