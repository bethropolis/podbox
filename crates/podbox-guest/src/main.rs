mod daemon;
mod entry;
mod error;
mod interceptors;
mod protocol;
mod socket;

use std::path::Path;

pub const VERSION: &str = env!("PODBOX_VERSION");

fn init_tracing() {
    use tracing_subscriber::prelude::*;
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    if let Ok(layer) = tracing_journald::layer() {
        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(layer)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(std::io::stderr)
            .try_init();
    }
}

fn main() {
    init_tracing();
    let args: Vec<String> = std::env::args().collect();
    let argv0 = args
        .first()
        .map_or("podbox-guest", std::string::String::as_str);
    let name = Path::new(argv0)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("podbox-guest");

    let result = match name {
        "podbox-guest" | "podmgr-guest" => match args.get(1).map(std::string::String::as_str) {
            Some("--daemon") => daemon::run(),
            Some("--entry") => {
                let cmd = if args.len() > 2 {
                    args[2..].to_vec()
                } else {
                    Vec::new()
                };
                entry::run(&cmd);
            }
            _ => {
                eprintln!("Usage: podbox-guest --daemon | --entry <cmd...>");
                std::process::exit(1);
            }
        },
        "notify-send" => {
            interceptors::notify::run(&args);
            Ok(())
        }
        "host-exec" => {
            interceptors::host_exec::run(&args);
            Ok(())
        }
        "xdg-open" => {
            interceptors::xdg_open::run(&args);
            Ok(())
        }
        "podbox-clipboard" | "podmgr-clipboard" => {
            interceptors::clipboard::run(&args);
            Ok(())
        }
        custom_cmd => {
            interceptors::host_exec::run_as_command(custom_cmd, &args[1..]);
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("podbox-guest error: {e}");
        std::process::exit(1);
    }
}
