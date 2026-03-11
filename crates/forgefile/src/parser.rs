use std::ops::Range;

use crate::ast::*;
use crate::error::ForgeError;
use crate::lexer::{tokenize, Token};

pub struct Parser {
    tokens: Vec<(Token, Range<usize>)>,
    pos: usize,
    source: String,
    errors: Vec<ForgeError>,
}

impl Parser {
    fn new(tokens: Vec<(Token, Range<usize>)>, source: String) -> Self {
        Self {
            tokens,
            pos: 0,
            source,
            errors: Vec::new(),
        }
    }

    fn line_of(&self, offset: usize) -> usize {
        self.source[..offset.min(self.source.len())]
            .chars()
            .filter(|&c| c == '\n')
            .count()
            + 1
    }

    fn current_line(&self) -> usize {
        if let Some((_, span)) = self.tokens.get(self.pos) {
            self.line_of(span.start)
        } else if let Some((_, span)) = self.tokens.last() {
            self.line_of(span.end)
        } else {
            1
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|(t, _)| t)
    }

    fn advance(&mut self) -> Option<(Token, Range<usize>)> {
        if self.pos < self.tokens.len() {
            let item = self.tokens[self.pos].clone();
            self.pos += 1;
            Some(item)
        } else {
            None
        }
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn expect(&mut self, expected: &Token) -> Result<(Token, Range<usize>), ForgeError> {
        if let Some(tok) = self.peek() {
            if tok == expected {
                return Ok(self.advance().unwrap());
            }
        }
        Err(ForgeError::ParseError {
            line: self.current_line(),
            message: format!("expected {:?}, found {:?}", expected, self.peek()),
        })
    }

    fn expect_string(&mut self) -> Result<String, ForgeError> {
        match self.peek() {
            Some(Token::StringLit(_)) | Some(Token::MultilineStringLit(_)) => {
                match self.advance().unwrap().0 {
                    Token::StringLit(s) | Token::MultilineStringLit(s) => Ok(s),
                    _ => unreachable!(),
                }
            }
            _ => Err(ForgeError::ParseError {
                line: self.current_line(),
                message: format!("expected string, found {:?}", self.peek()),
            }),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ForgeError> {
        match self.peek().and_then(|t| t.as_ident()) {
            Some(name) => {
                self.advance();
                Ok(name)
            }
            None => Err(ForgeError::ParseError {
                line: self.current_line(),
                message: format!("expected identifier, found {:?}", self.peek()),
            }),
        }
    }

    fn expect_int(&mut self) -> Result<i64, ForgeError> {
        match self.peek() {
            Some(Token::IntLit(_)) => match self.advance().unwrap().0 {
                Token::IntLit(n) => Ok(n),
                _ => unreachable!(),
            },
            _ => Err(ForgeError::ParseError {
                line: self.current_line(),
                message: format!("expected integer, found {:?}", self.peek()),
            }),
        }
    }

    /// Skip tokens until we find a closing brace or a top-level keyword.
    fn synchronize(&mut self) {
        let mut depth = 0i32;
        while !self.at_end() {
            match self.peek() {
                Some(Token::LBrace) => {
                    depth += 1;
                    self.advance();
                }
                Some(Token::RBrace) => {
                    if depth <= 0 {
                        self.advance();
                        return;
                    }
                    depth -= 1;
                    self.advance();
                }
                Some(Token::Runner | Token::Secrets | Token::Service | Token::On) if depth <= 0 => {
                    return;
                }
                _ => {
                    self.advance();
                }
            }
        }
    }

    // ------- Top-level parsing -------

    fn parse_forgefile(&mut self) -> Result<Forgefile, Vec<ForgeError>> {
        let mut forgefile = Forgefile {
            runners: Vec::new(),
            secrets: Vec::new(),
            services: Vec::new(),
            triggers: Vec::new(),
        };

        while !self.at_end() {
            match self.peek() {
                Some(Token::Runner) => match self.parse_runner_def() {
                    Ok(r) => forgefile.runners.push(r),
                    Err(e) => {
                        self.errors.push(e);
                        self.synchronize();
                    }
                },
                Some(Token::Secrets) => match self.parse_secrets_def() {
                    Ok(s) => forgefile.secrets.push(s),
                    Err(e) => {
                        self.errors.push(e);
                        self.synchronize();
                    }
                },
                Some(Token::Service) => match self.parse_service_def() {
                    Ok(s) => forgefile.services.push(s),
                    Err(e) => {
                        self.errors.push(e);
                        self.synchronize();
                    }
                },
                Some(Token::On) => match self.parse_trigger_block() {
                    Ok(t) => forgefile.triggers.push(t),
                    Err(e) => {
                        self.errors.push(e);
                        self.synchronize();
                    }
                },
                _ => {
                    self.errors.push(ForgeError::ParseError {
                        line: self.current_line(),
                        message: format!(
                            "unexpected token {:?}, expected runner, secrets, service, or on",
                            self.peek()
                        ),
                    });
                    self.advance();
                }
            }
        }

        if self.errors.is_empty() {
            Ok(forgefile)
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    // ------- Runner -------

    fn parse_runner_def(&mut self) -> Result<RunnerDef, ForgeError> {
        self.expect(&Token::Runner)?;
        let name = self.expect_string()?;
        self.expect(&Token::LBrace)?;

        let mut def = RunnerDef {
            name,
            tags: Vec::new(),
            cpu: None,
            mem: None,
            gpu: None,
            arch: None,
            image: None,
        };

        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            match self.peek() {
                Some(Token::Tags) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    self.expect(&Token::LBracket)?;
                    def.tags = self.parse_string_list(&Token::RBracket)?;
                    self.expect(&Token::RBracket)?;
                }
                Some(Token::Cpu) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.cpu = Some(self.expect_int()? as u32);
                }
                Some(Token::Mem) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.mem = Some(self.expect_string()?);
                }
                Some(Token::Gpu) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.gpu = Some(self.expect_int()? as u32);
                }
                Some(Token::Arch) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.arch = Some(self.expect_string()?);
                }
                Some(Token::Image) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.image = Some(self.expect_string()?);
                }
                _ => {
                    return Err(ForgeError::ParseError {
                        line: self.current_line(),
                        message: format!("unexpected token in runner block: {:?}", self.peek()),
                    });
                }
            }
        }

        self.expect(&Token::RBrace)?;
        Ok(def)
    }

    // ------- Secrets -------

    fn parse_secrets_def(&mut self) -> Result<SecretsDef, ForgeError> {
        self.expect(&Token::Secrets)?;
        self.expect(&Token::From)?;

        // Accept either vault("path") or store("path")
        let source = match self.peek() {
            Some(Token::Vault) => {
                self.advance();
                self.expect(&Token::LParen)?;
                let path = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                SecretsSource::Vault(path)
            }
            Some(Token::Store) => {
                self.advance();
                self.expect(&Token::LParen)?;
                let path = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                SecretsSource::Store(path)
            }
            _ => {
                return Err(ForgeError::ParseError {
                    line: self.current_line(),
                    message: format!(
                        "expected 'vault' or 'store' after 'secrets from', found {:?}",
                        self.peek()
                    ),
                });
            }
        };

        self.expect(&Token::LBrace)?;

        let mut keys = Vec::new();
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            let name = self.expect_ident()?;
            let alias = if self.peek() == Some(&Token::As) {
                self.advance();
                Some(self.expect_ident()?)
            } else {
                None
            };
            keys.push(SecretKey { name, alias });
        }

        self.expect(&Token::RBrace)?;
        Ok(SecretsDef { source, keys })
    }

    // ------- Service -------

    fn parse_service_def(&mut self) -> Result<ServiceDef, ForgeError> {
        self.expect(&Token::Service)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let mut def = ServiceDef {
            name,
            image: String::new(),
            env: Vec::new(),
            health: None,
            expose: Vec::new(),
        };

        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            match self.peek() {
                Some(Token::Image) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.image = self.expect_string()?;
                }
                Some(Token::Env) => {
                    self.advance();
                    let key = self.expect_ident()?;
                    self.expect(&Token::Eq)?;
                    let value = self.parse_expr()?;
                    def.env.push(EnvVar { key, value });
                }
                Some(Token::Health) => {
                    self.advance();
                    def.health = Some(self.expect_string()?);
                }
                Some(Token::Expose) => {
                    self.advance();
                    def.expose.push(self.expect_int()? as u16);
                }
                _ => {
                    return Err(ForgeError::ParseError {
                        line: self.current_line(),
                        message: format!("unexpected token in service block: {:?}", self.peek()),
                    });
                }
            }
        }

        self.expect(&Token::RBrace)?;
        Ok(def)
    }

    // ------- Trigger block -------

    fn parse_trigger_block(&mut self) -> Result<TriggerBlock, ForgeError> {
        self.expect(&Token::On)?;
        let mut triggers = vec![self.parse_trigger()?];
        while self.peek() == Some(&Token::Comma) {
            self.advance();
            triggers.push(self.parse_trigger()?);
        }
        self.expect(&Token::LBrace)?;

        let mut items = Vec::new();
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            match self.peek() {
                Some(Token::Stage) => items.push(PipelineItem::Stage(self.parse_stage_def()?)),
                Some(Token::Matrix) => items.push(PipelineItem::Matrix(self.parse_matrix_def()?)),
                _ => {
                    return Err(ForgeError::ParseError {
                        line: self.current_line(),
                        message: format!(
                            "expected stage or matrix, found {:?}",
                            self.peek()
                        ),
                    });
                }
            }
        }

        self.expect(&Token::RBrace)?;
        Ok(TriggerBlock { triggers, items })
    }

    fn parse_trigger(&mut self) -> Result<Trigger, ForgeError> {
        match self.peek() {
            Some(Token::Push) => {
                self.advance();
                self.expect(&Token::LParen)?;
                let patterns = self.parse_string_list(&Token::RParen)?;
                self.expect(&Token::RParen)?;
                Ok(Trigger::Push(patterns))
            }
            Some(Token::Pr) => {
                self.advance();
                self.expect(&Token::LParen)?;
                let patterns = self.parse_string_list(&Token::RParen)?;
                self.expect(&Token::RParen)?;
                Ok(Trigger::Pr(patterns))
            }
            Some(Token::Schedule) => {
                self.advance();
                self.expect(&Token::LParen)?;
                let cron = self.expect_string()?;
                let timezone = if self.peek() == Some(&Token::Comma) {
                    self.advance();
                    self.expect(&Token::Tz)?;
                    self.expect(&Token::Colon)?;
                    Some(self.expect_string()?)
                } else {
                    None
                };
                self.expect(&Token::RParen)?;
                Ok(Trigger::Schedule { cron, timezone })
            }
            Some(Token::Ident(s)) if s == "failure" => {
                self.advance();
                Ok(Trigger::Failure)
            }
            _ => Err(ForgeError::ParseError {
                line: self.current_line(),
                message: format!("expected trigger (push, pr, schedule, failure), found {:?}", self.peek()),
            }),
        }
    }

    // ------- Stage -------

    fn parse_stage_def(&mut self) -> Result<StageDef, ForgeError> {
        self.expect(&Token::Stage)?;
        let name = self.expect_ident()?;

        let runner = if self.peek() == Some(&Token::On) {
            self.advance();
            Some(self.parse_runner_ref()?)
        } else {
            None
        };

        self.expect(&Token::LBrace)?;
        let mut stage = self.new_stage(name, runner);
        self.parse_stage_fields(&mut stage)?;
        self.expect(&Token::RBrace)?;
        Ok(stage)
    }

    fn new_stage(&self, name: String, runner: Option<RunnerRef>) -> StageDef {
        StageDef {
            name,
            runner,
            needs: Vec::new(),
            commands: Vec::new(),
            env: Vec::new(),
            services: Vec::new(),
            artifacts: Vec::new(),
            outputs: Vec::new(),
            deploy: None,
            condition: None,
            allow_failure: false,
            retry: None,
            timeout: None,
        }
    }

    fn parse_stage_fields(&mut self, stage: &mut StageDef) -> Result<(), ForgeError> {
        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            match self.peek() {
                Some(Token::Run) => {
                    self.advance();
                    let s = self.expect_string()?;
                    stage.commands.push(parse_interpolation(&s));
                }
                Some(Token::Needs) => {
                    self.advance();
                    let dep_name = self.expect_ident()?;
                    let needs = if self.peek() == Some(&Token::LParen) {
                        self.advance();
                        self.expect(&Token::Star)?;
                        self.expect(&Token::RParen)?;
                        NeedsRef::MatrixAll(dep_name)
                    } else {
                        NeedsRef::Stage(dep_name)
                    };
                    stage.needs.push(needs);
                }
                Some(Token::Env) => {
                    self.advance();
                    let key = self.expect_ident()?;
                    self.expect(&Token::Eq)?;
                    let value = self.parse_expr()?;
                    stage.env.push(EnvVar { key, value });
                }
                Some(Token::Services) => {
                    self.advance();
                    self.expect(&Token::LBracket)?;
                    stage.services = self.parse_ident_list(&Token::RBracket)?;
                    self.expect(&Token::RBracket)?;
                }
                Some(Token::Artifacts) => {
                    self.advance();
                    let path = self.expect_string()?;
                    stage.artifacts.push(path);
                }
                Some(Token::Output) => {
                    self.advance();
                    let key = self.expect_ident()?;
                    self.expect(&Token::Eq)?;
                    let value = self.parse_expr()?;
                    stage.outputs.push(OutputDef { key, value });
                }
                Some(Token::Condition) => {
                    self.advance();
                    stage.condition = Some(self.parse_condition()?);
                }
                Some(Token::AllowFailure) => {
                    self.advance();
                    stage.allow_failure = true;
                }
                Some(Token::Retry) => {
                    self.advance();
                    stage.retry = Some(self.expect_int()? as u32);
                }
                Some(Token::Timeout) => {
                    self.advance();
                    stage.timeout = Some(self.parse_duration()?);
                }
                Some(Token::Deploy) => {
                    stage.deploy = Some(self.parse_deploy_block()?);
                }
                _ => {
                    return Err(ForgeError::ParseError {
                        line: self.current_line(),
                        message: format!("unexpected token in stage block: {:?}", self.peek()),
                    });
                }
            }
        }
        Ok(())
    }

    // ------- Matrix -------

    fn parse_matrix_def(&mut self) -> Result<MatrixDef, ForgeError> {
        self.expect(&Token::Matrix)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LParen)?;

        let mut variables = vec![self.parse_matrix_var()?];
        while self.peek() == Some(&Token::Comma) {
            self.advance();
            // Stop if the next token is RParen (trailing comma)
            if self.peek() == Some(&Token::RParen) {
                break;
            }
            variables.push(self.parse_matrix_var()?);
        }
        self.expect(&Token::RParen)?;

        let runner = if self.peek() == Some(&Token::On) {
            self.advance();
            Some(self.parse_runner_ref()?)
        } else {
            None
        };

        self.expect(&Token::LBrace)?;
        let mut stage = self.new_stage(name.clone(), runner.clone());
        self.parse_stage_fields(&mut stage)?;
        self.expect(&Token::RBrace)?;

        Ok(MatrixDef {
            name,
            variables,
            runner,
            stage,
        })
    }

    fn parse_matrix_var(&mut self) -> Result<MatrixVariable, ForgeError> {
        let name = self.expect_ident()?;
        self.expect(&Token::Colon)?;
        self.expect(&Token::LBracket)?;
        let values = self.parse_string_list(&Token::RBracket)?;
        self.expect(&Token::RBracket)?;
        Ok(MatrixVariable { name, values })
    }

    // ------- Deploy -------

    fn parse_deploy_block(&mut self) -> Result<DeployDef, ForgeError> {
        self.expect(&Token::Deploy)?;
        self.expect(&Token::LBrace)?;

        let mut def = DeployDef {
            name: String::new(),
            domain: None,
            port: None,
            compose_file: None,
        };

        while self.peek() != Some(&Token::RBrace) && !self.at_end() {
            match self.peek() {
                Some(Token::Name) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.name = self.expect_string()?;
                }
                Some(Token::Domain) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.domain = Some(self.expect_string()?);
                }
                Some(Token::Port) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.port = Some(self.expect_int()? as u16);
                }
                Some(Token::ComposeFile) => {
                    self.advance();
                    self.expect(&Token::Eq)?;
                    def.compose_file = Some(self.expect_string()?);
                }
                _ => {
                    return Err(ForgeError::ParseError {
                        line: self.current_line(),
                        message: format!("unexpected token in deploy block: {:?}", self.peek()),
                    });
                }
            }
        }

        self.expect(&Token::RBrace)?;
        Ok(def)
    }

    // ------- Runner ref -------

    fn parse_runner_ref(&mut self) -> Result<RunnerRef, ForgeError> {
        self.expect(&Token::Runner)?;
        self.expect(&Token::Dot)?;
        let name = self.expect_ident()?;
        Ok(RunnerRef::Named(name))
    }

    // ------- Expression -------

    fn parse_expr(&mut self) -> Result<Expr, ForgeError> {
        let s = self.expect_string()?;
        Ok(parse_interpolation(&s))
    }

    // ------- Condition -------

    fn parse_condition(&mut self) -> Result<Condition, ForgeError> {
        match self.peek() {
            Some(Token::Always) => { self.advance(); Ok(Condition::Always) }
            Some(Token::OnSuccess) => { self.advance(); Ok(Condition::OnSuccess) }
            Some(Token::OnFailure) => { self.advance(); Ok(Condition::OnFailure) }
            Some(Token::OnPush) => { self.advance(); Ok(Condition::OnPush) }
            Some(Token::OnPr) => { self.advance(); Ok(Condition::OnPr) }
            _ => Err(ForgeError::ParseError {
                line: self.current_line(),
                message: format!("expected condition keyword, found {:?}", self.peek()),
            }),
        }
    }

    // ------- Duration -------

    fn parse_duration(&mut self) -> Result<Duration, ForgeError> {
        match self.peek() {
            Some(Token::DurationLit(_)) => {
                let s = match self.advance().unwrap().0 {
                    Token::DurationLit(s) => s,
                    _ => unreachable!(),
                };
                parse_duration_str(&s).ok_or_else(|| ForgeError::ParseError {
                    line: self.current_line(),
                    message: format!("invalid duration: {}", s),
                })
            }
            _ => Err(ForgeError::ParseError {
                line: self.current_line(),
                message: format!("expected duration, found {:?}", self.peek()),
            }),
        }
    }

    // ------- Helpers -------

    fn parse_string_list(&mut self, terminator: &Token) -> Result<Vec<String>, ForgeError> {
        let mut items = Vec::new();
        if self.peek() == Some(terminator) {
            return Ok(items);
        }
        items.push(self.expect_string()?);
        while self.peek() == Some(&Token::Comma) {
            self.advance();
            if self.peek() == Some(terminator) {
                break;
            }
            items.push(self.expect_string()?);
        }
        Ok(items)
    }

    fn parse_ident_list(&mut self, terminator: &Token) -> Result<Vec<String>, ForgeError> {
        let mut items = Vec::new();
        if self.peek() == Some(terminator) {
            return Ok(items);
        }
        items.push(self.expect_ident()?);
        while self.peek() == Some(&Token::Comma) {
            self.advance();
            if self.peek() == Some(terminator) {
                break;
            }
            items.push(self.expect_ident()?);
        }
        Ok(items)
    }
}

