use std::collections::HashMap;

use crate::config::{Config, GpuMode, IntegrationConfig, XdgDirConfig, XdgDirValue};

/// Raw OCI labels read from `podman inspect`.
pub type LabelMap = HashMap<String, String>;

/// Fetch the OCI labels for a local image tag.
pub fn fetch(image_ref: &str) -> anyhow::Result<LabelMap> {
    let output = std::process::Command::new("podman")
        .args(["inspect", "--format", "{{json .Labels}}", image_ref])
        .output()?;

    if !output.status.success() {
        return Ok(LabelMap::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let map: LabelMap = serde_json::from_str(stdout.trim()).unwrap_or_default();
    Ok(map)
}

/// Boolean integration flags an image label may fill in. The label key is
/// `podbox.integration.<name>` (compat: `podmgr.integration.<name>`); the
/// explicit path is the matching `integration.<name>` definition key.
const BOOL_FIELDS: &[&str] = &[
    "wayland",
    "audio",
    "dbus",
    "notify",
    "xdg_open",
    "clipboard",
    "sync_fonts",
    "sync_icons",
    "sync_themes",
];

/// XDG directory flags an image label may fill in, same key scheme under
/// `podbox.xdg_dirs.<name>`.
const XDG_FIELDS: &[&str] = &[
    "documents",
    "downloads",
    "pictures",
    "music",
    "videos",
    "desktop",
    "projects",
];

fn bool_field<'a>(integration: &'a mut IntegrationConfig, name: &str) -> Option<&'a mut bool> {
    Some(match name {
        "wayland" => &mut integration.wayland,
        "audio" => &mut integration.audio,
        "dbus" => &mut integration.dbus,
        "notify" => &mut integration.notify,
        "xdg_open" => &mut integration.xdg_open,
        "clipboard" => &mut integration.clipboard,
        "sync_fonts" => &mut integration.sync_fonts,
        "sync_icons" => &mut integration.sync_icons,
        "sync_themes" => &mut integration.sync_themes,
        _ => return None,
    })
}

fn xdg_field<'a>(dirs: &'a mut XdgDirConfig, name: &str) -> Option<&'a mut XdgDirValue> {
    Some(match name {
        "documents" => &mut dirs.documents,
        "downloads" => &mut dirs.downloads,
        "pictures" => &mut dirs.pictures,
        "music" => &mut dirs.music,
        "videos" => &mut dirs.videos,
        "desktop" => &mut dirs.desktop,
        "projects" => &mut dirs.projects,
        _ => return None,
    })
}

/// Look up a label under the new `podbox.*` key first, then the compat
/// `podmgr.*` key.
fn lookup<'a>(labels: &'a LabelMap, new_key: &str, old_key: &str) -> Option<&'a String> {
    labels.get(new_key).or_else(|| labels.get(old_key))
}

