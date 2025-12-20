use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
pub struct CloudflareClient {
    pub account_id: String,
    pub api_token: String,
    pub zone_id: String,
    pub tunnel_name: String,
    client: reqwest::Client,
}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    success: bool,
    result: Option<T>,
    errors: Vec<ApiError>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Debug, Deserialize)]
pub struct Tunnel {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TunnelConfig {
    pub ingress: Vec<IngressRule>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IngressRule {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hostname: Option<String>,
    pub service: String,
    #[serde(rename = "originRequest", skip_serializing_if = "Option::is_none")]
    pub origin_request: Option<OriginRequest>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OriginRequest {
    #[serde(rename = "httpHostHeader", skip_serializing_if = "Option::is_none")]
    pub http_host_header: Option<String>,
}

impl CloudflareClient {
    pub fn from_env() -> Result<Option<Self>> {
        let account_id = std::env::var("CF_ACCOUNT_ID").ok();
        let api_token = std::env::var("CF_API_TOKEN").ok();
        let zone_id = std::env::var("CF_ZONE_ID").ok();
        let tunnel_name = std::env::var("CF_TUNNEL_NAME").ok();

        match (account_id, api_token, zone_id, tunnel_name) {
            (Some(account_id), Some(api_token), Some(zone_id), Some(tunnel_name)) => {
                Ok(Some(Self {
                    account_id,
                    api_token,
                    zone_id,
                    tunnel_name,
                    client: reqwest::Client::new(),
                }))
            }
            _ => Ok(None),
        }
    }

    pub fn new(account_id: String, api_token: String, zone_id: String, tunnel_name: String) -> Self {
        Self {
            account_id,
            api_token,
            zone_id,
            tunnel_name,
            client: reqwest::Client::new(),
        }
    }

    pub async fn get_tunnel(&self) -> Result<Option<Tunnel>> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/cfd_tunnel?name={}",
            self.account_id, self.tunnel_name
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
            let msg = resp.errors.first().map(|e| e.message.clone()).unwrap_or_default();
            return Err(anyhow!("Failed to get tunnel: {}", msg));
        }

        Ok(resp.result.and_then(|tunnels| tunnels.into_iter().next()))
    }

    pub async fn get_tunnel_config(&self, tunnel_id: &str) -> Result<TunnelConfig> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/cfd_tunnel/{}/configurations",
            self.account_id, tunnel_id
        );

