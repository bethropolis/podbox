#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroFamily {
    DebianLike,
    FedoraLike,
    ArchLike,
    AlpineLike,
    SuseLike,
    Unknown,
}

impl DistroFamily {
    /// Detect the distro family from an image reference by matching whole
    /// tokens, not substrings: only the last repository path segment and the
    /// tag are considered, and a token matches a distro name exactly or as a
    /// prefix followed by a separator (`-`, `_`, `.`) or version digit.
    /// Substring matching misfires on names like `monarch-tools` (contains
    /// `arch`) and misses refs like `podbox:dev-latest` entirely.
    pub fn from_base_image(base: &str) -> Self {
        let base_lower = base.to_lowercase();
        // Split off the tag: the last colon starts one unless a slash
        // follows it (a `registry:port/` prefix, not a tag).
        let (repo, tag) = match base_lower.rsplit_once(':') {
            Some((repo, tag)) if !tag.contains('/') => (repo, tag),
            _ => (base_lower.as_str(), ""),
        };
        let last_segment = repo.rsplit('/').next().unwrap_or("");
        for token in [last_segment, tag] {
            if token.is_empty() {
                continue;
            }
            if let Some(family) = match_distro_token(token) {
                return family;
            }
        }
        Self::Unknown
    }
}

/// Match one token (last path segment or tag) against known distro names:
/// exact, or a prefix followed by a separator or version digit, so
/// `ubuntu-22.04` and `mint22` match while `popular` and `monarch` do not.
fn match_distro_token(token: &str) -> Option<DistroFamily> {
    const NAMES: &[(&str, DistroFamily)] = &[
        ("debian", DistroFamily::DebianLike),
        ("ubuntu", DistroFamily::DebianLike),
        ("linuxmint", DistroFamily::DebianLike),
        ("mint", DistroFamily::DebianLike),
        ("kali", DistroFamily::DebianLike),
        ("pop", DistroFamily::DebianLike),
        ("elementary", DistroFamily::DebianLike),
        ("fedora", DistroFamily::FedoraLike),
        ("rhel", DistroFamily::FedoraLike),
        ("centos", DistroFamily::FedoraLike),
        ("rocky", DistroFamily::FedoraLike),
        ("alma", DistroFamily::FedoraLike),
        ("nobara", DistroFamily::FedoraLike),
        ("arch", DistroFamily::ArchLike),
        ("archlinux", DistroFamily::ArchLike),
        ("cachy", DistroFamily::ArchLike),
        ("cachyos", DistroFamily::ArchLike),
        ("manjaro", DistroFamily::ArchLike),
        ("endeavouros", DistroFamily::ArchLike),
        ("garuda", DistroFamily::ArchLike),
        ("alpine", DistroFamily::AlpineLike),
        ("opensuse", DistroFamily::SuseLike),
        ("suse", DistroFamily::SuseLike),
        ("tumbleweed", DistroFamily::SuseLike),
        ("leap", DistroFamily::SuseLike),
    ];
    NAMES.iter().find_map(|(name, family)| {
        if token == *name {
            return Some(*family);
        }
        token
            .strip_prefix(name)
            .filter(|rest| {
                rest.starts_with(['-', '_', '.']) || rest.starts_with(|c: char| c.is_ascii_digit())
            })
            .map(|_| *family)
    })
}

/// The single `PackageManager` → `DistroFamily` mapping. Both the prebuilt
/// overlay path and the drift diff go through this, so hand-written
/// copies can no longer disagree (zypper used to map to `SuseLike` in one
/// and `FedoraLike` in the other).
impl From<crate::config::PackageManager> for DistroFamily {
    fn from(manager: crate::config::PackageManager) -> Self {
        use crate::config::PackageManager as PM;
        match manager {
            PM::Apt => Self::DebianLike,
            PM::Dnf => Self::FedoraLike,
            PM::Pacman => Self::ArchLike,
            PM::Apk => Self::AlpineLike,
            PM::Zypper => Self::SuseLike,
        }
    }
}

impl DistroFamily {
    /// Package manager for the family.
    pub fn manager(&self) -> crate::config::PackageManager {
        match self {
            Self::DebianLike => crate::config::PackageManager::Apt,
            Self::FedoraLike => crate::config::PackageManager::Dnf,
            Self::ArchLike => crate::config::PackageManager::Pacman,
            Self::AlpineLike => crate::config::PackageManager::Apk,
            Self::SuseLike => crate::config::PackageManager::Zypper,
            Self::Unknown => crate::config::PackageManager::Dnf,
        }
    }

