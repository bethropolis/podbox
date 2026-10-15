//! Small filesystem safety helpers shared by the CLI.
//!
//! Pure std + anyhow, so this module stays available without the `cli`
//! feature (like `config` and `codegen`).

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};

/// Counter that makes temp file names unique within this process.
static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Write `contents` to `path` atomically.
///
/// The data goes to a temp file in the same directory first, then a rename
/// swaps it into place. A direct write truncates in place, so a crash,
/// power loss, or a concurrent reader can observe a half-written file; the
/// rename is atomic on POSIX, so readers always see either the old or the
/// new contents. Same-directory placement also guarantees the rename never
/// crosses filesystems (where it would silently become a non-atomic copy).
///
/// An existing file keeps its permissions; a new file inherits the
/// process umask, as with a direct write.
pub fn write_atomic(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let file_name = path
        .file_name()
        .with_context(|| format!("cannot atomically write '{}': no file name", path.display()))?;
    let dir: &Path = parent.unwrap_or_else(|| Path::new("."));
    if !dir.is_dir() {
        anyhow::bail!(
            "cannot atomically write '{}': directory '{}' does not exist",
            path.display(),
            dir.display()
        );
    }

    // Preserve the existing mode so a rewrite never widens permissions.
    let existing_mode = std::fs::metadata(path).ok().map(|m| m.permissions());

    let pid = std::process::id();
    let mut last_err: Option<std::io::Error> = None;
    for _ in 0..10 {
        let count = TEMP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let tmp_name = format!(
            ".{}.tmp-{pid}-{count}",
            file_name.to_string_lossy(),
            pid = pid,
            count = count
        );
        let tmp = dir.join(tmp_name);
        let mut file = match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)
        {
            Ok(file) => file,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        let result = (|| -> Result<()> {
            if let Some(mode) = &existing_mode {
                file.set_permissions(mode.clone())
                    .with_context(|| format!("failed to set permissions on '{}'", tmp.display()))?;
            }
            file.write_all(contents)
                .with_context(|| format!("failed to write '{}'", tmp.display()))?;
            file.flush()
                .with_context(|| format!("failed to flush '{}'", tmp.display()))?;
            file.sync_all()
                .with_context(|| format!("failed to sync '{}'", tmp.display()))?;
            drop(file);
            std::fs::rename(&tmp, path).with_context(|| {
                format!(
                    "failed to move '{}' into place as '{}'",
                    tmp.display(),
                    path.display()
                )
            })?;
            Ok(())
        })();
        if result.is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp);
        return result;
    }
    Err(anyhow::anyhow!(
        "cannot atomically write '{}': could not create a temp file ({})",
        path.display(),
        last_err.map_or_else(|| "unknown error".to_string(), |e| e.to_string())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "podbox-fsutil-{pid}-{tag}",
            pid = std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn writes_exact_bytes() {
        let dir = scratch_dir("bytes");
        let target = dir.join("out.toml");
        write_atomic(&target, b"tag = \"x\"\n").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"tag = \"x\"\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn leaves_no_temp_files_behind() {
        let dir = scratch_dir("clean");
        let target = dir.join("out.toml");
        write_atomic(&target, b"one").unwrap();
        write_atomic(&target, b"two").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"two");
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["out.toml".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rewrite_preserves_permissions() {
        let dir = scratch_dir("perms");
        let target = dir.join("shim");
        std::fs::write(&target, b"old").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        write_atomic(&target, b"new").unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn refuses_missing_directory() {
        let target = std::env::temp_dir().join(format!(
            "podbox-fsutil-nope-{}/out.toml",
            std::process::id()
        ));
        assert!(write_atomic(&target, b"x").is_err());
    }
}
