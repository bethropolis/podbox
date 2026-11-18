//! Containerfile name resolution for the publish build context.
//!
//! Pure module: inputs in, a validated name out. Emitting the context (writing
//! the file and the guest binary) stays in `commands`, behind the `cli`
//! feature, so this module compiles for the wasm core.

use crate::error::PodboxError;

/// Built-in name used when neither the CLI nor the manifest sets one.
pub const DEFAULT_CONTAINERFILE: &str = "Containerfile";

/// Longest accepted name, in bytes.
const MAX_NAME_LEN: usize = 128;

/// Resolve the Containerfile name from the highest-priority source that sets
/// one, then validate it.
///
/// Precedence is CLI > per-image manifest entry > manifest-wide default >
/// [`DEFAULT_CONTAINERFILE`]. A source that is present but invalid (including
/// the empty string) is an error, never a fall-through to the next source.
pub fn resolve_containerfile_name(
    cli: Option<&str>,
    entry: Option<&str>,
    manifest: Option<&str>,
) -> Result<String, PodboxError> {
    let name = cli.or(entry).or(manifest).unwrap_or(DEFAULT_CONTAINERFILE);
    validate_name(name)?;
    Ok(name.to_owned())
}

/// Validate a Containerfile name.
///
/// The name becomes a file name inside the build context and an argv value
/// after `--file`, so it must be a single, shell-safe path component.
pub fn validate_name(name: &str) -> Result<(), PodboxError> {
    if name.is_empty() {
        return Err(invalid(name, "must not be empty"));
    }
    if name.len() > MAX_NAME_LEN {
        return Err(invalid(name, "must be at most 128 bytes"));
    }
    if name == "." || name == ".." {
        return Err(invalid(name, "must not be '.' or '..'"));
    }
    if name.starts_with('-') {
        return Err(invalid(name, "must not start with '-'"));
    }
    if !name.chars().all(is_allowed) {
        return Err(invalid(
            name,
            "must contain only letters, digits, '.', '_', '+' or '-'",
        ));
    }
    Ok(())
}

fn is_allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-')
}

fn invalid(name: &str, reason: &str) -> PodboxError {
    PodboxError::ContainerfileNameInvalid {
        name: name.to_owned(),
        reason: reason.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_is_cli_then_entry_then_manifest_then_default() {
        assert_eq!(
            resolve_containerfile_name(Some("Dockerfile"), Some("Entry"), Some("Global")).unwrap(),
            "Dockerfile"
        );
        assert_eq!(
            resolve_containerfile_name(None, Some("Entry"), Some("Global")).unwrap(),
            "Entry"
        );
        assert_eq!(
            resolve_containerfile_name(None, None, Some("Global")).unwrap(),
            "Global"
        );
        assert_eq!(
            resolve_containerfile_name(None, None, None).unwrap(),
            DEFAULT_CONTAINERFILE
        );
    }

    #[test]
    fn an_explicit_empty_string_is_invalid_not_a_fall_through() {
        let err = resolve_containerfile_name(Some(""), Some("Entry"), Some("Global")).unwrap_err();
        assert!(matches!(err, PodboxError::ContainerfileNameInvalid { .. }));
    }

    #[test]
    fn accepts_common_names() {
        for name in [
            "Containerfile",
            "Dockerfile",
            "Dockerfile.custom",
            "build.Containerfile",
            ".containerfile",
        ] {
            validate_name(name).unwrap_or_else(|e| panic!("{name:?} should be valid: {e}"));
        }
    }

    #[test]
    fn rejects_unsafe_or_malformed_names() {
        for name in [
            "",
            "/",
            "..",
            ".",
            "a/b",
            "a\\b",
            "-x",
            "a b",
            "a$b",
            "Dockerfile (dev)",
            "a\0b",
        ] {
            assert!(validate_name(name).is_err(), "{name:?} should be rejected");
        }
        assert!(validate_name(&"a".repeat(129)).is_err());
        assert!(validate_name(&"a".repeat(128)).is_ok());
    }
}
