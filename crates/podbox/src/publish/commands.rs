//! `podbox publish` orchestration.

mod artifacts;
mod pipeline;
mod prepare;
mod probe;
mod render;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::SystemTime;

use anyhow::Result;

use crate::config::Config;
use crate::error::PodboxError;
use crate::publish::build_context;
use crate::publish::context::{context_from_flags, context_from_github_env};
use crate::publish::engine::select_engine;
use crate::publish::plan::build_plan;
use crate::publish::registry::SkopeoRegistry;

use prepare::{
    apply_context_overrides, load_manifest, manifest_source_is_lone_podbox_toml, parse_ref_type,
};
use render::print_list;

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
    let output = opts.output;

    // Build the publish context.
    let now = SystemTime::now();
    let ref_type = opts.ref_type.as_deref().map(parse_ref_type).transpose()?;
    let mut context = match opts.ci.as_deref() {
        Some("github") => context_from_github_env(now)?,
        Some(other) => {
            return Err(PodboxError::InvalidOptionValue {
                option: "--ci",
                value: other.into(),
                expected: "'github'",
            }
            .into());
        }
        None => context_from_flags(
            opts.owner.clone(),
            opts.repo.clone(),
            opts.sha.clone(),
            opts.ref_name.clone(),
            ref_type,
            opts.event.clone(),
            opts.default_branch.clone(),
            None,
            now,
        )?,
    };
    // `--owner`, `--sha`, etc. are documented as overriding `--ci github`.
    apply_context_overrides(&mut context, &opts)?;

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

    // Every publish build copies the embedded guest binary into the context.
    let guest = crate::guest::PODBOX_GUEST.ok_or(PodboxError::GuestBinaryUnavailable)?;

    // Select engine.
    let prefer_ci = opts.ci.as_deref() == Some("github");
    let engine: Box<dyn crate::publish::engine::Engine> =
        select_engine(prefer_ci, opts.engine.as_deref())?;

    let registry = SkopeoRegistry;
    let summary = pipeline::execute(pipeline::PipelineInput {
        plan: &plan,
        definitions: &definitions,
        containerfile_names: &containerfile_names,
        guest,
        context: &context,
        opts: &opts,
        registry: &registry,
        engine: engine.as_ref(),
        output,
    })?;
    render::render_summary(&summary, output)?;

    if summary.failed > 0 {
        return Err(PodboxError::PublishFailed {
            failed: summary.failed,
        }
        .into());
    }
    Ok(())
}
