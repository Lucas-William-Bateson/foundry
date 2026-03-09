use thiserror::Error;

#[derive(Debug, Error)]
pub enum ForgeError {
    #[error("lex error at position {position}: {message}")]
    LexError { position: usize, message: String },

    #[error("parse error at line {line}: {message}")]
    ParseError { line: usize, message: String },

    #[error("validation error: {0}")]
    ValidationError(String),
}
