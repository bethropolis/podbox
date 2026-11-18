//! `podbox publish` orchestration.
//!
//! I/O shell: loads manifest and definitions, probes the registry, invokes
//! the build engine, renders events. Policy lives in the pure modules.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context as AnyhowContext, Result};
use serde_json::json;

use crate::config::Config;
use crate::contract;
use crate::error::PodboxError;
use crate::publish::build_context;
use crate::publish::build_key::{compute_build_key, guest_hash};
use crate::publish::context::{RefType, context_from_flags, context_from_github_env};
use crate::publish::discover::{Discovery, discover, list_dir};
use crate::publish::engine::select_engine;
use crate::publish::events::{
    ImageEvent, done_document, done_event, image_event, plan_entry, step_event,
};
use crate::publish::labels::build_labels;
use crate::publish::manifest::Manifest;
use crate::publish::plan::{Plan, build_plan};
use crate::publish::registry::{Registry, SkopeoRegistry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    Text,
    Json,
    Ndjson,
}

/// CLI options for `podbox publish`.
#[derive(Debug, Clone)]
pub struct PublishOpts {
    pub keys: Vec<String>,
    pub manifest: Option<PathBuf>,
    pub dir: PathBuf,
    pub registry: Option<String>,
    pub containerfile: Option<String>,
    pub push: bool,
    pub no_push: bool,
    pub force: bool,
    pub list: bool,
    pub dry_run: bool,
    pub engine: Option<String>,
    pub ci: Option<String>,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub sha: Option<String>,
    pub ref_name: Option<String>,
    pub ref_type: Option<String>,
    pub event: Option<String>,
    pub default_branch: Option<String>,
    pub output: OutputFormat,
}

