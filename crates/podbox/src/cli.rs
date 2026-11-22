use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "podbox")]
#[command(version = env!("CARGO_PKG_VERSION"), long_version = concat!(
    env!("CARGO_PKG_VERSION"),
    " (build ",
    env!("PODBOX_BUILD"),
    ")\nembedded guest: ",
    env!("PODBOX_GUEST_KIND"),
    " (`podbox info` for the full contract report)",
))]
#[command(about = "Podman-native container environment manager")]
#[command(after_help = "Common workflow:\n  \
        podbox create <profile>   Create and start a prebuilt environment\n  \
        podbox enter              Open a shell in the active container\n  \
        podbox list               Show managed containers\n  \
        podbox doctor             Diagnose host and container issues")]
pub struct Cli {
    /// Path to the definition TOML file.
    // Global so it works on either side of the subcommand, which is what the
    // "Hint: Use `--config <PATH>`" on a missing-config error promises.
    #[arg(long, short, global = true, help_heading = "Global options")]
    pub config: Option<PathBuf>,

    /// Print what would happen without executing.
    #[arg(long, global = true, help_heading = "Global options")]
    pub dry_run: bool,

    /// Container name to use for commands (overrides config file detection)
    #[arg(long, short = 'C', global = true, help_heading = "Global options")]
    pub container: Option<String>,

    /// Suppress progress output; errors and data are still printed.
    #[arg(long, global = true, help_heading = "Global options")]
    pub quiet: bool,

    /// Increase log verbosity (repeatable: -v debug, -vv trace).
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help_heading = "Global options"
    )]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

mod command;

pub use command::{
    CacheCommand, Command, ConfigCommand, DotfilesCommand, ExportCommand, ExportTarget,
    ProfileCommand, ProvisionCommand, PublishEngine, SnapshotCommand,
};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    /// Newline-delimited JSON: one compact object per line, for streams
    /// (`watch`, `build`). Internal enum: each command declares the subset it
    /// accepts (`TextJson`, `StreamFormat`, `PublishFormat`) so help and
    /// clap validation stay accurate.
    Ndjson,
}

/// Single-document output: text or one JSON document.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TextJson {
    Text,
    Json,
}

/// Streamed output: text or one compact JSON object per line.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum StreamFormat {
    Text,
    Ndjson,
}

/// `publish` output: text, one JSON document, or an NDJSON event stream.
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum PublishFormat {
    Text,
    Json,
    Ndjson,
}

impl From<TextJson> for OutputFormat {
    fn from(value: TextJson) -> Self {
        match value {
            TextJson::Text => Self::Text,
            TextJson::Json => Self::Json,
        }
    }
}

impl From<StreamFormat> for OutputFormat {
    fn from(value: StreamFormat) -> Self {
        match value {
            StreamFormat::Text => Self::Text,
            StreamFormat::Ndjson => Self::Ndjson,
        }
    }
}

impl From<PublishFormat> for OutputFormat {
    fn from(value: PublishFormat) -> Self {
        match value {
            PublishFormat::Text => Self::Text,
            PublishFormat::Json => Self::Json,
            PublishFormat::Ndjson => Self::Ndjson,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl From<Shell> for clap_complete::shells::Shell {
    fn from(s: Shell) -> Self {
        match s {
            Shell::Bash => clap_complete::shells::Shell::Bash,
            Shell::Zsh => clap_complete::shells::Shell::Zsh,
            Shell::Fish => clap_complete::shells::Shell::Fish,
        }
    }
}
