use std::collections::HashSet;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

fn run_command_with_timeout(
    mut cmd: Command,
    timeout: std::time::Duration,
) -> Option<std::process::Output> {
    let child = cmd.spawn().ok()?;
    podbox_protocol::sysutil::wait_child_timeout(child, timeout).ok()
}

/// Marker-delimited probe output: interactive shells print MOTD/greetings
/// to stdout (fastfetch art, color codes), which would otherwise be captured
/// as part of `$PATH`. The markers isolate the actual value.
const PATH_MARKER_START: &str = "__podbox_path__";
const PATH_MARKER_END: &str = "__podbox_path_end__";

/// Extract the probed PATH from shell output: the text between the markers.
///
/// Returns `None` when either marker is missing. Interactive shells may
/// print banners (fastfetch, MOTD) to stdout, so we trust *only* what sits
/// between our own markers; an unmarked result is garbage, not a PATH.
fn extract_probed_path(output: &str) -> Option<&str> {
    let (_, rest) = output.split_once(PATH_MARKER_START)?;
    let (path, _) = rest.split_once(PATH_MARKER_END)?;
    Some(path.trim())
}

/// Validate that a candidate string is a sane colon-separated PATH: non-empty,
/// and every entry is absolute and free of whitespace/control characters.
fn is_valid_path(p: &str) -> bool {
    if p.is_empty() {
        return false;
    }
    p.split(':').all(|entry| {
        !entry.is_empty()
            && entry.starts_with('/')
            && !entry.chars().any(char::is_whitespace)
            && !entry.chars().any(char::is_control)
    })
}

/// Build the probe command for a given shell. Keeping the markers out of the
/// variable name is the whole point: `$PATH__podbox_path_end__` would be read
/// as a single (unset) variable in fish/bash/zsh. Fish stores PATH as a list,
/// so it gets joined explicitly.
fn path_probe(shell: &std::path::Path) -> String {
    let is_fish = shell
        .file_name()
        .is_some_and(|n| n == std::ffi::OsStr::new("fish"));
    if is_fish {
        format!("printf '%s%s%s' {PATH_MARKER_START} (string join : $PATH) {PATH_MARKER_END}")
    } else {
        format!("printf '%s%s%s' {PATH_MARKER_START} \"$PATH\" {PATH_MARKER_END}")
    }
}

/// System directories that must always be reachable even if a probe missed them.
const SYSTEM_PATH_DEFAULTS: &[&str] = &[
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];

/// Merge probed PATH entries with the system defaults, deduplicating while
/// preserving order (probed entries first, defaults appended).
fn merge_system_path(probed: &str) -> String {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<&str> = Vec::new();
    for entry in probed
        .split(':')
        .chain(SYSTEM_PATH_DEFAULTS.iter().copied())
    {
        if entry.is_empty() {
            continue;
        }
        if seen.insert(entry) {
            out.push(entry);
        }
    }
    out.join(":")
}

/// Persistent PATH cache: freshness window and sanity cap.
const PATH_CACHE_TTL_SECS: u64 = 24 * 3600;
const PATH_CACHE_MAX_LEN: usize = 16 * 1024;

fn path_cache_file(home: &str) -> PathBuf {
    PathBuf::from(home).join(".cache/podbox/path")
}

/// Cache format version. Bumped so caches written by older guests (which may
/// contain a poisoned PATH) are ignored and re-probed.
const PATH_CACHE_FORMAT: &str = "2";

fn unix_now_secs() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Read a cached PATH: accepted only on guest-version match, age under TTL,
/// and sane content. Anything else returns `None` (re-probe).
fn read_path_cache(home: &str, now_secs: u64) -> Option<String> {
    let content = std::fs::read_to_string(path_cache_file(home)).ok()?;
    let mut parts = content.splitn(4, '\n');
    let format = parts.next()?;
    let version = parts.next()?;
    let stamp: u64 = parts.next()?.trim().parse().ok()?;
    let path = parts.next()?.trim_end_matches('\n');
    if format != PATH_CACHE_FORMAT {
        return None;
    }
    if version != crate::VERSION {
        return None;
    }
    if now_secs.saturating_sub(stamp) > PATH_CACHE_TTL_SECS {
        return None;
    }
    if path.is_empty()
        || path.len() > PATH_CACHE_MAX_LEN
        || path.bytes().any(|b| b == 0 || b == b'\n' || b == b'\r')
        || !is_valid_path(path)
    {
        return None;
    }
    Some(path.to_string())
}

