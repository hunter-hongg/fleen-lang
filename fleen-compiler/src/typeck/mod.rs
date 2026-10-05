//! Typeck module: type inference and checking.
//!
//! Transforms HIR → TypedHir with type information attached to every node.
//!
//! Key semantics (per `DESIGN.md` and `TICKETS.md`):
//! - **Type inference**: literals have known types, identifiers lookup their binding type
//! - **Type checking**: binary operators require matching types, conditions must be Bool
//! - **Choose exhaustiveness**: must have `otherwise` or cover all cases (Bool, Result)
//! - **Function calls**: argument types must match parameter types

pub mod check;
pub mod error;
pub mod infer;
pub mod typed_hir;
pub mod unify;

#[cfg(test)]
mod tests;

use crate::resolver::hir::Hir;
use error::TypeckError;
use infer::TypeChecker;
use typed_hir::TypedHir;

/// Type check a HIR program, producing a Typed HIR.
///
/// # Arguments
/// - `hir`: The HIR from the resolver.
///
/// # Returns
/// - `Ok(TypedHir)`: Type checking successful, all types inferred.
/// - `Err(Vec<TypeckError>)`: Type errors (collected, not fail-fast).
pub fn typeck(hir: Hir) -> Result<TypedHir, Vec<TypeckError>> {
    TypeChecker::new().typeck(hir)
}
