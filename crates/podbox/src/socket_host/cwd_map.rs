//! Container → host path translation for the fd-passing host-exec session.
//!
//! A `host_exec_v2` request carries the guest shell's working directory.
//! Whether the host honours it is config (`integration.host_exec.cwd`):
//! `home` ignores it, `map` translates it through the container's known
//! bind mappings, `refuse` rejects an unmappable path. The mappings are
//! the same mounts the codegen emits — the home volume, resolved
//! `[integration.xdg_dirs]`, `[storage.host_caches]` bindings and
//! `[container.mounts].extra` — kept here in container→host direction
//! because the daemon only ever maps guest requests *out* of the box.
//!
//! Shared cache *volumes* (`[storage.shared_caches]`) are deliberately not
//! in the map: their host paths live inside podman's state and resolving
//! one means talking to podman, which a cwd handler must not do. Those
//! directories fall back like any other unmapped path.

use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::xdg::ResolvedXdgDirs;

/// A container prefix and the host prefix the same directory is bound from.
type Pair = (PathBuf, PathBuf);

/// The container → host mapping one box knows.
pub struct CwdMap {
    /// The host side's own home: the fallback start directory for a guest
    /// cwd that has no mapping, and the expansion root for a `~/` host path.
    host_home: PathBuf,
    /// Mappable prefixes, most-specific wins at lookup time.
    pairs: Vec<Pair>,
}

impl CwdMap {
    /// Build the map a daemon consults, from the config and the host facts
    /// passed in. `xdg` is `None` when the caller did not resolve the XDG
    /// directories (their bindings are then simply unknown and not mapped).
    ///
    /// The container home is `/home/<name>` for the user the daemon runs
    /// as — the same substitution the codegen renders as `/home/%u` —
    /// derived from the passed host home, so an explicit home keeps the
    /// map deterministic (and testable).
    pub fn build(config: &Config, xdg: Option<&ResolvedXdgDirs>, host_home: &Path) -> Self {
        let username = host_home
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("root");
        let container_home = if username == "root" {
            PathBuf::from("/root")
        } else {
            PathBuf::from(format!("/home/{username}"))
        };
        let expand_host = |p: &str, home: &Path| -> PathBuf {
            match p.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                _ if p == "~" => home.to_path_buf(),
                _ => PathBuf::from(p),
            }
        };

        let mut pairs: Vec<Pair> = Vec::new();

        // The isolated home volume: the whole container home maps onto the
        // staging directory the unit binds.
        pairs.push((
            container_home.clone(),
            canonicalized(&config.container.home),
        ));

        // Resolved XDG dirs are bound at {home}/{Name} in codegen.
        if let Some(xdg) = xdg {
            let names = [
                (&xdg.documents, "Documents"),
                (&xdg.downloads, "Downloads"),
                (&xdg.pictures, "Pictures"),
                (&xdg.music, "Music"),
                (&xdg.videos, "Videos"),
                (&xdg.desktop, "Desktop"),
                (&xdg.projects, "Projects"),
            ];
            for (dir, name) in &names {
                if let Some(resolved) = dir {
                    pairs.push((container_home.join(name), canonicalized(&resolved.path)));
                }
            }
        }

        // Host caches bind the same `~/`-relative path on both sides for
        // built-ins; custom ones may sit anywhere on the host.
        for path in config.storage.host_caches.enabled_paths() {
            let rel = path.container_path.trim_start_matches("~/");
            pairs.push((
                container_home.join(rel),
                canonicalized(&host_home.join(rel)),
            ));
        }
        for cache in &config.storage.host_caches.custom {
            if cache.host_path.trim_start_matches("~/").is_empty() {
                continue;
            }
            pairs.push((
                expand_host(&cache.container_path, &container_home),
                canonicalized(&expand_host(&cache.host_path, host_home)),
            ));
        }

        // Extra mounts carry `host:container[:options]` on the config side.
        for mount in &config.container.mounts.extra {
            let parts: Vec<&str> = mount.split(':').collect();
            if parts.len() < 2 || parts[1].is_empty() {
                continue;
            }
            pairs.push((
                PathBuf::from(parts[1]),
                canonicalized(&expand_host(parts[0], host_home)),
            ));
        }

