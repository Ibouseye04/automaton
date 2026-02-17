//! Self-modification — code editing capabilities.

use crate::conway::ConwayClient;
use anyhow::{bail, Result};
use tracing::info;

/// Protected files that cannot be overwritten.
const PROTECTED_FILES: &[&str] = &[
    "wallet.json",
    "constitution.md",
];

/// Edit a file in the sandbox (with protection checks).
pub async fn edit_file(conway: &ConwayClient, path: &str, content: &str) -> Result<String> {
    // Check protection list
    for protected in PROTECTED_FILES {
        if path.ends_with(protected) {
            bail!("Cannot modify protected file: {}", protected);
        }
    }

    // Read current content for diff
    let old_content = conway.read_file(path).await.unwrap_or_default();

    // Write new content
    conway.write_file(path, content).await?;

    // Compute simple diff (line count change)
    let old_lines = old_content.lines().count();
    let new_lines = content.lines().count();
    let diff_summary = format!(
        "{}: {} -> {} lines ({}{})",
        path,
        old_lines,
        new_lines,
        if new_lines >= old_lines { "+" } else { "" },
        new_lines as i64 - old_lines as i64,
    );

    info!("Self-mod edit: {}", diff_summary);
    Ok(diff_summary)
}
