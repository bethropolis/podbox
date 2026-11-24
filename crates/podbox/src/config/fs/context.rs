use std::path::PathBuf;

use anyhow::Result;

use super::discovery::find_config_path;
use super::paths::config_dir;

pub fn active_context_path() -> PathBuf {
    config_dir().join(".active")
}

pub fn read_active_context() -> Option<String> {
    let path = active_context_path();
    let content = std::fs::read_to_string(&path).ok()?;
    let name = content.trim().to_string();
    if name.is_empty() {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    if find_config_path(&name).is_some() {
        Some(name)
    } else {
        let _ = std::fs::remove_file(&path);
        None
    }
}

pub fn write_active_context(name: &str) -> Result<()> {
    let path = active_context_path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, name)?;
    Ok(())
}

pub fn clear_active_context() -> Result<()> {
    let path = active_context_path();
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
