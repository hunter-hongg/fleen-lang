//! Additional type checks for the type checker.
//!
//! This module contains checks that go beyond simple type inference,
//! such as choose exhaustiveness and Result handling.

use crate::lexer::Span;
use crate::typeck::error::{TypeckError, TypeckErrorKind};
use crate::typeck::typed_hir::*;
use crate::typeck::unify::as_result_type;

/// Check if a choose expression is exhaustive.
///
/// Returns `Ok(())` if exhaustive, `Err` otherwise.
pub fn check_choose_exhaustiveness(
    scrutinee_type: &Type,
    arms: &[TypedChooseArmHir],
    span: Span,
) -> Result<(), TypeckError> {
    // Check for otherwise (parsed as wildcard pattern `_`)
    let has_otherwise = arms.iter().any(|arm| {
        matches!(&arm.pattern, TypedPatternHir::Ident { name, .. } if name == "_" || name == "otherwise")
    });

    if has_otherwise {
        return Ok(());
    }

    // Check for Bool exhaustiveness
    if matches!(scrutinee_type, Type::Bool) {
        let has_true = arms.iter().any(|arm| {
            matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Bool(true, _)))
        });
        let has_false = arms.iter().any(|arm| {
            matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Bool(false, _)))
        });

        if !has_true || !has_false {
            let mut missing = Vec::new();
            if !has_true {
                missing.push("true".to_string());
            }
            if !has_false {
                missing.push("false".to_string());
            }
            return Err(TypeckError::new(
                TypeckErrorKind::ChooseNotExhaustive {
                    scrutinee_type: scrutinee_type.clone(),
                    missing_patterns: missing,
                },
                span,
            ));
        }
    }

    // Check for Result exhaustiveness
    if as_result_type(scrutinee_type).is_some() {
        let has_ok = arms.iter().any(|arm| {
            matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Call(..)))
        });
        let has_err = arms.iter().any(|arm| {
            matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Call(..)))
        });

        if !has_ok || !has_err {
            let mut missing = Vec::new();
            if !has_ok {
                missing.push("Ok".to_string());
            }
            if !has_err {
                missing.push("Err".to_string());
            }
            return Err(TypeckError::new(
                TypeckErrorKind::ChooseNotExhaustive {
                    scrutinee_type: scrutinee_type.clone(),
                    missing_patterns: missing,
                },
                span,
            ));
        }
    }

    Ok(())
}

/// Check if a Result type is properly handled.
///
/// In 0.0.1, this is a warning, not an error.
pub fn check_result_handling(
    result_type: &Type,
    arms: &[TypedChooseArmHir],
    span: Span,
) -> Result<(), TypeckError> {
    if as_result_type(result_type).is_some() {
        let has_ok = arms.iter().any(|arm| {
            matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Call(..)))
        });
        let has_err = arms.iter().any(|arm| {
            matches!(&arm.pattern, TypedPatternHir::Literal(expr) if matches!(**expr, TypedExprHir::Call(..)))
        });

        if !has_ok || !has_err {
            return Err(TypeckError::new(
                TypeckErrorKind::ResultHandlingRequired,
                span,
            ));
        }
    }

    Ok(())
}

/// Check if a type is supported for codegen.
pub fn check_supported_type(ty: &Type, span: Span) -> Result<(), TypeckError> {
    match ty {
        Type::Array(_) | Type::Box(_) | Type::Ref(_) => Err(TypeckError::new(
            TypeckErrorKind::UnsupportedType { ty: ty.clone() },
            span,
        )),
        _ => Ok(()),
    }
}
