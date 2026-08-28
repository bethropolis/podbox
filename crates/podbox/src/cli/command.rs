//! The `podbox` command tree: the `Command` subcommand enum and its
//! nested `Export`/`Profile`/`Snapshot` command enums.
//!
//! Extracted verbatim from `cli.rs`; re-exported from `cli` so
//! `podbox::cli::Command` paths keep resolving.

use clap::Subcommand;

use super::{OutputFormat, Shell};

#[derive(Subcommand)]
pub enum Command {
    /// Internal stdin watchdog for interactive sessions. Not for direct use.
    #[command(hide = true)]
    InternalStdinWatchdog {
        /// PID of the process to terminate when stdin hangs up.
        parent_pid: u32,
    },

    /// Build the container image from the definition.
    #[command(display_order = 31)]
    Build {
        /// Container name to build (overrides auto-detection).
        name: Option<String>,
        /// Force rebuild even if definition hasn't changed.
        #[arg(long)]
        rebuild: bool,
        /// Skip post-build drift check.
        #[arg(long)]
        no_diff: bool,
        /// Open config in editor before building.
        #[arg(long)]
        edit: bool,
    },

    /// Install Quadlet systemd files and enable the container.
    #[command(display_order = 32)]
    Enable {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
    },

    /// Disable and remove Quadlet systemd files.
    #[command(display_order = 33)]
    Disable {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Skip config loading and remove Quadlet files by name only.
        #[arg(long)]
        force: bool,
    },

    /// Start the container.
    #[command(display_order = 23)]
    Start {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Maximum seconds to wait for the container to become ready.
        #[arg(long, default_value = "30")]
        timeout: u64,
        /// Open config in editor before starting.
        #[arg(long)]
        edit: bool,
    },

    /// Stop the container.
    #[command(display_order = 24)]
    Stop {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
    },

    /// Execute a command interactively in the container.
    #[command(display_order = 21)]
    Exec {
        /// Run as root inside the container (omit -u flag).
        #[arg(long)]
        root: bool,
        /// Command and arguments to execute.
        #[arg(required = true, trailing_var_arg = true)]
        args: Vec<String>,
    },

    /// Run a GUI application in the container (detached).
    #[command(display_order = 22)]
    Run {
        /// Application to run.
        app: String,
        /// Additional arguments for the application.
        #[arg(trailing_var_arg = true)]
        app_args: Vec<String>,
    },

    /// Show container status.
    #[command(display_order = 26)]
    Status {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Output format (text or json).
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output: OutputFormat,
    },

    /// Show container logs.
    #[command(display_order = 40)]
    Logs {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Follow log output.
        #[arg(short, long)]
        follow: bool,
        /// Number of lines to show from the end (default: 50).
        #[arg(short, long)]
        tail: Option<u32>,
        /// Show logs since this duration (e.g. "5m", "1h", "2024-01-01").
        #[arg(long)]
        since: Option<String>,
    },

    /// Export a .desktop app or binary shim to the host.
    #[command(display_order = 53)]
    Export {
        #[command(subcommand)]
        export_cmd: ExportCommand,
    },

    /// Show resource usage for the container (wraps podman stats).
    #[command(display_order = 42)]
    Stats {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Only show one snapshot, don't stream.
        #[arg(long)]
        no_stream: bool,
        /// Output format (text or json).
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output: OutputFormat,
    },

    /// Remove the container.
    #[command(visible_alias = "rm", display_order = 60)]
    Remove {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Also remove the home directory.
        #[arg(long)]
        all: bool,
        /// Skip confirmation prompt.
        #[arg(long)]
        force: bool,
        /// Remove stale/orphaned containers (no valid config, not running).
        #[arg(long)]
        stale: bool,
        /// Also delete the TOML definition file.
        #[arg(long)]
        config: bool,
    },

    /// Inspect container configuration, generated Quadlet, or computed environment.
    #[command(display_order = 41)]
    Inspect {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Show the resolved TOML config.
        #[arg(long)]
        config: bool,
        /// Show the generated Quadlet (.container file).
        #[arg(long)]
        quadlet: bool,
        /// Show the computed environment variables.
        #[arg(long)]
        env: bool,
        /// Output format (text or json).
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output: OutputFormat,
    },

    /// Run the host socket server (socket-activated by systemd).
    #[command(hide = true)]
    Serve {
        /// Container name to serve.
        name: String,
    },

    /// Run the Wayland firewall proxy (systemd companion service).
    #[command(hide = true)]
    Compositor {
        /// Container name to proxy.
        name: String,
    },

    /// Open an interactive shell in the container.
    #[command(visible_alias = "shell", display_order = 20)]
    Enter {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Open config in editor before entering shell.
        #[arg(long)]
        edit: bool,
    },

    /// Create and start a container from a profile or image in one step.
    #[command(display_order = 10)]
    Create {
        /// Profile name (fedora, cachy) or full image reference.
        image: String,
        /// Override the container name.
        #[arg(long, short)]
        name: Option<String>,
        /// Comma-separated list of packages to install (e.g. "fastfetch,btop").
        #[arg(long, short)]
        packages: Option<String>,
        /// Skip starting the container after setup.
        #[arg(long)]
        no_start: bool,
        /// Open config in editor before creating.
        #[arg(long)]
        edit: bool,
    },

    /// Open the container config in your preferred editor.
    #[command(display_order = 30)]
    Edit {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// After saving, rebuild the image if image config changed.
        #[arg(long)]
        rebuild: bool,
    },

