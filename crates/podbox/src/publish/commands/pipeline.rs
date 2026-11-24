use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::Config;
use crate::contract;
use crate::publish::build_key::{compute_build_key, guest_hash};
use crate::publish::context::PublishContext;
use crate::publish::engine::Engine;
use crate::publish::events::{ImageEvent, step_event};
use crate::publish::labels::{build_labels, integration_label_pairs};
use crate::publish::plan::Plan;
use crate::publish::registry::Registry;

use super::artifacts::{emit_context, generate_containerfile};
use super::probe::{Action, probe_decision, resolve_base_digest};
use super::{OutputFormat, PublishOpts};

pub(super) struct PipelineInput<'a> {
    pub(super) plan: &'a Plan,
    pub(super) definitions: &'a BTreeMap<String, Config>,
    pub(super) containerfile_names: &'a BTreeMap<String, String>,
    pub(super) guest: &'static [u8],
    pub(super) context: &'a PublishContext,
    pub(super) opts: &'a PublishOpts,
    pub(super) registry: &'a dyn Registry,
    pub(super) engine: &'a dyn Engine,
    pub(super) output: OutputFormat,
}

pub(super) struct PublishSummary {
    pub(super) events: Vec<ImageEvent>,
    pub(super) failed: usize,
    pub(super) pushed: usize,
    pub(super) skipped: usize,
}

