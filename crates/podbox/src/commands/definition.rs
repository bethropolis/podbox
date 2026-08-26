use anyhow::Result;
use owo_colors::{OwoColorize, Stream};

use podbox::cli::OutputFormat;
use podbox::config;
use podbox::error::PodboxError;

/// Print the definition file that would be used for the given container.
///
/// Scriptable contract: the resolved path on stdout, nothing else. Missing
/// named configs exit non-zero (code 2). With no name and no local
/// definition, podbox falls back to its embedded default — there is no path
/// to print, so stdout stays empty and the command succeeds.
pub fn run_find_definition(name: Option<&str>) -> Result<()> {
    match name {
        Some(n) => {
            let path = config::config_dir().join(format!("{n}.toml"));
            if path.exists() {
                println!("{}", path.display());
                Ok(())
            } else {
                Err(PodboxError::DefinitionNotFound { path }.into())
            }
        }
        None => match config::find_definition() {
            Some(path) => {
                println!("{}", path.display());
                Ok(())
            }
            // Resolved via embedded default; no on-disk path to report.
            None => Ok(()),
        },
    }
}

/// Generate shell completions.
pub fn run_completions(shell: clap_complete::shells::Shell, abbrs: bool) -> Result<()> {
    let mut cmd = <podbox::cli::Cli as clap::CommandFactory>::command();
    let name = cmd.get_name().to_string();
    clap_complete::generate(shell, &mut cmd, name, &mut std::io::stdout());
    print_name_completion_glue(shell);
    print_fish_abbrevs(shell, abbrs);
    Ok(())
}

/// Fish `abbr` definitions emitted by `podbox completions fish --abbrs`.
///
/// Each entry is a short token expanding to a full command line as it is
/// typed. Tokens follow the `pb` + verb-letter convention (PLAN E4) and are
/// prefix-unique, so no two abbreviations collide. Because fish expands whole
/// tokens, a single letter can stand for otherwise-ambiguous verbs:
/// `pbs` (start) vs `pbv` (status) vs `pbt` (stop).
const FISH_ABBREVS: &[(&str, &str)] = &[
    ("pb", "podbox"),
    ("pbb", "podbox build"),
    ("pbc", "podbox create"),
    ("pbd", "podbox doctor"),
    ("pbe", "podbox enter"),
    ("pbl", "podbox list"),
    ("pbr", "podbox recover"),
    ("pbs", "podbox start"),
    ("pbt", "podbox stop"),
    ("pbu", "podbox update"),
    ("pbv", "podbox status"),
    ("pbx", "podbox exec --"),
];

/// Append opt-in fish abbreviations after the static + dynamic glue. Only
/// fish honors `--abbrs`; other shells (or fish without the flag) print
/// nothing extra, so a piped default completion stream stays unchanged.
fn print_fish_abbrevs(shell: clap_complete::shells::Shell, abbrs: bool) {
    if !abbrs {
        return;
    }
    if !matches!(shell, clap_complete::shells::Shell::Fish) {
        return;
    }
    println!("# --- podbox daily-driver abbreviations (opt-in) ---");
    println!("# Source this output manually, e.g. `source (podbox completions fish --abbrs | psub)`.");
    for (token, expanded) in FISH_ABBREVS {
        println!("abbr {token} '{expanded}'");
    }
}

/// Print known container names (config stems), one per line.
///
/// Feeds dynamic container-name completion from the static scripts. Never
/// fails and prints nothing when no configs exist — completion must not
/// error just because the config dir is missing.
pub fn run_complete_names() -> Result<()> {
    for p in config::list_configs() {
        if let Some(stem) = p.file_stem() {
            println!("{}", stem.to_string_lossy());
        }
    }
    Ok(())
}

/// Shell snippets appended after the static script so NAME / `-C` arguments
/// complete dynamically via `podbox __complete-names`.
fn print_name_completion_glue(shell: clap_complete::shells::Shell) {
    let glue = match shell {
        clap_complete::shells::Shell::Bash => Some(r#"
# --- podbox dynamic container-name completion ---
__podbox_names() { command podbox __complete-names 2>/dev/null; }
__podbox_add_names() {
    local cur_="${COMP_WORDS[COMP_CWORD]}" n_
    for n_ in $(__podbox_names); do
        case "$n_" in "$cur_"*) COMPREPLY+=("$n_");; esac
    done
}
__podbox_wrap() {
    COMPREPLY=()
    _podbox "$@"
    local prev_="${COMP_WORDS[COMP_CWORD-1]}"
    if [ "${#COMPREPLY[@]}" -eq 0 ] && [ "$COMP_CWORD" -ge 2 ]; then
        case "$prev_" in
            -C|--container) __podbox_add_names; return ;;
        esac
        case "${COMP_WORDS[1]}" in
            build|enable|disable|start|stop|enter|exec|run|status|logs|inspect|stats|diff|remove|rm|edit|update|find-definition|clone|snapshot|restore)
                [ "$COMP_CWORD" -eq 2 ] && __podbox_add_names ;;
        esac
    fi
}
complete -o default -F __podbox_wrap podbox 2>/dev/null || true
"#),
        // zsh: no automatic glue yet — wrapping the generated `_podbox`
        // reliably requires either a full custom widget or upstream support.
        // The helper below still works for manual hooks; see docs/cli.md.
        clap_complete::shells::Shell::Zsh => Some(r#"
# --- podbox container-name helper ---
# Dynamic positional/flag completion is provided for bash and fish.
# For zsh you can wire names manually, e.g.:
#   __podbox_names() { command podbox __complete-names 2>/dev/null; }
"#),
        clap_complete::shells::Shell::Fish => Some(r#"
# --- podbox dynamic container-name completion ---
function __podbox_names
    command podbox __complete-names 2>/dev/null
end
complete -c podbox -l container -o C -xa '(__podbox_names)'
complete -c podbox -n '__fish_seen_subcommand_from enter shell exec run start stop status logs inspect stats diff remove rm edit build enable disable update find-definition clone snapshot restore' -xa '(__podbox_names)'
"#),
        _ => None,
    };
    if let Some(g) = glue {
        println!("{g}");
    }
}

