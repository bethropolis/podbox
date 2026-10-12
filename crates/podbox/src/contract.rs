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

/// Structured error envelope for JSON mode. `code` is a stable lowercase
/// identifier ([`crate::error::PodboxError::code`]); `message` is for people.
pub fn error_payload(code: &str, message: &str) -> Value {
    envelope(serde_json::json!({
        "error": { "code": code, "message": message },
    }))
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
}