pub(super) fn execute(input: PipelineInput<'_>) -> Result<PublishSummary> {
    let PipelineInput {
        plan,
        definitions,
        containerfile_names,
        guest,
        context,
        opts,
        registry,
        engine,
        output,
    } = input;
    let dry_run = opts.dry_run;
    let push = opts.push && !opts.no_push;

    let mut events: Vec<ImageEvent> = Vec::new();
    let mut failed = 0usize;
    let mut pushed = 0usize;
    let mut skipped = 0usize;

    for (index, image) in plan.images.iter().enumerate() {
        let total = plan.images.len();
        if output == OutputFormat::Ndjson {
            contract::emit_event(step_event(
                index + 1,
                total,
                &format!("Resolving base image for '{}'", image.key),
            ));
        } else if output == OutputFormat::Text {
            eprintln!("{} / {}: {}", index + 1, total, image.key);
        }

        let config = definitions.get(&image.key).expect("validated definition");

        // Resolve base digest.
        let base_digest = resolve_base_digest(registry, config)?;

        // Compute build key.
        let containerfile = generate_containerfile(config)?;
        let guest_hash_str = guest_hash(guest);
        let integration_labels =
            integration_label_pairs(&config.integration, &config.explicit_keys);
        let build_key = compute_build_key(
            &containerfile,
            &guest_hash_str,
            crate::VERSION,
            &base_digest,
            &image.platforms,
            &integration_labels,
        );

        // Probe tag: first planned tag whose token is not sha or date.
        let probe_tag = image
            .tags
            .iter()
            .find(|t| {
                *t != "sha" && !t.starts_with("sha-") && *t != "date" && !t.starts_with("date")
            })
            .cloned();

        // Decide.
        let (action, retagged) = if opts.force {
            (Action::Build, Vec::new())
        } else {
            probe_decision(registry, &image.ref_str, probe_tag.clone(), &build_key)?
        };

        match action {
            Action::Skip => {
                skipped += 1;
                events.push(ImageEvent {
                    key: image.key.clone(),
                    ref_str: image.ref_str.clone(),
                    tags: image.tags.clone(),
                    pushed: false,
                    skipped: true,
                    digest: None,
                    build_key: Some(build_key.clone()),
                    retagged: Some(retagged),
                    first_push: None,
                    error: None,
                    error_code: None,
                });
                if output == OutputFormat::Text {
                    eprintln!("  skipped (up to date)");
                }
                continue;
            }
            Action::Retag { tags } => {
                skipped += 1;
                if push {
                    for tag in &tags {
                        let full_tag = format!("{}:{}", image.ref_str, tag);
                        engine
                            .push(image, &full_tag, Path::new(""))
                            .with_context(|| format!("failed to retag '{full_tag}'"))?;
                    }
                }
                events.push(ImageEvent {
                    key: image.key.clone(),
                    ref_str: image.ref_str.clone(),
                    tags: image.tags.clone(),
                    pushed: false,
                    skipped: true,
                    digest: None,
                    build_key: Some(build_key.clone()),
                    retagged: Some(tags),
                    first_push: None,
                    error: None,
                    error_code: None,
                });
                continue;
            }
            Action::Build => {}
        }

        // Build.
        if dry_run {
            eprintln!("  would: build '{}' -> {}", image.key, image.ref_str);
            events.push(ImageEvent {
                key: image.key.clone(),
                ref_str: image.ref_str.clone(),
                tags: image.tags.clone(),
                pushed: false,
                skipped: false,
                digest: None,
                build_key: Some(build_key),
                retagged: None,
                first_push: None,
                error: None,
                error_code: None,
            });
            continue;
        }

        // Emit context to temp dir. Keep the `TempDir` alive for the whole
        // build/push loop: dropping it deletes the Containerfile.
        let ctx = emit_context(&containerfile, &containerfile_names[&image.key])?;

        // Build every platform once, with all planned tags.
        let tags: Vec<String> = image
            .tags
            .iter()
            .map(|tag| format!("{}:{}", image.ref_str, tag))
            .collect();
        let labels = build_labels(
            context,
            &config.integration,
            &config.explicit_keys,
            &image.key,
            &build_key,
        );
        let mut image_failed = false;
        for platform in &image.platforms {
            if let Err(e) = engine.build(
                image,
                &tags,
                platform,
                ctx.dir.path(),
                &ctx.containerfile,
                &labels.map,
                push,
            ) {
                image_failed = true;
                failed += 1;
                events.push(ImageEvent {
                    key: image.key.clone(),
                    ref_str: image.ref_str.clone(),
                    tags: image.tags.clone(),
                    pushed: false,
                    skipped: false,
                    digest: None,
                    build_key: Some(build_key.clone()),
                    retagged: None,
                    first_push: None,
                    error: Some(e.to_string()),
                    error_code: Some("build_failed".into()),
                });
                break;
            }
        }

        if image_failed {
            continue;
        }

        // Push, then report the digest the registry now answers with.
        let mut pushed_digest: Option<String> = None;
        if push {
            for tag in &image.tags {
                let full_tag = format!("{}:{}", image.ref_str, tag);
                if let Err(e) = engine.push(image, &full_tag, ctx.dir.path()) {
                    failed += 1;
                    events.push(ImageEvent {
                        key: image.key.clone(),
                        ref_str: image.ref_str.clone(),
                        tags: image.tags.clone(),
                        pushed: false,
                        skipped: false,
                        digest: None,
                        build_key: Some(build_key.clone()),
                        retagged: None,
                        first_push: None,
                        error: Some(e.to_string()),
                        error_code: Some("push_failed".into()),
                    });
                    image_failed = true;
                    break;
                }
            }
            if !image_failed
                && let Some(first) = image.tags.first()
                && let Ok(Some(img)) = registry.inspect(&format!("{}:{}", image.ref_str, first))
            {
                pushed_digest = Some(img.digest);
            }
        }

        if image_failed {
            continue;
        }

        if push {
            pushed += 1;
        }
        events.push(ImageEvent {
            key: image.key.clone(),
            ref_str: image.ref_str.clone(),
            tags: image.tags.clone(),
            pushed: push,
            skipped: false,
            digest: pushed_digest,
            build_key: Some(build_key),
            retagged: None,
            first_push: None,
            error: None,
            error_code: None,
        });
    }

    Ok(PublishSummary {
        events,
        failed,
        pushed,
        skipped,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::error::PodboxError;
    use crate::publish::context::RefType;
    use crate::publish::plan::{Plan, PlanImage};
    use crate::publish::registry::RemoteImage;

    struct FakeRegistry {
        probe_key: Option<String>,
        pushed: bool,
    }

    impl Registry for FakeRegistry {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn inspect(&self, ref_str: &str) -> Result<Option<RemoteImage>, PodboxError> {
            if ref_str == "alpine:latest" {
                return Ok(Some(RemoteImage {
                    ref_str: ref_str.into(),
                    manifest_json: String::new(),
                    digest: "sha256:base".into(),
                    labels: BTreeMap::new(),
                }));
            }
            if ref_str == "ghcr.io/o/env:latest" {
                if let Some(key) = &self.probe_key {
                    return Ok(Some(RemoteImage {
                        ref_str: ref_str.into(),
                        manifest_json: String::new(),
                        digest: "sha256:existing".into(),
                        labels: BTreeMap::from([("podbox.build-key".into(), key.clone())]),
                    }));
                }
                if self.pushed {
                    return Ok(Some(RemoteImage {
                        ref_str: ref_str.into(),
                        manifest_json: String::new(),
                        digest: "sha256:pushed".into(),
                        labels: BTreeMap::new(),
                    }));
                }
            }
            Ok(None)
        }
    }

    struct FakeEngine {
        builds: AtomicUsize,
        pushes: AtomicUsize,
        fail_build: bool,
        fail_push: bool,
    }

    impl FakeEngine {
        fn new() -> Self {
            Self {
                builds: AtomicUsize::new(0),
                pushes: AtomicUsize::new(0),
                fail_build: false,
                fail_push: false,
            }
        }
    }

    impl Engine for FakeEngine {
        fn name(&self) -> &'static str {
            "fake"
        }

        fn build(
            &self,
            _image: &PlanImage,
            _tags: &[String],
            _platform: &str,
            _context_dir: &Path,
            _containerfile: &Path,
            _labels: &BTreeMap<String, String>,
            _push: bool,
        ) -> Result<(), PodboxError> {
            self.builds.fetch_add(1, Ordering::SeqCst);
            if self.fail_build {
                Err(PodboxError::PublishBuildFailed {
                    key: "env".into(),
                    details: "build exploded".into(),
                })
            } else {
                Ok(())
            }
        }

        fn push(
            &self,
            _image: &PlanImage,
            _tag: &str,
            _context_dir: &Path,
        ) -> Result<(), PodboxError> {
            self.pushes.fetch_add(1, Ordering::SeqCst);
            if self.fail_push {
                Err(PodboxError::PublishPushFailed {
                    key: "env".into(),
                    details: "push exploded".into(),
                })
            } else {
                Ok(())
            }
        }
    }

    fn config() -> Config {
        Config::parse(
            "[image]\nbase = \"alpine:latest\"\nname = \"env\"\n\
             [container]\nname = \"env\"\nhome = \"~/env\"\n",
        )
        .unwrap()
    }

    fn plan() -> Plan {
        Plan {
            images: vec![PlanImage {
                key: "env".into(),
                config: "env.toml".into(),
                ref_str: "ghcr.io/o/env".into(),
                tags: vec!["latest".into()],
                platforms: vec!["linux/amd64".into()],
                lone_podbox_toml: false,
            }],
        }
    }

    fn context() -> PublishContext {
        PublishContext {
            owner: "o".into(),
            repo: "repo".into(),
            sha: "abc123456789".into(),
            ref_name: "main".into(),
            ref_type: RefType::Branch,
            event: "push".into(),
            default_branch: Some("main".into()),
            source_url: None,
            now: std::time::SystemTime::now(),
        }
    }

    fn opts(dry_run: bool, force: bool, push: bool) -> PublishOpts {
        PublishOpts {
            keys: Vec::new(),
            manifest: None,
            dir: PathBuf::new(),
            registry: None,
            containerfile: None,
            push,
            no_push: false,
            force,
            list: false,
            dry_run,
            engine: None,
            ci: None,
            owner: None,
            repo: None,
            sha: None,
            ref_name: None,
            ref_type: None,
            event: None,
            default_branch: None,
            output: OutputFormat::Text,
        }
    }

    fn run(registry: &FakeRegistry, engine: &FakeEngine, opts: &PublishOpts) -> PublishSummary {
        let config = config();
        let plan = plan();
        let context = context();
        let definitions = BTreeMap::from([("env".to_string(), config)]);
        let containerfile_names =
            BTreeMap::from([("env".to_string(), "Containerfile".to_string())]);
        execute(PipelineInput {
            plan: &plan,
            definitions: &definitions,
            containerfile_names: &containerfile_names,
            guest: crate::guest::PODBOX_GUEST.unwrap(),
            context: &context,
            opts,
            registry,
            engine,
            output: opts.output,
        })
        .unwrap()
    }

    fn matching_build_key() -> String {
        let config = config();
        let containerfile = generate_containerfile(&config).unwrap();
        let labels = integration_label_pairs(&config.integration, &config.explicit_keys);
        compute_build_key(
            &containerfile,
            &guest_hash(crate::guest::PODBOX_GUEST.unwrap()),
            crate::VERSION,
            "sha256:base",
            &["linux/amd64".to_string()],
            &labels,
        )
    }

    #[test]
    fn dry_run_plans_a_build_without_calling_the_engine() {
        let registry = FakeRegistry {
            probe_key: None,
            pushed: false,
        };
        let engine = FakeEngine::new();
        let summary = run(&registry, &engine, &opts(true, true, false));
        assert_eq!(summary.events.len(), 1);
        assert!(!summary.events[0].skipped);
        assert_eq!(engine.builds.load(Ordering::SeqCst), 0);
        assert_eq!(engine.pushes.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn matching_remote_key_skips_the_build() {
        let registry = FakeRegistry {
            probe_key: Some(matching_build_key()),
            pushed: false,
        };
        let engine = FakeEngine::new();
        let summary = run(&registry, &engine, &opts(true, false, false));
        assert_eq!(summary.skipped, 1);
        assert!(summary.events[0].skipped);
        assert_eq!(engine.builds.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn build_failure_is_reported_in_the_summary() {
        let registry = FakeRegistry {
            probe_key: None,
            pushed: false,
        };
        let engine = FakeEngine {
            fail_build: true,
            ..FakeEngine::new()
        };
        let summary = run(&registry, &engine, &opts(false, true, false));
        assert_eq!(summary.failed, 1);
        assert_eq!(
            summary.events[0].error_code.as_deref(),
            Some("build_failed")
        );
    }

    #[test]
    fn push_failure_is_reported_in_the_summary() {
        let registry = FakeRegistry {
            probe_key: None,
            pushed: false,
        };
        let engine = FakeEngine {
            fail_push: true,
            ..FakeEngine::new()
        };
        let summary = run(&registry, &engine, &opts(false, true, true));
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.pushed, 0);
        assert_eq!(summary.events[0].error_code.as_deref(), Some("push_failed"));
    }

    #[test]
    fn successful_push_reports_the_registry_digest() {
        let registry = FakeRegistry {
            probe_key: None,
            pushed: true,
        };
        let engine = FakeEngine::new();
        let summary = run(&registry, &engine, &opts(false, true, true));
        assert_eq!(summary.failed, 0);
        assert_eq!(summary.pushed, 1);
        assert_eq!(summary.events[0].digest.as_deref(), Some("sha256:pushed"));
        assert_eq!(engine.pushes.load(Ordering::SeqCst), 1);
    }
}