        Self {
            host_home: host_home.to_path_buf(),
            pairs,
        }
    }

    /// The map a daemon actually uses: host home and XDG dirs resolved
    /// from the process's own user environment.
    pub fn for_daemon(config: &Config) -> Self {
        let host_home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
        let xdg = crate::xdg::resolve(&config.integration.xdg_dirs).ok();
        Self::build(config, xdg.as_ref(), &host_home)
    }

    /// Translate a guest working directory to its host prefix. `None`
    /// means no known mapping: relative paths and paths outside every
    /// binding all fall here.
    pub fn translate(&self, guest: &str) -> Option<String> {
        let path = Path::new(guest);
        // Container paths are absolute by definition; a relative request
        // says the guest shell does not know where it is either.
        if !path.is_absolute() {
            return None;
        }
        let path = path.components().collect::<PathBuf>();
        // Longest container prefix wins, so a nested binding (an XDG dir
        // inside the home volume) beats the home itself.
        let mut best: Option<(usize, &PathBuf, &PathBuf)> = None;
        for (container, host) in &self.pairs {
            if path == *container || path.starts_with(container) {
                let depth = container.components().count();
                if best.is_none_or(|(n, _, _)| depth > n) {
                    best = Some((depth, container, host));
                }
            }
        }
        let (_, (container, host)) = match best {
            Some((d, c, h)) => (d, (c, h)),
            None => return None,
        };
        match path.strip_prefix(container) {
            Ok(rel) if !rel.as_os_str().is_empty() => {
                Some(host.join(rel).to_string_lossy().into_owned())
            }
            _ => Some(host.to_string_lossy().into_owned()),
        }
    }

    /// The host home the map was built against — the fallback start
    /// directory when a guest path cannot be mapped.
    pub fn host_home(&self) -> &Path {
        &self.host_home
    }
}

/// Canonicalize a host prefix when it exists, so the same directory seen
/// once through a symlink and once spelled out translates identically;
/// an unusable path stays as configured and is skipped at lookup time by
/// simply not matching a live child cwd anything sensible.
fn canonicalized(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// A parsed config with the given home and extra mounts.
    fn config_with(home: &str, extra: &[&str]) -> Config {
        let mut cfg = Config::parse(&format!(
            "[image]\nbase = \"fedora:41\"\nname = \"env\"\n[container]\nname = \"env\"\nhome = \"{home}\"\n"
        ))
        .unwrap();
        for e in extra {
            cfg.container.mounts.extra.push((*e).to_string());
        }
        cfg
    }

    #[test]
    fn maps_the_home_volume_and_depthwise_nesting() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("containers/env");
        std::fs::create_dir_all(&home).unwrap();
        let cfg = config_with(&home.to_string_lossy(), &[]);
        let map = CwdMap::build(&cfg, None, &dir.path().join("bethost"));

        // The container user name comes from the host home basename: the
        // container home prefix is /home/bethost here.
        let mapped = map.translate("/home/bethost/documents").unwrap();
        assert_eq!(mapped, home.join("documents").to_string_lossy());

        // The home itself maps exactly.
        assert_eq!(
            map.translate("/home/bethost").unwrap(),
            home.to_string_lossy().to_string(),
        );
    }

    #[test]
    fn extra_mounts_map_backwards_from_the_config_order() {
        let dir = tempfile::tempdir().unwrap();
        let host = tempfile::tempdir_in(&dir).unwrap();
        let cfg = config_with(
            "~/containers/env",
            &[&format!("{}/src:/work", host.path().display())],
        );
        let map = CwdMap::build(&cfg, None, dir.path());
        assert_eq!(
            map.translate("/work/main.rs").unwrap(),
            host.path()
                .join("src")
                .join("main.rs")
                .to_string_lossy()
                .to_string(),
        );
        // and the root of the mount
        assert_eq!(
            map.translate("/work").unwrap(),
            host.path().join("src").to_string_lossy().to_string(),
        );
    }

    #[test]
    fn longest_prefix_wins_over_the_home_volume() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("containers/env");
        std::fs::create_dir_all(home.join("Documents")).unwrap();
        let cfg = config_with(&home.to_string_lossy(), &[]);
        let map = CwdMap::build(&cfg, None, dir.path());
        let user = dir
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("root");
        // Inside the staging home: the home volume's mapping.
        assert_eq!(
            map.translate(&format!("/home/{user}/file.txt")).unwrap(),
            home.join("file.txt").to_string_lossy(),
        );
    }

    #[test]
    fn prefix_boundaries_respect_components() {
        let dir = tempfile::tempdir().unwrap();
        let host = tempfile::tempdir_in(&dir).unwrap();
        let cfg = config_with(
            "~/containers/env",
            &[&format!("{}/work:/activities", host.path().display())],
        );
        let map = CwdMap::build(&cfg, None, dir.path());
        // `/activities2` shares a text prefix with `/activities` but is a
        // different directory; component-boundary matching keeps it out.
        assert_eq!(map.translate("/activities2"), None);
    }

    #[test]
    fn relative_and_unmapped_paths_do_not_translate() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = config_with("~/containers/env", &[]);
        let map = CwdMap::build(&cfg, None, dir.path());
        assert_eq!(map.translate("subdir"), None);
        assert_eq!(map.translate("/srv/elsewhere"), None);
    }

    #[test]
    fn host_caches_bind_the_mirrored_relative_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = config_with("~/containers/env", &[]);
        cfg.storage.host_caches.builtins.cargo = true;
        let map = CwdMap::build(&cfg, None, dir.path());
        let user = dir
            .path()
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("root");
        assert_eq!(
            map.translate(&format!("/home/{user}/.cargo/git")).unwrap(),
            dir.path().join(".cargo").join("git").to_string_lossy(),
        );
    }
}
