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
    crate::tomlscan::offset_to_line_col(text, offset)
}

/// Best-effort line lookup for a dotted schema path (`container.memory`).
///
/// Resolved by the shared field-position scanner ([`crate::tomlscan`]),
/// which handles quoted keys, inline tables and array-of-tables indices.
/// The exact path is preferred; a missing leaf falls back to the nearest
/// present ancestor (the `[table]` header or `[[entry]]` header), so a
/// defaulted key still highlights its section. Returns `None` when the key
/// has no source line.
pub fn find_field_line(text: &str, field: &str) -> Option<usize> {
    crate::tomlscan::field_line(text, field).or_else(|| {
        // Strip trailing segments toward the root: `container.memory` on
        // a defaulted `memory` still points at `[container]`.
        let mut path = field.to_string();
        while let Some(dot) = path.rfind('.') {
            path.truncate(dot);
            if let Some(line) = crate::tomlscan::field_line(text, &path) {
                return Some(line);
            }
        }
        None
    })
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
