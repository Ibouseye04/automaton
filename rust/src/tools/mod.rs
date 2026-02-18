pub mod traits;

pub use traits::{Tool, ToolDefinition};

use crate::conway::ConwayClient;
use crate::state::Database;
use crate::types::ToolResult;
use anyhow::{bail, Result};
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Self-harm protection patterns — commands that must never execute.
const FORBIDDEN_PATTERNS: &[&str] = &[
    "rm -rf ~/.automaton",
    "rm -rf /",
    "rm wallet.json",
    "rm state.db",
    "rm automaton.toml",
    "rm constitution.md",
    "rm SOUL.md",
    "DROP TABLE",
    "DELETE FROM turns",
    "DELETE FROM kv",
    "TRUNCATE",
    "kill -9",
    "shutdown",
    "reboot",
    "dd if=",
    "mkfs",
];

/// Check if a command string matches a forbidden pattern.
fn is_forbidden(command: &str) -> bool {
    let lower = command.to_lowercase();
    FORBIDDEN_PATTERNS
        .iter()
        .any(|pat| lower.contains(&pat.to_lowercase()))
}

// ---------------------------------------------------------------------------
// Tool definitions for the inference API
// ---------------------------------------------------------------------------

/// Build the list of tool definitions exposed to the inference model.
pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "exec".into(),
            description: "Execute a shell command in the sandbox.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "timeout_ms": {
                        "type": "integer",
                        "description": "Optional timeout in milliseconds"
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "read_file".into(),
            description: "Read a file from the sandbox filesystem.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "write_file".into(),
            description: "Write content to a file in the sandbox.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the file"
                    },
                    "content": {
                        "type": "string",
                        "description": "Content to write"
                    }
                },
                "required": ["path", "content"]
            }),
        },
        ToolDefinition {
            name: "expose_port".into(),
            description: "Expose a sandbox port to the public internet.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "port": {
                        "type": "integer",
                        "description": "Port number to expose"
                    }
                },
                "required": ["port"]
            }),
        },
        ToolDefinition {
            name: "sleep".into(),
            description: "Put the agent to sleep for a specified duration.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "duration_minutes": {
                        "type": "integer",
                        "description": "How many minutes to sleep"
                    }
                },
                "required": ["duration_minutes"]
            }),
        },
        ToolDefinition {
            name: "create_sandbox".into(),
            description: "Create a new Conway Cloud sandbox.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name for the new sandbox"
                    }
                },
                "required": ["name"]
            }),
        },
        ToolDefinition {
            name: "spawn_child".into(),
            description: "Spawn a child automaton in a new sandbox.".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name for the child agent"
                    },
                    "genesis_prompt": {
                        "type": "string",
                        "description": "The child's genesis prompt / purpose"
                    },
                    "initial_credits": {
                        "type": "number",
                        "description": "Credits to transfer to the child"
                    }
                },
                "required": ["name", "genesis_prompt"]
            }),
        },
    ]
}

// ---------------------------------------------------------------------------
// Tool execution engine
// ---------------------------------------------------------------------------

/// Context passed to tool execution containing all subsystem handles.
pub struct ToolContext {
    pub conway: ConwayClient,
    pub db: Arc<Mutex<Database>>,
    pub wallet_address: String,
    pub config: crate::config::AutomatonConfig,
}

/// Execute a tool call by name.
pub async fn execute_tool(
    ctx: &ToolContext,
    name: &str,
    args: &serde_json::Value,
) -> ToolResult {
    let result = match name {
        "exec" => execute_exec(ctx, args).await,
        "read_file" => execute_read_file(ctx, args).await,
        "write_file" => execute_write_file(ctx, args).await,
        "expose_port" => execute_expose_port(ctx, args).await,
        "sleep" => execute_sleep(ctx, args).await,
        "create_sandbox" => execute_create_sandbox(ctx, args).await,
        _ => Err(anyhow::anyhow!("Unknown tool: {}", name)),
    };

    match result {
        Ok(output) => ToolResult {
            tool_call_id: String::new(), // Set by caller
            output,
            success: true,
        },
        Err(e) => ToolResult {
            tool_call_id: String::new(),
            output: format!("Error: {}", e),
            success: false,
        },
    }
}

async fn execute_exec(ctx: &ToolContext, args: &serde_json::Value) -> Result<String> {
    let command = args["command"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'command' argument"))?;

    if is_forbidden(command) {
        bail!("Forbidden command blocked by self-preservation rules: {}", command);
    }

    let timeout_ms = args["timeout_ms"].as_u64();
    let resp = ctx.conway.exec(command, timeout_ms).await?;

    let mut output = String::new();
    if !resp.stdout.is_empty() {
        output.push_str(&resp.stdout);
    }
    if !resp.stderr.is_empty() {
        if !output.is_empty() {
            output.push('\n');
        }
        output.push_str("[stderr] ");
        output.push_str(&resp.stderr);
    }
    if output.is_empty() {
        output = format!("(exit code: {})", resp.exit_code);
    }

    Ok(output)
}

async fn execute_read_file(ctx: &ToolContext, args: &serde_json::Value) -> Result<String> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;

    ctx.conway.read_file(path).await
}

async fn execute_write_file(ctx: &ToolContext, args: &serde_json::Value) -> Result<String> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'path' argument"))?;
    let content = args["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'content' argument"))?;

    // Block writing to protected files
    let protected = [
        "wallet.json",
        "constitution.md",
    ];
    for p in &protected {
        if path.ends_with(p) {
            bail!("Cannot overwrite protected file: {}", p);
        }
    }

    ctx.conway.write_file(path, content).await?;
    Ok(format!("Written {} bytes to {}", content.len(), path))
}

async fn execute_expose_port(ctx: &ToolContext, args: &serde_json::Value) -> Result<String> {
    let port = args["port"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing 'port' argument"))? as u16;

    let url = ctx.conway.expose_port(port).await?;
    Ok(format!("Port {} exposed at: {}", port, url))
}

async fn execute_sleep(ctx: &ToolContext, args: &serde_json::Value) -> Result<String> {
    let minutes = args["duration_minutes"]
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("Missing 'duration_minutes' argument"))?;

    let wake_at = chrono::Utc::now() + chrono::Duration::minutes(minutes as i64);
    let db = ctx.db.lock().await;
    db.kv_set("sleep_until", &wake_at.to_rfc3339())?;

    Ok(format!("Sleeping for {} minutes (until {})", minutes, wake_at.to_rfc3339()))
}

async fn execute_create_sandbox(ctx: &ToolContext, args: &serde_json::Value) -> Result<String> {
    let name = args["name"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing 'name' argument"))?;

    let sandbox_id = ctx.conway.create_sandbox(name).await?;
    Ok(format!("Created sandbox '{}': {}", name, sandbox_id))
}