    pub fn install_cmd(&self) -> &'static str {
        match self {
            Self::DebianLike => "apt-get update && apt-get install -y --no-install-recommends",
            Self::FedoraLike => "dnf install -y",
            Self::ArchLike => "pacman -Syu --noconfirm",
            Self::AlpineLike => "apk add --no-cache",
            Self::SuseLike => "zypper install -y",
            Self::Unknown => "dnf install -y",
        }
    }

    pub fn clean_cmd(&self) -> &'static str {
        match self {
            Self::DebianLike => "rm -rf /var/lib/apt/lists/*",
            Self::FedoraLike => "dnf clean all",
            Self::ArchLike => "pacman -Scc --noconfirm",
            Self::AlpineLike => "",
            Self::SuseLike => "zypper clean --all",
            Self::Unknown => "dnf clean all",
        }
    }

    pub fn base_packages(&self, host_shell: Option<&str>) -> Vec<String> {
        let common: Vec<String> = [
            "sudo",
            "curl",
            "tar",
            "unzip",
            "wget",
            "which",
            "coreutils",
            "diffutils",
            "findutils",
            "grep",
            "sed",
            "gawk",
            "bash-completion",
        ]
        .into_iter()
        .map(String::from)
        .collect();

        let mut pkgs = common;

        if let Some(shell) = host_shell {
            let shell_pkgs = Self::shell_packages(self, shell);
            for pkg in shell_pkgs {
                if !pkgs.contains(&pkg) {
                    pkgs.push(pkg);
                }
            }
        }

        pkgs
    }

    /// Packages needed to provide `shell_path` (accepts `fish` or
    /// `/usr/bin/fish`). Public because the image has to contain the shell the
    /// container runs, not just the host's.
    pub fn shell_packages(&self, shell_path: &str) -> Vec<String> {
        let shell_name = shell_path.split('/').next_back().unwrap_or("");
        let mut pkgs = Vec::new();

        match shell_name {
            "bash" => {
                pkgs.push("bash".into());
                pkgs.push("bash-completion".into());
            }
            "zsh" => {
                pkgs.push("zsh".into());
                match self {
                    Self::DebianLike => pkgs.push("zsh-common".into()),
                    Self::FedoraLike => pkgs.push("zsh".into()),
                    Self::ArchLike => pkgs.push("zsh-completions".into()),
                    Self::AlpineLike => pkgs.push("zsh".into()),
                    Self::SuseLike => pkgs.push("zsh".into()),
                    Self::Unknown => pkgs.push("zsh".into()),
                }
            }
            "fish" => {
                pkgs.push("fish".into());
            }
            "sh" | "dash" => match self {
                Self::DebianLike => pkgs.push("dash".into()),
                Self::FedoraLike => pkgs.push("dash".into()),
                Self::ArchLike => pkgs.push("dash".into()),
                Self::AlpineLike => pkgs.push("dash".into()),
                Self::SuseLike => pkgs.push("dash".into()),
                Self::Unknown => pkgs.push("dash".into()),
            },
            _ => {}
        }

        pkgs
    }

    pub fn remove_cmd(&self, packages: &[String]) -> String {
        if packages.is_empty() {
            return String::new();
        }
        let pkgs = packages.join(" ");
        match self {
            Self::DebianLike => {
                format!(
                    "apt-get purge -y {} && apt-get autoremove -y && {}",
                    pkgs,
                    Self::clean_cmd(self)
                )
            }
            Self::FedoraLike => {
                format!("dnf remove -y {} && {}", pkgs, Self::clean_cmd(self))
            }
            Self::ArchLike => {
                format!(
                    "pacman -Rns --noconfirm {} && {}",
                    pkgs,
                    Self::clean_cmd(self)
                )
            }
            Self::AlpineLike => {
                format!("apk del {} && {}", pkgs, Self::clean_cmd(self))
            }
            Self::SuseLike => {
                format!("zypper rm {} && {}", pkgs, Self::clean_cmd(self))
            }
            Self::Unknown => {
                format!("dnf remove -y {} && {}", pkgs, Self::clean_cmd(self))
            }
        }
    }

    pub fn locale_packages(&self) -> Vec<String> {
        match self {
            Self::DebianLike => vec!["locales".into()],
            Self::FedoraLike => vec!["glibc-all-langpacks".into()],
            Self::ArchLike => vec!["glibc".into()],
            Self::AlpineLike => vec!["musl-locales".into()],
            Self::SuseLike => vec!["glibc-all-langpacks".into()],
            Self::Unknown => vec!["locales".into()],
        }
    }
}

