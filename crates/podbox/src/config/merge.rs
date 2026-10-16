//! AST-level TOML deep-merge for `extends` inheritance.
//!
//! Merge strategy:
//! - Table × Table → key-wise recurse (skipping `extends`)
//! - Array × Array → union with deduplication (preserve base order),
//!   except `image.run.commands`, which appends (order and repetition
//!   matter there). A `!name` entry in the child removes the parent
//!   entry instead of adding one.
//! - Arrays of `{ name = ... }` tables (`[[provision]]`) merge by name:
//!   same name replaces in the parent's position, new names append.
//! - else        → child overwrites base

/// Deep-merge `child` into `base` in place.
pub fn merge_toml_values(base: &mut toml::Value, child: toml::Value) {
    merge_at_path(base, child, String::new());
}

/// Recursive worker tracking the dotted path (`image.run.commands`) so
/// ordered recipe steps can merge differently from set-like lists.
fn merge_at_path(base: &mut toml::Value, child: toml::Value, path: String) {
    match (base, child) {
        (toml::Value::Table(base_map), toml::Value::Table(child_map)) => {
            for (k, child_v) in child_map {
                if k == "extends" {
                    continue;
                }
                let child_path = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match base_map.get_mut(&k) {
                    Some(base_v) => merge_at_path(base_v, child_v, child_path),
                    None => {
                        base_map.insert(k, child_v);
                    }
                }
            }
        }
        (toml::Value::Array(base_arr), toml::Value::Array(child_arr)) => {
            if arrays_are_named_tables(base_arr, &child_arr) {
                merge_named_tables(base_arr, child_arr);
            } else if path == "image.run.commands" {
                // Recipe steps, not a set: repetition is meaningful
                // (re-running a command twice can be intentional), so
                // append verbatim instead of unioning.
                base_arr.extend(child_arr);
            } else {
                // Set-like lists union with dedup, and a `!name` entry
                // removes the parent's entry (the `!` entry itself is
                // never added). Lets a child drop a parent's package,
                // port, or blocked interface.
                for item in &child_arr {
                    if let Some(removed) = item.as_str().and_then(|s| s.strip_prefix('!')) {
                        base_arr.retain(|b| b.as_str() != Some(removed));
                    }
                }
                for item in child_arr {
                    let is_removal = item.as_str().is_some_and(|s| s.starts_with('!'));
                    if !is_removal && !base_arr.contains(&item) {
                        base_arr.push(item);
                    }
                }
            }
        }
        (base_slot, child_v) => {
            *base_slot = child_v;
        }
    }
}

/// True when both arrays consist solely of tables carrying a string `name`.
/// This is the `[[provision]]` shape: keyed steps, not plain value lists.
fn arrays_are_named_tables(base_arr: &[toml::Value], child_arr: &[toml::Value]) -> bool {
    let all = base_arr.iter().chain(child_arr.iter());
    // Empty arrays never take the keyed path (nothing to key on).
    let mut any = false;
    for v in all {
        any = true;
        let is_named = v
            .as_table()
            .is_some_and(|t| t.get("name").is_some_and(|n| n.is_str()));
        if !is_named {
            return false;
        }
    }
    any
}

