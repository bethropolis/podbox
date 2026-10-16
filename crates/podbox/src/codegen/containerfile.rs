use crate::codegen::distros::{DistroFamily, family_for_config};
use crate::config::Config;
use crate::env::HostEnv;
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
///
/// Host facts (shell, locale) come from `env`: codegen is pure
/// (config in, string out) and never reads the process environment.
pub fn generate(
    config: &Config,
    guest_binary_name: &str,
    env: &HostEnv,
) -> Result<String, PodboxError> {
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
    Ok(generate_custom_with(config, guest_binary_name, true, env))
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
pub fn generate_preview(config: &Config, guest_binary_name: &str, env: &HostEnv) -> String {
    if config.image.source().is_prebuilt() {
        return generate_prebuilt(config);
    }
    generate_custom_with(
        config,
        guest_binary_name,
        crate::guest::PODBOX_GUEST.is_some(),
        env,
    )
}

fn generate_prebuilt(config: &Config) -> String {
    let base_ref = config
        .image
        .image_ref
        .as_deref()
        .unwrap_or(&config.image.base);
    generate_prebuilt_overlay(config, base_ref, crate::guest::PODBOX_GUEST.is_some())
        .unwrap_or_else(|| format!("FROM {base_ref}\n"))
}

/// True when a prebuilt image needs a local overlay: extra packages or RUN
/// steps to layer. The guest binary rides along with an overlay that exists
/// for other reasons; it never triggers one on its own (the registry image
/// already embeds a guest, and version skew is reported at build and checked
/// at runtime).
pub fn prebuilt_overlay_needed(config: &Config) -> bool {
    !config.image.packages.install.is_empty() || !config.image.run.commands.is_empty()
}

/// Generate the overlay Containerfile layered onto a prebuilt image.
///
/// Emits `FROM <resolved ref>`, the package install step via the shared
/// installer, the configured `RUN` commands, and the guest layer when the
/// host binary embeds one. Returns `None` when there is truly nothing to
/// add. The CLI build and the Studio preview share this function so the
/// preview shows what is actually built.
pub fn generate_prebuilt_overlay(
    config: &Config,
    base_ref: &str,
    guest_available: bool,
) -> Option<String> {
    if !prebuilt_overlay_needed(config) {
        return None;
    }
    let distro = family_for_config(config);
    let mut lines = vec![format!("FROM {base_ref}"), String::new()];
    if !config.image.packages.install.is_empty() {
        let pkgs = config.image.packages.install.join(" ");
        if let Some(layer) = install_layer(distro, base_ref, &pkgs) {
            lines.push(layer);
            lines.push(String::new());
        }
    }
    for cmd in &config.image.run.commands {
        lines.push(format!("RUN {cmd}"));
    }
    if !config.image.run.commands.is_empty() {
        lines.push(String::new());
    }
    if guest_available {
        lines.push("COPY podbox-guest /usr/local/bin/podbox-guest".into());
        lines.push("RUN chmod +x /usr/local/bin/podbox-guest".into());
        lines.push(String::new());
    }
    lines.push(format!("ENV PODBOX_CONTAINER={}", config.container.name));
    lines.push(format!("ENV PODBOX_HOST_VERSION={}", crate::VERSION));
    lines.push(String::new());
    lines.push("ENTRYPOINT [\"/usr/local/bin/podbox-guest\", \"--entry\"]".to_string());
    lines.push(format!("CMD [\"{}\"]", config.container.shell));
    lines.push(String::new());
    Some(lines.join("\n"))
}

/// Shared package-install layer for custom builds and prebuilt overlays
/// alike: one `RUN` with the family install command, the CachyOS hook-sandbox
/// relaxation, and the cache clean. `None` when there is nothing to install.
pub(crate) fn install_layer(distro: DistroFamily, base_image: &str, pkgs: &str) -> Option<String> {
    if pkgs.trim().is_empty() {
        return None;
    }
    let clean = distro.clean_cmd();
    if needs_hook_sandbox_relaxation(distro, base_image) {
        let mut cmd = String::from(
            "sed 's/^\\[options\\]/[options]\\nDisableSandboxNetwork/' /etc/pacman.conf > /tmp/podbox-pacman.conf",
        );
        use std::fmt::Write as _;
        let _ = write!(
            cmd,
            " && {} --config /tmp/podbox-pacman.conf {}",
            distro.install_cmd(),
            pkgs
        );
        cmd.push_str(" && rm -f /tmp/podbox-pacman.conf");
        if !clean.is_empty() {
            let _ = write!(cmd, " && {clean}");
        }
        return Some(format!("RUN {cmd}"));
    }
    let body = if clean.is_empty() {
        format!("{} {}", distro.install_cmd(), pkgs)
    } else {
        format!("{} {} && {}", distro.install_cmd(), pkgs, clean)
    };
    Some(format!("RUN {body}"))
}

/// True when the base needs CachyOS's hook sandbox lifted for installs.
///
/// CachyOS only, not vanilla Arch: its pacman is patched with a
/// network-isolation hook sandbox that rootless userns cannot grant, so
/// package installs fail their hooks there. Vanilla Arch's pacman lacks
/// the toggle entirely (an unrecognized directive only warns), hence the
/// base-name gate instead of a family gate.
fn needs_hook_sandbox_relaxation(distro: DistroFamily, base_image: &str) -> bool {
    distro == DistroFamily::ArchLike && base_image.to_lowercase().contains("cachy")
}

/// Shared body of the custom-build recipe.
///
/// `guest_available` decides whether the guest `COPY`/`chmod` pair is
/// emitted or replaced by [`GUEST_PLACEHOLDER`].
fn generate_custom_with(
    config: &Config,
    guest_name: &str,
    guest_available: bool,
    env: &HostEnv,
) -> String {
    let distro = family_for_config(config);
    let host_shell = env.host_shell.clone();
    let host_locale = env.host_locale.clone();

    let builder = ContainerfileBuilder::new(&config.image.base, &config.container.name);
    builder
        .set_distro(distro)
        .add_base_packages(distro, host_shell.as_deref(), host_locale.as_deref())
        // The image must contain the shell the container actually runs. Only
        // the host's shell was installed above, so a config asking for a
        // different one produced a `CMD` pointing at a binary that was never
        // installed, and the container failed to start a shell.
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
    distro: Option<DistroFamily>,
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
            distro: None,
            packages: Vec::new(),
            run_commands: Vec::new(),
            guest_name: "podbox-guest".to_string(),
            has_guest_binary: false,
            env_vars: Vec::new(),
            forced_shell: None,
        }
    }

    /// Pin the distro family (explicit manager wins). Without it, `build`
    /// falls back to detecting from the base image name.
    fn set_distro(mut self, distro: DistroFamily) -> Self {
        self.distro = Some(distro);
        self
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
            // LANG only: forcing LC_ALL would override per-category
            // settings inside the box and warn when the image lacks the
            // host locale. Locale data itself comes from locale_packages.
            self.env_vars.push(("LANG".into(), locale.to_string()));
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
        let distro = self
            .distro
            .unwrap_or_else(|| DistroFamily::from_base_image(&self.base_image));
        let mut lines = Vec::new();

        lines.push(format!("FROM {}", self.base_image));
        lines.push(String::new());

        if !self.packages.is_empty() {
            let pkgs = self.packages.join(" ");
            lines.push(self.install_command(distro, &pkgs));
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

    /// The install layer: one `RUN`, including the cache clean. Shared with
    /// prebuilt overlays through [`install_layer`].
    fn install_command(&self, distro: DistroFamily, pkgs: &str) -> String {
        install_layer(distro, &self.base_image, pkgs).unwrap_or_default()
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
            .unwrap_or("bash")
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
        assert!(
            !cf.contains("LC_ALL") && !cf.contains("LC_CTYPE"),
            "only LANG is forced; LC_* overrides stay with the user:\n{cf}"
        );
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
    fn test_cachy_hook_sandbox_relaxation_is_scoped_to_install() {
        // CachyOS pacman has the network-sandbox toggle; vanilla Arch's
        // lacks it, so only CachyOS bases get the relaxation.
        let cachy = ContainerfileBuilder::new("cachyos/cachyos:latest", "test").add_base_packages(
            DistroFamily::ArchLike,
            Some("/bin/bash"),
            None,
        );
        let cf = cachy.build();
        let unsandbox = "DisableSandboxNetwork";
        assert!(cf.contains(unsandbox));
        // Scoped via --config on a temp copy: /etc/pacman.conf is never
        // rewritten in place, so the image keeps the sandbox.
        assert!(
            cf.contains("--config /tmp/podbox-pacman.conf"),
            "relaxation must be scoped to the install:\n{cf}"
        );
        assert!(
            !cf.contains("sed -i 's/^"),
            "must not rewrite /etc/pacman.conf in place:\n{cf}"
        );
        assert!(
            cf.contains("rm -f /tmp/podbox-pacman.conf"),
            "temp config must be removed in the same layer:\n{cf}"
        );

        let arch = ContainerfileBuilder::new("archlinux:latest", "test").add_base_packages(
            DistroFamily::ArchLike,
            Some("/bin/bash"),
            None,
        );
        assert!(!arch.build().contains("DisableSandbox"));

        let fedora = ContainerfileBuilder::new("fedora:41", "test").add_base_packages(
            DistroFamily::FedoraLike,
            Some("/bin/bash"),
            None,
        );
        assert!(!fedora.build().contains("DisableSandbox"));
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
        let env = HostEnv::mock_preview();
        let cf = generate_custom_with(
            &crate::config::Config::embedded(),
            "podbox-guest",
            false,
            &env,
        );
        assert!(cf.contains("FROM "));
        assert!(cf.contains("RUN "));
        assert!(cf.contains(GUEST_PLACEHOLDER));
        assert!(!cf.contains("COPY podbox-guest"));
        // The entrypoint still points at the guest: the preview shows the
        // real recipe, and the comment explains the missing layer.
        assert!(cf.contains("ENTRYPOINT [\"/usr/local/bin/podbox-guest\", \"--entry\"]"));
    }

    /// A configured shell is installed even when the host uses another one;
    /// otherwise `CMD` would point at a binary that was never installed.
    #[test]
    fn test_configured_shell_is_installed_regardless_of_host_shell() {
        // A host with no detectable shell or locale: everything in the
        // recipe must come from the config and `env`, never the process
        // environment (codegen is pure).
        let env = HostEnv {
            host_shell: None,
            host_locale: None,
            ..HostEnv::mock_preview()
        };
        let mut config = crate::config::Config::embedded();
        config.container.shell = "fish".into();
        let cf = generate_custom_with(&config, "podbox-guest", true, &env);
        assert!(
            cf.lines()
                .any(|l| l.starts_with("RUN ") && l.contains(" fish")),
            "fish must be installed:\n{cf}"
        );
        assert!(cf.contains("CMD [\"fish\"]"), "{cf}");

        // The configured shell is installed exactly once.
        let mut config = crate::config::Config::embedded();
        config.container.shell = "zsh".into();
        let cf = generate_custom_with(&config, "podbox-guest", true, &env);
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
        let env = HostEnv::mock_preview();
        let cf = generate_custom_with(
            &crate::config::Config::embedded(),
            "podbox-guest",
            true,
            &env,
        );
        assert!(cf.contains("COPY podbox-guest /usr/local/bin/podbox-guest"));
        assert!(cf.contains("RUN chmod +x /usr/local/bin/podbox-guest"));
        assert!(!cf.contains(GUEST_PLACEHOLDER));
    }
}
