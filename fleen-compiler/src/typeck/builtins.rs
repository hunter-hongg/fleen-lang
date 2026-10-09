//! Builtin function signatures for the type checker.
//!
//! Replaces the 0.0.1 `print` special case in `infer.rs` (DESIGN.md §18
//! known hack). Each builtin carries its real signature here; calls are
//! checked against this table (`PLAN.md` §6).

use crate::typeck::typed_hir::Type;

/// Parameter shape of a builtin.
pub enum BuiltinParam {
    /// Single fixed parameter sequence (for future non-variadic builtins).
    Fixed(&'static [Type]),
    /// Variadic: any number of parameters, each in the given set.
    Variadic(&'static [Type]),
}

/// Signature of a builtin function.
pub struct BuiltinSig {
    pub name: &'static str,
    pub param: BuiltinParam,
    pub ret: Type,
}

/// The builtin function table.
///
/// `print`: `(string...) -> unit` — 0.0.2 strict string check (PLAN §6).
/// Non-string arguments report `ArgTypeMismatch` with help `x as string`
/// (F8, U13). Arity is unrestricted (`print()` prints an empty line).
pub const BUILTINS: &[BuiltinSig] = &[BuiltinSig {
    name: "print",
    param: BuiltinParam::Variadic(&[Type::String]),
    ret: Type::Unit,
}];

/// Look up a builtin by name.
pub fn find_builtin(name: &str) -> Option<&'static BuiltinSig> {
    BUILTINS.iter().find(|sig| sig.name == name)
}
