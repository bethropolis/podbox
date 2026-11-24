use std::collections::HashSet;
use std::os::fd::OwnedFd;
use std::sync::OnceLock;

use nix::poll::PollFlags;

use super::services;

const EXCLUDED_COMMS: &[&str] = &[
    "podbox-guest",
    "podmgr-guest",
    "podman-init",
    "catatonit",
    "tini",
];

/// Open a pidfd for a given PID (Linux 5.3+).
///
/// Returns `Err` on old kernels or when the PID does not exist.
fn open_pidfd(pid: i32) -> std::io::Result<OwnedFd> {
    podbox_protocol::sysutil::open_pidfd(pid)
}

pub(super) struct TrackedProcess {
    _pid: i32,
    pub(super) fd: OwnedFd,
}

/// Read a process's parent PID from `/proc/<pid>/stat`.
///
/// The second field is the executable name in parentheses and may itself
/// contain spaces or parentheses, so the fields after the final `)` are
/// split instead: `state`, then `ppid`.
fn ppid_of(pid: i32) -> Option<i32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_, tail) = stat.rsplit_once(')')?;
    tail.split_whitespace().nth(1)?.parse().ok()
}

/// Walk from `start` up to PID 1, collecting every PID on the way.
///
/// Stops on a cycle, on a missing parent, or after a depth bound.
fn collect_ancestry<F: FnMut(i32) -> Option<i32>>(start: i32, mut parent: F) -> HashSet<i32> {
    let mut chain = HashSet::new();
    let mut pid = start;
    // A PID tree is shallow; the bound only guards a corrupt /proc cycle.
    for _ in 0..64 {
        if pid <= 0 || !chain.insert(pid) {
            break;
        }
        match parent(pid) {
            Some(next) if next > 0 => pid = next,
            _ => break,
        }
    }
    chain
}

/// The guest daemon's own PID and that of each of its ancestors.
///
/// Excluding the daemon by process *name* is not enough: on musl-based
/// images the kernel reports the dynamic loader as the command name
/// (`ld-musl-x86_64.so.1`) rather than `podbox-guest`, so a name-based scan
/// counts the daemon itself as a user task and the container never goes
/// idle. Ancestry is independent of how the binary is named.
fn own_ancestry() -> &'static HashSet<i32> {
    static ANCESTRY: OnceLock<HashSet<i32>> = OnceLock::new();
    ANCESTRY.get_or_init(|| {
        let pid = i32::try_from(std::process::id()).unwrap_or(1);
        collect_ancestry(pid, ppid_of)
    })
}

/// Scan /proc for user processes (anything not in `EXCLUDED_COMMS`, not the
/// daemon or its ancestors, and not a supervised service; plus the container
/// init at PID 1 which is never a user task).
pub(super) fn scan_user_processes() -> Vec<i32> {
    let mut pids = Vec::new();
    let own = own_ancestry();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return pids;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.chars().all(|c| c.is_ascii_digit())
            && let Ok(pid) = name_str.parse::<i32>()
        {
            if pid == 1 {
                // The container init (reaper) is never a user task, whatever
                // its comm (catatonit/tini, but also e.g. `systemd`).
                continue;
            }
            if own.contains(&pid) {
                // The guest daemon and its ancestors. See `own_ancestry`.
                continue;
            }
            if let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) {
                let comm_trimmed = comm.trim();
                if !EXCLUDED_COMMS.contains(&comm_trimmed) && !services::is_service_process(pid) {
                    pids.push(pid);
                }
            }
        }
    }
    pids
}

/// Open pidfds for a list of PIDs.
pub(super) fn track_processes(pids: &[i32]) -> Vec<TrackedProcess> {
    pids.iter()
        .filter_map(|&pid| {
            open_pidfd(pid)
                .ok()
                .map(|fd| TrackedProcess { _pid: pid, fd })
        })
        .collect()
}

/// Check whether any poll events indicate fd readiness.
pub(super) fn has_event(revents: PollFlags) -> bool {
    revents.contains(PollFlags::POLLIN)
        || revents.contains(PollFlags::POLLHUP)
        || revents.contains(PollFlags::POLLERR)
}

/// Max poll interval in ms (`PollTimeout` caps at `u16::MAX` = 65535).
pub(super) const MAX_POLL_MS: u64 = 60_000;

/// Grace period between the final idle scan and firing `IdleTimeout`. A task
/// that spawns during this window is caught by the confirm scan, closing the
/// TOCTOU race where a freshly-started process would be killed by shutdown.
pub(super) const IDLE_CONFIRM_MS: u64 = 1_000;

/// Poll timeout in milliseconds, or `None` for "block forever".
///
/// `idle_timeout_secs == 0` disables idle shutdown entirely. Otherwise the
/// timeout is only armed when no user process is tracked; a non-zero value
/// means the deadline has not passed yet.
pub(super) fn idle_poll_ms(
    idle_enabled: bool,
    tracked_empty: bool,
    idle_deadline: std::time::Instant,
    now: std::time::Instant,
) -> Option<u16> {
    if !idle_enabled || !tracked_empty {
        return None;
    }
    let rem: u64 = idle_deadline
        .saturating_duration_since(now)
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    if rem == 0 {
        return Some(0);
    }
    Some(u16::try_from(rem.min(MAX_POLL_MS)).unwrap_or(u16::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn ancestry_includes_self_and_every_parent() {
        // 8 → 4 → 2 → 1, with 1 being the container init.
        let tree = HashMap::from([(8, Some(4)), (4, Some(2)), (2, Some(1)), (1, None)]);
        let chain = collect_ancestry(8, |pid| tree.get(&pid).copied().flatten());
        assert_eq!(chain, HashSet::from([8, 4, 2, 1]));
    }

    #[test]
    fn ancestry_stops_on_a_cycle() {
        // A corrupt or unusual /proc must not spin the walk.
        let tree = HashMap::from([(10, Some(11)), (11, Some(10))]);
        let chain = collect_ancestry(10, |pid| tree.get(&pid).copied().flatten());
        assert_eq!(chain, HashSet::from([10, 11]));
    }

    #[test]
    fn ancestry_stops_at_a_missing_parent() {
        let chain = collect_ancestry(42, |_| None);
        assert_eq!(chain, HashSet::from([42]));
    }

    #[test]
    fn ancestry_of_pid_one_is_just_pid_one() {
        let chain = collect_ancestry(1, |pid| if pid == 1 { Some(0) } else { None });
        assert_eq!(chain, HashSet::from([1]));
    }

    /// The regression this guards: on musl images the guest's `comm` is the
    /// loader, so name-based exclusion missed the daemon and the container
    /// could never idle. Ancestry excludes it regardless of the name.
    #[test]
    fn own_ancestry_covers_the_live_daemon_whatever_its_comm() {
        let own = own_ancestry();
        let self_pid = i32::try_from(std::process::id()).expect("pid fits in i32");
        assert!(own.contains(&self_pid), "the daemon must exclude itself");
        let comm = std::fs::read_to_string("/proc/self/comm").unwrap_or_default();
        let comm = comm.trim().to_string();
        // Whether or not the name-based list happens to cover this process,
        // ancestry must: that is what makes musl images work.
        if !EXCLUDED_COMMS.contains(&comm.as_str()) {
            assert!(
                own.contains(&self_pid),
                "daemon named {comm} must be excluded by ancestry"
            );
        }
    }
}
