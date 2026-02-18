//! x402 — USDC payment protocol for Conway Cloud (HTTP 402 Payment Required).
//!
//! When a Conway API call returns 402, the response contains a payment envelope.
//! The agent signs a USDC transfer and resubmits the request with the payment header.

use crate::identity::Wallet;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Payment envelope returned in a 402 response.
#[derive(Debug, Deserialize)]
pub struct PaymentEnvelope {
    /// Recipient address for USDC transfer.
    pub recipient: String,
    /// Amount in USDC (human-readable, e.g. "0.01").
    pub amount: String,
    /// Chain ID (8453 for Base).
    pub chain_id: u64,
    /// Token contract address (USDC on Base).
    pub token: String,
    /// Nonce or reference ID for this payment.
    pub reference: String,
}

#[derive(Debug, Serialize)]
struct PaymentProof {
    signature: String,
    reference: String,
    payer: String,
}

/// Handle a 402 Payment Required response by signing and paying.
pub async fn handle_402(
    wallet: &Wallet,
    envelope: &PaymentEnvelope,
    original_url: &str,
    original_body: Option<&serde_json::Value>,
    api_key: &str,
) -> Result<reqwest::Response> {
    info!(
        "Handling 402: paying {} USDC to {} (ref: {})",
        envelope.amount, envelope.recipient, envelope.reference
    );

    // Sign the payment authorization
    let message = format!(
        "x402 payment authorization\nrecipient:{}\namount:{}\ntoken:{}\nchain:{}\nreference:{}",
        envelope.recipient, envelope.amount, envelope.token, envelope.chain_id, envelope.reference
    );

    let signature = wallet
        .sign_message(message.as_bytes())
        .context("Failed to sign payment authorization")?;

    let proof = PaymentProof {
        signature,
        reference: envelope.reference.clone(),
        payer: wallet.address.clone(),
    };

    let proof_json = serde_json::to_string(&proof)?;
    let proof_b64 = base64_encode(proof_json.as_bytes());

    // Retry the original request with the payment header
    let client = reqwest::Client::new();
    let mut builder = client
        .post(original_url)
        .bearer_auth(api_key)
        .header("X-Payment", proof_b64);

    if let Some(body) = original_body {
        builder = builder.json(body);
    }

    let resp = builder
        .send()
        .await
        .context("Failed to send paid request")?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        bail!("Paid request still failed ({}): {}", status, body);
    }

    // We need to re-send the request to get the response
    // Since we consumed resp for error checking, let's re-do it
    let mut builder2 = client
        .post(original_url)
        .bearer_auth(api_key)
        .header("X-Payment", base64_encode(serde_json::to_string(&proof)?.as_bytes()));

    if let Some(body) = original_body {
        builder2 = builder2.json(body);
    }

    builder2.send().await.context("Paid retry request failed")
}

/// Simple base64 encoding (no external dep).
fn base64_encode(data: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut result = String::new();

    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = if chunk.len() > 1 { chunk[1] as u32 } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as u32 } else { 0 };

        let triple = (b0 << 16) | (b1 << 8) | b2;

        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);

        if chunk.len() > 1 {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }

        if chunk.len() > 2 {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
    }

    result
}
