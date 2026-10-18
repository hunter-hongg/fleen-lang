//! Resolver error types.

use crate::lexer::Span;
use std::fmt;

/// A single resolution error with span and kind.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolveError {
    pub kind: ResolveErrorKind,
    pub span: Span,
}

impl ResolveError {
    pub fn new(kind: ResolveErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Kinds of resolution errors.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolveErrorKind {
    /// Variable used before declaration (or not visible from this scope).
    UndeclaredVariable { name: String },
    /// Assignment to an immutable variable or const.
    AssignToImmutable { name: String },
    /// Assignment target is not a simple identifier (e.g., `f() = 1`).
    InvalidAssignmentTarget,
    /// 0.0.3 V05: a qualified path reached a private item in another module.
    PrivateAccess { item: String, module: String },
    /// 0.0.3 V05: an `import` path named a module with no matching file.
    ModuleNotFound { path: String, tried: Vec<String> },
    /// 0.0.3: two files resolved to the same absolute module path. With a
    /// dependency graph this means two crates contributed a module of the
    /// same name, or a crate was registered twice.
    DuplicateModule {
        path: String,
        /// The file that lost the collision.
        file: String,
        /// The file that claimed the path first.
        first: String,
    },
    /// 0.0.3: a module imported a path that resolves inside another crate —
    /// a dependency reaching into the depending project, or the reverse.
    CrossCrateImport {
        path: String,
        /// Name of the crate doing the importing (empty = the root crate).
        crate_name: String,
        /// Name of the crate that actually owns the module.
        owner: String,
    },
    /// 0.0.3 V05: the import graph contains a cycle.
    CircularImport { cycle: Vec<String> },
    /// Shadowing with mismatched mutability (DESIGN.md §4.3).
    ShadowingMutabilityMismatch {
        outer_mutable: bool,
        inner_mutable: bool,
    },
    /// Type annotation on an assignment (only first bindings may annotate,
    /// DESIGN.md §3.5).
    TypeAnnotationOnAssignment,
    /// A name is already bound in the same scope (DESIGN.md §3.5: `const`
    /// re-binding, duplicate `func`, duplicate parameters, ...).
    /// Carries the first declaration's span for diagnostics.
    DuplicateBinding { name: String, first_span: Span },
    /// `ref T` used in a type position other than a function parameter
    /// (0.0.2 U05, DESIGN §10.4): locals, globals, return types, nested
    /// `ref ref T`, or inside composite types. `context` names the offending
    /// position; the error's span points at the type annotation.
    RefNotAllowedHere { context: &'static str },
    /// A new binding re-declares the name of a `ref` parameter in scope
    /// (0.0.2 U05, DESIGN §10.4): borrows are read-only, so `s = …` at a
    /// ref parameter would silently shadow the borrow instead of writing
    /// through it — flagged rather than shadowed.
    AssignToRefParam { name: String },
}

impl fmt::Display for ResolveErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResolveErrorKind::UndeclaredVariable { name } => {
                write!(f, "undeclared variable `{}`", name)
            }
            ResolveErrorKind::AssignToImmutable { name } => {
                write!(f, "cannot assign to immutable variable `{}`", name)
            }
            ResolveErrorKind::InvalidAssignmentTarget => {
                write!(f, "invalid assignment target")
            }
            ResolveErrorKind::PrivateAccess { item, module } => {
                write!(
                    f,
                    "`{item}` is private to module `{module}` (mark it `pub` to expose it)"
                )
            }
            ResolveErrorKind::ModuleNotFound { path, tried } => {
                write!(f, "module not found: {path}")?;
                if !tried.is_empty() {
                    write!(f, " (tried {})", tried.join(", "))?;
                }
                Ok(())
            }
            ResolveErrorKind::DuplicateModule { path, file, first } => {
                write!(
                    f,
                    "duplicate module `{path}`: `{file}` collides with `{first}`"
                )
            }
            ResolveErrorKind::CrossCrateImport {
                path,
                crate_name,
                owner,
            } => {
                let importer = if crate_name.is_empty() {
                    "this crate".to_string()
                } else {
                    format!("crate `{crate_name}`")
                };
                let owning = if owner.is_empty() {
                    "this crate".to_string()
                } else {
                    format!("crate `{owner}`")
                };
                write!(
                    f,
                    "{importer} cannot import `{path}`: {owning} already depends on \
                     {importer}, so the dependency would be circular"
                )
            }
            ResolveErrorKind::CircularImport { cycle } => {
                write!(f, "circular import: {}", cycle.join(" → "))
            }
            ResolveErrorKind::ShadowingMutabilityMismatch {
                outer_mutable,
                inner_mutable,
            } => {
                write!(
                    f,
                    "shadowing must preserve mutability: outer is {}, inner is {}",
                    if *outer_mutable {
                        "mutable"
                    } else {
                        "immutable"
                    },
                    if *inner_mutable {
                        "mutable"
                    } else {
                        "immutable"
                    }
                )
            }
            ResolveErrorKind::TypeAnnotationOnAssignment => {
                write!(f, "type annotation not allowed on assignment")
            }
            ResolveErrorKind::DuplicateBinding { name, .. } => {
                write!(f, "duplicate declaration of `{}` in same scope", name)
            }
            ResolveErrorKind::RefNotAllowedHere { context } => {
                write!(
                    f,
                    "`ref` type is not allowed in {context}: borrows are \
                     only accepted as function parameters (`func f(x: ref T)`, \
                     0.0.2); locals, globals and return types must own their \
                     values"
                )
            }
            ResolveErrorKind::AssignToRefParam { name } => {
                write!(
                    f,
                    "cannot assign to `ref` parameter `{name}` — a borrow is \
                     read-only; help: the `ref` parameter shadows the caller's \
                     value, rebind a fresh local with a different name instead"
                )
            }
        }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error: {}", self.kind)
    }
}