/// Parse `${...}` interpolation within a string.
fn parse_interpolation(s: &str) -> Expr {
    if !s.contains("${") {
        return Expr::Literal(s.to_string());
    }

    let mut parts = Vec::new();
    let mut rest = s;

    while let Some(idx) = rest.find("${") {
        if idx > 0 {
            parts.push(ExprPart::Text(rest[..idx].to_string()));
        }
        rest = &rest[idx + 2..];
        if let Some(end) = rest.find('}') {
            let var = &rest[..end];
            if let Some(dot) = var.find('.') {
                parts.push(ExprPart::StageOutput(
                    var[..dot].to_string(),
                    var[dot + 1..].to_string(),
                ));
            } else {
                parts.push(ExprPart::Variable(var.to_string()));
            }
            rest = &rest[end + 1..];
        } else {
            // Unterminated interpolation — treat as text
            parts.push(ExprPart::Text(format!("${{{}", rest)));
            rest = "";
        }
    }
    if !rest.is_empty() {
        parts.push(ExprPart::Text(rest.to_string()));
    }

    Expr::Interpolated(parts)
}

/// Parse a duration string like "10m", "30s", "1h", "2d".
fn parse_duration_str(s: &str) -> Option<Duration> {
    let (num, unit) = s.split_at(s.len() - 1);
    let n: u64 = num.parse().ok()?;
    let seconds = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        "d" => n * 86400,
        _ => return None,
    };
    Some(Duration { seconds })
}

