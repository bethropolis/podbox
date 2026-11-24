use anyhow::Result;

use crate::config::Config;
use crate::error::PodboxError;
use crate::publish::registry::Registry;

/// Resolve base digest for a definition's FROM image.
pub(super) fn resolve_base_digest(
    registry: &dyn Registry,
    config: &Config,
) -> Result<String, PodboxError> {
    let base_ref = if config.image.source().is_prebuilt() {
        config
            .image
            .image_ref
            .as_deref()
            .unwrap_or(&config.image.base)
    } else {
        &config.image.base
    };
    // `Registry::inspect` takes a scheme-less reference; the backend adds
    // `docker://` itself.
    if let Some(img) = registry.inspect(base_ref)? {
        return Ok(img.digest);
    }
    if crate::podman::image_exists(base_ref).map_err(|_| PodboxError::BaseUnresolved {
        image: base_ref.into(),
    })? {
        return crate::podman::image_digest(base_ref).map_err(|_| PodboxError::BaseUnresolved {
            image: base_ref.into(),
        });
    }
    Err(PodboxError::BaseUnresolved {
        image: base_ref.into(),
    })
}

pub(super) enum Action {
    Build,
    Skip,
    #[allow(dead_code)]
    Retag {
        tags: Vec<String>,
    },
}

