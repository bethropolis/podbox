use std::path::{Path, PathBuf};

use anyhow::Result;

use podbox::config::Config;
use podbox::env::HostEnv;
use podbox::xdg::ResolvedXdgDirs;

/// Everything a runtime command needs to know about its target container.
///
/// `enter`, `exec` and `run` all resolve this same handful of values before
/// they build a `podman exec` line. Carrying them together keeps each
/// signature about the command being run rather than the container it runs
/// in, and stops a swapped pair of same-typed parameters from compiling.
///
/// Every field is a shared reference or a `bool`, so the context is `Copy`
/// and a function can destructure it back into the names its body already
/// uses:
///
/// ```ignore
/// let RunContext { env, config, name, xdg, env_overrides, dry_run } = ctx;
/// ```
#[derive(Clone, Copy)]
pub(crate) struct RunContext<'a> {
    pub env: &'a HostEnv,
    pub config: &'a Config,
    pub xdg: &'a ResolvedXdgDirs,
    pub name: &'a str,
    /// `-e KEY=VALUE` overrides given on the command line.
    pub env_overrides: &'a [String],
    pub dry_run: bool,
}

/// Resolve the working directory inside the container from the host CWD.
///
/// Builds a map of host→container mount paths from the config, canonicalizes
/// the host CWD, and picks the longest-prefix match. Falls back to
/// `/home/<username>` when nothing matches.
pub(super) fn resolve_container_workdir(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    strict: bool,
) -> Result<String> {
    let home = format!("/home/{}", env.username);

    let host_cwd = match std::env::current_dir() {
        Ok(p) => match std::fs::canonicalize(&p) {
            Ok(c) => c,
            Err(err) if strict => anyhow::bail!(
                "cannot canonicalize current directory '{}': {err}",
                p.display()
            ),
            Err(_) => p,
        },
        Err(err) if strict => anyhow::bail!("cannot resolve current directory: {err}"),
        Err(_) => return Ok(home.clone()),
    };

    // (canonicalized host path, container path)
    let mut mounts: Vec<(PathBuf, PathBuf)> = Vec::new();

    if let Ok(host) = std::fs::canonicalize(&config.container.home) {
        mounts.push((host, PathBuf::from(&home)));
    }

    let xdg_map: &[(&Option<podbox::xdg::ResolvedXdgDir>, &str)] = &[
        (&xdg.documents, "Documents"),
        (&xdg.downloads, "Downloads"),
        (&xdg.pictures, "Pictures"),
        (&xdg.music, "Music"),
        (&xdg.videos, "Videos"),
        (&xdg.desktop, "Desktop"),
        (&xdg.projects, "Projects"),
    ];
    for (dir, name) in xdg_map {
        if let Some(resolved) = dir
            && let Ok(host) = std::fs::canonicalize(&resolved.path)
        {
            mounts.push((host, PathBuf::from(format!("{home}/{name}"))));
        }
    }

    for mount in &config.container.mounts.extra {
        let parts: Vec<&str> = mount.split(':').collect();
        if parts.len() < 2 {
            continue;
        }
        let host_path = podbox::config::expand_tilde(parts[0]);
        if let Ok(host) = std::fs::canonicalize(host_path) {
            mounts.push((host, PathBuf::from(parts[1])));
        }
    }

    if let Some(path) = translate_host_path(&host_cwd, &mounts) {
        return Ok(path);
    }
    if strict {
        anyhow::bail!(
            "Current directory '{}' is not accessible inside container '{}'.\nHint: Add it to '{}.toml' under [container.mounts] extra = [\"/path/to/dir:/path/to/dir:z\"]",
            host_cwd.display(),
            config.container.name,
            config.container.name
        );
    }
    Ok(home)
}

fn translate_host_path(host_cwd: &Path, mounts: &[(PathBuf, PathBuf)]) -> Option<String> {
    let mut best: Option<(PathBuf, PathBuf)> = None;
    for (host_path, container_path) in mounts {
        if host_cwd == host_path || host_cwd.starts_with(host_path) {
            match &best {
                Some((best_host, _))
                    if best_host.components().count() >= host_path.components().count() => {}
                _ => best = Some((host_path.clone(), container_path.clone())),
            }
        }
    }

    best.map(
        |(host_path, container_path)| match host_cwd.strip_prefix(&host_path) {
            Ok(rel) if !rel.as_os_str().is_empty() => {
                container_path.join(rel).to_string_lossy().to_string()
            }
            _ => container_path.to_string_lossy().to_string(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::translate_host_path;
    use std::path::PathBuf;

    #[test]
    fn translates_using_longest_mount_prefix_and_respects_boundaries() {
        let mounts = vec![
            (PathBuf::from("/work"), PathBuf::from("/home/user/Work")),
            (PathBuf::from("/work/project"), PathBuf::from("/workspace")),
        ];
        assert_eq!(
            translate_host_path(&PathBuf::from("/work/project/src"), &mounts).as_deref(),
            Some("/workspace/src")
        );
        assert_eq!(translate_host_path(&PathBuf::from("/work2"), &mounts), None);
    }
}