        let resp: ApiResponse<ConfigWrapper> = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            let msg = resp.errors.first().map(|e| e.message.clone()).unwrap_or_default();
            return Err(anyhow!("Failed to get tunnel config: {}", msg));
        }

        Ok(resp.result.map(|w| w.config).unwrap_or_else(|| TunnelConfig {
            ingress: vec![IngressRule {
                hostname: None,
                service: "http_status:404".to_string(),
                origin_request: None,
            }],
        }))
    }

    pub async fn update_tunnel_config(&self, tunnel_id: &str, config: &TunnelConfig) -> Result<()> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/cfd_tunnel/{}/configurations",
            self.account_id, tunnel_id
        );

        let body = serde_json::json!({ "config": config });

        let resp: ApiResponse<serde_json::Value> = self
            .client
            .put(&url)
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        if !resp.success {
            let msg = resp.errors.first().map(|e| e.message.clone()).unwrap_or_default();
            return Err(anyhow!("Failed to update tunnel config: {}", msg));
        }

        Ok(())
    }

    pub async fn ensure_dns_record(&self, hostname: &str, tunnel_id: &str) -> Result<()> {
        let cname_target = format!("{}.cfargotunnel.com", tunnel_id);
        
        let record_name = hostname.split('.').next().unwrap_or(hostname);

        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records?type=CNAME&name={}",
            self.zone_id, hostname
        );

        let resp: ApiResponse<Vec<DnsRecord>> = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?
            .json()
            .await?;

        if let Some(records) = resp.result {
            if let Some(record) = records.first() {
                if record.content != cname_target {
                    let update_url = format!(
                        "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
                        self.zone_id, record.id
                    );

                    let body = serde_json::json!({
                        "type": "CNAME",
                        "name": record_name,
                        "content": cname_target,
                        "proxied": true
                    });

                    self.client
                        .put(&update_url)
                        .bearer_auth(&self.api_token)
                        .json(&body)
                        .send()
                        .await?;
                }
                return Ok(());
            }
        }

        let create_url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records",
            self.zone_id
        );

        let body = serde_json::json!({
            "type": "CNAME",
            "name": record_name,
            "content": cname_target,
            "proxied": true
        });

        self.client
            .post(&create_url)
            .bearer_auth(&self.api_token)
            .json(&body)
            .send()
            .await?;

        Ok(())
    }

    pub async fn remove_route(&self, hostname: &str) -> Result<bool> {
        let tunnel = self
            .get_tunnel()
            .await?
            .ok_or_else(|| anyhow!("Tunnel '{}' not found", self.tunnel_name))?;

        let mut config = self.get_tunnel_config(&tunnel.id).await?;
        
        let original_len = config.ingress.len();
        config.ingress.retain(|rule| rule.hostname.as_deref() != Some(hostname));
        
        if config.ingress.len() == original_len {
            return Ok(false);
        }

        if !config.ingress.iter().any(|r| r.hostname.is_none()) {
            config.ingress.push(IngressRule {
                hostname: None,
                service: "http_status:404".to_string(),
                origin_request: None,
            });
        }

        self.update_tunnel_config(&tunnel.id, &config).await?;
        tracing::info!("Removed route for: {}", hostname);
        Ok(true)
    }

    pub async fn get_route(&self, hostname: &str) -> Result<Option<String>> {
        let tunnel = self
            .get_tunnel()
            .await?
            .ok_or_else(|| anyhow!("Tunnel '{}' not found", self.tunnel_name))?;

        let config = self.get_tunnel_config(&tunnel.id).await?;
        
        Ok(config.ingress.iter()
            .find(|rule| rule.hostname.as_deref() == Some(hostname))
            .map(|rule| rule.service.clone()))
    }

    pub async fn add_route(&self, hostname: &str, service: &str) -> Result<()> {
        let tunnel = self
            .get_tunnel()
            .await?
            .ok_or_else(|| anyhow!("Tunnel '{}' not found", self.tunnel_name))?;

        let mut config = self.get_tunnel_config(&tunnel.id).await?;
        
        tracing::debug!("Current tunnel config has {} ingress rules", config.ingress.len());

        let existing_idx = config.ingress.iter().position(|rule| {
            rule.hostname.as_deref() == Some(hostname)
        });

        if let Some(idx) = existing_idx {
            let old_service = &config.ingress[idx].service;
            if old_service == service {
                tracing::info!("Route already exists and matches: {} -> {}", hostname, service);
                return Ok(());
            }
            tracing::info!("Updating route: {} -> {} (was: {})", hostname, service, old_service);
            config.ingress[idx].service = service.to_string();
        } else {
            let catch_all_idx = config.ingress.iter().position(|rule| rule.hostname.is_none());
            
            let new_rule = IngressRule {
                hostname: Some(hostname.to_string()),
                service: service.to_string(),
                origin_request: None,
            };

            if let Some(idx) = catch_all_idx {
                config.ingress.insert(idx, new_rule);
            } else {
                config.ingress.push(new_rule);
                config.ingress.push(IngressRule {
                    hostname: None,
                    service: "http_status:404".to_string(),
                    origin_request: None,
                });
            }
            tracing::info!("Adding new route: {} -> {}", hostname, service);
        }

        tracing::debug!("Updated tunnel config has {} ingress rules", config.ingress.len());

        self.update_tunnel_config(&tunnel.id, &config).await?;

        self.ensure_dns_record(hostname, &tunnel.id).await?;

        tracing::info!("Route configured: {} -> {}", hostname, service);
        Ok(())
    }

    pub async fn remove_dns_record(&self, hostname: &str) -> Result<bool> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/zones/{}/dns_records?type=CNAME&name={}",
            self.zone_id, hostname
        );

        let resp: ApiResponse<Vec<DnsRecord>> = self
            .client
            .get(&url)
            .bearer_auth(&self.api_token)
            .send()
            .await?
            .json()
            .await?;

        if let Some(records) = resp.result {
            if let Some(record) = records.first() {
                let delete_url = format!(
                    "https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}",
                    self.zone_id, record.id
                );

                self.client
                    .delete(&delete_url)
                    .bearer_auth(&self.api_token)
                    .send()
                    .await?;

                tracing::info!("Deleted DNS record for: {}", hostname);
                return Ok(true);
            }
        }

        Ok(false)
    }

    pub async fn remove_domain(&self, hostname: &str) -> Result<()> {
        self.remove_route(hostname).await?;
        self.remove_dns_record(hostname).await?;
        tracing::info!("Removed domain completely: {}", hostname);
        Ok(())
    }

    pub async fn get_tunnel_token(&self, tunnel_id: &str) -> Result<String> {
        let url = format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/cfd_tunnel/{}/token",
            self.account_id, tunnel_id
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
            let msg = resp.errors.first().map(|e| e.message.clone()).unwrap_or_default();
            return Err(anyhow!("Failed to get tunnel token: {}", msg));
        }

        resp.result.ok_or_else(|| anyhow!("No token in response"))
    }
}

#[derive(Debug, Deserialize)]
struct ConfigWrapper {
    config: TunnelConfig,
}

#[derive(Debug, Deserialize)]
struct DnsRecord {
    id: String,
    content: String,
}
