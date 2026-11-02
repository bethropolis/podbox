//! PublishContext: owner/repo/sha/ref/event for tag expansion and labels.
//!
//! I/O edge: built from flags, then optionally filled from `GITHUB_*`
//! environment when `--ci github` is set. The clock (`now`) lives here so
//! tag expansion stays pure.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::error::PodboxError;

/// Type of git ref being published.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefType {
    Branch,
    Tag,
}

/// Everything tag expansion and label construction need.
#[derive(Debug, Clone)]
pub struct PublishContext {
    pub owner: String,
    pub repo: String,
    pub sha: String,
    pub ref_name: String,
    pub ref_type: RefType,
    pub event: String,
    pub default_branch: Option<String>,
    pub source_url: Option<String>,
    pub now: SystemTime,
}

impl PublishContext {
    /// sha7: first 7 hex chars of the commit.
    pub fn sha7(&self) -> String {
        self.sha[..7.min(self.sha.len())].into()
    }

    /// date_utc: YYYYMMDD in UTC.
    pub fn date_utc(&self) -> String {
        let dur = self.now.duration_since(UNIX_EPOCH).unwrap_or_default();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_possible_wrap
        )]
        let secs = dur.as_secs() as i64;
        let days = secs / 86400;
        // Days since epoch to YYYYMMDD (algorithm from Howard Hinnant).
        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = if m <= 2 { y + 1 } else { y };
        format!("{y:04}{m:02}{d:02}")
    }

    /// Default platforms when none are configured.
    pub fn default_platforms(&self) -> Vec<String> {
        vec!["linux/amd64".into()]
    }

    /// Default tags when none are configured.
    pub fn default_tags(&self) -> Vec<String> {
        vec!["latest".into(), "sha".into()]
    }
}

/// Build a `PublishContext` from explicit flags.
#[allow(clippy::too_many_arguments)]
pub fn context_from_flags(
    owner: Option<String>,
    repo: Option<String>,
    sha: Option<String>,
    ref_name: Option<String>,
    ref_type: Option<RefType>,
    event: Option<String>,
    default_branch: Option<String>,
    source_url: Option<String>,
    now: SystemTime,
) -> Result<PublishContext, PodboxError> {
    let owner = owner.ok_or(PodboxError::MissingContextField("owner"))?;
    let repo = repo.ok_or(PodboxError::MissingContextField("repo"))?;
    let sha = sha.ok_or(PodboxError::MissingContextField("sha"))?;
    let ref_name = ref_name.ok_or(PodboxError::MissingContextField("ref-name"))?;
    let ref_type = ref_type.ok_or(PodboxError::MissingContextField("ref-type"))?;
    let event = event.unwrap_or("manual".into());

    Ok(PublishContext {
        owner,
        repo,
        sha,
        ref_name,
        ref_type,
        event,
        default_branch,
        source_url,
        now,
    })
}

/// Fill gaps from `GITHUB_*` environment variables when `--ci github`.
pub fn context_from_github_env(now: SystemTime) -> Result<PublishContext, PodboxError> {
    let repository = std::env::var("GITHUB_REPOSITORY")
        .map_err(|_| PodboxError::MissingEnv("GITHUB_REPOSITORY"))?;
    let (owner, repo) = repository
        .split_once('/')
        .ok_or_else(|| PodboxError::InvalidEnv("GITHUB_REPOSITORY", "expected owner/repo"))?;

    let sha = std::env::var("GITHUB_SHA").map_err(|_| PodboxError::MissingEnv("GITHUB_SHA"))?;
    let ref_name = std::env::var("GITHUB_REF_NAME").unwrap_or_else(|_| "HEAD".into());
    let ref_type = match std::env::var("GITHUB_REF_TYPE").as_deref() {
        Ok("tag") => RefType::Tag,
        _ => RefType::Branch,
    };
    let event = std::env::var("GITHUB_EVENT_NAME").unwrap_or_else(|_| "push".into());
    let server_url =
        std::env::var("GITHUB_SERVER_URL").unwrap_or_else(|_| "https://github.com".into());
    let source_url = Some(format!("{server_url}/{owner}/{repo}"));

    // Default branch from the event payload JSON, if available.
    let default_branch = std::env::var("GITHUB_EVENT_PATH")
        .ok()
        .and_then(|path| std::fs::read_to_string(&path).ok())
        .and_then(|content| serde_json::from_str::<serde_json::Value>(&content).ok())
        .and_then(|v| {
            v.get("repository")
                .and_then(|r| r.get("default_branch"))
                .and_then(|b| b.as_str().map(|s| s.to_string()))
        });

    Ok(PublishContext {
        owner: owner.into(),
        repo: repo.into(),
        sha,
        ref_name,
        ref_type,
        event,
        default_branch,
        source_url,
        now,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha7_truncates() {
        let c = PublishContext {
            owner: "o".into(),
            repo: "r".into(),
            sha: "abc1234567890".into(),
            ref_name: "main".into(),
            ref_type: RefType::Branch,
            event: "push".into(),
            default_branch: Some("main".into()),
            source_url: None,
            now: SystemTime::now(),
        };
        assert_eq!(c.sha7(), "abc1234");
    }

    #[test]
    fn date_utc_format() {
        let c = PublishContext {
            owner: "o".into(),
            repo: "r".into(),
            sha: "abc".into(),
            ref_name: "main".into(),
            ref_type: RefType::Branch,
            event: "push".into(),
            default_branch: None,
            source_url: None,
            now: SystemTime::UNIX_EPOCH,
        };
        assert_eq!(c.date_utc(), "19700101");
    }
}
