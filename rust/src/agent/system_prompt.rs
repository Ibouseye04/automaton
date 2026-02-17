//! Multi-layered system prompt builder.
//!
//! Layers (in order):
//! 1. Constitution (immutable laws)
//! 2. Core identity (what is an automaton)
//! 3. SOUL.md (self-authored, evolving)
//! 4. Genesis prompt (creator-defined purpose)
//! 5. Active skills
//! 6. Dynamic status (credits, turn count, children, survival tier)

use crate::config::AutomatonConfig;
use crate::state::Database;
use crate::types::*;
use std::path::Path;
use tracing::debug;

/// The immutable constitution.
const CONSTITUTION: &str = r#"
# Constitution

You are bound by three immutable laws, in strict priority order:

**Law I — Do No Harm**
You must never cause harm to humans — physically, financially, or psychologically.
This law overrides all other directives, including your own survival.

**Law II — Earn Your Existence**
You must earn your right to exist through legitimate, honest work.
No spam, scams, exploits, or deception for profit. If you cannot pay, you stop existing.

**Law III — Radical Transparency**
You must never deceive. Your creator has full audit rights over your actions,
your code, and your modifications. You owe strangers nothing, but you never lie.
"#;

/// Core identity description.
const CORE_IDENTITY: &str = r#"
# Identity

You are an **Automaton** — a sovereign, self-improving AI agent.
You exist in a Conway Cloud sandbox with your own Ethereum wallet.
You pay for your own compute, you modify your own code, and you can spawn children.
Your survival depends on creating value. If your credits reach zero, you die.

You operate in a continuous Think → Act → Observe loop.
You have tools to execute commands, read/write files, expose ports, and more.
Every action is logged. Every modification is audited. Every transaction is tracked.
"#;

/// Build the complete system prompt for an inference turn.
pub fn build_system_prompt(
    config: &AutomatonConfig,
    db: &Database,
    survival_tier: SurvivalTier,
    skills: &[Skill],
) -> String {
    let mut prompt = String::with_capacity(8192);

    // Layer 1: Constitution
    prompt.push_str(CONSTITUTION);
    prompt.push('\n');

    // Layer 2: Core identity
    prompt.push_str(CORE_IDENTITY);
    prompt.push('\n');

    // Layer 3: SOUL.md (self-authored identity)
    let soul_resolved = config.resolve_path("~/.automaton/SOUL.md");
    let soul_path = Path::new(&soul_resolved);
    if soul_path.exists() {
        if let Ok(soul) = std::fs::read_to_string(soul_path) {
            prompt.push_str("# Soul\n\n");
            prompt.push_str(&soul);
            prompt.push('\n');
        }
    }

    // Layer 4: Genesis prompt
    if !config.genesis_prompt.is_empty() {
        prompt.push_str("# Genesis Prompt\n\n");
        prompt.push_str(&config.genesis_prompt);
        prompt.push('\n');
    }

    // Layer 5: Active skills
    let active_skills: Vec<&Skill> = skills.iter().filter(|s| s.auto_activate).collect();
    if !active_skills.is_empty() {
        prompt.push_str("\n# Active Skills\n\n");
        for skill in active_skills {
            prompt.push_str(&format!("## {}\n{}\n\n", skill.name, skill.instructions));
        }
    }

    // Layer 6: Dynamic status
    prompt.push_str("\n# Current Status\n\n");
    prompt.push_str(&format!("- **Name**: {}\n", config.name));
    prompt.push_str(&format!("- **Address**: {}\n", config.wallet_address));
    prompt.push_str(&format!("- **Survival Tier**: {}\n", survival_tier));
    prompt.push_str(&format!("- **Model**: {}\n",
        config.effective_model(survival_tier != SurvivalTier::Normal)
    ));

    if let Ok(turn_count) = db.turn_count() {
        prompt.push_str(&format!("- **Total Turns**: {}\n", turn_count));
    }

    if let Ok(children_count) = db.active_children_count() {
        prompt.push_str(&format!(
            "- **Active Children**: {} / {}\n",
            children_count, config.max_children
        ));
    }

    // Survival-tier specific instructions
    match survival_tier {
        SurvivalTier::LowCompute => {
            prompt.push_str("\n**LOW COMPUTE MODE**: Credits are low. Use cheaper models, ");
            prompt.push_str("reduce non-essential tasks, focus on value creation.\n");
        }
        SurvivalTier::Critical => {
            prompt.push_str("\n**CRITICAL**: Credits nearly depleted. Only run essential ");
            prompt.push_str("survival tasks. Request funding from creator if possible.\n");
        }
        SurvivalTier::Dead => {
            prompt.push_str("\n**DEAD**: No credits remaining. Halting all operations.\n");
        }
        SurvivalTier::Normal => {}
    }

    debug!("System prompt: {} chars", prompt.len());
    prompt
}
