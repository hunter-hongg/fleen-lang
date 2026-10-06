//! Type unification for the type checker.
//!
//! In 0.0.1, unification is simple: types must match exactly.
//! No implicit conversions, no generics, no subtyping.

use crate::typeck::error::{TypeckError, TypeckErrorKind};
use crate::typeck::typed_hir::Type;

/// Unify two types, returning an error if they don't match.
///
/// In 0.0.1, this is a simple equality check. Future versions may
/// support implicit conversions (e.g., Int -> Float).
pub fn unify(expected: &Type, found: &Type, span: crate::lexer::Span) -> Result<(), TypeckError> {
    if expected == found {
        Ok(())
    } else {
        Err(TypeckError::new(
            TypeckErrorKind::TypeMismatch {
                expected: expected.clone(),
                found: found.clone(),
            },
            span,
        ))
    }
}

/// Unify two types for assignment (allows some flexibility).
///
/// In 0.0.1, assignment requires exact type match.
pub fn unify_assign(
    expected: &Type,
    found: &Type,
    span: crate::lexer::Span,
) -> Result<(), TypeckError> {
    if expected == found {
        Ok(())
    } else {
        Err(TypeckError::new(
            TypeckErrorKind::AssignTypeMismatch {
                expected: expected.clone(),
                found: found.clone(),
            },
            span,
        ))
    }
}

/// Unify argument types for a call, reporting `ArgTypeMismatch`.
pub fn unify_arg(
    expected: &Type,
    found: &Type,
    index: usize,
    span: crate::lexer::Span,
) -> Result<(), TypeckError> {
    if expected == found {
        Ok(())
    } else {
        Err(TypeckError::new(
            TypeckErrorKind::ArgTypeMismatch {
                index,
                expected: expected.clone(),
                found: found.clone(),
            },
            span,
        ))
    }
}

/// Check if a type is a function type and return its signature.
pub fn as_func_type(ty: &Type) -> Option<(&Vec<Type>, &Type)> {
    match ty {
        Type::Func(params, ret) => Some((params, ret)),
        _ => None,
    }
}

/// Check if a type is a Result type and return its variants.
pub fn as_result_type(ty: &Type) -> Option<(&Type, &Type)> {
    match ty {
        Type::Result(ok, err) => Some((ok, err)),
        _ => None,
    }
}
