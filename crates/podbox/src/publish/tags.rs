//! Tag token grammar and expansion (section 4.4).
//!
//! Pure module: token list + `TagContext` -> expanded tag strings.
//! The clock lives in `PublishContext` (the I/O edge), so expansion stays
//! pure and testable.

use std::collections::HashSet;

use crate::error::PodboxError;

/// A tag token: either a known expansion or a literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagToken {
    Latest,
    Sha,
    Semver,
    Date,
    Branch,
    Literal(String),
}

/// Context for tag expansion: what ref is being published.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagContext {
    pub ref_type: RefType,
    pub ref_name: String,
    pub sha7: String,
    pub date_utc: String, // YYYYMMDD, supplied by PublishContext (I/O edge)
    pub default_branch: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefType {
    Branch,
    Tag,
}

/// Parse tag strings into tokens.
///
/// A token name (`latest`, `sha`, `semver`, `date`, `branch`) is always the
/// token, never a literal. Anything else is a literal that must match the
/// OCI tag grammar.
pub fn parse_tags(tags: &[String]) -> Result<Vec<TagToken>, TagError> {
    let mut out = Vec::with_capacity(tags.len());
    for t in tags {
        match t.as_str() {
            "latest" => out.push(TagToken::Latest),
            "sha" => out.push(TagToken::Sha),
            "semver" => out.push(TagToken::Semver),
            "date" => out.push(TagToken::Date),
            "branch" => out.push(TagToken::Branch),
            other => {
                validate_tag_literal(other)?;
                out.push(TagToken::Literal(other.into()));
            }
        }
    }
    Ok(out)
}

