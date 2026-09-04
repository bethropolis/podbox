//! Device passthrough emitters: GPU, hardware presets, secrets.
//!
//! Extracted verbatim from `quadlet.rs`; see `super` for the unit entry
//! points.

use crate::config::{Config, GpuMode};
use crate::env::HostEnv;

pub(super) fn emit_gpu(lines: &mut Vec<String>, config: &Config, env: &HostEnv) {
    match config.integration.gpu {
        GpuMode::Enabled => {
            lines.push("AddDevice=/dev/dri".into());
            lines.push(String::new());
        }
        GpuMode::Nvidia => {
            lines.push("AddDevice=/dev/dri".into());
            lines.push("AddDevice=-/dev/nvidiactl".into());
            lines.push("AddDevice=-/dev/nvidia0".into());
            if env.gpu_has_nvidia_uvm {
                lines.push("AddDevice=-/dev/nvidia-uvm".into());
            }
            lines.push(String::new());
        }
        GpuMode::Auto => {
            if env.gpu_has_dri {
                lines.push("AddDevice=/dev/dri".into());
            }
            if env.gpu_has_nvidia {
                lines.push("AddDevice=-/dev/nvidiactl".into());
                lines.push("AddDevice=-/dev/nvidia0".into());
                if env.gpu_has_nvidia_uvm {
                    lines.push("AddDevice=-/dev/nvidia-uvm".into());
                }
            }
            if env.gpu_has_dri || env.gpu_has_nvidia {
                lines.push(String::new());
            }
        }
        GpuMode::Disabled => {}
    }
}

pub fn emit_hardware_devices(lines: &mut Vec<String>, config: &Config) {
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
            lines.push(format!("AddDevice=-/dev/video{i}"));
            lines.push(format!("AddDevice=-/dev/media{i}"));
        }
        emitted = true;
    }

    if hw.serial {
        for i in 0..8 {
            lines.push(format!("AddDevice=-/dev/ttyUSB{i}"));
            lines.push(format!("AddDevice=-/dev/ttyACM{i}"));
        }
        emitted = true;
    }

    if hw.yubikey {
        lines.push("Volume=-%t/pcscd/pcscd.comm:/run/pcscd/pcscd.comm:ro".into());
        for i in 0..16 {
            lines.push(format!("AddDevice=-/dev/hidraw{i}"));
        }
        emitted = true;
    }

    if emitted {
        lines.push(String::new());
    }
}

pub fn emit_secrets(lines: &mut Vec<String>, config: &Config) {
    use crate::config::{SecretEntry, SecretSource, SecretType};

    let mut emitted = false;
    for secret in &config.security.secrets {
        emitted = true;
        match secret {
            SecretEntry::Simple(name) => {
                lines.push(format!("Secret={name},type=env,target={name}"));
            }
            SecretEntry::Detailed {
                name,
                secret_type,
                target,
                mode,
                source,
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
                    lines.push(format!("Secret={}", opts.join(",")));
                }
                SecretSource::Systemd => {
                    lines.push(format!(
                        "Environment={}=%d/{}",
                        target.as_deref().unwrap_or(name),
                        name
                    ));
                }
            },
        }
    }
    if emitted {
        lines.push(String::new());
    }
}
