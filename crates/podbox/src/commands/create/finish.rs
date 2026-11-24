use anyhow::Result;

use podbox::config;
use podbox::config::Config;

/// Build image, install Quadlet, and start the container.
pub(super) fn finish_create(
    cfg: &Config,
    _caller_name: &str,
    dry_run: bool,
    no_start: bool,
) -> Result<()> {
    podbox::quadlet_install::preflight::resolve_x11_dependencies(cfg)?;
    // The caller's name can be a file name or `--name`; the box itself is
    // installed under `container.name`, so that is what starts and exports.
    let container_name = cfg.runtime_name();
    if dry_run {
        println!("podbox build");
    } else {
        let local_tag = cfg.local_image_tag();
        if podbox::podman::image_exists(&local_tag).unwrap_or(false) {
            println!("Image already exists, skipping build.");
        } else {
            let env = podbox::env::resolve()?;
            let xdg = podbox::xdg::resolve(&cfg.integration.xdg_dirs)?;
            podbox::build::run(
                cfg,
                &env,
                &xdg,
                false,
                false,
                podbox::cli::OutputFormat::Text,
            )?;
        }
    }

    if dry_run {
        println!("podbox enable");
    } else {
        println!("Installing Quadlet files...");
        let env = podbox::env::resolve()?;
        let xdg = podbox::xdg::resolve(&cfg.integration.xdg_dirs)?;
        podbox::quadlet_install::install(cfg, &env, &xdg, false)?;
    }

    let dotfiles_pending =
        cfg.dotfiles.is_some() && !crate::commands::dotfiles::is_provisioned(cfg);
    let dotfiles_staged = if !dotfiles_pending {
        true
    } else if dry_run {
        crate::commands::dotfiles::stage(cfg, &podbox::env::resolve()?, true).is_ok()
    } else {
        match crate::commands::dotfiles::stage(cfg, &podbox::env::resolve()?, false) {
            Ok(()) => true,
            Err(err) => {
                eprintln!("Warning: dotfiles source setup failed: {err:#}");
                eprintln!(
                    "Fix the source issue and run `podbox dotfiles sync {container_name}` to retry."
                );
                false
            }
        }
    };

    if no_start {
        println!("Container created but not started (--no-start).");
        println!("Run `podbox enter {container_name}` to start and enter it.");
    } else if dry_run {
        println!("podman start {container_name}");
        if dotfiles_pending && dotfiles_staged {
            crate::commands::dotfiles::provision(cfg, &podbox::env::resolve()?, true)?;
        }
        crate::commands::provision::run_for_create(cfg, &podbox::env::resolve()?, true)?;
    } else {
        podbox::ui::step("Starting container...");
        // The Quadlet unit is freshly installed, so whether systemd owns the
        // start depends only on systemctl being present (see
        // `runner::Runtime::for_system`).
        let ctx = crate::ctx::Ctx { dry_run };
        crate::commands::runner::Runtime::for_system().start(&ctx, cfg)?;
        podbox::ui::ok(&format!("Container '{container_name}' is running"));
        // `create` starts via systemd/podman directly, bypassing `run_start`,
        // so exports happen here rather than riding along with it.
        podbox::export::ensure_exports(cfg, container_name);
        if dotfiles_pending && dotfiles_staged {
            if let Err(err) =
                crate::commands::dotfiles::provision(cfg, &podbox::env::resolve()?, false)
            {
                eprintln!("Warning: dotfiles provisioning failed: {err:#}");
                eprintln!(
                    "Fix the issue and run `podbox dotfiles sync {container_name}` to retry."
                );
            }
        }
        // Provision steps run once, after dotfiles, in declared order.
        if let Err(err) =
            crate::commands::provision::run_for_create(cfg, &podbox::env::resolve()?, false)
        {
            eprintln!("Warning: guest provisioning failed: {err:#}");
            eprintln!(
                "Run `podbox provision status {container_name}`, then `podbox provision sync {container_name}` to retry."
            );
        }
        println!("Run `podbox enter` to enter.");
    }
    if no_start && dotfiles_pending {
        println!(
            "Dotfiles provisioning will run after the container is started; use `podbox dotfiles sync {container_name}` to run it explicitly."
        );
    }
    if no_start && !cfg.provision.is_empty() {
        println!(
            "Provisioning steps will run after the container is started; use `podbox provision status {container_name}` to check."
        );
    }

    if !dry_run {
        // `create` retargets the active context so the follow-up commands
        // (`podbox enter`, bare `podbox logs`) work with no name. Say so:
        // `.active` is shared state, and a silent switch surprised users
        // who kept several boxes on one host.
        let previous = config::read_active_context();
        if let Some(line) = context_announcement(previous.as_deref(), container_name) {
            println!("{line}");
        }
        let _ = config::write_active_context(container_name);
        let _ = podbox::history::record(
            container_name,
            "create",
            &format!("image {}", cfg.image.name),
        );
    }

    Ok(())
}

/// The line `create` prints when it retargets the active context, or `None`
/// when the context already pointed at the box (no news).
fn context_announcement(previous: Option<&str>, next: &str) -> Option<String> {
    if previous == Some(next) {
        return None;
    }
    let was = previous.unwrap_or("(none)");
    Some(format!("Active context is now '{next}' (was '{was}')."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_switch_is_announced_only_when_it_changes() {
        assert_eq!(
            context_announcement(None, "dev").as_deref(),
            Some("Active context is now 'dev' (was '(none)').")
        );
        assert_eq!(
            context_announcement(Some("cachy"), "dev").as_deref(),
            Some("Active context is now 'dev' (was 'cachy').")
        );
        // Already the active box: nothing to say.
        assert!(context_announcement(Some("dev"), "dev").is_none());
    }
}
