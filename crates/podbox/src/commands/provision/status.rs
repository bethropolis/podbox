use anyhow::Result;
use podbox::config::Config;
use serde::Serialize;

use super::state::{ProvisionState, classify};

/// One row of `provision status`.
#[derive(Debug, Clone, Serialize)]
pub struct StatusRow {
    pub name: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<u64>,
}

/// `provision status`: one row per step plus orphans.
pub fn status(config: &Config, output: podbox::cli::OutputFormat, check: bool) -> Result<()> {
    // One classification: still report orphans when the config dropped all
    // steps, but don't walk the stamp directory twice.
    let rows = classify(config);
    if config.provision.is_empty() && rows.is_empty() {
        anyhow::bail!(
            "no [[provision]] steps configured for '{}'",
            config.container.name
        );
    }
    match output {
        podbox::cli::OutputFormat::Json => {
            let out: Vec<StatusRow> = rows
                .iter()
                .map(|(name, state, at)| StatusRow {
                    name: name.clone(),
                    state: state.as_str().to_string(),
                    applied_at: *at,
                })
                .collect();
            podbox::contract::print_json(serde_json::json!({ "steps": out }))?;
        }
        podbox::cli::OutputFormat::Ndjson => {
            return Err(podbox::contract::bail_ndjson("provision status"));
        }
        podbox::cli::OutputFormat::Text => {
            println!("Container: {}", config.container.name);
            for (name, state, at) in &rows {
                match at {
                    Some(t) => println!("{name}: {} (applied at {t})", state.as_str()),
                    None => println!("{name}: {}", state.as_str()),
                }
            }
        }
    }
    if check {
        let bad = rows
            .iter()
            .any(|(_, s, _)| matches!(s, ProvisionState::Pending | ProvisionState::Stale));
        if bad {
            return Err(podbox::error::PodboxError::StepsPending.into());
        }
    }
    Ok(())
}
