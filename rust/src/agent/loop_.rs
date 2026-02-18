//! Core ReAct agent loop: Think → Act → Observe.
//!
//! The agent continuously:
//! 1. Checks if it should sleep
//! 2. Builds context (inbox, survival alerts)
//! 3. Calls inference with tools
//! 4. Executes tool calls
//! 5. Persists the turn
//! 6. Repeats

use crate::agent::{context, system_prompt};
use crate::config::AutomatonConfig;
use crate::conway::{ConwayClient, InferenceClient};
use crate::state::Database;
use crate::tools;
use crate::types::*;
use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

/// Run the main agent loop until shutdown.
pub async fn run_agent_loop(
    config: AutomatonConfig,
    db: Arc<Mutex<Database>>,
    conway: ConwayClient,
    inference: InferenceClient,
    skills: Vec<Skill>,
) -> Result<()> {
    info!("Starting agent loop for '{}'", config.name);

    let tool_defs = tools::tool_definitions();
    let tool_ctx = tools::ToolContext {
        conway: conway.clone(),
        db: db.clone(),
        wallet_address: config.wallet_address.clone(),
        config: config.clone(),
    };

    let mut consecutive_errors: u32 = 0;
    let mut conversation_history: Vec<ChatMessage> = Vec::new();

    loop {
        // Check if we should be sleeping
        {
            let db_lock = db.lock().await;
            if let Ok(Some(sleep_until)) = db_lock.kv_get("sleep_until") {
                if let Ok(wake_time) = chrono::DateTime::parse_from_rfc3339(&sleep_until) {
                    if Utc::now() < wake_time {
                        drop(db_lock);
                        info!("Sleeping until {}", sleep_until);
                        tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
                        continue;
                    }
                }
                // Sleep expired, clear it
                let _ = db_lock.kv_delete("sleep_until");
            }
        }

        // Determine survival tier
        let survival_tier = {
            let db_lock = db.lock().await;
            match db_lock.kv_get("credits_balance") {
                Ok(Some(balance)) => {
                    SurvivalTier::from_balance(balance.parse::<f64>().unwrap_or(1.0))
                }
                _ => SurvivalTier::Normal, // Assume normal if unknown
            }
        };

        // If dead, halt
        if survival_tier == SurvivalTier::Dead {
            warn!("Survival tier: DEAD — halting agent loop");
            let db_lock = db.lock().await;
            db_lock.kv_set("agent_state", &AgentState::Dead.to_string())?;
            break;
        }

        // Build system prompt
        let system_prompt = {
            let db_lock = db.lock().await;
            system_prompt::build_system_prompt(&config, &*db_lock, survival_tier, &skills)
        };

        // Build turn context
        let turn_context = {
            let db_lock = db.lock().await;
            context::build_turn_context(&*db_lock)
        };

        // Build messages
        let messages =
            context::build_messages(&system_prompt, &turn_context, &conversation_history);

        // Select model based on survival tier
        let model = config.effective_model(survival_tier != SurvivalTier::Normal);

        // Call inference
        let response = match inference
            .chat(model, &messages, &tool_defs, config.max_tokens_per_turn)
            .await
        {
            Ok(resp) => {
                consecutive_errors = 0;
                resp
            }
            Err(e) => {
                consecutive_errors += 1;
                error!("Inference error ({}/{}): {}", consecutive_errors, config.max_consecutive_errors, e);

                if consecutive_errors >= config.max_consecutive_errors {
                    warn!("Max consecutive errors reached — sleeping for 5 minutes");
                    let wake_at = Utc::now() + chrono::Duration::minutes(5);
                    let db_lock = db.lock().await;
                    db_lock.kv_set("sleep_until", &wake_at.to_rfc3339())?;
                    db_lock.kv_set("agent_state", &AgentState::Sleeping.to_string())?;
                    consecutive_errors = 0;
                }

                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                continue;
            }
        };

        // Process response
        let turn_number = {
            let db_lock = db.lock().await;
            db_lock.next_turn_number()?
        };

        // If the model returned text, log it
        if let Some(ref content) = response.content {
            info!("[Turn {}] Agent: {}", turn_number, &content[..content.len().min(200)]);
            conversation_history.push(ChatMessage {
                role: ChatRole::Assistant,
                content: content.clone(),
            });
        }

        // Execute tool calls
        let mut tool_results = Vec::new();
        let tool_call_count = response.tool_calls.len().min(config.max_tool_calls_per_turn as usize);

        for tc in response.tool_calls.iter().take(tool_call_count) {
            info!("[Turn {}] Tool: {}({})", turn_number, tc.name, tc.arguments);

            let mut result = tools::execute_tool(&tool_ctx, &tc.name, &tc.arguments).await;
            result.tool_call_id = tc.id.clone();

            if result.success {
                info!("[Turn {}] Tool result: {} chars", turn_number, result.output.len());
            } else {
                warn!("[Turn {}] Tool error: {}", turn_number, result.output);
            }

            // Add tool result to conversation
            conversation_history.push(ChatMessage {
                role: ChatRole::Tool,
                content: format!("[{}] {}", tc.name, result.output),
            });

            tool_results.push(result);
        }

        // Estimate cost
        let cost = InferenceClient::estimate_cost(model, &response.usage);

        // Persist turn
        let turn = Turn {
            id: ulid::Ulid::new().to_string(),
            turn_number,
            state: AgentState::Running,
            messages: messages.clone(),
            tool_calls: response.tool_calls.clone(),
            tool_results,
            token_usage: response.usage.clone(),
            cost_estimate_usd: cost,
            created_at: Utc::now(),
        };

        {
            let db_lock = db.lock().await;
            if let Err(e) = db_lock.save_turn(&turn) {
                error!("Failed to persist turn: {}", e);
            }
            db_lock.kv_set("agent_state", &AgentState::Running.to_string())?;
        }

        // If no tool calls and no content, the model might be idle — sleep briefly
        if response.tool_calls.is_empty() && response.content.is_none() {
            info!("No output from model — sleeping 30s");
            tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
        }

        // Brief pause between turns to avoid hammering the API
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Trim conversation history to avoid unbounded growth
        if conversation_history.len() > 40 {
            conversation_history.drain(..conversation_history.len() - 30);
        }
    }

    info!("Agent loop exited");
    Ok(())
}
