use crate::codegen::distros::{DistroFamily, detect_host_locale, detect_host_shell};
use crate::config::Config;
use crate::error::PodboxError;

/// Marker emitted in place of the guest `COPY` when no guest binary is
/// embedded. Keeps the preview honest about what the real build adds.
const GUEST_PLACEHOLDER: &str = "\
# --- podbox-guest: not embedded in this build (browser preview) ---
# A real build bakes it in (`podbox build` on Linux); without it the image
# cannot start, so `podbox build` on a non-musl host fails outright.";

/// Render the Containerfile for a real build.
///
/// A custom build without an embedded guest binary is an error here: the
/// image it would produce cannot start, and [`crate::build`] would fail on
/// the missing bytes anyway. Preview callers that cannot embed a guest
/// (the wasm engine behind Studio) use [`generate_preview`] instead.
pub fn generate(config: &Config, guest_binary_name: &str) -> Result<String, PodboxError> {
    if config.image.source().is_prebuilt() {
        // Prebuilt registry images already embed a guest binary; the local
        // overlay path (`build::prebuilt`) layers the host guest on top when
        // one is embedded in this binary. Keep this minimal so pulls stay
        // cache-friendly.
        return Ok(generate_prebuilt(config));
    }
    if crate::guest::PODBOX_GUEST.is_none() {
        return Err(PodboxError::GuestBinaryUnavailable);
    }
    Ok(generate_custom_with(config, guest_binary_name, true))
}

/// Render the Containerfile for preview, never failing on a missing guest.
///
/// The wasm build has no guest binary to embed, so the recipe is still worth
/// showing: packages, RUN steps, locale and ENV are all accurate. What the
/// preview cannot show is the guest layer, so it is replaced by
/// [`GUEST_PLACEHOLDER`].
///
/// That placeholder is deliberately not reported as a warning: it is a fact
/// about the preview environment rather than anything wrong with the config,
/// and it is plainly visible in the rendered output.
pub fn generate_preview(config: &Config, guest_binary_name: &str) -> String {
    if config.image.source().is_prebuilt() {
        return generate_prebuilt(config);
    }
    generate_custom_with(
        config,
        guest_binary_name,
        crate::guest::PODBOX_GUEST.is_some(),
    )
}

fn generate_prebuilt(config: &Config) -> String {
    let builder = ContainerfileBuilder::new(&config.image.base, &config.container.name);
    builder
        .add_user_packages(config.image.packages.install.clone())
        .add_run_commands(config.image.run.commands.clone())
        .set_shell(&config.container.shell)
        .build()
}

/// Shared body of the custom-build recipe.
///
/// `guest_available` decides whether the guest `COPY`/`chmod` pair is
/// emitted or replaced by [`GUEST_PLACEHOLDER`].
fn generate_custom_with(config: &Config, guest_name: &str, guest_available: bool) -> String {
    let distro = DistroFamily::from_base_image(&config.image.base);
    let host_shell = detect_host_shell();
    let host_locale = detect_host_locale();

    let builder = ContainerfileBuilder::new(&config.image.base, &config.container.name);
    builder
        .add_base_packages(distro, host_shell.as_deref(), host_locale.as_deref())
        // The image must contain the shell the container actually runs. Only
        // the host's shell was installed above, so a config asking for a
        // different one (podbox's own default is `fish`) produced `CMD ["fish"]`
        // on an image without fish, and the container failed to start a shell.
        .add_shell_packages(distro, &config.container.shell)
        .add_user_packages(config.image.packages.install.clone())
        .add_run_commands(config.image.run.commands.clone())
        .set_guest(guest_name, guest_available)
        .set_shell(&config.container.shell)
        .build()
}

struct ContainerfileBuilder {
    base_image: String,
    container_name: String,
    packages: Vec<String>,
    run_commands: Vec<String>,
    guest_name: String,
    has_guest_binary: bool,
    env_vars: Vec<(String, String)>,
    forced_shell: Option<String>,
}

impl ContainerfileBuilder {
    fn new(base_image: &str, container_name: &str) -> Self {
        Self {
            base_image: base_image.to_string(),
            container_name: container_name.to_string(),
            packages: Vec::new(),
            run_commands: Vec::new(),
            guest_name: "podbox-guest".to_string(),
            has_guest_binary: false,
            env_vars: Vec::new(),
            forced_shell: None,
        }
    }

