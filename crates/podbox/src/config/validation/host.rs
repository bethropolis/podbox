use crate::config::Config;

/// Host-side paths this definition names, checked for existence only.
///
/// A missing binary or directory fails later with no pointer back to the
/// config: a host-exec shim errors when invoked, and a host cache whose
/// directory is absent is skipped at runtime. Both are worth knowing at
/// validation time.
///
/// Deliberately *warnings*, never errors: definitions are portable, and
/// this host may simply not have the path yet. `podbox doctor` reports
/// the same facts as FAILs for the host a command is about to run on.
/// Ecosystem facts (does this package exist in the base's repos, does
/// this tool version exist) are intentionally not here: answering them
/// needs network access and per-ecosystem knowledge that would make
/// validation non-deterministic.
pub(super) fn host_path_warnings(config: &Config) -> Vec<String> {
    let mut out = Vec::new();

    // host-exec: every allowlist entry points at a host binary.
    if config.integration.host_exec.enabled
        && let Some(map) = &config.integration.host_exec.allowlist
    {
        let mut sorted: Vec<_> = map.iter().collect();
        sorted.sort_by_key(|(k, _)| *k);
        for (alias, entry) in sorted {
            let path = entry.path();
            if super::values::is_absolute_path(path) && !std::path::Path::new(path).exists() {
                out.push(format!(
                    "integration.host_exec.allowlist.{alias}: host binary '{path}' does not exist — \
                     the '{alias}' shim fails until it is installed"
                ));
            }
        }
    }

    // Host caches: the built-ins share their relative path on both
    // sides; customs carry an explicit host path.
    let host_caches = &config.storage.host_caches;
    for p in host_caches.enabled_paths() {
        let host_path = crate::config::fs::expand_tilde(p.container_path);
        if !host_path.exists() {
            out.push(format!(
                "storage.host_caches.{}: host directory '{}' does not exist — \
                 the cache is skipped at runtime",
                p.cache,
                host_path.display()
            ));
        }
    }
    for cache in &host_caches.custom {
        let host_path = crate::config::fs::expand_tilde(&cache.host_path);
        if !host_path.exists() {
            out.push(format!(
                "storage.host_caches.custom.{}: host directory '{}' does not exist — \
                 the cache is skipped at runtime",
                cache.name,
                host_path.display()
            ));
        }
    }

    // Dotfiles from the host must exist to be copied in.
    if let Some(dotfiles) = &config.dotfiles
        && let Some(dir) = dotfiles.source.strip_prefix("host:")
        && !dir.trim().is_empty()
    {
        let path = crate::config::fs::expand_tilde(dir);
        if !path.exists() {
            out.push(format!(
                "dotfiles.source: host directory '{}' does not exist — \
                 nothing will be copied into the home",
                path.display()
            ));
        }
    }

    out
}
