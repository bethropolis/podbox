use std::path::PathBuf;

use anyhow::Result;

use podbox::config::{self, Config};

/// Run the host-side socket server for a container.
pub fn run_serve(cli_config_path: Option<&PathBuf>, serve_name: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("podbox serve {serve_name}");
        return Ok(());
    }
    let serve_config = if let Some(path) = cli_config_path {
        Config::load(path)?
    } else {
        let config_path = config::find_config_path(serve_name)
            .ok_or_else(|| config::missing_config_error(serve_name))?;
        Config::load(&config_path)?
    };
    let socket_path = podbox::protocol::sysutil::xdg_runtime_dir()
        .join("podbox")
        .join(format!("{serve_name}.sock"));
    podbox::socket_host::run(&socket_path, &serve_config, serve_name)?;
    Ok(())
}
