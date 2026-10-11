//! Fleen compiler crate.
//!
//! Pipeline: Lex → Parse → Resolve → Typeck → Lower → Codegen → Bytecode

pub mod codegen;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod resolver;
pub mod source_map;
pub mod typeck;

pub use source_map::{FileId, SourceMap};

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
/// Single-file convenience: the source is registered as `FileId(0)` of an
/// internal session (the single-file convention of 0.0.3 V01), so all
/// pre-0.0.3 diagnostics and outputs are unchanged. For multi-file
/// compilations use [`Session`].
///
/// # Errors
/// [`CompileError`] describing which stage failed and why.
///
/// # Example
/// ```
/// let module = fleen_compiler::compile("func main(): int = 42").unwrap();
/// assert_eq!(module.version, 3);
/// ```
pub fn compile(source: &str) -> Result<codegen::Bytecode, CompileError> {
    let mut session = Session::new();
    let root = session.add_file("source", source);
    session.compile(root)
}

/// Multi-file compilation session (0.0.3 V01): a [`SourceMap`] plus the
/// stage chain, so drivers can register several files up front and compile
/// one of them as the root.
///
/// In 0.0.3 the module system is not built yet, so `compile` accepts any
/// registered file and treats it as a standalone program; the module table
/// (V05) will consume the full source map.
///
/// # Example
/// ```
/// let mut session = fleen_compiler::Session::new();
/// let root = session.add_file("main.fln", "func main(): int = 42");
/// let module = session.compile(root).unwrap();
/// assert_eq!(module.version, 3);
/// ```
pub struct Session {
    sources: SourceMap,
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl Session {
    /// Create an empty session.
    pub fn new() -> Self {
        Self {
            sources: SourceMap::new(),
        }
    }

    /// Register a source file; returns its [`FileId`] (0, 1, 2, …).
    pub fn add_file(&mut self, name: impl Into<String>, source: impl Into<String>) -> FileId {
        self.sources.add_file(name, source)
    }

    /// The registered files.
    pub fn sources(&self) -> &SourceMap {
        &self.sources
    }

    /// Compile the file registered under `root`, running every stage in
    /// order. Each stage remains an independent, separately testable step;
    /// this only chains them and stops at the first failing stage.
    ///
    /// # Errors
    /// [`CompileError`] describing which stage failed and why.
    ///
    /// # Panics
    /// If `root` was never registered.
    pub fn compile(&self, root: FileId) -> Result<codegen::Bytecode, CompileError> {
        let source = &self.sources.get(root).source;
        let tokens = lexer::tokenize_in_file(root, source).map_err(CompileError::Lex)?;
        let ast = parser::parse(tokens).map_err(CompileError::Parse)?;
        let hir = resolver::resolve(ast).map_err(CompileError::Resolve)?;
        let typed = typeck::typeck(hir).map_err(CompileError::Typeck)?;
        let mir = lower::lower(typed).map_err(CompileError::Lower)?;
        codegen::codegen(mir).map_err(CompileError::Codegen)
    }
}

#[cfg(test)]
mod tests;
