//! Plan construction: Manifest + Context -> Plan (section 5).
//!
//! Pure module. For each selected image: ref, tags, platforms.

use crate::error::PodboxError;
use crate::publish::context::PublishContext;
use crate::publish::manifest::ValidatedManifest;
use crate::publish::naming::derive_ref;
use crate::publish::tags::{TagContext, expand_tags, parse_tags};

/// One image in the publish plan.
#[derive(Debug, Clone)]
pub struct PlanImage {
    pub key: String,
    pub config: String,    // relative path under --dir
    pub ref_str: String,   // full ref without tag
    pub tags: Vec<String>, // expanded tags
    pub platforms: Vec<String>,
    pub lone_podbox_toml: bool,
}

/// The full publish plan.
#[derive(Debug, Clone)]
pub struct Plan {
    pub images: Vec<PlanImage>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.images.is_empty()
    }
}

/// Build a plan from a validated manifest and context.
///
/// `selected` keys filter the images; empty means all. Unknown keys are an
/// error (image_missing).
pub fn build_plan(
    manifest: &ValidatedManifest,
    context: &PublishContext,
    selected: &[String],
    is_lone_podbox_toml: bool,
) -> Result<Plan, PodboxError> {
    // Validate all selected keys exist.
    for key in selected {
        if manifest.image(key).is_none() {
            return Err(PodboxError::ImageMissing { key: key.clone() });
        }
    }

    let keys: Vec<&str> = if selected.is_empty() {
        manifest.images.keys().map(|k| k.as_str()).collect()
    } else {
        selected.iter().map(|s| s.as_str()).collect()
    };

    let mut plan_images = Vec::new();
    for key in keys {
        let entry = manifest.image(key).unwrap();
        let ref_str = derive_ref(
            &manifest.registry,
            &context.owner,
            &context.repo,
            key,
            entry.name.as_deref(),
            is_lone_podbox_toml,
        )?;
        // Tags: image-level replaces top-level.
        let tags = if entry.tags.is_empty() {
            &manifest.tags
        } else {
            &entry.tags
        };
        let tokens = parse_tags(tags)?;
        let tag_context = TagContext {
            ref_type: match context.ref_type {
                crate::publish::context::RefType::Branch => crate::publish::tags::RefType::Branch,
                crate::publish::context::RefType::Tag => crate::publish::tags::RefType::Tag,
            },
            ref_name: context.ref_name.clone(),
            sha7: context.sha7(),
            date_utc: context.date_utc(),
            default_branch: context.default_branch.clone(),
        };
        let expanded = expand_tags(&tokens, &tag_context)?;
        if expanded.is_empty() {
            return Err(PodboxError::NoTags {
                key: key.to_string(),
                context: format!("{}/{}", context.owner, context.repo),
            });
        }

        // Platforms: image-level replaces top-level.
        let platforms = if entry.platforms.is_empty() {
            manifest.platforms.clone()
        } else {
            entry.platforms.clone()
        };

        plan_images.push(PlanImage {
            key: key.to_string(),
            config: entry.config.clone(),
            ref_str,
            tags: expanded,
            platforms,
            lone_podbox_toml: is_lone_podbox_toml,
        });
    }

    Ok(Plan {
        images: plan_images,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::context::RefType;
    use crate::publish::manifest::ImageEntry;

    fn context() -> PublishContext {
        PublishContext {
            owner: "bethropolis".into(),
            repo: "repo".into(),
            sha: "abc1234567890".into(),
            ref_name: "main".into(),
            ref_type: RefType::Branch,
            event: "push".into(),
            default_branch: Some("main".into()),
            source_url: None,
            now: std::time::SystemTime::now(),
        }
    }

    fn manifest() -> ValidatedManifest {
        ValidatedManifest {
            registry: "ghcr.io".into(),
            platforms: vec!["linux/amd64".into()],
            tags: vec!["latest".to_string(), "sha".to_string()],
            images: std::collections::BTreeMap::from([
                (
                    "dev".into(),
                    ImageEntry {
                        config: "envs/dev.toml".into(),
                        tags: Vec::new(),
                        platforms: Vec::new(),
                        name: None,
                    },
                ),
                (
                    "gaming".into(),
                    ImageEntry {
                        config: "envs/gaming.toml".into(),
                        tags: vec!["latest".to_string(), "nightly".to_string()],
                        platforms: Vec::new(),
                        name: Some("ghcr.io/me/gaming".into()),
                    },
                ),
            ]),
        }
    }

    #[test]
    fn plan_all_images() {
        let plan = build_plan(&manifest(), &context(), &[], false).unwrap();
        assert_eq!(plan.images.len(), 2);
        assert_eq!(plan.images[0].key, "dev");
        assert_eq!(plan.images[0].ref_str, "ghcr.io/bethropolis/repo/dev");
        assert_eq!(plan.images[1].key, "gaming");
        assert_eq!(plan.images[1].ref_str, "ghcr.io/me/gaming");
    }

    #[test]
    fn plan_selected_keys() {
        let plan = build_plan(&manifest(), &context(), &["gaming".to_string()], false).unwrap();
        assert_eq!(plan.images.len(), 1);
        assert_eq!(plan.images[0].key, "gaming");
    }

    #[test]
    fn plan_unknown_key_errors() {
        let err = build_plan(&manifest(), &context(), &["nope".to_string()], false).unwrap_err();
        assert!(matches!(err, PodboxError::ImageMissing { .. }));
    }

    #[test]
    fn plan_image_level_tags_replace() {
        let plan = build_plan(&manifest(), &context(), &[], false).unwrap();
        let gaming = plan.images.iter().find(|i| i.key == "gaming").unwrap();
        assert_eq!(
            gaming.tags,
            vec!["latest".to_string(), "nightly".to_string()]
        );
    }

    #[test]
    fn plan_tag_expansion_default_branch() {
        let plan = build_plan(&manifest(), &context(), &[], false).unwrap();
        let dev = plan.images.iter().find(|i| i.key == "dev").unwrap();
        // latest (default branch) + sha
        assert!(dev.tags.contains(&"latest".to_string()));
        assert!(dev.tags.iter().any(|t| t.starts_with("sha-")));
    }
}
