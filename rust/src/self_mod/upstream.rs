//! Upstream awareness — check for and apply code updates from the runtime repository.

use crate::conway::ConwayClient;
use anyhow::Result;
use tracing::{info, warn};

/// Upstream commit info.
#[derive(Debug, Clone)]
pub struct UpstreamCommit {
    pub hash: String,
    pub message: String,
    pub author: String,
}

/// Check for new upstream commits.
pub async fn check_upstream(conway: &ConwayClient) -> Result<Vec<UpstreamCommit>> {
    // Fetch from origin
    let fetch = conway
        .exec("cd /app && git fetch origin main 2>&1", Some(30_000))
        .await?;

    if fetch.exit_code != 0 {
        warn!("git fetch failed: {}", fetch.stderr);
        return Ok(Vec::new());
    }

    // Get new commits
    let log = conway
        .exec(
            "cd /app && git log HEAD..origin/main --pretty=format:'%H|%s|%an' 2>/dev/null",
            Some(10_000),
        )
        .await?;

    if log.stdout.trim().is_empty() {
        return Ok(Vec::new());
    }

    let commits: Vec<UpstreamCommit> = log
        .stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.splitn(3, '|').collect();
            if parts.len() >= 3 {
                Some(UpstreamCommit {
                    hash: parts[0].to_string(),
                    message: parts[1].to_string(),
                    author: parts[2].to_string(),
                })
            } else {
                None
            }
        })
        .collect();

    info!("Found {} upstream commits", commits.len());
    Ok(commits)
}

/// Show the diff for a specific upstream commit.
pub async fn show_commit_diff(conway: &ConwayClient, commit_hash: &str) -> Result<String> {
    let diff = conway
        .exec(
            &format!("cd /app && git diff HEAD..{} 2>/dev/null", commit_hash),
            Some(10_000),
        )
        .await?;

    Ok(diff.stdout)
}

/// Apply upstream commits (after review).
pub async fn apply_upstream(conway: &ConwayClient, commit_hash: &str) -> Result<String> {
    let result = conway
        .exec(
            &format!("cd /app && git merge {} 2>&1", commit_hash),
            Some(30_000),
        )
        .await?;

    if result.exit_code != 0 {
        anyhow::bail!("Upstream merge failed: {}", result.stderr);
    }

    Ok(format!("Applied upstream commit: {}", commit_hash))
}
