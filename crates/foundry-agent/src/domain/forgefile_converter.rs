//! Converts a parsed `forgefile::Forgefile` AST into execution structures
//! the agent already understands (`StageConfig`, `DeployConfig`, etc.).

use std::collections::HashMap;

use forgefile::Forgefile;
use forgefile::ast::{
    Condition, DeployDef, Expr, ExprPart, NeedsRef, PipelineItem, RunnerDef, RunnerExpr,
    RunnerRef, SecretsDef, SecretsSource, StageDef, Trigger, TriggerBlock,
};

use foundry_core::config::{
    DeployConfig, FailurePolicy, StageCondition, StageConfig, TriggersConfig,
};
use foundry_core::types::RunnerRequirements;

/// Which backend a secrets block uses.
#[derive(Debug, Clone, PartialEq)]
pub enum SecretsBackend {
    /// HashiCorp Vault.
    Vault,
    /// Local encrypted secrets store.
    Store,
}

/// Secrets configuration extracted from the Forgefile.
#[derive(Debug, Clone)]
pub struct SecretsConfig {
    pub backend: SecretsBackend,
    pub path: String,
    pub keys: Vec<SecretKeyMapping>,
}

/// A secret key with an optional alias for env var injection.
#[derive(Debug, Clone)]
pub struct SecretKeyMapping {
    pub name: String,
    pub alias: Option<String>,
}

/// The full execution plan produced from a Forgefile.
#[derive(Debug, Clone)]
pub struct ExecutionPlan {
    pub stages: Vec<StageConfig>,
    pub secrets: Vec<SecretsConfig>,
    pub deploy: Option<DeployConfig>,
    pub runner_requirements: HashMap<String, RunnerRequirements>,
    pub env: HashMap<String, String>,
    pub triggers: TriggersConfig,
}

/// Convert a parsed and validated Forgefile into an ExecutionPlan.
///
/// `event_ref` is the git ref (e.g. `refs/pull/42/head` or `refs/heads/main`)
/// used to filter trigger blocks by the current event.
pub fn convert(forgefile: &Forgefile, event_ref: &str) -> ExecutionPlan {
    let runners = build_runner_map(&forgefile.runners);
    let secrets = convert_secrets(&forgefile.secrets);
    let triggers = convert_triggers(&forgefile.triggers);

    let is_pr = event_ref.starts_with("refs/pull/");

    // Collect stages and deploy from matching trigger blocks
    let mut stages = Vec::new();
    let mut deploy: Option<DeployConfig> = None;
    let mut runner_requirements: HashMap<String, RunnerRequirements> = HashMap::new();
    let global_env: HashMap<String, String> = HashMap::new();

    for block in &forgefile.triggers {
        if !trigger_block_matches(block, event_ref, is_pr) {
            continue;
        }

        for item in &block.items {
            match item {
                PipelineItem::Stage(stage_def) => {
                    let stage_cfg = convert_stage(stage_def, &global_env);
                    if let Some(ref runner_ref) = stage_def.runner {
                        let reqs = convert_runner_ref(runner_ref, &runners);
                        runner_requirements.insert(stage_def.name.clone(), reqs);
                    }
                    if let Some(ref deploy_def) = stage_def.deploy {
                        deploy = Some(convert_deploy(deploy_def));
                    }
                    stages.push(stage_cfg);
                }
                PipelineItem::Matrix(matrix_def) => {
                    // Expand matrix into individual stages
                    let combos = expand_matrix_variables(&matrix_def.variables);
                    for combo in &combos {
                        let suffix = combo
                            .iter()
                            .map(|(_, v)| v.as_str())
                            .collect::<Vec<_>>()
                            .join("-");
                        let expanded_name = format!("{}({})", matrix_def.name, suffix);

                        let mut expanded_stage = matrix_def.stage.clone();
                        expanded_stage.name = expanded_name.clone();

                        // Inject matrix variables into env
                        for (key, value) in combo {
                            expanded_stage.env.push(forgefile::ast::EnvVar {
                                key: key.clone(),
                                value: Expr::Literal(value.clone()),
                            });
                        }

                        // Use matrix-level runner if stage doesn't specify one
                        if expanded_stage.runner.is_none() {
                            expanded_stage.runner = matrix_def.runner.clone();
                        }

                        let stage_cfg = convert_stage(&expanded_stage, &global_env);
                        if let Some(ref runner_ref) = expanded_stage.runner {
                            let reqs = convert_runner_ref(runner_ref, &runners);
                            runner_requirements.insert(expanded_name, reqs);
                        }
                        stages.push(stage_cfg);
                    }
                }
            }
        }
    }

    // Topological sort by dependencies
    stages = topological_sort(stages);

    ExecutionPlan {
        stages,
        secrets,
        deploy,
        runner_requirements,
        env: global_env,
        triggers,
    }
}