/// Persist a probed PATH for future boots. Best-effort: failures are silent
/// (worst case is today's behavior — probe every boot).
fn write_path_cache(home: &str, uid: u32, gid: u32, path: &str, now_secs: u64) {
    if !is_valid_path(path) {
        return;
    }
    let file = path_cache_file(home);
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
        // `create_dir_all` may create `~/.cache` and `~/.cache/podbox` as
        // root; chown both so the user can write the cache later.
        let _ = std::os::unix::fs::chown(parent, Some(uid), Some(gid));
        if let Some(cache_dir) = parent.parent() {
            let _ = std::os::unix::fs::chown(cache_dir, Some(uid), Some(gid));
        }
    }
    let content = format!(
        "{}\n{}\n{}\n{}",
        PATH_CACHE_FORMAT,
        crate::VERSION,
        now_secs,
        path
    );
    if std::fs::write(&file, &content).is_ok() {
        let _ = std::os::unix::fs::chown(&file, Some(uid), Some(gid));
    }
}

/// Resolve the user's full PATH by spawning their configured shell in
/// interactive mode and capturing `$PATH`.  Writes the result to
/// `/run/podbox/path` for consumption by the host-side `read_user_path()`.
///
/// Probing is skipped when a fresh persistent cache exists: the home volume
/// survives container recreation, so only the first-ever boot (and any boot
/// a day after the last probe) pays for shell spawns.
///
/// Silently skips on any error (no file = host falls back to Quadlet default).
#[allow(clippy::similar_names)]
pub(super) fn resolve_user_path() {
    let host_user = std::env::var("HOST_USER").ok();
    let host_uid = std::env::var("HOST_UID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());
    let host_gid = std::env::var("HOST_GID")
        .ok()
        .and_then(|s| s.parse::<u32>().ok());

    let (Some(ref user), Some(uid), Some(gid)) = (host_user.as_ref(), host_uid, host_gid) else {
        return;
    };
    let home = format!("/home/{user}");

    // Fast path: a fresh persistent cache (home volume survives recreation).
    if let Some(now) = unix_now_secs()
        && let Some(cached) = read_path_cache(&home, now)
    {
        // Merge system defaults before publishing; a failed probe leaves the
        // current PATH untouched (we simply don't write here on failure).
        let merged = merge_system_path(&cached);
        let _ = std::fs::write("/run/podbox/path", &merged);
        return;
    }

    // Determine the best shell for PATH resolution.
    // The user's interactive shell (e.g. fish) adds bun, cargo,
    // mise, etc. to PATH via its config — the passwd shell may be /bin/sh
    // which won't source those.  Try fish first, then passwd, then bash.
    let passwd_shell = std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|p| {
            p.lines()
                .find(|l| l.starts_with(&format!("{user}:")))
                .and_then(|l| l.split(':').nth(6))
                .map(std::string::ToString::to_string)
        })
        .unwrap_or_else(|| "/bin/sh".to_string());

    let mut candidates = vec![
        PathBuf::from("/usr/bin/fish"),
        PathBuf::from(&passwd_shell),
        PathBuf::from("/bin/bash"),
        PathBuf::from("/bin/sh"),
    ];
    candidates.sort();
    candidates.dedup();

    let mut best_path = String::new();

    for shell in &candidates {
        if !shell.exists() {
            continue;
        }
        // Interactive (-ic) on purpose: user configs (fish config.fish, .bashrc)
        // may extend PATH only in interactive mode. Those configs may also print
        // banners/MOTD to stdout — that is exactly why we use markers and
        // validate the result instead of trusting raw output.
        let probe = path_probe(shell);
        let mut cmd = Command::new(shell);
        cmd.args(["-ic", &probe])
            .uid(uid)
            .gid(gid)
            .env("HOME", format!("/home/{user}"))
            .env("USER", user)
            .env("LOGNAME", user)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null());

        if let Some(output) = run_command_with_timeout(cmd, std::time::Duration::from_secs(2))
            && output.status.success()
        {
            let resolved = String::from_utf8_lossy(&output.stdout);
            if let Some(trimmed) = extract_probed_path(&resolved)
                && is_valid_path(trimmed)
                && trimmed.len() > best_path.len()
            {
                best_path = trimmed.to_string();
            }
        }
    }

    if best_path.is_empty() {
        return;
    }

    let merged = merge_system_path(&best_path);
    let _ = std::fs::write(PathBuf::from("/run/podbox/path"), &merged);
    if let Some(now) = unix_now_secs() {
        write_path_cache(&home, uid, gid, &best_path, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_probed_path_strips_motd_noise() {
        let noisy = "\x1b[100m   \x1b[m\nfastfetch art here\n__podbox_path__/usr/bin:/bin__podbox_path_end__\n";
        assert_eq!(extract_probed_path(noisy), Some("/usr/bin:/bin"));
    }

    #[test]
    fn extract_probed_path_rejects_missing_markers() {
        assert_eq!(extract_probed_path("  /usr/bin:/bin\n"), None);
    }

    #[test]
    fn extract_probed_path_empty_when_markers_empty() {
        assert_eq!(
            extract_probed_path("noise__podbox_path____podbox_path_end__tail"),
            Some("")
        );
        assert!(!is_valid_path(""));
    }

    #[test]
    fn extract_probed_path_rejects_start_without_end() {
        assert_eq!(extract_probed_path("banner\n__podbox_path__"), None);
    }

    #[test]
    fn path_probe_fish_contains_string_join() {
        let fish = std::path::Path::new("/usr/bin/fish");
        let p = path_probe(fish);
        assert!(p.contains("string join :"));
        assert!(!p.contains("$PATH__podbox"));
        assert!(!p.contains("$PATH_"));
    }

    #[test]
    fn path_probe_bash_uses_quoted_path() {
        let bash = std::path::Path::new("/bin/bash");
        let p = path_probe(bash);
        assert!(p.contains("\"$PATH\""));
        assert!(!p.contains("$PATH__podbox"));
        assert!(!p.contains("$PATH_"));
    }

    #[test]
    fn is_valid_path_accepts_and_rejects() {
        assert!(is_valid_path("/usr/bin:/bin"));
        assert!(!is_valid_path("/a::/b"));
        assert!(!is_valid_path("/usr/bin:/bin with space"));
        assert!(!is_valid_path("/usr/bin\n:/bin"));
        assert!(!is_valid_path("relative:/bin"));
        assert!(!is_valid_path("fastfetch banner"));
    }

    fn cache_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("podbox-pathcache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn path_cache_roundtrip_when_fresh() {
        let home = cache_dir("fresh");
        let home_str = home.to_string_lossy().to_string();
        write_path_cache(&home_str, 1000, 1000, "/usr/bin:/bin", 1_700_000_000);
        assert_eq!(
            read_path_cache(&home_str, 1_700_000_000 + 3600),
            Some("/usr/bin:/bin".to_string())
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn path_cache_ignores_invalid_and_old_format() {
        let home = cache_dir("invalid");
        let home_str = home.to_string_lossy().to_string();
        // Invalid cached value (banner, not a PATH).
        let file = home.join(".cache/podbox/path");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(
            &file,
            format!(
                "{}\n{}\n{}\n{}",
                PATH_CACHE_FORMAT,
                crate::VERSION,
                1_700_000_000_u64,
                "fastfetch banner"
            ),
        )
        .unwrap();
        assert_eq!(read_path_cache(&home_str, 1_700_000_000 + 1), None);
        // Old format (3 lines, no format header).
        std::fs::write(
            &file,
            format!(
                "{}\n{}\n{}",
                crate::VERSION,
                1_700_000_000_u64,
                "/usr/bin:/bin"
            ),
        )
        .unwrap();
        assert_eq!(read_path_cache(&home_str, 1_700_000_000 + 1), None);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn path_cache_rejects_stale_version_and_age() {
        let home = cache_dir("stale");
        let home_str = home.to_string_lossy().to_string();
        write_path_cache(&home_str, 1000, 1000, "/usr/bin:/bin", 1_700_000_000);
        // Wrong version line.
        let file = home.join(".cache/podbox/path");
        let bad = std::fs::read_to_string(&file)
            .unwrap()
            .replacen(crate::VERSION, "v0.0.0", 1);
        std::fs::write(&file, bad).unwrap();
        assert_eq!(read_path_cache(&home_str, 1_700_000_000 + 1), None);
        // Expired.
        write_path_cache(&home_str, 1000, 1000, "/usr/bin:/bin", 1_700_000_000);
        assert_eq!(
            read_path_cache(&home_str, 1_700_000_000 + PATH_CACHE_TTL_SECS + 1),
            None
        );
        // Empty and overlong content.
        std::fs::write(home.join(".cache/podbox/path"), "").unwrap();
        assert_eq!(read_path_cache(&home_str, 1_700_000_000 + 1), None);
        let _ = std::fs::remove_dir_all(&home);
    }
}
