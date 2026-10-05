//! Fleen compiler crate.
//!
//! Pipeline: Lex → Parse → Resolve → Typeck → Lower → Codegen → Bytecode

pub mod lexer;
pub mod parser;
pub mod resolver;
pub mod typeck;
