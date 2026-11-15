//! Typeck module: type inference and checking.
//!
//! Transforms HIR → TypedHir with type information attached to every node.
//!
//! Key semantics (per `DESIGN.md` and `TICKETS.md`):
//! - **Type inference**: literals have known types, identifiers lookup their binding type
//! - **Type checking**: binary operators require matching types, conditions must be Bool
//! - **Choose exhaustiveness**: must have `otherwise` or cover all cases (Bool, Result)
//! - **Function calls**: argument types must match parameter types

pub mod builtins;
pub mod error;
pub mod infer;
pub mod ownership;
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
/// Runs type inference, then the flow-sensitive ownership pass (0.0.2 U05),
/// which fills in every `Ident`'s `Access` and reports move errors.
///
/// # Arguments
/// - `hir`: The HIR from the resolver.
///
/// # Returns
/// - `Ok(TypedHir)`: Type checking successful, all types inferred.
/// - `Err(Vec<TypeckError>)`: Type errors (collected, not fail-fast).
pub fn typeck(hir: Hir) -> Result<TypedHir, Vec<TypeckError>> {
    typeck_with_features(hir, &crate::FeatureTable::empty())
}

/// Type check a HIR program with the program's per-file `#feature` table
/// (0.0.8 T05, D1/D5): the flag is consulted per call site through the
/// checked node's `Span.file`, so a flag-on file's partial application and
/// a flag-off file's exact `ArityMismatch` coexist in one merged HIR.
///
/// # Arguments
/// - `hir`: The HIR from the resolver.
/// - `features`: The `FileId → FeatureSet` table from the parse results.
///
/// # Returns
/// - `Ok(TypedHir)`: Type checking successful, all types inferred.
/// - `Err(Vec<TypeckError>)`: Type errors (collected, not fail-fast).
pub fn typeck_with_features(
    hir: Hir,
    features: &crate::FeatureTable,
) -> Result<TypedHir, Vec<TypeckError>> {
    let mut typed = TypeChecker::with_features(features).typeck(hir)?;
    let ownership_errors = ownership::check_ownership(&mut typed);
    if !ownership_errors.is_empty() {
        return Err(ownership_errors);
    }
    Ok(typed)
}
