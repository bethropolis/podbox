//! Device passthrough emitters: GPU, hardware presets, secrets.
//!
//! Extracted verbatim from `quadlet.rs`; see `super` for the unit entry
//! points.

use super::value;
use crate::config::{Config, GpuMode};
use crate::env::HostEnv;

/// NVIDIA device nodes passed through when no CDI spec is in use.
///
/// Every entry carries the `-` prefix, so a host without that node skips
/// it instead of failing the unit. This is the pure-codegen approximation
/// of globbing `/dev/nvidia*`: all GPU indexes, the modeset node and the
/// UVM nodes. Device nodes alone do not carry the user-space driver
/// libraries, which must match the host kernel driver exactly — `doctor`
/// points at `nvidia-ctk cdi generate` for that. Once the CDI fact arrives
/// via the host environment, the CDI-present path emits
/// `AddDevice=nvidia.com/gpu=all` instead.
fn emit_nvidia_fallback(lines: &mut Vec<String>, warnings: &mut Vec<String>) {
    value::push_raw(lines, warnings, "AddDevice", "-/dev/nvidiactl");
    for i in 0..8 {
        value::push_raw(lines, warnings, "AddDevice", &format!("-/dev/nvidia{i}"));
    }
    value::push_raw(lines, warnings, "AddDevice", "-/dev/nvidia-modeset");
    value::push_raw(lines, warnings, "AddDevice", "-/dev/nvidia-uvm");
    value::push_raw(lines, warnings, "AddDevice", "-/dev/nvidia-uvm-tools");
}

pub(super) fn emit_gpu(
    lines: &mut Vec<String>,
    config: &Config,
    env: &HostEnv,
    warnings: &mut Vec<String>,
) {
    // With a CDI spec on the host, pass the GPU through by device ID:
    // unlike plain device nodes this also carries the matching
    // user-space driver libraries.
    let mut nvidia = |lines: &mut Vec<String>| {
        if env.nvidia_cdi_present {
            value::push_raw(lines, warnings, "AddDevice", "nvidia.com/gpu=all");
        } else {
            emit_nvidia_fallback(lines, warnings);
        }
    };
    match config.integration.gpu {
        GpuMode::Enabled => {
            lines.push("AddDevice=/dev/dri".into());
            lines.push(String::new());
        }
        GpuMode::Nvidia => {
            lines.push("AddDevice=/dev/dri".into());
            nvidia(lines);
            lines.push(String::new());
        }
        GpuMode::Auto => {
            if env.gpu_has_dri {
                lines.push("AddDevice=/dev/dri".into());
            }
            if env.gpu_has_nvidia {
                nvidia(lines);
            }
            if env.gpu_has_dri || env.gpu_has_nvidia {
                lines.push(String::new());
            }
        }
        GpuMode::Disabled => {}
    }
}

pub fn emit_hardware_devices(
    lines: &mut Vec<String>,
    config: &Config,
    env: &HostEnv,
    _warnings: &mut Vec<String>,
) {
    let hw = &config.integration.hardware;

    let mut emitted = false;

    if hw.kvm {
        lines.push("AddDevice=-/dev/kvm".into());
        emitted = true;
    }

    if hw.joystick {
        lines.push("AddDevice=-/dev/uinput".into());
        lines.push("AddDevice=-/dev/input".into());
        emitted = true;
    }

    if hw.webcam {
        for i in 0..16 {
            value::push_composed(lines, format!("AddDevice=-/dev/video{i}"));
            value::push_composed(lines, format!("AddDevice=-/dev/media{i}"));
        }
        emitted = true;
    }

    if hw.serial {
        for i in 0..8 {
            value::push_composed(lines, format!("AddDevice=-/dev/ttyUSB{i}"));
            value::push_composed(lines, format!("AddDevice=-/dev/ttyACM{i}"));
        }
        emitted = true;
    }

    if hw.yubikey {
        lines.push("Volume=-%t/pcscd/pcscd.comm:/run/pcscd/pcscd.comm:ro".into());
        for i in 0..16 {
            value::push_composed(lines, format!("AddDevice=-/dev/hidraw{i}"));
        }
        emitted = true;
    }

    if hw.usb && env.host_has_usb_bus {
        // Raw USB devices (adb, fastboot, flashers, SDR): a bind mount,
        // not AddDevice. Nodes appear per plug and re-enumeration, so
        // start-time enumeration would miss later devices; the bind mount
        // is hotplug-aware the way the host is (rootless podman applies
        // no device-cgroup filter). Never `:z` or `:Z`: relabeling would
        // retag the host's device nodes.
        lines.push("Volume=/dev/bus/usb:/dev/bus/usb".into());
        emitted = true;
    }

    if emitted {
        lines.push(String::new());
    }
}

/// Container-side path of a systemd credential: always
/// `/run/secrets/<name>`, matching the `Volume=` line in [`emit_secrets`].
fn secret_container_path(name: &str) -> String {
    format!("/run/secrets/{name}")
}

pub fn emit_secrets(lines: &mut Vec<String>, config: &Config, warnings: &mut Vec<String>) {
    use crate::config::{SecretEntry, SecretSource, SecretType};

    let mut emitted = false;
    for secret in &config.security.secrets {
        emitted = true;
        match secret {
            SecretEntry::Simple(name) => {
                value::push_raw(
                    lines,
                    warnings,
                    "Secret",
                    &format!("{name},type=env,target={name}"),
                );
            }
            SecretEntry::Detailed {
                name,
                secret_type,
                target,
                mode,
                source,
                path: _,
                encrypted: _,
            } => match source {
                SecretSource::Podman => {
                    let mut opts = vec![name.clone()];
                    match secret_type {
                        SecretType::Env => {
                            opts.push("type=env".into());
                            if let Some(t) = target {
                                opts.push(format!("target={t}"));
                            }
                        }
                        SecretType::Mount => {
                            opts.push("type=mount".into());
                            if let Some(t) = target {
                                opts.push(format!("target={t}"));
                            }
                            if let Some(m) = mode {
                                opts.push(format!("mode={m}"));
                            }
                            opts.push("uid=%U".into());
                            opts.push("gid=%G".into());
                        }
                    }
                    value::push_raw(lines, warnings, "Secret", &opts.join(","));
                }
                SecretSource::Systemd => {
                    // systemd credentials live in the service's
                    // $CREDENTIALS_DIRECTORY (%d); the matching
                    // LoadCredential= line is emitted under [Service].
                    // Mount the credential file into the container at a
                    // fixed path and point env vars at the container path.
                    match (value::raw(name), value::raw(&secret_container_path(name))) {
                        (Ok(name_escaped), Ok(container_path)) => {
                            value::push_composed(
                                lines,
                                format!("Volume=%d/{name_escaped}:{container_path}:ro,z"),
                            );
                            if *secret_type == SecretType::Env {
                                value::push_env(
                                    lines,
                                    warnings,
                                    target.as_deref().unwrap_or(name),
                                    &container_path,
                                );
                            }
                        }
                        _ => {
                            warnings.push(format!("skipping systemd secret '{name}': invalid name"))
                        }
                    }
                }
            },
        }
    }
    if emitted {
        lines.push(String::new());
    }
}
