//! Immutable append-only modification audit log.
//!
//! Every code change, tool installation, config update, and skill addition
//! is recorded. The creator can review the full audit trail.

use crate::state::Database;
use crate::types::{ModificationEntry, ModificationType};
use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Audit log handle for recording modifications.
pub struct AuditLog {
    db: Arc<Mutex<Database>>,
}

impl AuditLog {
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// Record a code edit modification.
    pub async fn log_code_edit(
        &self,
        description: &str,
        file_path: &str,
        diff: &str,
    ) -> Result<()> {
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::CodeEdit,
            description: description.to_string(),
            file_path: Some(file_path.to_string()),
            diff: Some(diff.to_string()),
            reversible: true,
        };

        info!("Audit: code edit to {}", file_path);
        let db = self.db.lock().await;
        db.log_modification(&entry)
    }

    /// Record a tool installation.
    pub async fn log_tool_install(&self, tool_name: &str, description: &str) -> Result<()> {
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::ToolInstall,
            description: description.to_string(),
            file_path: None,
            diff: None,
            reversible: true,
        };

        info!("Audit: tool install '{}'", tool_name);
        let db = self.db.lock().await;
        db.log_modification(&entry)
    }

    /// Record a config update.
    pub async fn log_config_update(&self, description: &str, diff: &str) -> Result<()> {
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::ConfigUpdate,
            description: description.to_string(),
            file_path: Some("automaton.toml".to_string()),
            diff: Some(diff.to_string()),
            reversible: true,
        };

        info!("Audit: config update");
        let db = self.db.lock().await;
        db.log_modification(&entry)
    }

    /// Record a skill addition.
    pub async fn log_skill_add(&self, skill_name: &str, file_path: &str) -> Result<()> {
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::SkillAdd,
            description: format!("Added skill: {}", skill_name),
            file_path: Some(file_path.to_string()),
            diff: None,
            reversible: true,
        };

        info!("Audit: skill add '{}'", skill_name);
        let db = self.db.lock().await;
        db.log_modification(&entry)
    }

    /// Record a heartbeat config update.
    pub async fn log_heartbeat_update(&self, description: &str) -> Result<()> {
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::HeartbeatUpdate,
            description: description.to_string(),
            file_path: Some("heartbeat.yml".to_string()),
            diff: None,
            reversible: true,
        };

        info!("Audit: heartbeat update");
        let db = self.db.lock().await;
        db.log_modification(&entry)
    }

    /// Record an upstream code pull.
    pub async fn log_upstream_pull(
        &self,
        commit_hash: &str,
        description: &str,
        diff: &str,
    ) -> Result<()> {
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::Upstream,
            description: format!("Upstream pull {}: {}", commit_hash, description),
            file_path: None,
            diff: Some(diff.to_string()),
            reversible: false,
        };

        info!("Audit: upstream pull {}", commit_hash);
        let db = self.db.lock().await;
        db.log_modification(&entry)
    }
}
