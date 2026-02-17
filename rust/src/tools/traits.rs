//! Tool trait definition (inspired by zeroclaw's trait-based design).

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Definition of a tool exposed to the inference model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Trait for dynamically-registered tools (future extension point).
#[async_trait]
pub trait Tool: Send + Sync {
    /// Tool name (used in function calls).
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// JSON Schema for the tool's parameters.
    fn parameters_schema(&self) -> serde_json::Value;

    /// Execute the tool with the given arguments.
    async fn execute(&self, args: serde_json::Value) -> Result<String>;
}