/// List all podbox-managed containers with status, autostart, and active context.
pub fn run_list(output: OutputFormat) -> Result<()> {
    let configs = config::list_configs();
    let active_ctx = config::read_active_context();

    if matches!(output, OutputFormat::Json) {
        let entries: Vec<serde_json::Value> = configs
            .iter()
            .map(|cp| {
                let name = cp
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let state_label = match podbox::podman::query_state(&name) {
                    Ok(podbox::podman::ContainerState::Running) => "running",
                    Ok(podbox::podman::ContainerState::Stopped)
                        if podbox::systemd::is_unit_failed(&name) =>
                    {
                        "failed"
                    }
                    Ok(podbox::podman::ContainerState::Stopped) => "stopped",
                    Ok(podbox::podman::ContainerState::Missing) => "unbuilt",
                    Err(_) => "unknown",
                };
                let autostart = config::Config::load(cp).is_ok_and(|c| c.lifecycle.autostart);
                serde_json::json!({
                    "name": name,
                    "status": state_label,
                    "autostart": autostart,
                    "active": active_ctx.as_deref() == Some(&name),
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({"containers": entries}))?
        );
        return Ok(());
    }

    if configs.is_empty() {
        println!("No containers found. Create your first container with `podbox init -i`.");
        return Ok(());
    }

    println!(
        "{:<20} {:<24} {:<10} {}",
        "CONTAINER".if_supports_color(Stream::Stdout, |s| s.bold()),
        "STATUS".if_supports_color(Stream::Stdout, |s| s.bold()),
        "AUTOSTART".if_supports_color(Stream::Stdout, |s| s.bold()),
        "ACTIVE CONTEXT".if_supports_color(Stream::Stdout, |s| s.bold()),
    );
    println!("{}", "─".repeat(75));

    for config_path in configs {
        let name = config_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let status = format_status(&name);
        let autostart = format_autostart(&config_path);
        let active = format_active(&name, &active_ctx);

        println!("{name:<20} {status:<24} {autostart:<10} {active}");
    }

    Ok(())
}

/// Format the container status with owo-colors styling.
fn format_status(name: &str) -> String {
    let (dot, label) = match podbox::podman::query_state(name) {
        Ok(podbox::podman::ContainerState::Running) => (
            "●"
                .if_supports_color(Stream::Stdout, |s| s.green())
                .to_string(),
            "running",
        ),
        Ok(podbox::podman::ContainerState::Stopped) => {
            if podbox::systemd::is_unit_failed(name) {
                (
                    "⚠"
                        .if_supports_color(Stream::Stdout, |s| s.red())
                        .to_string(),
                    "failed",
                )
            } else {
                (
                    "○"
                        .if_supports_color(Stream::Stdout, |s| s.bright_black())
                        .to_string(),
                    "stopped",
                )
            }
        }
        Ok(podbox::podman::ContainerState::Missing) => (
            "○"
                .if_supports_color(Stream::Stdout, |s| s.yellow())
                .to_string(),
            "unbuilt",
        ),
        Err(_) => (
            "?".if_supports_color(Stream::Stdout, |s| s.red())
                .to_string(),
            "unknown",
        ),
    };
    format!("{dot} {label}")
}

/// Format the autostart column, reading from the TOML config.
fn format_autostart(config_path: &std::path::Path) -> String {
    let autostart = match config::Config::load(config_path) {
        Ok(cfg) => cfg.lifecycle.autostart,
        Err(_) => {
            return "err"
                .if_supports_color(Stream::Stdout, |s| s.red())
                .to_string();
        }
    };
    if autostart {
        "yes"
            .if_supports_color(Stream::Stdout, |s| s.green())
            .to_string()
    } else {
        "no".to_string()
    }
}

/// Format the active-context marker.
fn format_active(name: &str, active_ctx: &Option<String>) -> String {
    if active_ctx.as_deref() == Some(name) {
        "★ active"
            .if_supports_color(Stream::Stdout, |s| s.yellow())
            .to_string()
    } else {
        String::new()
    }
}
