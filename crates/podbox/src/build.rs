use std::ffi::OsString;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};
use sha2::{Digest, Sha256};

use crate::cli::OutputFormat;
use crate::codegen::containerfile;
use crate::config::Config;
use crate::env::{HostEnv, build_context_dir};
use crate::error::PodboxError;
use crate::ui;
use crate::xdg::ResolvedXdgDirs;

mod prebuilt;

pub(crate) use prebuilt::run_prebuilt;

/// SHA-256 hex digest of a string, used for lock-file invalidation.
pub fn checksum(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hex::encode(hasher.finalize())
}

/// Image cache key: `sha256(rendered recipe ‖ guest binary hash ‖ version)`.
///
/// Everything that affects the image is in the rendered recipe (Containerfile
/// or prebuilt overlay, which already folds in host shell/locale), so
/// toggling a Quadlet-only setting like `integration.audio` leaves the key
/// alone, while a podbox upgrade (new guest bytes or version) busts it.
/// Pure and unit-tested; both build paths share it.
pub(crate) fn image_cache_key(rendered: &str, guest: Option<&[u8]>, version: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(rendered.as_bytes());
    hasher.update([0]);
    match guest {
        Some(bytes) => {
            let mut inner = Sha256::new();
            inner.update(bytes);
            hasher.update(hex::encode(inner.finalize()).as_bytes());
        }
        None => hasher.update(b"no-guest"),
    }
    hasher.update([0]);
    hasher.update(version.as_bytes());
    hex::encode(hasher.finalize())
}

/// Skip a build only when the cached key matches *and* the image still
/// exists. A user-run `podman rmi` must rebuild even with a matching key.
pub(crate) fn cache_hit(
    lock: Option<&crate::lock::LockFile>,
    key: &str,
    image_exists: bool,
) -> bool {
    lock.is_some_and(|l| l.config_checksum == key) && image_exists
}

/// Full build log for a container: ~/.local/state/podbox/<name>/build.log
pub fn build_log_path(name: &str) -> PathBuf {
    dirs::state_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/state"))
        .join("podbox")
        .join(name)
        .join("build.log")
}

/// Last `n` non-empty lines of `text`, joined with newlines.
/// Shown after a failed build so the user sees the error without opening
/// the full log.
fn tail_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

/// Open (truncate) the build log and record the command being run.
pub(crate) fn open_log(name: &str, cmd: &str) -> Result<std::fs::File> {
    let path = build_log_path(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create log dir '{}'", parent.display()))?;
    }
    let mut f = std::fs::File::create(&path)
        .with_context(|| format!("failed to create build log '{}'", path.display()))?;
    writeln!(f, "$ {cmd}")?;
    Ok(f)
}

/// Run one timed phase with a progress step line.
fn phase<T>(label: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    ui::step(label);
    let start = Instant::now();
    match f() {
        Ok(v) => {
            ui::ok(&format!("{label} ({:.1}s)", start.elapsed().as_secs_f32()));
            Ok(v)
        }
        Err(e) => Err(e),
    }
}

/// Run podman via the log-teeing runner; on failure emit a tail + rich
/// BuildFailed error pointing at the log file.
pub(crate) fn run_podman_logged(
    args: &[OsString],
    name: &str,
    what: &str,
    log: &mut std::fs::File,
) -> Result<()> {
    let mirror = ui::is_verbose();
    let status = crate::process::run_with_log("podman", args, log, mirror)?;
    if status.success() {
        return Ok(());
    }
    let log_path = build_log_path(name);
    let text = std::fs::read_to_string(&log_path).unwrap_or_default();
    let tail = tail_lines(&text, 15);
    if !tail.is_empty() {
        eprintln!("\n{tail}");
    }
    Err(PodboxError::BuildFailed(format!(
        "{what} failed ({status}).\n\n\
         Hint: Full output: podman's complete log is at\n      {}\n\
         Re-run with --verbose to stream build output live.",
        log_path.display()
    ))
    .into())
}

