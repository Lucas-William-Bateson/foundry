use logos::Logos;
use std::ops::Range;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\r\n]+")]
#[logos(skip r"#[^\n]*")]
pub enum Token {
    // Keywords
    #[token("runner")]
    Runner,
    #[token("stage")]
    Stage,
    #[token("on")]
    On,
    #[token("push")]
    Push,
    #[token("pr")]
    Pr,
    #[token("schedule")]
    Schedule,
    #[token("needs")]
    Needs,
    #[token("run")]
    Run,
    #[token("env")]
    Env,
    #[token("secrets")]
    Secrets,
    #[token("from")]
    From,
    #[token("vault")]
    Vault,
    #[token("service")]
    Service,
    #[token("services")]
    Services,
    #[token("matrix")]
    Matrix,
    #[token("deploy")]
    Deploy,
    #[token("artifacts")]
    Artifacts,
    #[token("output")]
    Output,
    #[token("condition")]
    Condition,
    #[token("allow_failure")]
    AllowFailure,
    #[token("retry")]
    Retry,
    #[token("timeout")]
    Timeout,
    #[token("image")]
    Image,
    #[token("tags")]
    Tags,
    #[token("cpu")]
    Cpu,
    #[token("mem")]
    Mem,
    #[token("gpu")]
    Gpu,
    #[token("arch")]
    Arch,
    #[token("health")]
    Health,
    #[token("expose")]
    Expose,
    #[token("as")]
    As,
    #[token("let")]
    Let,
    #[token("fn")]
    Fn,
    #[token("use")]
    Use,
    #[token("auto")]
    Auto,
    #[token("tz")]
    Tz,
    #[token("name")]
    Name,
    #[token("domain")]
    Domain,
    #[token("port")]
    Port,
    #[token("compose_file")]
    ComposeFile,

    // Condition keywords
    #[token("always")]
    Always,
    #[token("on_success")]
    OnSuccess,
    #[token("on_failure")]
    OnFailure,
    #[token("on_push")]
    OnPush,
    #[token("on_pr")]
    OnPr,

    // Punctuation
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("=")]
    Eq,
    #[token(",")]
    Comma,
    #[token("*")]
    Star,
    #[token(".")]
    Dot,
    #[token(":")]
    Colon,

    // Triple-quoted strings (must come before single-quoted to match first)
    #[token(r#"""""#, lex_multiline_string)]
    MultilineStringLit(String),

    // Single-quoted strings
    #[regex(r#""[^"]*""#, lex_string)]
    StringLit(String),

    // Duration literals (must come before integers)
    #[regex(r"[0-9]+[smhd]", |lex| lex.slice().to_string())]
    DurationLit(String),

    // Integer literals
    #[regex(r"[0-9]+", |lex| lex.slice().parse::<i64>().ok())]
    IntLit(i64),

    // Boolean literals
    #[token("true")]
    True,
    #[token("false")]
    False,

    // Identifiers
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_-]*", |lex| lex.slice().to_string())]
    Ident(String),
}

fn lex_string(lex: &mut logos::Lexer<Token>) -> String {
    let s = lex.slice();
    s[1..s.len() - 1].to_string()
}

fn lex_multiline_string(lex: &mut logos::Lexer<Token>) -> Option<String> {
    let remainder = lex.remainder();
    let end = remainder.find("\"\"\"")?;
    lex.bump(end + 3); // consume content + closing """
    let content = &remainder[..end];
    Some(content.trim().to_string())
}

#[derive(Debug, Clone)]
pub struct LexError {
    pub span: Range<usize>,
    pub message: String,
}

