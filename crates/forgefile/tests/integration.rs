use forgefile::ast::*;
use forgefile::{parse, validate};
use std::fs;

fn fixture(name: &str) -> String {
    fs::read_to_string(format!(
        "{}/tests/fixtures/{}",
        env!("CARGO_MANIFEST_DIR"),
        name
    ))
    .unwrap()
}

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
        .unwrap_or_else(|| panic!("stage '{}' not found", name))
}

// =========================================================================
// Fixture: minimal.forge
// =========================================================================

#[test]
fn test_minimal_parse_and_validate() {
    let ast = parse_and_validate(&fixture("minimal.forge"));

    assert_eq!(ast.triggers.len(), 1);
    assert!(ast.runners.is_empty());
    assert!(ast.secrets.is_empty());
    assert!(ast.services.is_empty());

    let block = &ast.triggers[0];
    assert_eq!(block.triggers.len(), 1);
    assert_eq!(block.triggers[0], Trigger::Push(vec!["main".into()]));
    assert_eq!(block.items.len(), 1);

    let stage = get_stage(block, "test");
    assert_eq!(stage.commands.len(), 1);
    assert_eq!(stage.commands[0], Expr::Literal("cargo test".into()));
    assert!(stage.runner.is_none());
    assert!(stage.needs.is_empty());
    assert!(!stage.allow_failure);
}

// =========================================================================
// Fixture: pipeline.forge
// =========================================================================

#[test]
fn test_pipeline_stages_and_triggers() {
    let ast = parse_and_validate(&fixture("pipeline.forge"));

    assert_eq!(ast.triggers.len(), 1);
    let block = &ast.triggers[0];

    // Two triggers: push + pr
    assert_eq!(block.triggers.len(), 2);
    assert_eq!(block.triggers[0], Trigger::Push(vec!["main".into()]));
    assert_eq!(block.triggers[1], Trigger::Pr(vec!["main".into()]));

    // Three stages
    assert_eq!(block.items.len(), 3);
}

#[test]
fn test_pipeline_lint_stage() {
    let ast = parse_and_validate(&fixture("pipeline.forge"));
    let lint = get_stage(&ast.triggers[0], "lint");

    assert!(lint.allow_failure);
    assert!(lint.needs.is_empty());
    assert_eq!(
        lint.commands[0],
        Expr::Literal("cargo clippy -- -D warnings".into())
    );
}

#[test]
fn test_pipeline_test_stage() {
    let ast = parse_and_validate(&fixture("pipeline.forge"));
    let test = get_stage(&ast.triggers[0], "test");

    assert_eq!(test.needs, vec![NeedsRef::Stage("lint".into())]);
    assert_eq!(test.timeout, Some(Duration::from_minutes(10)));
    assert_eq!(test.retry, Some(2));
    assert!(!test.allow_failure);
}

#[test]
fn test_pipeline_build_stage() {
    let ast = parse_and_validate(&fixture("pipeline.forge"));
    let build = get_stage(&ast.triggers[0], "build");

    assert_eq!(build.needs, vec![NeedsRef::Stage("test".into())]);
    assert_eq!(build.artifacts, vec!["target/release/myapp".to_string()]);
}

// =========================================================================
// Fixture: runners.forge
// =========================================================================

#[test]
fn test_runners_definitions() {
    let ast = parse_and_validate(&fixture("runners.forge"));

    assert_eq!(ast.runners.len(), 2);

    let fast = &ast.runners[0];
    assert_eq!(fast.name, "fast");
    assert_eq!(fast.tags, vec!["ssd", "x86"]);
    assert_eq!(fast.cpu, Some(4));
    assert_eq!(fast.mem, Some("8G".into()));
    assert_eq!(fast.image, Some("rust:1.87-slim".into()));
    assert_eq!(fast.gpu, None);

    let gpu = &ast.runners[1];
    assert_eq!(gpu.name, "gpu-box");
    assert_eq!(gpu.tags, vec!["nvidia", "cuda"]);
    assert_eq!(gpu.gpu, Some(1));
    assert_eq!(gpu.image, Some("nvidia/cuda:12.6-devel".into()));
    assert_eq!(gpu.cpu, None);
}

#[test]
fn test_runners_stage_references() {
    let ast = parse_and_validate(&fixture("runners.forge"));
    let block = &ast.triggers[0];

    let train = get_stage(block, "train");
    assert_eq!(train.runner, Some(RunnerRef::Named("gpu-box".into())));

    let test = get_stage(block, "test");
    assert_eq!(test.runner, Some(RunnerRef::Named("fast".into())));
}

