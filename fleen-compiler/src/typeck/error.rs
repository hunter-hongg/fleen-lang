//! Typeck error types.

use crate::lexer::Span;
use crate::typeck::typed_hir::Type;
use std::fmt;

/// A single type error with span and kind.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeckError {
    pub kind: TypeckErrorKind,
    pub span: Span,
}

impl TypeckError {
    pub fn new(kind: TypeckErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Kinds of type errors.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeckErrorKind {
    /// Type mismatch between expected and found types.
    TypeMismatch { expected: Type, found: Type },
    /// Choose expression is not exhaustive.
    ChooseNotExhaustive {
        scrutinee_type: Type,
        missing_patterns: Vec<String>,
    },
    /// Result type must be handled (warning in 0.0.1).
    ResultHandlingRequired,
    /// Unsupported type for codegen.
    UnsupportedType { ty: Type },
    /// Invalid operand type for operator.
    InvalidOperand { op: String, ty: Type },
    /// Function call argument count mismatch.
    ArityMismatch { expected: usize, found: usize },
    /// Function call argument type mismatch.
    ArgTypeMismatch {
        index: usize,
        expected: Type,
        found: Type,
    },
    /// Condition must be Bool.
    ConditionNotBool { found: Type },
    /// Assignment type mismatch.
    AssignTypeMismatch { expected: Type, found: Type },
    /// Variable not found (should not happen after resolver).
    UndefinedVariable { name: String },
}

impl fmt::Display for TypeckErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeckErrorKind::TypeMismatch { expected, found } => {
                write!(
                    f,
                    "type mismatch: expected `{}`, found `{}`",
                    expected.name(),
                    found.name()
                )
            }
            TypeckErrorKind::ChooseNotExhaustive {
                scrutinee_type,
                missing_patterns,
            } => {
                write!(
                    f,
                    "choose is not exhaustive for type `{}`: missing {}",
                    scrutinee_type.name(),
                    missing_patterns.join(", ")
                )
            }
            TypeckErrorKind::ResultHandlingRequired => {
                write!(f, "Result type must be handled")
            }
            TypeckErrorKind::UnsupportedType { ty } => {
                write!(f, "type `{}` is not supported in 0.0.1", ty.name())
            }
            TypeckErrorKind::InvalidOperand { op, ty } => {
                write!(
                    f,
                    "invalid operand type `{}` for operator `{}`",
                    ty.name(),
                    op
                )
            }
            TypeckErrorKind::ArityMismatch { expected, found } => {
                write!(f, "expected {} arguments, found {}", expected, found)
            }
            TypeckErrorKind::ArgTypeMismatch {
                index,
                expected,
                found,
            } => {
                write!(
                    f,
                    "argument {} type mismatch: expected `{}`, found `{}`",
                    index + 1,
                    expected.name(),
                    found.name()
                )
            }
            TypeckErrorKind::ConditionNotBool { found } => {
                write!(f, "condition must be `bool`, found `{}`", found.name())
            }
            TypeckErrorKind::AssignTypeMismatch { expected, found } => {
                write!(
                    f,
                    "cannot assign `{}` to variable of type `{}`",
                    found.name(),
                    expected.name()
                )
            }
            TypeckErrorKind::UndefinedVariable { name } => {
                write!(f, "undefined variable `{}`", name)
            }
        }
    }
}

impl fmt::Display for TypeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error: {}", self.kind)
    }
}
