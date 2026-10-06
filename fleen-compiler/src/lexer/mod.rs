//! Lexer module: converts source code into tokens.

pub mod error;
pub mod lexer_impl;
pub mod token;

pub use error::{LexError, LexErrorKind};
pub use lexer_impl::{Lexer, tokenize};
pub use token::{Span, Token, TokenKind};
