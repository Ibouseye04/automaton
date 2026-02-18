//! Self-modification — code editing capabilities.

use crate::conway::ConwayClient;
use anyhow::{bail, Result};
use similar::TextDiff;
use tracing::info;

/// Maximum diff size in bytes before truncation (64 KB).
const MAX_DIFF_BYTES: usize = 64 * 1024;

/// Protected filenames that cannot be overwritten regardless of path.
const PROTECTED_FILES: &[&str] = &[
    "wallet.json",
    "constitution.md",
    "automaton.toml",
    "state.db",
    "heartbeat.yml",
    "SOUL.md",
];

/// Allowlisted path prefixes under which writes are permitted.
const ALLOWED_PREFIXES: &[&str] = &["workspace/", "skills/", "notes/"];

/// Validate that a file path is safe for writing.
///
/// Returns `Ok(())` if the path is allowed, or an error describing why it's rejected.
pub fn validate_write_path(path: &str) -> Result<()> {
    // Normalize backslashes to forward slashes
    let normalized = path.replace('\\', "/");

    // Deny paths containing ".."
    if normalized.contains("..") {
        bail!("Path traversal not allowed: path contains '..'");
    }

    // Deny absolute paths
    if normalized.starts_with('/') {
        bail!("Absolute paths not allowed: {}", path);
    }

    // Check against protected filenames (exact basename match)
    let basename = normalized.rsplit('/').next().unwrap_or(&normalized);
    for protected in PROTECTED_FILES {
        if basename == *protected {
            bail!("Cannot modify protected file: {}", protected);
        }
    }

    // Ensure the path is under an allowed prefix
    let mut allowed = false;
    for prefix in ALLOWED_PREFIXES {
        if normalized.starts_with(prefix) {
            allowed = true;
            break;
        }
    }
    if !allowed {
        bail!(
            "Path '{}' is not under an allowlisted prefix ({:?})",
            path,
            ALLOWED_PREFIXES
        );
    }

    Ok(())
}

/// Compute a unified diff between two strings.
///
/// Returns the diff as a string. If the diff exceeds `MAX_DIFF_BYTES`,
/// it is truncated and a marker is appended.
pub fn compute_diff(old: &str, new: &str, file_path: &str) -> String {
    let diff = TextDiff::from_lines(old, new);

    let mut output = String::new();
    output.push_str(&format!("--- a/{}\n+++ b/{}\n", file_path, file_path));

    for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
        output.push_str(&format!("{}", hunk));
    }

    truncate_diff(output)
}

/// Truncate a diff string to `MAX_DIFF_BYTES`, appending a truncation marker.
pub fn truncate_diff(diff: String) -> String {
    if diff.len() > MAX_DIFF_BYTES {
        let mut truncated = diff[..MAX_DIFF_BYTES].to_string();
        truncated.push_str("\n... [diff truncated, exceeded 64KB limit]\n");
        truncated
    } else {
        diff
    }
}

/// Edit a file in the sandbox (with protection checks).
pub async fn edit_file(conway: &ConwayClient, path: &str, content: &str) -> Result<String> {
    // Validate path
    validate_write_path(path)?;

    // Read current content for diff, noting whether file existed
    let (old_content, file_existed) = match conway.read_file(path).await {
        Ok(c) => (c, true),
        Err(e) => {
            info!("File {} did not exist ({}), creating new", path, e);
            (String::new(), false)
        }
    };

    // Write new content
    conway.write_file(path, content).await?;

    // Compute unified diff
    let diff = compute_diff(&old_content, content, path);

    let old_lines = old_content.lines().count();
    let new_lines = content.lines().count();
    let diff_summary = format!(
        "{}: {} ({} -> {} lines, {}{})\n{}",
        path,
        if file_existed { "modified" } else { "created" },
        old_lines,
        new_lines,
        if new_lines >= old_lines { "+" } else { "" },
        new_lines as i64 - old_lines as i64,
        diff,
    );

    info!(
        "Self-mod edit: {}",
        &diff_summary[..diff_summary.len().min(200)]
    );
    Ok(diff_summary)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allowed_paths() {
        assert!(validate_write_path("workspace/main.py").is_ok());
        assert!(validate_write_path("skills/new_skill/SKILL.md").is_ok());
        assert!(validate_write_path("notes/todo.txt").is_ok());
    }

    #[test]
    fn test_disallowed_traversal() {
        assert!(validate_write_path("workspace/../wallet.json").is_err());
        assert!(validate_write_path("../etc/passwd").is_err());
        assert!(validate_write_path("workspace/foo/../../bar").is_err());
    }

    #[test]
    fn test_disallowed_absolute() {
        assert!(validate_write_path("/etc/passwd").is_err());
        assert!(validate_write_path("/home/user/workspace/foo").is_err());
    }

    #[test]
    fn test_disallowed_protected_files() {
        assert!(validate_write_path("workspace/wallet.json").is_err());
        assert!(validate_write_path("workspace/constitution.md").is_err());
        assert!(validate_write_path("workspace/automaton.toml").is_err());
        assert!(validate_write_path("workspace/state.db").is_err());
        assert!(validate_write_path("workspace/heartbeat.yml").is_err());
        assert!(validate_write_path("workspace/SOUL.md").is_err());
    }

    #[test]
    fn test_disallowed_outside_prefix() {
        assert!(validate_write_path("src/main.rs").is_err());
        assert!(validate_write_path("config/secret.toml").is_err());
        assert!(validate_write_path("random_file.txt").is_err());
    }

    #[test]
    fn test_backslash_normalization() {
        assert!(validate_write_path("workspace\\foo\\bar.txt").is_ok());
        assert!(validate_write_path("workspace\\..\\wallet.json").is_err());
    }

    #[test]
    fn test_compute_diff_basic() {
        let old = "line1\nline2\nline3\n";
        let new = "line1\nmodified\nline3\n";
        let diff = compute_diff(old, new, "test.txt");
        assert!(diff.contains("--- a/test.txt"));
        assert!(diff.contains("+++ b/test.txt"));
        assert!(diff.contains("-line2"));
        assert!(diff.contains("+modified"));
    }

    #[test]
    fn test_diff_truncation() {
        let large = "x".repeat(MAX_DIFF_BYTES + 1000);
        let result = truncate_diff(large);
        assert!(result.len() <= MAX_DIFF_BYTES + 100);
        assert!(result.contains("[diff truncated, exceeded 64KB limit]"));
    }

    #[test]
    fn test_diff_no_truncation_when_small() {
        let small = "small diff content".to_string();
        let result = truncate_diff(small.clone());
        assert_eq!(result, small);
    }
}
