use anyhow::{Context, Result};

use podbox::config;

use podbox::ui;

pub struct MigrateOpts {
    pub dry_run: bool,
    pub force: bool,
}

pub fn run_migrate(opts: MigrateOpts) -> Result<()> {
    let root = config::config_dir();
    let target_dir = config::profiles_dir();

    let legacy_files = config::find_legacy_root_configs();

    if legacy_files.is_empty() {
        ui::ok("No legacy configs found. All configs are in ~/.config/podbox/profiles/.");
        return Ok(());
    }

    ui::step(&format!(
        "Found {} legacy config(s) in {}",
        legacy_files.len(),
        root.display()
    ));

    if !opts.dry_run {
        std::fs::create_dir_all(&target_dir)
            .with_context(|| format!("failed to create directory {}", target_dir.display()))?;
    }

    let mut migrated_count = 0;

    for src in legacy_files {
        let file_name = src.file_name().unwrap();
        let dst = target_dir.join(file_name);
        let name_str = src.file_stem().unwrap().to_string_lossy();

        if dst.exists() && !opts.force {
            ui::warn(&format!(
                "Skipping {}: target '{}' already exists (use --force to overwrite)",
                src.display(),
                dst.display()
            ));
            continue;
        }

        if opts.dry_run {
            println!("Would move: {} → {}", src.display(), dst.display());
        } else {
            std::fs::rename(&src, &dst).with_context(|| {
                format!("failed to move {} to {}", src.display(), dst.display())
            })?;
            ui::ok(&format!(
                "Moved {} → profiles/{}",
                name_str,
                file_name.to_string_lossy()
            ));

            let _ = podbox::history::record(&name_str, "migrate", "moved to profiles/ directory");
            migrated_count += 1;
        }
    }

    if opts.dry_run {
        println!("\n(Dry run: no files moved)");
    } else {
        ui::ok(&format!(
            "Migration complete. {migrated_count} config(s) moved to profiles/."
        ));
    }

    Ok(())
}
