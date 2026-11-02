//! One-shot guest provisioning steps (`[[provision]]`).
//!
//! Steps run inside the container once per home volume, tracked by stamp
//! files carrying a content hash. A changed step is reported as stale, never
//! silently re-run. This generalizes `[dotfiles] install` (which stays
//! unchanged) to an ordered list of declarative setup commands.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use podbox::config::{Config, ProvisionStep};
use podbox::env::HostEnv;
use serde::Serialize;

const STAMP_VERSION: &str = "v1";
const HASH_DOMAIN: &str = "podbox-provision-v1";

/// Provisioning state of a single step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvisionState {
    Pending,
    Applied,
    Stale,
    Orphaned,
    Unknown,
}

impl ProvisionState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Applied => "applied",
            Self::Stale => "stale",
            Self::Orphaned => "orphaned",
            Self::Unknown => "unknown",
        }
    }
}

/// One row of `provision status`.
#[derive(Debug, Clone, Serialize)]
pub struct StatusRow {
    pub name: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applied_at: Option<u64>,
}

pub fn provision_dir(config: &Config) -> PathBuf {
    config.container.home.join(".local/state/podbox/provision")
}

fn stamp_path(config: &Config, name: &str) -> PathBuf {
    provision_dir(config).join(format!("{name}.stamp"))
}

/// Hash covering what changes the provisioned result.
///
/// `timeout` and `on_failure` are excluded (they change how, not what).
/// `run` is hashed raw; env is sorted by key for determinism. `verify` is
/// part of the result: a tightened postcondition must re-run the step.
pub fn step_hash(step: &ProvisionStep) -> String {
    let mut input = String::new();
    input.push_str(HASH_DOMAIN);
    input.push('\n');
    input.push_str(&step.name);
    input.push('\n');
    input.push_str(&step.run);
    input.push('\n');
    input.push_str(step.verify.as_deref().unwrap_or(""));
    input.push('\n');
    input.push_str(if step.root { "1" } else { "0" });
    input.push('\n');
    let sorted: BTreeMap<&String, &String> = step.env.iter().collect();
    for (k, v) in sorted {
        input.push_str(k);
        input.push('=');
        input.push_str(v);
        input.push('\n');
    }
    podbox::build::checksum(&input)
}

fn parse_stamp(content: &str) -> Option<(String, u64)> {
    let mut parts = content.split_whitespace();
    let version = parts.next()?;
    let hash = parts.next()?;
    let ts = parts.next()?;
    if version != STAMP_VERSION || hash.is_empty() {
        return None;
    }
    let applied_at = ts.parse::<u64>().ok()?;
    Some((hash.to_string(), applied_at))
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Compute the state of every declared step plus orphaned stamps.
///
/// Reads stamps read-only from the host so status works with the container
/// stopped. The home directory is a host path, so no `podman exec` fallback
/// is needed: a missing home simply means every step is pending.
pub fn classify(config: &Config) -> Vec<(String, ProvisionState, Option<u64>)> {
    let dir = provision_dir(config);
    let mut rows: Vec<(String, ProvisionState, Option<u64>)> = Vec::new();

    // If the home/provision dir is missing, everything is pending.
    let entries: Option<Vec<(String, String)>> = std::fs::read_dir(&dir).ok().map(|rd| {
        rd.filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                name.strip_suffix(".stamp")
                    .map(|s| (s.to_string(), e.path()))
            })
            .filter_map(|(step_name, path)| {
                std::fs::read_to_string(&path)
                    .ok()
                    .map(|content| (step_name, content))
            })
            .collect()
    });
    // IO error on an existing home (permissions, etc.) -> unknown.
    let stamps: BTreeMap<String, (String, u64)> = match entries {
        Some(list) => list
            .into_iter()
            .filter_map(|(name, content)| parse_stamp(&content).map(|(h, t)| (name, (h, t))))
            .collect(),
        None => {
            if dir.exists() {
                // Exists but unreadable: unknown for every declared step.
                return config
                    .provision
                    .iter()
                    .map(|s| (s.name.clone(), ProvisionState::Unknown, None))
                    .collect();
            }
            BTreeMap::new()
        }
    };
    // Malformed stamp files were dropped above; treat them as pending
    // (they need a run). Detect by re-reading names: any `.stamp` file
    // whose content failed to parse counts as pending, not applied.
    let malformed: HashSet<String> = std::fs::read_dir(&dir)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter_map(|e| {
                    let fname = e.file_name().to_string_lossy().into_owned();
                    fname.strip_suffix(".stamp").map(|s| s.to_string())
                })
                .filter(|n| !stamps.contains_key(n))
                .collect()
        })
        .unwrap_or_default();
    let _ = malformed;

    for step in &config.provision {
        let current = step_hash(step);
        match stamps.get(&step.name) {
            None => rows.push((step.name.clone(), ProvisionState::Pending, None)),
            Some((hash, at)) if *hash == current => {
                rows.push((step.name.clone(), ProvisionState::Applied, Some(*at)));
            }
            Some((_, at)) => rows.push((step.name.clone(), ProvisionState::Stale, Some(*at))),
        }
    }
    // Orphaned stamps: on disk but no declared step.
    let declared: HashSet<&str> = config.provision.iter().map(|s| s.name.as_str()).collect();
    let mut orphans: Vec<&String> = stamps
        .keys()
        .filter(|n| !declared.contains(n.as_str()))
        .collect();
    orphans.sort();
    for name in orphans {
        let at = stamps.get(name).map(|(_, t)| *t);
        rows.push((name.clone(), ProvisionState::Orphaned, at));
    }
    rows
}