impl Token {
    /// Returns the string representation if this token can be used as an identifier.
    /// Keywords are valid identifiers in name positions (stage names, service names, etc.).
    pub fn as_ident(&self) -> Option<String> {
        match self {
            Token::Ident(s) => Some(s.clone()),
            Token::Runner => Some("runner".into()),
            Token::Stage => Some("stage".into()),
            Token::On => Some("on".into()),
            Token::Push => Some("push".into()),
            Token::Pr => Some("pr".into()),
            Token::Schedule => Some("schedule".into()),
            Token::Needs => Some("needs".into()),
            Token::Run => Some("run".into()),
            Token::Env => Some("env".into()),
            Token::Secrets => Some("secrets".into()),
            Token::From => Some("from".into()),
            Token::Vault => Some("vault".into()),
            Token::Service => Some("service".into()),
            Token::Services => Some("services".into()),
            Token::Matrix => Some("matrix".into()),
            Token::Deploy => Some("deploy".into()),
            Token::Artifacts => Some("artifacts".into()),
            Token::Output => Some("output".into()),
            Token::Condition => Some("condition".into()),
            Token::AllowFailure => Some("allow_failure".into()),
            Token::Retry => Some("retry".into()),
            Token::Timeout => Some("timeout".into()),
            Token::Image => Some("image".into()),
            Token::Tags => Some("tags".into()),
            Token::Cpu => Some("cpu".into()),
            Token::Mem => Some("mem".into()),
            Token::Gpu => Some("gpu".into()),
            Token::Arch => Some("arch".into()),
            Token::Health => Some("health".into()),
            Token::Expose => Some("expose".into()),
            Token::As => Some("as".into()),
            Token::Let => Some("let".into()),
            Token::Fn => Some("fn".into()),
            Token::Use => Some("use".into()),
            Token::Auto => Some("auto".into()),
            Token::Tz => Some("tz".into()),
            Token::Name => Some("name".into()),
            Token::Domain => Some("domain".into()),
            Token::Port => Some("port".into()),
            Token::ComposeFile => Some("compose_file".into()),
            Token::Always => Some("always".into()),
            Token::OnSuccess => Some("on_success".into()),
            Token::OnFailure => Some("on_failure".into()),
            Token::OnPush => Some("on_push".into()),
            Token::OnPr => Some("on_pr".into()),
            // Punctuation, literals, and booleans cannot be identifiers
            _ => None,
        }
    }
}

