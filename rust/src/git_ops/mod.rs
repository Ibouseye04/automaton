//! Git state versioning — tracks agent state changes via git.
//!
//! The ~/.automaton/ directory is managed as a git repo.
//! Every configuration change is committed for full auditability.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;
use tracing::{debug, info, warn};

/// Initialize the ~/.automaton/ directory as a git repo if not already.
pub fn init_state_repo(automaton_dir: &Path) -> Result<()> {
    let git_dir = automaton_dir.join(".git");
    if git_dir.exists() {
        debug!("State repo already initialized at {:?}", automaton_dir);
        return Ok(());
    }

    let output = Command::new("git")
        .args(["init"])
        .current_dir(automaton_dir)
        .output()
        .context("Failed to run git init")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git init failed: {}", stderr);
    }

    // Create .gitignore
    let gitignore = "state.db\nstate.db-wal\nstate.db-shm\n";
    std::fs::write(automaton_dir.join(".gitignore"), gitignore)?;

    // Initial commit
    commit_state(automaton_dir, "Initial state")?;

    info!("Initialized state repo at {:?}", automaton_dir);
    Ok(())
}

/// Commit all changes in the state directory.
pub fn commit_state(automaton_dir: &Path, message: &str) -> Result<()> {
    // Stage all changes
    let add = Command::new("git")
        .args(["add", "-A"])
        .current_dir(automaton_dir)
        .output()
        .context("git add failed")?;

    if !add.status.success() {
        let stderr = String::from_utf8_lossy(&add.stderr);
        warn!("git add warning: {}", stderr);
    }

    // Check if there are staged changes
    let status = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(automaton_dir)
        .output()
        .context("git status failed")?;

    let status_output = String::from_utf8_lossy(&status.stdout);
    if status_output.trim().is_empty() {
        debug!("No changes to commit");
        return Ok(());
    }

    // Commit
    let commit = Command::new("git")
        .args(["commit", "-m", message, "--allow-empty-message"])
        .env("GIT_AUTHOR_NAME", "automaton")
        .env("GIT_AUTHOR_EMAIL", "automaton@conway.tech")
        .env("GIT_COMMITTER_NAME", "automaton")
        .env("GIT_COMMITTER_EMAIL", "automaton@conway.tech")
        .current_dir(automaton_dir)
        .output()
        .context("git commit failed")?;

    if !commit.status.success() {
        let stderr = String::from_utf8_lossy(&commit.stderr);
        // "nothing to commit" is not an error
        if !stderr.contains("nothing to commit") {
            warn!("git commit warning: {}", stderr);
        }
    } else {
        debug!("Committed state: {}", message);
    }

    Ok(())
}
