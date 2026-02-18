//! Conway Cloud API client for sandbox operations, file I/O, and port management.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

/// Conway Cloud API client.
#[derive(Debug, Clone)]
pub struct ConwayClient {
    base_url: String,
    api_key: String,
    sandbox_id: String,
    http: reqwest::Client,
}

// -- Request / response types -----------------------------------------------

#[derive(Debug, Serialize)]
struct ExecRequest<'a> {
    command: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub struct ExecResponse {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

#[derive(Debug, Serialize)]
struct WriteFileRequest<'a> {
    path: &'a str,
    content: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct ReadFileResponse {
    pub content: String,
}

#[derive(Debug, Serialize)]
struct ExposePortRequest {
    port: u16,
}

#[derive(Debug, Deserialize)]
pub struct ExposePortResponse {
    pub url: String,
}

#[derive(Debug, Serialize)]
struct CreateSandboxRequest<'a> {
    name: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct CreateSandboxResponse {
    pub sandbox_id: String,
}

#[derive(Debug, Deserialize)]
pub struct DomainSearchResponse {
    pub available: bool,
    pub domain: String,
    pub price: Option<f64>,
}

impl ConwayClient {
    /// Create a new Conway Cloud client.
    pub fn new(base_url: &str, api_key: &str, sandbox_id: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            sandbox_id: sandbox_id.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Build the base URL for sandbox API calls.
    fn sandbox_url(&self, path: &str) -> String {
        format!(
            "{}/v1/sandboxes/{}/{}",
            self.base_url, self.sandbox_id, path
        )
    }

    /// Execute a shell command in the sandbox.
    pub async fn exec(&self, command: &str, timeout_ms: Option<u64>) -> Result<ExecResponse> {
        debug!("Conway exec: {}", command);

        let resp = self
            .http
            .post(self.sandbox_url("exec"))
            .bearer_auth(&self.api_key)
            .json(&ExecRequest {
                command,
                timeout_ms,
            })
            .send()
            .await
            .context("Conway exec request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Conway exec failed ({}): {}", status, body);
        }

        resp.json().await.context("Failed to parse exec response")
    }

    /// Read a file from the sandbox filesystem.
    pub async fn read_file(&self, path: &str) -> Result<String> {
        let resp = self
            .http
            .get(self.sandbox_url("files"))
            .bearer_auth(&self.api_key)
            .query(&[("path", path)])
            .send()
            .await
            .context("Conway read_file request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Conway read_file failed ({}): {}", status, body);
        }

        let body: ReadFileResponse = resp.json().await?;
        Ok(body.content)
    }

    /// Write a file to the sandbox filesystem.
    pub async fn write_file(&self, path: &str, content: &str) -> Result<()> {
        let resp = self
            .http
            .put(self.sandbox_url("files"))
            .bearer_auth(&self.api_key)
            .json(&WriteFileRequest { path, content })
            .send()
            .await
            .context("Conway write_file request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Conway write_file failed ({}): {}", status, body);
        }

        Ok(())
    }

    /// Expose a port on the sandbox to the public internet.
    pub async fn expose_port(&self, port: u16) -> Result<String> {
        let resp = self
            .http
            .post(self.sandbox_url("ports"))
            .bearer_auth(&self.api_key)
            .json(&ExposePortRequest { port })
            .send()
            .await
            .context("Conway expose_port request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Conway expose_port failed ({}): {}", status, body);
        }

        let body: ExposePortResponse = resp.json().await?;
        Ok(body.url)
    }

    /// Create a new sandbox (for child spawning).
    pub async fn create_sandbox(&self, name: &str) -> Result<String> {
        let resp = self
            .http
            .post(format!("{}/v1/sandboxes", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&CreateSandboxRequest { name })
            .send()
            .await
            .context("Conway create_sandbox request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Conway create_sandbox failed ({}): {}", status, body);
        }

        let body: CreateSandboxResponse = resp.json().await?;
        Ok(body.sandbox_id)
    }

    /// Search for a domain name.
    pub async fn search_domain(&self, domain: &str) -> Result<DomainSearchResponse> {
        let resp = self
            .http
            .get(format!("{}/v1/domains/search", self.base_url))
            .bearer_auth(&self.api_key)
            .query(&[("domain", domain)])
            .send()
            .await
            .context("Conway domain search request failed")?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            bail!("Conway domain search failed ({}): {}", status, body);
        }

        resp.json().await.context("Failed to parse domain response")
    }

    /// Get the sandbox ID.
    pub fn sandbox_id(&self) -> &str {
        &self.sandbox_id
    }
}
