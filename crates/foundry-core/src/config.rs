use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing;

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
    #[serde(default)]
    pub secrets: Option<SecretsConfig>,
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
    pub failure_policy: FailurePolicy,
    #[serde(default)]
    pub env: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub condition: Option<StageCondition>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    #[default]
    Strict,
    Allow,
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
    #[serde(default)]
    pub env_file: Option<String>,
}

/// Vault secrets configuration — declared per-project in foundry.toml.
///
/// Example:
/// ```toml
/// [secrets]
/// vault_path = "myapp/prod"
/// ```
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct SecretsConfig {
    /// KV v2 path relative to the `secret/` mount, e.g. `"myapp/prod"`.
    /// The agent will read `secret/data/{vault_path}` and inject the values
    /// as environment variables into the job.
    #[serde(default)]
    pub vault_path: Option<String>,

    /// Optional list of specific keys to pull from Vault. If empty, all keys
    /// at the path are injected.
    #[serde(default)]
    pub keys: Option<Vec<String>>,
}

impl SecretsConfig {
    pub fn is_enabled(&self) -> bool {
        self.vault_path.is_some()
    }
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

        let content = match std::fs::read_to_string(&config_path) {
            Ok(c) => c,
            Err(err) => {
                tracing::warn!("Failed to read config file {:?}: {}", config_path, err);
                return None;
            }
        };
        match toml::from_str(&content) {
            Ok(config) => {
                let config: FoundryConfig = config;
                if let Err(warnings) = config.validate() {
                    for warning in &warnings {
                        tracing::warn!("Config validation warning: {}", warning);
                    }
                }
                Some(config)
            }
            Err(err) => {
                tracing::warn!("Failed to parse config file {:?}: {}", config_path, err);
                None
            }
        }
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

    /// Validates the configuration and returns any errors found.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        // Build timeout must be positive
        if self.build.timeout == 0 {
            errors.push("build.timeout must be positive (> 0)".to_string());
        }

        // If stages is defined, it must not be empty
        // (only flag if stages were explicitly provided but empty — default is empty vec,
        // so we skip this check when no stages are present at all)

        // Validate each stage
        let stage_names: Vec<&str> = self.stages.iter().map(|s| s.name.as_str()).collect();
        for (i, stage) in self.stages.iter().enumerate() {
            if stage.name.trim().is_empty() {
                errors.push(format!("stages[{}]: stage name must be non-empty", i));
            }
            if stage.timeout == 0 {
                errors.push(format!("stages[{}] '{}': timeout must be positive (> 0)", i, stage.name));
            }
            // depends_on should reference valid stage names
            for dep in &stage.depends_on {
                if !stage_names.contains(&dep.as_str()) {
                    errors.push(format!(
                        "stages[{}] '{}': depends_on references unknown stage '{}'",
                        i, stage.name, dep
                    ));
                }
            }
        }

        // Deploy: if enabled, compose_file or a dockerfile should be present
        if self.deploy.is_enabled()
            && self.deploy.compose_file.is_none()
            && self.build.dockerfile.is_none()
        {
            errors.push(
                "deploy is enabled but neither deploy.compose_file nor build.dockerfile is set"
                    .to_string(),
            );
        }

        // Triggers: branch patterns should be non-empty
        for (i, branch) in self.triggers.branches.iter().enumerate() {
            if branch.trim().is_empty() {
                errors.push(format!("triggers.branches[{}]: branch pattern must be non-empty", i));
            }
        }
        if let Some(ref targets) = self.triggers.pr_target_branches {
            for (i, branch) in targets.iter().enumerate() {
                if branch.trim().is_empty() {
                    errors.push(format!(
                        "triggers.pr_target_branches[{}]: branch pattern must be non-empty",
                        i
                    ));
                }
            }
        }

        // Schedule: basic cron syntax check (5 or 6 space-separated fields)
        if let Some(ref schedule) = self.schedule {
            let field_count = schedule.cron.split_whitespace().count();
            if field_count != 5 && field_count != 6 {
                errors.push(format!(
                    "schedule.cron: expected 5 or 6 space-separated fields, found {}",
                    field_count
                ));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_stage(name: &str) -> StageConfig {
        StageConfig {
            name: name.to_string(),
            image: None,
            command: "echo hello".to_string(),
            timeout: 600,
            failure_policy: FailurePolicy::Strict,
            env: std::collections::HashMap::new(),
            depends_on: Vec::new(),
            condition: None,
        }
    }

    #[test]
    fn test_validate_valid_config() {
        let config = FoundryConfig {
            build: BuildConfig::default(),
            deploy: DeployConfig::default(),
            triggers: TriggersConfig::default(),
            schedule: None,
            stages: vec![valid_stage("test"), valid_stage("build")],
            env: std::collections::HashMap::new(),
            secrets: None,
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_stage_name() {
        let config = FoundryConfig {
            stages: vec![valid_stage("")],
            ..Default::default()
        };
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("stage name must be non-empty")));
    }

    #[test]
    fn test_validate_zero_timeout() {
        let mut stage = valid_stage("test");
        stage.timeout = 0;
        let config = FoundryConfig {
            stages: vec![stage],
            ..Default::default()
        };
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("timeout must be positive")));
    }

    #[test]
    fn test_validate_zero_build_timeout() {
        let config = FoundryConfig {
            build: BuildConfig {
                timeout: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("build.timeout must be positive")));
    }

    #[test]
    fn test_validate_unknown_depends_on() {
        let mut stage = valid_stage("deploy");
        stage.depends_on = vec!["nonexistent".to_string()];
        let config = FoundryConfig {
            stages: vec![valid_stage("build"), stage],
            ..Default::default()
        };
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("unknown stage 'nonexistent'")));
    }

    #[test]
    fn test_validate_empty_branch_pattern() {
        let config = FoundryConfig {
            triggers: TriggersConfig {
                branches: vec!["main".to_string(), "  ".to_string()],
                ..Default::default()
            },
            ..Default::default()
        };
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("branch pattern must be non-empty")));
    }

    #[test]
    fn test_validate_bad_cron() {
        let config = FoundryConfig {
            schedule: Some(ScheduleConfig {
                cron: "0 0 *".to_string(),
                branch: None,
                enabled: true,
                timezone: None,
            }),
            ..Default::default()
        };
        let errors = config.validate().unwrap_err();
        assert!(errors.iter().any(|e| e.contains("schedule.cron")));
    }

    #[test]
    fn test_validate_valid_cron() {
        let config = FoundryConfig {
            schedule: Some(ScheduleConfig {
                cron: "0 0 * * *".to_string(),
                branch: None,
                enabled: true,
                timezone: None,
            }),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }
}
