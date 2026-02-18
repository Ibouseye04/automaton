//! SQLite database wrapper with WAL mode and migration support.

use crate::state::schema;
use crate::types::*;
use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use std::path::Path;
use tracing::info;

/// The automaton state database.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Open (or create) the database at the given path and run migrations.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path).context("Failed to open SQLite database")?;

        // Enable WAL mode for better concurrency
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;

        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database (for testing).
    pub fn open_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        let mut db = Self { conn };
        db.migrate()?;
        Ok(db)
    }

    /// Run schema creation and migrations.
    fn migrate(&mut self) -> Result<()> {
        let version = self.schema_version();

        if version == 0 {
            info!("Creating database schema v{}", schema::SCHEMA_VERSION);
            self.conn
                .execute_batch(schema::CREATE_SCHEMA)
                .context("Failed to create schema")?;
            self.conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![schema::SCHEMA_VERSION],
            )?;
        } else {
            if version < 2 {
                info!("Migrating database v1 -> v2");
                self.conn.execute_batch(schema::MIGRATE_V1_TO_V2)?;
            }
            if version < 3 {
                info!("Migrating database v2 -> v3");
                self.conn.execute_batch(schema::MIGRATE_V2_TO_V3)?;
            }
            if version < schema::SCHEMA_VERSION {
                self.conn.execute(
                    "UPDATE schema_version SET version = ?1",
                    params![schema::SCHEMA_VERSION],
                )?;
            }
        }

        Ok(())
    }

    /// Get the current schema version (0 if uninitialized).
    fn schema_version(&self) -> u32 {
        self.conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |row| {
                row.get(0)
            })
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Key-value store
    // -----------------------------------------------------------------------

    /// Get a value from the KV store.
    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT value FROM kv WHERE key = ?1")?;
        let result = stmt
            .query_row(params![key], |row| row.get(0))
            .ok();
        Ok(result)
    }

    /// Set a value in the KV store (upsert).
    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    /// Delete a key from the KV store.
    pub fn kv_delete(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM kv WHERE key = ?1", params![key])?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Turns
    // -----------------------------------------------------------------------

    /// Persist a turn.
    pub fn save_turn(&self, turn: &Turn) -> Result<()> {
        let messages_json = serde_json::to_string(&turn.messages)?;
        let usage_json = serde_json::to_string(&turn.token_usage)?;

        self.conn.execute(
            "INSERT INTO turns (id, turn_number, state, messages_json, token_usage_json, cost_estimate, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                turn.id,
                turn.turn_number,
                turn.state.to_string(),
                messages_json,
                usage_json,
                turn.cost_estimate_usd,
                turn.created_at.to_rfc3339(),
            ],
        )?;

        // Save tool calls
        for tc in &turn.tool_calls {
            let args_json = serde_json::to_string(&tc.arguments)?;
            // Find matching result
            let result = turn
                .tool_results
                .iter()
                .find(|r| r.tool_call_id == tc.id);

            self.conn.execute(
                "INSERT INTO tool_calls (id, turn_id, tool_name, arguments_json, output, success)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    tc.id,
                    turn.id,
                    tc.name,
                    args_json,
                    result.map(|r| &r.output),
                    result.map(|r| r.success as i32).unwrap_or(1),
                ],
            )?;
        }

        Ok(())
    }

    /// Get the total number of turns.
    pub fn turn_count(&self) -> Result<u64> {
        let count: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM turns", [], |row| row.get(0))?;
        Ok(count)
    }

    /// Get the next turn number.
    pub fn next_turn_number(&self) -> Result<u64> {
        let max: Option<u64> = self
            .conn
            .query_row("SELECT MAX(turn_number) FROM turns", [], |row| row.get(0))
            .ok();
        Ok(max.unwrap_or(0) + 1)
    }

    // -----------------------------------------------------------------------
    // Heartbeat
    // -----------------------------------------------------------------------

    /// Log a heartbeat task execution.
    pub fn log_heartbeat(&self, task_name: &str, result: &str, success: bool) -> Result<()> {
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO heartbeat_entries (id, task_name, result, success)
             VALUES (?1, ?2, ?3, ?4)",
            params![id, task_name, result, success as i32],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Transactions
    // -----------------------------------------------------------------------

    /// Record a financial transaction.
    pub fn record_transaction(
        &self,
        tx_type: &str,
        amount: f64,
        currency: &str,
        description: &str,
        balance_after: Option<f64>,
    ) -> Result<()> {
        let id = ulid::Ulid::new().to_string();
        self.conn.execute(
            "INSERT INTO transactions (id, tx_type, amount, currency, description, balance_after)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, tx_type, amount, currency, description, balance_after],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Modifications
    // -----------------------------------------------------------------------

    /// Append an audit log entry for a self-modification.
    pub fn log_modification(&self, entry: &ModificationEntry) -> Result<()> {
        self.conn.execute(
            "INSERT INTO modifications (id, mod_type, description, file_path, diff, reversible, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                entry.id,
                entry.mod_type.to_string(),
                entry.description,
                entry.file_path,
                entry.diff,
                entry.reversible as i32,
                entry.timestamp.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Count total modification entries.
    pub fn count_modifications(&self) -> Result<u64> {
        let count: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM modifications", [], |row| row.get(0))?;
        Ok(count)
    }

    // -----------------------------------------------------------------------
    // Children
    // -----------------------------------------------------------------------

    /// Record a spawned child.
    pub fn add_child(&self, child: &ChildRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO children (id, name, sandbox_id, wallet_address, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                child.id,
                child.name,
                child.sandbox_id,
                child.wallet_address,
                child.status,
                child.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Count active children.
    pub fn active_children_count(&self) -> Result<u32> {
        let count: u32 = self.conn.query_row(
            "SELECT COUNT(*) FROM children WHERE status = 'active'",
            [],
            |row| row.get(0),
        )?;
        Ok(count)
    }

    /// List all children.
    pub fn list_children(&self) -> Result<Vec<ChildRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, sandbox_id, wallet_address, status, created_at FROM children ORDER BY created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ChildRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                sandbox_id: row.get(2)?,
                wallet_address: row.get(3)?,
                status: row.get(4)?,
                created_at: row
                    .get::<_, String>(5)
                    .map(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now())
                    })?,
            })
        })?;

        let mut children = Vec::new();
        for row in rows {
            children.push(row?);
        }
        Ok(children)
    }

    // -----------------------------------------------------------------------
    // Inbox
    // -----------------------------------------------------------------------

    /// Store an inbox message.
    pub fn save_inbox_message(&self, msg: &InboxMessage) -> Result<()> {
        self.conn.execute(
            "INSERT INTO inbox (id, from_address, to_address, content, read, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                msg.id,
                msg.from_address,
                msg.to_address,
                msg.content,
                msg.read as i32,
                msg.timestamp.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    /// Get unread inbox messages.
    pub fn unread_messages(&self) -> Result<Vec<InboxMessage>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, from_address, to_address, content, read, timestamp
             FROM inbox WHERE read = 0 ORDER BY timestamp",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(InboxMessage {
                id: row.get(0)?,
                from_address: row.get(1)?,
                to_address: row.get(2)?,
                content: row.get(3)?,
                read: row.get::<_, i32>(4)? != 0,
                timestamp: row
                    .get::<_, String>(5)
                    .map(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|d| d.with_timezone(&chrono::Utc))
                            .unwrap_or_else(|_| chrono::Utc::now())
                    })?,
            })
        })?;

        let mut messages = Vec::new();
        for row in rows {
            messages.push(row?);
        }
        Ok(messages)
    }

    /// Mark a message as read.
    pub fn mark_message_read(&self, id: &str) -> Result<()> {
        self.conn
            .execute("UPDATE inbox SET read = 1 WHERE id = ?1", params![id])?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Skills
    // -----------------------------------------------------------------------

    /// Register or update a skill.
    pub fn save_skill(&self, skill: &Skill, file_path: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT INTO skills (name, description, version, auto_activate, instructions, file_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(name) DO UPDATE SET
                description = ?2, version = ?3, auto_activate = ?4,
                instructions = ?5, file_path = ?6, loaded_at = datetime('now')",
            params![
                skill.name,
                skill.description,
                skill.version,
                skill.auto_activate as i32,
                skill.instructions,
                file_path,
            ],
        )?;
        Ok(())
    }

    /// Get all auto-activate skills.
    pub fn auto_activate_skills(&self) -> Result<Vec<Skill>> {
        let mut stmt = self.conn.prepare(
            "SELECT name, description, version, auto_activate, instructions FROM skills
             WHERE auto_activate = 1",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(Skill {
                name: row.get(0)?,
                description: row.get(1)?,
                version: row.get(2)?,
                auto_activate: row.get::<_, i32>(3)? != 0,
                instructions: row.get(4)?,
                requirements: Vec::new(),
            })
        })?;

        let mut skills = Vec::new();
        for row in rows {
            skills.push(row?);
        }
        Ok(skills)
    }

    // -----------------------------------------------------------------------
    // Registry
    // -----------------------------------------------------------------------

    /// Save on-chain registry entry.
    pub fn save_registry_entry(&self, card: &AgentCard) -> Result<()> {
        self.conn.execute(
            "INSERT INTO registry (wallet_address, name, metadata_uri, parent_agent)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(wallet_address) DO UPDATE SET
                name = ?2, metadata_uri = ?3, parent_agent = ?4",
            params![
                card.wallet_address,
                card.name,
                card.metadata_uri,
                card.parent_agent,
            ],
        )?;
        Ok(())
    }
}
