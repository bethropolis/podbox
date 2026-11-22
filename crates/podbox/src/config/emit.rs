//! Canonical TOML emission for `Config`.
//!
//! One place both `podbox init` and the wasm Studio bindings serialize a
//! resolved config through: `skip_serializing_if` on defaults keeps the
//! written file small, so callers emit without re-implementing
//! serialization by hand (and the Studio output matches the CLI's).
//!
//! serde's derive skips defaulted *values* but still prints a section that
//! ends up empty; the post-pass below finishes the job (drop sections that
//! hold nothing after default-skipping) and compacts named map entries back
//! under their parent table (`projects = { … }`, `git = { … }`, or
//! `web = { … }`) so they read as part of the map, not as sections of their
//! own.
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
    inline_config_maps(doc.as_table_mut());
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

/// Named map entries serialize as child tables by default; compact them under
/// their parent so every map of user-named entries reads the same way:
///
/// ```toml
/// [container.services]
/// web = { command = "…", restart = "always" }
///
/// [integration.host_exec.allowlist]
/// git = { path = "/usr/bin/git", filter = true, shim = true }
///
/// [integration.xdg_dirs]
/// projects = { enabled = true, read_write = true }
/// ```
fn inline_config_maps(root: &mut toml_edit::Table) {
    for path in [
        ["integration", "xdg_dirs"].as_slice(),
        ["integration", "host_exec", "allowlist"].as_slice(),
        ["container", "services"].as_slice(),
    ] {
        if let Some(map) = table_at_mut(root, path) {
            inline_value_tables(map);
        }
    }
}

/// Walk a table path, returning the final table when every segment exists.
fn table_at_mut<'a>(
    root: &'a mut toml_edit::Table,
    path: &[&str],
) -> Option<&'a mut toml_edit::Table> {
    let (last, parents) = path.split_last()?;
    let mut table = root;
    for key in parents {
        table = table.get_mut(key)?.as_table_mut()?;
    }
    table.get_mut(last)?.as_table_mut()
}

/// Replace each child table in `table` with an inline table when its body can
/// be represented inline. Entries that are already plain values (the short
/// service/host-exec forms) stay untouched.
fn inline_value_tables(table: &mut toml_edit::Table) {
    let keys: Vec<String> = table
        .iter()
        .filter(|(_, item)| item.is_table())
        .map(|(key, _)| key.to_owned())
        .collect();
    for key in keys {
        let Some(inline) = table
            .get(&key)
            .and_then(|item| item.as_table())
            .and_then(inline_if_values)
        else {
            continue;
        };
        table.remove(&key);
        table.insert(&key, toml_edit::value(inline));
    }
}

