use anyhow::Result;

use podbox::process;

/// Show resource usage for a container (wraps `podman stats`).
pub fn run_stats(name: &str, no_stream: bool) -> Result<()> {
    let container_name = format!("podbox-{}", name);

    let mut args = process::args(&["stats"]);
    if no_stream {
        args.push("--no-stream".into());
    }
    args.push(container_name.into());

    process::spawn_interactive("podman", &args)?;
    Ok(())
}
