use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use podbox::config::{Config, ProvisionStep};

pub(super) const STAMP_VERSION: &str = "v1";
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

pub fn provision_dir(config: &Config) -> PathBuf {
    config.container.home.join(".local/state/podbox/provision")
}

pub(super) fn stamp_path(config: &Config, name: &str) -> PathBuf {
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

pub(super) fn parse_stamp(content: &str) -> Option<(String, u64)> {
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

pub(super) fn now_secs() -> u64 {
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
    // Unparseable stamps are absent from `stamps` and therefore already
    // reported as Pending below — no second directory scan is needed.

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

#[cfg(test)]
mod tests {
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
    fn stamp_round_trips_and_rejects_bad() {
        assert!(parse_stamp("v1 abc 123").is_some());
        assert!(parse_stamp("v2 abc 123").is_none());
        assert!(parse_stamp("v1 123").is_none());
        assert!(parse_stamp("garbage").is_none());
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
