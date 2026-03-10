use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::error::ForgeError;

/// Validate a parsed Forgefile for semantic correctness.
pub fn validate(forgefile: &Forgefile) -> Result<(), Vec<ForgeError>> {
    let mut errors = Vec::new();

    let runner_names: HashSet<&str> = forgefile.runners.iter().map(|r| r.name.as_str()).collect();
    let service_names: HashSet<&str> =
        forgefile.services.iter().map(|s| s.name.as_str()).collect();

    // Rule 8: vault paths must not be empty
    for secret in &forgefile.secrets {
        validate_vault_path(&secret.vault_path, &mut errors);
    }

    // Rule 1: stage names must be globally unique across all trigger blocks
    validate_unique_stage_names(forgefile, &mut errors);

    for trigger_block in &forgefile.triggers {
        validate_trigger_block(trigger_block, &runner_names, &service_names, &mut errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Check that no two stages share the same name across all trigger blocks.
fn validate_unique_stage_names(forgefile: &Forgefile, errors: &mut Vec<ForgeError>) {
    let mut seen: HashSet<&str> = HashSet::new();
    for trigger_block in &forgefile.triggers {
        for item in &trigger_block.items {
            let name = match item {
                PipelineItem::Stage(s) => &s.name,
                PipelineItem::Matrix(m) => &m.name,
            };
            if !seen.insert(name.as_str()) {
                errors.push(ForgeError::ValidationError(format!(
                    "duplicate stage name: '{name}'"
                )));
            }
        }
    }
}

fn validate_trigger_block(
    block: &TriggerBlock,
    runner_names: &HashSet<&str>,
    service_names: &HashSet<&str>,
    errors: &mut Vec<ForgeError>,
) {
    // Collect all stage/matrix names defined in this trigger block
    let mut item_names: HashSet<&str> = HashSet::new();
    for item in &block.items {
        match item {
            PipelineItem::Stage(s) => {
                item_names.insert(&s.name);
            }
            PipelineItem::Matrix(m) => {
                item_names.insert(&m.name);
            }
        }
    }

    for item in &block.items {
        match item {
            PipelineItem::Stage(stage) => {
                validate_stage(stage, &item_names, runner_names, service_names, errors);
            }
            PipelineItem::Matrix(matrix) => {
                validate_matrix(matrix, &item_names, runner_names, service_names, errors);
            }
        }
    }

    // Rule 4: no circular dependencies
    detect_cycles(block, errors);
}

fn validate_stage(
    stage: &StageDef,
    item_names: &HashSet<&str>,
    runner_names: &HashSet<&str>,
    service_names: &HashSet<&str>,
    errors: &mut Vec<ForgeError>,
) {
    // Rule 6: stages must have at least one run command OR a deploy block
    if stage.commands.is_empty() && stage.deploy.is_none() {
        errors.push(ForgeError::ValidationError(format!(
            "stage '{}' has no run commands",
            stage.name
        )));
    }

    // Rule 2: needs references must exist
    for need in &stage.needs {
        let ref_name = match need {
            NeedsRef::Stage(n) => n,
            NeedsRef::MatrixAll(n) => n,
        };
        if !item_names.contains(ref_name.as_str()) {
            errors.push(ForgeError::ValidationError(format!(
                "stage '{}' needs '{}' which does not exist",
                stage.name, ref_name
            )));
        }
    }

    // Rule 3: runner references must exist
    if let Some(RunnerRef::Named(name)) = &stage.runner {
        if !runner_names.contains(name.as_str()) {
            errors.push(ForgeError::ValidationError(format!(
                "stage '{}' references undefined runner '{name}'",
                stage.name
            )));
        }
    }

    // Rule 5: service references must exist
    for svc in &stage.services {
        if !service_names.contains(svc.as_str()) {
            errors.push(ForgeError::ValidationError(format!(
                "stage '{}' references undefined service '{svc}'",
                stage.name
            )));
        }
    }

    // Rule 7: deploy block must have a name
    if let Some(deploy) = &stage.deploy {
        if deploy.name.is_empty() {
            errors.push(ForgeError::ValidationError(format!(
                "stage '{}' has a deploy block with an empty name",
                stage.name
            )));
        }
    }
}

fn validate_matrix(
    matrix: &MatrixDef,
    item_names: &HashSet<&str>,
    runner_names: &HashSet<&str>,
    service_names: &HashSet<&str>,
    errors: &mut Vec<ForgeError>,
) {
    // Rule 9: matrix variables must have at least one value
    for var in &matrix.variables {
        if var.values.is_empty() {
            errors.push(ForgeError::ValidationError(format!(
                "matrix '{}' variable '{}' has no values",
                matrix.name, var.name
            )));
        }
    }

    // Rule 3: runner on the matrix itself
    if let Some(RunnerRef::Named(name)) = &matrix.runner {
        if !runner_names.contains(name.as_str()) {
            errors.push(ForgeError::ValidationError(format!(
                "matrix '{}' references undefined runner '{name}'",
                matrix.name
            )));
        }
    }

    // Validate the inner stage (but skip the run-command check since the matrix
    // stage body is the template)
    validate_stage(&matrix.stage, item_names, runner_names, service_names, errors);
}

fn validate_vault_path(expr: &Expr, errors: &mut Vec<ForgeError>) {
    let is_empty = match expr {
        Expr::Literal(s) => s.trim().is_empty(),
        Expr::Interpolated(parts) => parts.is_empty(),
    };
    if is_empty {
        errors.push(ForgeError::ValidationError(
            "vault path must not be empty".to_string(),
        ));
    }
}

/// DFS-based cycle detection on the needs DAG within a trigger block.
fn detect_cycles(block: &TriggerBlock, errors: &mut Vec<ForgeError>) {
    // Build adjacency list: name -> list of names it depends on
    let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();

    for item in &block.items {
        let (name, needs) = match item {
            PipelineItem::Stage(s) => (s.name.as_str(), &s.needs),
            PipelineItem::Matrix(m) => (m.name.as_str(), &m.stage.needs),
        };
        graph.entry(name).or_default();
        for need in needs {
            let dep = match need {
                NeedsRef::Stage(n) => n.as_str(),
                NeedsRef::MatrixAll(n) => n.as_str(),
            };
            graph.entry(name).or_default().push(dep);
        }
    }

    // Standard 3-colour DFS
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut colors: HashMap<&str, Color> = graph.keys().map(|&k| (k, Color::White)).collect();

    fn dfs<'a>(
        node: &'a str,
        graph: &HashMap<&'a str, Vec<&'a str>>,
        colors: &mut HashMap<&'a str, Color>,
        path: &mut Vec<&'a str>,
        errors: &mut Vec<ForgeError>,
    ) {
        colors.insert(node, Color::Gray);
        path.push(node);

        if let Some(neighbours) = graph.get(node) {
            for &dep in neighbours {
                match colors.get(dep) {
                    Some(Color::Gray) => {
                        // Found a cycle — extract the cycle portion of the path
                        let cycle_start = path.iter().position(|&n| n == dep).unwrap();
                        let cycle: Vec<&str> = path[cycle_start..].to_vec();
                        errors.push(ForgeError::ValidationError(format!(
                            "circular dependency detected: {}",
                            cycle.join(" -> ")
                        )));
                    }
                    Some(Color::White) | None => {
                        dfs(dep, graph, colors, path, errors);
                    }
                    Some(Color::Black) => {}
                }
            }
        }

        path.pop();
        colors.insert(node, Color::Black);
    }

    let nodes: Vec<&str> = graph.keys().copied().collect();
    for node in nodes {
        if colors.get(node) == Some(&Color::White) {
            let mut path = Vec::new();
            dfs(node, &graph, &mut colors, &mut path, errors);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_stage(name: &str) -> StageDef {
        StageDef {
            name: name.to_string(),
            runner: None,
            needs: vec![],
            commands: vec![Expr::Literal("echo hello".into())],
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

    fn minimal_forgefile(items: Vec<PipelineItem>) -> Forgefile {
        Forgefile {
            runners: vec![],
            secrets: vec![],
            services: vec![],
            triggers: vec![TriggerBlock {
                triggers: vec![Trigger::Push(vec!["main".into()])],
                items,
            }],
        }
    }

    #[test]
    fn valid_forgefile_passes() {
        let ff = minimal_forgefile(vec![PipelineItem::Stage(empty_stage("build"))]);
        assert!(validate(&ff).is_ok());
    }

    #[test]
    fn duplicate_stage_names_error() {
        let ff = minimal_forgefile(vec![
            PipelineItem::Stage(empty_stage("build")),
            PipelineItem::Stage(empty_stage("build")),
        ]);
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("duplicate stage name")));
    }

    #[test]
    fn needs_nonexistent_stage_error() {
        let mut stage = empty_stage("deploy");
        stage.needs = vec![NeedsRef::Stage("nonexistent".into())];
        let ff = minimal_forgefile(vec![PipelineItem::Stage(stage)]);
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("does not exist")));
    }

    #[test]
    fn runner_reference_nonexistent_error() {
        let mut stage = empty_stage("build");
        stage.runner = Some(RunnerRef::Named("fast".into()));
        let ff = minimal_forgefile(vec![PipelineItem::Stage(stage)]);
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("undefined runner")));
    }

    #[test]
    fn runner_reference_exists_passes() {
        let mut stage = empty_stage("build");
        stage.runner = Some(RunnerRef::Named("fast".into()));
        let ff = Forgefile {
            runners: vec![RunnerDef {
                name: "fast".into(),
                tags: vec![],
                cpu: None,
                mem: None,
                gpu: None,
                arch: None,
                image: None,
            }],
            secrets: vec![],
            services: vec![],
            triggers: vec![TriggerBlock {
                triggers: vec![Trigger::Push(vec!["main".into()])],
                items: vec![PipelineItem::Stage(stage)],
            }],
        };
        assert!(validate(&ff).is_ok());
    }

    #[test]
    fn circular_dependency_error() {
        let mut a = empty_stage("a");
        a.needs = vec![NeedsRef::Stage("b".into())];
        let mut b = empty_stage("b");
        b.needs = vec![NeedsRef::Stage("a".into())];
        let ff = minimal_forgefile(vec![
            PipelineItem::Stage(a),
            PipelineItem::Stage(b),
        ]);
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("circular dependency")));
    }

    #[test]
    fn service_reference_nonexistent_error() {
        let mut stage = empty_stage("test");
        stage.services = vec!["postgres".into()];
        let ff = minimal_forgefile(vec![PipelineItem::Stage(stage)]);
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("undefined service")));
    }

    #[test]
    fn stage_no_run_command_error() {
        let mut stage = empty_stage("build");
        stage.commands.clear();
        let ff = minimal_forgefile(vec![PipelineItem::Stage(stage)]);
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("no run commands")));
    }

    #[test]
    fn stage_with_deploy_block_no_run_passes() {
        let mut stage = empty_stage("release");
        stage.commands.clear();
        stage.deploy = Some(crate::ast::DeployDef {
            name: "my-app".into(),
            domain: None,
            port: None,
            compose_file: None,
        });
        let ff = minimal_forgefile(vec![PipelineItem::Stage(stage)]);
        assert!(validate(&ff).is_ok());
    }

    #[test]
    fn empty_vault_path_error() {
        let ff = Forgefile {
            runners: vec![],
            secrets: vec![SecretsDef {
                vault_path: Expr::Literal("".into()),
                keys: vec![],
            }],
            services: vec![],
            triggers: vec![],
        };
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("vault path must not be empty")));
    }

    #[test]
    fn valid_complex_forgefile() {
        let mut test_stage = empty_stage("test");
        test_stage.runner = Some(RunnerRef::Named("fast".into()));
        test_stage.services = vec!["postgres".into()];

        let mut deploy_stage = empty_stage("deploy");
        deploy_stage.needs = vec![NeedsRef::Stage("test".into())];

        let ff = Forgefile {
            runners: vec![RunnerDef {
                name: "fast".into(),
                tags: vec!["linux".into()],
                cpu: Some(4),
                mem: None,
                gpu: None,
                arch: None,
                image: None,
            }],
            secrets: vec![SecretsDef {
                vault_path: Expr::Literal("secret/ci".into()),
                keys: vec![SecretKey {
                    name: "TOKEN".into(),
                    alias: None,
                }],
            }],
            services: vec![ServiceDef {
                name: "postgres".into(),
                image: "postgres:15".into(),
                env: vec![],
                health: None,
                expose: vec![5432],
            }],
            triggers: vec![TriggerBlock {
                triggers: vec![Trigger::Push(vec!["main".into()])],
                items: vec![
                    PipelineItem::Stage(test_stage),
                    PipelineItem::Stage(deploy_stage),
                ],
            }],
        };
        assert!(validate(&ff).is_ok());
    }

    #[test]
    fn matrix_empty_variable_error() {
        let matrix = MatrixDef {
            name: "build".into(),
            variables: vec![MatrixVariable {
                name: "os".into(),
                values: vec![],
            }],
            runner: None,
            stage: empty_stage("build"),
        };
        let ff = minimal_forgefile(vec![PipelineItem::Matrix(matrix)]);
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("has no values")));
    }

    #[test]
    fn deploy_empty_name_error() {
        let mut stage = empty_stage("deploy");
        stage.deploy = Some(DeployDef {
            name: "".into(),
            domain: None,
            port: None,
            compose_file: None,
        });
        let ff = minimal_forgefile(vec![PipelineItem::Stage(stage)]);
        let errs = validate(&ff).unwrap_err();
        assert!(errs.iter().any(|e| format!("{e}").contains("empty name")));
    }
}
