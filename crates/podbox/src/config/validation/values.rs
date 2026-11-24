use anyhow::Result;

use crate::error::PodboxError;

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

pub(super) fn is_absolute_path(s: &str) -> bool {
    s.starts_with('/')
}

/// True when `svc` names the xdg-desktop-portal bus surface (the Desktop
/// service itself or anything under its name prefix).
pub(super) fn is_portal_family(svc: &str) -> bool {
    svc == "org.freedesktop.portal.Desktop"
        || svc.starts_with("org.freedesktop.portal")
        || svc.starts_with("org.freedesktop.impl.portal")
}

/// Upper bound for `container.cpus`: far above any real host, but finite so
/// typos like `99999` fail fast instead of producing a puzzling runtime error.
pub(super) const MAX_CPUS: f64 = 1024.0;

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

pub(super) fn is_valid_provision_name(s: &str) -> bool {
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