/// Compute which declared steps to run.
///
/// - `--step X` restricts to named steps (errors on unknown names).
/// - Otherwise pending + stale run; applied steps are skipped unless
///   `--force` (which re-runs everything, including applied).
/// - Orphaned stamps never run.
pub fn plan(
    config: &Config,
    states: &[(String, ProvisionState, Option<u64>)],
    only: &[String],
    force: bool,
) -> Result<Vec<ProvisionStep>> {
    if !only.is_empty() {
        let declared: HashSet<&str> = config.provision.iter().map(|s| s.name.as_str()).collect();
        for name in only {
            if !declared.contains(name.as_str()) {
                anyhow::bail!("unknown provision step '{name}'");
            }
        }
        let wanted: HashSet<&str> = only.iter().map(|s| s.as_str()).collect();
        return Ok(config
            .provision
            .iter()
            .filter(|s| wanted.contains(s.name.as_str()))
            .cloned()
            .collect());
    }
    let state_by_name: BTreeMap<&str, ProvisionState> =
        states.iter().map(|(n, s, _)| (n.as_str(), *s)).collect();
    Ok(config
        .provision
        .iter()
        .filter(|s| match state_by_name.get(s.name.as_str()) {
            Some(ProvisionState::Applied) => force,
            // Pending, stale, unknown all need a run. Unknown means we
            // couldn't read stamps, so attempt the run rather than skip.
            _ => true,
        })
        .cloned()
        .collect())
}

fn distro(config: &Config) -> String {
    let image = config
        .image
        .image_ref
        .as_deref()
        .unwrap_or(&config.image.base)
        .to_ascii_lowercase();
    ["fedora", "debian", "ubuntu", "arch", "alpine", "cachy"]
        .into_iter()
        .find(|name| image.contains(name))
        .unwrap_or("unknown")
        .to_string()
}

/// Build the `podman exec` argv for one script inside the container: same
/// user, workdir, environment and container as the step's `run`.
fn step_exec_args(
    config: &Config,
    env: &HostEnv,
    step: &ProvisionStep,
    script: &str,
) -> Vec<std::ffi::OsString> {
    let guest_home = format!("/home/{}", env.username);

    let mut args: Vec<std::ffi::OsString> = vec!["exec".into()];
    args.push("--user".into());
    args.push(if step.root {
        "root".into()
    } else {
        env.uid.to_string().into()
    });
    args.push("--workdir".into());
    args.push(guest_home.clone().into());

    let mut variables: Vec<(String, String)> = vec![
        ("PODBOX".into(), "1".into()),
        ("PODBOX_CONTAINER".into(), config.container.name.clone()),
        ("PODBOX_DISTRO".into(), distro(config)),
        ("PODBOX_HOME".into(), guest_home.clone()),
        ("PODBOX_STEP".into(), step.name.clone()),
    ];
    if podbox::profiles::find(&config.image.name).is_some() {
        variables.push(("PODBOX_PROFILE".into(), config.image.name.clone()));
    }
    if config.dotfiles.is_some() {
        // Same guest target derivation as dotfiles (target relative to home).
        if let Some(dotfiles) = &config.dotfiles {
            let relative = if dotfiles.target == "~" {
                String::new()
            } else {
                dotfiles
                    .target
                    .strip_prefix("~/")
                    .unwrap_or(&dotfiles.target)
                    .to_string()
            };
            let guest_target = std::path::Path::new(&guest_home)
                .join(&relative)
                .to_string_lossy()
                .into_owned();
            variables.push(("PODBOX_DOTFILES_DIR".into(), guest_target));
        }
    }
    for (k, v) in &variables {
        args.push("--env".into());
        args.push(format!("{k}={v}").into());
    }
    for (k, v) in &step.env {
        args.push("--env".into());
        args.push(format!("{k}={v}").into());
    }
    args.push(config.container.name.clone().into());
    args.push("sh".into());
    args.push("-c".into());
    args.push(script.into());
    args
}

