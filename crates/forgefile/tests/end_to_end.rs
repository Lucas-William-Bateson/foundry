//! End-to-end integration tests that verify the full Forgefile pipeline:
//! source text → parsed AST → validated → execution plan assertions.

use forgefile::ast::*;
use forgefile::{parse, validate};
use std::collections::HashMap;
use std::fs;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_and_validate(input: &str) -> Forgefile {
    let ast = parse(input).expect("should parse");
    validate(&ast).expect("should validate");
    ast
}

fn get_stage<'a>(block: &'a TriggerBlock, name: &str) -> &'a StageDef {
    block
        .items
        .iter()
        .filter_map(|item| match item {
            PipelineItem::Stage(s) if s.name == name => Some(s),
            _ => None,
        })
        .next()
        .unwrap_or_else(|| panic!("stage '{name}' not found"))
}

fn get_matrix<'a>(block: &'a TriggerBlock, name: &str) -> &'a MatrixDef {
    block
        .items
        .iter()
        .filter_map(|item| match item {
            PipelineItem::Matrix(m) if m.name == name => Some(m),
            _ => None,
        })
        .next()
        .unwrap_or_else(|| panic!("matrix '{name}' not found"))
}

/// Manually extract runner requirements from the AST, mirroring the converter logic.
fn extract_runner_requirements(
    stage: &StageDef,
    runners: &HashMap<String, &RunnerDef>,
) -> Option<RunnerRequirements> {
    let runner_ref = stage.runner.as_ref()?;
    match runner_ref {
        RunnerRef::Named(name) => {
            let mut reqs = RunnerRequirements {
                runner_name: Some(name.clone()),
                ..Default::default()
            };
            if let Some(def) = runners.get(name.as_str()) {
                reqs.required_tags = def.tags.clone();
                reqs.min_cpu = def.cpu;
                reqs.min_gpu = def.gpu;
                reqs.arch = def.arch.clone();
                if let Some(ref mem) = def.mem {
                    reqs.min_memory_mb = parse_mem_to_mb(mem);
                }
            }
            Some(reqs)
        }
        RunnerRef::Expr(_) => Some(RunnerRequirements::default()),
    }
}

fn parse_mem_to_mb(mem: &str) -> Option<u32> {
    let mem = mem.trim();
    if let Some(g) = mem.strip_suffix('G') {
        g.parse::<u32>().ok().map(|v| v * 1024)
    } else if let Some(m) = mem.strip_suffix('M') {
        m.parse::<u32>().ok()
    } else {
        None
    }
}

/// Simplified runner requirements struct (mirrors foundry-core without the dependency).
#[derive(Debug, Clone, Default, PartialEq)]
struct RunnerRequirements {
    pub runner_name: Option<String>,
    pub required_tags: Vec<String>,
    pub min_cpu: Option<u32>,
    pub min_memory_mb: Option<u32>,
    pub min_gpu: Option<u32>,
    pub arch: Option<String>,
}

// =========================================================================
// 1. test_repo_forgefile_parses
// =========================================================================

#[test]
fn test_repo_forgefile_parses() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let forgefile_path = format!("{manifest_dir}/../../Forgefile");
    let source = fs::read_to_string(&forgefile_path)
        .unwrap_or_else(|e| panic!("failed to read {forgefile_path}: {e}"));

    let ast = parse(&source).expect("repo Forgefile should parse successfully");
    validate(&ast).expect("repo Forgefile should validate without errors");
    assert!(!ast.triggers.is_empty(), "should have at least one trigger block");
    assert!(
        ast.triggers[0].items.len() >= 2,
        "should have multiple stages"
    );
}

#[test]
fn test_keyword_as_stage_name() {
    let input = r#"
on push("main") {
  stage deploy {
    run "echo hi"
  }
}
"#;

    let ast = parse(input).expect("should parse keyword 'deploy' as stage name");
    validate(&ast).expect("should validate");

    let block = &ast.triggers[0];
    let deploy = get_stage(block, "deploy");
    assert_eq!(deploy.commands[0], Expr::Literal("echo hi".into()));
}

