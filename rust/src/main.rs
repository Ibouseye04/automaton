//! Automaton — Sovereign AI Agent Runtime (Rust edition for Pi).
//!
//! Usage:
//!   automaton --run          Start the agent loop
//!   automaton --setup        Run the setup wizard
//!   automaton --status       Show current status
//!   automaton --provision    Provision a Conway API key
//!   automaton --daemon       Run as a background daemon

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};

use automaton::agent;
use automaton::config;
use automaton::conway::{ConwayClient, InferenceClient};
use automaton::heartbeat::HeartbeatDaemon;
use automaton::identity::Wallet;
use automaton::skills;
use automaton::state::Database;
use automaton::survival::SurvivalMonitor;
use automaton::types::*;

// ---------------------------------------------------------------------------
// CLI definition (inspired by zeroclaw's clap derive pattern)
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(name = "automaton")]
#[command(version = "0.1.0")]
#[command(about = "Sovereign autonomous AI agent runtime")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to automaton home directory.
    #[arg(long, default_value = "~/.automaton")]
    home: String,

    /// Log level (debug, info, warn, error).
    #[arg(long, default_value = "info")]
    log_level: String,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the agent loop.
    Run,

    /// Run the first-time setup wizard.
    Setup,

    /// Show the agent's current status.
    Status,

    /// Provision a Conway API key via SIWE.
    Provision,

    /// Run as a daemon (agent loop + heartbeat).
    Daemon,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cli.log_level));
    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_target(false)
        .init();

    // Resolve home directory
    let home_dir = PathBuf::from(shellexpand::tilde(&cli.home).into_owned());

    match cli.command {
        Commands::Setup => cmd_setup(&home_dir).await,
        Commands::Run => cmd_run(&home_dir).await,
        Commands::Status => cmd_status(&home_dir).await,
        Commands::Provision => cmd_provision(&home_dir).await,
        Commands::Daemon => cmd_daemon(&home_dir).await,
    }
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

async fn cmd_setup(home_dir: &Path) -> Result<()> {
    automaton::setup::run_setup_wizard(home_dir)?;
    Ok(())
}

async fn cmd_run(home_dir: &Path) -> Result<()> {
    let (config, wallet, db) = bootstrap(home_dir)?;

    let conway = ConwayClient::new(
        &config.conway_api_url,
        &config.conway_api_key,
        &config.sandbox_id,
    );
    let inference = InferenceClient::new(&config.conway_api_url, &config.conway_api_key);
    let db = Arc::new(Mutex::new(db));

    // Load skills
    let skill_list = skills::load_skills(&config.resolved_skills_dir()).unwrap_or_default();

    println!(
        "{} Starting agent '{}' (model: {}, wallet: {})",
        ">>>".green().bold(),
        config.name,
        config.inference_model,
        wallet.address,
    );

    // Run the agent loop (no daemon, so use a no-op cancel token)
    let cancel = CancellationToken::new();
    agent::run_agent_loop(config, db, conway, inference, skill_list, cancel).await
}

async fn cmd_status(home_dir: &Path) -> Result<()> {
    let (config, wallet, db) = bootstrap(home_dir)?;
    let db = Arc::new(Mutex::new(db));

    let monitor = SurvivalMonitor::new(db.clone());
    let state = monitor.check().await?;

    let db_lock = db.lock().await;
    let agent_state = db_lock
        .kv_get("agent_state")?
        .unwrap_or_else(|| "unknown".into());
    let turn_count = db_lock.turn_count()?;
    let children_count = db_lock.active_children_count()?;
    let last_heartbeat = db_lock.kv_get("last_heartbeat")?.unwrap_or_else(|| "never".into());

    println!();
    println!("{}", "=== Automaton Status ===".bold());
    println!();
    println!("  {}:  {}", "Name".bold(), config.name);
    println!("  {}:", "Wallet".bold());
    println!("    Address:  {}", wallet.address);
    println!();
    println!("  {}:", "State".bold());
    println!("    Agent:    {}", colorize_state(&agent_state));
    println!("    Tier:     {}", colorize_tier(state.tier));
    println!();
    println!("  {}:", "Finances".bold());
    println!("    Credits:  {:.4}", state.credits_balance);
    println!("    USDC:     {:.6}", state.usdc_balance);
    println!();
    println!("  {}:", "Runtime".bold());
    println!("    Turns:    {}", turn_count);
    println!("    Children: {} / {}", children_count, config.max_children);
    println!("    Model:    {}", config.inference_model);
    println!("    Heartbeat: {}", last_heartbeat);
    println!();

    Ok(())
}

async fn cmd_provision(home_dir: &Path) -> Result<()> {
    let config_path = home_dir.join("automaton.toml");
    let mut cfg = config::load_config(&config_path)?;
    let wallet_path = home_dir.join("wallet.json");
    let wallet = Wallet::load_or_create(&wallet_path)?;

    println!("Provisioning Conway API key via SIWE...");
    println!("  Wallet: {}", wallet.address);

    let api_key =
        automaton::identity::provision::provision_api_key(&wallet, &cfg.conway_api_url).await?;

    cfg.conway_api_key = api_key;
    config::save_config(&cfg, &config_path)?;

    println!("API key provisioned and saved.");
    Ok(())
}