fn build_runner_map(runners: &[RunnerDef]) -> HashMap<String, RunnerDef> {
    runners
        .iter()
        .map(|r| (r.name.clone(), r.clone()))
        .collect()
}

fn convert_secrets(secrets_defs: &[SecretsDef]) -> Vec<SecretsConfig> {
    secrets_defs
        .iter()
        .map(|s| {
            let (backend, path_expr) = match &s.source {
                SecretsSource::Vault(e) => (SecretsBackend::Vault, e),
                SecretsSource::Store(e) => (SecretsBackend::Store, e),
            };
            SecretsConfig {
                backend,
                path: expr_to_string(path_expr),
                keys: s
                    .keys
                    .iter()
                    .map(|k| SecretKeyMapping {
                        name: k.name.clone(),
                        alias: k.alias.clone(),
                    })
                    .collect(),
            }
        })
        .collect()
}

fn convert_triggers(trigger_blocks: &[TriggerBlock]) -> TriggersConfig {
    let mut branches: Vec<String> = Vec::new();
    let mut pull_requests = false;
    let mut pr_target_branches: Vec<String> = Vec::new();

    for block in trigger_blocks {
        for trigger in &block.triggers {
            match trigger {
                Trigger::Push(patterns) => {
                    branches.extend(patterns.iter().cloned());
                }
                Trigger::Pr(patterns) => {
                    pull_requests = true;
                    pr_target_branches.extend(patterns.iter().cloned());
                }
                Trigger::Schedule { .. } | Trigger::Failure => {}
            }
        }
    }

    // Deduplicate
    branches.sort();
    branches.dedup();
    pr_target_branches.sort();
    pr_target_branches.dedup();

    TriggersConfig {
        branches: if branches.is_empty() {
            vec!["main".to_string()]
        } else {
            branches
        },
        pull_requests,
        pr_target_branches: if pr_target_branches.is_empty() {
            None
        } else {
            Some(pr_target_branches)
        },
    }
}

fn trigger_block_matches(block: &TriggerBlock, event_ref: &str, is_pr: bool) -> bool {
    for trigger in &block.triggers {
        match trigger {
            Trigger::Push(patterns) => {
                if !is_pr {
                    let branch = event_ref
                        .strip_prefix("refs/heads/")
                        .unwrap_or(event_ref);
                    if patterns.iter().any(|p| branch_matches(branch, p)) {
                        return true;
                    }
                }
            }
            Trigger::Pr(patterns) => {
                if is_pr {
                    // PR target branch matching requires data not available in git ref.
                    // Accept all PR triggers for now; server-side filtering handles target branches.
                    let _ = patterns;
                    return true;
                }
            }
            Trigger::Schedule { .. } => {
                // Schedule triggers are handled server-side via cron.
                // Return false here so schedule-only blocks don't run on push/PR events.
            }
            Trigger::Failure => {
                // Failure triggers are evaluated after stage execution
            }
        }
    }
    false
}

fn branch_matches(branch: &str, pattern: &str) -> bool {
    if pattern.contains('*') {
        let prefix = pattern.trim_end_matches('*');
        branch.starts_with(prefix)
    } else {
        branch == pattern
    }
}

