use serde::{Deserialize, Serialize};

/// Schemes the host opens with `xdg-open` when the config names none.
///
/// An allowlist, not a blocklist: a blocklist misses every handler the host
/// has installed (`smb:`, `vscode:`, `steam:`, `trash:`, ...), and each one
/// is a way for guest input to reach a host app or a network share.
pub const DEFAULT_ALLOWED_URI_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// Shorthand tokens a scheme name may be made of (`RFC 3986` scheme).
///
/// Only lowercase tokens are accepted: the wire comparison is
/// case-sensitive against a normalized list, so `"HTTPS"` would silently
/// refuse https traffic a user believes they enabled.
pub(crate) fn is_scheme_token(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '+' || c == '.')
}

/// `integration.xdg_open`: the bare `bool` shorthand, or a table naming the
/// schemes the host may open.
///
/// ```toml
/// [integration]
/// xdg_open = true
/// # or, relaxing the scheme allowlist:
/// xdg_open = { allowed_schemes = ["http", "https", "mailto", "smb"] }
/// ```
///
/// The table form implies `enabled = true` unless it is set explicitly:
/// a user naming schemes is asking to open something. Every extra scheme
/// reachable through `xdg-open` exposes another host handler to guest
/// input, so the list is a deliberate, documented risk
/// (`docs/guest.md` / `docs/config.md`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct XdgOpenConfig {
    pub enabled: bool,
    /// Empty means "the default allowlist"; validation rejects entries that
    /// are not lowercase scheme tokens.
    pub allowed_schemes: Vec<String>,
}

impl XdgOpenConfig {
    /// The schemes a guest URI is allowed to use: the configured list, or
    /// the default when the config names none.
    pub fn scheme_list(&self) -> Vec<String> {
        if self.allowed_schemes.is_empty() {
            DEFAULT_ALLOWED_URI_SCHEMES
                .iter()
                .map(|s| (*s).to_string())
                .collect()
        } else {
            self.allowed_schemes.clone()
        }
    }

    fn rounds_to_bool(&self) -> bool {
        self.allowed_schemes.is_empty()
            || self
                .allowed_schemes
                .iter()
                .map(String::as_str)
                .eq(DEFAULT_ALLOWED_URI_SCHEMES.iter().copied())
    }
}

impl Serialize for XdgOpenConfig {
    /// Keep the common shapes readable: a plain `xdg_open = true` while the
    /// scheme list is the default, and a table only once it is customized
    /// (or explicitly disabled with a custom list).
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        if self.rounds_to_bool() {
            return serializer.serialize_bool(self.enabled);
        }
        let mut map = serializer.serialize_map(Some(2))?;
        map.serialize_entry("enabled", &self.enabled)?;
        map.serialize_entry("allowed_schemes", &self.allowed_schemes)?;
        map.end()
    }
}

impl<'de> Deserialize<'de> for XdgOpenConfig {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(XdgOpenVisitor)
    }
}

struct XdgOpenVisitor;

impl<'de> serde::de::Visitor<'de> for XdgOpenVisitor {
    type Value = XdgOpenConfig;

    fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a boolean or { enabled = bool, allowed_schemes = [string, ...] }")
    }

    fn visit_bool<E>(self, v: bool) -> Result<Self::Value, E> {
        Ok(XdgOpenConfig {
            enabled: v,
            allowed_schemes: Vec::new(),
        })
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut enabled: Option<bool> = None;
        let mut schemes: Option<Vec<String>> = None;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "enabled" => enabled = Some(map.next_value()?),
                "allowed_schemes" => schemes = Some(map.next_value()?),
                other => {
                    return Err(serde::de::Error::unknown_field(
                        other,
                        &["enabled", "allowed_schemes"],
                    ));
                }
            }
        }
        Ok(XdgOpenConfig {
            // A table naming schemes means "open these", so the default
            // mirrors the bool shorthand's `true`.
            enabled: enabled.unwrap_or(true),
            allowed_schemes: schemes.unwrap_or_default(),
        })
    }
}

/// The `skip_serializing_if` hook for `integration.xdg_open`: write nothing
/// while the integration is off and the scheme list is untouched, so
/// default configs stay out of written TOML.
pub(super) fn is_default_xdg_open(v: &XdgOpenConfig) -> bool {
    !v.enabled && v.allowed_schemes.is_empty()
}
