use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use tokio::process::{Child, Command};
use tracing::info;

const CF_API_BASE: &str = "https://api.cloudflare.com/client/v4";

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

#[derive(Deserialize)]
struct ApiResponse<T> {
    success: bool,
    result: Option<T>,
    errors: Vec<ApiError>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Deserialize)]
struct Tunnel {
    id: String,
    name: String,
}

#[derive(Deserialize)]
struct TunnelCredentials {
    account_tag: String,
    tunnel_id: String,
    tunnel_secret: String,
}

#[derive(Serialize)]
struct TunnelConfig {
    config: TunnelConfigInner,
}

#[derive(Serialize)]
struct TunnelConfigInner {
    ingress: Vec<IngressRule>,
}

#[derive(Serialize)]
struct IngressRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>,
    service: String,
}

#[derive(Serialize)]
struct DnsRecord {
    #[serde(rename = "type")]
    record_type: String,
    name: String,
    content: String,
    proxied: bool,
}

#[derive(Deserialize)]
struct DnsRecordResult {
    id: String,
}

struct CloudflareClient {
    client: Client,
    account_id: String,
    api_token: String,
    zone_id: String,
}

impl CloudflareClient {
    fn new(config: &CloudflareConfig) -> Self {
        Self {
            client: Client::new(),
            account_id: config.account_id.clone(),
            api_token: config.api_token.clone(),
            zone_id: config.zone_id.clone(),
        }
    }

    async fn list_tunnels(&self) -> Result<Vec<Tunnel>> {
        let url = format!(
            "{}/accounts/{}/cfd_tunnel",
            CF_API_BASE, self.account_id
        );

        let resp: ApiResponse<Vec<Tunnel>> = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            let errors: Vec<_> = resp.errors.iter().map(|e| &e.message).collect();
            anyhow::bail!("Cloudflare API error: {:?}", errors);
        }

