use anyhow::Result;
use serde_json::json;

use podbox::cli::OutputFormat;
use podbox::codegen::quadlet;
use podbox::config::{Config, ImageSource};
use podbox::env::HostEnv;
use podbox::xdg::ResolvedXdgDirs;

/// Inspect container config, Quadlet, or computed env.
#[allow(clippy::too_many_arguments)]
pub fn run_inspect(
    config: &Config,
    _name: &str,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    show_config: bool,
    show_quadlet: bool,
    show_env: bool,
    output: OutputFormat,
) -> Result<()> {
    let all = !show_config && !show_quadlet && !show_env;

    match output {
        OutputFormat::Json => {
            let mut out = serde_json::Map::new();
            if all || show_config {
                let toml_str = toml::to_string_pretty(config)?;
                out.insert("config".into(), json!(toml_str));
            }
            if all || show_quadlet {
                let q = quadlet::generate_container(config, env, xdg);
                let s = quadlet::generate_socket(config);
                out.insert(
                    "quadlet".into(),
                    json!({
                        "container": q,
                        "socket": s,
                    }),
                );
            }
            if all || show_env {
                let mut env_map = serde_json::Map::new();
                env_map.insert("container_name".into(), json!(config.container.name));
                let image_ref = match config.image.source() {
                    ImageSource::Build { base } => format!("build:{base}"),
                    ImageSource::Prebuilt { ref_str } => ref_str.clone(),
                };
                env_map.insert("image_ref".into(), json!(image_ref));
                env_map.insert(
                    "image_source".into(),
                    json!(format!("{:?}", config.image.source())),
                );
                env_map.insert("quadlet".into(), json!(config.lifecycle.quadlet));
                env_map.insert("autostart".into(), json!(config.lifecycle.autostart));
                env_map.insert("auto_update".into(), json!(config.lifecycle.auto_update));
                env_map.insert(
                    "xdg_runtime_dir".into(),
                    json!(env.xdg_runtime_dir.display().to_string()),
                );
                if let Some(ref w) = env.wayland_display {
                    env_map.insert("wayland_display".into(), json!(w));
                }
                env_map.insert("gpu_dri".into(), json!(env.gpu_has_dri));
                env_map.insert("gpu_nvidia".into(), json!(env.gpu_has_nvidia));
                if let Some(ref dbus) = env.dbus_socket {
                    env_map.insert("dbus_socket".into(), json!(dbus.display().to_string()));
                }
                if env.gpg_agent_socket.is_some() {
                    env_map.insert("gpg_agent".into(), json!("available"));
                }
                if let Some(ref shell) = env.host_shell {
                    env_map.insert("host_shell".into(), json!(shell));
                }
                if let Some(ref locale) = env.host_locale {
                    env_map.insert("host_locale".into(), json!(locale));
                }
                out.insert("env".into(), json!(env_map));
            }
            println!("{}", serde_json::to_string_pretty(&out)?);
        }
        OutputFormat::Text => {
            if all || show_config {
                println!("--- Config ---");
                let toml_str = toml::to_string_pretty(config)?;
                println!("{toml_str}");
            }
            if all || show_quadlet {
                println!("--- Quadlet (.container) ---");
                let q = quadlet::generate_container(config, env, xdg);
                println!("{q}");
                println!();
                println!("--- Quadlet (.socket) ---");
                let s = quadlet::generate_socket(config);
                println!("{s}");
            }
            if all || show_env {
                println!("--- Environment ---");
                println!("Container name:  {}", config.container.name);
                let image_ref = match config.image.source() {
                    ImageSource::Build { base } => format!("build:{base}"),
                    ImageSource::Prebuilt { ref_str } => ref_str.clone(),
                };
                println!("Image ref:       {image_ref}");
                println!("Image source:    {:?}", config.image.source());
                println!("Quadlet:         {}", config.lifecycle.quadlet);
                println!("Auto-start:      {}", config.lifecycle.autostart);
                println!("Auto-update:     {}", config.lifecycle.auto_update);
                println!();
                println!("XDG_RUNTIME_DIR: {}", env.xdg_runtime_dir.display());
                if let Some(ref w) = env.wayland_display {
                    println!("WAYLAND_DISPLAY: {w}");
                }
                if env.gpu_has_dri {
                    println!("GPU (DRI):       yes");
                }
                if env.gpu_has_nvidia {
                    println!("GPU (NVIDIA):    yes");
                }
                if let Some(ref dbus) = env.dbus_socket {
                    println!("D-Bus socket:    {}", dbus.display());
                }
                if env.gpg_agent_socket.is_some() {
                    println!("GPG agent:       available");
                }
                if let Some(ref shell) = env.host_shell {
                    println!("Host shell:      {shell}");
                }
                if let Some(ref locale) = env.host_locale {
                    println!("Host locale:     {locale}");
                }
            }
        }
    }

    Ok(())
}