/// The step's timeout, with the documented 5-minute default when the value
/// does not parse.
fn step_timeout(step: &ProvisionStep) -> std::time::Duration {
    use podbox::config::validation::parse_provision_timeout_secs;
    let secs = parse_provision_timeout_secs(&step.timeout);
    std::time::Duration::from_secs(if secs == 0 { 300 } else { secs })
}

/// Execute one already-built argv under `timeout`, streaming output live.
///
/// stdin is `/dev/null`, no TTY; stdout/stderr are inherited so output
/// streams as it happens. Returns Err on a non-zero exit or expiry.
fn exec_script(args: &[std::ffi::OsString], timeout: std::time::Duration) -> Result<()> {
    let mut cmd = std::process::Command::new("podman");
    cmd.args(args);
    cmd.stdin(Stdio::null());
    // Timeout kills the podman client; the in-container child may linger.
    let status = podbox::process::spawn_interactive_timeout_with_stdio(&mut cmd, timeout)
        .with_context(|| "failed to run provision command in the container")?;
    if !status.success() {
        anyhow::bail!("command failed with {status}");
    }
    Ok(())
}

/// Run one step via `podman exec`, streaming output live, then its
/// postcondition when the step declares one.
///
/// `verify` runs with the same user, workdir, environment and timeout as
/// `run`, right after it exits 0. It catches tools that report partial
/// failure with a success exit code — `android sdk install` prints
/// "package not found" and still exits 0 — by asking the system what state
/// it is actually in. Ok(()) only when both hold.
fn run_step(config: &Config, env: &HostEnv, step: &ProvisionStep, dry_run: bool) -> Result<()> {
    let timeout = step_timeout(step);
    let run_args = step_exec_args(config, env, step, &step.run);
    if dry_run {
        print_exec_line(&run_args);
        if let Some(verify) = &step.verify {
            println!("  then verify:");
            print_exec_line(&step_exec_args(config, env, step, verify));
        }
        return Ok(());
    }

    println!("==> provision: {}", step.name);
    exec_script(&run_args, timeout)
        .map_err(|e| anyhow::anyhow!("provision step '{}' failed with {e:#}", step.name))?;
    if let Some(verify) = &step.verify {
        let verify_args = step_exec_args(config, env, step, verify);
        exec_script(&verify_args, timeout).map_err(|e| {
            anyhow::anyhow!(
                "verify for step '{}' failed with {e:#} — \
                 `run` exited 0 but its postcondition did not hold",
                step.name
            )
        })?;
    }
    Ok(())
}

/// One `podman ...` line, for dry-run plans.
fn print_exec_line(args: &[std::ffi::OsString]) {
    println!(
        "podman {}",
        args.iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );
}

fn write_stamp(config: &Config, step: &ProvisionStep, dry_run: bool) -> Result<()> {
    if dry_run {
        println!("touch provision stamp for '{}'", step.name);
        return Ok(());
    }
    let dir = provision_dir(config);
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("failed to create provision dir '{}'", dir.display()))?;
    let content = format!("{STAMP_VERSION} {} {}\n", step_hash(step), now_secs());
    podbox::fsutil::write_atomic(&stamp_path(config, &step.name), content.as_bytes())
        .with_context(|| format!("failed to write stamp for step '{}'", step.name))?;
    Ok(())
}

