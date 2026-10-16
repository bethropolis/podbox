//! Single writer for Quadlet unit values.
//!
//! Every interpolated `Key=value` line in generated units goes through this
//! module, so newline/control-character injection and systemd specifier (`%`)
//! expansion are handled in one place. Validation rejects control characters
//! with field paths before codegen runs; the writer doubles `%` and quotes
//! words the way podman's unit parser splits them. A value that still carries
//! a control character is never emitted: the line is skipped and the caller
//! records a warning (defense in depth — unreachable when validation ran).

use std::fmt::Write as _;

/// A unit value that must not be emitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueError {
    /// Short machine-readable reason (`"control-character"`, `"empty-key"`, `"bad-key"`).
    pub reason: &'static str,
}

impl std::fmt::Display for ValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid unit value ({})", self.reason)
    }
}

impl std::error::Error for ValueError {}

/// Escape a raw unit value: reject NUL/CR/LF, double `%`.
///
/// systemd expands `%` specifiers in most unit directives, so a literal `%`
/// from config (a home path containing `%h`, `date +%s` in an env value)
/// must arrive doubled. Intentional specifiers (`%t`, `%h`, `%U`, …) live in
/// the emitter's literal format string and never pass through here — only
/// the interpolated parts do.
pub fn raw(value: &str) -> Result<String, ValueError> {
    if value.bytes().any(|b| b == 0 || b == b'\n' || b == b'\r') {
        return Err(ValueError {
            reason: "control-character",
        });
    }
    Ok(value.replace('%', "%%"))
}

/// Escape one whitespace-separated word the way podman splits unit values.
///
/// Values containing whitespace, a double quote, a backslash or a `%` are
/// wrapped in double quotes with C-style escapes, matching podman's
/// `escapeWords` rules; anything else goes through [`raw`] unchanged.
pub fn word(value: &str) -> Result<String, ValueError> {
    let escaped = raw(value)?;
    if !escaped
        .bytes()
        .any(|b| b.is_ascii_whitespace() || b == b'"' || b == b'\\' || b == b'%')
    {
        return Ok(escaped);
    }
    let mut out = String::with_capacity(escaped.len() + 2);
    out.push('"');
    for c in escaped.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    Ok(out)
}

/// Render a full `KEY=value` environment assignment.
///
/// The key must be a non-empty `[A-Za-z0-9_]` name (matching the check in
/// `emit_env`); the value goes through [`word`].
pub fn env(key: &str, value: &str) -> Result<String, ValueError> {
    if key.is_empty() {
        return Err(ValueError {
            reason: "empty-key",
        });
    }
    if !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err(ValueError { reason: "bad-key" });
    }
    Ok(format!("{key}={}", word(value)?))
}

/// Push `KEY=<raw(value)>`, skipping the line with a warning on error.
pub fn push_raw(lines: &mut Vec<String>, warnings: &mut Vec<String>, key: &str, value: &str) {
    match raw(value) {
        Ok(escaped) => {
            let mut line = String::with_capacity(key.len() + escaped.len() + 1);
            let _ = write!(line, "{key}={escaped}");
            lines.push(line);
        }
        Err(e) => warnings.push(format!("skipping {key}= line: {e}")),
    }
}

/// Push `KEY=<word(value)>`, skipping the line with a warning on error.
pub fn push_word(lines: &mut Vec<String>, warnings: &mut Vec<String>, key: &str, value: &str) {
    match word(value) {
        Ok(escaped) => {
            let mut line = String::with_capacity(key.len() + escaped.len() + 1);
            let _ = write!(line, "{key}={escaped}");
            lines.push(line);
        }
        Err(e) => warnings.push(format!("skipping {key}= line: {e}")),
    }
}

/// Push `Environment=KEY=<word(value)>`, skipping with a warning on error.
pub fn push_env(lines: &mut Vec<String>, warnings: &mut Vec<String>, key: &str, value: &str) {
    match env(key, value) {
        Ok(assignment) => lines.push(format!("Environment={assignment}")),
        Err(e) => warnings.push(format!("skipping Environment={key}= line: {e}")),
    }
}

/// Push a fully composed line whose interpolated parts were already escaped
/// with [`raw`]/[`word`]/[`env`] and whose `%` sequences are intentional
/// systemd specifiers (`%t`, `%h`, `%U`, …) from the emitter's literal.
pub fn push_composed(lines: &mut Vec<String>, line: String) {
    lines.push(line);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_rejects_control_characters_and_doubles_percent() {
        assert_eq!(raw("plain").unwrap(), "plain");
        assert_eq!(raw("%h/.themes").unwrap(), "%%h/.themes");
        assert_eq!(raw("100%").unwrap(), "100%%");
        for bad in ["a\nb", "a\rb", "a\0b", "x\nExecStartPre=/bin/evil"] {
            assert!(raw(bad).is_err(), "{bad:?} must be rejected");
        }
    }

    #[test]
    fn word_quotes_and_c_escapes() {
        assert_eq!(word("simple").unwrap(), "simple");
        // Tab, quote and backslash force quoting with C-escapes.
        assert_eq!(word("a\tb\"c").unwrap(), "\"a\\tb\\\"c\"");
        assert_eq!(word("a b").unwrap(), "\"a b\"");
        assert_eq!(word("C:\\path").unwrap(), "\"C:\\\\path\"");
        assert_eq!(word("100%").unwrap(), "\"100%%\"");
        assert_eq!(word("").unwrap(), "");
    }

    #[test]
    fn word_round_trips_through_podman_unescaping() {
        // Re-implementation of podman's word-splitting unescape rules:
        // inside quotes, \\ and \" collapse; outside, words split on
        // whitespace. Every quoted output must decode back to the input.
        fn unescape_word(s: &str) -> String {
            let mut out = String::new();
            let mut chars = s.chars();
            if s.starts_with('"') {
                chars.next();
            }
            while let Some(c) = chars.next() {
                if c == '"' {
                    break;
                }
                if c == '\\' {
                    match chars.next() {
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some(other) => out.push(other),
                        None => out.push('\\'),
                    }
                } else {
                    out.push(c);
                }
            }
            out.replace("%%", "%")
        }
        for v in ["a\tb\"c", "hello world", "C:\\path", "100%", "plain"] {
            assert_eq!(unescape_word(&word(v).unwrap()), v, "{v:?}");
        }
    }

    #[test]
    fn env_validates_keys() {
        assert_eq!(env("FOO", "a b").unwrap(), "FOO=\"a b\"");
        assert!(env("", "x").is_err());
        assert!(env("HAS-DASH", "x").is_err());
        assert!(env("HAS SPACE", "x").is_err());
    }

    #[test]
    fn push_helpers_skip_with_warning() {
        let mut lines = Vec::new();
        let mut warnings = Vec::new();
        push_raw(&mut lines, &mut warnings, "After", "good.service");
        push_raw(&mut lines, &mut warnings, "After", "x\nEvil=1");
        assert_eq!(lines, ["After=good.service"]);
        assert_eq!(warnings.len(), 1);
    }
}