        Ok(resp.result.unwrap_or_default())
    }

    async fn create_tunnel(&self, name: &str) -> Result<(String, TunnelCredentials)> {
        let url = format!(
            "{}/accounts/{}/cfd_tunnel",
            CF_API_BASE, self.account_id
        );

        let tunnel_secret = base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            uuid::Uuid::new_v4().as_bytes(),
        );

        let body = serde_json::json!({
            "name": name,
            "tunnel_secret": tunnel_secret,
            "config_src": "cloudflare"
        });

        let resp: ApiResponse<Tunnel> = self
            .client
            .post(&url)
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            let errors: Vec<_> = resp.errors.iter().map(|e| &e.message).collect();
            anyhow::bail!("Failed to create tunnel: {:?}", errors);
        }

        let tunnel = resp.result.context("No tunnel in response")?;

        let creds = TunnelCredentials {
            account_tag: self.account_id.clone(),
            tunnel_id: tunnel.id.clone(),
            tunnel_secret,
        };

        Ok((tunnel.id, creds))
    }

    async fn update_tunnel_config(
        &self,
        tunnel_id: &str,
        domain: &str,
        local_port: u16,
    ) -> Result<()> {
        let url = format!(
            "{}/accounts/{}/cfd_tunnel/{}/configurations",
            CF_API_BASE, self.account_id, tunnel_id
        );

        let config = TunnelConfig {
            config: TunnelConfigInner {
                ingress: vec![
                    IngressRule {
                        hostname: Some(domain.to_string()),
                        service: format!("http://localhost:{}", local_port),
                    },
                    IngressRule {
                        hostname: None,
                        service: "http_status:404".to_string(),
                    },
                ],
            },
        };

        let resp: ApiResponse<serde_json::Value> = self
            .client
            .put(&url)
            .bearer_auth(&self.api_token)
            .json(&config)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            let errors: Vec<_> = resp.errors.iter().map(|e| &e.message).collect();
            anyhow::bail!("Failed to update tunnel config: {:?}", errors);
        }

        Ok(())
    }

    async fn ensure_dns_record(&self, domain: &str, tunnel_id: &str) -> Result<()> {
        let tunnel_cname = format!("{}.cfargotunnel.com", tunnel_id);

        let list_url = format!(
            "{}/zones/{}/dns_records?type=CNAME&name={}",
            CF_API_BASE, self.zone_id, domain
        );

        let resp: ApiResponse<Vec<DnsRecordResult>> = self
            .client
            .get(&list_url)
            .bearer_auth(&self.api_token)
            .send()
            .await?
            .json()
            .await?;

        if let Some(records) = resp.result {
            if !records.is_empty() {
                let update_url = format!(
                    "{}/zones/{}/dns_records/{}",
                    CF_API_BASE, self.zone_id, records[0].id
                );

                let record = DnsRecord {
                    record_type: "CNAME".to_string(),
                    name: domain.to_string(),
                    content: tunnel_cname,
                    proxied: true,
                };

                let _: ApiResponse<serde_json::Value> = self
                    .client
                    .put(&update_url)
                    .bearer_auth(&self.api_token)
                    .json(&record)
                    .send()
                    .await?
                    .json()
                    .await?;

                info!("Updated DNS record for {}", domain);
                return Ok(());
            }
        }

        let create_url = format!("{}/zones/{}/dns_records", CF_API_BASE, self.zone_id);

        let record = DnsRecord {
            record_type: "CNAME".to_string(),
            name: domain.to_string(),
            content: tunnel_cname,
            proxied: true,
        };

        let resp: ApiResponse<serde_json::Value> = self
            .client
            .post(&create_url)
            .bearer_auth(&self.api_token)
            .json(&record)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            let errors: Vec<_> = resp.errors.iter().map(|e| &e.message).collect();
            anyhow::bail!("Failed to create DNS record: {:?}", errors);
        }

        info!("Created DNS record for {}", domain);
        Ok(())
    }

    async fn get_tunnel_token(&self, tunnel_id: &str) -> Result<String> {
        let url = format!(
            "{}/accounts/{}/cfd_tunnel/{}/token",
            CF_API_BASE, self.account_id, tunnel_id
        );

        let resp: ApiResponse<String> = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            let errors: Vec<_> = resp.errors.iter().map(|e| &e.message).collect();
            anyhow::bail!("Failed to get tunnel token: {:?}", errors);
        }

        resp.result.context("No token in response")
    }
}

impl CloudflareTunnel {
    pub async fn start(config: CloudflareConfig) -> Result<Self> {
        let client = CloudflareClient::new(&config);

        info!("Checking for existing tunnel '{}'...", config.tunnel_name);
        let tunnels = client.list_tunnels().await?;

        let tunnel_id = if let Some(existing) = tunnels.iter().find(|t| t.name == config.tunnel_name)
        {
            info!("Found existing tunnel: {}", existing.id);
            existing.id.clone()
        } else {
            info!("Creating new tunnel '{}'...", config.tunnel_name);
            let (id, _creds) = client.create_tunnel(&config.tunnel_name).await?;
            info!("Created tunnel: {}", id);
            id
        };

        info!("Updating tunnel config for {}...", config.domain);
        client
            .update_tunnel_config(&tunnel_id, &config.domain, config.local_port)
            .await?;

        info!("Ensuring DNS record...");
        client.ensure_dns_record(&config.domain, &tunnel_id).await?;

        info!("Getting tunnel token...");
        let token = client.get_tunnel_token(&tunnel_id).await?;

        info!("Starting cloudflared...");
        let creds_dir = std::env::temp_dir().join(format!("foundry-tunnel-{}", std::process::id()));
        tokio::fs::create_dir_all(&creds_dir).await?;
        let token_file = creds_dir.join("token");
        tokio::fs::write(&token_file, &token).await?;

        let process = Command::new("cloudflared")
            .args(["tunnel", "--no-autoupdate", "run", "--token-file"])
            .arg(&token_file)
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .context("Failed to start cloudflared")?;

        info!("Tunnel running at https://{}", config.domain);

        Ok(Self {
            _process: process,
            tunnel_id,
            domain: config.domain,
        })
    }

    pub fn webhook_url(&self) -> String {
        format!("https://{}/webhook/github", self.domain)
    }
}
