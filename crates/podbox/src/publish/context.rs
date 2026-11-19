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

/// Convert days since the Unix epoch to a civil date (Howard Hinnant).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shifted = days + 719_468;
    let era = if shifted >= 0 {
        shifted
    } else {
        shifted - 146_096
    } / 146_097;
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    };
    let year = if month <= 2 { year + 1 } else { year };
    (year, month, day)
}

impl PublishContext {
    /// sha7: first 7 hex chars of the commit.
    pub fn sha7(&self) -> String {
        self.sha[..7.min(self.sha.len())].into()
    }

    /// date_utc: YYYYMMDD in UTC.
    pub fn date_utc(&self) -> String {
        let (y, m, d) = civil_from_days(self.epoch_days());
        format!("{y:04}{m:02}{d:02}")
    }

    /// RFC 3339 UTC timestamp for OCI `org.opencontainers.image.created`.
    pub fn created_rfc3339(&self) -> String {
        let dur = self.now.duration_since(UNIX_EPOCH).unwrap_or_default();
        let secs = i64::try_from(dur.as_secs()).unwrap_or(i64::MAX);
        let days = secs.div_euclid(86_400);
        let seconds_of_day = secs.rem_euclid(86_400);
        let (year, month, day) = civil_from_days(days);
        let hour = seconds_of_day / 3600;
        let minute = (seconds_of_day % 3600) / 60;
        let second = seconds_of_day % 60;
        format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
    }

    fn epoch_days(&self) -> i64 {
        let dur = self.now.duration_since(UNIX_EPOCH).unwrap_or_default();
        i64::try_from(dur.as_secs())
            .unwrap_or(i64::MAX)
            .div_euclid(86_400)
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
    let (owner, repo) = repository.split_once('/').ok_or(PodboxError::InvalidEnv(
        "GITHUB_REPOSITORY",
        "expected owner/repo",
    ))?;

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
        assert_eq!(c.created_rfc3339(), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn created_rfc3339_includes_the_time_of_day() {
        let c = PublishContext {
            owner: "o".into(),
            repo: "r".into(),
            sha: "abc".into(),
            ref_name: "main".into(),
            ref_type: RefType::Branch,
            event: "push".into(),
            default_branch: None,
            source_url: None,
            now: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(3661),
        };
        assert_eq!(c.created_rfc3339(), "1970-01-01T01:01:01Z");
    }
}
