//! Doctor diagnostics for `podbox doctor`.
//!
//! Slim dispatcher module; Host checks live in [`host`], Integration checks
//! in [`integration`], Container checks in [`container`], the exposure
//! summary in [`exposure`], and `--fix` actions in [`fix`]. See `super` for
//! the rest of the runtime command surface.

mod container;
mod exposure;
mod fix;
mod host;
mod integration;

pub use fix::try_fix_bare_memory_for_target;
pub(crate) use host::is_systemd_managed;

use anyhow::Result;
use owo_colors::{OwoColorize, Stream};
use serde::Serialize;

use podbox::cli::OutputFormat;
use podbox::config::Config;
use podbox::env::HostEnv;

/// One collected check result. Collectors in the submodules return these;
/// [`run_doctor`] assigns the group and tallies pass/fail via `check!`.
#[derive(Debug)]
pub(crate) struct Check {
    name: String,
    status: &'static str,
    message: String,
}

impl Check {
    pub(crate) fn new(
        name: impl Into<String>,
        status: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status,
            message: message.into(),
        }
    }
}

#[derive(Serialize)]
struct DoctorEntry {
    group: &'static str,
    name: String,
    status: String,
    message: String,
}

/// Report section for a doctor check. Host = machine/system prerequisites,
/// Container = this definition's lifecycle artifacts, Integration = the
/// host↔container bridges (Wayland, D-Bus, clipboard, exports).
fn group_for(check_name: &str) -> &'static str {
    match check_name {
        "podman"
        | "/etc/subuid"
        | "/etc/subgid"
        | "loginctl linger"
        | "embedded guest binary"
        | "config layout" => "Host",
        "Quadlet files" | "orphaned snapshot" | "memory" | "guest version" | "protocol" => {
            "Container"
        }
        _ => "Integration",
    }
}

pub fn run_doctor(config: &Config, env: &HostEnv, fix: bool, output: OutputFormat) -> Result<()> {
    let mut entries: Vec<DoctorEntry> = Vec::new();
    let mut passes = 0u32;
    let mut failures = 0u32;

    macro_rules! check {
        ($name:expr, $status:expr, $msg:expr $(,)?) => {{
            entries.push(DoctorEntry {
                group: group_for($name),
                name: $name.to_string(),
                status: $status.to_string(),
                message: $msg.to_string(),
            });
            match $status {
                "pass" => passes += 1,
                "fail" => failures += 1,
                _ => {}
            }
        }};
    }

    macro_rules! collect {
        ($checks:expr) => {{
            for c in $checks {
                check!(c.name.as_str(), c.status, c.message.as_str());
            }
        }};
    }

    collect!(host::check_podman());
    collect!(integration::check_wayland(config, env, fix));
    collect!(integration::check_xdg_user_dir());
    collect!(host::check_sub_ids(env));
    collect!(host::check_embedded_guest());
    collect!(host::check_linger(config, fix));
    collect!(container::check_quadlet(config));
    collect!(integration::check_host_exec(config));
    collect!(integration::check_hardware(config, env));
    collect!(integration::check_secrets(config));
    collect!(host::check_config_layout(fix));
    collect!(container::check_memory(config, fix));
    collect!(container::check_guest_version(config));
    collect!(integration::check_toolchain());
    collect!(integration::check_stale_sockets(fix));
    collect!(container::check_orphaned_snapshots());
    collect!(integration::check_dead_exports(fix));

    match output {
        OutputFormat::Json => {
            let report = serde_json::json!({
                "checks": entries,
                "summary": {
                    "passes": passes,
                    "failures": failures,
                    "total": entries.len(),
                }
            });
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        OutputFormat::Text => {
            // Header: which container this run is for, with active hint on default run.
            let active = podbox::config::read_active_context();
            let is_active = active.as_deref() == Some(config.container.name.as_str());
            if is_active {
                println!(
                    "Doctor — container '{}' (active context)\n",
                    config.container.name
                );
            } else {
                println!("Doctor — container '{}'\n", config.container.name);
            }
            // Grouped sections in stable order; container-specific at the bottom.
            for group in ["Host", "Integration", "Container"] {
                let section: Vec<_> = entries.iter().filter(|e| e.group == group).collect();
                if section.is_empty() {
                    continue;
                }
                println!("{}", group.if_supports_color(Stream::Stdout, |s| s.bold()));
                for entry in &section {
                    let tag = match entry.status.as_str() {
                        "pass" => "PASS"
                            .if_supports_color(Stream::Stdout, |s| s.green())
                            .to_string(),
                        "warn" => "WARN"
                            .if_supports_color(Stream::Stdout, |s| s.yellow())
                            .to_string(),
                        "fail" => "FAIL"
                            .if_supports_color(Stream::Stdout, |s| s.red())
                            .to_string(),
                        _ => entry.status.clone(),
                    };
                    println!("  [{tag}] {}: {}", entry.name, entry.message);
                }
            }
            println!("\n{passes} / {} checks passed", entries.len());
            exposure::print_exposure_summary(config);
        }
    }

    if failures > 0 {
        Err(anyhow::anyhow!("{failures} check(s) failed"))
    } else {
        Ok(())
    }
}
