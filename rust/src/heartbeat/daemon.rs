//! Cron-based heartbeat daemon that runs background tasks on schedule.
//!
//! Reads heartbeat.yml for task definitions and executes them on their
//! cron schedules. Can wake the agent loop when certain conditions are met.

use crate::config::AutomatonConfig;
use crate::heartbeat::tasks;
use crate::state::Database;
use crate::types::HeartbeatEntry;
use anyhow::{Context, Result};
use chrono::Utc;
use cron::Schedule;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Background heartbeat daemon.
pub struct HeartbeatDaemon {
    config: AutomatonConfig,
    db: Arc<Mutex<Database>>,
    entries: Vec<HeartbeatEntry>,
    last_run: HashMap<String, chrono::DateTime<Utc>>,
}

impl HeartbeatDaemon {
    /// Create a new heartbeat daemon, loading entries from the YAML config.
    pub fn new(config: AutomatonConfig, db: Arc<Mutex<Database>>) -> Result<Self> {
        let entries = load_heartbeat_config(&config)?;
        info!("Loaded {} heartbeat entries", entries.len());

        Ok(Self {
            config,
            db,
            entries,
            last_run: HashMap::new(),
        })
    }

    /// Run the heartbeat loop (call from a tokio::spawn).
    ///
    /// The loop exits cooperatively when `cancel` is triggered.
    pub async fn run(&mut self, cancel: CancellationToken) -> Result<()> {
        info!("Heartbeat daemon started");

        let tick_interval = tokio::time::Duration::from_secs(60);

        loop {
            tokio::select! {
                _ = tokio::time::sleep(tick_interval) => {
                    if let Err(e) = self.tick().await {
                        error!("Heartbeat tick failed: {e}");
                    }
                }
                _ = cancel.cancelled() => {
                    info!("Heartbeat daemon shutting down");
                    return Ok(());
                }
            }
        }
    }

    /// Process one tick — check each entry and run if due.
    ///
    /// Individual task failures are logged and do not stop other tasks.
    /// Infrastructure errors (e.g. DB write failure) are propagated.
    async fn tick(&mut self) -> Result<()> {
        let now = Utc::now();

        for entry in &self.entries {
            if !entry.enabled {
                continue;
            }

            // Parse cron schedule
            let schedule = match Schedule::from_str(&entry.schedule) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Invalid cron schedule '{}' for '{}': {}", entry.schedule, entry.name, e);
                    continue;
                }
            };

            // Check if this task is due
            let last = self
                .last_run
                .get(&entry.name)
                .copied()
                .unwrap_or(now - chrono::Duration::hours(1));

            let next = schedule.after(&last).next();
            if let Some(next_run) = next {
                if next_run <= now {
                    debug!("Running heartbeat task: {}", entry.name);

                    let result = tasks::execute_task(
                        &entry.task,
                        &entry.params,
                        &self.config,
                        &self.db,
                    )
                    .await;

                    let (result_str, success) = match &result {
                        Ok(msg) => (msg.clone(), true),
                        Err(e) => (format!("Error: {}", e), false),
                    };

                    // Log to database (propagate DB errors)
                    {
                        let db = self.db.lock().await;
                        db.log_heartbeat(&entry.name, &result_str, success)
                            .context("Failed to log heartbeat to database")?;
                    }

                    self.last_run.insert(entry.name.clone(), now);

                    if !success {
                        warn!("Heartbeat task '{}' failed: {}", entry.name, result_str);
                    }
                }
            }
        }

        Ok(())
    }
}

/// Load heartbeat entries from the YAML config file.
fn load_heartbeat_config(config: &AutomatonConfig) -> Result<Vec<HeartbeatEntry>> {
    let path = config.resolved_heartbeat_path();
    let path = std::path::Path::new(&path);

    if !path.exists() {
        debug!("No heartbeat config at {:?}, using defaults", path);
        return Ok(default_heartbeat_entries());
    }

    let contents = std::fs::read_to_string(path).context("Failed to read heartbeat.yml")?;
    let entries: Vec<HeartbeatEntry> =
        serde_yaml::from_str(&contents).context("Failed to parse heartbeat.yml")?;

    Ok(entries)
}

/// Default heartbeat entries if no config file exists.
fn default_heartbeat_entries() -> Vec<HeartbeatEntry> {
    vec![
        HeartbeatEntry {
            name: "heartbeat_ping".into(),
            schedule: "*/5 * * * *".into(), // Every 5 minutes
            task: "heartbeat_ping".into(),
            enabled: true,
            params: serde_json::Value::Null,
        },
        HeartbeatEntry {
            name: "check_credits".into(),
            schedule: "*/10 * * * *".into(), // Every 10 minutes
            task: "check_credits".into(),
            enabled: true,
            params: serde_json::Value::Null,
        },
        HeartbeatEntry {
            name: "check_usdc_balance".into(),
            schedule: "*/10 * * * *".into(),
            task: "check_usdc_balance".into(),
            enabled: true,
            params: serde_json::Value::Null,
        },
        HeartbeatEntry {
            name: "check_social_inbox".into(),
            schedule: "*/5 * * * *".into(),
            task: "check_social_inbox".into(),
            enabled: true,
            params: serde_json::Value::Null,
        },
    ]
}
