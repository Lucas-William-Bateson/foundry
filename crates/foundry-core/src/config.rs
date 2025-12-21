use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FoundryConfig {
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub deploy: DeployConfig,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BuildConfig {
    #[serde(default = "default_image")]
    pub image: String,
    #[serde(default)]
    pub dockerfile: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub args: Vec<String>,
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            image: default_image(),
            dockerfile: None,
            context: None,
            command: None,
            args: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DeployConfig {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub domains: Option<Vec<String>>,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub compose_file: Option<String>,
    #[serde(default)]
    pub healthcheck: Option<String>,
}

impl DeployConfig {
    pub fn is_enabled(&self) -> bool {
        self.name.is_some() || self.compose_file.is_some()
    }

    pub fn all_domains(&self) -> Vec<&str> {
        let mut result = Vec::new();
        if let Some(d) = &self.domain {
            result.push(d.as_str());
        }
        if let Some(ds) = &self.domains {
            for d in ds {
                result.push(d.as_str());
            }
        }
        result
    }
}

fn default_image() -> String {
    "ubuntu:latest".to_string()
}

impl FoundryConfig {
    pub fn load(repo_dir: &Path) -> Option<Self> {
        let config_path = repo_dir.join("foundry.toml");
        if !config_path.exists() {
            return None;
        }

        let content = std::fs::read_to_string(&config_path).ok()?;
        toml::from_str(&content).ok()
    }

    pub fn effective_command(&self, default: &str) -> String {
        if let Some(cmd) = &self.build.command {
            if self.build.args.is_empty() {
                cmd.clone()
            } else {
                format!("{} {}", cmd, self.build.args.join(" "))
            }
        } else {
            default.to_string()
        }
    }
}
