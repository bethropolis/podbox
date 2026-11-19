//! Label construction for published images (section 7).
//!
//! Pure module: context + integration config -> label map.
//! Encodings match what [`crate::labels`] reads from prebuilt images.

use std::collections::{BTreeMap, HashSet};

use crate::config::IntegrationConfig;

/// OCI + podbox labels every published image carries.
#[derive(Debug, Clone, Default)]
pub struct Labels {
    pub map: BTreeMap<String, String>,
}

impl Labels {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn insert(mut self, k: &str, v: &str) -> Self {
        self.map.insert(k.into(), v.into());
        self
    }
}

/// Build all labels for a published image.
pub fn build_labels(
    context: &crate::publish::context::PublishContext,
    integration: &IntegrationConfig,
    explicit: &HashSet<String>,
    image_key: &str,
    build_key: &str,
) -> Labels {
    let mut map = BTreeMap::new();

    // OCI labels.
    if let Some(url) = &context.source_url {
        map.insert("org.opencontainers.image.source".into(), url.clone());
    }
    map.insert(
        "org.opencontainers.image.revision".into(),
        context.sha.clone(),
    );
    map.insert(
        "org.opencontainers.image.created".into(),
        context.created_rfc3339(),
    );
    map.insert("org.opencontainers.image.title".into(), image_key.into());

    // podbox.build-key.
    map.insert("podbox.build-key".into(), build_key.into());

    // podbox.integration.* / podbox.xdg_dirs.*: only explicitly-set keys, so
    // the labels and the build key hash the same inputs.
    for (key, value) in integration_label_pairs(integration, explicit) {
        map.insert(key, value);
    }

    Labels { map }
}

fn explicit_contains(explicit: &HashSet<String>, path: &str) -> bool {
    explicit.contains(path)
        || explicit
            .iter()
            .any(|key| key.starts_with(path) && key[path.len()..].starts_with('.'))
}

