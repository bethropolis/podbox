//! AST-level TOML deep-merge for `extends` inheritance.
//!
//! Merge strategy (from REVIEW §1.2):
//! - Table × Table → key-wise recurse (skipping `extends`)
//! - Array × Array → union with deduplication (preserve base order)
//! - else        → child overwrites base

/// Deep-merge `child` into `base` in place.
pub fn merge_toml_values(base: &mut toml::Value, child: toml::Value) {
    match (base, child) {
        (toml::Value::Table(base_map), toml::Value::Table(child_map)) => {
            for (k, child_v) in child_map {
                if k == "extends" {
                    continue;
                }
                match base_map.get_mut(&k) {
                    Some(base_v) => merge_toml_values(base_v, child_v),
                    None => {
                        base_map.insert(k, child_v);
                    }
                }
            }
        }
        (toml::Value::Array(base_arr), toml::Value::Array(child_arr)) => {
            for item in child_arr {
                if !base_arr.contains(&item) {
                    base_arr.push(item);
                }
            }
        }
        (base_slot, child_v) => {
            *base_slot = child_v;
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
}