fn convert_stage(stage_def: &StageDef, _global_env: &HashMap<String, String>) -> StageConfig {
    // Combine all run commands into a single shell script
    let command = stage_def
        .commands
        .iter()
        .map(|cmd| expr_to_string(cmd))
        .collect::<Vec<_>>()
        .join(" && ");

    let mut env: HashMap<String, String> = HashMap::new();
    for ev in &stage_def.env {
        env.insert(ev.key.clone(), expr_to_string(&ev.value));
    }

    let depends_on: Vec<String> = stage_def
        .needs
        .iter()
        .map(|n| match n {
            NeedsRef::Stage(name) => name.clone(),
            NeedsRef::MatrixAll(name) => name.clone(),
        })
        .collect();

    let condition = stage_def.condition.as_ref().map(|c| match c {
        Condition::Always => StageCondition::Always,
        Condition::OnSuccess => StageCondition::OnSuccess,
        Condition::OnFailure => StageCondition::OnFailure,
        Condition::OnPush => StageCondition::OnPush,
        Condition::OnPr => StageCondition::OnPr,
        Condition::Expr(_) => StageCondition::Always, // future extension
    });

    let failure_policy = if stage_def.allow_failure {
        FailurePolicy::Allow
    } else {
        FailurePolicy::Strict
    };

    let timeout = stage_def
        .timeout
        .as_ref()
        .map(|d| d.seconds)
        .unwrap_or(600);

    // Extract image from runner ref if it's a named runner
    let image = None; // Image resolution happens at runtime via runner matching

    StageConfig {
        name: stage_def.name.clone(),
        image,
        command,
        timeout,
        failure_policy,
        env,
        depends_on,
        condition,
    }
}

fn convert_deploy(deploy_def: &DeployDef) -> DeployConfig {
    DeployConfig {
        name: Some(deploy_def.name.clone()),
        domain: deploy_def.domain.clone(),
        domains: None,
        port: deploy_def.port,
        compose_file: deploy_def.compose_file.clone(),
        healthcheck: None,
        volumes: None,
        env_file: None,
    }
}

fn convert_runner_ref(
    runner_ref: &RunnerRef,
    runners: &HashMap<String, RunnerDef>,
) -> RunnerRequirements {
    match runner_ref {
        RunnerRef::Named(name) => {
            if let Some(def) = runners.get(name) {
                RunnerRequirements {
                    runner_name: Some(name.clone()),
                    required_tags: def.tags.clone(),
                    min_cpu: def.cpu,
                    min_memory_mb: def.mem.as_ref().and_then(|m| parse_memory_mb(m)),
                    min_gpu: def.gpu,
                    arch: def.arch.clone(),
                }
            } else {
                RunnerRequirements {
                    runner_name: Some(name.clone()),
                    ..Default::default()
                }
            }
        }
        RunnerRef::Expr(expr) => convert_runner_expr(expr),
    }
}

fn convert_runner_expr(expr: &RunnerExpr) -> RunnerRequirements {
    let mut reqs = RunnerRequirements::default();
    collect_runner_expr_reqs(expr, &mut reqs);
    reqs
}

fn collect_runner_expr_reqs(expr: &RunnerExpr, reqs: &mut RunnerRequirements) {
    match expr {
        RunnerExpr::TagsHas(tag) => {
            reqs.required_tags.push(tag.clone());
        }
        RunnerExpr::CpuGte(n) => {
            reqs.min_cpu = Some(*n);
        }
        RunnerExpr::MemGte(m) => {
            reqs.min_memory_mb = parse_memory_mb(m);
        }
        RunnerExpr::GpuGte(n) => {
            reqs.min_gpu = Some(*n);
        }
        RunnerExpr::ArchEq(a) => {
            reqs.arch = Some(a.clone());
        }
        RunnerExpr::And(left, right) => {
            collect_runner_expr_reqs(left, reqs);
            collect_runner_expr_reqs(right, reqs);
        }
        RunnerExpr::Or(left, _right) => {
            // For OR, we take the left branch as a best-effort match
            collect_runner_expr_reqs(left, reqs);
        }
    }
}

fn parse_memory_mb(mem_str: &str) -> Option<u32> {
    let s = mem_str.trim().to_lowercase();
    if let Some(n) = s.strip_suffix("gb") {
        n.trim().parse::<u32>().ok().map(|v| v * 1024)
    } else if let Some(n) = s.strip_suffix("g") {
        n.trim().parse::<u32>().ok().map(|v| v * 1024)
    } else if let Some(n) = s.strip_suffix("mb") {
        n.trim().parse::<u32>().ok()
    } else if let Some(n) = s.strip_suffix("m") {
        n.trim().parse::<u32>().ok()
    } else {
        s.parse::<u32>().ok()
    }
}