/// Run `podbox publish`.
pub fn run(opts: PublishOpts) -> Result<()> {
    let dry_run = opts.dry_run;
    let push = opts.push && !opts.no_push;
    let output = opts.output;

    // Build the publish context.
    let now = SystemTime::now();
    let context = match opts.ci.as_deref() {
        Some("github") => context_from_github_env(now)?,
        _ => context_from_flags(
            opts.owner.clone(),
            opts.repo.clone(),
            opts.sha.clone(),
            opts.ref_name.clone(),
            opts.ref_type.as_deref().map(|s| match s {
                "tag" => RefType::Tag,
                _ => RefType::Branch,
            }),
            opts.event.clone(),
            opts.default_branch.clone(),
            None,
            now,
        )?,
    };

    // Load manifest: explicit --manifest or discovery.
    let manifest_path = opts
        .manifest
        .clone()
        .unwrap_or_else(|| opts.dir.join("images.toml"));
    let mut manifest = load_manifest(&manifest_path, opts.manifest.is_some(), &opts.dir)?;

    // Determine registry: flag > manifest > default.
    if let Some(registry) = opts.registry.as_deref() {
        registry.clone_into(&mut manifest.registry);
    }

    // Validate manifest against context.
    let validated = manifest
        .validate(&context)
        .map_err(|e| PodboxError::ManifestInvalid {
            details: e.to_string(),
        })?;

    // Load + validate every referenced definition up front (fail before any
    // registry/engine I/O).
    let mut definitions: BTreeMap<String, Config> = BTreeMap::new();
    for (key, entry) in &validated.images {
        if !opts.keys.is_empty() && !opts.keys.contains(key) {
            continue;
        }
        let config_path = opts.dir.join(&entry.config);
        let config =
            Config::load(&config_path).map_err(|e| PodboxError::PublishDefinitionLoad {
                key: key.clone(),
                details: e.to_string(),
            })?;
        definitions.insert(key.clone(), config);
    }

    // Build plan.
    let is_lone_podbox_toml = manifest_source_is_lone_podbox_toml(&opts, &manifest_path)?;
    let plan = build_plan(&validated, &context, &opts.keys, is_lone_podbox_toml)?;

    // Resolve each image's Containerfile name up front so an invalid name fails
    // before any registry or build I/O, including under `--list`.
    let mut containerfile_names: BTreeMap<String, String> = BTreeMap::new();
    for image in &plan.images {
        let entry = validated
            .image(&image.key)
            .expect("plan images come from the validated manifest");
        let name = build_context::resolve_containerfile_name(
            opts.containerfile.as_deref(),
            entry.containerfile.as_deref(),
            validated.containerfile.as_deref(),
        )?;
        containerfile_names.insert(image.key.clone(), name);
    }

    // --list: print plan only, no network, no build.
    if opts.list {
        return print_list(&plan, output);
    }

    // Select engine.
    let prefer_ci = opts.ci.as_deref() == Some("github");
    let engine: Box<dyn crate::publish::engine::Engine> =
        select_engine(prefer_ci, opts.engine.as_deref()).map_err(|e| anyhow::anyhow!("{e}"))?;

    // Per-image pipeline.
    let mut events: Vec<ImageEvent> = Vec::new();
    let mut failed = 0usize;
    let mut pushed = 0usize;
    let mut skipped = 0usize;
    let registry = SkopeoRegistry;

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
        let base_digest = resolve_base_digest(&registry, config)?;

        // Compute build key.
        let containerfile = generate_containerfile(config)?;
        let guest_hash_str = guest_hash(crate::guest::PODBOX_GUEST.unwrap_or(&[]));
        let integration_labels = integration_label_pairs(config);
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
            probe_decision(&registry, &image.ref_str, probe_tag.clone(), &build_key)?
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

        // Build each platform tag.
        let mut image_failed = false;
        for platform in &image.platforms {
            for tag in &image.tags {
                let full_tag = format!("{}:{}", image.ref_str, tag);
                let labels = build_labels(&context, &config.integration, &build_key);
                if let Err(e) = engine.build(
                    image,
                    &full_tag,
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
                break;
            }
        }

        if image_failed {
            continue;
        }

        // Push.
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
        }

        if image_failed {
            continue;
        }

        pushed += 1;
        events.push(ImageEvent {
            key: image.key.clone(),
            ref_str: image.ref_str.clone(),
            tags: image.tags.clone(),
            pushed: push,
            skipped: false,
            digest: Some(base_digest.clone()),
            build_key: Some(build_key),
            retagged: None,
            first_push: None,
            error: None,
            error_code: None,
        });
    }

    // Emit output.
    match output {
        OutputFormat::Ndjson => {
            for ev in &events {
                contract::emit_event(image_event(ev));
            }
            let ok = failed == 0;
            contract::emit_event(done_event(ok, events.len(), pushed, skipped, failed));
        }
        OutputFormat::Json => {
            let mut json_events = Vec::new();
            for ev in &events {
                json_events.push(image_event(ev));
            }
            let ok = failed == 0;
            contract::print_json(done_document(ok, json_events, pushed, skipped, failed))?;
        }
        OutputFormat::Text => {
            if failed > 0 {
                eprintln!("{failed} image(s) failed");
            }
        }
    }

    if failed > 0 {
        return Err(anyhow::anyhow!("publish failed: {failed} image(s) failed"));
    }
    Ok(())
}

/// Synthesize a manifest for a lone `podbox.toml`, which is a container
/// definition rather than an `images.toml` manifest.
fn lone_podbox_manifest(config: String) -> Manifest {
    let mut images = BTreeMap::new();
    images.insert(
        "podbox".to_owned(),
        crate::publish::manifest::ImageEntry {
            config,
            tags: Vec::new(),
            platforms: Vec::new(),
            name: None,
            containerfile: None,
        },
    );
    Manifest {
        schema_version: 1,
        registry: "ghcr.io".into(),
        platforms: Vec::new(),
        tags: vec!["latest".into(), "sha".into()],
        containerfile: None,
        images,
    }
}

