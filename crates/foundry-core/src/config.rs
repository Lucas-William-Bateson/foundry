use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct FoundryConfig {
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub deploy: DeployConfig,
    #[serde(default)]
    pub triggers: TriggersConfig,
    #[serde(default)]
    pub schedule: Option<ScheduleConfig>,
    #[serde(default)]
    pub stages: Vec<StageConfig>,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StageConfig {
    pub name: String,
    #[serde(default)]
    pub image: Option<String>,
    pub command: String,
    #[serde(default = "default_stage_timeout")]
    pub timeout: u64,
    #[serde(default)]
    pub allow_failure: bool,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub condition: Option<StageCondition>,
}

fn default_stage_timeout() -> u64 {
    600
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StageCondition {
    Always,
    OnSuccess,
    OnFailure,
    OnPr,
    OnPush,
}

impl Default for StageCondition {
    fn default() -> Self {
        StageCondition::OnSuccess
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScheduleConfig {
    pub cron: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub timezone: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
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
    #[serde(default = "default_timeout")]
    pub timeout: u64,
}

fn default_timeout() -> u64 {
    1800
}

impl Default for BuildConfig {
    fn default() -> Self {
        Self {
            image: default_image(),
            dockerfile: None,
            context: None,
            command: None,
            args: Vec::new(),
            timeout: default_timeout(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TriggersConfig {
    #[serde(default = "default_branches")]
    pub branches: Vec<String>,
    #[serde(default = "default_true")]
    pub pull_requests: bool,
    #[serde(default)]
    pub pr_target_branches: Option<Vec<String>>,
}

fn default_branches() -> Vec<String> {
    vec!["main".to_string(), "master".to_string()]
}

fn default_true() -> bool {
    true
}

impl Default for TriggersConfig {
    fn default() -> Self {
        Self {
            branches: default_branches(),
            pull_requests: default_true(),
            pr_target_branches: None,
        }
    }
}

impl TriggersConfig {
    pub fn should_build_branch(&self, branch: &str) -> bool {
        self.branches.iter().any(|b| b == branch)
    }

    pub fn should_build_pr(&self, target_branch: &str) -> bool {
        if !self.pull_requests {
            return false;
        }
        if let Some(ref targets) = self.pr_target_branches {
            targets.iter().any(|b| b == target_branch)
        } else {
            true
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
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
    #[serde(default)]
    pub volumes: Option<Vec<String>>,
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

    pub fn has_stages(&self) -> bool {
        !self.stages.is_empty()
    }

    pub fn has_dockerfile(&self) -> bool {
        self.build.dockerfile.is_some()
    }

    pub fn stages_for_trigger(&self, is_pr: bool, previous_failed: bool) -> Vec<&StageConfig> {
        self.stages
            .iter()
            .filter(|s| {
                match &s.condition {
                    Some(StageCondition::Always) => true,
                    Some(StageCondition::OnSuccess) => !previous_failed,
                    Some(StageCondition::OnFailure) => previous_failed,
                    Some(StageCondition::OnPr) => is_pr,
                    Some(StageCondition::OnPush) => !is_pr,
                    None => !previous_failed,
                }
            })
            .collect()
    }
}
