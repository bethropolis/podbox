use super::value;
use crate::config::Config;
use crate::env::HostEnv;

pub(super) fn emit_container_image(
    lines: &mut Vec<String>,
    config: &Config,
    name: &str,
    home_in_container: &str,
    env: &HostEnv,
    warnings: &mut Vec<String>,
) {
    lines.push("[Container]".into());
    if config.image.source().is_prebuilt()
        && !crate::codegen::containerfile::prebuilt_overlay_needed(config)
    {
        let ref_str = match config.image.source() {
            crate::config::ImageSource::Prebuilt { ref_str } => ref_str,
            _ => config.image.base.clone(),
        };
        value::push_raw(lines, warnings, "Image", &ref_str);
        value::push_raw(
            lines,
            warnings,
            "Retry",
            &config.image.pull_retry.to_string(),
        );
        value::push_raw(
            lines,
            warnings,
            "RetryDelay",
            &config.image.pull_retry_delay,
        );
    } else {
        value::push_raw(lines, warnings, "Image", &config.local_image_tag());
    }
    value::push_raw(lines, warnings, "ContainerName", name);
    if let Some(ref mode) = config.security.userns {
        value::push_raw(lines, warnings, "UserNS", mode.as_str());
    } else {
        lines.push("UserNS=keep-id".into());
    }
    lines.push("User=root".into());
    if config.security.security_label_disable {
        lines.push("SecurityLabelDisable=true".into());
    }
    if let Some(ref seccomp) = config.security.seccomp {
        value::push_raw(lines, warnings, "SeccompProfile", seccomp);
    }
    if config.security.no_new_privileges {
        lines.push("NoNewPrivileges=true".into());
    }
    if let Some(ref mem) = config.container.memory {
        value::push_raw(lines, warnings, "Memory", mem);
    }
    if let Some(ref cpus) = config.container.cpus {
        // Quadlet has no `CpuQuota` key: the quadlet-generator rejects the
        // whole unit as "unsupported key", so the container silently never
        // starts. CPU limits go through `--cpus`, which also takes fractions
        // (`0.5`) that a period-based quota cannot express.
        if let Ok(v) = cpus.parse::<f64>() {
            value::push_raw(lines, warnings, "PodmanArgs", &format!("--cpus={v}"));
        }
    }
    if config.security.read_only_rootfs {
        lines.push("ReadOnly=true".into());
        // A read-only root leaves no writable runtime paths, and the guest
        // daemon needs some: it does `create_dir_all("/run/podbox/bin")?`
        // before it can talk to the host at all, so without a tmpfs there the
        // image never starts. Ordinary tools need /tmp and /var/tmp too.
        // tmpfs keeps the image layers immutable while the running container
        // still has somewhere to work.
        lines.push("Mount=type=tmpfs,destination=/run,tmpfs-mode=1777".into());
        lines.push("Mount=type=tmpfs,destination=/tmp,tmpfs-mode=1777".into());
        lines.push("Mount=type=tmpfs,destination=/var/tmp,tmpfs-mode=1777".into());
    }
    if let Some(ref profile) = config.security.apparmor {
        value::push_raw(lines, warnings, "AppArmor", profile);
    }
    value::push_composed(lines, format!("Environment=HOME={home_in_container}"));
    value::push_env(lines, warnings, "HOST_USER", &env.username);
    lines.push("Environment=HOST_UID=%U".into());
    lines.push("Environment=HOST_GID=%G".into());
    lines.push("Environment=PATH=/run/podbox/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".into());
    lines.push(String::new());
}

pub(super) fn emit_network(lines: &mut Vec<String>, config: &Config, warnings: &mut Vec<String>) {
    let mode = config.network.effective_mode();
    value::push_raw(lines, warnings, "Network", mode.as_str());
    if mode != crate::config::NetworkMode::Host && mode != crate::config::NetworkMode::None {
        for port in &config.network.ports {
            value::push_raw(lines, warnings, "PublishPort", port);
        }
    }
    lines.push(String::new());
}

pub(crate) fn wayland_socket_path(config: &Config, display: &str, name: &str) -> String {
    if config.use_wayland_proxy() {
        format!("%t/podbox/{name}-wayland.sock")
    } else {
        format!("%t/{display}")
    }
}

pub(super) fn emit_auto_update(lines: &mut Vec<String>, config: &Config) {
    if config.lifecycle.auto_update {
        if config.image.source().is_prebuilt() {
            lines.push("AutoUpdate=registry".into());
        } else {
            lines.push("AutoUpdate=local".into());
        }
        lines.push(String::new());
    }
}

pub(super) fn emit_podman_args(
    lines: &mut Vec<String>,
    config: &Config,
    warnings: &mut Vec<String>,
) {
    // Native Quadlet keys where they exist (all predate the Podman 5.5
    // floor, so no version fallback is needed): only --cpus still goes
    // through PodmanArgs, because Quadlet has no CpuQuota key.
    lines.push("RunInit=true".into());
    value::push_composed(lines, "WorkingDir=/home/%u".to_string());
    let cap_preset = config.security.cap_preset;
    let has_any_cap = !cap_preset.caps().is_empty() || !config.security.cap_add.is_empty();
    for cap in cap_preset.caps() {
        value::push_raw(lines, warnings, "AddCapability", cap);
    }
    for cap in &config.security.cap_add {
        value::push_raw(lines, warnings, "AddCapability", cap);
    }
    if has_any_cap {
        lines.push(String::new());
    }
    if let Some(ref cmd) = config.container.reload_cmd {
        value::push_raw(lines, warnings, "ReloadCmd", cmd);
        lines.push(String::new());
    }
}
