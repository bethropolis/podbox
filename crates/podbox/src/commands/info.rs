use anyhow::Result;

use podbox::cli::OutputFormat;

/// Show podbox/podman versions and client-contract capabilities.
///
/// Needs no config and never touches a container: safe as the first probe
/// a client runs. `podman` is best-effort — a missing podman reports null,
/// it never fails the command.
pub fn run_info(output: OutputFormat) -> Result<()> {
    let podman = podbox::podman::podman_version()
        .ok()
        .map(|v| format!("{}.{}.{}", v.major, v.minor, v.patch));
    match output {
        OutputFormat::Json => {
            podbox::contract::print_json(serde_json::json!({
                "podbox": podbox::VERSION,
                "podman": podman,
                "features": podbox::contract::CONTRACT_FEATURES,
            }))?;
        }
        OutputFormat::Text => {
            println!("podbox {}", podbox::VERSION);
            match &podman {
                Some(v) => println!("podman {v}"),
                None => println!("podman not installed"),
            }
            println!("contract schema {}", podbox::contract::CONTRACT_SCHEMA);
            println!(
                "features: {}",
                podbox::contract::CONTRACT_FEATURES.join(", ")
            );
        }
    }
    Ok(())
}