pub fn detect_package_manager(image: &str) -> crate::config::PackageManager {
    DistroFamily::from_base_image(image).manager()
}

/// Resolve the distro family for package installation: an explicit
/// `image.packages.manager` always wins over detection from the image
/// reference (or base name), so bundled profiles state theirs outright and
/// never depend on ref guessing.
pub fn family_for_config(config: &crate::config::Config) -> DistroFamily {
    if let Some(manager) = config.image.packages.manager {
        return DistroFamily::from(manager);
    }
    let probe = config
        .image
        .image_ref
        .as_deref()
        .unwrap_or(&config.image.base);
    DistroFamily::from_base_image(probe)
}

/// True when stdin is a TTY. Always false off-host (wasm): there is no
/// controlling terminal, and the `nix` dependency is `cli`-gated.
#[cfg(feature = "cli")]
pub fn is_tty() -> bool {
    nix::unistd::isatty(std::io::stdin()).unwrap_or(false)
}

#[cfg(not(feature = "cli"))]
pub fn is_tty() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_distro_from_base_image() {
        assert_eq!(
            DistroFamily::from_base_image("debian:12"),
            DistroFamily::DebianLike
        );
        assert_eq!(
            DistroFamily::from_base_image("ubuntu:26.04"),
            DistroFamily::DebianLike
        );
        assert_eq!(
            DistroFamily::from_base_image("fedora:41"),
            DistroFamily::FedoraLike
        );
        assert_eq!(
            DistroFamily::from_base_image("cachy-latest"),
            DistroFamily::ArchLike
        );
        assert_eq!(
            DistroFamily::from_base_image("archlinux:latest"),
            DistroFamily::ArchLike
        );
        assert_eq!(
            DistroFamily::from_base_image("docker.io/library/archlinux:latest"),
            DistroFamily::ArchLike
        );
        assert_eq!(
            DistroFamily::from_base_image("alpine:3.20"),
            DistroFamily::AlpineLike
        );
        assert_eq!(
            DistroFamily::from_base_image("opensuse/tumbleweed:latest"),
            DistroFamily::SuseLike
        );
        assert_eq!(
            DistroFamily::from_base_image("registry.fedoraproject.org/fedora:44"),
            DistroFamily::FedoraLike
        );
        assert_eq!(
            DistroFamily::from_base_image("ubuntu:24.04"),
            DistroFamily::DebianLike
        );
        assert_eq!(
            DistroFamily::from_base_image("unknown:latest"),
            DistroFamily::Unknown
        );
    }

    #[test]
    fn test_distro_detection_uses_tokens_not_substrings() {
        // `arch` inside `monarch` and `pop` inside `popular` must not match;
        // the bundled dev ref matches nothing and stays Unknown (its profile
        // pins the manager explicitly instead).
        assert_eq!(
            DistroFamily::from_base_image("ghcr.io/x/monarch-tools:1"),
            DistroFamily::Unknown
        );
        assert_eq!(
            DistroFamily::from_base_image("ghcr.io/x/popular-base:1"),
            DistroFamily::Unknown
        );
        assert_eq!(
            DistroFamily::from_base_image("ghcr.io/bethropolis/podbox:dev-latest"),
            DistroFamily::Unknown
        );
        // A registry port colon is not a tag separator.
        assert_eq!(
            DistroFamily::from_base_image("localhost:5000/mybox:latest"),
            DistroFamily::Unknown
        );
        assert_eq!(
            DistroFamily::from_base_image("localhost:5000/fedora-box:41"),
            DistroFamily::FedoraLike
        );
    }

    #[test]
    fn test_package_manager_maps_to_one_family() {
        // The single shared mapping: zypper is SuseLike everywhere
        // (the drift diff used to say FedoraLike).
        use crate::config::PackageManager as PM;
        for (manager, family) in [
            (PM::Apt, DistroFamily::DebianLike),
            (PM::Dnf, DistroFamily::FedoraLike),
            (PM::Pacman, DistroFamily::ArchLike),
            (PM::Apk, DistroFamily::AlpineLike),
            (PM::Zypper, DistroFamily::SuseLike),
        ] {
            assert_eq!(DistroFamily::from(manager), family);
        }
    }

    #[test]
    fn test_explicit_zypper_manager_resolves_suse() {
        // The prebuilt overlay path: an explicit zypper manager means
        // SuseLike, even for an undetectable ref.
        let cfg = crate::config::Config::parse(
            r#"
[image]
base = "ghcr.io/x/custom:1"
name = "env"
[image.packages]
manager = "zypper"
[container]
name = "env"
home = "~/env"
"#,
        )
        .unwrap();
        assert_eq!(family_for_config(&cfg), DistroFamily::SuseLike);
    }

    #[test]
    fn test_explicit_manager_wins_over_detection() {
        // The bundled dev profile's ref is undetectable, but it pins pacman.
        let dev = crate::config::Config::parse(include_str!("../profiles/dev.toml")).unwrap();
        assert_eq!(family_for_config(&dev), DistroFamily::ArchLike);
        let cachy = crate::config::Config::parse(include_str!("../profiles/cachy.toml")).unwrap();
        let fedora = crate::config::Config::parse(include_str!("../profiles/fedora.toml")).unwrap();
        let ubuntu = crate::config::Config::parse(include_str!("../profiles/ubuntu.toml")).unwrap();
        assert_eq!(family_for_config(&cachy), DistroFamily::ArchLike);
        assert_eq!(family_for_config(&fedora), DistroFamily::FedoraLike);
        assert_eq!(family_for_config(&ubuntu), DistroFamily::DebianLike);
    }

    #[test]
    fn test_unknown_family_with_packages_is_rejected() {
        let err = crate::config::Config::parse(
            r#"
[image]
base = "ghcr.io/x/monarch-tools:1"
name = "env"
[image.packages]
install = ["htop"]
[container]
name = "env"
home = "~/env"
"#,
        )
        .expect_err("unknown family with packages must fail validation");
        assert!(
            err.to_string().contains("cannot determine package manager"),
            "{err}"
        );
    }

    #[test]
    fn test_debian_base_packages() {
        let pkgs = DistroFamily::DebianLike.base_packages(Some("/usr/bin/fish"));
        assert!(pkgs.contains(&"sudo".into()));
        assert!(pkgs.contains(&"curl".into()));
        assert!(pkgs.contains(&"fish".into()));
    }

    #[test]
    fn test_fedora_base_packages() {
        let pkgs = DistroFamily::FedoraLike.base_packages(Some("/usr/bin/zsh"));
        assert!(pkgs.contains(&"sudo".into()));
        assert!(pkgs.contains(&"zsh".into()));
        assert!(pkgs.contains(&"zsh-completions".into()) || pkgs.contains(&"zsh".into()));
    }

    #[test]
    fn test_arch_base_packages() {
        let pkgs = DistroFamily::ArchLike.base_packages(Some("/bin/bash"));
        assert!(pkgs.contains(&"sudo".into()));
        assert!(pkgs.contains(&"bash".into()));
        assert!(pkgs.contains(&"bash-completion".into()));
    }

    #[test]
    fn test_alpine_base_packages() {
        let pkgs = DistroFamily::AlpineLike.base_packages(None);
        assert!(pkgs.contains(&"sudo".into()));
        assert!(pkgs.contains(&"curl".into()));
    }

    #[test]
    fn test_install_cmd() {
        assert_eq!(
            DistroFamily::DebianLike.install_cmd(),
            "apt-get update && apt-get install -y --no-install-recommends"
        );
        assert_eq!(DistroFamily::FedoraLike.install_cmd(), "dnf install -y");
        assert_eq!(
            DistroFamily::ArchLike.install_cmd(),
            "pacman -Syu --noconfirm"
        );
        assert_eq!(DistroFamily::AlpineLike.install_cmd(), "apk add --no-cache");
        assert_eq!(DistroFamily::SuseLike.install_cmd(), "zypper install -y");
    }
}
