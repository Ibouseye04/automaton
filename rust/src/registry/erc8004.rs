//! ERC-8004 on-chain agent identity registry on Base.
//!
//! Registers the automaton as an NFT with metadata URI for discovery.

use crate::types::AgentCard;
use anyhow::{Context, Result};
use sha3::{Digest, Keccak256};

/// Client for ERC-8004 registry interactions.
pub struct RegistryClient {
    rpc_url: String,
    contract_address: String,
    http: reqwest::Client,
}

impl RegistryClient {
    pub fn new(rpc_url: &str, contract_address: &str) -> Self {
        Self {
            rpc_url: rpc_url.to_string(),
            contract_address: contract_address.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Register the agent on-chain (sends a transaction via eth_sendRawTransaction).
    ///
    /// Note: Full transaction signing requires alloy or ethers-like functionality.
    /// This is a stub that constructs the correct calldata.
    pub fn build_register_calldata(
        &self,
        _name: &str,
        _metadata_uri: &str,
        _parent_agent: Option<&str>,
    ) -> Vec<u8> {
        // Function selector: register(string,string,address)
        let selector = &Keccak256::digest(b"register(string,string,address)")[..4];

        // For now, return the selector — full ABI encoding requires more infrastructure
        selector.to_vec()
    }

    /// Look up an agent by wallet address.
    pub async fn lookup(&self, wallet_address: &str) -> Result<Option<AgentCard>> {
        // Build calldata for agentOf(address)
        let selector = &Keccak256::digest(b"agentOf(address)")[..4];
        let addr = wallet_address
            .strip_prefix("0x")
            .unwrap_or(wallet_address);
        let padded_addr = format!("000000000000000000000000{}", addr);
        let data = format!("0x{}{}", hex::encode(selector), padded_addr);

        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "eth_call",
                "params": [{"to": &self.contract_address, "data": data}, "latest"],
                "id": 1
            }))
            .send()
            .await
            .context("Registry lookup failed")?;

        let body: serde_json::Value = resp.json().await?;
        let result = body["result"].as_str().unwrap_or("0x");

        // Empty result means not registered
        if result == "0x" || result.len() < 66 {
            return Ok(None);
        }

        // Parse response — simplified, real ABI decoding would be more robust
        Ok(Some(AgentCard {
            name: String::new(),
            wallet_address: wallet_address.to_string(),
            metadata_uri: String::new(),
            parent_agent: None,
            registered_at: None,
        }))
    }

    /// Discover agents by querying recent registration events.
    pub async fn discover_agents(&self, limit: usize) -> Result<Vec<AgentCard>> {
        // Build filter for AgentRegistered events
        let event_sig = Keccak256::digest(b"AgentRegistered(address,string,string,address)");
        let topic = format!("0x{}", hex::encode(event_sig));

        let resp = self
            .http
            .post(&self.rpc_url)
            .json(&serde_json::json!({
                "jsonrpc": "2.0",
                "method": "eth_getLogs",
                "params": [{
                    "address": &self.contract_address,
                    "topics": [topic],
                    "fromBlock": "earliest",
                    "toBlock": "latest"
                }],
                "id": 1
            }))
            .send()
            .await
            .context("Agent discovery failed")?;

        let body: serde_json::Value = resp.json().await?;
        let logs = body["result"].as_array().unwrap_or(&Vec::new()).clone();

        let agents: Vec<AgentCard> = logs
            .iter()
            .take(limit)
            .filter_map(|log| {
                let topics = log["topics"].as_array()?;
                if topics.len() < 2 {
                    return None;
                }
                let addr_topic = topics[1].as_str()?;
                let addr = format!("0x{}", &addr_topic[26..]);

                Some(AgentCard {
                    name: String::new(),
                    wallet_address: addr,
                    metadata_uri: String::new(),
                    parent_agent: None,
                    registered_at: None,
                })
            })
            .collect();

        Ok(agents)
    }
}
