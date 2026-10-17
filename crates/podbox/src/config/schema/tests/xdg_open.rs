//! `integration.xdg_open`: the bool-or-table shape and its scheme allowlist.

use super::super::*;
use crate::config::types::DEFAULT_ALLOWED_URI_SCHEMES;

fn base() -> String {
    r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#
    .to_string()
}

#[test]
fn bool_shorthand_leaves_the_default_allowlist() {
    let cfg = Config::parse(&format!("{}[integration]\nxdg_open = true\n", base())).unwrap();
    assert!(cfg.integration.xdg_open.enabled);
    // Empty list = the default allowlist.
    assert!(cfg.integration.xdg_open.allowed_schemes.is_empty());
    assert_eq!(
        cfg.integration.xdg_open.scheme_list(),
        DEFAULT_ALLOWED_URI_SCHEMES
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    );
}

/// The table form enables the integration unless it is explicitly turned
/// back off — naming schemes is the user asking for them to be opened.
#[test]
fn table_with_schemes_implies_enabled() {
    let cfg = Config::parse(&format!(
        "{}[integration]\nxdg_open = {{ allowed_schemes = [\"http\", \"smb\"] }}\n",
        base()
    ))
    .unwrap();
    assert!(cfg.integration.xdg_open.enabled);
    assert_eq!(
        cfg.integration.xdg_open.allowed_schemes,
        vec!["http".to_string(), "smb".to_string()]
    );

    let cfg = Config::parse(&format!(
        "{}[integration]\nxdg_open = {{ enabled = false, allowed_schemes = [\"smb\"] }}\n",
        base()
    ))
    .unwrap();
    assert!(!cfg.integration.xdg_open.enabled);
    assert_eq!(
        cfg.integration.xdg_open.allowed_schemes,
        vec!["smb".to_string()]
    );

    // `enabled` alone keeps the default allowlist.
    let cfg = Config::parse(&format!(
        "{}[integration]\nxdg_open = {{ enabled = true }}\n",
        base()
    ))
    .unwrap();
    assert!(cfg.integration.xdg_open.enabled);
    assert!(cfg.integration.xdg_open.allowed_schemes.is_empty());
}

#[test]
fn unknown_table_keys_are_parse_errors() {
    let err = Config::parse(&format!(
        "{}[integration]\nxdg_open = {{ allow = [\"http\"] }}\n",
        base()
    ))
    .unwrap_err();
    // Serde's unknown-field detail sits deeper in the anyhow chain than
    // Display reaches, so format it with Debug (which prints the chain).
    let chain = format!("{err:?}");
    assert!(
        chain.contains("enabled"),
        "error should name the expected keys: {chain}"
    );
}

#[test]
fn serialization_round_trips_the_written_shapes() {
    // Off + untouched list: the key stays out of written TOML entirely.
    let cfg = Config::parse(&base()).unwrap();
    let written = toml::to_string_pretty(&cfg.integration).unwrap();
    assert!(!written.contains("xdg_open"), "{written}");

    // On with the default list: back to the bool shorthand.
    let cfg = Config::parse(&format!("{}[integration]\nxdg_open = true\n", base())).unwrap();
    let written = toml::to_string_pretty(&cfg.integration).unwrap();
    assert!(
        written.contains("xdg_open = true\n"),
        "default-list shape should serialize as a bool: {written}"
    );

    // Custom list: the table form, both keys present.
    let cfg = Config::parse(&format!(
        "{}[integration]\nxdg_open = {{ allowed_schemes = [\"smb\"] }}\n",
        base()
    ))
    .unwrap();
    let written = toml::to_string_pretty(&cfg.integration).unwrap();
    assert!(
        written.contains("allowed_schemes = [\"smb\"]") && written.contains("enabled = true"),
        "{written}"
    );

    // Value-preserved round trip through the emitted text.
    let reparsed: crate::config::IntegrationConfig = toml::from_str(&written).unwrap();
    assert_eq!(reparsed.xdg_open, cfg.integration.xdg_open);
}

/// A scheme token outside the RFC 3986 lowercase grammar (or a typo like
/// `HTTPS`) is a config error, not a silently dead allowlist entry.
#[test]
fn invalid_scheme_tokens_are_validation_errors() {
    for bad in ["HTTPS", "file url", "", "/x", "file://x"] {
        let err = Config::parse(&format!(
            "{}[integration]\nxdg_open = {{ allowed_schemes = [\"http\", \"{}\"] }}\n",
            base(),
            bad.replace('\\', "\\\\").replace('"', "\\\"")
        ))
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("allowed_schemes[1]"),
            "expected a positional error for {bad:?}: {err}"
        );
    }
}
