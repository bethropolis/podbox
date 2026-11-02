//! Ref derivation: registry, owner, repo, key -> full image ref without tag.
//!
//! Pure module.

use crate::error::PodboxError;

/// Derive the repository reference for an image.
///
/// Rules (section 4.3 of the spec):
///   - `image.name` set      -> `name` (validated, lowercased)
///   - lone podbox.toml      -> `{registry}/{owner}/{repo}`
///   - otherwise             -> `{registry}/{owner}/{repo}/{key}`
///
/// All components are lowercased.
pub fn derive_ref(
    registry: &str,
    owner: &str,
    repo: &str,
    key: &str,
    explicit_name: Option<&str>,
    is_lone_podbox_toml: bool,
) -> Result<String, PodboxError> {
    let ref_str = if let Some(name) = explicit_name {
        name.to_lowercase()
    } else if is_lone_podbox_toml {
        format!(
            "{}/{}/{}",
            registry.to_lowercase(),
            owner.to_lowercase(),
            repo.to_lowercase()
        )
    } else {
        format!(
            "{}/{}/{}/{}",
            registry.to_lowercase(),
            owner.to_lowercase(),
            repo.to_lowercase(),
            key.to_lowercase()
        )
    };
    validate_ref(&ref_str)?;
    Ok(ref_str)
}

/// Validate an OCI repository-name grammar subset.
///
/// OCI repository names: `[a-z0-9]+(?:(?:[._]|__|[-]*)[a-z0-9]+)*` per
/// component, optionally preceded by a namespace. We enforce a simpler,
/// stricter rule: lowercase alphanumerics, `.`, `_`, `-`, `/`, starting
/// with a letter or digit.
fn validate_ref(ref_str: &str) -> Result<(), PodboxError> {
    if ref_str.is_empty() {
        return Err(PodboxError::InvalidName {
            kind: "image ref",
            name: ref_str.into(),
        });
    }
    let first = ref_str.chars().next().unwrap();
    if !(first.is_ascii_alphanumeric()) {
        return Err(PodboxError::InvalidName {
            kind: "image ref",
            name: ref_str.into(),
        });
    }
    for c in ref_str.chars() {
        if !(c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' || c == '/') {
            return Err(PodboxError::InvalidName {
                kind: "image ref",
                name: ref_str.into(),
            });
        }
    }
    // No empty path components (no `//`, no leading/trailing `/`).
    if ref_str.contains("//") || ref_str.starts_with('/') || ref_str.ends_with('/') {
        return Err(PodboxError::InvalidName {
            kind: "image ref",
            name: ref_str.into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_ref_with_explicit_name() {
        let r = derive_ref(
            "ghcr.io",
            "bethropolis",
            "repo",
            "dev",
            Some("ghcr.io/me/custom"),
            false,
        )
        .unwrap();
        assert_eq!(r, "ghcr.io/me/custom");
    }

    #[test]
    fn derives_ref_lone_podbox_toml() {
        let r = derive_ref("ghcr.io", "bethropolis", "myrepo", "podbox", None, true).unwrap();
        assert_eq!(r, "ghcr.io/bethropolis/myrepo");
    }

    #[test]
    fn derives_ref_keyed() {
        let r = derive_ref("ghcr.io", "bethropolis", "myrepo", "gaming", None, false).unwrap();
        assert_eq!(r, "ghcr.io/bethropolis/myrepo/gaming");
    }

    #[test]
    fn lowercases_components() {
        let r = derive_ref("GHCR.IO", "Bethropolis", "MyRepo", "Gaming", None, false).unwrap();
        assert_eq!(r, "ghcr.io/bethropolis/myrepo/gaming");
    }

    #[test]
    fn rejects_invalid_ref() {
        let err = derive_ref("ghcr.io", "owner", "repo", "bad key", None, false).unwrap_err();
        assert!(matches!(err, PodboxError::InvalidName { .. }));
    }
}
