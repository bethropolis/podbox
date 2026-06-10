use crate::config::{Config, GpuMode, ImageSource, OnStop};

pub(super) fn print_summary(config: &Config, name: &str) {
    let image_type = match config.image.source() {
        ImageSource::Prebuilt { ref_str } => format!("prebuilt ({})", ref_str),
        ImageSource::Build { base } => format!("build from {}", base),
    };
    let lifecycle = if config.lifecycle.quadlet {
        let extras = vec![
            if config.lifecycle.autostart {
                Some("autostart")
            } else {
                None
            },
            if config.lifecycle.auto_update {
                Some("auto-update")
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(", ");
        if extras.is_empty() {
            "quadlet".to_string()
        } else {
            format!("quadlet ({})", extras)
        }
    } else {
        "manual".to_string()
    };
    let on_stop = match config.lifecycle.on_stop {
        OnStop::Keep => "keep",
        OnStop::Remove => "remove",
    };
    let gpu = match config.integration.gpu {
        GpuMode::Auto => "auto",
        GpuMode::Enabled => "enabled",
        GpuMode::Disabled => "disabled",
        GpuMode::Nvidia => "nvidia",
    };
    let xdg_count = [
        config.integration.xdg_dirs.documents.is_enabled(),
        config.integration.xdg_dirs.downloads.is_enabled(),
        config.integration.xdg_dirs.pictures.is_enabled(),
        config.integration.xdg_dirs.music.is_enabled(),
        config.integration.xdg_dirs.videos.is_enabled(),
        config.integration.xdg_dirs.desktop.is_enabled(),
        config.integration.xdg_dirs.projects.is_enabled(),
    ]
    .iter()
    .filter(|&&b| b)
    .count();

    println!("\n── Summary ──");
    println!("  Name:        {}", name);
    println!("  Image:       {}", image_type);
    println!("  Shell:       {}", config.container.shell);
    println!("  Home:        {}", config.container.home.display());
    if let Some(ref mem) = config.container.memory {
        println!("  Memory:      {}", mem);
    }
    if !config.container.mounts.extra.is_empty() {
        println!(
            "  Mounts:      {}",
            config.container.mounts.extra.join(", ")
        );
    }
    println!("  Integration:");
    println!(
        "    wayland: {}, audio: {}, dbus: {}, gpu: {}",
        config.integration.wayland, config.integration.audio, config.integration.dbus, gpu
    );
    let extras = vec![
        if config.integration.notify {
            Some("notify")
        } else {
            None
        },
        if config.integration.clipboard {
            Some("clipboard")
        } else {
            None
        },
        if config.integration.xdg_open {
            Some("xdg_open")
        } else {
            None
        },
        if config.integration.ssh_agent {
            Some("ssh_agent")
        } else {
            None
        },
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !extras.is_empty() {
        println!("    extras:    {}", extras.join(", "));
    }
    if config.integration.sync_themes
        || config.integration.sync_icons
        || config.integration.sync_fonts
    {
        let sync = vec![
            if config.integration.sync_themes {
                Some("themes")
            } else {
                None
            },
            if config.integration.sync_icons {
                Some("icons")
            } else {
                None
            },
            if config.integration.sync_fonts {
                Some("fonts")
            } else {
                None
            },
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        println!("    sync:      {}", sync.join(", "));
    }
    println!("    xdg dirs:  {} shared", xdg_count);
    if !config.integration.export.apps.is_empty() {
        println!(
            "    exports:   apps: {}",
            config.integration.export.apps.join(", ")
        );
    }
    if !config.integration.export.bins.is_empty() {
        println!(
            "               bins: {}",
            config.integration.export.bins.join(", ")
        );
    }
    println!("  Lifecycle:   {} (on_stop: {})", lifecycle, on_stop);
    println!();
}

pub(super) fn preview_and_confirm(toml: &str) -> bool {
    println!("{}\n", toml);
    dialoguer::Confirm::with_theme(&dialoguer::theme::ColorfulTheme::default())
        .with_prompt("Write to config file?")
        .default(true)
        .interact()
        .expect("failed to get confirmation")
}