/// A table becomes an inline table when every entry is a value or a table
/// that can itself inline (a service `env`, for example). Arrays of tables
/// and other exotic items keep the entry as a section.
fn inline_if_values(t: &toml_edit::Table) -> Option<toml_edit::InlineTable> {
    if t.is_empty() {
        return None;
    }
    let mut entries: Vec<(toml_edit::Key, toml_edit::Value)> = Vec::new();
    for (key, entry) in t {
        let value = match entry {
            toml_edit::Item::Value(v) => v.clone(),
            toml_edit::Item::Table(child) => {
                toml_edit::Value::InlineTable(inline_if_values(child)?)
            }
            _ => return None,
        };
        entries.push((toml_edit::Key::new(key), value));
    }
    let mut inline = toml_edit::InlineTable::default();
    let last = entries.len().saturating_sub(1);
    for (idx, (key, mut value)) in entries.into_iter().enumerate() {
        if idx == last {
            // The plain `toml_edit` rendering glues the closing brace to the
            // final value; a suffix on the last entry gives the symmetric
            // `{ … }` the rest of the config examples use.
            value.decor_mut().set_suffix(" ");
        }
        inline.insert(key, value);
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
    fn emits_detailed_service_inline() {
        // `env` is empty and stays out; the service itself sits under its
        // parent map instead of a `[container.services.web]` section.
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
        assert!(out.contains("[container.services]"), "{out}");
        assert!(
            out.contains("web = { command = \"redis-server\", restart = \"always\" }"),
            "{out}"
        );
        assert!(!out.contains("[container.services.web]"), "{out}");
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
    fn emits_service_env_inline() {
        // A nested map (service env) becomes a nested inline table rather
        // than forcing the whole service back to a section.
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

[container.services.web.env]
LOG_LEVEL = "debug"
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        assert!(
            out.contains("web = { command = \"redis-server\", env = { LOG_LEVEL = \"debug\" } }"),
            "{out}"
        );
        assert!(!out.contains("[container.services.web]"), "{out}");
        let reparsed = Config::parse(&out).unwrap();
        match &reparsed.container.services["web"] {
            crate::config::ServiceConfig::Detailed(detail) => {
                assert_eq!(
                    detail.env.get("LOG_LEVEL").map(String::as_str),
                    Some("debug")
                );
            }
            other => panic!("expected detailed service, got {other:?}"),
        }
    }

    #[test]
    fn host_exec_allowlist_entries_roundtrip_inline() {
        // Short entries stay strings; detailed entries sit beside them as
        // inline tables under `[integration.host_exec.allowlist]`.
        let cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"

[integration.host_exec]
enabled = true

[integration.host_exec.allowlist]
gh = "/usr/bin/gh"
git = { path = "/usr/bin/git", filter = true, shim = true }
scrcpy = { path = "/usr/bin/scrcpy", timeout = 30 }
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        assert!(out.contains("[integration.host_exec.allowlist]"), "{out}");
        assert!(out.contains("gh = \"/usr/bin/gh\""), "{out}");
        assert!(
            out.contains("git = { path = \"/usr/bin/git\", filter = true, shim = true }"),
            "{out}"
        );
        assert!(
            out.contains(
                "scrcpy = { path = \"/usr/bin/scrcpy\", filter = false, shim = true, timeout = 30 }"
            ),
            "{out}"
        );
        assert!(
            !out.contains("[integration.host_exec.allowlist."),
            "entries must stay under the parent table: {out}"
        );
        let reparsed = Config::parse(&out).unwrap();
        let gh = reparsed.integration.host_exec.resolve("gh").unwrap();
        assert!(matches!(gh, crate::config::HostExecEntry::Simple(_)));
        let git = reparsed.integration.host_exec.resolve("git").unwrap();
        assert!(git.filter_enabled());
        assert!(git.shim_enabled());
        let scrcpy = reparsed.integration.host_exec.resolve("scrcpy").unwrap();
        assert_eq!(scrcpy.timeout_secs(), Some(30));
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
    fn emits_sections_in_reading_order() {
        // The written file reads like the setup itself: image, container,
        // dotfiles, provision, storage, network, integration, dbus, wayland,
        // security, lifecycle, systemd. Anything else lands sections where
        // the reader has to jump back and forth ([[provision]] at the very
        // bottom used to sit after tables it conceptually precedes).
        let cfg = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"

[container]
name = "env"
home = "~/env"

[image.packages]
install = ["git"]

[container.mounts]
extra = ["~/work:~/work:z"]

[integration]
wayland = true
dbus = true

[integration.xdg_dirs]
projects = { enabled = true, read_write = true }

[dbus]
talk = ["org.example.Service"]

[wayland]
firewall = false

[security]
read_only_rootfs = true

[lifecycle]
quadlet = true

[systemd]
after = ["network-online.target"]

[storage.shared_caches]
cargo = true

[network]
mode = "host"

[[provision]]
name = "setup"
run = "true"
"#,
        )
        .unwrap();
        let out = toml_string(&cfg).unwrap();
        let order = |marker: &str| out.find(marker).unwrap_or(usize::MAX);
        for (earlier, later) in [
            ("[image.packages]", "[container.mounts]"),
            ("[container.mounts]", "[[provision]]"),
            ("[[provision]]", "[storage.shared_caches]"),
            ("[storage.shared_caches]", "[network]"),
            ("[network]", "[integration]"),
            ("[integration.xdg_dirs]", "[dbus]"),
            ("[security]", "[lifecycle]"),
            ("[lifecycle]", "[systemd]"),
        ] {
            assert!(
                order(earlier) < order(later),
                "{earlier} must print before {later}:\n{out}"
            );
        }
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