/// Run the given steps in order, writing stamps on success.
///
/// `strict` controls failure handling: `sync` is strict (any failure,
/// including `warn`, exits 1); `create` is lenient (warn continues with a
/// warning, abort stops with an error).
fn execute(
    config: &Config,
    env: &HostEnv,
    steps: &[ProvisionStep],
    dry_run: bool,
    strict: bool,
) -> Result<()> {
    let mut failed: Vec<String> = Vec::new();
    for step in steps {
        match run_step(config, env, step, dry_run) {
            Ok(()) => {
                write_stamp(config, step, dry_run)?;
                let _ = podbox::history::record(
                    &config.container.name,
                    "provision",
                    &format!("{}:ok", step.name),
                );
            }
            Err(err) => {
                let _ = podbox::history::record(
                    &config.container.name,
                    "provision",
                    &format!("{}:failed", step.name),
                );
                let is_abort = matches!(
                    step.on_failure,
                    podbox::config::types::ProvisionOnFailure::Abort
                );
                if is_abort {
                    return Err(anyhow::anyhow!(
                        "provision step '{}' failed (on_failure = abort): {err:#}",
                        step.name
                    ));
                }
                if strict {
                    failed.push(step.name.clone());
                    eprintln!("Warning: provision step '{}' failed: {err:#}", step.name);
                } else {
                    eprintln!("Warning: provision step '{}' failed: {err:#}", step.name);
                    eprintln!("The container stays usable; fix and run `podbox provision sync`.");
                }
            }
        }
    }
    if strict && !failed.is_empty() {
        anyhow::bail!("provision failed for step(s): {}", failed.join(", "));
    }
    Ok(())
}

/// Names of stamps whose step no longer exists in the config.
fn orphan_names(states: &[(String, ProvisionState, Option<u64>)]) -> Vec<String> {
    states
        .iter()
        .filter(|(_, s, _)| *s == ProvisionState::Orphaned)
        .map(|(n, _, _)| n.clone())
        .collect()
}

