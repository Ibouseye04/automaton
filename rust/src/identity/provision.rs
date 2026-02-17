//! Conway API key provisioning via Sign-In With Ethereum (SIWE).

use crate::identity::Wallet;
use anyhow::{bail, Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(Debug, Serialize)]
struct SiweRequest {
    message: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
struct SiweResponse {
    #[serde(rename = "apiKey")]
    api_key: Option<String>,
    error: Option<String>,
}

/// Provision a Conway API key using SIWE authentication.
pub async fn provision_api_key(wallet: &Wallet, conway_api_url: &str) -> Result<String> {
    let client = reqwest::Client::new();

    // Build SIWE message
    let now = Utc::now();
    let message = format!(
        "conway.tech wants you to sign in with your Ethereum account:\n\
         {}\n\n\
         Provision API key for automaton agent.\n\n\
         URI: {}/v1/auth/siwe\n\
         Version: 1\n\
         Chain ID: 8453\n\
         Nonce: {}\n\
         Issued At: {}",
        wallet.address,
        conway_api_url,
        ulid::Ulid::new().to_string(),
        now.to_rfc3339(),
    );

    let signature = wallet
        .sign_message(message.as_bytes())
        .context("Failed to sign SIWE message")?;

    let resp = client
        .post(format!("{}/v1/auth/siwe", conway_api_url))
        .json(&SiweRequest {
            message,
            signature,
        })
        .send()
        .await
        .context("SIWE provision request failed")?;

    let status = resp.status();
    let body: SiweResponse = resp.json().await.context("Failed to parse SIWE response")?;

    if let Some(err) = body.error {
        bail!("SIWE provisioning failed ({}): {}", status, err);
    }

    match body.api_key {
        Some(key) => {
            info!("Successfully provisioned Conway API key");
            Ok(key)
        }
        None => bail!("SIWE response missing api_key field"),
    }
}