/// Probe decision: skip / retag / build.
pub(super) fn probe_decision(
    registry: &dyn Registry,
    ref_str: &str,
    probe_tag: Option<String>,
    build_key: &str,
) -> Result<(Action, Vec<String>), PodboxError> {
    let Some(probe) = probe_tag else {
        return Ok((Action::Build, Vec::new()));
    };
    let probe_ref = format!("{ref_str}:{probe}");

    if let Some(img) = registry.inspect(&probe_ref)? {
        let remote_key = img
            .labels
            .get("podbox.build-key")
            .cloned()
            .unwrap_or_default();
        if remote_key == build_key {
            // Up to date: the image at the probe tag is identical.
            // Conservatively treat all planned tags as present (skip).
            Ok((Action::Skip, Vec::new()))
        } else {
            Ok((Action::Build, Vec::new()))
        }
    } else {
        Ok((Action::Build, Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::registry::{RemoteImage, SkopeoRegistry};

    /// Registry stand-in: no network, no tools.
    struct FakeRegistry {
        reply: FakeReply,
    }

    enum FakeReply {
        Found {
            digest: &'static str,
            labels: &'static [(&'static str, &'static str)],
        },
        Missing,
        Failed,
    }

    impl Registry for FakeRegistry {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn inspect(&self, ref_str: &str) -> Result<Option<RemoteImage>, PodboxError> {
            match &self.reply {
                FakeReply::Found { digest, labels } => Ok(Some(RemoteImage {
                    ref_str: ref_str.into(),
                    manifest_json: String::new(),
                    digest: (*digest).into(),
                    labels: labels
                        .iter()
                        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
                        .collect(),
                })),
                FakeReply::Missing => Ok(None),
                FakeReply::Failed => Err(PodboxError::RegistryProbeFailed {
                    reference: ref_str.into(),
                    details: "skopeo inspect --raw: invalid reference format".into(),
                }),
            }
        }
    }

    fn config_with_base(base: &str) -> Config {
        Config::parse(&format!(
            "[image]\nbase = \"{base}\"\nname = \"env\"\n\
             [container]\nname = \"env\"\nhome = \"~/env\"\n"
        ))
        .expect("minimal config validates")
    }

    #[test]
    fn resolve_base_digest_uses_the_remote_digest() {
        let registry = FakeRegistry {
            reply: FakeReply::Found {
                digest: "sha256:b9284176",
                labels: &[],
            },
        };
        let config = config_with_base("alpine:latest");
        assert_eq!(
            resolve_base_digest(&registry, &config).unwrap(),
            "sha256:b9284176"
        );
    }

    #[test]
    fn resolve_base_digest_uses_image_ref_for_prebuilt_definitions() {
        let registry = FakeRegistry {
            reply: FakeReply::Found {
                digest: "sha256:prebuilt",
                labels: &[],
            },
        };
        let config = Config::parse(
            "[image]\nbase = \"fedora:41\"\nimage = \"ghcr.io/o/base:2026.1\"\nname = \"env\"\n\
             [container]\nname = \"env\"\nhome = \"~/env\"\n",
        )
        .expect("prebuilt config validates");
        assert_eq!(
            resolve_base_digest(&registry, &config).unwrap(),
            "sha256:prebuilt"
        );
    }

    #[test]
    fn resolve_base_digest_propagates_probe_failures() {
        let registry = FakeRegistry {
            reply: FakeReply::Failed,
        };
        let config = config_with_base("alpine:latest");
        let err = resolve_base_digest(&registry, &config).expect_err("probe must fail");
        assert!(matches!(err, PodboxError::RegistryProbeFailed { .. }));
    }

    #[test]
    fn missing_remote_image_falls_back_to_local_and_reports_base_unresolved() {
        // `.invalid` is reserved by RFC 2606, so neither the registry nor a
        // local image can resolve it regardless of whether podman is installed.
        let registry = FakeRegistry {
            reply: FakeReply::Missing,
        };
        let config = config_with_base("podbox-test.invalid/never/base:test");
        let err = resolve_base_digest(&registry, &config).expect_err("nothing can resolve this");
        assert!(matches!(err, PodboxError::BaseUnresolved { .. }));
    }

    #[test]
    fn probe_skips_when_the_remote_build_key_matches() {
        let registry = FakeRegistry {
            reply: FakeReply::Found {
                digest: "sha256:abc",
                labels: &[("podbox.build-key", "key-1")],
            },
        };
        let (action, _) = probe_decision(&registry, "ghcr.io/o/r", Some("latest".into()), "key-1")
            .expect("probe succeeds");
        assert!(matches!(action, Action::Skip));
    }

    #[test]
    fn probe_builds_when_the_remote_build_key_differs() {
        let registry = FakeRegistry {
            reply: FakeReply::Found {
                digest: "sha256:abc",
                labels: &[("podbox.build-key", "key-1")],
            },
        };
        let (action, _) = probe_decision(&registry, "ghcr.io/o/r", Some("latest".into()), "key-2")
            .expect("probe succeeds");
        assert!(matches!(action, Action::Build));
    }

    #[test]
    fn probe_builds_without_a_build_key_label() {
        let registry = FakeRegistry {
            reply: FakeReply::Found {
                digest: "sha256:abc",
                labels: &[],
            },
        };
        let (action, _) = probe_decision(&registry, "ghcr.io/o/r", Some("latest".into()), "key-1")
            .expect("probe succeeds");
        assert!(matches!(action, Action::Build));
    }

    #[test]
    fn probe_builds_when_the_image_is_missing_or_has_no_probe_tag() {
        let registry = FakeRegistry {
            reply: FakeReply::Missing,
        };
        let (action, _) = probe_decision(&registry, "ghcr.io/o/r", Some("latest".into()), "key-1")
            .expect("probe succeeds");
        assert!(matches!(action, Action::Build));

        let (action, _) =
            probe_decision(&registry, "ghcr.io/o/r", None, "key-1").expect("no tag means no probe");
        assert!(matches!(action, Action::Build));
    }

    #[test]
    #[ignore = "needs network access and skopeo"]
    fn digest_pinned_base_resolves_with_only_skopeo() {
        // `alpine@sha256:c2661275…` is the ref from the podbox-action fixture.
        let registry = SkopeoRegistry;
        let base = "alpine@sha256:c26612752c78b18faacd63c7d50f4592c2e7b0f25e0e20e82bd74e465c57ab0d";
        let img = registry
            .inspect(base)
            .expect("the probe itself must succeed")
            .expect("the image exists");
        assert!(!img.digest.is_empty());
        assert!(img.digest.starts_with("sha256:"));
    }

    #[test]
    #[ignore = "needs network access and skopeo"]
    fn tag_base_resolves_to_the_index_digest() {
        let registry = SkopeoRegistry;
        let img = registry
            .inspect("alpine:latest")
            .expect("the probe itself must succeed")
            .expect("the image exists");
        assert!(img.digest.starts_with("sha256:"));
    }
}