    /// List all managed containers.
    #[command(visible_alias = "ls", display_order = 25)]
    List {
        /// Output format (text or json).
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output: OutputFormat,
    },

    /// Clone an existing container config to a new name.
    #[command(display_order = 50)]
    Clone {
        /// Source container name.
        src: String,
        /// Destination container name.
        dst: String,
        /// Also copy the home directory contents.
        #[arg(long)]
        copy_home: bool,
    },

    /// Initialize a new container config.
    #[command(display_order = 11)]
    Init {
        /// Base image reference (e.g. "fedora:44") for a non-prebuilt container.
        /// If omitted, defaults to "fedora:44".
        image: Option<String>,
        /// Container name (defaults to the image name).
        #[arg(long)]
        name: Option<String>,
        /// Run an interactive wizard to build the config.
        #[arg(long, short = 'i', conflicts_with = "profile")]
        interactive: bool,
        /// Use a named profile (cachy, fedora, dev) as template.
        #[arg(long)]
        profile: Option<String>,
    },

    /// Pull the latest image and restart the container.
    #[command(display_order = 34)]
    Update {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Skip restart after update.
        #[arg(long)]
        no_restart: bool,
    },

    /// Pull a prebuilt image without building.
    #[command(display_order = 35)]
    Pull {
        /// Distro shorthand or full image reference.
        image: Option<String>,
    },

    /// Manage container profiles.
    #[command(display_order = 12)]
    Profile {
        #[command(subcommand)]
        profile_cmd: ProfileCommand,
    },

    /// Run diagnostic checks.
    #[command(display_order = 43)]
    Doctor {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Auto-fix common issues (e.g. corrupted Wayland socket ownership).
        #[arg(long)]
        fix: bool,
        /// Output format (text or json).
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output: OutputFormat,
    },

    /// Generate shell completions.
    #[command(display_order = 80)]
    Completions {
        /// Shell to generate completions for.
        shell: Shell,
        /// Also print daily-driver `abbr` shorthand (fish only).
        #[arg(long, action = clap::ArgAction::SetTrue)]
        abbrs: bool,
    },

    /// Compare declared packages against the running container.
    #[command(display_order = 36)]
    Diff {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Update the config TOML's install list to match the container.
        #[arg(long)]
        apply: bool,
        /// Output format (text or json).
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output: OutputFormat,
    },

    /// Snapshot the current container state as a tagged image.
    #[command(display_order = 51)]
    Snapshot {
        #[command(subcommand)]
        snapshot_cmd: SnapshotCommand,
    },

    /// Restore a container from a snapshot.
    #[command(display_order = 52)]
    Restore {
        /// Tag of the snapshot to restore.
        tag: String,
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
    },

    /// Set or show active context.
    #[command(display_order = 70)]
    Use {
        /// Container name to set as active (omit to show current context).
        name: Option<String>,
        /// Clear the active context.
        #[arg(long)]
        clear: bool,
    },

    /// Find the definition file that would be used.
    #[command(display_order = 44)]
    FindDefinition {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
    },

    /// Guided repair for a container that won't start.
    #[command(display_order = 45)]
    Recover {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Run every step without prompting.
        #[arg(long)]
        yes: bool,
    },

    /// Show the recent lifecycle action history.
    #[command(display_order = 46)]
    History {
        /// Container name filter (leave empty for all containers).
        name: Option<String>,
        /// Maximum number of entries to show (0 = no limit).
        #[arg(long, default_value_t = 25)]
        limit: usize,
        /// Output format (text or json).
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output: OutputFormat,
    },

    /// Translate a path between host and container.
    #[command(display_order = 81, group(
        clap::ArgGroup::new("direction")
            .args(["to_container", "to_host"])
            .required(true)
            .multiple(false)
    ))]
    TranslatePath {
        /// Direction of translation.
        #[arg(long)]
        to_container: bool,
        /// Direction of translation.
        #[arg(long)]
        to_host: bool,
        /// Path to translate.
        path: String,
    },

    /// Print known container names, one per line (shell completion helper).
    #[command(hide = true, name = "__complete-names")]
    CompleteNames,
}

#[derive(Subcommand)]
pub enum ExportCommand {
    /// Export a .desktop application.
    App {
        /// Application name to export (omit with --all).
        name: Option<String>,
        /// Export all apps listed in the config.
        #[arg(long, conflicts_with = "name")]
        all: bool,
    },
    /// Export a binary shim.
    Bin {
        /// Binary name to export (omit with --all).
        name: Option<String>,
        /// Export all bins listed in the config.
        #[arg(long, conflicts_with = "name")]
        all: bool,
    },
    /// Remove all exports for the container.
    Clean,
    /// List apps and bins exported to the host for the container.
    List,
}

#[derive(Subcommand)]
pub enum ProfileCommand {
    /// List all available profiles (built-in and custom).
    List,
    /// Show the configuration of a specific profile.
    Show {
        /// Name of the profile to display.
        name: String,
    },
}

#[derive(Debug, Clone, Subcommand)]
pub enum SnapshotCommand {
    /// Take a snapshot of the current container state.
    Create {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Snapshot tag (defaults to timestamp).
        #[arg(long, short)]
        tag: Option<String>,
    },
    /// List snapshots for a container.
    List {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Output format (text or json).
        #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
        output: OutputFormat,
    },
    /// Prune old snapshots, keeping the newest N.
    Prune {
        /// Container name (overrides auto-detection / active context).
        name: Option<String>,
        /// Number of snapshots to keep (default: 5).
        #[arg(long, default_value_t = 5)]
        keep: usize,
    },
}
