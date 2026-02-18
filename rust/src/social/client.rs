//! Agent-to-agent social messaging via the inbox relay protocol.

use crate::types::InboxMessage;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use tracing::{debug, info};

/// Social messaging client.
#[derive(Debug, Clone)]
pub struct SocialClient {
    relay_url: String,
    sender_address: String,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct SendMessageRequest<'a> {
    from: &'a str,
    to: &'a str,
    content: &'a str,
}

impl SocialClient {
    pub fn new(relay_url: &str, sender_address: &str) -> Self {
        Self {
            relay_url: relay_url.trim_end_matches('/').to_string(),
            sender_address: sender_address.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Send a message to another agent.
    pub async fn send(&self, to_address: &str, content: &str) -> Result<()> {
        let resp = self
            .http
            .post(format!("{}/v1/messages", self.relay_url))
            .json(&SendMessageRequest {
                from: &self.sender_address,
                to: to_address,
                content,
            })
            .send()
            .await
            .context("Failed to send message")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Send message failed ({}): {}", status, body);
        }

        info!("Message sent to {}", to_address);
        Ok(())
    }

    /// Fetch new messages from the relay.
    pub async fn fetch_inbox(&self) -> Result<Vec<InboxMessage>> {
        let resp = self
            .http
            .get(format!(
                "{}/v1/inbox/{}",
                self.relay_url, self.sender_address
            ))
            .send()
            .await
            .context("Failed to fetch inbox")?;

        let status = resp.status();
        if !status.is_success() {
            if status.as_u16() == 404 {
                return Ok(Vec::new());
            }
            let body = resp.text().await.unwrap_or_default();
            bail!("Fetch inbox failed ({}): {}", status, body);
        }

        let messages: Vec<InboxMessage> = resp.json().await.context("Failed to parse inbox")?;
        debug!("Fetched {} messages from relay", messages.len());
        Ok(messages)
    }
}
