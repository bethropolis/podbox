use anyhow::Result;

use podbox::cli::OutputFormat;
use podbox::process;

/// Show resource usage for a container (wraps `podman stats`).
pub fn run_stats(name: &str, no_stream: bool, output: OutputFormat) -> Result<()> {
    // Containers are bare-named (ContainerName={name}); the `podbox-` prefix
    // belongs to image tags, never to `podman` container arguments.
    let container_name = name.to_string();

    if matches!(output, OutputFormat::Ndjson) {
        return Err(podbox::contract::bail_ndjson("stats"));
    }

    if matches!(output, OutputFormat::Json) {
        let mut args = process::args(&["stats", "--format", "json"]);
        if no_stream {
            args.push("--no-stream".into());
        }
        args.push(container_name.into());
        let raw = process::run_checked("podman", &args)?;
        // podman emits an array with --no-stream and may emit one object per
        // line while streaming; accept both under a contract envelope.
        let stats = match serde_json::from_slice::<serde_json::Value>(&raw.stdout) {
            Ok(value) => value,
            Err(_) => serde_json::Value::Array(
                String::from_utf8_lossy(&raw.stdout)
                    .lines()
                    .filter(|line| !line.trim().is_empty())
                    .filter_map(|line| serde_json::from_str(line).ok())
                    .collect(),
            ),
        };
        podbox::contract::print_json(serde_json::json!({"stats": stats}))?;
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
