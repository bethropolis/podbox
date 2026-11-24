use super::value;
use crate::config::Config;
use crate::env::HostEnv;

pub(super) fn emit_env(
    lines: &mut Vec<String>,
    config: &Config,
    name: &str,
    home_in_container: &str,
    env: &HostEnv,
    warnings: &mut Vec<String>,
) {
    // Locale environment: LANG only. Forcing LC_ALL would override the
    // user's per-category settings (LC_TIME, LC_NUMERIC, …) inside the
    // box, and a host locale the image lacks makes every program warn.
    // Explicit [container.env] entries still layer over this below.
    if let Some(ref locale) = env.host_locale {
        value::push_env(lines, warnings, "LANG", locale);
        lines.push(String::new());
    }

    // Keep container-side user entries discoverable while the host database
    // supplies the terminal the session is actually attached to. Root ignores
    // TERMINFO_DIRS (ncurses security), so the system mount above covers it.
    if env.host_terminfo_dir.is_some() && !config.container.env.values.contains_key("TERMINFO_DIRS")
    {
        let dirs = format!(
            "/usr/share/terminfo:/usr/local/share/terminfo:\
             {home_in_container}/.terminfo:{home_in_container}/.local/share/terminfo:\
             /lib/terminfo:/usr/lib/terminfo:/etc/terminfo"
        );
        value::push_composed(lines, format!("Environment=TERMINFO_DIRS={dirs}"));
        lines.push(String::new());
    }

    // X11 display, like other derived values, is overridden by explicit
    // [container.env] entries below.
    if config.integration.x11 && !config.container.env.values.contains_key("DISPLAY") {
        lines.push("Environment=DISPLAY=:0".into());
    }

    // Extra user env, layered over engine-derived defaults (SHELL and
    // friends — see ContainerConfig::effective_env). Explicit wins.
    // Names are validated up front (`config::is_valid_env_key`), so every
    // key here is emitted; nothing is warned-and-skipped at this layer.
    for (key, value) in &config.container.effective_env() {
        value::push_env(lines, warnings, key, value);
    }
    if !config.container.services.is_empty() {
        let payload =
            serde_json::to_string(&config.container.services).unwrap_or_else(|_| "{}".into());
        match value::word(&payload) {
            Ok(escaped) => {
                value::push_composed(lines, format!("Environment=PODBOX_SERVICES_JSON={escaped}"))
            }
            Err(e) => warnings.push(format!("skipping PODBOX_SERVICES_JSON line: {e}")),
        }
    }
    value::push_env(lines, warnings, "PODBOX_CONTAINER", name);
    lines.push(String::new());
}
