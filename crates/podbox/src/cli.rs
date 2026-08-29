use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "podbox")]
#[command(version = env!("PODBOX_VERSION"))]
#[command(about = "Podman-native container environment manager")]
#[command(after_help = "Common workflow:\n  \
        podbox create <profile>   Create and start a prebuilt environment\n  \
        podbox enter              Open a shell in the active container\n  \
        podbox list               Show managed containers\n  \
        podbox doctor             Diagnose host and container issues")]
pub struct Cli {
    /// Path to the definition TOML file.
    #[arg(long, short)]
    pub config: Option<PathBuf>,

    /// Print what would happen without executing.
    #[arg(long, global = true)]
    pub dry_run: bool,

    /// Container name to use for commands (overrides config file detection)
    #[arg(long, short = 'C', global = true)]
    pub container: Option<String>,

    /// Suppress progress output; errors and data are still printed.
    #[arg(long, global = true)]
    pub quiet: bool,

    /// Increase log verbosity (repeatable: -v debug, -vv trace).
    #[arg(long, short = 'v', action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Command,
}

mod command;

pub use command::{Command, ExportCommand, ProfileCommand, SnapshotCommand};

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
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
