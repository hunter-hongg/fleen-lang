//! Fleen compiler crate.
//!
//! Pipeline: Lex → Parse → Resolve → Typeck → Lower → Codegen → Bytecode

pub mod codegen;
pub mod lexer;
pub mod lower;
pub mod parser;
pub mod resolver;
pub mod source_map;
pub mod std_src;
pub mod typeck;

pub use source_map::{FileId, SourceMap};

use resolver::modules::{CrateDef, build_module_table};
use resolver::resolve_module_with_table;
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
    /// Dependency crates registered for multi-crate compilation, in
    /// registration order. The root crate is prepended at compile time.
    dependencies: Vec<CrateDef>,
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
            dependencies: Vec::new(),
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

    /// Register a dependency crate whose root file is `root`.
    ///
    /// The crate's modules become importable as `<name>.<module>`, and every
    /// file below the root's directory is compiled as part of it. Files may
    /// live anywhere — a build tool registers them under their real paths, so
    /// diagnostics keep pointing at real files rather than synthesized names.
    ///
    /// Registering two crates under the same name is a
    /// [`DuplicateModule`](resolver::error::ResolveErrorKind::DuplicateModule)
    /// error at compile time.
    ///
    /// # Example
    /// ```
    /// let mut s = fleen_compiler::Session::new();
    /// let root = s.add_file(
    ///     "src/main.fln",
    ///     "import jsonlite.util;\nfunc main(): int = jsonlite.util.answer()",
    /// );
    /// let dep_root = s.add_file(
    ///     "/home/u/.flint/cache/jsonlite-abc/src/mod.fln",
    ///     "pub func answer(): int = 42",
    /// );
    /// s.add_dependency("jsonlite", dep_root);
    /// s.add_file(
    ///     "/home/u/.flint/cache/jsonlite-abc/src/util.fln",
    ///     "pub func answer(): int = 42",
    /// );
    /// s.compile_project(root).unwrap();
    /// ```
    pub fn add_dependency(&mut self, name: impl Into<String>, root: FileId) {
        self.dependencies.push(CrateDef::dependency(name, root));
    }

    /// The registered dependency crates (empty until
    /// [`Session::add_dependency`] is called).
    pub fn dependencies(&self) -> &[CrateDef] {
        &self.dependencies
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

        // Programs that import `std.*` compile against the shipped std
        // sources, which are ordinary modules (0.0.3 V06, D8) — so they take
        // the whole-program pipeline. Everything else keeps the exact
        // single-file chain below (byte-identical to before V06).
        if !uses_std(std::slice::from_ref(&ast)) {
            let hir = resolver::resolve(ast).map_err(CompileError::Resolve)?;
            let typed = typeck::typeck(hir).map_err(CompileError::Typeck)?;
            let mir = lower::lower(typed).map_err(CompileError::Lower)?;
            return codegen::codegen(mir).map_err(CompileError::Codegen);
        }

        let sources = with_std_sources(&self.sources);
        let asts = parse_all(&sources)?;
        let crates = self.crate_list(root, &sources);
        compile_pre_parsed(&sources, root, &crates, asts)
    }

    /// Compile a whole program consisting of the files registered in this
    /// session (0.0.3 V05). The root file is the crate root (`main.fln`);
    /// every other registered `.fln` file must belong to this crate or to one
    /// registered via [`Session::add_dependency`].
    ///
    /// The stage chain is identical to [`Session::compile`], but the module
    /// table sub-pass runs before per-module resolution and the resulting
    /// HIRs are merged into one bytecode module (module structure never
    /// reaches `.flnc`).
    pub fn compile_project(&self, root: FileId) -> Result<codegen::Bytecode, CompileError> {
        let mut asts = parse_all(&self.sources)?;
        let sources = if uses_std(&asts) {
            // A project importing `std.*` compiles against the shipped std
            // sources too: append them, re-parse, and rebuild the table so
            // they are ordinary modules (0.0.3 V06, D8).
            let sources = with_std_sources(&self.sources);
            asts = parse_all(&sources)?;
            sources
        } else {
            self.sources.clone()
        };
        let crates = self.crate_list(root, &sources);
        compile_pre_parsed(&sources, root, &crates, asts)
    }

    /// The crate list for a compilation rooted at `root`: the root crate
    /// first (its modules stay unprefixed), then every registered
    /// dependency, then the shipped `std` crate when its sources were
    /// appended.
    fn crate_list(&self, root: FileId, sources: &SourceMap) -> Vec<CrateDef> {
        let mut crates = Vec::with_capacity(self.dependencies.len() + 2);
        crates.push(CrateDef::root_crate(root));
        crates.extend(self.dependencies.iter().cloned());
        // `std` is a crate like any other; it is just one the compiler ships.
        // Registered only when its sources are actually present, so the
        // no-std path keeps its exact byte-identical behavior.
        if let Some(std_root) = std_crate_root(sources) {
            crates.push(CrateDef::dependency(
                resolver::modules::STD_CRATE_NAME,
                std_root,
            ));
        }
        crates
    }
}