/// Apply podbox label defaults to a Config, with the user config winning
/// on every field that was explicitly set.
///
/// Precedence: an explicitly set definition value (including values from
/// `extends` profiles) beats an image label, which beats the built-in
/// default. Explicitness is tracked in [`Config::explicit_keys`].
///
/// Accepts both `podbox.*` (preferred) and `podmgr.*` (compat fallback)
/// label keys. New keys take precedence when both are present.
pub fn apply_defaults(config: &mut Config, labels: &LabelMap) {
    // Schema check: accept both new and old key
    match labels
        .get("podbox.schema")
        .or_else(|| labels.get("podmgr.schema"))
        .map(|s| s.as_str())
    {
        Some("1") => {}
        Some(v) => {
            eprintln!(
                "Warning: image declares podbox.schema={v}, host supports 1. \
                 Ignoring image labels."
            );
            return;
        }
        None => return,
    }

    let int = &mut config.integration;

    for name in BOOL_FIELDS {
        if config
            .explicit_keys
            .contains(&format!("integration.{name}"))
        {
            continue;
        }
        let new_key = format!("podbox.integration.{name}");
        let old_key = format!("podmgr.integration.{name}");
        if let Some(v) = lookup(labels, &new_key, &old_key)
            && let Some(field) = bool_field(int, name)
        {
            *field = v == "true";
        }
    }

    for name in XDG_FIELDS {
        if config
            .explicit_keys
            .contains(&format!("integration.xdg_dirs.{name}"))
        {
            continue;
        }
        let new_key = format!("podbox.xdg_dirs.{name}");
        let old_key = format!("podmgr.xdg_dirs.{name}");
        if let Some(v) = lookup(labels, &new_key, &old_key)
            && let Some(field) = xdg_field(&mut int.xdg_dirs, name)
            && v == "true"
        {
            *field = XdgDirValue::Simple(true);
        }
    }

    // GPU: accept both keys
    if !config.explicit_keys.contains("integration.gpu") {
        let gpu_key = if labels.contains_key("podbox.integration.gpu") {
            "podbox.integration.gpu"
        } else {
            "podmgr.integration.gpu"
        };
        if let Some(gpu_str) = labels.get(gpu_key) {
            if config.integration.gpu == GpuMode::Auto {
                config.integration.gpu = match gpu_str.as_str() {
                    "true" => GpuMode::Enabled,
                    "false" => GpuMode::Disabled,
                    "nvidia" => GpuMode::Nvidia,
                    _ => GpuMode::Auto,
                };
            }
        }
    }

    // Shell: accept both keys
    if !config.explicit_keys.contains("container.shell") {
        let shell_key = if labels.contains_key("podbox.default_shell") {
            "podbox.default_shell"
        } else {
            "podmgr.default_shell"
        };
        if let Some(shell) = labels.get(shell_key) {
            if config.container.shell == "bash" {
                config.container.shell.clone_from(shell);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    const BASE: &str = r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
"#;

    fn labels(pairs: &[(&str, &str)]) -> LabelMap {
        let mut map = LabelMap::from([("podbox.schema".into(), "1".into())]);
        for (k, v) in pairs {
            map.insert((*k).into(), (*v).into());
        }
        map
    }

    fn config_with(extra: &str) -> Config {
        Config::parse(&format!("{BASE}\n{extra}")).unwrap()
    }

    #[test]
    fn explicit_false_beats_label_true() {
        let mut cfg = config_with("[integration]\naudio = false\n");
        apply_defaults(&mut cfg, &labels(&[("podbox.integration.audio", "true")]));
        assert!(!cfg.integration.audio);
    }

    #[test]
    fn unset_field_takes_label_true() {
        let mut cfg = config_with("");
        apply_defaults(&mut cfg, &labels(&[("podbox.integration.audio", "true")]));
        assert!(cfg.integration.audio);
    }

    #[test]
    fn explicit_true_beats_label_false() {
        let mut cfg = config_with("[integration]\naudio = true\n");
        apply_defaults(&mut cfg, &labels(&[("podbox.integration.audio", "false")]));
        assert!(cfg.integration.audio);
    }

    #[test]
    fn explicit_xdg_false_beats_label_true() {
        let mut cfg = config_with("[integration.xdg_dirs]\ndownloads = false\n");
        apply_defaults(&mut cfg, &labels(&[("podbox.xdg_dirs.downloads", "true")]));
        assert!(!cfg.integration.xdg_dirs.downloads.is_enabled());
    }

    #[test]
    fn unset_xdg_takes_label_true() {
        let mut cfg = config_with("");
        apply_defaults(&mut cfg, &labels(&[("podbox.xdg_dirs.downloads", "true")]));
        assert!(cfg.integration.xdg_dirs.downloads.is_enabled());
    }

    #[test]
    fn explicit_shell_beats_label() {
        let mut cfg = Config::parse(
            r#"
[image]
base = "fedora:41"
name = "env"
[container]
name = "env"
home = "~/env"
shell = "bash"
"#,
        )
        .unwrap();
        apply_defaults(&mut cfg, &labels(&[("podbox.default_shell", "fish")]));
        assert_eq!(cfg.container.shell, "bash");
    }

    #[test]
    fn compat_label_keys_still_apply_when_unset() {
        let mut cfg = config_with("");
        apply_defaults(&mut cfg, &labels(&[("podmgr.integration.audio", "true")]));
        assert!(cfg.integration.audio);
    }
}
