mod daemon;
mod entry;
mod error;
mod interceptors;
mod protocol;
mod socket;

use std::path::Path;

pub const VERSION: &str = env!("PODBOX_VERSION");

fn main() {
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
        _ => {
            eprintln!("Unknown invocation: argv[0] = {argv0}");
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("podbox-guest error: {e}");
        std::process::exit(1);
    }
}
