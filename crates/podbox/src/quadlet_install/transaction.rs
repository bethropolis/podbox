use super::units;

/// A Quadlet/systemd file as it was before an install: its path and its
/// prior bytes, or `None` when the file did not exist (so rollback removes
/// a half-written new file instead of leaving it behind).
pub(super) struct PriorFile {
    path: std::path::PathBuf,
    bytes: Option<Vec<u8>>,
}

/// Snapshot every unit file an install may replace, so a failure can put
/// the previous state back.
pub(super) fn snapshot_prior_units(
    name: &str,
    qdir: &std::path::Path,
    sdir: &std::path::Path,
) -> Vec<PriorFile> {
    let mut paths = vec![
        qdir.join(format!("{name}.container")),
        qdir.join(format!("{name}.build")),
        qdir.join(name).join(format!("{name}.container")),
        qdir.join(name).join(format!("{name}.build")),
    ];
    paths.extend(
        units::custom_unit_filenames(name)
            .iter()
            .map(|f| sdir.join(f)),
    );
    paths
        .into_iter()
        .filter_map(|path| match std::fs::read(&path) {
            Ok(bytes) => Some(PriorFile {
                path,
                bytes: Some(bytes),
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Some(PriorFile {
                path,
                bytes: None,
            }),
            Err(e) => {
                eprintln!(
                    "Warning: cannot snapshot '{}' for rollback ({e:#}); it will be left alone on failure",
                    path.display()
                );
                None
            }
        })
        .collect()
}

/// Best-effort restore of a [`snapshot_prior_units`] snapshot. Returns the
/// number of files that could not be restored.
pub(super) fn restore_prior_units(prior: &[PriorFile]) -> usize {
    let mut failed = 0usize;
    for file in prior {
        let result = match &file.bytes {
            Some(bytes) => {
                if let Some(parent) = file.path.parent() {
                    if std::fs::create_dir_all(parent).is_err() {
                        Err(())
                    } else {
                        crate::fsutil::write_atomic(&file.path, bytes).map_err(|_| ())
                    }
                } else {
                    crate::fsutil::write_atomic(&file.path, bytes).map_err(|_| ())
                }
            }
            None => {
                if file.path.exists() {
                    std::fs::remove_file(&file.path).map_err(|_| ())
                } else {
                    Ok(())
                }
            }
        };
        if result.is_err() {
            eprintln!(
                "Warning: rollback could not restore '{}'",
                file.path.display()
            );
            failed += 1;
        }
    }
    failed
}
