//! Append-only action history for podbox containers.
//!
//! Recorded at the *success* points of the lifecycle commands so
//! `podbox history` can answer "what did I do to `<name>`, and when?".
//! Writes are best-effort: recording must never break the command that
//! triggered it, so callers discard the [`record`] error. The log lives in the
//! XDG state dir at `~/.local/state/podbox/history.log`.
//!
//! Line format (one event per line, tab-separated):
//! `TIMESTAMP\tNAME\tACTION\tDETAIL`

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// A single recorded action.
#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    /// UTC timestamp in RFC3339 (seconds resolution).
    pub timestamp: String,
    /// The container name the action targeted.
    pub name: String,
    /// Action verb, e.g. "create", "build", "start".
    pub action: String,
    /// Free-form detail (empty when there is none).
    pub detail: String,
}

const LOG_FILE: &str = "history.log";

/// Path to the history log: `<state>/podbox/history.log`.
pub fn log_path() -> PathBuf {
    dirs::state_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/state"))
        .join("podbox")
        .join(LOG_FILE)
}

/// Record an action by appending to the log (best-effort).
pub fn record(name: &str, action: &str, detail: &str) -> io::Result<()> {
    let path = log_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    record_to(&path, name, action, detail)
}

/// Append one event to `path` (used by [`record`] and by unit tests).
fn record_to(path: &Path, name: &str, action: &str, detail: &str) -> io::Result<()> {
    let line = if detail.is_empty() {
        format!("{}\t{}\t{}\n", timestamp(), name, action)
    } else {
        format!("{}\t{}\t{}\t{}\n", timestamp(), name, action, detail)
    };
    let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(line.as_bytes())
}

/// Load the history, most recent first. A missing/unreadable log yields an
/// empty vec with a read error (callers treat that as "nothing to show").
pub fn load() -> io::Result<Vec<HistoryEntry>> {
    load_from(&log_path())
}

/// Read and parse `path`, newest first (used by [`load`] and tests).
fn load_from(path: &Path) -> io::Result<Vec<HistoryEntry>> {
    let content = fs::read_to_string(path)?;
    let mut entries: Vec<HistoryEntry> = content.lines().filter_map(parse_line).collect();
    entries.reverse(); // newest last
    Ok(entries)
}

/// Parse a single tab-separated line into an entry (or `None` if malformed).
fn parse_line(line: &str) -> Option<HistoryEntry> {
    let raw = line.trim_end_matches('\r');
    if raw.trim().is_empty() {
        return None;
    }
    let mut it = raw.splitn(4, '\t');
    let timestamp = it.next()?.to_string();
    let name = it.next()?.to_string();
    let action = it.next()?.to_string();
    let detail = it.next().unwrap_or("").to_string();
    Some(HistoryEntry {
        timestamp,
        name,
        action,
        detail,
    })
}

fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    format_rfc3339(secs)
}

/// Format a Unix epoch seconds as RFC3339 UTC (`YYYY-MM-DDTHH:MM:SSZ`) using
/// the civil-from-days algorithm so the log stays readable without `chrono`.
fn format_rfc3339(secs: i64) -> String {
    const DAY: i64 = 86_400;
    let days = secs.div_euclid(DAY);
    let rem = secs.rem_euclid(DAY);
    let (y, m, d) = civil_from_days(days);
    let hh = rem / 3600;
    let mm = (rem % 3600) / 60;
    let ss = rem % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Howard Hinnant's civil-from-days (days since epoch → (year, month, day)).
/// All intermediate values stay in `i64`; only the provably-in-range day and
/// month are narrowed to `u32`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = u32::try_from(doy - (153 * mp + 2) / 5 + 1).unwrap_or(1); // [1, 31]
    let m = u32::try_from(if mp < 10 { mp + 3 } else { mp - 9 }).unwrap_or(1); // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339_formats_known_epochs() {
        assert_eq!(format_rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_rfc3339(86_400), "1970-01-02T00:00:00Z");
        assert_eq!(format_rfc3339(1_000_000_000), "2001-09-09T01:46:40Z");
        // Before the epoch round-trips into 1969 (div_euclid/rem_euclid).
        assert_eq!(format_rfc3339(-1), "1969-12-31T23:59:59Z");
    }

    #[test]
    fn round_trip_records_and_loads_preserving_order() {
        let dir = std::env::temp_dir().join(format!("podbox-history-{}", std::process::id()));
        let path = dir.join("history.log");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();

        record_to(&path, "alpha", "build", "rebuilt after stable pull").unwrap();
        record_to(&path, "beta", "start", "").unwrap();
        record_to(&path, "alpha", "enable", "quadlet installed").unwrap();

        let entries = load_from(&path).unwrap();
        // Newest first.
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].action, "enable");
        assert_eq!(entries[0].name, "alpha");
        assert_eq!(entries[0].detail, "quadlet installed");
        assert_eq!(entries[1].action, "start");
        assert_eq!(entries[1].detail, "");
        assert_eq!(entries[2].action, "build");
        // Every timestamp is RFC3339 UTC.
        for e in &entries {
            assert!(e.timestamp.ends_with('Z'), "{}", e.timestamp);
        }

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_and_blank_lines_are_skipped() {
        let file = std::env::temp_dir().join("podbox-history-parse.log");
        fs::write(
            &file,
            "garbage-line\n\n\t\n2026-08-26T03:00:00Z\tfoo\tbuild\tx\tyzw\n",
        )
        .unwrap();
        let entries = load_from(&file).unwrap();
        // The trailing line parses with detail `x\tyzw` (detail keeps tabs);
        // garbage / blank / whitespace-only lines yield nothing.
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].timestamp, "2026-08-26T03:00:00Z");
        assert_eq!(entries[0].name, "foo");
        assert_eq!(entries[0].action, "build");
        assert_eq!(entries[0].detail, "x\tyzw");
        let _ = fs::remove_file(&file);
    }
}