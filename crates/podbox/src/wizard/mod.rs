use crate::config::Config;
use crate::profiles;

mod prompts;
pub mod shell;
mod summary;

pub use shell::{ShellInfo, apply_shell_defaults, detect_host_shell};

/// Result of the interactive wizard.
pub struct WizardResult {
    pub config: Config,
    pub name: String,
    pub confirmed: bool,
}

enum ProfileChoice<'a> {
    Custom,
    Named(&'a profiles::Profile, bool),
}

/// Run the interactive init wizard.
pub fn run_wizard(
    profiles_data: &[profiles::Profile],
    detected_shell: &shell::ShellInfo,
) -> anyhow::Result<WizardResult> {
    let (mut config, default_name) = match prompts::prompt_profile(profiles_data) {
        ProfileChoice::Named(profile, customize) => {
            let mut cfg: Config = toml::from_str(&profile.toml).map_err(|e| {
                anyhow::anyhow!("failed to parse profile '{}': {}", profile.name, e)
            })?;
            if customize {
                cfg = prompts::prompt_customize_profile(cfg)?;
            }
            (cfg, profile.name.clone())
        }
        ProfileChoice::Custom => prompts::prompt_custom_image()?,
    };

    // Phase 2: Container
    println!("\n── Container ──\n");
    let name = prompts::prompt_name(&default_name, &crate::config::config_dir())?;
    config.container.name = name.clone();
    config.image.name = name.clone();
    config.container.home = crate::config::expand_tilde(&format!("~/containers/{}", name));

    let shell = prompts::prompt_shell(detected_shell);
    config.container.shell = shell.full_path.clone();
    if !config
        .image
        .packages
        .install
        .iter()
        .any(|p| p == &shell.package_name)
    {
        config
            .image
            .packages
            .install
            .push(shell.package_name.clone());
    }

    if let Some(mem) = prompts::prompt_memory() {
        config.container.memory = Some(mem);
    }

    let mounts = prompts::prompt_extra_mounts();
    config.container.mounts.extra = mounts;

    // Phase 3: Integration
    println!("\n── Integration ──\n");
    config.integration.wayland = prompts::prompt_wayland();
    config.integration.audio = prompts::prompt_audio();
    config.integration.dbus = prompts::prompt_dbus();
    config.integration.gpu = prompts::prompt_gpu();
    config.integration.sync_themes = prompts::confirm_default("Sync themes from host?", true);
    config.integration.sync_icons = prompts::confirm_default("Sync icons from host?", true);
    config.integration.sync_fonts = prompts::confirm_default("Sync fonts from host?", true);

    let (notify, clipboard, xdg_open, ssh_agent) = prompts::prompt_integration_extras(
        config.integration.notify,
        config.integration.clipboard,
        config.integration.xdg_open,
        config.integration.ssh_agent,
    );
    config.integration.notify = notify;
    config.integration.clipboard = clipboard;
    config.integration.xdg_open = xdg_open;
    config.integration.ssh_agent = ssh_agent;

    config.integration.xdg_dirs = prompts::prompt_xdg_dirs();

    // Phase 4: Lifecycle
    println!("\n── Lifecycle ──\n");
    let (quadlet, autostart) = prompts::prompt_lifecycle();
    config.lifecycle.quadlet = quadlet;
    config.lifecycle.autostart = autostart;

    config.lifecycle.on_stop = prompts::prompt_on_stop();
    config.lifecycle.auto_update = prompts::prompt_auto_update(&config);

    // Phase 5: Review
    summary::print_summary(&config, &name);
    let toml_str = toml::to_string_pretty(&config)
        .map_err(|e| anyhow::anyhow!("failed to serialize config: {}", e))?;
    let confirmed = summary::preview_and_confirm(&toml_str);

    Ok(WizardResult {
        config,
        name,
        confirmed,
    })
}
