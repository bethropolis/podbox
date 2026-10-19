//! Binary entrypoint: option parsing, tracing init, and the exit-code
//! mapping of the client contract. Everything else lives in the `main/*`
//! modules — [`dispatch`] classifies and runs commands, [`resolve`] is the
//! single config-resolution chain, [`handlers`] holds shared command
//! helpers, and [`ctx`] threads dispatch-level flags into handlers.

use std::process::ExitCode;

use anyhow::Result;
use clap::Parser;

use podbox::cli::Cli;
use podbox::error::PodboxError;
use podbox::ui;

mod commands;

#[path = "main/ctx.rs"]
mod ctx;

#[path = "main/dispatch.rs"]
mod dispatch;

#[path = "main/handlers.rs"]
mod handlers;

#[path = "main/resolve.rs"]
mod resolve;

use ctx::Ctx;

fn init_tracing(verbosity: u8) {
    use tracing_subscriber::prelude::*;
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let level = match verbosity {
            0 => "info",
            1 => "debug",
            _ => "trace",
        };
        tracing_subscriber::EnvFilter::new(level)
    });
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

fn main() -> ExitCode {
    let cli = Cli::parse();
    let format = dispatch::command_output_format(&cli.command);
    if let Err(e) = run(cli) {
        // Contract: in JSON mode failures also print a structured error on
        // stdout (same exit code). Diagnostics stay on stderr either way.
        // Skipped when the command already printed its JSON report and the
        // error only carries the exit status (doctor failures, --check).
        // NDJSON streams get the compact one-line form so line parsers keep
        // working; the envelope terminates the stream.
        if matches!(
            format,
            Some(podbox::cli::OutputFormat::Json | podbox::cli::OutputFormat::Ndjson)
        ) {
            let reported = e
                .downcast_ref::<PodboxError>()
                .is_some_and(PodboxError::json_reported);
            if !reported {
                let code = e
                    .downcast_ref::<PodboxError>()
                    .map_or("error", PodboxError::code);
                let payload = podbox::contract::error_payload(code, &format!("{e:#}"));
                let text = match format {
                    Some(podbox::cli::OutputFormat::Ndjson) => serde_json::to_string(&payload),
                    _ => serde_json::to_string_pretty(&payload),
                };
                if let Ok(text) = text {
                    println!("{text}");
                }
            }
        }
        if !matches!(
            e.downcast_ref::<PodboxError>(),
            Some(PodboxError::ChildExit(_))
        ) {
            ui::error(&format!("{e:#}"));
        }
        exit_code_for_error(&e)
    } else {
        ExitCode::SUCCESS
    }
}

fn exit_code_for_error(err: &anyhow::Error) -> ExitCode {
    if let Some(podbox_err) = err.downcast_ref::<PodboxError>() {
        match podbox_err {
            // 2: the definition file (or the explicit `--config` file) is missing,
            // unreadable or unparseable. A missing `--config` is a hard error here.
            PodboxError::DefinitionNotFound { .. }
            | PodboxError::ConfigFileMissing { .. }
            | PodboxError::DefinitionReadFailed(_)
            | PodboxError::DefinitionParseFailed(_) => ExitCode::from(2),
            PodboxError::ContainerMissing(_) => ExitCode::from(3),
            PodboxError::BuildFailed(_) | PodboxError::PodmanInspectFailed { .. } => {
                ExitCode::from(4)
            }
            PodboxError::PodmanNotFound => ExitCode::from(5),
            // 64 is EX_USAGE: a name that cannot be used. Not 2, which is the
            // definition-file code above.
            PodboxError::InvalidName { .. } => ExitCode::from(64),
            PodboxError::ChildExit(code) => ExitCode::from(u8::try_from(*code).unwrap_or(1)),
            PodboxError::PullFailed { .. } | PodboxError::TagFailed { .. } => ExitCode::from(6),
            _ => ExitCode::FAILURE,
        }
    } else {
        ExitCode::FAILURE
    }
}

fn run(mut cli: Cli) -> Result<()> {
    ui::set_quiet(cli.quiet);
    ui::set_verbose(cli.verbose);
    init_tracing(cli.verbose);

    // `exec` / `run` accept an optional leading container name (podman-style).
    // Must run before config resolution so the promoted name wins over
    // PODBOX_CONTAINER / active context, but never overrides an explicit -C.
    handlers::promote_leading_container_name(&mut cli.command, &mut cli.container);
    dispatch::validate_cli_names(&cli)?;

    // Exit early if podman is not installed — clean error instead of a cryptic
    // spawn failure deep in the stack.
    if dispatch::needs_podman(&cli.command) && which::which("podman").is_err() {
        return Err(PodboxError::PodmanNotFound.into());
    }

    let ctx = Ctx::from_cli(&cli);
    let target = dispatch::target_name(&cli.command, cli.container.as_deref());
    match dispatch::needs_config(&cli.command) {
        dispatch::ConfigNeed::None => dispatch::dispatch_no_config(&ctx, &cli, target),
        dispatch::ConfigNeed::Named(_) | dispatch::ConfigNeed::All => {
            dispatch::dispatch_with_config(&ctx, &cli, target)
        }
    }
}
