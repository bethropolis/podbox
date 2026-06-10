/// Information about the host shell, resolved at startup.
pub struct ShellInfo {
    pub bin_name: String,
    pub full_path: String,
    pub package_name: String,
    pub detected: bool,
}

/// Detect the host shell from $SHELL.
pub fn detect_host_shell() -> ShellInfo {
    detect_host_shell_from(std::env::var("SHELL").ok().as_deref())
}

pub(super) fn detect_host_shell_from(shell_path: Option<&str>) -> ShellInfo {
    match shell_path {
        Some(path) if !path.is_empty() => {
            let bin = std::path::Path::new(path)
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            if bin.is_empty() || bin == "sh" || bin == "dash" {
                return fallback_shell();
            }
            let mut info = shell_info_from_bin(&bin);
            info.detected = true;
            info
        }
        _ => fallback_shell(),
    }
}

fn fallback_shell() -> ShellInfo {
    ShellInfo {
        bin_name: "fish".into(),
        full_path: "/usr/bin/fish".into(),
        package_name: "fish".into(),
        detected: false,
    }
}

pub(super) fn shell_info_from_bin(bin: &str) -> ShellInfo {
    match bin {
        "fish" => ShellInfo {
            bin_name: "fish".into(),
            full_path: "/usr/bin/fish".into(),
            package_name: "fish".into(),
            detected: false,
        },
        "bash" => ShellInfo {
            bin_name: "bash".into(),
            full_path: "/bin/bash".into(),
            package_name: "bash".into(),
            detected: false,
        },
        "zsh" => ShellInfo {
            bin_name: "zsh".into(),
            full_path: "/bin/zsh".into(),
            package_name: "zsh".into(),
            detected: false,
        },
        "nu" | "nushell" => ShellInfo {
            bin_name: "nu".into(),
            full_path: "/usr/bin/nu".into(),
            package_name: "nushell".into(),
            detected: false,
        },
        other => ShellInfo {
            bin_name: other.into(),
            full_path: format!("/usr/bin/{}", other),
            package_name: other.into(),
            detected: false,
        },
    }
}

/// Apply shell defaults to a config loaded from a profile.
pub fn apply_shell_defaults(config: &mut crate::config::Config, shell: &ShellInfo) {
    if config.container.shell.trim().is_empty() {
        config.container.shell = shell.full_path.clone();
    }
    if !config
        .image
        .packages
        .install
        .iter()
        .any(|p| p == &shell.package_name)
    {
        config
            .image
            .packages
            .install
            .push(shell.package_name.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_fish_from_path() {
        let info = detect_host_shell_from(Some("/usr/bin/fish"));
        assert_eq!(info.bin_name, "fish");
        assert_eq!(info.full_path, "/usr/bin/fish");
        assert_eq!(info.package_name, "fish");
        assert!(info.detected);
    }

    #[test]
    fn detect_zsh_from_path() {
        let info = detect_host_shell_from(Some("/bin/zsh"));
        assert_eq!(info.bin_name, "zsh");
        assert!(info.detected);
    }

    #[test]
    fn fallback_on_dash() {
        let info = detect_host_shell_from(Some("/bin/dash"));
        assert_eq!(info.bin_name, "fish");
        assert!(!info.detected);
    }

    #[test]
    fn fallback_on_empty_shell() {
        let info = detect_host_shell_from(None);
        assert_eq!(info.bin_name, "fish");
        assert!(!info.detected);
    }

    #[test]
    fn fallback_on_sh() {
        let info = detect_host_shell_from(Some("/bin/sh"));
        assert_eq!(info.bin_name, "fish");
        assert!(!info.detected);
    }

    #[test]
    fn nushell_binary_maps_to_nushell_package() {
        let info = detect_host_shell_from(Some("/usr/bin/nu"));
        assert_eq!(info.bin_name, "nu");
        assert_eq!(info.package_name, "nushell");
    }

    #[test]
    fn shell_info_unknown_binary() {
        let info = shell_info_from_bin("tcsh");
        assert_eq!(info.bin_name, "tcsh");
        assert_eq!(info.full_path, "/usr/bin/tcsh");
        assert_eq!(info.package_name, "tcsh");
    }

    #[test]
    fn detect_host_shell_is_idempotent() {
        let a = detect_host_shell_from(Some("/usr/bin/fish"));
        let b = detect_host_shell_from(Some("/usr/bin/fish"));
        assert_eq!(a.bin_name, b.bin_name);
        assert_eq!(a.full_path, b.full_path);
        assert_eq!(a.package_name, b.package_name);
    }

    #[test]
    fn apply_shell_adds_package_when_missing() {
        let toml = r#"
[image]
base = "fedora:41"
name = "testenv"
packages = { install = ["fastfetch"] }
[container]
name = "testenv"
home = "~/containers/testenv"
"#;
        let mut cfg: crate::config::Config = toml::from_str(toml).unwrap();
        let shell = ShellInfo {
            bin_name: "zsh".into(),
            full_path: "/bin/zsh".into(),
            package_name: "zsh".into(),
            detected: true,
        };
        apply_shell_defaults(&mut cfg, &shell);
        assert!(cfg.image.packages.install.contains(&"zsh".to_string()));
        assert_eq!(
            cfg.container.shell, "fish",
            "should not override existing shell"
        );
    }

    #[test]
    fn apply_shell_fills_empty_shell() {
        let toml = r#"
[image]
base = "fedora:41"
name = "testenv"
[container]
name = "testenv"
home = "~/containers/testenv"
"#;
        let mut cfg: crate::config::Config = toml::from_str(toml).unwrap();
        cfg.container.shell.clear();
        let shell = ShellInfo {
            bin_name: "zsh".into(),
            full_path: "/bin/zsh".into(),
            package_name: "zsh".into(),
            detected: true,
        };
        apply_shell_defaults(&mut cfg, &shell);
        assert_eq!(cfg.container.shell, "/bin/zsh", "should fill empty shell");
        assert!(cfg.image.packages.install.contains(&"zsh".to_string()));
    }

    #[test]
    fn apply_shell_no_duplicate_when_present() {
        let toml = r#"
[image]
base = "fedora:41"
name = "testenv"
packages = { install = ["fish", "fastfetch"] }
[container]
name = "testenv"
home = "~/containers/testenv"
"#;
        let mut cfg: crate::config::Config = toml::from_str(toml).unwrap();
        let shell = ShellInfo {
            bin_name: "fish".into(),
            full_path: "/usr/bin/fish".into(),
            package_name: "fish".into(),
            detected: true,
        };
        apply_shell_defaults(&mut cfg, &shell);
        let fish_count = cfg
            .image
            .packages
            .install
            .iter()
            .filter(|s| s.as_str() == "fish")
            .count();
        assert_eq!(fish_count, 1);
    }

    #[test]
    fn tty_guard_logic_is_correct() {
        // is_tty() is tested in codegen::distros — placeholder kept for symmetry
    }
}
