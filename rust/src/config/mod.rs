pub mod schema;

pub use schema::AutomatonConfig;

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Default automaton home directory (~/.automaton).
pub fn default_home_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|d| d.home_dir().join(".automaton"))
        .unwrap_or_else(|| PathBuf::from(".automaton"))
}

/// Load config from the given path, or return defaults.
pub fn load_config(path: &Path) -> Result<AutomatonConfig> {
    if path.exists() {
        let contents =
            std::fs::read_to_string(path).context("Failed to read automaton config file")?;
        let config: AutomatonConfig =
            toml::from_str(&contents).context("Failed to parse automaton config (TOML)")?;
        Ok(config)
    } else {
        Ok(AutomatonConfig::default())
    }
}

/// Save config to the given path (TOML format).
pub fn save_config(config: &AutomatonConfig, path: &Path) -> Result<()> {
    let contents = toml::to_string_pretty(config).context("Failed to serialize config")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents).context("Failed to write config file")?;
    Ok(())
}
