//! Canonical TOML emission for `Config`.
//!
//! One place both `podbox init` and the wasm Studio bindings serialize a
//! resolved config through: `skip_serializing_if` on defaults keeps the
//! written file small, so callers emit without re-implementing
//! serialization by hand (and the Studio output matches the CLI's).
//!
//! serde's derive skips defaulted *values* but still prints a section that
//! ends up empty; the post-pass below finishes the job (drop sections that
//! hold nothing after default-skipping) and compacts detailed per-directory
//! `xdg_dirs` entries back under their parent table (`projects = { … }`
//! reads as part of the directory list, not as a section of its own).
//!
//! Pure module: no host dependencies, compiles for wasm.

use anyhow::Result;

use super::Config;

/// Serialize a config as pretty TOML.
///
/// Deterministic aside from the values themselves: field order is the
/// struct order, and defaults are skipped, so a config that came from a
/// profile or wizard lands in the same shape readers expect.
pub fn toml_string(config: &Config) -> Result<String> {
    // The document comes from our own serializer and cannot fail to parse;
    // the transformations only prune dead weight the issuer already wrote.
    let mut doc = toml::to_string_pretty(config)?.parse::<toml_edit::DocumentMut>()?;
    drop_defaulted_tables(doc.as_table_mut());
    inline_xdg_dirs(doc.as_table_mut());
    Ok(doc.to_string())
}

/// Recursively remove tables that end up without a single entry (every
/// value defaulted away). Their parse result is identical to the section's
/// absence, so removing them changes nothing about what podbox reads.
fn drop_defaulted_tables(table: &mut toml_edit::Table) {
    for (_, item) in table.iter_mut() {
        match item {
            toml_edit::Item::Table(t) => drop_defaulted_tables(t),
            toml_edit::Item::ArrayOfTables(aot) => {
                for t in aot.iter_mut() {
                    drop_defaulted_tables(t);
                }
            }
            _ => {}
        }
    }
    let dead: Vec<String> = table
        .iter()
        .filter(|(_, item)| is_default_empty(item))
        .map(|(key, _)| key.to_owned())
        .collect();
    for key in dead {
        table.remove(&key);
    }
}

/// An item that renders as an empty table (`[x]` with nothing under it, or
/// `x = {}`) — both parse to the same defaults as the key being absent.
fn is_default_empty(item: &toml_edit::Item) -> bool {
    match item {
        toml_edit::Item::Table(t) => t.is_empty(),
        toml_edit::Item::Value(toml_edit::Value::InlineTable(t)) => t.is_empty(),
        _ => false,
    }
}

/// Detailed per-directory entries under `[integration.xdg_dirs]` serialize
/// as child tables; hoisted under the parent (`projects = { … }`) they read
/// as part of the directory list. Only tables whose body is pure values
/// convert; anything nested keeps its header.
fn inline_xdg_dirs(root: &mut toml_edit::Table) {
    let Some(dirs) = root
        .get_mut("integration")
        .and_then(|item| item.as_table_mut())
        .and_then(|integration| integration.get_mut("xdg_dirs"))
        .and_then(|item| item.as_table_mut())
    else {
        return;
    };

    let keys: Vec<String> = dirs
        .iter()
        .filter(|(_, item)| item.is_table())
        .map(|(key, _)| key.to_owned())
        .collect();
    for key in keys {
        let Some(inline) = dirs
            .get(&key)
            .and_then(|item| item.as_table())
            .and_then(inline_if_values)
        else {
            continue;
        };
        dirs.remove(&key);
        dirs.insert(&key, toml_edit::value(inline));
    }
}