/// Run the full build orchestration.
///
/// With `OutputFormat::Ndjson`, stdout carries only progress events
/// (`step` lines, then `done`) instead of human text; diagnostics stay on
/// stderr either way. `--dry-run` always prints the human plan.
pub fn run(
    config: &Config,
    env: &HostEnv,
    xdg: &ResolvedXdgDirs,
    dry_run: bool,
    rebuild: bool,
    output: OutputFormat,
) -> Result<()> {
    if config.image.source().is_prebuilt() {
        run_prebuilt(config, dry_run, rebuild, output)
    } else {
        // Custom builds bake the embedded guest into the image. Builds from
        // the published crate have no guest (PODBOX_GUEST is None); reject
        // up front so the user never gets partway through codegen first.
        if crate::guest::PODBOX_GUEST.is_none() {
            return Err(PodboxError::GuestBinaryUnavailable.into());
        }
        run_build(config, env, xdg, dry_run, rebuild, output)
    }
}

// --- Custom build path ------------------------------------------------------

fn run_build(
    config: &Config,
    env: &HostEnv,
    _xdg: &ResolvedXdgDirs,
    dry_run: bool,
    rebuild: bool,
    output: OutputFormat,
) -> Result<()> {
    let events = matches!(output, OutputFormat::Ndjson);
    let name = &config.container.name;
    let context_dir = build_context_dir(name);
    let containerfile_path = context_dir.join("Containerfile");
    let lock_path = context_dir.join(".podbox.lock");

    // Acquire exclusive build lock (auto-releases on panic/crash via kernel flock)
    let _build_lock = if !dry_run {
        std::fs::create_dir_all(&context_dir)?;
        let file = std::fs::File::create(context_dir.join(".build.lock"))?;
        Some(Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, e)| e)?)
    } else {
        None
    };

    // Guarded by `run()` for custom builds; prebuilt builds never reach here.
    let guest_bin = crate::guest::PODBOX_GUEST.expect("custom build without embedded guest");

    let containerfile = containerfile::generate(config, "podbox-guest", env)?;
    let image_key = image_cache_key(&containerfile, Some(guest_bin), crate::VERSION);

    if dry_run {
        println!("=== Build context: {} ===", context_dir.display());
        println!("=== Containerfile ===");
        println!("{containerfile}");
        println!();
        println!("=== Embedded podbox-guest ===");
        println!("{} bytes (embedded in podbox binary)", guest_bin.len());
        println!(
            "podman build -t {} {}",
            config.local_image_tag(),
            context_dir.display()
        );
        return Ok(());
    }

    if !rebuild {
        let lock = crate::lock::read(&lock_path)?;
        if cache_hit(
            lock.as_ref(),
            &image_key,
            crate::podman::image_exists(&config.local_image_tag())?,
        ) {
            if events {
                crate::contract::emit_event(serde_json::json!({
                    "event": "done", "ok": true, "cached": true,
                }));
            } else {
                println!("Definition unchanged and image already built. Skipping.");
                println!("Use --rebuild to force.");
            }
            return Ok(());
        }
    }

    if events {
        crate::contract::emit_event(serde_json::json!({
            "event": "step", "index": 1, "total": 3, "text": "Writing build context",
        }));
    }
    phase("Writing build context", || {
        std::fs::create_dir_all(&context_dir).map_err(|e| PodboxError::HomeCreateFailed {
            path: context_dir.clone(),
            source: e,
        })?;
        let _ = std::fs::set_permissions(&context_dir, std::fs::Permissions::from_mode(0o700));

        std::fs::write(&containerfile_path, containerfile).with_context(|| {
            format!(
                "failed to write Containerfile to '{}'",
                containerfile_path.display()
            )
        })?;

        let guest_dest = context_dir.join("podbox-guest");
        std::fs::write(&guest_dest, guest_bin).with_context(|| {
            format!("failed to write guest binary to '{}'", guest_dest.display())
        })?;

        std::fs::create_dir_all(&config.container.home).with_context(|| {
            format!(
                "failed to create home dir '{}'",
                config.container.home.display()
            )
        })?;
        Ok(())
    })?;

    let tag = config.local_image_tag();
    let args: Vec<OsString> = vec![
        "build".into(),
        "-t".into(),
        tag.clone().into(),
        context_dir.clone().into(),
    ];

    let mut log = open_log(
        name,
        &format!("podman build -t {tag} {}", context_dir.display()),
    )?;
    ui::step(&format!(
        "Building image {tag} (log: {})",
        build_log_path(name).display()
    ));
    let start = Instant::now();
    if events {
        crate::contract::emit_event(serde_json::json!({
            "event": "step", "index": 2, "total": 3,
            "text": format!("Building image {tag}"),
        }));
    }
    run_podman_logged(&args, name, "podman build", &mut log)?;
    ui::ok(&format!(
        "Image {tag} built ({:.1}s)",
        start.elapsed().as_secs_f32()
    ));

    phase("Writing lock file", || {
        if events {
            crate::contract::emit_event(serde_json::json!({
                "event": "step", "index": 3, "total": 3, "text": "Writing lock file",
            }));
        }
        let digest = crate::podman::image_digest(&tag)?;
        let lock = crate::lock::LockFile {
            config_checksum: image_key,
            image_digest: digest,
        };
        crate::lock::write(&lock_path, &lock)?;
        Ok(())
    })?;

    if events {
        crate::contract::emit_event(serde_json::json!({
            "event": "done", "ok": true, "cached": false,
        }));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tail_lines_keeps_last_n_non_empty() {
        let text = "a\n\nb\nc\nd";
        assert_eq!(tail_lines(text, 2), "c\nd");
        assert_eq!(tail_lines(text, 10), "a\nb\nc\nd");
        assert_eq!(tail_lines("", 3), "");
    }

    #[test]
    fn build_log_path_uses_state_dir() {
        let p = build_log_path("myenv");
        assert!(p.ends_with("podbox/myenv/build.log"));
    }

    /// Image key for a custom config: rendered Containerfile plus the
    /// ambient guest binary and version.
    fn custom_key(config: &Config) -> String {
        let env = crate::env::HostEnv::mock_preview();
        let recipe = crate::codegen::containerfile::generate(config, "podbox-guest", &env).unwrap();
        image_cache_key(&recipe, crate::guest::PODBOX_GUEST, crate::VERSION)
    }

    #[test]
    fn quadlet_only_toggles_leave_the_image_key_alone() {
        // `integration.audio` affects the unit, never the image.
        let base = r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
        let without = Config::parse(base).unwrap();
        let with = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"
[integration]
audio = true
"#,
        )
        .unwrap();
        assert_eq!(custom_key(&without), custom_key(&with));
    }

    #[test]
    fn provision_steps_leave_the_image_key_alone() {
        // Editing a step must never trigger an image rebuild.
        let without = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"
"#,
        )
        .unwrap();
        let with = Config::parse(
            r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"

[[provision]]
name = "toolchain"
run = "mise use -g rust@stable"
"#,
        )
        .unwrap();
        assert_eq!(custom_key(&without), custom_key(&with));
    }

    #[test]
    fn image_affecting_changes_move_the_image_key() {
        let base_toml = r#"
[image]
base = "fedora:44"
name = "env"
[container]
name = "env"
home = "~/env"
"#;
        let base = Config::parse(base_toml).unwrap();
        // A different base image changes the rendered recipe.
        let changed_image = Config::parse(&base_toml.replace("fedora:44", "fedora:45")).unwrap();
        assert_ne!(custom_key(&base), custom_key(&changed_image));
        // So does an added package.
        let with_package = Config::parse(&format!(
            "{base_toml}\n[image.packages]\ninstall = [\"htop\"]\n"
        ))
        .unwrap();
        assert_ne!(custom_key(&base), custom_key(&with_package));
        // So does a different guest binary or host version.
        let env = crate::env::HostEnv::mock_preview();
        let recipe = crate::codegen::containerfile::generate(&base, "podbox-guest", &env).unwrap();
        let key = image_cache_key(&recipe, crate::guest::PODBOX_GUEST, crate::VERSION);
        assert_ne!(
            key,
            image_cache_key(&recipe, Some(b"other-guest"), crate::VERSION)
        );
        assert_ne!(
            key,
            image_cache_key(&recipe, crate::guest::PODBOX_GUEST, "v9.9.9")
        );
    }

    #[test]
    fn missing_image_is_not_a_cache_hit() {
        let lock = crate::lock::LockFile {
            config_checksum: "abc123".into(),
            image_digest: "sha256:deadbeef".into(),
        };
        assert!(cache_hit(Some(&lock), "abc123", true));
        assert!(!cache_hit(Some(&lock), "abc123", false));
        assert!(!cache_hit(Some(&lock), "other", true));
        assert!(!cache_hit(None, "abc123", true));
    }
}
