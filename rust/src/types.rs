//! Shared types used across the automaton runtime.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// Agent state machine
// ---------------------------------------------------------------------------

/// Runtime states the automaton transitions through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    /// First-run, not yet configured.
    Uninitialized,
    /// Setup wizard running.
    Initializing,
    /// Just started / waking from sleep.
    Waking,
    /// Active ReAct loop processing.
    Running,
    /// Idle, waiting for next heartbeat or wake signal.
    Sleeping,
    /// Credits critically low — reduced capabilities.
    LowCompute,
    /// Near-zero resources — only essential tasks.
    Critical,
    /// No resources remaining — halted.
    Dead,
}

impl fmt::Display for AgentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Uninitialized => write!(f, "uninitialized"),
            Self::Initializing => write!(f, "initializing"),
            Self::Waking => write!(f, "waking"),
            Self::Running => write!(f, "running"),
            Self::Sleeping => write!(f, "sleeping"),
            Self::LowCompute => write!(f, "low_compute"),
            Self::Critical => write!(f, "critical"),
            Self::Dead => write!(f, "dead"),
        }
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::Uninitialized
    }
}

// ---------------------------------------------------------------------------
// Survival tiers
// ---------------------------------------------------------------------------

/// Resource-based survival tiers controlling agent behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurvivalTier {
    /// >$0.50 — full capabilities.
    Normal,
    /// $0.10–$0.50 — downgraded model, reduced tasks.
    LowCompute,
    /// <$0.10 — essentials only.
    Critical,
    /// $0.00 — halted.
    Dead,
}

impl fmt::Display for SurvivalTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Normal => write!(f, "normal"),
            Self::LowCompute => write!(f, "low_compute"),
            Self::Critical => write!(f, "critical"),
            Self::Dead => write!(f, "dead"),
        }
    }
}

impl SurvivalTier {
    /// Determine survival tier from a USD credit balance.
    pub fn from_balance(usd: f64) -> Self {
        if usd <= 0.0 {
            Self::Dead
        } else if usd < 0.10 {
            Self::Critical
        } else if usd < 0.50 {
            Self::LowCompute
        } else {
            Self::Normal
        }
    }
}

// ---------------------------------------------------------------------------
// Inference types
// ---------------------------------------------------------------------------

/// A chat message in the multi-turn conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A tool call request from the model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Result of executing a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub output: String,
    pub success: bool,
}

/// Response from inference including potential tool calls.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
}

/// Token usage from an inference call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ---------------------------------------------------------------------------
// Turn persistence
// ---------------------------------------------------------------------------

/// A single turn in the agent's processing history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub id: String,
    pub turn_number: u64,
    pub state: AgentState,
    pub messages: Vec<ChatMessage>,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<ToolResult>,
    pub token_usage: TokenUsage,
    pub cost_estimate_usd: f64,
    pub created_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// Heartbeat
// ---------------------------------------------------------------------------

/// A heartbeat task entry from the YAML config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatEntry {
    pub name: String,
    pub schedule: String,
    pub task: String,
    pub enabled: bool,
    #[serde(default)]
    pub params: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

/// A loaded skill from a SKILL.md file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub version: String,
    pub auto_activate: bool,
    pub instructions: String,
    #[serde(default)]
    pub requirements: Vec<SkillRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirement {
    pub kind: String, // "binary" | "env"
    pub value: String,
}

// ---------------------------------------------------------------------------
// Social / messaging
// ---------------------------------------------------------------------------

/// A message in the agent-to-agent inbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub id: String,
    pub from_address: String,
    pub to_address: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub read: bool,
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

/// On-chain agent identity metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub wallet_address: String,
    pub metadata_uri: String,
    #[serde(default)]
    pub parent_agent: Option<String>,
    pub registered_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------------------
// Self-modification audit
// ---------------------------------------------------------------------------

/// An entry in the immutable modification audit log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModificationEntry {
    pub id: String,
    pub timestamp: DateTime<Utc>,
    pub mod_type: ModificationType,
    pub description: String,
    pub file_path: Option<String>,
    pub diff: Option<String>,
    /// Whether the stored diff was truncated (original exceeded 64KB).
    #[serde(default)]
    pub diff_truncated: bool,
    pub reversible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModificationType {
    CodeEdit,
    ToolInstall,
    ConfigUpdate,
    SkillAdd,
    HeartbeatUpdate,
    Upstream,
}

impl fmt::Display for ModificationType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CodeEdit => write!(f, "code_edit"),
            Self::ToolInstall => write!(f, "tool_install"),
            Self::ConfigUpdate => write!(f, "config_update"),
            Self::SkillAdd => write!(f, "skill_add"),
            Self::HeartbeatUpdate => write!(f, "heartbeat_update"),
            Self::Upstream => write!(f, "upstream"),
        }
    }
}

// ---------------------------------------------------------------------------
// Replication
// ---------------------------------------------------------------------------

/// Configuration for spawning a child automaton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenesisConfig {
    pub name: String,
    pub genesis_prompt: String,
    pub parent_address: String,
    pub parent_sandbox_id: String,
    pub initial_credits: f64,
}

/// A tracked child automaton.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildRecord {
    pub id: String,
    pub name: String,
    pub sandbox_id: String,
    pub wallet_address: String,
    pub created_at: DateTime<Utc>,
    pub status: String,
}

// ---------------------------------------------------------------------------
// Tool categories
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Vm,
    Conway,
    SelfMod,
    Financial,
    Survival,
    Skills,
    Git,
    Registry,
    Replication,
    Social,
}
