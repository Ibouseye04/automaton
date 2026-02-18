//! EVM wallet generation and management.
//!
//! Generates or loads a secp256k1 private key, derives the Ethereum address,
//! and persists the key to `~/.automaton/wallet.json` with strict file permissions.

use anyhow::{Context, Result};
use k256::ecdsa::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use std::path::{Path, PathBuf};
use tracing::info;

/// Wallet file stored at `~/.automaton/wallet.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletFile {
    /// Hex-encoded private key with 0x prefix.
    #[serde(rename = "privateKey")]
    pub private_key: String,
    /// ISO 8601 creation timestamp.
    #[serde(rename = "createdAt")]
    pub created_at: String,
}

/// An in-memory wallet handle.
#[derive(Debug, Clone)]
pub struct Wallet {
    /// Raw private key bytes (32 bytes).
    private_key_bytes: Vec<u8>,
    /// Hex-encoded private key with 0x prefix.
    pub private_key_hex: String,
    /// Ethereum address (checksummed).
    pub address: String,
    /// Path to the wallet file on disk.
    pub path: PathBuf,
}

impl Wallet {
    /// Load an existing wallet or generate a new one at the given path.
    pub fn load_or_create(wallet_path: &Path) -> Result<Self> {
        if wallet_path.exists() {
            Self::load(wallet_path)
        } else {
            Self::generate(wallet_path)
        }
    }

    /// Load a wallet from disk.
    pub fn load(wallet_path: &Path) -> Result<Self> {
        let contents =
            std::fs::read_to_string(wallet_path).context("Failed to read wallet file")?;
        let file: WalletFile =
            serde_json::from_str(&contents).context("Failed to parse wallet JSON")?;

        let key_hex = file.private_key.strip_prefix("0x").unwrap_or(&file.private_key);
        let key_bytes = hex::decode(key_hex).context("Invalid hex in private key")?;

        let address = derive_address(&key_bytes)?;

        info!("Loaded wallet: {}", address);

        Ok(Self {
            private_key_bytes: key_bytes,
            private_key_hex: file.private_key,
            address,
            path: wallet_path.to_path_buf(),
        })
    }

    /// Generate a new random wallet and persist it.
    pub fn generate(wallet_path: &Path) -> Result<Self> {
        let signing_key = SigningKey::random(&mut OsRng);
        let key_bytes = signing_key.to_bytes().to_vec();
        let key_hex = format!("0x{}", hex::encode(&key_bytes));
        let address = derive_address(&key_bytes)?;

        let file = WalletFile {
            private_key: key_hex.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        // Ensure parent directory exists
        if let Some(parent) = wallet_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(&file)?;
        std::fs::write(wallet_path, &json).context("Failed to write wallet file")?;

        // Restrict permissions (Unix only)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(wallet_path, std::fs::Permissions::from_mode(0o600))?;
        }

        info!("Generated new wallet: {}", address);

        Ok(Self {
            private_key_bytes: key_bytes,
            private_key_hex: key_hex,
            address,
            path: wallet_path.to_path_buf(),
        })
    }

    /// Sign a message using EIP-191 personal sign.
    pub fn sign_message(&self, message: &[u8]) -> Result<String> {
        let signing_key = SigningKey::from_bytes(self.private_key_bytes.as_slice().into())
            .context("Invalid private key")?;

        // EIP-191 prefix
        let prefixed = format!(
            "\x19Ethereum Signed Message:\n{}{}",
            message.len(),
            String::from_utf8_lossy(message)
        );
        let hash = Keccak256::digest(prefixed.as_bytes());

        let (signature, recovery_id) = signing_key
            .sign_prehash_recoverable(&hash)
            .context("Signing failed")?;

        let mut sig_bytes = signature.to_bytes().to_vec();
        sig_bytes.push(recovery_id.to_byte() + 27);

        Ok(format!("0x{}", hex::encode(sig_bytes)))
    }

    /// Get the private key bytes (for internal use only).
    pub fn private_key_bytes(&self) -> &[u8] {
        &self.private_key_bytes
    }
}

/// Derive an Ethereum address from raw private key bytes.
fn derive_address(private_key: &[u8]) -> Result<String> {
    let signing_key =
        SigningKey::from_bytes(private_key.into()).context("Invalid private key bytes")?;
    let verifying_key = signing_key.verifying_key();

    // Get the uncompressed public key (65 bytes: 0x04 || x || y)
    let pubkey_bytes = verifying_key.to_encoded_point(false);
    let pubkey_uncompressed = pubkey_bytes.as_bytes();

    // Keccak256 of the public key (skip the 0x04 prefix byte)
    let hash = Keccak256::digest(&pubkey_uncompressed[1..]);

    // Take last 20 bytes as the address
    let address_bytes = &hash[12..];
    let address = format!("0x{}", hex::encode(address_bytes));

    // Return checksummed address
    Ok(checksum_address(&address))
}

/// EIP-55 checksum an Ethereum address.
fn checksum_address(address: &str) -> String {
    let addr = address.strip_prefix("0x").unwrap_or(address).to_lowercase();
    let hash = Keccak256::digest(addr.as_bytes());
    let hash_hex = hex::encode(hash);

    let mut checksummed = String::with_capacity(42);
    checksummed.push_str("0x");

    for (i, c) in addr.chars().enumerate() {
        if c.is_ascii_alphabetic() {
            let nibble = u8::from_str_radix(&hash_hex[i..i + 1], 16).unwrap_or(0);
            if nibble >= 8 {
                checksummed.push(c.to_ascii_uppercase());
            } else {
                checksummed.push(c);
            }
        } else {
            checksummed.push(c);
        }
    }

    checksummed
}
