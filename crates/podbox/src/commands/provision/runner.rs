use std::process::Stdio;

use anyhow::{Context, Result};
use podbox::config::{Config, ProvisionStep};
use podbox::env::HostEnv;

use super::state::{ProvisionState, STAMP_VERSION, now_secs, provision_dir, stamp_path, step_hash};

pub(super) fn distro(config: &Config) -> String {
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
pub(super) fn step_exec_args(
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
pub(super) fn step_timeout(step: &ProvisionStep) -> std::time::Duration {
    use podbox::config::validation::parse_provision_timeout_secs;
    let secs = parse_provision_timeout_secs(&step.timeout);
    std::time::Duration::from_secs(if secs == 0 { 300 } else { secs })
}

/// Execute one already-built argv under `timeout`, streaming output live.
///
/// stdin is `/dev/null`, no TTY; stdout/stderr are inherited so output
/// streams as it happens. Returns Err on a non-zero exit or expiry.
pub(super) fn exec_script(args: &[std::ffi::OsString], timeout: std::time::Duration) -> Result<()> {
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
pub(super) fn run_step(
    config: &Config,
    env: &HostEnv,
    step: &ProvisionStep,
    dry_run: bool,
) -> Result<()> {
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
pub(super) fn print_exec_line(args: &[std::ffi::OsString]) {
    println!(
        "podman {}",
        args.iter()
            .map(|a| a.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ")
    );
}

pub(super) fn write_stamp(config: &Config, step: &ProvisionStep, dry_run: bool) -> Result<()> {
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
pub(super) fn execute(
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
pub(super) fn orphan_names(states: &[(String, ProvisionState, Option<u64>)]) -> Vec<String> {
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
pub(super) fn prune_orphans(
    config: &Config,
    states: &[(String, ProvisionState, Option<u64>)],
) -> usize {
    let mut removed = 0usize;
    for name in orphan_names(states) {
        if std::fs::remove_file(stamp_path(config, &name)).is_ok() {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::super::state::{classify, plan};
    use super::*;
    use std::collections::BTreeMap;

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
}
