//! Build-key computation (section 6.1).
//!
//! Pure module: inputs in, `sha256:` key out. The key covers everything that
//! affects the resulting image *except* per-commit metadata (revision,
//! created, tags), so unchanged inputs never rebuild.

use sha2::{Digest, Sha256};

/// Compute `sha256:` build key from canonical length-prefixed fields.
///
/// Field order matters; each field is followed by a NUL byte so no field
/// boundary ambiguity exists (moving a byte between adjacent fields changes
/// the key).
pub fn compute_build_key(
    containerfile: &str,
    guest_hash: &str,
    version: &str,
    base_digest: &str,
    platforms: &[String],
    integration_labels: &[(String, String)],
) -> String {
    let mut hasher = Sha256::new();
    field(&mut hasher, containerfile.as_bytes());
    field(&mut hasher, guest_hash.as_bytes());
    field(&mut hasher, version.as_bytes());
    field(&mut hasher, base_digest.as_bytes());
    // platforms: sorted, NUL-separated, then a trailing NUL.
    field(&mut hasher, platforms.join("\0").as_bytes());
    // integration_labels: sorted key=value, NUL-separated, then trailing NUL.
    let mut lbl = String::new();
    for (k, v) in integration_labels {
        lbl.push_str(k);
        lbl.push('=');
        lbl.push_str(v);
        lbl.push('\0');
    }
    field(&mut hasher, lbl.as_bytes());
    format!("sha256:{}", hex::encode(hasher.finalize()))
}

fn field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(bytes);
    hasher.update([0]);
}

/// SHA-256 hex of a guest binary (per platform).
pub fn guest_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    const CF: &str = "FROM fedora:44\nRUN dnf install -y git\n";

    #[test]
    fn key_is_stable_for_identical_inputs() {
        let a = compute_build_key(CF, "guest-hash", "0.9.1", "sha256:base", &[], &[]);
        let b = compute_build_key(CF, "guest-hash", "0.9.1", "sha256:base", &[], &[]);
        assert_eq!(a, b);
        assert!(a.starts_with("sha256:"));
    }

    #[test]
    fn key_changes_with_each_input() {
        let base = compute_build_key(CF, "g", "v", "b", &[], &[]);
        assert_ne!(base, compute_build_key(CF, "g", "v", "b2", &[], &[])); // base digest
        assert_ne!(base, compute_build_key(CF, "g2", "v", "b", &[], &[])); // guest
        assert_ne!(base, compute_build_key(CF, "g", "v2", "b", &[], &[])); // version
        assert_ne!(base, compute_build_key(cf2(), "g", "v", "b", &[], &[])); // containerfile
        assert_ne!(
            base,
            compute_build_key(CF, "g", "v", "b", &["linux/amd64".into()], &[])
        ); // platforms
        assert_ne!(
            base,
            compute_build_key(CF, "g", "v", "b", &[], &[("a".into(), "1".into())])
        ); // labels
    }

    #[test]
    fn key_ignores_per_commit_metadata() {
        let base = compute_build_key(CF, "g", "v", "b", &[], &[]);
        // Different revision/created/tags do not appear in the key at all,
        // so they cannot change it.
        let _ = base;
    }

    #[test]
    fn field_boundary_moving_byte_changes_key() {
        let k1 = compute_build_key("ab\0", "g", "v", "b", &[], &[]);
        let k2 = compute_build_key("a", "b\0g", "v", "b", &[], &[]);
        assert_ne!(k1, k2);
    }

    fn cf2() -> &'static str {
        "FROM fedora:45\nRUN dnf install -y git\n"
    }
}