pub fn tokenize(input: &str) -> Result<Vec<(Token, Range<usize>)>, LexError> {
    let lexer = Token::lexer(input);
    let mut tokens = Vec::new();
    for (result, span) in lexer.spanned() {
        match result {
            Ok(token) => tokens.push((token, span)),
            Err(_) => {
                return Err(LexError {
                    span: span.clone(),
                    message: format!("Unexpected character: {:?}", &input[span]),
                })
            }
        }
    }
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(input: &str) -> Vec<Token> {
        tokenize(input)
            .expect("tokenize failed")
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    }

    #[test]
    fn test_simple_runner_block() {
        let input = r#"runner "fast" { cpu = 4 mem = "8G" }"#;
        let tokens = tok(input);
        assert_eq!(
            tokens,
            vec![
                Token::Runner,
                Token::StringLit("fast".into()),
                Token::LBrace,
                Token::Cpu,
                Token::Eq,
                Token::IntLit(4),
                Token::Mem,
                Token::Eq,
                Token::StringLit("8G".into()),
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn test_string_literals() {
        let tokens = tok(r#""hello" "world" "with spaces""#);
        assert_eq!(
            tokens,
            vec![
                Token::StringLit("hello".into()),
                Token::StringLit("world".into()),
                Token::StringLit("with spaces".into()),
            ]
        );
    }

    #[test]
    fn test_string_interpolation_passthrough() {
        let tokens = tok(r#""target/${arch}-linux""#);
        assert_eq!(tokens, vec![Token::StringLit("target/${arch}-linux".into())]);
    }

    #[test]
    fn test_multiline_strings() {
        let input = r#""""
      cargo test --workspace
      cargo test --doc
    """"#;
        let tokens = tok(input);
        assert_eq!(tokens.len(), 1);
        match &tokens[0] {
            Token::MultilineStringLit(s) => {
                assert!(s.contains("cargo test --workspace"));
                assert!(s.contains("cargo test --doc"));
            }
            other => panic!("Expected MultilineStringLit, got {:?}", other),
        }
    }

    #[test]
    fn test_duration_literals() {
        let tokens = tok("10m 30s 1h 2d");
        assert_eq!(
            tokens,
            vec![
                Token::DurationLit("10m".into()),
                Token::DurationLit("30s".into()),
                Token::DurationLit("1h".into()),
                Token::DurationLit("2d".into()),
            ]
        );
    }

    #[test]
    fn test_integer_vs_duration() {
        let tokens = tok("42 10m");
        assert_eq!(
            tokens,
            vec![Token::IntLit(42), Token::DurationLit("10m".into())]
        );
    }

    #[test]
    fn test_keywords_over_identifiers() {
        let tokens = tok("runner stage on push pr schedule");
        assert_eq!(
            tokens,
            vec![
                Token::Runner,
                Token::Stage,
                Token::On,
                Token::Push,
                Token::Pr,
                Token::Schedule,
            ]
        );
    }

    #[test]
    fn test_identifiers() {
        let tokens = tok("my_var some-name _private");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("my_var".into()),
                Token::Ident("some-name".into()),
                Token::Ident("_private".into()),
            ]
        );
    }

    #[test]
    fn test_comment_skipping() {
        let tokens = tok("runner # this is a comment\nstage");
        assert_eq!(tokens, vec![Token::Runner, Token::Stage]);
    }

    #[test]
    fn test_booleans() {
        let tokens = tok("true false");
        assert_eq!(tokens, vec![Token::True, Token::False]);
    }

    #[test]
    fn test_punctuation() {
        let tokens = tok("{ } ( ) [ ] = , * . :");
        assert_eq!(
            tokens,
            vec![
                Token::LBrace,
                Token::RBrace,
                Token::LParen,
                Token::RParen,
                Token::LBracket,
                Token::RBracket,
                Token::Eq,
                Token::Comma,
                Token::Star,
                Token::Dot,
                Token::Colon,
            ]
        );
    }

    #[test]
    fn test_full_mini_forgefile() {
        let input = r#"
# A mini forgefile
runner "fast" {
  cpu = 4
  image = "rust:1.87-slim"
}

on push("main"), pr("main") {
  stage lint on runner.fast {
    run "cargo clippy"
    allow_failure
  }

  stage test on runner.fast {
    needs lint
    run "cargo test"
    timeout 10m
    retry 2
  }
}
"#;
        let tokens = tok(input);
        // Verify it tokenizes without error and has reasonable token count
        assert!(tokens.len() > 30, "Expected many tokens, got {}", tokens.len());

        // Spot-check key tokens
        assert_eq!(tokens[0], Token::Runner);
        assert_eq!(tokens[1], Token::StringLit("fast".into()));
        assert_eq!(tokens[2], Token::LBrace);
        assert!(tokens.contains(&Token::AllowFailure));
        assert!(tokens.contains(&Token::Needs));
        assert!(tokens.contains(&Token::DurationLit("10m".into())));
        assert!(tokens.contains(&Token::IntLit(2)));
    }

    #[test]
    fn test_condition_keywords() {
        let tokens = tok("condition on_push on_pr on_success on_failure always");
        assert_eq!(
            tokens,
            vec![
                Token::Condition,
                Token::OnPush,
                Token::OnPr,
                Token::OnSuccess,
                Token::OnFailure,
                Token::Always,
            ]
        );
    }

    #[test]
    fn test_deploy_block_tokens() {
        let input = r#"deploy { name = "app" domain = "example.com" port = 8080 compose_file = "docker-compose.yml" }"#;
        let tokens = tok(input);
        assert_eq!(tokens[0], Token::Deploy);
        assert!(tokens.contains(&Token::Name));
        assert!(tokens.contains(&Token::Domain));
        assert!(tokens.contains(&Token::Port));
        assert!(tokens.contains(&Token::ComposeFile));
    }

    #[test]
    fn test_secrets_block() {
        let input = r#"secrets from vault("foundry/prod") { API_KEY as GH_TOKEN }"#;
        let tokens = tok(input);
        assert_eq!(
            tokens,
            vec![
                Token::Secrets,
                Token::From,
                Token::Vault,
                Token::LParen,
                Token::StringLit("foundry/prod".into()),
                Token::RParen,
                Token::LBrace,
                Token::Ident("API_KEY".into()),
                Token::As,
                Token::Ident("GH_TOKEN".into()),
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn test_service_block() {
        let input = r#"service postgres { image = "postgres:17" expose 5432 }"#;
        let tokens = tok(input);
        assert_eq!(
            tokens,
            vec![
                Token::Service,
                Token::Ident("postgres".into()),
                Token::LBrace,
                Token::Image,
                Token::Eq,
                Token::StringLit("postgres:17".into()),
                Token::Expose,
                Token::IntLit(5432),
                Token::RBrace,
            ]
        );
    }

    #[test]
    fn test_matrix_block() {
        let input = r#"matrix build(target: ["x86_64", "aarch64"]) on runner.fast { needs test }"#;
        let tokens = tok(input);
        assert_eq!(tokens[0], Token::Matrix);
        assert_eq!(tokens[1], Token::Ident("build".into()));
        assert!(tokens.contains(&Token::Colon));
        assert!(tokens.contains(&Token::LBracket));
        assert!(tokens.contains(&Token::RBracket));
    }
}
