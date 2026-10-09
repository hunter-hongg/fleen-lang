//! Fleen compiler crate.
//!
//! Pipeline: Lex → Parse → Resolve → Typeck → Lower → Codegen → Bytecode

pub mod codegen;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod resolver;
pub mod typeck;

use std::fmt;

/// Failure of any stage in the source → bytecode pipeline.
///
/// The variant names the stage that failed; the inner types are the same
/// errors the individual stage functions return.
#[derive(Debug)]
pub enum CompileError {
    /// Lexing failed.
    Lex(Vec<lexer::LexError>),
    /// Parsing failed.
    Parse(parser::ParseError),
    /// Name resolution failed.
    Resolve(Vec<resolver::error::ResolveError>),
    /// Type checking failed.
    Typeck(Vec<typeck::error::TypeckError>),
    /// HIR → MIR lowering failed.
    Lower(lower::LowerError),
    /// MIR → bytecode generation failed.
    Codegen(codegen::CodegenError),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Lists are formatted one error per line; single errors inline.
        match self {
            CompileError::Lex(errors) => write_error_list(f, "lex", errors),
            CompileError::Parse(err) => write!(f, "parse: {err}"),
            CompileError::Resolve(errors) => write_error_list(f, "resolve", errors),
            CompileError::Typeck(errors) => write_error_list(f, "typeck", errors),
            CompileError::Lower(err) => write!(f, "lower: {err}"),
            CompileError::Codegen(err) => write!(f, "codegen: {err}"),
        }
    }
}

fn write_error_list<E: fmt::Display>(
    f: &mut fmt::Formatter<'_>,
    stage: &str,
    errors: &[E],
) -> fmt::Result {
    write!(f, "{stage}: {} error(s)", errors.len())?;
    for err in errors {
        write!(f, "\n  {err}")?;
    }
    Ok(())
}

impl std::error::Error for CompileError {}

/// Compile Fleen source to a bytecode module, running every stage in order.
///
/// Convenience entry point for drivers that hold source text (e.g. the
/// `fleen-vm` binary executing `.fln` files directly). Each stage remains an
/// independent, separately testable step; this only chains them and stops at
/// the first failing stage.
///
/// # Errors
/// [`CompileError`] describing which stage failed and why.
///
/// # 示例
/// ```
/// let module = fleen_compiler::compile("func main(): int = 42").unwrap();
/// assert_eq!(module.version, 2);
/// ```
pub fn compile(source: &str) -> Result<codegen::Bytecode, CompileError> {
    let tokens = lexer::tokenize(source).map_err(CompileError::Lex)?;
    let ast = parser::parse(tokens).map_err(CompileError::Parse)?;
    let hir = resolver::resolve(ast).map_err(CompileError::Resolve)?;
    let typed = typeck::typeck(hir).map_err(CompileError::Typeck)?;
    let mir = lower::lower(typed).map_err(CompileError::Lower)?;
    codegen::codegen(mir).map_err(CompileError::Codegen)
}

#[cfg(test)]
mod tests;