/// Load manifest: explicit --manifest or discovery.
fn load_manifest(
    manifest_path: &Path,
    explicit: bool,
    dir: &Path,
) -> Result<Manifest, PodboxError> {
    // A `podbox.toml` is a definition, never a manifest. Handle it before the
    // "path exists" branch so it is not parsed with the manifest schema.
    if manifest_path.file_name().and_then(|n| n.to_str()) == Some("podbox.toml") {
        let relative = manifest_path
            .strip_prefix(dir)
            .unwrap_or(Path::new("podbox.toml"));
        return Ok(lone_podbox_manifest(
            relative.to_string_lossy().into_owned(),
        ));
    }
    if manifest_path.exists() {
        let content =
            std::fs::read_to_string(manifest_path).map_err(|e| PodboxError::ManifestInvalid {
                details: format!("failed to read manifest '{}': {e}", manifest_path.display()),
            })?;
        return Manifest::parse(&content).map_err(|e| PodboxError::ManifestInvalid {
            details: e.to_string(),
        });
    }
    if explicit {
        return Err(PodboxError::ManifestMissing {
            dir: manifest_path.display().to_string(),
        });
    }
    // Discovery.
    let entries = list_dir(dir).map_err(|e| PodboxError::ManifestInvalid {
        details: format!("failed to list directory '{}': {e}", dir.display()),
    })?;
    match discover(&entries, dir) {
        Ok(Discovery::ImagesToml(path)) => {
            let content =
                std::fs::read_to_string(&path).map_err(|e| PodboxError::ManifestInvalid {
                    details: format!("failed to read manifest '{}': {e}", path.display()),
                })?;
            Manifest::parse(&content).map_err(|e| PodboxError::ManifestInvalid {
                details: e.to_string(),
            })
        }
        Ok(Discovery::EnvsDir { images, .. }) => Ok(Manifest {
            schema_version: 1,
            registry: "ghcr.io".into(),
            platforms: Vec::new(),
            tags: vec!["latest".into(), "sha".into()],
            containerfile: None,
            images,
        }),
        Ok(Discovery::LonePodboxToml(_path)) => Ok(lone_podbox_manifest("podbox.toml".into())),
        Err(e) => Err(PodboxError::ManifestMissing { dir: e.dir }),
    }
}

/// Print the plan for `--list`.
fn print_list(plan: &Plan, output: OutputFormat) -> Result<()> {
    match output {
        OutputFormat::Json => {
            let entries: Vec<Value> = plan.images.iter().map(plan_entry).collect();
            contract::print_json(json!({"images": entries}))?;
        }
        OutputFormat::Ndjson => {
            for image in &plan.images {
                contract::emit_event(plan_entry(image));
            }
        }
        OutputFormat::Text => {
            for image in &plan.images {
                println!("{}  {}  {}", image.key, image.ref_str, image.tags.join(","));
            }
        }
    }
    Ok(())
}

