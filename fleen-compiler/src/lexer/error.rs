//! Lexer errors and diagnostics.

use crate::lexer::token::Span;
use std::fmt;

/// Error kind for lexer errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexErrorKind {
    /// Invalid character encountered
    InvalidChar(char),
    /// Unterminated string literal
    UnterminatedString,
    /// Unterminated multi-line comment
    UnterminatedComment,
    /// Invalid escape sequence in string
    InvalidEscape(char),
    /// Invalid number format
    InvalidNumber(String),
    /// Integer literal out of range
    IntOverflow,
    /// Float literal out of range
    FloatOverflow,
}

/// Lexer error with span information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Span,
}

impl LexError {
    pub fn new(kind: LexErrorKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn invalid_char(ch: char, span: Span) -> Self {
        Self::new(LexErrorKind::InvalidChar(ch), span)
    }

    pub fn unterminated_string(span: Span) -> Self {
        Self::new(LexErrorKind::UnterminatedString, span)
    }

    pub fn invalid_escape(ch: char, span: Span) -> Self {
        Self::new(LexErrorKind::InvalidEscape(ch), span)
    }

    pub fn invalid_number(msg: String, span: Span) -> Self {
        Self::new(LexErrorKind::InvalidNumber(msg), span)
    }

    pub fn int_overflow(span: Span) -> Self {
        Self::new(LexErrorKind::IntOverflow, span)
    }

    pub fn float_overflow(span: Span) -> Self {
        Self::new(LexErrorKind::FloatOverflow, span)
    }

    pub fn unterminated_comment(span: Span) -> Self {
        Self::new(LexErrorKind::UnterminatedComment, span)
    }
}

impl fmt::Display for LexErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexErrorKind::InvalidChar(ch) => {
                write!(f, "invalid character '{}'", ch.escape_default())
            }
            LexErrorKind::UnterminatedString => write!(f, "unterminated string literal"),
            LexErrorKind::UnterminatedComment => write!(f, "unterminated multi-line comment"),
            LexErrorKind::InvalidEscape(ch) => write!(f, "invalid escape sequence '\\{}'", ch),
            LexErrorKind::InvalidNumber(msg) => write!(f, "invalid number: {}", msg),
            LexErrorKind::IntOverflow => write!(f, "integer literal out of range"),
            LexErrorKind::FloatOverflow => write!(f, "float literal out of range"),
        }
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.kind, self.span)
    }
}

impl std::error::Error for LexError {}