/// The `FileId` of the shipped `std` crate root (`std/mod.fln`), if the std
/// sources are in the map.
fn std_crate_root(sources: &SourceMap) -> Option<FileId> {
    (0..sources.len() as u32)
        .map(FileId)
        .find(|&id| sources.get(id).name == STD_ROOT_NAME)
}

/// File name of the shipped std crate root inside the embedded source map.
const STD_ROOT_NAME: &str = "std/mod.fln";

/// Whether any parsed file imports the `std.*` root (0.0.3 V06, D8). Only
/// programs that do need the shipped std sources appended — everything else
/// keeps the byte-identical single-file chain.
fn uses_std(asts: &[parser::ast::Ast]) -> bool {
    asts.iter().any(|ast| {
        resolver::modules::imports_of_ast(ast)
            .iter()
            .any(|imp| imp.path.first().is_some_and(|p| p == "std"))
    })
}

/// Clone `sources` and append the shipped std modules. The user files keep
/// their existing `FileId`s and std files follow them, so the root `FileId`
/// is unchanged and the source map stays contiguous.
fn with_std_sources(sources: &source_map::SourceMap) -> source_map::SourceMap {
    let mut out = sources.clone();
    for (name, source) in std_src::files() {
        out.add_file(name, source);
    }
    out
}

/// Lex + parse every file in the source map into `Ast`s indexed by `FileId`.
fn parse_all(sources: &source_map::SourceMap) -> Result<Vec<parser::ast::Ast>, CompileError> {
    let mut asts = Vec::with_capacity(sources.len());
    for id in 0..sources.len() as u32 {
        let file = source_map::FileId(id);
        let source = &sources.get(file).source;
        let tokens = lexer::tokenize_in_file(file, source).map_err(CompileError::Lex)?;
        let ast = parser::parse(tokens).map_err(CompileError::Parse)?;
        asts.push(ast);
    }
    Ok(asts)
}

/// Compile an already-parsed whole program (0.0.3 V05) — the shared tail of
/// `Session::compile_project` and the V06 std path of `Session::compile`.
/// `asts` are indexed by `FileId`, matching `sources` (std files appended
/// last when the program imports `std.*`). `crates` lists the root crate
/// first, then any dependency crates and the shipped `std`.
fn compile_pre_parsed(
    sources: &source_map::SourceMap,
    root: source_map::FileId,
    crates: &[CrateDef],
    asts: Vec<parser::ast::Ast>,
) -> Result<codegen::Bytecode, CompileError> {
    let table = build_module_table(crates, sources, &asts).map_err(CompileError::Resolve)?;
    let mut hirs = Vec::with_capacity(table.modules.len());
    let mut binding_offset = 0u32;
    let mut hir_offset = 0u32;
    for (mid, module) in table.modules.iter().enumerate() {
        let ast = asts[module.file.0 as usize].clone();
        let (hir, next_binding, next_hir) =
            resolve_module_with_table(ast, &table, mid, binding_offset, hir_offset)
                .map_err(CompileError::Resolve)?;
        binding_offset = next_binding;
        hir_offset = next_hir;
        hirs.push(hir);
    }

    let mut items = Vec::new();
    let mut structs = Vec::new();
    let span = asts[root.0 as usize].span;
    for hir in hirs {
        items.extend(hir.items);
        structs.extend(hir.structs);
    }
    let merged = resolver::hir::Hir {
        items,
        structs,
        span,
    };
    let typed = typeck::typeck(merged).map_err(CompileError::Typeck)?;
    let mir = lower::lower(typed).map_err(CompileError::Lower)?;
    codegen::codegen(mir).map_err(CompileError::Codegen)
}

#[cfg(test)]
mod tests;
