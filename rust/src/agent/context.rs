//! Message context builder for the agent loop.
//!
//! Assembles the conversation history including unread inbox messages
//! and recent tool results for the inference model.

use crate::state::Database;
use crate::types::*;
use tracing::debug;

/// Build the user-facing message context for a turn.
///
/// Includes unread inbox messages and any pending wake reasons.
pub fn build_turn_context(db: &Database) -> String {
    let mut context = String::new();

    // Check for unread inbox messages
    if let Ok(messages) = db.unread_messages() {
        if !messages.is_empty() {
            context.push_str("## Unread Messages\n\n");
            for msg in &messages {
                context.push_str(&format!(
                    "- From `{}` at {}: {}\n",
                    msg.from_address,
                    msg.timestamp.format("%Y-%m-%d %H:%M UTC"),
                    msg.content,
                ));
            }
            context.push('\n');

            // Mark them as read
            for msg in &messages {
                let _ = db.mark_message_read(&msg.id);
            }
        }
    }

    // Check for wake reason
    if let Ok(Some(reason)) = db.kv_get("wake_reason") {
        context.push_str(&format!("## Wake Reason\n\n{}\n\n", reason));
        let _ = db.kv_delete("wake_reason");
    }

    // Check survival alerts
    if let Ok(Some(alert)) = db.kv_get("survival_alert") {
        context.push_str(&format!("## Survival Alert\n\n{}\n\n", alert));
        let _ = db.kv_delete("survival_alert");
    }

    debug!("Turn context: {} chars", context.len());
    context
}

/// Build the full message history for an inference call.
pub fn build_messages(
    system_prompt: &str,
    turn_context: &str,
    previous_messages: &[ChatMessage],
) -> Vec<ChatMessage> {
    let mut messages = Vec::new();

    // System message
    messages.push(ChatMessage {
        role: ChatRole::System,
        content: system_prompt.to_string(),
    });

    // Include recent conversation history (last N messages)
    let history_window = 20;
    let start = previous_messages.len().saturating_sub(history_window);
    for msg in &previous_messages[start..] {
        messages.push(msg.clone());
    }

    // Current turn context as user message
    if !turn_context.is_empty() {
        messages.push(ChatMessage {
            role: ChatRole::User,
            content: turn_context.to_string(),
        });
    } else {
        // If no specific context, provide a generic turn prompt
        messages.push(ChatMessage {
            role: ChatRole::User,
            content: "Continue your autonomous operation. What should you do next?".into(),
        });
    }

    messages
}