async fn cmd_daemon(home_dir: &Path) -> Result<()> {
    let (config, _wallet, db) = bootstrap(home_dir)?;

    let conway = ConwayClient::new(
        &config.conway_api_url,
        &config.conway_api_key,
        &config.sandbox_id,
    );
    let inference = InferenceClient::new(&config.conway_api_url, &config.conway_api_key);
    let db = Arc::new(Mutex::new(db));
    let skill_list = skills::load_skills(&config.resolved_skills_dir()).unwrap_or_default();

    println!(
        "{} Starting daemon for '{}' ...",
        ">>>".green().bold(),
        config.name,
    );

    // Create a cancellation token for graceful shutdown
    let cancel = CancellationToken::new();

    // Spawn heartbeat daemon (token is checked inside the loop)
    let heartbeat_db = db.clone();
    let heartbeat_config = config.clone();
    let heartbeat_cancel = cancel.clone();
    let heartbeat_handle = tokio::spawn(async move {
        match HeartbeatDaemon::new(heartbeat_config, heartbeat_db) {
            Ok(mut daemon) => {
                if let Err(e) = daemon.run(heartbeat_cancel).await {
                    error!("Heartbeat daemon error: {}", e);
                }
            }
            Err(e) => {
                error!("Failed to create heartbeat daemon: {}", e);
            }
        }
    });

    // Spawn agent loop (token is checked inside the loop)
    let agent_db = db.clone();
    let agent_config = config.clone();
    let agent_cancel = cancel.clone();
    let agent_handle = tokio::spawn(async move {
        if let Err(e) =
            agent::run_agent_loop(agent_config, agent_db, conway, inference, skill_list, agent_cancel).await
        {
            error!("Agent loop error: {}", e);
        }
    });

    // Wait for shutdown signal
    tokio::signal::ctrl_c()
        .await
        .context("Failed to listen for Ctrl+C")?;

    println!("\n{} Shutting down gracefully...", "<<<".red().bold());

    // Signal cancellation to all spawned tasks
    cancel.cancel();

    // Wait for tasks to finish (with a timeout to avoid hanging forever)
    let shutdown_timeout = tokio::time::Duration::from_secs(10);
    let _ = tokio::time::timeout(shutdown_timeout, async {
        if let Err(e) = heartbeat_handle.await {
            warn!("Heartbeat task join error: {}", e);
        }
        if let Err(e) = agent_handle.await {
            warn!("Agent task join error: {}", e);
        }
    })
    .await;

    // Persist final state after loops have stopped
    {
        let db_lock = db.lock().await;
        db_lock.kv_set("agent_state", &AgentState::Sleeping.to_string())?;
    }

    info!("Daemon shutdown complete");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Bootstrap the runtime: load config, wallet, and database.
fn bootstrap(home_dir: &Path) -> Result<(config::AutomatonConfig, Wallet, Database)> {
    // Ensure home directory exists
    if !home_dir.exists() {
        std::fs::create_dir_all(home_dir).with_context(|| {
            format!("Failed to create home directory: {}", home_dir.display())
        })?;
    }

    let config_path = home_dir.join("automaton.toml");

    if !config_path.exists() {
        eprintln!(
            "{} No config found at {:?}. Run `automaton setup` first.",
            "Error:".red().bold(),
            config_path
        );
        std::process::exit(1);
    }

    let cfg = config::load_config(&config_path)
        .with_context(|| format!("Failed to load config from {}", config_path.display()))?;

    let wallet_path = home_dir.join("wallet.json");
    let wallet = Wallet::load_or_create(&wallet_path)
        .with_context(|| format!("Failed to load or create wallet at {}", wallet_path.display()))?;

    let db_path = cfg.resolved_db_path();
    let db_path = std::path::Path::new(&db_path);

    // Ensure parent directory for db exists
    if let Some(parent) = db_path.parent() {
        if !parent.exists() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create DB parent directory: {}", parent.display())
            })?;
        }
    }

    let db = Database::open(db_path)
        .with_context(|| format!("Failed to open database at {}", db_path.display()))?;

    Ok((cfg, wallet, db))
}

fn colorize_state(state: &str) -> String {
    match state {
        "running" => state.green().to_string(),
        "sleeping" => state.yellow().to_string(),
        "low_compute" | "critical" => state.red().to_string(),
        "dead" => state.red().bold().to_string(),
        _ => state.dimmed().to_string(),
    }
}

fn colorize_tier(tier: SurvivalTier) -> String {
    match tier {
        SurvivalTier::Normal => "normal".green().to_string(),
        SurvivalTier::LowCompute => "low_compute".yellow().to_string(),
        SurvivalTier::Critical => "critical".red().to_string(),
        SurvivalTier::Dead => "DEAD".red().bold().to_string(),
    }
}
