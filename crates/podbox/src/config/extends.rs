//! `extends` inheritance resolution — chain building + cycle detection.
//!
//! Resolves `extends = "<target>"` where target is:
//! - `profile:<name>` → bundled or user-defined profile TOML
//! - `./` / `../` / absolute path → filesystem TOML relative to current file
//! - bare name (`fedora`) → `~/.config/podbox/profiles/<name>.toml` (canonical)
//!   with fallback to `~/.config/podbox/<name>.toml` (legacy)

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::merge::merge_toml_values;

/// Identity of a config source for cycle detection.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ConfigSource {
    Profile(String),
    Path(PathBuf),
}

/// Resolve a chain of `extends` starting at `initial_path`/`initial_toml`.
///
/// Returns a single merged `toml::Value` where every `extends` has been
/// resolved and deep-merged (parent → child order, `extends` key dropped).
pub fn resolve_extends_chain(initial_path: &Path, initial_toml: &str) -> Result<toml::Value> {
    let mut visited: HashSet<ConfigSource> = HashSet::new();
    let mut chain: Vec<toml::Value> = Vec::new();

    let mut current_val: toml::Value =
        toml::from_str(initial_toml).with_context(|| {
            format!(
                "failed to parse TOML at '{}'",
                initial_path.display()
            )
        })?;
    let mut current_dir = initial_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();

    // Canonicalize initial path when possible; fallback to absolute-ish.
    let canon_initial = std::fs::canonicalize(initial_path)
        .unwrap_or_else(|_| initial_path.to_path_buf());
    visited.insert(ConfigSource::Path(canon_initial));
    chain.push(current_val.clone());

    while let Some(extends_val) = current_val.get("extends").and_then(|v| v.as_str()) {
        let trimmed = extends_val.trim();
        if trimmed.is_empty() {
            break;
        }
        let (source, next_raw, next_dir) =
            resolve_extends_target(trimmed, &current_dir).with_context(|| {
                format!(
                    "failed to resolve extends target '{}' from '{}'",
                    trimmed,
                    current_dir.display()
                )
            })?;

        if !visited.insert(source.clone()) {
            anyhow::bail!("Circular dependency detected in 'extends': {:?}", source);
        }

        current_val = toml::from_str(&next_raw).with_context(|| {
            format!("failed to parse TOML for extends target {:?}", source)
        })?;
        current_dir = next_dir;
        chain.push(current_val.clone());
    }

    // Merge from base (last in chain) down to leaf child (first).
    let mut merged = chain.pop().expect("chain has at least initial");
    while let Some(child) = chain.pop() {
        merge_toml_values(&mut merged, child);
    }

    // Ensure extends key is stripped from final merged value
    if let Some(tbl) = merged.as_table_mut() {
        tbl.remove("extends");
    }

    Ok(merged)
}

