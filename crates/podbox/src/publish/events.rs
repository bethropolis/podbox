//! Event rendering for publish: ndjson stream and json document.
//!
//! Pure module: build enveloped JSON payloads.

use serde_json::{Value, json};

use crate::contract;
use crate::publish::plan::PlanImage;

/// One image event in the ndjson stream (or element of the json array).
#[derive(Debug, Clone)]
pub struct ImageEvent {
    pub key: String,
    pub ref_str: String,
    pub tags: Vec<String>,
    pub pushed: bool,
    pub skipped: bool,
    pub digest: Option<String>,
    pub build_key: Option<String>,
    pub retagged: Option<Vec<String>>,
    pub first_push: Option<bool>,
    pub error: Option<String>,
    pub error_code: Option<String>,
}

/// `step` event: human progress text for one image.
pub fn step_event(index: usize, total: usize, text: &str) -> Value {
    contract::envelope(json!({
        "event": "step",
        "index": index,
        "total": total,
        "text": text,
    }))
}

/// `image` event.
pub fn image_event(event: &ImageEvent) -> Value {
    let mut payload = serde_json::Map::new();
    payload.insert("event".into(), json!("image"));
    payload.insert("key".into(), json!(&event.key));
    payload.insert("ref".into(), json!(&event.ref_str));
    payload.insert("tags".into(), json!(&event.tags));
    payload.insert("pushed".into(), json!(event.pushed));
    payload.insert("skipped".into(), json!(event.skipped));
    if let Some(d) = &event.digest {
        payload.insert("digest".into(), json!(d));
    }
    if let Some(b) = &event.build_key {
        payload.insert("build_key".into(), json!(b));
    }
    if let Some(r) = &event.retagged {
        payload.insert("retagged".into(), json!(r));
    }
    if let Some(f) = event.first_push {
        payload.insert("first_push".into(), json!(f));
    }
    if let Some(e) = &event.error {
        payload.insert("error".into(), json!(e));
    }
    if let Some(c) = &event.error_code {
        payload.insert("error_code".into(), json!(c));
    }
    contract::envelope(Value::Object(payload))
}

/// `done` event: terminates the ndjson stream.
pub fn done_event(ok: bool, images: usize, pushed: usize, skipped: usize, failed: usize) -> Value {
    contract::envelope(json!({
        "event": "done",
        "ok": ok,
        "images": images,
        "pushed": pushed,
        "skipped": skipped,
        "failed": failed,
    }))
}

/// `done` document for `--output json` (single document at the end).
pub fn done_document(
    ok: bool,
    images: Vec<Value>,
    pushed: usize,
    skipped: usize,
    failed: usize,
) -> Value {
    contract::envelope(json!({
        "ok": ok,
        "images": images,
        "pushed": pushed,
        "skipped": skipped,
        "failed": failed,
    }))
}

/// Plan entry for `--list --output json`.
pub fn plan_entry(image: &PlanImage) -> Value {
    json!({
        "key": image.key,
        "config": image.config,
        "ref": image.ref_str,
        "tags": image.tags,
        "platforms": image.platforms,
    })
}
