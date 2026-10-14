use anyhow::Result;

use podbox::cli::OutputFormat;
use podbox::process;

/// Show resource usage for a container (wraps `podman stats`).
pub fn run_stats(name: &str, no_stream: bool, output: OutputFormat) -> Result<()> {
    // Containers are bare-named (ContainerName={name}); the `podbox-` prefix
    // belongs to image tags, never to `podman` container arguments.
    let container_name = name.to_string();

    if matches!(output, OutputFormat::Json) {
        let mut args = process::args(&["stats", "--format", "json"]);
        if no_stream {
            args.push("--no-stream".into());
        }
        args.push(container_name.into());
        process::spawn_interactive_checked("podman", &args)?;
        return Ok(());
    }

    let mut args = process::args(&["stats"]);
    if no_stream {
        args.push("--no-stream".into());
    }
    args.push(container_name.into());

    process::spawn_interactive_checked("podman", &args)?;
    Ok(())
}
