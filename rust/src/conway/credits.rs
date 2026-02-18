//! Conway compute credit monitoring.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tracing::debug;

/// Current credit balance.
#[derive(Debug, Clone, Deserialize)]
pub struct CreditBalance {
    pub credits: f64,
    pub currency: String,
}

/// Check credit balance from Conway API.
pub async fn check_credits(base_url: &str, api_key: &str) -> Result<CreditBalance> {
    let client = reqwest::Client::new();
    let url = format!("{}/v1/credits/balance", base_url.trim_end_matches('/'));

    let resp = client
        .get(&url)
        .bearer_auth(api_key)
        .send()
        .await
        .context("Credit check request failed")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        bail!("Credit check failed ({}): {}", status, body);
    }

    let balance: CreditBalance = resp.json().await.context("Failed to parse credit balance")?;
    debug!("Credit balance: {} {}", balance.credits, balance.currency);
    Ok(balance)
}