    fn add_base_packages(
        mut self,
        distro: DistroFamily,
        host_shell: Option<&str>,
        host_locale: Option<&str>,
    ) -> Self {
        let mut pkgs = distro.base_packages(host_shell);
        let locale_pkgs = distro.locale_packages();
        for pkg in locale_pkgs {
            if !pkgs.contains(&pkg) {
                pkgs.push(pkg);
            }
        }
        self.packages = pkgs;
        if let Some(locale) = host_locale {
            self.env_vars.push(("LANG".into(), locale.to_string()));
            self.env_vars.push(("LC_ALL".into(), locale.to_string()));
            self.env_vars.push(("LC_CTYPE".into(), locale.to_string()));
        }
        self
    }

    fn add_shell_packages(mut self, distro: DistroFamily, shell: &str) -> Self {
        for pkg in distro.shell_packages(shell) {
            if !self.packages.contains(&pkg) {
                self.packages.push(pkg);
            }
        }
        self
    }

    fn add_user_packages(mut self, pkgs: Vec<String>) -> Self {
        for pkg in pkgs {
            if !self.packages.contains(&pkg) {
                self.packages.push(pkg);
            }
        }
        self
    }

    fn add_run_commands(mut self, cmds: Vec<String>) -> Self {
        self.run_commands = cmds;
        self
    }

    fn set_guest(mut self, guest_name: &str, available: bool) -> Self {
        self.guest_name = guest_name.to_string();
        self.has_guest_binary = available;
        self
    }

    fn set_shell(mut self, shell: &str) -> Self {
        self.forced_shell = Some(shell.to_string());
        self
    }

    fn build(self) -> String {
        let distro = DistroFamily::from_base_image(&self.base_image);
        let mut lines = Vec::new();

        lines.push(format!("FROM {}", self.base_image));
        lines.push(String::new());

        if !self.packages.is_empty() {
            let pkgs = self.packages.join(" ");
            let clean = distro.clean_cmd();
            let cmd = if clean.is_empty() {
                format!("{} {}", distro.install_cmd(), pkgs)
            } else {
                format!("{} {} && {}", distro.install_cmd(), pkgs, clean)
            };
            lines.push(format!("RUN {cmd}"));
            lines.push(String::new());
        }

        for cmd in &self.run_commands {
            lines.push(format!("RUN {cmd}"));
        }
        if !self.run_commands.is_empty() {
            lines.push(String::new());
        }

        if let Some(locale) = self
            .env_vars
            .iter()
            .find(|(k, _)| k == "LANG")
            .map(|(_, v)| v.as_str())
        {
            match distro {
                DistroFamily::DebianLike | DistroFamily::ArchLike => {
                    let (name, charset) = locale.split_once('.').unwrap_or((locale, "UTF-8"));
                    lines.push(format!(
                        "RUN localedef -i {name} -f {charset} {locale} || true"
                    ));
                    lines.push(String::new());
                }
                DistroFamily::FedoraLike | DistroFamily::SuseLike => {
                    // glibc-all-langpacks includes pre-generated locales, no localedef needed
                }
                DistroFamily::AlpineLike | DistroFamily::Unknown => {}
            }
        }

        if self.has_guest_binary {
            lines.push(format!(
                "COPY {} /usr/local/bin/{}",
                self.guest_name, self.guest_name
            ));
            lines.push(format!("RUN chmod +x /usr/local/bin/{}", self.guest_name));
            lines.push(String::new());
        } else {
            lines.push(GUEST_PLACEHOLDER.to_string());
            lines.push(String::new());
        }

        for (key, value) in &self.env_vars {
            lines.push(format!("ENV {key}={value}"));
        }

        lines.push(format!("ENV PODBOX_CONTAINER={}", self.container_name));
        lines.push(format!("ENV PODBOX_HOST_VERSION={}", crate::VERSION));
        lines.push(String::new());

        lines.push(format!(
            "ENTRYPOINT [\"/usr/local/bin/{}\", \"--entry\"]",
            self.guest_name
        ));
        lines.push(format!("CMD [\"{}\"]", self.default_shell()));
        lines.push(String::new());

        lines.join("\n")
    }