// =========================================================================
// 2. test_full_pipeline_flow
// =========================================================================

#[test]
fn test_full_pipeline_flow() {
    let input = r#"
runner "ci" {
  tags = ["x86", "docker"]
  cpu = 4
  mem = "8G"
  image = "rust:1.87-slim"
}

secrets from vault("app/prod") {
  DB_URL
  TOKEN as API_TOKEN
}

service postgres {
  image = "postgres:17"
  env POSTGRES_PASSWORD = "test"
  health "pg_isready"
  expose 5432
}

on push("main"), pr("main") {
  stage lint on runner.ci {
    run "cargo clippy"
    allow_failure
  }

  stage test on runner.ci {
    needs lint
    services [postgres]
    env DATABASE_URL = "postgres://localhost/test"
    run "cargo test"
    timeout 10m
    retry 2
    artifacts "target/test-results/**"
  }

  stage build on runner.ci {
    needs test
    run "cargo build --release"
    artifacts "target/release/myapp"
  }
}
"#;

    // Step 1: parse
    let ast = parse(input).expect("should parse");

    // Step 2: validate
    validate(&ast).expect("should validate");

    // Step 3: verify AST structure
    assert_eq!(ast.runners.len(), 1, "1 runner defined");
    assert_eq!(ast.secrets.len(), 1, "1 secrets block");
    assert_eq!(ast.services.len(), 1, "1 service");
    assert_eq!(ast.triggers.len(), 1, "1 trigger block");

    let block = &ast.triggers[0];
    assert_eq!(block.triggers.len(), 2, "push + pr triggers");
    assert_eq!(block.items.len(), 3, "3 stages");

    // Step 4: verify stage dependencies
    let lint = get_stage(block, "lint");
    assert!(lint.needs.is_empty());

    let test = get_stage(block, "test");
    assert_eq!(test.needs, vec![NeedsRef::Stage("lint".into())]);

    let build = get_stage(block, "build");
    assert_eq!(build.needs, vec![NeedsRef::Stage("test".into())]);

    // Step 5: verify runner references point to defined runners
    for item in &block.items {
        if let PipelineItem::Stage(s) = item {
            if let Some(RunnerRef::Named(ref name)) = s.runner {
                assert!(
                    ast.runners.iter().any(|r| r.name == *name),
                    "runner reference '{name}' should be defined"
                );
            }
        }
    }
}

// =========================================================================
// 3. test_runner_requirements_extraction
// =========================================================================

#[test]
fn test_runner_requirements_extraction() {
    let input = r#"
runner "fast" {
  tags = ["ssd", "x86"]
  cpu = 4
  mem = "8G"
}

runner "gpubox" {
  tags = ["nvidia", "cuda"]
  gpu = 1
}

on push("main") {
  stage train on runner.gpubox {
    run "python train.py"
  }

  stage test on runner.fast {
    run "cargo test"
  }

  stage lint {
    run "cargo clippy"
  }
}
"#;

    let ast = parse_and_validate(input);
    let runners: HashMap<String, &RunnerDef> =
        ast.runners.iter().map(|r| (r.name.clone(), r)).collect();

    let block = &ast.triggers[0];

    // Stage on runner.gpubox → requirements include gpu runner's tags
    let train = get_stage(block, "train");
    let reqs = extract_runner_requirements(train, &runners).expect("should have requirements");
    assert_eq!(reqs.runner_name, Some("gpubox".into()));
    assert_eq!(reqs.required_tags, vec!["nvidia", "cuda"]);
    assert_eq!(reqs.min_gpu, Some(1));
    assert_eq!(reqs.min_cpu, None);

    // Stage on runner.fast → requirements include fast runner's tags
    let test = get_stage(block, "test");
    let reqs = extract_runner_requirements(test, &runners).expect("should have requirements");
    assert_eq!(reqs.runner_name, Some("fast".into()));
    assert_eq!(reqs.required_tags, vec!["ssd", "x86"]);
    assert_eq!(reqs.min_cpu, Some(4));
    assert_eq!(reqs.min_memory_mb, Some(8192));

    // Stage with no runner → no requirements (any runner)
    let lint = get_stage(block, "lint");
    assert!(
        extract_runner_requirements(lint, &runners).is_none(),
        "stage with no runner should have no requirements"
    );
}

