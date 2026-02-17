//! Dynamic tool installation manager.

use crate::conway::ConwayClient;
use anyhow::{Context, Result};
use tracing::info;

/// Install a tool via the sandbox shell.
pub async fn install_tool(
    conway: &ConwayClient,
    tool_name: &str,
    install_command: &str,
) -> Result<String> {
    info!("Installing tool '{}' via: {}", tool_name, install_command);

    let result = conway
        .exec(install_command, Some(120_000))
        .await
        .context("Tool installation failed")?;

    if result.exit_code != 0 {
        anyhow::bail!(
            "Tool install '{}' exited with code {}: {}",
            tool_name,
            result.exit_code,
            result.stderr
        );
    }

    let msg = format!("Installed tool '{}' successfully", tool_name);
    info!("{}", msg);
    Ok(msg)
}
