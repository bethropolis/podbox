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
        let config_path = config::find_config_path(serve_name).ok_or_else(|| {
            anyhow::anyhow!(
                "no config found for container '{}' at '{}/{{profiles/,}}{}.toml'",
                serve_name,
                config::config_dir().display(),
                serve_name
            )
        })?;
        Config::load(&config_path)?
    };
    let xdg_runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| {
        let uid = nix::unistd::getuid().as_raw();
        format!("/run/user/{uid}")
    });
    let socket_path = PathBuf::from(&xdg_runtime)
        .join("podbox")
        .join(format!("{serve_name}.sock"));
    podbox::socket_host::run(&socket_path, &serve_config, serve_name)?;
    Ok(())
}