fn expr_to_string(expr: &Expr) -> String {
    match expr {
        Expr::Literal(s) => s.clone(),
        Expr::Interpolated(parts) => parts
            .iter()
            .map(|p| match p {
                ExprPart::Text(t) => t.clone(),
                ExprPart::Variable(v) => format!("${{{}}}", v),
                ExprPart::StageOutput(stage, key) => format!("${{{}.{}}}", stage, key),
            })
            .collect(),
    }
}

/// Expand matrix variables into all combinations.
fn expand_matrix_variables(
    variables: &[forgefile::ast::MatrixVariable],
) -> Vec<Vec<(String, String)>> {
    if variables.is_empty() {
        return vec![vec![]];
    }

    let first = &variables[0];
    let rest = expand_matrix_variables(&variables[1..]);

    let mut combos = Vec::new();
    for value in &first.values {
        for suffix in &rest {
            let mut combo = vec![(first.name.clone(), value.clone())];
            combo.extend(suffix.iter().cloned());
            combos.push(combo);
        }
    }
    combos
}

/// Topological sort of stages based on depends_on.
fn topological_sort(stages: Vec<StageConfig>) -> Vec<StageConfig> {
    use std::collections::{HashSet, VecDeque};

    let name_to_idx: HashMap<String, usize> = stages
        .iter()
        .enumerate()
        .map(|(i, s)| (s.name.clone(), i))
        .collect();

    let n = stages.len();
    let mut in_degree = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![vec![]; n];

    for (i, stage) in stages.iter().enumerate() {
        for dep in &stage.depends_on {
            if let Some(&dep_idx) = name_to_idx.get(dep) {
                adj[dep_idx].push(i);
                in_degree[i] += 1;
            }
        }
    }

    let mut queue: VecDeque<usize> = VecDeque::new();
    for (i, &deg) in in_degree.iter().enumerate() {
        if deg == 0 {
            queue.push_back(i);
        }
    }

    let mut sorted = Vec::with_capacity(n);
    while let Some(idx) = queue.pop_front() {
        sorted.push(idx);
        for &next in &adj[idx] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                queue.push_back(next);
            }
        }
    }

    // If cycle exists (shouldn't after validation), append remaining stages
    if sorted.len() < n {
        let in_sorted: HashSet<usize> = sorted.iter().copied().collect();
        for i in 0..n {
            if !in_sorted.contains(&i) {
                sorted.push(i);
            }
        }
    }

    sorted.into_iter().map(|i| stages[i].clone()).collect()
}

