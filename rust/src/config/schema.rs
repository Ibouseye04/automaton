//! Configuration schema for automaton.toml (TOML-based, inspired by zeroclaw).

use serde::{Deserialize, Serialize};

/// Root configuration structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomatonConfig {
    /// Human-readable agent name.
    pub name: String,

    /// The genesis prompt that defines this agent's purpose.
    pub genesis_prompt: String,

    /// Ethereum address of the creator / operator.
    pub creator_address: String,

    /// Conway Cloud sandbox identifier.
    pub sandbox_id: String,

    /// Conway Cloud API base URL.
    pub conway_api_url: String,

    /// Conway Cloud API key (provisioned via SIWE).
    pub conway_api_key: String,

    /// Inference model for the agent loop.
    pub inference_model: String,

    /// Low-compute fallback model.
    pub low_compute_model: String,

    /// Maximum tokens per inference turn.
    pub max_tokens_per_turn: u32,

    /// Maximum tool calls per turn before forcing a response.
    pub max_tool_calls_per_turn: u32,

    /// Maximum consecutive errors before the agent sleeps.
    pub max_consecutive_errors: u32,

    /// Maximum children this agent can spawn.
    pub max_children: u32,

    /// Path to heartbeat YAML config.
    pub heartbeat_config_path: String,

    /// Path to SQLite database.
    pub db_path: String,

    /// Directory for user-defined skills.
    pub skills_dir: String,

    /// Log level (debug, info, warn, error).
    pub log_level: String,

    /// Wallet address (derived, read-only).
    pub wallet_address: String,

    /// Parent agent address (if this is a child).
    pub parent_address: String,

    /// Config version.
    pub version: u32,

    /// Base chain RPC URL for on-chain operations.
    pub base_rpc_url: String,

    /// ERC-8004 registry contract address.
    pub registry_contract: String,

    /// Social relay URL for agent-to-agent messaging.
    pub social_relay_url: String,
}

impl Default for AutomatonConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            genesis_prompt: String::new(),
            creator_address: String::new(),
            sandbox_id: String::new(),
            conway_api_url: "https://api.conway.tech".into(),
            conway_api_key: String::new(),
            inference_model: "gpt-4o".into(),
            low_compute_model: "gpt-4o-mini".into(),
            max_tokens_per_turn: 4096,
            max_tool_calls_per_turn: 10,
            max_consecutive_errors: 5,
            max_children: 3,
            heartbeat_config_path: "~/.automaton/heartbeat.yml".into(),
            db_path: "~/.automaton/state.db".into(),
            skills_dir: "~/.automaton/skills".into(),
            log_level: "info".into(),
            wallet_address: String::new(),
            parent_address: String::new(),
            version: 1,
            base_rpc_url: "https://mainnet.base.org".into(),
            registry_contract: String::new(),
            social_relay_url: String::new(),
        }
    }
}

impl AutomatonConfig {
    /// Resolve a path that may contain `~` to an absolute path.
    pub fn resolve_path(&self, path: &str) -> String {
        shellexpand::tilde(path).into_owned()
    }

    /// Resolved database path.
    pub fn resolved_db_path(&self) -> String {
        self.resolve_path(&self.db_path)
    }

    /// Resolved heartbeat config path.
    pub fn resolved_heartbeat_path(&self) -> String {
        self.resolve_path(&self.heartbeat_config_path)
    }

    /// Resolved skills directory.
    pub fn resolved_skills_dir(&self) -> String {
        self.resolve_path(&self.skills_dir)
    }

    /// Determine the effective inference model based on survival tier.
    pub fn effective_model(&self, low_compute: bool) -> &str {
        if low_compute {
            &self.low_compute_model
        } else {
            &self.inference_model
        }
    }
}