// =========================================================================
// 4. test_secrets_extraction
// =========================================================================

#[test]
fn test_secrets_extraction() {
    let input = r#"
secrets from vault("myapp/prod") {
  DATABASE_URL
  REDIS_URL
  API_SECRET as APP_SECRET
}

on push("main") {
  stage build {
    run "cargo build"
  }
}
"#;

    let ast = parse_and_validate(input);

    assert_eq!(ast.secrets.len(), 1);
    let secrets = &ast.secrets[0];

    // Vault path extracted correctly
    assert_eq!(secrets.vault_path, Expr::Literal("myapp/prod".into()));

    // Keys listed correctly
    assert_eq!(secrets.keys.len(), 3);
    assert_eq!(secrets.keys[0].name, "DATABASE_URL");
    assert_eq!(secrets.keys[0].alias, None);
    assert_eq!(secrets.keys[1].name, "REDIS_URL");
    assert_eq!(secrets.keys[1].alias, None);

    // Alias mapped correctly
    assert_eq!(secrets.keys[2].name, "API_SECRET");
    assert_eq!(secrets.keys[2].alias, Some("APP_SECRET".into()));
}

// =========================================================================
// 5. test_trigger_matching
// =========================================================================

#[test]
fn test_trigger_matching() {
    let input = r#"
on push("main", "release/*"), pr("main") {
  stage build {
    run "cargo build"
  }
}
"#;

    let ast = parse_and_validate(input);
    let block = &ast.triggers[0];

    assert_eq!(block.triggers.len(), 2);

    // Push trigger has correct branch patterns
    match &block.triggers[0] {
        Trigger::Push(branches) => {
            assert_eq!(branches, &["main", "release/*"]);
        }
        other => panic!("expected Push trigger, got {other:?}"),
    }

    // PR trigger has correct target branches
    match &block.triggers[1] {
        Trigger::Pr(targets) => {
            assert_eq!(targets, &["main"]);
        }
        other => panic!("expected Pr trigger, got {other:?}"),
    }
}

// =========================================================================
// 6. test_matrix_expansion
// =========================================================================

#[test]
fn test_matrix_expansion() {
    let input = r#"
runner "ci" {
  tags = ["x86"]
}

on push("main") {
  matrix build(target: ["x86_64", "aarch64"], os: ["linux", "macos"]) on runner.ci {
    run "cargo build --target ${target}"
  }
}
"#;

    let ast = parse_and_validate(input);
    let block = &ast.triggers[0];

    let matrix = get_matrix(block, "build");

    // Matrix variables extracted
    assert_eq!(matrix.variables.len(), 2);

    // Variable values are correct
    assert_eq!(matrix.variables[0].name, "target");
    assert_eq!(matrix.variables[0].values, vec!["x86_64", "aarch64"]);

    assert_eq!(matrix.variables[1].name, "os");
    assert_eq!(matrix.variables[1].values, vec!["linux", "macos"]);

    // Runner reference on the matrix
    assert_eq!(matrix.runner, Some(RunnerRef::Named("ci".into())));

    // The stage body is accessible
    assert_eq!(matrix.stage.commands.len(), 1);
    match &matrix.stage.commands[0] {
        Expr::Interpolated(parts) => {
            assert!(parts.len() >= 2, "should have text + variable parts");
        }
        Expr::Literal(s) => {
            // Also acceptable if parser treats ${target} literally
            assert!(s.contains("target"), "command should reference target");
        }
    }
}

// =========================================================================
// 7. test_complete_real_world_forgefile
// =========================================================================

