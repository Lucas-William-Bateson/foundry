use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::{Child, Command};
use tracing::info;

use foundry_core::cloudflare::CloudflareClient;

pub struct CloudflareConfig {
    pub account_id: String,
    pub api_token: String,
    pub zone_id: String,
    pub tunnel_name: String,
    pub domain: String,
    pub local_port: u16,
}

pub struct CloudflareTunnel {
    _process: Child,
    pub tunnel_id: String,
    pub domain: String,
}

impl CloudflareTunnel {
    pub async fn start(config: CloudflareConfig) -> Result<Self> {
        let client = CloudflareClient::new(
            config.account_id.clone(),
            config.api_token.clone(),
            config.zone_id.clone(),
            config.tunnel_name.clone(),
        );

        info!("Checking for existing tunnel '{}'...", config.tunnel_name);
        let tunnel = client
            .get_tunnel()
            .await?
            .context(format!("Tunnel '{}' not found. Please create it first.", config.tunnel_name))?;

        info!("Found existing tunnel: {}", tunnel.id);

        info!("Adding route for {}...", config.domain);
        let service = format!("http://127.0.0.1:{}", config.local_port);
        client.add_route(&config.domain, &service).await?;

        info!("Getting tunnel token...");
        let token = client.get_tunnel_token(&tunnel.id).await?;

        info!("Starting cloudflared...");
        let process = Command::new("cloudflared")
            .args(["tunnel", "--no-autoupdate", "run", "--token", &token])
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("Failed to start cloudflared")?;

        info!("Tunnel running at https://{}", config.domain);

        Ok(Self {
            _process: process,
            tunnel_id: tunnel.id,
            domain: config.domain,
        })
    }

    pub fn webhook_url(&self) -> String {
        format!("https://{}/webhook/github", self.domain)
    }
}
