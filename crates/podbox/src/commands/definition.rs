use anyhow::Result;

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
        // zsh: re-register a wrapper after the generated body. The tail of the
        // file executes when zsh sources it, so `compdef` here wins over the
        // `#compdef podbox` header. We decide by context first (deterministic
        // across zsh versions) and delegate everything else to `_podbox`.
        clap_complete::shells::Shell::Zsh => Some(r#"
# --- podbox dynamic container-name completion ---
__podbox_names() { command podbox __complete-names 2>/dev/null }

# Verbs whose first positional argument is a container name.
__podbox_name_verbs="build enable disable start stop enter shell status logs inspect stats diff remove rm edit update find-definition clone snapshot restore recover"

__podbox_wants_names() {
    # After -C/--container anywhere on the line.
    if [[ "${words[CURRENT-1]}" == -C || "${words[CURRENT-1]}" == --container ]]; then
        return 0
    fi
    # First positional slot of a name-taking verb: podbox <verb> <TAB>
    if (( CURRENT == 3 )) && [[ " $__podbox_name_verbs " == *" ${words[2]} "* ]]; then
        return 0
    fi
    return 1
}

__podbox_wrap() {
    if __podbox_wants_names; then
        local -a names
        names=( $(__podbox_names) )
        # Missing configs yield no candidates — never fabricate, never error.
        (( ${#names[@]} )) || return 1
        _describe -t podbox-containers 'container' names
        return 0
    fi
    _podbox "$@"
}
compdef __podbox_wrap podbox 2>/dev/null || true
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
pub use super::list::run_list;

