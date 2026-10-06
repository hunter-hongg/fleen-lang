//! Parser error types.

use crate::lexer::{Span, TokenKind};
use std::fmt;

/// Parser error with span information.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

/// Error kinds for parse errors.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    /// Expected a specific token but found something else.
    Expected { expected: String, found: TokenKind },
    /// Reached end of file unexpectedly.
    UnexpectedEof,
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseErrorKind::Expected { expected, found } => {
                write!(f, "expected {}, found {}", expected, found)
            }
            ParseErrorKind::UnexpectedEof => {
                write!(f, "unexpected end of input")
            }
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error: {} at {}", self.kind, self.span)
    }
}

impl std::error::Error for ParseError {}
