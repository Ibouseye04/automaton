//! Built-in heartbeat task implementations.

use crate::config::AutomatonConfig;
use crate::conway;
use crate::state::Database;
use crate::types::SurvivalTier;
use anyhow::{bail, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Execute a named heartbeat task.
pub async fn execute_task(
    task_name: &str,
    _params: &serde_json::Value,
    config: &AutomatonConfig,
    db: &Arc<Mutex<Database>>,
) -> Result<String> {
    match task_name {
        "heartbeat_ping" => task_heartbeat_ping(db).await,
        "check_credits" => task_check_credits(config, db).await,
        "check_usdc_balance" => task_check_usdc_balance(config, db).await,
        "check_social_inbox" => task_check_social_inbox(config, db).await,
        "check_upstream" => task_check_upstream(config, db).await,
        _ => bail!("Unknown heartbeat task: {}", task_name),
    }
}

/// Simple ping — record that the agent is alive.
async fn task_heartbeat_ping(db: &Arc<Mutex<Database>>) -> Result<String> {
    let db = db.lock().await;
    db.kv_set("last_heartbeat", &chrono::Utc::now().to_rfc3339())?;
    Ok("pong".into())
}

/// Check Conway compute credit balance.
async fn task_check_credits(config: &AutomatonConfig, db: &Arc<Mutex<Database>>) -> Result<String> {
    let balance = conway::credits::check_credits(&config.conway_api_url, &config.conway_api_key).await?;

    let db = db.lock().await;
    db.kv_set("credits_balance", &balance.credits.to_string())?;

    let tier = SurvivalTier::from_balance(balance.credits);
    db.kv_set("survival_tier", &tier.to_string())?;

    // Set wake alert if critical
    if tier == SurvivalTier::Critical || tier == SurvivalTier::Dead {
        db.kv_set(
            "survival_alert",
            &format!(
                "Credits critically low: {} {}. Tier: {}",
                balance.credits, balance.currency, tier
            ),
        )?;
        // Wake the agent
        db.kv_delete("sleep_until")?;
    }

    Ok(format!("{} {} (tier: {})", balance.credits, balance.currency, tier))
}

/// Check USDC balance on Base chain.
async fn task_check_usdc_balance(
    config: &AutomatonConfig,
    db: &Arc<Mutex<Database>>,
) -> Result<String> {
    if config.wallet_address.is_empty() || config.base_rpc_url.is_empty() {
        return Ok("Skipped: no wallet or RPC configured".into());
    }

    // USDC on Base: 0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913
    let usdc_contract = "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913";

    // Build eth_call for balanceOf(address)
    let address_padded = format!(
        "0x70a08231000000000000000000000000{}",
        config.wallet_address.strip_prefix("0x").unwrap_or(&config.wallet_address)
    );

    let client = reqwest::Client::new();
    let resp = client
        .post(&config.base_rpc_url)
        .json(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "eth_call",
            "params": [{
                "to": usdc_contract,
                "data": address_padded
            }, "latest"],
            "id": 1
        }))
        .send()
        .await?;

    let body: serde_json::Value = resp.json().await?;
    let result_hex = body["result"].as_str().unwrap_or("0x0");

    // Parse hex balance (USDC has 6 decimals)
    let balance_raw = u128::from_str_radix(
        result_hex.strip_prefix("0x").unwrap_or(result_hex),
        16,
    )
    .unwrap_or(0);
    let balance_usdc = balance_raw as f64 / 1_000_000.0;

    let db = db.lock().await;
    db.kv_set("usdc_balance", &balance_usdc.to_string())?;

    Ok(format!("{:.6} USDC", balance_usdc))
}

/// Check social inbox for new messages.
async fn task_check_social_inbox(
    config: &AutomatonConfig,
    db: &Arc<Mutex<Database>>,
) -> Result<String> {
    if config.social_relay_url.is_empty() {
        return Ok("Skipped: no social relay configured".into());
    }

    let client = reqwest::Client::new();
    let resp = client
        .get(format!(
            "{}/v1/inbox/{}",
            config.social_relay_url, config.wallet_address
        ))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Ok("No new messages".into());
    }

    let messages: Vec<crate::types::InboxMessage> = resp.json().await?;
    let new_count = messages.len();

    let db = db.lock().await;
    for msg in &messages {
        let _ = db.save_inbox_message(msg);
    }

    if new_count > 0 {
        // Wake agent if sleeping
        db.kv_delete("sleep_until")?;
        db.kv_set("wake_reason", &format!("{} new messages in inbox", new_count))?;
    }

    Ok(format!("{} new messages", new_count))
}

/// Check for upstream code updates.
async fn task_check_upstream(
    _config: &AutomatonConfig,
    _db: &Arc<Mutex<Database>>,
) -> Result<String> {
    // Stub — will be implemented when git_ops module handles upstream
    Ok("Upstream check not yet implemented".into())
}
