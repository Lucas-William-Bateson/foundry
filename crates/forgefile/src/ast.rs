/// Root node of a parsed Forgefile.
///
/// A Forgefile describes a CI/CD pipeline: runners, secrets, services,
/// and one or more trigger blocks that contain stages and matrices.
#[derive(Debug, Clone, PartialEq)]
pub struct Forgefile {
    pub runners: Vec<RunnerDef>,
    pub secrets: Vec<SecretsDef>,
    pub services: Vec<ServiceDef>,
    pub triggers: Vec<TriggerBlock>,
}

// ---------------------------------------------------------------------------
// Runner definitions
// ---------------------------------------------------------------------------

/// `runner "name" { ... }` — declares a named execution environment.
#[derive(Debug, Clone, PartialEq)]
pub struct RunnerDef {
    pub name: String,
    pub tags: Vec<String>,
    pub cpu: Option<u32>,
    pub mem: Option<String>,
    pub gpu: Option<u32>,
    pub arch: Option<String>,
    pub image: Option<String>,
}

/// How a stage references a runner.
#[derive(Debug, Clone, PartialEq)]
pub enum RunnerRef {
    /// `runner.fast` — by name.
    Named(String),
    /// `runner[tags has "gpu" && cpu >= 4]` — expression-based matching.
    Expr(RunnerExpr),
}

/// Runner matching expression (composable with `And`/`Or`).
#[derive(Debug, Clone, PartialEq)]
pub enum RunnerExpr {
    TagsHas(String),
    CpuGte(u32),
    MemGte(String),
    GpuGte(u32),
    ArchEq(String),
    And(Box<RunnerExpr>, Box<RunnerExpr>),
    Or(Box<RunnerExpr>, Box<RunnerExpr>),
}

// ---------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------

/// Where secrets are loaded from.
#[derive(Debug, Clone, PartialEq)]
pub enum SecretsSource {
    /// `vault("path")` — HashiCorp Vault.
    Vault(Expr),
    /// `store("path")` — local encrypted secrets store.
    Store(Expr),
}

/// `secrets from vault("path") { ... }` or `secrets from store("path") { ... }`.
#[derive(Debug, Clone, PartialEq)]
pub struct SecretsDef {
    pub source: SecretsSource,
    pub keys: Vec<SecretKey>,
}

impl SecretsDef {
    /// Returns the path expression regardless of source kind.
    pub fn path_expr(&self) -> &Expr {
        match &self.source {
            SecretsSource::Vault(e) | SecretsSource::Store(e) => e,
        }
    }
}

/// A single key inside a `secrets` block, optionally aliased.
#[derive(Debug, Clone, PartialEq)]
pub struct SecretKey {
    pub name: String,
    /// `KEY as ALIAS`
    pub alias: Option<String>,
}

// ---------------------------------------------------------------------------
// Services
// ---------------------------------------------------------------------------

/// `service name { ... }` — a sidecar container available during stages.
#[derive(Debug, Clone, PartialEq)]
pub struct ServiceDef {
    pub name: String,
    pub image: String,
    pub env: Vec<EnvVar>,
    pub health: Option<String>,
    pub expose: Vec<u16>,
}

// ---------------------------------------------------------------------------
// Triggers
// ---------------------------------------------------------------------------

/// `on push(...), pr(...) { ... }` — a block of pipeline items gated by triggers.
#[derive(Debug, Clone, PartialEq)]
pub struct TriggerBlock {
    pub triggers: Vec<Trigger>,
    pub items: Vec<PipelineItem>,
}

/// An individual trigger predicate.
#[derive(Debug, Clone, PartialEq)]
pub enum Trigger {
    /// `push("main", "release/*")` — branch patterns.
    Push(Vec<String>),
    /// `pr("main")` — target-branch patterns.
    Pr(Vec<String>),
    /// `schedule("0 3 * * *", tz: "Europe/Berlin")`.
    Schedule {
        cron: String,
        timezone: Option<String>,
    },
    /// `on failure { ... }`.
    Failure,
}

// ---------------------------------------------------------------------------
// Pipeline items (stages & matrices)
// ---------------------------------------------------------------------------

/// Items that appear inside a trigger block.
#[derive(Debug, Clone, PartialEq)]
pub enum PipelineItem {
    Stage(StageDef),
    Matrix(MatrixDef),
}

/// `stage name [on runner.X] { ... }` — a single pipeline step.
#[derive(Debug, Clone, PartialEq)]
pub struct StageDef {
    pub name: String,
    pub runner: Option<RunnerRef>,
    pub needs: Vec<NeedsRef>,
    pub commands: Vec<Expr>,
    pub env: Vec<EnvVar>,
    pub services: Vec<String>,
    pub artifacts: Vec<String>,
    pub outputs: Vec<OutputDef>,
    pub deploy: Option<DeployDef>,
    pub condition: Option<Condition>,
    pub allow_failure: bool,
    pub retry: Option<u32>,
    pub timeout: Option<Duration>,
}

/// Reference to a dependency stage.
#[derive(Debug, Clone, PartialEq)]
pub enum NeedsRef {
    /// `needs test` — single named stage.
    Stage(String),
    /// `needs build(*)` — all matrix expansions of the named matrix.
    MatrixAll(String),
}

/// `output key = "value"` — stage output declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct OutputDef {
    pub key: String,
    pub value: Expr,
}

/// `deploy { ... }` — deployment metadata block.
#[derive(Debug, Clone, PartialEq)]
pub struct DeployDef {
    pub name: String,
    pub domain: Option<String>,
    pub port: Option<u16>,
    pub compose_file: Option<String>,
}

/// Conditions that gate stage execution.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    Always,
    OnSuccess,
    OnFailure,
    OnPush,
    OnPr,
    /// Custom expression (future extension).
    Expr(Expr),
}

// ---------------------------------------------------------------------------
// Matrix
// ---------------------------------------------------------------------------

/// `matrix name(var: [vals], ...) { ... }` — expands a stage across variable combinations.
#[derive(Debug, Clone, PartialEq)]
pub struct MatrixDef {
    pub name: String,
    pub variables: Vec<MatrixVariable>,
    pub runner: Option<RunnerRef>,
    /// The stage body that will be expanded for each combination.
    pub stage: StageDef,
}

/// A single axis in a matrix expansion.
#[derive(Debug, Clone, PartialEq)]
pub struct MatrixVariable {
    pub name: String,
    pub values: Vec<String>,
}

// ---------------------------------------------------------------------------
// Shared primitives
// ---------------------------------------------------------------------------

/// An environment variable binding.
#[derive(Debug, Clone, PartialEq)]
pub struct EnvVar {
    pub key: String,
    pub value: Expr,
}

/// Expression type supporting string interpolation.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A plain string with no interpolation.
    Literal(String),
    /// A string containing `${...}` interpolation segments.
    Interpolated(Vec<ExprPart>),
}

/// One segment of an interpolated expression.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprPart {
    /// Literal text between interpolation sites.
    Text(String),
    /// `${branch}` — a simple variable reference.
    Variable(String),
    /// `${build.binary_path}` — output from another stage.
    StageOutput(String, String),
}

/// Duration with human-friendly parsing (e.g. `10m`, `30s`).
#[derive(Debug, Clone, PartialEq)]
pub struct Duration {
    pub seconds: u64,
}

impl Duration {
    pub fn from_minutes(m: u64) -> Self {
        Self { seconds: m * 60 }
    }

    pub fn from_seconds(s: u64) -> Self {
        Self { seconds: s }
    }
}