/// Expand tokens into concrete tag strings.
///
/// Output order is declared order, de-duplicated, stable.
pub fn expand_tags(tokens: &[TagToken], context: &TagContext) -> Result<Vec<String>, TagError> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for token in tokens {
        let expanded = match token {
            TagToken::Latest => {
                if context.ref_type == RefType::Branch
                    && context.default_branch.as_deref() == Some(&context.ref_name)
                {
                    Some("latest".into())
                } else if context.ref_type == RefType::Tag {
                    // latest on a version tag too (semver row: push to default branch or version tag)
                    if is_version_tag(&context.ref_name) {
                        Some("latest".into())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            TagToken::Sha => Some(format!("sha-{}", context.sha7)),
            TagToken::Semver => {
                if context.ref_type == RefType::Tag {
                    semver_version(&context.ref_name)
                } else {
                    None
                }
            }
            TagToken::Date => Some(context.date_utc.clone()),
            TagToken::Branch => {
                if context.ref_type == RefType::Branch {
                    Some(sanitize_branch(&context.ref_name))
                } else {
                    None
                }
            }
            TagToken::Literal(l) => Some(l.clone()),
        };

        if let Some(tag) = expanded {
            if seen.insert(tag.clone()) {
                out.push(tag);
            }
        }
    }

    Ok(out)
}

fn is_version_tag(ref_name: &str) -> bool {
    let v = ref_name.strip_prefix('v').unwrap_or(ref_name);
    semver_version(v).is_some()
}

/// `X.Y.Z`, `X.Y`, `X` — leading `v` stripped. Pre-release never reduces.
fn semver_version(v: &str) -> Option<String> {
    let v = v.strip_prefix('v').unwrap_or(v);
    // Split off any pre-release / build suffix (-rc.1, +build) before validating digits.
    let core = v.split(['-', '+']).next()?;
    let parts: Vec<&str> = core.split('.').collect();
    if parts.len() < 3 {
        return None;
    }
    let (major, minor, patch) = (parts[0], parts[1], parts[2]);
    if !major.chars().all(|c| c.is_ascii_digit())
        || !minor.chars().all(|c| c.is_ascii_digit())
        || !patch.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some(format!("{major}.{minor}.{patch}"))
}

/// Sanitize a branch name for use as a tag: lowercase, replace illegal
/// chars with `-`, trim leading `-`/`.`, truncate to 128.
fn sanitize_branch(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_start_matches('-').trim_start_matches('.');
    let s = &s[..s.len().min(128)];
    if s.is_empty() {
        "branch".into()
    } else {
        s.into()
    }
}

/// OCI tag grammar: `^[A-Za-z0-9_][A-Za-z0-9._-]{0,127}$`.
fn validate_tag_literal(literal: &str) -> Result<(), TagError> {
    if literal.is_empty() {
        return Err(TagError::InvalidLiteral {
            literal: literal.into(),
            reason: "empty".into(),
        });
    }
    let first = literal.chars().next().unwrap();
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return Err(TagError::InvalidLiteral {
            literal: literal.into(),
            reason: "must start with letter, digit or underscore".into(),
        });
    }
    if literal.len() > 128 {
        return Err(TagError::InvalidLiteral {
            literal: literal.into(),
            reason: "exceeds 128 characters".into(),
        });
    }
    for c in literal.chars() {
        if !(c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-') {
            return Err(TagError::InvalidLiteral {
                literal: literal.into(),
                reason: format!("invalid character '{c}'"),
            });
        }
    }
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub enum TagError {
    #[error("tag expansion produced no tags for this context")]
    NoTags,
    #[error("invalid tag literal '{literal}': {reason}")]
    InvalidLiteral { literal: String, reason: String },
}

impl From<TagError> for PodboxError {
    fn from(e: TagError) -> Self {
        match e {
            TagError::NoTags => PodboxError::NoTags {
                key: "unknown".into(),
                context: "unknown".into(),
            },
            TagError::InvalidLiteral { literal, .. } => PodboxError::InvalidName {
                kind: "tag",
                name: literal,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(branch: &str, sha7: &str, date: &str, default: Option<&str>) -> TagContext {
        TagContext {
            ref_type: RefType::Branch,
            ref_name: branch.into(),
            sha7: sha7.into(),
            date_utc: date.into(),
            default_branch: default.map(|s| s.into()),
        }
    }

    fn tag_ctx(tag: &str, sha7: &str, date: &str) -> TagContext {
        TagContext {
            ref_type: RefType::Tag,
            ref_name: tag.into(),
            sha7: sha7.into(),
            date_utc: date.into(),
            default_branch: None,
        }
    }

    #[test]
    fn parses_tokens() {
        let tokens = parse_tags(&["latest".into(), "sha".into(), "v1.2".into()]).unwrap();
        assert_eq!(
            tokens,
            vec![
                TagToken::Latest,
                TagToken::Sha,
                TagToken::Literal("v1.2".into())
            ]
        );
    }

    #[test]
    fn rejects_bad_literal() {
        let err = parse_tags(&["bad tag!".into()]).unwrap_err();
        assert!(matches!(err, TagError::InvalidLiteral { .. }));
    }

    #[test]
    fn expands_branch_context() {
        let tokens = parse_tags(&["latest".into(), "sha".into(), "branch".into()]).unwrap();
        let c = ctx("my-feature!", "abc1234", "20260102", Some("main"));
        let tags = expand_tags(&tokens, &c).unwrap();
        assert_eq!(tags, vec!["sha-abc1234", "my-feature-"]);
        // latest omitted: not default branch
    }

    #[test]
    fn expands_default_branch_with_latest() {
        let tokens = parse_tags(&["latest".into(), "sha".into()]).unwrap();
        let c = ctx("main", "abc1234", "20260102", Some("main"));
        let tags = expand_tags(&tokens, &c).unwrap();
        assert_eq!(tags, vec!["latest", "sha-abc1234"]);
    }

    #[test]
    fn expands_tag_context() {
        let tokens = parse_tags(&["latest".into(), "sha".into(), "semver".into()]).unwrap();
        let c = tag_ctx("v1.2.3", "abc1234", "20260102");
        let tags = expand_tags(&tokens, &c).unwrap();
        assert_eq!(tags, vec!["latest", "sha-abc1234", "1.2.3"]);
    }

    #[test]
    fn pre_release_semver_full_only() {
        let tokens = parse_tags(&["semver".into()]).unwrap();
        let c = tag_ctx("v1.2.3-rc.1", "abc1234", "20260102");
        let tags = expand_tags(&tokens, &c).unwrap();
        assert_eq!(tags, vec!["1.2.3"]);
    }

    #[test]
    fn semver_on_branch_omitted() {
        let tokens = parse_tags(&["semver".into()]).unwrap();
        let c = ctx("main", "abc1234", "20260102", Some("main"));
        let tags = expand_tags(&tokens, &c).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn branch_on_tag_omitted() {
        let tokens = parse_tags(&["branch".into()]).unwrap();
        let c = tag_ctx("v1.2.3", "abc1234", "20260102");
        let tags = expand_tags(&tokens, &c).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn dedupes_and_preserves_order() {
        let tokens = parse_tags(&["latest".into(), "sha".into(), "latest".into()]).unwrap();
        let c = ctx("main", "abc1234", "20260102", Some("main"));
        let tags = expand_tags(&tokens, &c).unwrap();
        assert_eq!(tags, vec!["latest", "sha-abc1234"]);
    }

    #[test]
    fn empty_result_is_ok() {
        let tokens = parse_tags(&["semver".into()]).unwrap();
        let c = ctx("main", "abc1234", "20260102", Some("main"));
        let tags = expand_tags(&tokens, &c).unwrap();
        assert!(tags.is_empty());
    }

    #[test]
    fn sanitize_branch_replaces_illegal_chars() {
        assert_eq!(sanitize_branch("feat/fix#1"), "feat-fix-1");
        assert_eq!(sanitize_branch("-leading"), "leading");
        assert_eq!(sanitize_branch(".dot"), "dot");
    }
}