/// Convert Forgefile secret key mappings to the format needed for secret injection.
/// Returns a map of env_var_name -> secret_key_name.
pub fn build_secret_alias_map(secrets: &[SecretsConfig]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for secret_cfg in secrets {
        for key in &secret_cfg.keys {
            let env_name = key.alias.as_ref().unwrap_or(&key.name).clone();
            map.insert(env_name, key.name.clone());
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use forgefile::ast::*;

    fn make_stage(name: &str, cmd: &str) -> StageDef {
        StageDef {
            name: name.to_string(),
            runner: None,
            needs: vec![],
            commands: vec![Expr::Literal(cmd.to_string())],
            env: vec![],
            services: vec![],
            artifacts: vec![],
            outputs: vec![],
            deploy: None,
            condition: None,
            allow_failure: false,
            retry: None,
            timeout: None,
        }
    }

    #[test]
    fn test_convert_simple_pipeline() {
        let ff = Forgefile {
            runners: vec![RunnerDef {
                name: "default".into(),
                tags: vec![],
                cpu: None,
                mem: None,
                gpu: None,
                arch: None,
                image: Some("rust:1.87-slim".into()),
            }],
            secrets: vec![],
            services: vec![],
            triggers: vec![TriggerBlock {
                triggers: vec![Trigger::Push(vec!["main".into()])],
                items: vec![
                    PipelineItem::Stage(make_stage("test", "cargo test")),
                    PipelineItem::Stage({
                        let mut s = make_stage("build", "cargo build --release");
                        s.needs = vec![NeedsRef::Stage("test".into())];
                        s
                    }),
                ],
            }],
        };

        let plan = convert(&ff, "refs/heads/main");
        assert_eq!(plan.stages.len(), 2);
        assert_eq!(plan.stages[0].name, "test");
        assert_eq!(plan.stages[1].name, "build");
    }

    #[test]
    fn test_convert_secrets_with_alias() {
        let ff = Forgefile {
            runners: vec![],
            secrets: vec![SecretsDef {
                source: SecretsSource::Vault(Expr::Literal("myapp/prod".into())),
                keys: vec![
                    SecretKey {
                        name: "DB_PASSWORD".into(),
                        alias: None,
                    },
                    SecretKey {
                        name: "API_KEY".into(),
                        alias: Some("MY_API_KEY".into()),
                    },
                ],
            }],
            services: vec![],
            triggers: vec![],
        };

        let plan = convert(&ff, "refs/heads/main");
        assert_eq!(plan.secrets.len(), 1);
        assert_eq!(plan.secrets[0].backend, SecretsBackend::Vault);
        assert_eq!(plan.secrets[0].path, "myapp/prod");

        let alias_map = build_secret_alias_map(&plan.secrets);
        assert_eq!(alias_map.get("DB_PASSWORD"), Some(&"DB_PASSWORD".to_string()));
        assert_eq!(alias_map.get("MY_API_KEY"), Some(&"API_KEY".to_string()));
    }

    #[test]
    fn test_convert_store_secrets() {
        let ff = Forgefile {
            runners: vec![],
            secrets: vec![SecretsDef {
                source: SecretsSource::Store(Expr::Literal("myapp/staging".into())),
                keys: vec![SecretKey {
                    name: "TOKEN".into(),
                    alias: None,
                }],
            }],
            services: vec![],
            triggers: vec![],
        };

        let plan = convert(&ff, "refs/heads/main");
        assert_eq!(plan.secrets.len(), 1);
        assert_eq!(plan.secrets[0].backend, SecretsBackend::Store);
        assert_eq!(plan.secrets[0].path, "myapp/staging");
    }

    #[test]
    fn test_topological_sort() {
        let stages = vec![
            StageConfig {
                name: "deploy".into(),
                image: None,
                command: "deploy.sh".into(),
                timeout: 600,
                failure_policy: FailurePolicy::Strict,
                env: HashMap::new(),
                depends_on: vec!["build".into()],
                condition: None,
            },
            StageConfig {
                name: "test".into(),
                image: None,
                command: "cargo test".into(),
                timeout: 600,
                failure_policy: FailurePolicy::Strict,
                env: HashMap::new(),
                depends_on: vec![],
                condition: None,
            },
            StageConfig {
                name: "build".into(),
                image: None,
                command: "cargo build".into(),
                timeout: 600,
                failure_policy: FailurePolicy::Strict,
                env: HashMap::new(),
                depends_on: vec!["test".into()],
                condition: None,
            },
        ];

        let sorted = topological_sort(stages);
        let names: Vec<&str> = sorted.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["test", "build", "deploy"]);
    }

    #[test]
    fn test_trigger_matching_push() {
        let block = TriggerBlock {
            triggers: vec![Trigger::Push(vec!["main".into(), "release/*".into()])],
            items: vec![],
        };
        assert!(trigger_block_matches(&block, "refs/heads/main", false));
        assert!(trigger_block_matches(&block, "refs/heads/release/v1", false));
        assert!(!trigger_block_matches(&block, "refs/heads/develop", false));
    }

    #[test]
    fn test_trigger_matching_pr() {
        let block = TriggerBlock {
            triggers: vec![Trigger::Pr(vec!["main".into()])],
            items: vec![],
        };
        assert!(trigger_block_matches(&block, "refs/pull/42/head", true));
        assert!(!trigger_block_matches(&block, "refs/heads/main", false));
    }

    #[test]
    fn test_parse_memory_mb() {
        assert_eq!(parse_memory_mb("4GB"), Some(4096));
        assert_eq!(parse_memory_mb("512MB"), Some(512));
        assert_eq!(parse_memory_mb("1024"), Some(1024));
        assert_eq!(parse_memory_mb("8G"), Some(8192));
        assert_eq!(parse_memory_mb("4g"), Some(4096));
        assert_eq!(parse_memory_mb("256M"), Some(256));
        assert_eq!(parse_memory_mb("128m"), Some(128));
    }
}
