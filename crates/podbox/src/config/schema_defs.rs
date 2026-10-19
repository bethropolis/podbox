//! Studio-side schema data, served from Rust.
//!
//! The Studio replica of the config schema used to hand-maintain field
//! defaults and enum variant lists. This module exposes both as pure data:
//! defaults are hand-built, per-field, and limited to paths where the
//! schema actually has a value — no placeholders, and required keys
//! (`image.base`, `container.name`) are absent by design, because there is
//! no `Default` on `Config`. The TOML emission half lives in
//! [`super::toml_string`]; the wasm bindings serve all three to the Studio,
//! where a missing default means "no default" instead of a copy-pasted one.
//!
//! Pure module: no host dependencies, compiles for wasm.

use serde_json::{Map, Value, json};
use strum::VariantNames;

use super::defaults;
use super::enums::{CapPreset, NetworkMode, OnStop, PackageManager, ServiceRestart, UsernsMode};
use super::types::{DbusPreset, DotfilesCloneOn, ProvisionOnFailure, SecretSource, SecretType};

/// Field defaults the Studio may prefill, keyed by dotted config path.
///
/// Like the `skip_serializing_if` rule that keeps these keys out of
/// written TOML, an absent entry means "no engine default" — the Studio
/// treats such a field as unset rather than inventing a value.
pub fn schema_defaults() -> Map<String, Value> {
    let mut out = Map::new();
    let mut put = |path: &str, value: Value| {
        out.insert(path.into(), value);
    };
    // [image]
    put("image.pull_retry", json!(defaults::default_pull_retry()));
    put(
        "image.pull_retry_delay",
        json!(defaults::default_pull_retry_delay()),
    );
    put("image.packages.install", json!([]));
    put("image.packages.remove", json!([]));
    put("image.run.commands", json!([]));
    // [container]
    put("container.shell", json!(defaults::default_shell()));
    put("container.slice", json!("podbox.slice"));
    put("container.cpu_weight", json!(200u16));
    // [dotfiles]
    put("dotfiles.target", json!(super::types::DOTFILES_TARGET));
    put("dotfiles.clone_on", json!("host"));
    // [security]
    put(
        "security.security_label_disable",
        json!(defaults::default_true()),
    );
    put(
        "security.no_new_privileges",
        json!(defaults::default_true()),
    );
    put("security.cap_preset", json!("default"));
    // [network]
    put(
        "network.mode",
        json!(defaults::default_network_mode().as_str()),
    );
    // [integration]
    put("integration.gpu", json!("auto"));
    // [lifecycle]
    put(
        "lifecycle.idle_timeout",
        json!(defaults::default_idle_timeout()),
    );
    put("lifecycle.on_stop", json!(OnStop::default()));
    // [wayland]
    // podbox enables the firewall by default, so the Studio prefills what
    // the engine keeps when the key is absent.
    put("wayland.firewall", json!(defaults::default_true()));
    // [dbus] — the effective default once `integration.dbus` is on is the
    // portal preset (`Config::apply_defaults` promotes `none` to it), so
    // the Studio prefills what the engine ends up using.
    put("dbus.preset", json!("portal"));
    // [[provision]]
    put("provision.timeout", json!("5m"));
    out
}

/// Enum variant lists for every config enum, keyed by the config path (or
/// accepted shorthand) the Studio edits.
///
/// Sourced from `strum::VariantNames`, so a variant added in Rust lands
/// here and the parity check below (and in the Studio) fails until its UI
/// metadata labels it.
pub fn schema_enums() -> Map<String, Value> {
    let mut out = Map::new();
    let mut put = |path: &str, variants: &[&str]| {
        out.insert(path.into(), json!(variants));
    };
    put("image.packages.manager", PackageManager::VARIANTS);
    put("dotfiles.clone_on", DotfilesCloneOn::VARIANTS);
    put("security.userns", UsernsMode::VARIANTS);
    put("security.cap_preset", CapPreset::VARIANTS);
    put("network.mode", NetworkMode::VARIANTS);
    put("integration.gpu", GpuModeTokens::VARIANTS);
    put("lifecycle.on_stop", OnStop::VARIANTS);
    put("dbus.preset", DbusPreset::VARIANTS);
    put("provision.on_failure", ProvisionOnFailure::VARIANTS);
    put("security.secrets.type", SecretType::VARIANTS);
    put("security.secrets.source", SecretSource::VARIANTS);
    put("container.services.restart", ServiceRestart::VARIANTS);
    // The `[storage.*_caches]` tick-lists: cache names in declaration
    // order, straight from `BUILTIN_CACHES`.
    let caches = crate::config::types::builtin_cache_list();
    put(
        "storage.shared_caches",
        caches.collect::<Vec<_>>().as_slice(),
    );
    put(
        "storage.host_caches",
        crate::config::types::builtin_cache_list()
            .collect::<Vec<_>>()
            .as_slice(),
    );
    out
}

/// GpuMode's TOML spellings: `auto`/`nvidia` are variant names while
/// `true`/`false` are the boolean shorthand the reader normalizes, so the
/// plain variant-names derive would name value tokens (enabled/disabled)
/// the document never accepts.
struct GpuModeTokens;

impl strum::VariantNames for GpuModeTokens {
    const VARIANTS: &'static [&'static str] = &["auto", "true", "false", "nvidia"];
}