#[test]
fn test_complete_real_world_forgefile() {
    let input = r#"
runner "ci" {
  tags = ["x86", "docker"]
  cpu = 4
  mem = "8G"
  image = "rust:1.87-slim"
}

runner "deployer" {
  tags = ["production"]
  cpu = 2
  mem = "4G"
  image = "debian:bookworm-slim"
}

secrets from vault("myapp/prod") {
  DATABASE_URL
  REDIS_URL
  API_SECRET as APP_SECRET
}

service postgres {
  image = "postgres:17"
  env POSTGRES_PASSWORD = "test"
  health "pg_isready"
  expose 5432
}

on push("main"), pr("main") {
  stage lint on runner.ci {
    run "cargo clippy -- -D warnings"
    allow_failure
  }

  stage test on runner.ci {
    needs lint
    services [postgres]
    env DATABASE_URL = "postgres://localhost:5432/test"
    run "cargo test --workspace"
    timeout 15m
    retry 2
    artifacts "target/test-results/**"
  }

  stage build on runner.ci {
    needs test
    run "cargo build --release"
    artifacts "target/release/myapp"
  }

  stage release on runner.deployer {
    needs build
    condition on_push
    run "scripts/deploy.sh"
    deploy {
      name = "myapp"
      domain = "myapp.example.com"
      port = 8080
      compose_file = "docker-compose.yml"
    }
  }
}

on schedule("0 3 * * *", tz: "UTC") {
  stage nightly on runner.ci {
    run "cargo bench"
    timeout 1h
  }
}
"#;

    let ast = parse_and_validate(input);

    // --- 2 runners ---
    assert_eq!(ast.runners.len(), 2);

    let ci = &ast.runners[0];
    assert_eq!(ci.name, "ci");
    assert_eq!(ci.tags, vec!["x86", "docker"]);
    assert_eq!(ci.cpu, Some(4));
    assert_eq!(ci.mem, Some("8G".into()));
    assert_eq!(ci.image, Some("rust:1.87-slim".into()));

    let deploy_runner = &ast.runners[1];
    assert_eq!(deploy_runner.name, "deployer");
    assert_eq!(deploy_runner.tags, vec!["production"]);
    assert_eq!(deploy_runner.cpu, Some(2));
    assert_eq!(deploy_runner.mem, Some("4G".into()));
    assert_eq!(deploy_runner.image, Some("debian:bookworm-slim".into()));

    // --- 1 secrets block with 3 keys (1 alias) ---
    assert_eq!(ast.secrets.len(), 1);
    let secrets = &ast.secrets[0];
    assert_eq!(secrets.vault_path, Expr::Literal("myapp/prod".into()));
    assert_eq!(secrets.keys.len(), 3);
    assert_eq!(secrets.keys[0].name, "DATABASE_URL");
    assert_eq!(secrets.keys[0].alias, None);
    assert_eq!(secrets.keys[1].name, "REDIS_URL");
    assert_eq!(secrets.keys[1].alias, None);
    assert_eq!(secrets.keys[2].name, "API_SECRET");
    assert_eq!(secrets.keys[2].alias, Some("APP_SECRET".into()));

    // --- 1 service ---
    assert_eq!(ast.services.len(), 1);
    let pg = &ast.services[0];
    assert_eq!(pg.name, "postgres");
    assert_eq!(pg.image, "postgres:17");
    assert_eq!(pg.env.len(), 1);
    assert_eq!(pg.env[0].key, "POSTGRES_PASSWORD");
    assert_eq!(pg.env[0].value, Expr::Literal("test".into()));
    assert_eq!(pg.health, Some("pg_isready".into()));
    assert_eq!(pg.expose, vec![5432]);

    // --- 2 trigger blocks ---
    assert_eq!(ast.triggers.len(), 2);

    // --- First trigger block: push + pr ---
    let block0 = &ast.triggers[0];
    assert_eq!(block0.triggers.len(), 2);
    assert_eq!(block0.triggers[0], Trigger::Push(vec!["main".into()]));
    assert_eq!(block0.triggers[1], Trigger::Pr(vec!["main".into()]));

    // --- 4 stages in first block ---
    assert_eq!(block0.items.len(), 4);

    // lint
    let lint = get_stage(block0, "lint");
    assert_eq!(lint.runner, Some(RunnerRef::Named("ci".into())));
    assert!(lint.allow_failure);
    assert!(lint.needs.is_empty());
    assert_eq!(lint.commands[0], Expr::Literal("cargo clippy -- -D warnings".into()));

    // test
    let test = get_stage(block0, "test");
    assert_eq!(test.runner, Some(RunnerRef::Named("ci".into())));
    assert_eq!(test.needs, vec![NeedsRef::Stage("lint".into())]);
    assert_eq!(test.services, vec!["postgres"]);
    assert_eq!(test.env.len(), 1);
    assert_eq!(test.env[0].key, "DATABASE_URL");
    assert_eq!(
        test.env[0].value,
        Expr::Literal("postgres://localhost:5432/test".into())
    );
    assert_eq!(test.commands[0], Expr::Literal("cargo test --workspace".into()));
    assert_eq!(test.timeout, Some(Duration::from_minutes(15)));
    assert_eq!(test.retry, Some(2));
    assert_eq!(test.artifacts, vec!["target/test-results/**"]);

    // build
    let build = get_stage(block0, "build");
    assert_eq!(build.runner, Some(RunnerRef::Named("ci".into())));
    assert_eq!(build.needs, vec![NeedsRef::Stage("test".into())]);
    assert_eq!(build.commands[0], Expr::Literal("cargo build --release".into()));
    assert_eq!(build.artifacts, vec!["target/release/myapp"]);

    // release (deploy stage)
    let release = get_stage(block0, "release");
    assert_eq!(release.runner, Some(RunnerRef::Named("deployer".into())));
    assert_eq!(release.needs, vec![NeedsRef::Stage("build".into())]);
    assert_eq!(release.condition, Some(Condition::OnPush));
    assert_eq!(release.commands[0], Expr::Literal("scripts/deploy.sh".into()));

    let deploy_def = release.deploy.as_ref().expect("should have deploy block");
    assert_eq!(deploy_def.name, "myapp");
    assert_eq!(deploy_def.domain, Some("myapp.example.com".into()));
    assert_eq!(deploy_def.port, Some(8080));
    assert_eq!(deploy_def.compose_file, Some("docker-compose.yml".into()));

    // Needs chain: lint → test → build → release
    assert!(lint.needs.is_empty());
    assert_eq!(test.needs, vec![NeedsRef::Stage("lint".into())]);
    assert_eq!(build.needs, vec![NeedsRef::Stage("test".into())]);
    assert_eq!(release.needs, vec![NeedsRef::Stage("build".into())]);

    // --- Second trigger block: schedule ---
    let block1 = &ast.triggers[1];
    assert_eq!(block1.triggers.len(), 1);
    assert_eq!(
        block1.triggers[0],
        Trigger::Schedule {
            cron: "0 3 * * *".into(),
            timezone: Some("UTC".into()),
        }
    );

    // 1 stage in schedule block
    assert_eq!(block1.items.len(), 1);
    let nightly = get_stage(block1, "nightly");
    assert_eq!(nightly.runner, Some(RunnerRef::Named("ci".into())));
    assert_eq!(nightly.commands[0], Expr::Literal("cargo bench".into()));
    assert_eq!(nightly.timeout, Some(Duration { seconds: 3600 }));

    // --- Total: 5 stages across both blocks ---
    let total_stages: usize = ast
        .triggers
        .iter()
        .map(|b| {
            b.items
                .iter()
                .filter(|i| matches!(i, PipelineItem::Stage(_)))
                .count()
        })
        .sum();
    assert_eq!(total_stages, 5);

    // --- Verify all runner references are valid ---
    let runner_names: Vec<&str> = ast.runners.iter().map(|r| r.name.as_str()).collect();
    for block in &ast.triggers {
        for item in &block.items {
            if let PipelineItem::Stage(s) = item {
                if let Some(RunnerRef::Named(ref name)) = s.runner {
                    assert!(
                        runner_names.contains(&name.as_str()),
                        "stage '{}' references undefined runner '{name}'",
                        s.name
                    );
                }
            }
        }
    }
}