/// A table whose entries are all value items becomes an inline table;
/// anything holding a nested table returns `None`.
fn inline_if_values(t: &toml_edit::Table) -> Option<toml_edit::InlineTable> {
    if t.is_empty() {
        return None;
    }
    let mut entries: Vec<(toml_edit::Key, &toml_edit::Value)> = Vec::new();
    for (key, entry) in t {
        match entry {
            toml_edit::Item::Value(v) => entries.push((toml_edit::Key::new(key), v)),
            _ => return None,
        }
    }
    let mut inline = toml_edit::InlineTable::default();
    let last = entries.len().saturating_sub(1);
    for (idx, (key, v)) in entries.into_iter().enumerate() {
        if idx == last {
            // The plain `toml_edit` rendering glues the closing brace to the
            // final value; a suffix on the last entry gives the symmetric
            // `{ … }` the rest of the config examples use.
            let mut owned = v.clone();
            owned.decor_mut().set_suffix(" ");
            inline.insert(key, owned);
        } else {
            inline.insert(key, v.clone());
        }
    }
    Some(inline)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::enums::ServiceRestart;

    #[test]
    fn emits_untagged_short_form() {
        // The obsolete externally-tagged enum serialization wrote an
        // invalid `Short = "cmd"` key; the untagged form writes what
        // readers accept.
        let cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"
services.web = "redis-server"
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        assert!(out.contains("web = \"redis-server\""), "{out}");
        assert!(!out.contains("Short"), "{out}");
        let reparsed = Config::parse(&out).unwrap();
        assert_eq!(reparsed.container.services.len(), 1);
    }

    #[test]
    fn emits_detailed_service_without_empty_fields() {
        // `env` is empty and stays out; no empty `[container.services.web.env]`
        // table is written.
        let cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"

[container.services.web]
command = "redis-server"
restart = "always"
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        assert!(out.contains("[container.services.web]"), "{out}");
        assert!(out.contains("command = \"redis-server\""), "{out}");
        assert!(!out.contains("[container.services.web.env]"), "{out}");
        let reparsed = Config::parse(&out).unwrap();
        match &reparsed.container.services["web"] {
            crate::config::ServiceConfig::Detailed(detail) => {
                assert_eq!(detail.command, "redis-server");
                assert_eq!(detail.restart, ServiceRestart::Always);
            }
            other => panic!("expected detailed service, got {other:?}"),
        }
    }

    #[test]
    fn emits_no_defaulted_or_empty_sections() {
        // A config that is only `[container] integration.wayland` writes one
        // section: serde-defaulted sections (`[lifecycle]`, `[systemd]`,
        // `[network]`, `[dbus]`, `[wayland]`, `[security]`) are the same
        // config as their absence and must not print, and a section that
        // drops to empty because its defaults are skipped must not keep its
        // header either (the wayland frame emitted `[security]` with nothing
        // under it).
        let cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"

[integration]
wayland = true
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        assert!(out.contains("[integration]"), "{out}");
        assert!(out.contains("wayland = true"), "{out}");
        for empty in [
            "[container.env]",
            "[dotfiles",
            "[security]",
            "[systemd]",
            "[lifecycle]",
            "[network]",
            "[wayland]",
            "[storage]",
        ] {
            assert!(
                !out.contains(empty),
                "defaulted section `{empty}` written: {out}"
            );
        }
        // Roundtrip stability: the emitted shape reparses to the same config.
        let reparsed = Config::parse(&out).unwrap();
        assert!(reparsed.integration.wayland);
        assert_eq!(reparsed.container.name, cfg.container.name);
    }

    #[test]
    fn xdg_dirs_detailed_roundtrips_inline() {
        // The detailed per-dir table serializes compactly under its parent,
        // as `projects = { enabled = true, read_write = true }`; the emitted
        // file must contain the inline form and reparses identically.
        let cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"

[integration.xdg_dirs.projects]
enabled = true
read_write = true
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        assert!(
            out.contains("[integration.xdg_dirs]"),
            "parent table must hold the inline entry: {out}"
        );
        assert!(
            out.contains("projects = { enabled = true, read_write = true }"),
            "detailed xdg dir must print inline: {out}"
        );
        assert!(!out.contains("[integration.xdg_dirs.projects]"), "{out}");
        let reparsed = Config::parse(&out).unwrap();
        assert!(matches!(
            reparsed.integration.xdg_dirs.projects,
            crate::config::enums::XdgDirValue::Detailed {
                enabled: true,
                read_write: true
            }
        ));
    }
}
