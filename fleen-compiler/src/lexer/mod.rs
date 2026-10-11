//! Lexer module: converts source code into tokens.

pub mod error;
pub mod lexer_impl;
pub mod token;

pub use error::{LexError, LexErrorKind};
pub use lexer_impl::{Lexer, tokenize, tokenize_in_file};
pub use token::{Span, Token, TokenKind};
