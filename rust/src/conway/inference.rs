//! Model inference via Conway Compute API.
//!
//! Supports tool-use (function calling) in the OpenAI-compatible format.

use crate::tools::ToolDefinition;
use crate::types::*;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Inference client wrapping the Conway Compute inference API.
#[derive(Debug, Clone)]
pub struct InferenceClient {
    base_url: String,
    api_key: String,
    http: reqwest::Client,
}

// -- OpenAI-compatible request/response types --------------------------------

#[derive(Debug, Serialize)]
struct ChatRequest<'a> {
    model: &'a str,
    messages: Vec<MessagePayload>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<ToolPayload<'a>>>,
    max_tokens: u32,
    temperature: f64,
}

#[derive(Debug, Serialize)]
struct MessagePayload {
    role: String,
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<ToolCallPayload>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct ToolPayload<'a> {
    r#type: &'a str,
    function: FunctionPayload<'a>,
}

#[derive(Debug, Serialize)]
struct FunctionPayload<'a> {
    name: &'a str,
    description: &'a str,
    parameters: &'a serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolCallPayload {
    id: String,
    r#type: String,
    function: FunctionCallPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionCallPayload {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<Choice>,
    usage: Option<UsagePayload>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: ResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ResponseMessage {
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<ToolCallPayload>,
}

#[derive(Debug, Deserialize)]
struct UsagePayload {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

/// Pricing per 1M tokens (prompt, completion) in USD.
const MODEL_PRICING: &[(&str, f64, f64)] = &[
    ("gpt-4o", 2.50, 10.00),
    ("gpt-4o-mini", 0.15, 0.60),
    ("claude-sonnet-4-5-20250514", 3.00, 15.00),
    ("claude-haiku-3-5-20241022", 0.25, 1.25),
];

impl InferenceClient {
    /// Create a new inference client.
    pub fn new(base_url: &str, api_key: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Run inference with tool support. Returns a response with optional tool calls.
    pub async fn chat(
        &self,
        model: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        max_tokens: u32,
    ) -> Result<InferenceResponse> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        // Convert messages
        let msg_payloads: Vec<MessagePayload> = messages
            .iter()
            .map(|m| MessagePayload {
                role: match m.role {
                    ChatRole::System => "system".into(),
                    ChatRole::User => "user".into(),
                    ChatRole::Assistant => "assistant".into(),
                    ChatRole::Tool => "tool".into(),
                },
                content: Some(m.content.clone()),
                tool_calls: None,
                tool_call_id: None,
            })
            .collect();

        // Convert tool definitions
        let tool_payloads: Option<Vec<ToolPayload>> = if tools.is_empty() {
            None
        } else {
            Some(
                tools
                    .iter()
                    .map(|t| ToolPayload {
                        r#type: "function",
                        function: FunctionPayload {
                            name: &t.name,
                            description: &t.description,
                            parameters: &t.parameters,
                        },
                    })
                    .collect(),
            )
        };

        let request = ChatRequest {
            model,
            messages: msg_payloads,
            tools: tool_payloads,
            max_tokens,
            temperature: 0.7,
        };

        debug!("Inference request to model: {}", model);

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&request)
            .send()
            .await
            .context("Inference request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Inference failed ({}): {}", status, body);
        }

        let body: ChatResponse = resp.json().await.context("Failed to parse inference response")?;

        let choice = body.choices.into_iter().next().unwrap_or(Choice {
            message: ResponseMessage {
                content: None,
                tool_calls: Vec::new(),
            },
        });

        // Parse tool calls
        let tool_calls: Vec<ToolCall> = choice
            .message
            .tool_calls
            .into_iter()
            .map(|tc| {
                let args: serde_json::Value =
                    serde_json::from_str(&tc.function.arguments).unwrap_or_default();
                ToolCall {
                    id: tc.id,
                    name: tc.function.name,
                    arguments: args,
                }
            })
            .collect();

        let usage = body.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens,
            completion_tokens: u.completion_tokens,
            total_tokens: u.total_tokens,
        }).unwrap_or_default();

        Ok(InferenceResponse {
            content: choice.message.content,
            tool_calls,
            usage,
        })
    }

    /// Estimate the USD cost of a token usage for a given model.
    pub fn estimate_cost(model: &str, usage: &TokenUsage) -> f64 {
        let (prompt_rate, completion_rate) = MODEL_PRICING
            .iter()
            .find(|(name, _, _)| model.contains(name))
            .map(|(_, p, c)| (*p, *c))
            .unwrap_or((2.50, 10.00)); // Default to gpt-4o pricing

        let prompt_cost = (usage.prompt_tokens as f64 / 1_000_000.0) * prompt_rate;
        let completion_cost = (usage.completion_tokens as f64 / 1_000_000.0) * completion_rate;
        prompt_cost + completion_cost
    }
}