/// Integration/xdg labels the publisher set explicitly.
///
/// Sorted by key so the build key is deterministic. Shared with
/// [`crate::publish::build_key::compute_build_key`] callers.
pub fn integration_label_pairs(
    integration: &IntegrationConfig,
    explicit: &HashSet<String>,
) -> Vec<(String, String)> {
    let mut out = Vec::new();

    let bool_fields = [
        ("wayland", integration.wayland),
        ("audio", integration.audio),
        ("dbus", integration.dbus),
        ("notify", integration.notify),
        ("clipboard", integration.clipboard),
        ("sync_fonts", integration.sync_fonts),
        ("sync_icons", integration.sync_icons),
        ("sync_themes", integration.sync_themes),
    ];
    for (name, value) in bool_fields {
        if explicit_contains(explicit, &format!("integration.{name}")) {
            out.push((format!("podbox.integration.{name}"), value.to_string()));
        }
    }

    if explicit_contains(explicit, "integration.xdg_open") {
        out.push((
            "podbox.integration.xdg_open".to_string(),
            integration.xdg_open.enabled.to_string(),
        ));
    }

    let xdg_fields = [
        ("documents", integration.xdg_dirs.documents.is_enabled()),
        ("downloads", integration.xdg_dirs.downloads.is_enabled()),
        ("pictures", integration.xdg_dirs.pictures.is_enabled()),
        ("music", integration.xdg_dirs.music.is_enabled()),
        ("videos", integration.xdg_dirs.videos.is_enabled()),
        ("desktop", integration.xdg_dirs.desktop.is_enabled()),
        ("projects", integration.xdg_dirs.projects.is_enabled()),
    ];
    for (name, value) in xdg_fields {
        if explicit_contains(explicit, &format!("integration.xdg_dirs.{name}")) {
            out.push((format!("podbox.xdg_dirs.{name}"), value.to_string()));
        }
    }

    if explicit_contains(explicit, "integration.gpu") {
        out.push((
            "podbox.integration.gpu".to_string(),
            match integration.gpu {
                crate::config::GpuMode::Auto => "auto",
                crate::config::GpuMode::Enabled => "true",
                crate::config::GpuMode::Disabled => "false",
                crate::config::GpuMode::Nvidia => "nvidia",
            }
            .to_string(),
        ));
    }

    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ExportConfig, GpuMode, HardwareConfig, HostExecConfig, XdgDirValue};

    fn context() -> crate::publish::context::PublishContext {
        crate::publish::context::PublishContext {
            owner: "bethropolis".into(),
            repo: "repo".into(),
            sha: "abc1234567890".into(),
            ref_name: "main".into(),
            ref_type: crate::publish::context::RefType::Branch,
            event: "push".into(),
            default_branch: Some("main".into()),
            source_url: Some("https://github.com/bethropolis/repo".into()),
            now: std::time::SystemTime::UNIX_EPOCH,
        }
    }

    fn integration() -> IntegrationConfig {
        IntegrationConfig {
            git_identity: false,
            wayland: true,
            x11: false,
            audio: false,
            gpu: GpuMode::Nvidia,
            dbus: true,
            notify: false,
            xdg_open: crate::config::XdgOpenConfig {
                enabled: true,
                allowed_schemes: vec![],
            },
            clipboard: true,
            host_exec: HostExecConfig::default(),
            ssh_agent: false,
            gpg_agent: false,
            sync_fonts: false,
            sync_icons: true,
            sync_themes: false,
            hardware: HardwareConfig::default(),
            xdg_dirs: crate::config::XdgDirConfig {
                documents: XdgDirValue::Simple(true),
                downloads: XdgDirValue::Simple(false),
                pictures: XdgDirValue::Simple(true),
                music: XdgDirValue::Simple(false),
                videos: XdgDirValue::Simple(false),
                desktop: XdgDirValue::Simple(false),
                projects: XdgDirValue::Simple(true),
            },
            export: ExportConfig::default(),
        }
    }

    fn explicit_all() -> HashSet<String> {
        let mut set: HashSet<String> = [
            "wayland",
            "audio",
            "dbus",
            "notify",
            "clipboard",
            "sync_fonts",
            "sync_icons",
            "sync_themes",
        ]
        .into_iter()
        .map(|name| format!("integration.{name}"))
        .collect();
        set.insert("integration.xdg_open.enabled".into());
        for name in [
            "documents",
            "downloads",
            "pictures",
            "music",
            "videos",
            "desktop",
            "projects",
        ] {
            set.insert(format!("integration.xdg_dirs.{name}"));
        }
        set.insert("integration.gpu".into());
        set
    }

    #[test]
    fn builds_all_labels() {
        let labels = build_labels(
            &context(),
            &integration(),
            &explicit_all(),
            "dev",
            "sha256:deadbeef",
        );
        let m = &labels.map;
        assert_eq!(
            m.get("org.opencontainers.image.source"),
            Some(&"https://github.com/bethropolis/repo".into())
        );
        assert_eq!(
            m.get("org.opencontainers.image.revision"),
            Some(&"abc1234567890".into())
        );
        assert_eq!(
            m.get("org.opencontainers.image.created"),
            Some(&"1970-01-01T00:00:00Z".into())
        );
        assert_eq!(m.get("org.opencontainers.image.title"), Some(&"dev".into()));
        assert_eq!(m.get("podbox.build-key"), Some(&"sha256:deadbeef".into()));
        assert_eq!(m.get("podbox.integration.audio"), Some(&"false".into()));
        assert_eq!(m.get("podbox.integration.gpu"), Some(&"nvidia".into()));
        assert_eq!(m.get("podbox.xdg_dirs.downloads"), Some(&"false".into()));
    }

    #[test]
    fn only_explicit_integration_keys_are_labeled() {
        let labels = build_labels(&context(), &integration(), &HashSet::new(), "dev", "k");
        assert!(
            !labels
                .map
                .keys()
                .any(|k| k.starts_with("podbox.integration.") || k.starts_with("podbox.xdg_dirs."))
        );
        assert_eq!(
            labels.map.get("org.opencontainers.image.title"),
            Some(&"dev".into())
        );
    }

    #[test]
    fn labels_are_sorted() {
        let labels = build_labels(
            &context(),
            &integration(),
            &explicit_all(),
            "dev",
            "sha256:deadbeef",
        );
        let keys: Vec<&str> = labels.map.keys().map(|s| s.as_str()).collect();
        let mut sorted: Vec<&str> = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
    }
}