/// Keyed merge: child steps with a matching `name` replace the parent step
/// in the parent's position; new names append. Order is parent-first.
fn merge_named_tables(base_arr: &mut Vec<toml::Value>, child_arr: Vec<toml::Value>) {
    for item in child_arr {
        let child_name = item
            .as_table()
            .and_then(|t| t.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or_default()
            .to_string();
        let mut replaced = false;
        for base_item in base_arr.iter_mut() {
            let base_name = base_item
                .as_table()
                .and_then(|t| t.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or_default();
            if base_name == child_name {
                *base_item = item.clone();
                replaced = true;
                break;
            }
        }
        if !replaced {
            base_arr.push(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: &str) -> toml::Value {
        toml::from_str(s).unwrap()
    }

    #[test]
    fn scalar_child_overrides_parent() {
        let mut base = parse(r#"shell = "/usr/bin/fish""#);
        let child = parse(r#"shell = "/bin/zsh""#);
        merge_toml_values(&mut base, child);
        assert_eq!(base.get("shell").unwrap().as_str().unwrap(), "/bin/zsh");
    }

    #[test]
    fn array_union_dedup() {
        let mut base = parse(r#"install = ["git", "neovim"]"#);
        let child = parse(r#"install = ["rustup", "git"]"#);
        merge_toml_values(&mut base, child);
        let arr = base.get("install").unwrap().as_array().unwrap();
        let strs: Vec<_> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(strs, vec!["git", "neovim", "rustup"]);
    }

    #[test]
    fn map_keywise_merge() {
        let mut base = parse(
            r#"
            [env]
            A = "1"
            B = "2"
            "#,
        );
        let child = parse(
            r#"
            [env]
            B = "override"
            C = "3"
            "#,
        );
        merge_toml_values(&mut base, child);
        let env = base.get("env").unwrap().as_table().unwrap();
        assert_eq!(env.get("A").unwrap().as_str().unwrap(), "1");
        assert_eq!(env.get("B").unwrap().as_str().unwrap(), "override");
        assert_eq!(env.get("C").unwrap().as_str().unwrap(), "3");
    }

    #[test]
    fn extends_key_not_propagated() {
        let mut base = parse(r#"extends = "profile:dev""#);
        let child = parse(
            r#"
            extends = "profile:cachy"
            foo = "bar"
            "#,
        );
        merge_toml_values(&mut base, child);
        // base had extends, child extends should not overwrite / propagate
        assert_eq!(
            base.get("extends").unwrap().as_str().unwrap(),
            "profile:dev"
        );
        assert_eq!(base.get("foo").unwrap().as_str().unwrap(), "bar");
    }

    #[test]
    fn nested_table_merge() {
        let mut base = parse(
            r#"
            [container.mounts]
            extra = ["/a:/a:ro"]
            "#,
        );
        let child = parse(
            r#"
            [container.mounts]
            extra = ["/b:/b:ro", "/a:/a:ro"]
            "#,
        );
        merge_toml_values(&mut base, child);
        let extra = base
            .get("container")
            .unwrap()
            .get("mounts")
            .unwrap()
            .get("extra")
            .unwrap()
            .as_array()
            .unwrap();
        let strs: Vec<_> = extra.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(strs, vec!["/a:/a:ro", "/b:/b:ro"]);
    }

    #[test]
    fn named_tables_merge_in_place_then_append() {
        // `[[provision]]`: same name replaces in the parent's position,
        // new names append after it.
        let mut base = parse(
            r#"
            [[provision]]
            name = "a"
            run = "echo a1"

            [[provision]]
            name = "b"
            run = "echo b1"
            "#,
        );
        let child = parse(
            r#"
            [[provision]]
            name = "b"
            run = "echo b2"

            [[provision]]
            name = "c"
            run = "echo c"
            "#,
        );
        merge_toml_values(&mut base, child);
        let arr = base.get("provision").unwrap().as_array().unwrap();
        let rows: Vec<(&str, &str)> = arr
            .iter()
            .map(|v| {
                let t = v.as_table().unwrap();
                (
                    t.get("name").unwrap().as_str().unwrap(),
                    t.get("run").unwrap().as_str().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            vec![("a", "echo a1"), ("b", "echo b2"), ("c", "echo c"),]
        );
    }

    #[test]
    fn empty_arrays_still_union() {
        // The keyed path needs at least one named table; empty arrays fall
        // back to plain union, which is the existing behavior.
        let mut base = parse(r#"install = []"#);
        let child = parse(r#"install = ["git"]"#);
        merge_toml_values(&mut base, child);
        let arr = base.get("install").unwrap().as_array().unwrap();
        assert_eq!(arr.len(), 1);
    }

    #[test]
    fn removal_syntax_drops_parent_entries() {
        // The parent has `nano`, the child has `!nano` → result without
        // `nano`; `vim` unions in as usual and `!nano` itself is dropped.
        let mut base = parse(r#"install = ["git", "nano"]"#);
        let child = parse(r#"install = ["!nano", "vim"]"#);
        merge_toml_values(&mut base, child);
        let arr = base.get("install").unwrap().as_array().unwrap();
        let strs: Vec<_> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(strs, vec!["git", "vim"]);
    }

    #[test]
    fn removal_of_absent_entry_is_a_no_op() {
        let mut base = parse(r#"install = ["git"]"#);
        let child = parse(r#"install = ["!nano"]"#);
        merge_toml_values(&mut base, child);
        let arr = base.get("install").unwrap().as_array().unwrap();
        let strs: Vec<_> = arr.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(strs, vec!["git"]);
    }

    #[test]
    fn run_commands_append_without_dedup() {
        // Order and repetition matter for RUN steps: `[a, b]` + `[a]`
        // gives `[a, b, a]`, not the union `[a, b]`.
        let mut base = parse(
            r#"
            [image.run]
            commands = ["a", "b"]
            "#,
        );
        let child = parse(
            r#"
            [image.run]
            commands = ["a"]
            "#,
        );
        merge_toml_values(&mut base, child);
        let commands = base
            .get("image")
            .unwrap()
            .get("run")
            .unwrap()
            .get("commands")
            .unwrap()
            .as_array()
            .unwrap();
        let strs: Vec<_> = commands.iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(strs, vec!["a", "b", "a"]);
    }
}
