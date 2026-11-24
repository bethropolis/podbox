use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static SHOWN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
static QUIET: AtomicBool = AtomicBool::new(false);

/// Silence [`super::Config::validate`]'s diagnostics until the call
/// site is done scanning. Only inspection commands set this; a command
/// that acts on the definition must hear the warnings.
pub fn set_quiet(quiet: bool) {
    QUIET.store(quiet, Ordering::Relaxed);
}

/// Current quiet state, so a short-lived scan can restore it.
pub fn quiet() -> bool {
    QUIET.load(Ordering::Relaxed)
}

/// Claim and print one diagnostic line: quietly skipped under
/// [`set_quiet`], and deduplicated by text so repeating a parse in the
/// same process does not n-repeat the same line.
pub fn note(message: impl Into<String>) {
    if QUIET.load(Ordering::Relaxed) {
        return;
    }
    let message = message.into();
    let Ok(mut shown) = SHOWN.get_or_init(Mutex::default).lock() else {
        return; // a poisoned set degrades to no printing, never to noise
    };
    if !shown.insert(message.clone()) {
        return;
    }
    eprintln!("{message}");
}

/// Whether a diagnostic was claimed by the once-per-process gate. Used
/// by the lookup regression test to prove a scan stayed silent.
#[cfg(test)]
pub fn was_noted(message: &str) -> bool {
    SHOWN
        .get_or_init(Mutex::default)
        .lock()
        .map(|shown| shown.contains(message))
        .unwrap_or(false)
}