/// Resolve base digest for a definition's FROM image.
fn resolve_base_digest(registry: &dyn Registry, config: &Config) -> Result<String, PodboxError> {
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

/// A build context emitted to a temp dir.
#[derive(Debug)]
struct BuildContext {
    /// Owns the directory: it must outlive the build, otherwise the
    /// Containerfile is deleted before the engine reads it.
    dir: tempfile::TempDir,
    /// Absolute path to the generated build file (the `--file` argument).
    containerfile: PathBuf,
}

/// Emit the build context (Containerfile + guest binary) to a temp dir.
fn emit_context(containerfile: &str, name: &str) -> Result<BuildContext, PodboxError> {
    // Defense in depth: names were already validated during resolution.
    build_context::validate_name(name)?;
    let dir = tempfile::Builder::new()
        .prefix("podbox-publish-")
        .tempdir()
        .map_err(publish_build_failed)?;
    let path = dir.path().join(name);
    std::fs::write(&path, containerfile).map_err(publish_build_failed)?;
    if let Some(guest) = crate::guest::PODBOX_GUEST {
        std::fs::write(dir.path().join("podbox-guest"), guest).map_err(publish_build_failed)?;
    }
    Ok(BuildContext {
        dir,
        containerfile: path,
    })
}

fn publish_build_failed(error: std::io::Error) -> PodboxError {
    PodboxError::PublishBuildFailed {
        key: "unknown".into(),
        details: error.to_string(),
    }
}

/// Generate the Containerfile for a definition.
fn generate_containerfile(config: &Config) -> Result<String, PodboxError> {
    let env = crate::env::HostEnv::mock_preview();
    crate::codegen::containerfile::generate(config, "podbox-guest", &env)
}

/// Extract integration label pairs (explicit keys only) for the build key.
fn integration_label_pairs(config: &Config) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let int = &config.integration;
    if config.explicit_keys.contains("integration.wayland") {
        out.push(("integration.wayland".into(), int.wayland.to_string()));
    }
    if config.explicit_keys.contains("integration.audio") {
        out.push(("integration.audio".into(), int.audio.to_string()));
    }
    if config.explicit_keys.contains("integration.dbus") {
        out.push(("integration.dbus".into(), int.dbus.to_string()));
    }
    if config.explicit_keys.contains("integration.gpu") {
        out.push(("integration.gpu".into(), format!("{:?}", int.gpu)));
    }
    out
}

enum Action {
    Build,
    Skip,
    #[allow(dead_code)]
    Retag {
        tags: Vec<String>,
    },
}

/// Probe decision: skip / retag / build.
fn probe_decision(
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

/// True if the manifest source is a lone podbox.toml (affects ref derivation).
fn manifest_source_is_lone_podbox_toml(
    opts: &PublishOpts,
    manifest_path: &Path,
) -> Result<bool, PodboxError> {
    if let Some(m) = &opts.manifest {
        return Ok(m.file_name().and_then(|n| n.to_str()) == Some("podbox.toml"));
    }
    let entries = list_dir(manifest_path.parent().unwrap_or(Path::new("."))).map_err(|e| {
        PodboxError::ManifestInvalid {
            details: e.to_string(),
        }
    })?;
    match discover(&entries, manifest_path.parent().unwrap_or(Path::new("."))) {
        Ok(Discovery::LonePodboxToml(_)) => Ok(true),
        _ => Ok(false),
    }
}

use serde_json::Value;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::publish::registry::RemoteImage;

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
    fn lone_podbox_toml_is_a_definition_not_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("podbox.toml");
        std::fs::write(&path, "[image]\nbase = \"alpine:latest\"\n").unwrap();

        let manifest = load_manifest(&path, false, dir.path()).unwrap();
        assert_eq!(manifest.images.len(), 1);
        assert_eq!(manifest.images["podbox"].config, "podbox.toml");
    }

    #[test]
    fn emitted_context_survives_until_the_builder_is_done() {
        let ctx = emit_context("FROM alpine:latest\n", "Containerfile").unwrap();
        assert!(ctx.dir.path().join("Containerfile").is_file());
        assert!(ctx.dir.path().join("podbox-guest").is_file());
        assert_eq!(ctx.containerfile, ctx.dir.path().join("Containerfile"));
    }

    #[test]
    fn emitted_context_uses_the_configured_name() {
        let ctx = emit_context("FROM alpine:latest\n", "Dockerfile.custom").unwrap();
        assert!(ctx.containerfile.is_file());
        assert!(ctx.containerfile.ends_with("Dockerfile.custom"));
        assert!(!ctx.dir.path().join("Containerfile").exists());
        assert!(ctx.dir.path().join("podbox-guest").is_file());
    }

    #[test]
    fn invalid_containerfile_name_fails_before_io() {
        let err = emit_context("FROM x\n", "../escape").unwrap_err();
        assert!(matches!(err, PodboxError::ContainerfileNameInvalid { .. }));
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
