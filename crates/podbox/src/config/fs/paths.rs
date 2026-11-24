use std::path::PathBuf;

/// Default home directory for a container: `~/containers/<name>`.
pub fn default_home(name: &str) -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("containers")
        .join(name)
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(stripped) = path.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    if path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    PathBuf::from(path)
}

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("podbox"))
        .unwrap_or_else(|| PathBuf::from("~/.config/podbox"))
}

/// Canonical profiles directory: ~/.config/podbox/profiles/
pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(expand_tilde("~/foo"), home.join("foo"));
        assert_eq!(expand_tilde("~"), home.clone());
        assert_eq!(expand_tilde("/foo/bar"), PathBuf::from("/foo/bar"));
    }
}