// =========================================================================
// Fixture: secrets.forge
// =========================================================================

#[test]
fn test_secrets_vault_path() {
    let ast = parse_and_validate(&fixture("secrets.forge"));

    assert_eq!(ast.secrets.len(), 1);
    assert_eq!(
        ast.secrets[0].source,
        SecretsSource::Vault(Expr::Literal("myapp/prod".into()))
    );
}

#[test]
fn test_secrets_keys_and_alias() {
    let ast = parse_and_validate(&fixture("secrets.forge"));
    let keys = &ast.secrets[0].keys;

    assert_eq!(keys.len(), 3);

    assert_eq!(keys[0].name, "DATABASE_URL");
    assert_eq!(keys[0].alias, None);

    assert_eq!(keys[1].name, "API_KEY");
    assert_eq!(keys[1].alias, None);

    assert_eq!(keys[2].name, "GITHUB_TOKEN");
    assert_eq!(keys[2].alias, Some("GH_TOKEN".into()));
}

// =========================================================================
// Fixture: services.forge
// =========================================================================

#[test]
fn test_services_definitions() {
    let ast = parse_and_validate(&fixture("services.forge"));

    assert_eq!(ast.services.len(), 2);

    let pg = &ast.services[0];
    assert_eq!(pg.name, "postgres");
    assert_eq!(pg.image, "postgres:17");
    assert_eq!(pg.env.len(), 1);
    assert_eq!(pg.env[0].key, "POSTGRES_PASSWORD");
    assert_eq!(pg.env[0].value, Expr::Literal("test".into()));
    assert_eq!(pg.health, Some("pg_isready -U postgres".into()));
    assert_eq!(pg.expose, vec![5432]);

    let redis = &ast.services[1];
    assert_eq!(redis.name, "redis");
    assert_eq!(redis.image, "redis:7-alpine");
    assert_eq!(redis.health, Some("redis-cli ping".into()));
    assert_eq!(redis.expose, vec![6379]);
    assert!(redis.env.is_empty());
}

#[test]
fn test_services_stage_references() {
    let ast = parse_and_validate(&fixture("services.forge"));
    let stage = get_stage(&ast.triggers[0], "integration_test");

    assert_eq!(stage.services, vec!["postgres", "redis"]);
    assert_eq!(stage.env.len(), 1);
    assert_eq!(stage.env[0].key, "DATABASE_URL");
    assert_eq!(
        stage.env[0].value,
        Expr::Literal("postgres://localhost:5432/test".into())
    );
}

// =========================================================================
// Fixture: full.forge
// =========================================================================

#[test]
fn test_full_roundtrip_parse_validate() {
    let ast = parse_and_validate(&fixture("full.forge"));

    assert_eq!(ast.runners.len(), 1);
    assert_eq!(ast.secrets.len(), 1);
    assert_eq!(ast.services.len(), 1);
    assert_eq!(ast.triggers.len(), 2);
}

#[test]
fn test_full_runner() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let runner = &ast.runners[0];

    assert_eq!(runner.name, "default");
    assert_eq!(runner.tags, vec!["x86"]);
    assert_eq!(runner.cpu, Some(2));
    assert_eq!(runner.mem, Some("4G".into()));
    assert_eq!(runner.image, Some("rust:1.87-slim".into()));
}

#[test]
fn test_full_secrets() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let secrets = &ast.secrets[0];

    assert_eq!(secrets.source, SecretsSource::Vault(Expr::Literal("foundry/prod".into())));
    assert_eq!(secrets.keys.len(), 2);
    assert_eq!(secrets.keys[0].name, "DATABASE_URL");
    assert_eq!(secrets.keys[0].alias, None);
    assert_eq!(secrets.keys[1].name, "API_KEY");
    assert_eq!(secrets.keys[1].alias, Some("FOUNDRY_API_KEY".into()));
}

#[test]
fn test_full_service() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let svc = &ast.services[0];

    assert_eq!(svc.name, "postgres");
    assert_eq!(svc.image, "postgres:17");
    assert_eq!(svc.env[0].key, "POSTGRES_DB");
    assert_eq!(svc.health, Some("pg_isready".into()));
    assert_eq!(svc.expose, vec![5432]);
}

#[test]
fn test_full_push_pr_trigger() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let block = &ast.triggers[0];

    assert_eq!(block.triggers.len(), 2);
    assert_eq!(
        block.triggers[0],
        Trigger::Push(vec!["main".into(), "release/*".into()])
    );
    assert_eq!(block.triggers[1], Trigger::Pr(vec!["main".into()]));
    assert_eq!(block.items.len(), 4);
}

