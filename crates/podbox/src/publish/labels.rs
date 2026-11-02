//! Label construction for published images (section 7).
//!
//! Pure module: context + integration config -> label map.
//! Encodings match what [`crate::labels`] reads from prebuilt images.

use std::collections::BTreeMap;

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

    pub fn insert(mut self, k: &str, v: &str) -> Self {
        self.map.insert(k.into(), v.into());
        self
    }
}

/// Build all labels for a published image.
pub fn build_labels(
    context: &crate::publish::context::PublishContext,
    integration: &IntegrationConfig,
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
        context
            .now
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string(),
    );
    map.insert(
        "org.opencontainers.image.title".into(),
        integration_key(integration),
    );

    // podbox.build-key.
    map.insert("podbox.build-key".into(), build_key.into());

    // podbox.integration.*: only explicitly-set keys.
    add_integration_labels(&mut map, integration);

    Labels { map }
}

/// Title label: image key (best effort from integration config).
fn integration_key(_integration: &IntegrationConfig) -> String {
    "podbox-image".into()
}

/// Emit `podbox.integration.*` for keys the publisher set explicitly.
///
/// Matches [`crate::labels::apply_defaults`] precedence: explicit definition
/// value beats image label beats built-in default. We only emit what the
/// publisher chose, never defaults that might change later.
fn add_integration_labels(map: &mut BTreeMap<String, String>, integration: &IntegrationConfig) {
    // Bool fields: emit when explicitly set (true or false).
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
        map.insert(format!("podbox.integration.{name}"), value.to_string());
    }

    // xdg_open: emit enabled state.
    map.insert(
        "podbox.integration.xdg_open".into(),
        integration.xdg_open.enabled.to_string(),
    );

    // XDG dirs: emit enabled state per dir.
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
        map.insert(format!("podbox.xdg_dirs.{name}"), value.to_string());
    }

    // GPU: emit as string.
    map.insert(
        "podbox.integration.gpu".into(),
        match integration.gpu {
            crate::config::GpuMode::Auto => "auto".into(),
            crate::config::GpuMode::Enabled => "true".into(),
            crate::config::GpuMode::Disabled => "false".into(),
            crate::config::GpuMode::Nvidia => "nvidia".into(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GpuMode, XdgDirValue};

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
            host_exec: Default::default(),
            ssh_agent: false,
            gpg_agent: false,
            sync_fonts: false,
            sync_icons: true,
            sync_themes: false,
            hardware: Default::default(),
            xdg_dirs: crate::config::XdgDirConfig {
                documents: XdgDirValue::Simple(true),
                downloads: XdgDirValue::Simple(false),
                pictures: XdgDirValue::Simple(true),
                music: XdgDirValue::Simple(false),
                videos: XdgDirValue::Simple(false),
                desktop: XdgDirValue::Simple(false),
                projects: XdgDirValue::Simple(true),
            },
            export: Default::default(),
        }
    }

    #[test]
    fn builds_all_labels() {
        let labels = build_labels(&context(), &integration(), "sha256:deadbeef");
        let m = &labels.map;
        assert_eq!(
            m.get("org.opencontainers.image.source"),
            Some(&"https://github.com/bethropolis/repo".into())
        );
        assert_eq!(
            m.get("org.opencontainers.image.revision"),
            Some(&"abc1234567890".into())
        );
        assert_eq!(m.get("podbox.build-key"), Some(&"sha256:deadbeef".into()));
        assert_eq!(m.get("podbox.integration.audio"), Some(&"false".into()));
        assert_eq!(m.get("podbox.integration.gpu"), Some(&"nvidia".into()));
        assert_eq!(m.get("podbox.xdg_dirs.downloads"), Some(&"false".into()));
    }

    #[test]
    fn labels_are_sorted() {
        let labels = build_labels(&context(), &integration(), "sha256:deadbeef");
        let keys: Vec<&str> = labels.map.keys().map(|s| s.as_str()).collect();
        let mut sorted: Vec<&str> = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }
}
