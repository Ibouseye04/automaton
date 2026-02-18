//! Immutable append-only modification audit log.
//!
//! Every code change, tool installation, config update, and skill addition
//! is recorded. The creator can review the full audit trail.
//!
//! DB writes are offloaded via `spawn_blocking` so sqlite I/O does not
//! block the async runtime.

use crate::self_mod::code::truncate_diff;
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

    /// Persist an entry via spawn_blocking to avoid blocking the async runtime.
    async fn persist(&self, entry: ModificationEntry) -> Result<()> {
        let db = self.db.clone();
        tokio::task::spawn_blocking(move || {
            let db = db.blocking_lock();
            db.log_modification(&entry)
        })
        .await??;
        Ok(())
    }

    /// Record a code edit modification.
    pub async fn log_code_edit(
        &self,
        description: &str,
        file_path: &str,
        diff: &str,
    ) -> Result<()> {
        let truncated_diff = truncate_diff(diff.to_string());
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::CodeEdit,
            description: description.to_string(),
            file_path: Some(file_path.to_string()),
            diff: Some(truncated_diff),
            reversible: true,
        };

        info!("Audit: code edit to {}", file_path);
        self.persist(entry).await
    }

    /// Record a tool installation.
    pub async fn log_tool_install(&self, tool_name: &str, description: &str) -> Result<()> {
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::ToolInstall,
            description: format!("[{}] {}", tool_name, description),
            file_path: None,
            diff: None,
            reversible: true,
        };

        info!("Audit: tool install '{}'", tool_name);
        self.persist(entry).await
    }

    /// Record a config update.
    pub async fn log_config_update(&self, description: &str, diff: &str) -> Result<()> {
        let truncated_diff = truncate_diff(diff.to_string());
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::ConfigUpdate,
            description: description.to_string(),
            file_path: Some("automaton.toml".to_string()),
            diff: Some(truncated_diff),
            reversible: true,
        };

        info!("Audit: config update");
        self.persist(entry).await
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
        self.persist(entry).await
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
        self.persist(entry).await
    }

    /// Record an upstream code pull.
    pub async fn log_upstream_pull(
        &self,
        commit_hash: &str,
        description: &str,
        diff: &str,
    ) -> Result<()> {
        let truncated_diff = truncate_diff(diff.to_string());
        let entry = ModificationEntry {
            id: ulid::Ulid::new().to_string(),
            timestamp: Utc::now(),
            mod_type: ModificationType::Upstream,
            description: format!("Upstream pull {}: {}", commit_hash, description),
            file_path: None,
            diff: Some(truncated_diff),
            reversible: false,
        };

        info!("Audit: upstream pull {}", commit_hash);
        self.persist(entry).await
    }
}