fn resolve_extends_target(
    target: &str,
    current_dir: &Path,
) -> Result<(ConfigSource, String, PathBuf)> {
    // 1. profile:<name>
    if let Some(name) = target.strip_prefix("profile:") {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("extends 'profile:' requires a profile name");
        }
        let profile = crate::profiles::find(name)
            .ok_or_else(|| anyhow::anyhow!("unknown profile '{}'", name))?;
        let source = ConfigSource::Profile(name.to_string());
        let next_dir = current_dir.to_path_buf();
        return Ok((source, profile.toml, next_dir));
    }

    // 2. filesystem path: ./, ../, /, or contains '/' or ends with .toml
    // Heuristic: if it looks like a path, treat as path.
    let is_path_like = target.starts_with("./")
        || target.starts_with("../")
        || target.starts_with('/')
        || target.ends_with(".toml")
        || target.contains('/');
    if is_path_like {
        let candidate = if Path::new(target).is_absolute() {
            PathBuf::from(target)
        } else {
            current_dir.join(target)
        };
        let content = std::fs::read_to_string(&candidate).with_context(|| {
            format!("failed to read extends path '{}'", candidate.display())
        })?;
        let canon = std::fs::canonicalize(&candidate).unwrap_or(candidate.clone());
        let source = ConfigSource::Path(canon);
        let next_dir = candidate
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| current_dir.to_path_buf());
        return Ok((source, content, next_dir));
    }

    // 3. bare sibling name → profiles/<name>.toml (canonical) or legacy root
    let sibling_path = crate::config::find_config_path(target).ok_or_else(|| {
        anyhow::anyhow!(
            "failed to read sibling extends '{}' — no config found at '{}/{{profiles/,}}/{}.toml'",
            target,
            crate::config::config_dir().display(),
            target
        )
    })?;
    let content = std::fs::read_to_string(&sibling_path).with_context(|| {
        format!(
            "failed to read sibling extends '{}' at '{}'",
            target,
            sibling_path.display()
        )
    })?;
    let canon = std::fs::canonicalize(&sibling_path).unwrap_or(sibling_path.clone());
    let source = ConfigSource::Path(canon);
    let next_dir = sibling_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| current_dir.to_path_buf());
    Ok((source, content, next_dir))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_toml(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn single_extends_merges() {
        let tmp = tempfile::tempdir().unwrap();
        let base = write_toml(
            tmp.path(),
            "base.toml",
            r#"
            [image]
            base = "fedora:41"
            name = "base"

            [container]
            name = "base"
            home = "~/containers/base"

            [image.packages]
            install = ["git"]
            "#,
        );
        let child_path = tmp.path().join("child.toml");
        let child_toml = format!(
            r#"
            extends = "./base.toml"
            [image]
            name = "child"
            [container]
            name = "child"
            home = "~/containers/child"
            [image.packages]
            install = ["rustup"]
            "#
        );
        fs::write(&child_path, &child_toml).unwrap();
        let merged = resolve_extends_chain(&child_path, &child_toml).unwrap();
        // child overrides name, arrays union
        assert_eq!(
            merged.get("image").unwrap().get("name").unwrap().as_str().unwrap(),
            "child"
        );
        let install = merged
            .get("image")
            .unwrap()
            .get("packages")
            .unwrap()
            .get("install")
            .unwrap()
            .as_array()
            .unwrap();
        let strs: Vec<_> = install.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(strs.contains(&"git"));
        assert!(strs.contains(&"rustup"));
        let _ = base;
    }

    #[test]
    fn circular_detected() {
        let tmp = tempfile::tempdir().unwrap();
        let a_path = tmp.path().join("a.toml");
        let b_path = tmp.path().join("b.toml");
        fs::write(&a_path, r#"extends = "./b.toml"
[image]
base = "fedora:41"
name = "a"
[container]
name = "a"
home = "~/a"
"#)
        .unwrap();
        fs::write(&b_path, r#"extends = "./a.toml"
[image]
base = "fedora:41"
name = "b"
[container]
name = "b"
home = "~/b"
"#)
        .unwrap();
        let a_content = fs::read_to_string(&a_path).unwrap();
        let err = resolve_extends_chain(&a_path, &a_content).unwrap_err();
        assert!(err.to_string().contains("Circular"));
    }

    #[test]
    fn profile_extends() {
        let tmp = tempfile::tempdir().unwrap();
        let child_path = tmp.path().join("child.toml");
        let child_toml = r#"
            extends = "profile:dev"
            [container]
            name = "mydev"
            home = "~/containers/mydev"
            "#;
        fs::write(&child_path, child_toml).unwrap();
        let merged = resolve_extends_chain(&child_path, child_toml).unwrap();
        // dev profile has image.base; merged should have it if not overridden
        assert!(merged.get("image").is_some());
        assert_eq!(
            merged
                .get("container")
                .unwrap()
                .get("name")
                .unwrap()
                .as_str()
                .unwrap(),
            "mydev"
        );
    }
}
