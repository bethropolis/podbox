use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Lock file tracking definition hash and image digest.
#[derive(Debug, Serialize, Deserialize)]
pub struct LockFile {
    pub config_checksum: String,
    pub image_digest: String,
}

/// Read a lock file if it exists.
pub fn read(path: &Path) -> Result<Option<LockFile>> {
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read lock file '{}'", path.display()))?;
    let lock: LockFile = serde_json::from_str(&content)
        .with_context(|| format!("failed to parse lock file '{}'", path.display()))?;
    Ok(Some(lock))
}

/// Write a lock file, atomically so a crash never leaves a torn lock behind.
pub fn write(path: &Path, lock: &LockFile) -> Result<()> {
    let json = serde_json::to_string_pretty(lock)
        .with_context(|| format!("failed to serialize lock data for '{}'", path.display()))?;
    crate::fsutil::write_atomic(path, format!("{json}\n").as_bytes())
        .with_context(|| format!("failed to write lock file '{}'", path.display()))?;
    Ok(())
}
