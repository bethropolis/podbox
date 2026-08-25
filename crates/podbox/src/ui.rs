//! Minimal terminal-output helpers for the podbox CLI.
//!
//! Contract (REVIEW.md "Output and error contract"):
//! - Read commands print data on stdout; diagnostics always go to stderr.
//! - [`step`]/[`ok`] are human progress lines: suppressed by `--quiet`.
//! - [`warn`], [`hint`], and [`error`] are always shown.
//! - Colors use `owo-colors` + `supports-color`, which honor `NO_COLOR`.
//!
//! This module is intentionally not a framework: plain functions, no macros,
//! no global logger. Verbosity beyond progress lines goes through `tracing`.

use std::io::Write;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use owo_colors::{OwoColorize, Stream};

static QUIET: AtomicBool = AtomicBool::new(false);
static VERBOSE: AtomicU8 = AtomicU8::new(0);

/// Enable/disable quiet mode (wired to the global `--quiet` flag).
pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

pub fn is_quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

/// Store the `-v` count (0 = info, 1 = debug, 2+ = trace).
pub fn set_verbose(count: u8) {
    VERBOSE.store(count, Ordering::Relaxed);
}

/// True when any `--verbose` level is active. Long-running child processes
/// (podman build/pull) stream their output to the terminal in this mode
/// instead of being captured to the build log.
pub fn is_verbose() -> bool {
    VERBOSE.load(Ordering::Relaxed) > 0
}

fn write_stderr(line: &str) {
    let _ = writeln!(std::io::stderr(), "{line}");
}

/// Print a progress line ("→ msg") to stderr unless quiet.
pub fn step(msg: &str) {
    if is_quiet() {
        return;
    }
    let glyph = "→".if_supports_color(Stream::Stderr, |t| t.dimmed());
    write_stderr(&format!("{glyph} {msg}"));
}

/// Print a success line ("✔ msg") to stderr unless quiet.
pub fn ok(msg: &str) {
    if is_quiet() {
        return;
    }
    let mark = "✔".if_supports_color(Stream::Stderr, |t| t.green());
    let body = msg.if_supports_color(Stream::Stderr, |t| t.green());
    write_stderr(&format!("{mark} {body}"));
}

/// Print a warning to stderr (never suppressed).
pub fn warn(msg: &str) {
    let mark = "!".if_supports_color(Stream::Stderr, |t| t.yellow());
    let body = msg.if_supports_color(Stream::Stderr, |t| t.yellow());
    write_stderr(&format!("{mark} {body}"));
}

/// Print an actionable hint block to stderr (never suppressed).
// No caller yet: wired up when BuildFailed grows its log-path hint (PR 3)
// and by doctor/recover (PR 5). Part of the module's documented contract.
#[allow(dead_code)]
pub fn hint(lines: &[&str]) {
    if lines.is_empty() {
        return;
    }
    let head = "Hint:".if_supports_color(Stream::Stderr, |t| t.cyan().bold().to_string());
    let mut err = std::io::stderr();
    let _ = writeln!(err, "\n{head} {}", lines[0]);
    for l in &lines[1..] {
        let _ = writeln!(err, "      {l}");
    }
}

/// Print the top-level error line to stderr (never suppressed).
///
/// Used once from `main`; command code should return errors instead of
/// printing them so exit-code mapping stays centralized.
pub fn error(msg: &str) {
    let head = "Error:".if_supports_color(Stream::Stderr, |t| t.red().bold().to_string());
    write_stderr(&format!("\n{head} {msg}"));
}
