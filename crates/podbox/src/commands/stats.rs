use anyhow::Result;

use podbox::cli::OutputFormat;
use podbox::process;

/// Show resource usage for a container (wraps `podman stats`).
pub fn run_stats(name: &str, no_stream: bool, output: OutputFormat) -> Result<()> {
    let container_name = format!("podbox-{name}");

    if matches!(output, OutputFormat::Json) {
        let mut args = process::args(&["stats", "--format", "json"]);
        if no_stream {
            args.push("--no-stream".into());
        }
        args.push(container_name.into());
        process::spawn_interactive("podman", &args)?;
        return Ok(());
    }

    let mut args = process::args(&["stats"]);
    if no_stream {
        args.push("--no-stream".into());
    }
    args.push(container_name.into());

    process::spawn_interactive("podman", &args)?;
    Ok(())
}
