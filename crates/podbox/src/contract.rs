//! Client contract helpers (`docs/clients.md`).
//!
//! podbox has no daemon, so the CLI is the API. Anything a client parses
//! is covered here and versioned by [`CONTRACT_SCHEMA`]:
//! - Every podbox-owned `--output json` payload carries `"schema": 1`.
//!   The number changes only on a breaking change; new fields and new
//!   commands never bump it.
//! - In JSON mode, failures print a structured error object on stdout
//!   (same exit code as the human error on stderr).
//!
//! Pure module: no host dependencies, compiles for wasm.

use serde_json::{Map, Value};

/// Contract version stamped on every JSON payload. Bump only on a
/// breaking change to a documented shape.
pub const CONTRACT_SCHEMA: u32 = 1;

/// Merge `"schema": 1` into an object payload. Non-objects are wrapped
/// as `{"schema": 1, "data": ...}` so the stamp is always present.
pub fn envelope(mut payload: Value) -> Value {
    match &mut payload {
        Value::Object(map) => {
            map.insert("schema".into(), Value::from(CONTRACT_SCHEMA));
            payload
        }
        other => {
            let mut map = Map::new();
            map.insert("schema".into(), Value::from(CONTRACT_SCHEMA));
            map.insert("data".into(), other.take());
            Value::Object(map)
        }
    }
}

/// Print an enveloped payload to stdout.
pub fn print_json(payload: Value) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(&envelope(payload))?);
    Ok(())
}

/// One compact enveloped line on stdout: the NDJSON stream primitive.
/// (`watch` status lines, `build` progress events, and the terminating
/// error object all go through here.)
pub fn emit_event(payload: Value) {
    let line =
        serde_json::to_string(&envelope(payload)).unwrap_or_else(|_| r#"{"schema":1}"#.into());
    println!("{line}");
}

/// Capability flags for `podbox info --output json`. Additive only:
/// clients probe for the flags they need and tolerate unknown ones.
pub const CONTRACT_FEATURES: &[&str] = &[
    "schema",
    "error_envelope",
    "status_extras",
    "export_list",
    "restart",
    "config",
    "watch",
    "build_ndjson",
];

/// Convert a byte offset into 1-based (line, column). Columns count Unicode
/// scalar values, matching how editors render the line.
pub fn offset_to_line_col(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text[..offset];
    let line = before.bytes().filter(|&b| b == b'\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    (line, column)
}

/// Best-effort line lookup for a dotted schema path (`container.memory`).
///
/// Matches the `key =` assignment for the last segment, preferring one under
/// the matching `[table.path]` header; single-segment paths match top-level
/// assignments. Returns `None` when the key is absent (derived or defaulted
/// values have no source line).
pub fn find_field_line(text: &str, field: &str) -> Option<usize> {
    let segments: Vec<String> = field
        .split('.')
        .map(|s| match s.find('[') {
            // `provision[0].name` style paths index into arrays-of-tables.
            Some(bracket) => s[..bracket].to_string(),
            None => s.to_string(),
        })
        .collect();
    let (table_path, key) = match segments.as_slice() {
        [] => return None,
        [single] => (None, single.clone()),
        _ => (
            Some(segments[..segments.len() - 1].join(".")),
            segments[segments.len() - 1].clone(),
        ),
    };
    let mut current_table = String::new();
    let mut header_line: Option<usize> = None;
    let mut fallback: Option<usize> = None;
    for (idx, raw) in text.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw.trim();
        if trimmed.starts_with('[') && !trimmed.starts_with("[![") {
            // `[[array]]` headers flow through here too: stripping brackets
            // leaves the bare table name, which still qualifies lookups.
            let header = trimmed
                .trim_start_matches('[')
                .trim_end_matches(']')
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();
            current_table = header.trim_start_matches('[').to_string();
            if table_path.as_deref() == Some(current_table.as_str()) {
                header_line = Some(line_no);
            }
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((lhs, _)) = trimmed.split_once('=') {
            let lhs = lhs.trim().trim_matches('"').trim_matches('\'');
            if lhs == key {
                if table_path.is_none() {
                    return Some(line_no);
                }
                if table_path.as_deref() == Some(current_table.as_str()) {
                    return Some(line_no);
                }
                fallback.get_or_insert(line_no);
            }
        }
    }
    fallback.or(header_line)
}

/// Structured error envelope for JSON mode. `code` is a stable lowercase
/// identifier ([`crate::error::PodboxError::code`]); `message` is for people.
pub fn error_payload(code: &str, message: &str) -> Value {
    envelope(serde_json::json!({
        "error": { "code": code, "message": message },
    }))
}

/// Reject `--output ndjson` on single-document commands.
///
/// Only the streaming commands (`watch`, `build`) speak NDJSON; every other
/// reader bails through here so the flag fails loudly instead of silently
/// printing pretty JSON into a line parser.
pub fn bail_ndjson(cmd: &'static str) -> anyhow::Error {
    anyhow::anyhow!(
        "--output ndjson is only supported by `podbox watch` and `podbox build`; \
         `{cmd}` supports text and json"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_stamps_objects() {
        let v = envelope(serde_json::json!({"containers": []}));
        assert_eq!(v["schema"], 1);
        assert!(v.get("containers").is_some());
    }

    #[test]
    fn envelope_wraps_non_objects() {
        let v = envelope(serde_json::json!([1, 2]));
        assert_eq!(v["schema"], 1);
        assert_eq!(v["data"], serde_json::json!([1, 2]));
    }

    #[test]
    fn error_payload_shape() {
        let v = error_payload("container_missing", "No environment named 'dev'.");
        assert_eq!(v["schema"], 1);
        assert_eq!(v["error"]["code"], "container_missing");
    }

    #[test]
    fn field_line_prefers_matching_table() {
        let text = "name = \"x\"\n[container]\nmemory = \"4G\"\n[network]\nmemory = \"y\"\n";
        assert_eq!(find_field_line(text, "container.memory"), Some(3));
        assert_eq!(find_field_line(text, "network.memory"), Some(5));
        assert_eq!(find_field_line(text, "name"), Some(1));
        // Missing key under a present table falls back to the header.
        assert_eq!(find_field_line(text, "container.missing"), Some(2));
        assert_eq!(find_field_line(text, "nosuch.key"), None);
    }
}
