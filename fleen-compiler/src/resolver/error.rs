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
        }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error: {}", self.kind)
    }
}