    fn default_shell(&self) -> &str {
        if let Some(ref shell) = self.forced_shell {
            return shell;
        }
        self.packages
            .iter()
            .find_map(|p| match p.as_str() {
                "fish" => Some("fish"),
                "zsh" => Some("zsh"),
                "bash" => Some("bash"),
                _ => None,
            })
            .unwrap_or("fish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::distros::DistroFamily;

    #[test]
    fn test_builder_debian() {
        let builder = ContainerfileBuilder::new("debian:12", "test").add_base_packages(
            DistroFamily::DebianLike,
            Some("/usr/bin/fish"),
            Some("en_US.UTF-8"),
        );
        let cf = builder.build();
        assert!(cf.contains("apt-get update"));
        assert!(cf.contains("sudo"));
        assert!(cf.contains("fish"));
        assert!(cf.contains("locales"));
        assert!(cf.contains("ENV LANG=en_US.UTF-8"));
        assert!(cf.contains("localedef -i en_US -f UTF-8 en_US.UTF-8"));
        assert!(cf.contains("ENV PODBOX_CONTAINER=test"));
    }

    #[test]
    fn test_builder_fedora() {
        let builder = ContainerfileBuilder::new("fedora:41", "test").add_base_packages(
            DistroFamily::FedoraLike,
            Some("/usr/bin/zsh"),
            None,
        );
        let cf = builder.build();
        assert!(cf.contains("dnf install -y"));
        assert!(cf.contains("sudo"));
        assert!(cf.contains("zsh"));
        assert!(cf.contains("ENV PODBOX_CONTAINER=test"));
        // Fedora uses glibc-all-langpacks (no localedef needed)
        assert!(!cf.contains("localedef"));
    }

    #[test]
    fn test_builder_arch() {
        let builder = ContainerfileBuilder::new("archlinux:latest", "test").add_base_packages(
            DistroFamily::ArchLike,
            Some("/bin/bash"),
            None,
        );
        let cf = builder.build();
        assert!(cf.contains("pacman -Syu --noconfirm"));
        assert!(cf.contains("bash"));
        assert!(cf.contains("bash-completion"));
        assert!(cf.contains("ENV PODBOX_CONTAINER=test"));
        // No locale requested, so no localedef
        assert!(!cf.contains("localedef"));
    }

    #[test]
    fn test_builder_arch_with_locale() {
        let builder = ContainerfileBuilder::new("archlinux:latest", "test").add_base_packages(
            DistroFamily::ArchLike,
            Some("/bin/bash"),
            Some("en_US.UTF-8"),
        );
        let cf = builder.build();
        assert!(cf.contains("localedef -i en_US -f UTF-8 en_US.UTF-8"));
    }

    #[test]
    fn test_builder_alpine() {
        let builder = ContainerfileBuilder::new("alpine:3.20", "test").add_base_packages(
            DistroFamily::AlpineLike,
            None,
            None,
        );
        let cf = builder.build();
        assert!(cf.contains("apk add --no-cache"));
        assert!(cf.contains("sudo"));
        assert!(cf.contains("ENV PODBOX_CONTAINER=test"));
    }

    /// The browser preview has no guest binary to embed: the recipe must
    /// still render, with the guest layer marked rather than silently dropped.
    #[test]
    fn test_preview_without_guest_marks_the_guest_layer() {
        let cf = generate_custom_with(&crate::config::Config::embedded(), "podbox-guest", false);
        assert!(cf.contains("FROM "));
        assert!(cf.contains("RUN "));
        assert!(cf.contains(GUEST_PLACEHOLDER));
        assert!(!cf.contains("COPY podbox-guest"));
        // The entrypoint still points at the guest: the preview shows the
        // real recipe, and the comment explains the missing layer.
        assert!(cf.contains("ENTRYPOINT [\"/usr/local/bin/podbox-guest\", \"--entry\"]"));
    }

    /// podbox's default shell is `fish`, but the image only ever installed the
    /// *host's* shell package. Configuring a shell the host does not use
    /// produced a `CMD` pointing at a binary that was never installed.
    #[test]
    fn test_configured_shell_is_installed_regardless_of_host_shell() {
        let mut config = crate::config::Config::embedded();
        config.container.shell = "fish".into();
        let cf = generate_custom_with(&config, "podbox-guest", true);
        assert!(
            cf.lines()
                .any(|l| l.starts_with("RUN ") && l.contains(" fish")),
            "fish must be installed:\n{cf}"
        );
        assert!(cf.contains("CMD [\"fish\"]"), "{cf}");

        // A shell the host also runs stays installed exactly once.
        let mut config = crate::config::Config::embedded();
        config.container.shell = "zsh".into();
        let cf = generate_custom_with(&config, "podbox-guest", true);
        assert!(cf.contains("CMD [\"zsh\"]"), "{cf}");
        let install_line = cf
            .lines()
            .find(|l| l.starts_with("RUN ") && l.contains("install"))
            .unwrap();
        assert_eq!(
            install_line.matches(" zsh").count(),
            1,
            "zsh listed twice: {install_line}"
        );
    }

    #[test]
    fn test_guest_layer_present_when_available() {
        let cf = generate_custom_with(&crate::config::Config::embedded(), "podbox-guest", true);
        assert!(cf.contains("COPY podbox-guest /usr/local/bin/podbox-guest"));
        assert!(cf.contains("RUN chmod +x /usr/local/bin/podbox-guest"));
        assert!(!cf.contains(GUEST_PLACEHOLDER));
    }
}
