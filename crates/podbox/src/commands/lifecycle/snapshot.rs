//! Container snapshotting: tag the current container state as an image,
//! list/prune snapshot metadata, and restore from a snapshot.
//!
//! Extracted verbatim from `commands/lifecycle.rs`.

use std::path::PathBuf;

use anyhow::Result;
use serde::Deserialize;

use podbox::config::Config;

pub(crate) fn snapshot_tag(tag: &str, name: &str) -> String {
    format!("localhost/podbox-{name}:snapshot-{tag}")
}

pub(crate) fn snapshots_dir() -> PathBuf {
    podbox::config::config_dir().join("snapshots")
}

/// Snapshot the current container state as a tagged image.
pub fn run_snapshot(_config: &Config, name: &str, tag: Option<&str>) -> Result<()> {
    let tag: String = match tag {
        Some(t) => t.to_string(),
        None => std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or_else(|_| "0".to_string(), |d| d.as_secs().to_string()),
    };

    let container_name = format!("podbox-{name}");
    let image_tag = snapshot_tag(&tag, name);

    eprintln!("Snapshotting container '{container_name}' as '{image_tag}'...");

    let output = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["commit", &container_name, &image_tag]),
    )?;
    print!("{}", String::from_utf8_lossy(&output.stdout));

    // Store metadata
    let dir = snapshots_dir().join(name);
    std::fs::create_dir_all(&dir)?;
    let meta_path = dir.join(format!("{tag}.toml"));
    let now_rfc = date_now_rfc3339();
    let meta = format!("tag = \"{tag}\"\ncreated = \"{now_rfc}\"\nimage = \"{image_tag}\"\n");
    std::fs::write(&meta_path, &meta)?;

    println!("✓ Snapshot '{image_tag}' saved (tag: {tag})");
    Ok(())
}

#[derive(Deserialize)]
struct SnapshotMeta {
    tag: String,
    created: String,
    image: String,
}

fn list_snapshots(name: &str) -> Result<Vec<SnapshotMeta>> {
    let dir = snapshots_dir().join(name);
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut snapshots: Vec<SnapshotMeta> = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|e| e == "toml") {
            let content = std::fs::read_to_string(entry.path())?;
            if let Ok(meta) = toml::from_str::<SnapshotMeta>(&content) {
                snapshots.push(meta);
            }
        }
    }
    Ok(snapshots)
}

/// List all snapshots for a container.
pub fn run_snapshot_list(name: &str, output: podbox::cli::OutputFormat) -> Result<()> {
    let snapshots = list_snapshots(name)?;
    if let podbox::cli::OutputFormat::Json = output {
        let entries: Vec<serde_json::Value> = snapshots
            .iter()
            .map(|s| serde_json::json!({ "tag": s.tag, "created": s.created, "image": s.image }))
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "snapshots": entries }))?
        );
        return Ok(());
    }
    if snapshots.is_empty() {
        println!("No snapshots for '{name}'.");
        return Ok(());
    }
    println!("{:<16}  {:<29}  IMAGE", "TAG", "CREATED");
    println!("{}", "─".repeat(80));
    for s in &snapshots {
        println!("{:<16}  {:<29}  {}", s.tag, s.created, s.image);
    }
    Ok(())
}

/// Prune old snapshots, keeping the newest N.
pub fn run_snapshot_prune(name: &str, keep: usize, dry_run: bool) -> Result<()> {
    let mut snapshots = list_snapshots(name)?;
    if snapshots.len() <= keep {
        if !dry_run {
            println!(
                "Only {} snapshot(s) exist, nothing to prune (keep={keep}).",
                snapshots.len()
            );
        }
        return Ok(());
    }

    // Sort newest-first
    snapshots.sort_by(|a, b| b.created.cmp(&a.created));

    let to_remove: Vec<&SnapshotMeta> = snapshots.iter().skip(keep).collect();
    println!("Pruning {} snapshot(s), keeping {}:", to_remove.len(), keep);

    for s in &to_remove {
        if dry_run {
            println!("  Would remove: {} (image: {})", s.tag, s.image);
            continue;
        }
        // Remove podman image
        let result =
            podbox::process::run_piped("podman", &podbox::process::args(&["rmi", &s.image]));
        if let Err(e) = result {
            eprintln!("Warning: failed to remove image '{}': {e}", s.image);
        } else {
            println!("  Removed image: {}", s.image);
        }

        // Delete metadata file
        let meta_path = snapshots_dir().join(name).join(format!("{}.toml", s.tag));
        if meta_path.exists() {
            std::fs::remove_file(&meta_path)?;
        }
    }

    if dry_run {
        println!("(dry run, no changes made)");
    }
    Ok(())
}

fn date_now_rfc3339() -> String {
    // Simple RFC 3339 without chrono
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = now.as_secs();
    // Days since epoch
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;

    // Compute year/month/day from days since epoch
    let (year, month, day) = days_to_date(days.cast_signed());
    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}+00:00")
}

fn days_to_date(days: i64) -> (i64, u32, u32) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    (y, m as u32, d as u32)
}

/// Restore a container from a snapshot image.
pub fn run_restore(_config: &Config, name: &str, tag: &str) -> Result<()> {
    let snapshot_img = snapshot_tag(tag, name);
    let latest_img = format!("localhost/podbox-{name}:latest");

    // Verify snapshot exists
    let exists = podbox::podman::image_exists(&snapshot_img).unwrap_or(false);
    if !exists {
        anyhow::bail!("Snapshot '{tag}' not found as image '{snapshot_img}'");
    }

    // Stop the container
    eprintln!("Stopping container 'podbox-{name}'...");
    if let Err(e) = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["stop", &format!("podbox-{name}")]),
    ) {
        eprintln!("Warning: failed to stop container 'podbox-{name}': {e}");
    }

    // Re-tag snapshot as the main image
    eprintln!("Restoring from snapshot '{snapshot_img}'...");
    let output = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["tag", &snapshot_img, &latest_img]),
    )?;
    if !output.status.success() {
        anyhow::bail!("Failed to tag snapshot image");
    }

    // Start the container
    eprintln!("Starting container...");
    if let Err(e) = podbox::process::run_piped(
        "podman",
        &podbox::process::args(&["start", &format!("podbox-{name}")]),
    ) {
        eprintln!("Warning: failed to start container 'podbox-{name}': {e}");
    }

    println!("✓ Restored '{name}' from snapshot '{tag}'");
    Ok(())
}
