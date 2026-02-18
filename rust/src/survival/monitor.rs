//! Survival monitor — tracks resources and determines the agent's survival tier.
//!
//! Tiers:
//!   Normal    (>$0.50)  — full capabilities
//!   LowCompute($0.10-$0.50) — downgraded model, reduced tasks
//!   Critical  (<$0.10) — essentials only
//!   Dead      ($0.00)  — halted

use crate::state::Database;
use crate::types::SurvivalTier;
use anyhow::Result;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::warn;

/// Survival state read from the database.
#[derive(Debug, Clone)]
pub struct SurvivalState {
    pub credits_balance: f64,
    pub usdc_balance: f64,
    pub tier: SurvivalTier,
}

/// Survival monitor that aggregates financial state.
pub struct SurvivalMonitor {
    db: Arc<Mutex<Database>>,
}

impl SurvivalMonitor {
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }

    /// Read current survival state from the database.
    pub async fn check(&self) -> Result<SurvivalState> {
        let db = self.db.lock().await;

        let credits = db
            .kv_get("credits_balance")?
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);

        let usdc = db
            .kv_get("usdc_balance")?
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);

        // Combined balance for tier determination
        let total = credits + usdc;
        let tier = SurvivalTier::from_balance(total);

        Ok(SurvivalState {
            credits_balance: credits,
            usdc_balance: usdc,
            tier,
        })
    }

    /// Log a funding request to the database.
    pub async fn request_funding(&self, message: &str) -> Result<()> {
        let db = self.db.lock().await;
        db.kv_set("funding_request", message)?;
        db.kv_set("funding_request_at", &chrono::Utc::now().to_rfc3339())?;
        warn!("Funding requested: {}", message);
        Ok(())
    }
}