/// Main entry point: parse Forgefile source into AST.
pub fn parse(input: &str) -> Result<Forgefile, Vec<ForgeError>> {
    let tokens = tokenize(input).map_err(|e| {
        vec![ForgeError::LexError {
            position: e.span.start,
            message: e.message,
        }]
    })?;
    let mut parser = Parser::new(tokens, input.to_string());
    parser.parse_forgefile()
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_minimal_forgefile() {
        let input = r#"on push("main") { stage test { run "cargo test" } }"#;
        let ff = parse(input).unwrap();
        assert_eq!(ff.triggers.len(), 1);
        assert_eq!(ff.triggers[0].triggers, vec![Trigger::Push(vec!["main".into()])]);
        assert_eq!(ff.triggers[0].items.len(), 1);
        match &ff.triggers[0].items[0] {
            PipelineItem::Stage(s) => {
                assert_eq!(s.name, "test");
                assert_eq!(s.commands, vec![Expr::Literal("cargo test".into())]);
            }
            _ => panic!("expected stage"),
        }
    }

    #[test]
    fn test_runner_all_fields() {
        let input = r#"runner "heavy" {
            tags = ["gpu", "fast"]
            cpu = 8
            mem = "16G"
            gpu = 2
            arch = "x86_64"
            image = "rust:1.87"
        }"#;
        let ff = parse(input).unwrap();
        assert_eq!(ff.runners.len(), 1);
        let r = &ff.runners[0];
        assert_eq!(r.name, "heavy");
        assert_eq!(r.tags, vec!["gpu", "fast"]);
        assert_eq!(r.cpu, Some(8));
        assert_eq!(r.mem, Some("16G".into()));
        assert_eq!(r.gpu, Some(2));
        assert_eq!(r.arch, Some("x86_64".into()));
        assert_eq!(r.image, Some("rust:1.87".into()));
    }

    #[test]
    fn test_secrets_with_aliases() {
        let input = r#"secrets from vault("foundry/prod") {
            API_KEY as GH_TOKEN
            DB_PASSWORD
        }"#;
        let ff = parse(input).unwrap();
        assert_eq!(ff.secrets.len(), 1);
        assert_eq!(ff.secrets[0].source, SecretsSource::Vault(Expr::Literal("foundry/prod".into())));
        assert_eq!(ff.secrets[0].keys.len(), 2);
        assert_eq!(ff.secrets[0].keys[0].name, "API_KEY");
        assert_eq!(ff.secrets[0].keys[0].alias, Some("GH_TOKEN".into()));
        assert_eq!(ff.secrets[0].keys[1].name, "DB_PASSWORD");
        assert_eq!(ff.secrets[0].keys[1].alias, None);
    }

    #[test]
    fn test_service_definition() {
        let input = r#"service postgres {
            image = "postgres:17"
            env POSTGRES_PASSWORD = "test"
            health "pg_isready"
            expose 5432
        }"#;
        let ff = parse(input).unwrap();
        assert_eq!(ff.services.len(), 1);
        let s = &ff.services[0];
        assert_eq!(s.name, "postgres");
        assert_eq!(s.image, "postgres:17");
        assert_eq!(s.env.len(), 1);
        assert_eq!(s.env[0].key, "POSTGRES_PASSWORD");
        assert_eq!(s.health, Some("pg_isready".into()));
        assert_eq!(s.expose, vec![5432]);
    }

    #[test]
    fn test_multi_stage_with_needs() {
        let input = r#"on push("main") {
            stage lint {
                run "cargo clippy"
            }
            stage test {
                needs lint
                run "cargo test"
            }
            stage ship {
                needs test
                run "cargo build --release"
            }
        }"#;
        let ff = parse(input).unwrap();
        let items = &ff.triggers[0].items;
        assert_eq!(items.len(), 3);
        match &items[1] {
            PipelineItem::Stage(s) => {
                assert_eq!(s.name, "test");
                assert_eq!(s.needs, vec![NeedsRef::Stage("lint".into())]);
            }
            _ => panic!("expected stage"),
        }
    }

    #[test]
    fn test_matrix_definition() {
        let input = r#"on push("main") {
            matrix build(target: ["x86_64", "aarch64"], os: ["linux", "macos"]) on runner.fast {
                needs test
                run "cargo build"
            }
        }"#;
        let ff = parse(input).unwrap();
        match &ff.triggers[0].items[0] {
            PipelineItem::Matrix(m) => {
                assert_eq!(m.name, "build");
                assert_eq!(m.variables.len(), 2);
                assert_eq!(m.variables[0].name, "target");
                assert_eq!(m.variables[0].values, vec!["x86_64", "aarch64"]);
                assert_eq!(m.variables[1].name, "os");
                assert_eq!(m.variables[1].values, vec!["linux", "macos"]);
                assert_eq!(m.runner, Some(RunnerRef::Named("fast".into())));
                assert_eq!(m.stage.needs, vec![NeedsRef::Stage("test".into())]);
            }
            _ => panic!("expected matrix"),
        }
    }

    #[test]
    fn test_deploy_block() {
        let input = r#"on push("main") {
            stage release {
                run "cargo build --release"
                deploy {
                    name = "my-app"
                    domain = "example.com"
                    port = 8080
                    compose_file = "docker-compose.yml"
                }
            }
        }"#;
        let ff = parse(input).unwrap();
        match &ff.triggers[0].items[0] {
            PipelineItem::Stage(s) => {
                let d = s.deploy.as_ref().unwrap();
                assert_eq!(d.name, "my-app");
                assert_eq!(d.domain, Some("example.com".into()));
                assert_eq!(d.port, Some(8080));
                assert_eq!(d.compose_file, Some("docker-compose.yml".into()));
            }
            _ => panic!("expected stage"),
        }
    }

    #[test]
    fn test_duration_parsing() {
        assert_eq!(parse_duration_str("10m"), Some(Duration { seconds: 600 }));
        assert_eq!(parse_duration_str("30s"), Some(Duration { seconds: 30 }));
        assert_eq!(parse_duration_str("1h"), Some(Duration { seconds: 3600 }));
        assert_eq!(parse_duration_str("2d"), Some(Duration { seconds: 172800 }));
    }

    #[test]
    fn test_string_interpolation() {
        let expr = parse_interpolation("target/${arch}-linux");
        assert_eq!(
            expr,
            Expr::Interpolated(vec![
                ExprPart::Text("target/".into()),
                ExprPart::Variable("arch".into()),
                ExprPart::Text("-linux".into()),
            ])
        );

        let expr2 = parse_interpolation("${build.binary_path}");
        assert_eq!(
            expr2,
            Expr::Interpolated(vec![ExprPart::StageOutput("build".into(), "binary_path".into())])
        );

        let plain = parse_interpolation("hello world");
        assert_eq!(plain, Expr::Literal("hello world".into()));
    }

    #[test]
    fn test_complete_forgefile() {
        let input = r#"
            runner "fast" {
                cpu = 4
                mem = "8G"
                image = "rust:1.87-slim"
            }

            secrets from vault("foundry/prod") {
                API_KEY as GH_TOKEN
            }

            service postgres {
                image = "postgres:17"
                expose 5432
            }

            on push("main", "release/*"), pr("main") {
                stage lint on runner.fast {
                    run "cargo clippy"
                    allow_failure
                    condition on_push
                }

                stage test on runner.fast {
                    needs lint
                    run "cargo test"
                    timeout 10m
                    retry 2
                    services [postgres]
                    artifacts "target/test-results"
                    env CI = "true"
                }

                matrix build(target: ["x86_64", "aarch64"]) {
                    needs test
                    run "cargo build --release"
                    output binary = "${build.binary_path}"
                }
            }

            on schedule("0 3 * * *", tz: "Europe/Berlin") {
                stage nightly {
                    run "cargo test --all-features"
                }
            }
        "#;
        let ff = parse(input).unwrap();
        assert_eq!(ff.runners.len(), 1);
        assert_eq!(ff.secrets.len(), 1);
        assert_eq!(ff.services.len(), 1);
        assert_eq!(ff.triggers.len(), 2);
        assert_eq!(ff.triggers[0].triggers.len(), 2);
        assert_eq!(ff.triggers[0].items.len(), 3);
        assert_eq!(ff.triggers[1].triggers[0], Trigger::Schedule {
            cron: "0 3 * * *".into(),
            timezone: Some("Europe/Berlin".into()),
        });
    }

    #[test]
    fn test_error_missing_brace() {
        let input = r#"on push("main") { stage test { run "cargo test" }"#;
        let err = parse(input).unwrap_err();
        assert!(!err.is_empty());
        assert!(format!("{:?}", err[0]).contains("expected"));
    }

    #[test]
    fn test_error_unknown_toplevel() {
        let input = r#"foobar "something""#;
        let err = parse(input).unwrap_err();
        assert!(!err.is_empty());
        assert!(format!("{:?}", err[0]).contains("unexpected token"));
    }

    #[test]
    fn test_secrets_from_store() {
        let input = r#"secrets from store("myapp/prod") {
            KEY1
            KEY2
        }"#;
        let ff = parse(input).unwrap();
        assert_eq!(ff.secrets.len(), 1);
        assert_eq!(
            ff.secrets[0].source,
            SecretsSource::Store(Expr::Literal("myapp/prod".into()))
        );
        assert_eq!(ff.secrets[0].keys.len(), 2);
        assert_eq!(ff.secrets[0].keys[0].name, "KEY1");
        assert_eq!(ff.secrets[0].keys[0].alias, None);
        assert_eq!(ff.secrets[0].keys[1].name, "KEY2");
        assert_eq!(ff.secrets[0].keys[1].alias, None);
    }

    #[test]
    fn test_secrets_from_vault_still_works() {
        let input = r#"secrets from vault("myapp/prod") {
            KEY1
        }"#;
        let ff = parse(input).unwrap();
        assert_eq!(ff.secrets.len(), 1);
        assert_eq!(
            ff.secrets[0].source,
            SecretsSource::Vault(Expr::Literal("myapp/prod".into()))
        );
        assert_eq!(ff.secrets[0].keys.len(), 1);
        assert_eq!(ff.secrets[0].keys[0].name, "KEY1");
    }
}
