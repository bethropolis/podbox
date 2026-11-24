use anyhow::Result;

use podbox::cli::{Cli, Command, PublishEngine, PublishFormat};
use podbox::publish::commands::{OutputFormat, PublishOpts};

pub(super) fn run_publish(cli: &Cli, dry_run: bool) -> Result<()> {
    let Command::Publish {
        keys,
        manifest,
        dir,
        registry,
        containerfile,
        push,
        no_push,
        force,
        list,
        engine,
        ci,
        owner,
        repo,
        sha,
        ref_name,
        ref_type,
        event,
        default_branch,
        output,
    } = &cli.command
    else {
        unreachable!("publish adapter called for a non-publish command");
    };
    let opts = PublishOpts {
        keys: keys.clone(),
        manifest: manifest.clone(),
        dir: dir.clone(),
        registry: registry.clone(),
        containerfile: containerfile.clone(),
        push: *push,
        no_push: *no_push,
        force: *force,
        list: *list,
        dry_run,
        engine: engine.as_ref().map(|e| match e {
            PublishEngine::Buildx => "buildx".into(),
            PublishEngine::Podman => "podman".into(),
        }),
        ci: ci.clone(),
        owner: owner.clone(),
        repo: repo.clone(),
        sha: sha.clone(),
        ref_name: ref_name.clone(),
        ref_type: ref_type.clone(),
        event: event.clone(),
        default_branch: default_branch.clone(),
        output: match *output {
            PublishFormat::Text => OutputFormat::Text,
            PublishFormat::Json => OutputFormat::Json,
            PublishFormat::Ndjson => OutputFormat::Ndjson,
        },
    };
    podbox::publish::commands::run(opts)
}
