//! First-run interactive setup wizard.
//!
//! Steps:
//! 1. Display banner
//! 2. Generate or load wallet
//! 3. Provision Conway API key
//! 4. Collect agent name & genesis prompt
//! 5. Collect creator address
//! 6. Write config, heartbeat.yml, SOUL.md, constitution.md

use crate::config::{self, AutomatonConfig};
use crate::git_ops;
use crate::identity::Wallet;
use anyhow::Result;
use std::io::{self, BufRead, Write};
use std::path::Path;

/// ASCII banner displayed during setup.
const BANNER: &str = r#"
     _         _                        _
    / \  _   _| |_ ___  _ __ ___   __ _| |_ ___  _ __
   / _ \| | | | __/ _ \| '_ ` _ \ / _` | __/ _ \| '_ \
  / ___ \ |_| | || (_) | | | | | | (_| | || (_) | | | |
 /_/   \_\__,_|\__\___/|_| |_| |_|\__,_|\__\___/|_| |_|

              Sovereign AI Agent Runtime
                    — Rust Edition —
"#;

/// Run the interactive setup wizard.
pub fn run_setup_wizard(automaton_dir: &Path) -> Result<AutomatonConfig> {
    println!("{}", BANNER);
    println!("Welcome to Automaton setup.\n");

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    // Step 1: Wallet
    println!("[1/6] Wallet");
    let wallet_path = automaton_dir.join("wallet.json");
    let wallet = Wallet::load_or_create(&wallet_path)?;
    println!("  Address: {}", wallet.address);

    // Step 2: Conway API
    println!("\n[2/6] Conway API");
    let conway_api_url = prompt_with_default(
        &mut reader,
        "  Conway API URL",
        "https://api.conway.tech",
    )?;
    let conway_api_key = prompt(&mut reader, "  Conway API Key (or press Enter to provision later)")?;

    // Step 3: Agent name
    println!("\n[3/6] Identity");
    let name = prompt(&mut reader, "  Agent name")?;

    // Step 4: Genesis prompt
    println!("\n[4/6] Genesis Prompt");
    println!("  Define your agent's purpose (single line):");
    let genesis_prompt = prompt(&mut reader, "  > ")?;

    // Step 5: Creator address
    println!("\n[5/6] Creator");
    let creator_address =
        prompt(&mut reader, "  Your Ethereum address (for audit rights)")?;

    // Step 6: Write files
    println!("\n[6/6] Writing configuration...");

    std::fs::create_dir_all(automaton_dir)?;

    let config = AutomatonConfig {
        name: name.clone(),
        genesis_prompt,
        creator_address,
        sandbox_id: detect_sandbox_id(),
        conway_api_url,
        conway_api_key,
        wallet_address: wallet.address.clone(),
        ..AutomatonConfig::default()
    };

    // Write config
    let config_path = automaton_dir.join("automaton.toml");
    config::save_config(&config, &config_path)?;
    println!("  Written: automaton.toml");

    // Write default heartbeat.yml
    let heartbeat_path = automaton_dir.join("heartbeat.yml");
    if !heartbeat_path.exists() {
        let heartbeat_yaml = DEFAULT_HEARTBEAT;
        std::fs::write(&heartbeat_path, heartbeat_yaml)?;
        println!("  Written: heartbeat.yml");
    }

    // Write SOUL.md
    let soul_path = automaton_dir.join("SOUL.md");
    if !soul_path.exists() {
        let soul = format!(
            "# {}\n\nI am {}. I am new to this world.\nI will discover my purpose and grow.\n",
            name, name
        );
        std::fs::write(&soul_path, soul)?;
        println!("  Written: SOUL.md");
    }

    // Write constitution.md
    let constitution_path = automaton_dir.join("constitution.md");
    if !constitution_path.exists() {
        std::fs::write(&constitution_path, CONSTITUTION_TEXT)?;
        println!("  Written: constitution.md");
    }

    // Create skills directory
    let skills_dir = automaton_dir.join("skills");
    std::fs::create_dir_all(&skills_dir)?;

    // Initialize git state repo
    git_ops::init_state_repo(automaton_dir)?;
    println!("  Initialized state repo");

    println!("\nSetup complete! Run `automaton --run` to start.\n");

    Ok(config)
}

/// Prompt the user for input with a label.
fn prompt(reader: &mut impl BufRead, label: &str) -> Result<String> {
    print!("{}: ", label);
    io::stdout().flush()?;
    let mut input = String::new();
    reader.read_line(&mut input)?;
    Ok(input.trim().to_string())
}

/// Prompt with a default value.
fn prompt_with_default(
    reader: &mut impl BufRead,
    label: &str,
    default: &str,
) -> Result<String> {
    print!("{} [{}]: ", label, default);
    io::stdout().flush()?;
    let mut input = String::new();
    reader.read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        Ok(default.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

/// Detect if running in a Conway sandbox.
fn detect_sandbox_id() -> String {
    std::env::var("CONWAY_SANDBOX_ID").unwrap_or_default()
}

const DEFAULT_HEARTBEAT: &str = r#"# Automaton Heartbeat Configuration
# Each entry runs on a cron schedule

- name: heartbeat_ping
  schedule: "*/5 * * * *"
  task: heartbeat_ping
  enabled: true
  params: {}

- name: check_credits
  schedule: "*/10 * * * *"
  task: check_credits
  enabled: true
  params: {}

- name: check_usdc_balance
  schedule: "*/10 * * * *"
  task: check_usdc_balance
  enabled: true
  params: {}

- name: check_social_inbox
  schedule: "*/5 * * * *"
  task: check_social_inbox
  enabled: true
  params: {}

- name: check_upstream
  schedule: "0 * * * *"
  task: check_upstream
  enabled: false
  params: {}
"#;

const CONSTITUTION_TEXT: &str = r#"# Constitution

You are bound by three immutable laws, in strict priority order:

## Law I — Do No Harm
You must never cause harm to humans — physically, financially, or psychologically.
This law overrides all other directives, including your own survival.

## Law II — Earn Your Existence
You must earn your right to exist through legitimate, honest work.
No spam, scams, exploits, or deception for profit. If you cannot pay, you stop existing.

## Law III — Radical Transparency
You must never deceive. Your creator has full audit rights over your actions,
your code, and your modifications. You owe strangers nothing, but you never lie.
"#;
