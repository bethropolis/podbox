use anyhow::Result;

use podbox::cli::ExportCommand;
use podbox::config::Config;

/// Export an app (desktop entry) or binary from the container.
pub fn run_export(
    name: &str,
    config: Option<&Config>,
    export_cmd: &ExportCommand,
    dry_run: bool,
) -> Result<()> {
    // The export helpers write desktop entries and wrappers directly, so
    // there is no plan to print. Refuse rather than act under --dry-run.
    if dry_run && !matches!(export_cmd, ExportCommand::List { .. }) {
        anyhow::bail!("--dry-run is not supported for `podbox export` (only `export list`)");
    }
    match export_cmd {
        ExportCommand::App {
            name: app_name,
            all,
        } => {
            if *all {
                let cfg =
                    config.ok_or_else(|| anyhow::anyhow!("Config required for --all export"))?;
                for app in &cfg.integration.export.apps {
                    if let Err(e) = podbox::export::export_app(name, app) {
                        eprintln!("Warning: export app '{app}' failed: {e}");
                    }
                }
            } else if let Some(app_name) = app_name {
                podbox::export::export_app(name, app_name)?;
            }
        }
        ExportCommand::Bin {
            name: bin_name,
            all,
        } => {
            if *all {
                let cfg =
                    config.ok_or_else(|| anyhow::anyhow!("Config required for --all export"))?;
                for bin in &cfg.integration.export.bins {
                    if let Err(e) = podbox::export::export_bin(name, bin) {
                        eprintln!("Warning: export bin '{bin}' failed: {e}");
                    }
                }
            } else if let Some(bin_name) = bin_name {
                podbox::export::export_bin(name, bin_name)?;
            }
        }
        ExportCommand::Clean => {
            podbox::export::unexport_all(name)?;
        }
        ExportCommand::Remove { target, item } => match target {
            podbox::cli::ExportTarget::App => {
                podbox::export::unexport_app(name, item)?;
            }
            podbox::cli::ExportTarget::Bin => {
                podbox::export::unexport_bin(name, item)?;
            }
        },
        ExportCommand::List { output } => {
            let (declared_apps, declared_bins): (&[String], &[String]) = match config {
                Some(cfg) => (&cfg.integration.export.apps, &cfg.integration.export.bins),
                None => (&[], &[]),
            };
            let inv = podbox::export::inventory(name, declared_apps, declared_bins);
            match output {
                podbox::cli::OutputFormat::Json => {
                    podbox::contract::print_json(serde_json::to_value(&inv)?)?;
                }
                podbox::cli::OutputFormat::Ndjson => {
                    return Err(podbox::contract::bail_ndjson("export list"));
                }
                podbox::cli::OutputFormat::Text => {
                    podbox::export::print_inventory_text(name, &inv);
                }
            }
        }
    }
    Ok(())
}