/// Remove stamps that no declared step can ever reference. An orphan only
/// exists because its step was renamed or deleted, so pruning it is lossless:
/// re-adding the step makes it pending again and it simply runs. Returns the
/// number of stamps removed.
fn prune_orphans(config: &Config, states: &[(String, ProvisionState, Option<u64>)]) -> usize {
    let mut removed = 0usize;
    for name in orphan_names(states) {
        if std::fs::remove_file(stamp_path(config, &name)).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// `provision sync`: ensure running, then run pending/stale (or forced) steps.
pub fn sync(
    config: &Config,
    env: &HostEnv,
    only: &[String],
    force: bool,
    dry_run: bool,
) -> Result<()> {
    if config.provision.is_empty() {
        anyhow::bail!(
            "no [[provision]] steps configured for '{}'",
            config.container.name
        );
    }
    if dry_run {
        let states = classify(config);
        let orphans = orphan_names(&states);
        if !orphans.is_empty() {
            println!(
                "Would prune {} orphaned stamp(s): {}",
                orphans.len(),
                orphans.join(", ")
            );
        }
        let steps = plan(config, &states, only, force)?;
        if steps.is_empty() {
            if orphans.is_empty() {
                println!("Nothing to do: all steps applied.");
            }
            return Ok(());
        }
        println!("Would run {} step(s):", steps.len());
        for step in &steps {
            // Print the exec line via run_step dry-run for fidelity.
            run_step(config, env, step, true)?;
        }
        return Ok(());
    }
    crate::commands::ensure_running(
        &config.container.name,
        false,
        crate::commands::DEFAULT_START_TIMEOUT_SECS,
    )?;
    let states = classify(config);
    let pruned = prune_orphans(config, &states);
    if pruned > 0 {
        println!("Pruned {pruned} orphaned provision stamp(s).");
    }
    let steps = plan(config, &states, only, force)?;
    if steps.is_empty() {
        if pruned == 0 {
            println!("Nothing to do: all steps applied.");
        }
        return Ok(());
    }
    execute(config, env, &steps, false, true)
}

/// Steps for `create`: dotfiles first (handled by caller), then these.
/// Lenient on `warn` (warning + continue), strict on `abort`.
pub fn run_for_create(config: &Config, env: &HostEnv, dry_run: bool) -> Result<()> {
    if config.provision.is_empty() {
        return Ok(());
    }
    let states = classify(config);
    let steps = plan(config, &states, &[], false)?;
    if steps.is_empty() {
        return Ok(());
    }
    if dry_run {
        for step in &steps {
            run_step(config, env, step, true)?;
        }
        return Ok(());
    }
    execute(config, env, &steps, false, false)
}

/// `provision status`: one row per step plus orphans.
pub fn status(config: &Config, output: podbox::cli::OutputFormat, check: bool) -> Result<()> {
    if config.provision.is_empty() {
        // Still report orphans when the config dropped all steps.
        let rows = classify(config);
        if rows.is_empty() {
            anyhow::bail!(
                "no [[provision]] steps configured for '{}'",
                config.container.name
            );
        }
    }
    let rows = classify(config);
    match output {
        podbox::cli::OutputFormat::Json => {
            let out: Vec<StatusRow> = rows
                .iter()
                .map(|(name, state, at)| StatusRow {
                    name: name.clone(),
                    state: state.as_str().to_string(),
                    applied_at: *at,
                })
                .collect();
            podbox::contract::print_json(serde_json::json!({ "steps": out }))?;
        }
        podbox::cli::OutputFormat::Ndjson => {
            return Err(podbox::contract::bail_ndjson("provision status"));
        }
        podbox::cli::OutputFormat::Text => {
            println!("Container: {}", config.container.name);
            for (name, state, at) in &rows {
                match at {
                    Some(t) => println!("{name}: {} (applied at {t})", state.as_str()),
                    None => println!("{name}: {}", state.as_str()),
                }
            }
        }
    }
    if check {
        let bad = rows
            .iter()
            .any(|(_, s, _)| matches!(s, ProvisionState::Pending | ProvisionState::Stale));
        if bad {
            return Err(podbox::error::PodboxError::StepsPending.into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(name: &str, run: &str) -> ProvisionStep {
        ProvisionStep {
            name: name.into(),
            run: run.into(),
            verify: None,
            timeout: "5m".into(),
            on_failure: podbox::config::types::ProvisionOnFailure::Warn,
            env: BTreeMap::new(),
            root: false,
        }
    }

    #[test]
    fn hash_stable_and_sensitive() {
        let first = step("a", "echo hi");
        let same = step("a", "echo hi");
        assert_eq!(step_hash(&first), step_hash(&same));
        // `run` changes the hash.
        let mut changed_run = first.clone();
        changed_run.run = "echo bye".into();
        assert_ne!(step_hash(&first), step_hash(&changed_run));
        // timeout excluded
        let mut changed_timeout = first.clone();
        changed_timeout.timeout = "10m".into();
        assert_eq!(step_hash(&first), step_hash(&changed_timeout));
        // env order-independent
        let mut env_forward = first.clone();
        env_forward.env.insert("B".into(), "2".into());
        env_forward.env.insert("A".into(), "1".into());
        let mut env_reverse = first.clone();
        env_reverse.env.insert("A".into(), "1".into());
        env_reverse.env.insert("B".into(), "2".into());
        assert_eq!(step_hash(&env_forward), step_hash(&env_reverse));
        // root matters
        let mut changed_root = first.clone();
        changed_root.root = true;
        assert_ne!(step_hash(&first), step_hash(&changed_root));
        // verify is part of the result: tightening the postcondition must
        // re-run the step, not keep a stamp for the weaker check.
        let mut with_verify = first.clone();
        with_verify.verify = Some("adb --version".into());
        assert_ne!(step_hash(&first), step_hash(&with_verify));
        let mut verify_again = with_verify.clone();
        verify_again.verify = Some("fastboot --version".into());
        assert_ne!(step_hash(&with_verify), step_hash(&verify_again));
    }

    /// An empty `verify` is refused: `sh -c ""` exits 0, so a blank
    /// postcondition would read as a check that always agrees.
    #[test]
    fn empty_verify_is_refused() {
        // Parsing validates eagerly, so the blank postcondition refuses.
        let err = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "x"
[container]
name = "x"
home = "~/x"
[[provision]]
name = "a"
run = "echo a"
verify = "   "
"#,
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("verify"), "{err}");
    }

    /// The postcondition rides the same argv shape as the step: one
    /// `podman exec -c` line, same user, workdir and environment.
    #[test]
    fn verify_rides_the_same_exec_shape() {
        let cfg_toml = r#"
[image]
base = "fedora:44"
name = "x"
[container]
name = "x"
home = "~/x"
[[provision]]
name = "a"
run = "android sdk install platform-tools"
verify = "android sdk list --installed | grep -q platform-tools"
"#;
        let cfg = Config::parse(cfg_toml).unwrap();
        let states = classify(&cfg);
        let steps = plan(&cfg, &states, &[], false).unwrap();
        let verify = steps[0].verify.as_deref().expect("verify parsed");
        let args = step_exec_args(&cfg, &HostEnv::mock_preview(), &steps[0], verify);
        let joined: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(joined.contains(&"exec".to_string()), "{joined:?}");
        assert!(joined.contains(&"-c".to_string()), "{joined:?}");
        assert!(joined.contains(&verify.to_string()), "{joined:?}");
        // Same workdir as the step itself.
        let run_args = step_exec_args(&cfg, &HostEnv::mock_preview(), &steps[0], &steps[0].run);
        assert_eq!(args[1..4], run_args[1..4], "same user and workdir");
    }

    #[test]
    fn stamp_round_trips_and_rejects_bad() {
        assert!(parse_stamp("v1 abc 123").is_some());
        assert!(parse_stamp("v2 abc 123").is_none());
        assert!(parse_stamp("v1 123").is_none());
        assert!(parse_stamp("garbage").is_none());
    }

    #[test]
    fn timeout_parses() {
        use podbox::config::validation::parse_provision_timeout_secs;
        assert_eq!(parse_provision_timeout_secs("30s"), 30);
        assert_eq!(parse_provision_timeout_secs("5m"), 300);
        assert_eq!(parse_provision_timeout_secs("1h"), 3600);
        assert_eq!(parse_provision_timeout_secs("0s"), 0);
        assert_eq!(parse_provision_timeout_secs("off"), 0);
        assert_eq!(parse_provision_timeout_secs(""), 0);
        assert_eq!(parse_provision_timeout_secs("5"), 0);
    }

    /// Orphans are stamps whose step no longer exists in the config: nothing
    /// can reference them, so sync prunes them instead of teaching people to
    /// ignore `provision status`. Live stamps survive untouched.
    #[test]
    fn orphaned_stamps_are_pruned_and_live_ones_survive() {
        let home = tempfile::tempdir().expect("temp home");
        let cfg_toml = r#"
[image]
base = "fedora:44"
name = "x"
[container]
name = "x"
home = "~/x"
[[provision]]
name = "a"
run = "echo a"
"#;
        let mut cfg = Config::parse(cfg_toml).unwrap();
        cfg.container.home = home.path().to_path_buf();
        let dir = provision_dir(&cfg);
        std::fs::create_dir_all(&dir).unwrap();
        // A parseable stamp for a step the config no longer declares.
        std::fs::write(
            dir.join("gone.stamp"),
            format!(
                "{STAMP_VERSION} {} 1\n",
                step_hash(&step("gone", "echo gone"))
            ),
        )
        .unwrap();
        // The live step's stamp must stay.
        std::fs::write(
            dir.join("a.stamp"),
            format!("{STAMP_VERSION} {} 1\n", step_hash(&step("a", "echo a"))),
        )
        .unwrap();

        let states = classify(&cfg);
        assert_eq!(orphan_names(&states), vec!["gone".to_string()]);
        assert_eq!(prune_orphans(&cfg, &states), 1);
        assert!(
            !dir.join("gone.stamp").exists(),
            "orphan stamp must be removed"
        );
        assert!(dir.join("a.stamp").exists(), "live stamp must survive");

        // Nothing left to prune on a second pass.
        let states = classify(&cfg);
        assert!(orphan_names(&states).is_empty());
        assert_eq!(prune_orphans(&cfg, &states), 0);
    }

    #[test]
    fn plan_skips_applied_unless_forced() {
        let cfg_toml = r#"
[image]
base = "fedora:44"
name = "x"
[container]
name = "x"
home = "~/x"
[[provision]]
name = "a"
run = "echo a"
[[provision]]
name = "b"
run = "echo b"
"#;
        let cfg = Config::parse(cfg_toml).unwrap();
        let states = vec![
            ("a".to_string(), ProvisionState::Applied, Some(1)),
            ("b".to_string(), ProvisionState::Pending, None),
        ];
        let p = plan(&cfg, &states, &[], false).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].name, "b");
        let p2 = plan(&cfg, &states, &[], true).unwrap();
        assert_eq!(p2.len(), 2);
        let p3 = plan(&cfg, &states, &["a".to_string()], false).unwrap();
        assert_eq!(p3.len(), 1);
        assert!(plan(&cfg, &states, &["nope".to_string()], false).is_err());
    }
}
