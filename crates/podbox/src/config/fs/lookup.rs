use std::path::PathBuf;

use super::super::Config;
use super::discovery::{find_config_path, list_configs};

/// Config whose `container.name` is `runtime`, the name podman and systemd use.
///
/// A config that fails to load still owns its file-stem name, so it is matched
/// by stem. Otherwise an installed box with a broken config would look stale.
pub fn find_by_runtime_name(runtime: &str) -> Option<PathBuf> {
    find_by_runtime_in(&list_configs(), runtime)
}

/// Resolve a user-supplied config name: accept a file stem (the `-C` lookup
/// users know) or a runtime `container.name` when the two differ.
pub fn find_by_runtime_or_stem(name: &str) -> Option<PathBuf> {
    find_config_path(name).or_else(|| find_by_runtime_name(name))
}

/// [`find_by_runtime_name`] over an explicit list of config paths.
pub(crate) fn find_by_runtime_in(paths: &[PathBuf], runtime: &str) -> Option<PathBuf> {
    paths
        .iter()
        .find(|path| match Config::load_for_lookup(path) {
            Ok(cfg) => cfg.container.name == runtime,
            Err(_) => path.file_stem().is_some_and(|stem| stem == runtime),
        })
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "cli")]
    #[test]
    fn runtime_lookup_does_not_emit_other_profiles_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("host-dotfiles-does-not-exist");
        let path = dir.path().join("noisy.toml");
        std::fs::write(
            &path,
            format!(
                r#"
[image]
base = "fedora:44"
name = "noisy"

[container]
name = "noisy"
home = "{}"

[dotfiles]
source = "host:{}"
target = "~/.dotfiles"
"#,
                dir.path().join("home").display(),
                missing.display()
            ),
        )
        .unwrap();
        let warning = format!(
            "dotfiles.source: host directory '{}' does not exist — nothing will be copied into the home",
            missing.display()
        );

        assert!(!crate::config::validation::note_was_seen(&warning));
        assert_eq!(
            find_by_runtime_in(std::slice::from_ref(&path), "noisy"),
            Some(path)
        );
        assert!(
            !crate::config::validation::note_was_seen(&warning),
            "runtime lookup leaked a warning from an unrelated definition"
        );
    }

    /// `work.toml` whose `container.name` is `dev`: the runtime name `dev`
    /// must resolve to it, and the file name `work` must not.
    #[test]
    fn runtime_lookup_uses_container_name_not_file_name() {
        let dir = std::env::temp_dir().join(format!("podbox-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixture = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/full.toml"),
        )
        .unwrap();
        let work = dir.join("work.toml");
        std::fs::write(
            &work,
            fixture.replace("name  = \"myenv\"", "name  = \"dev\""),
        )
        .unwrap();
        let paths = vec![work.clone()];

        assert_eq!(find_by_runtime_in(&paths, "dev"), Some(work.clone()));
        assert_eq!(find_by_runtime_in(&paths, "work"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A config that no longer parses still owns its file-stem name, so an
    /// installed box with that name is not reported stale.
    #[test]
    fn broken_config_still_claims_its_stem() {
        let dir = std::env::temp_dir().join(format!("podbox-rt-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let broken = dir.join("box.toml");
        std::fs::write(&broken, "not = [valid").unwrap();

        assert_eq!(
            find_by_runtime_in(std::slice::from_ref(&broken), "box"),
            Some(broken)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