#[test]
fn test_full_lint_stage() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let lint = get_stage(&ast.triggers[0], "lint");

    assert_eq!(lint.runner, Some(RunnerRef::Named("default".into())));
    assert!(lint.allow_failure);
}

#[test]
fn test_full_test_stage() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let test = get_stage(&ast.triggers[0], "test");

    assert_eq!(test.runner, Some(RunnerRef::Named("default".into())));
    assert_eq!(test.needs, vec![NeedsRef::Stage("lint".into())]);
    assert_eq!(test.services, vec!["postgres"]);
    assert_eq!(test.timeout, Some(Duration::from_minutes(15)));
    assert_eq!(test.retry, Some(1));
    assert_eq!(
        test.artifacts,
        vec!["target/test-results/**".to_string()]
    );
    assert_eq!(test.env.len(), 1);
    assert_eq!(test.env[0].key, "DATABASE_URL");
}

#[test]
fn test_full_build_stage() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let build = get_stage(&ast.triggers[0], "build");

    assert_eq!(build.needs, vec![NeedsRef::Stage("test".into())]);
    assert_eq!(build.artifacts, vec!["target/release/foundry*".to_string()]);
}

#[test]
fn test_full_deploy_stage() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let release = get_stage(&ast.triggers[0], "release");

    assert_eq!(release.needs, vec![NeedsRef::Stage("build".into())]);
    assert_eq!(release.condition, Some(Condition::OnPush));

    let deploy_def = release.deploy.as_ref().expect("should have deploy block");
    assert_eq!(deploy_def.name, "foundry");
    assert_eq!(deploy_def.domain, Some("foundry.l3s.me".into()));
    assert_eq!(deploy_def.port, Some(8081));
    assert_eq!(deploy_def.compose_file, Some("docker-compose.yml".into()));
}

#[test]
fn test_full_schedule_trigger() {
    let ast = parse_and_validate(&fixture("full.forge"));
    let block = &ast.triggers[1];

    assert_eq!(block.triggers.len(), 1);
    assert_eq!(
        block.triggers[0],
        Trigger::Schedule {
            cron: "0 3 * * *".into(),
            timezone: Some("Europe/Berlin".into()),
        }
    );

    let nightly = get_stage(block, "nightly");
    assert_eq!(nightly.runner, Some(RunnerRef::Named("default".into())));
    assert_eq!(nightly.commands[0], Expr::Literal("cargo bench".into()));
}

// =========================================================================
// Error cases
// =========================================================================

#[test]
fn error_duplicate_stages() {
    let input = r#"
on push("main") {
  stage build {
    run "cargo build"
  }
  stage build {
    run "cargo build --release"
  }
}
"#;
    let ast = parse(input).expect("should parse");
    let errs = validate(&ast).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| format!("{e}").contains("duplicate stage name")));
}

#[test]
fn error_missing_run() {
    let input = r#"
on push("main") {
  stage empty {
    allow_failure
  }
}
"#;
    let ast = parse(input).expect("should parse");
    let errs = validate(&ast).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| format!("{e}").contains("no run commands")));
}

#[test]
fn error_undefined_runner() {
    let input = r#"
on push("main") {
  stage build on runner.nonexistent {
    run "cargo build"
  }
}
"#;
    let ast = parse(input).expect("should parse");
    let errs = validate(&ast).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| format!("{e}").contains("undefined runner")));
}

#[test]
fn error_undefined_service() {
    let input = r#"
on push("main") {
  stage test {
    services [postgres]
    run "cargo test"
  }
}
"#;
    let ast = parse(input).expect("should parse");
    let errs = validate(&ast).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| format!("{e}").contains("undefined service")));
}

#[test]
fn error_circular_deps() {
    let input = r#"
on push("main") {
  stage a {
    needs b
    run "echo a"
  }
  stage b {
    needs a
    run "echo b"
  }
}
"#;
    let ast = parse(input).expect("should parse");
    let errs = validate(&ast).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| format!("{e}").contains("circular dependency")));
}

#[test]
fn error_syntax_with_line_number() {
    let input = "on push(\"main\") {\n  stage test {\n    !!!\n  }\n}";
    let errs = parse(input).unwrap_err();
    assert!(!errs.is_empty());
    // Should report a meaningful position/line
    let msg = format!("{}", errs[0]);
    assert!(
        msg.contains("position") || msg.contains("line"),
        "error should include position info: {msg}"
    );
}
