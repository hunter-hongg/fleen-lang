//! Builtin function signatures for the type checker (0.0.3 V06).
//!
//! Each builtin carries its real signature here; calls are checked against
//! this table by `typeck/infer.rs`. Builtins fall into three classes:
//!
//! - **Fixed** (`std.os.exit`, ...): a fixed parameter sequence.
//! - **Variadic** (`print`, `std.io.println`): any number of arguments,
//!   each in a fixed set.
//! - **Schematic** (`std.arr.len`, ...): a single type variable `T0` is
//!   created per call site, unified against the argument types, then
//!   substituted into the return type. Type variables appear ONLY in this
//!   table — never in user-visible syntax (PLAN §10 risk rule), and never
//!   in TypedHir: every substitution happens at the call site.
//!
//! Names of std-provided builtins are the fully-qualified module paths
//! (`std.io.println`), so a user reaches them through `import std.io;` +
//! `std.io.println(...)` (V05 mangling). The prelude (`print`) and the two
//! `Range` constructors (`range`/`range_from`, D5) keep their bare global
//! names. Functions that Fleen itself can express (`std.num.abs`,
//! `std.str.from_int`, ...) are *not* here — they live in the `std/` Fleen
//! sources and typecheck as ordinary user functions (D8).

use crate::typeck::typed_hir::Type;

/// Parameter shape of a builtin.
#[derive(Clone)]
pub enum BuiltinParam {
    /// Fixed parameter sequence.
    Fixed(Vec<Type>),
    /// Variadic: any number of parameters, each in the given set.
    Variadic(Vec<Type>),
    /// Schematic: a single type variable `T0` appears in the param/return
    /// types. On each call, `T0` is unified with the argument type and
    /// substituted in the return type (0.0.3 V06).
    Schematic { params: Vec<Type>, ret: Type },
}

/// How a builtin call reaches the runtime.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BuiltinLowering {
    /// The VM implements it by name (`exec_builtin`).
    Intrinsic,
    /// A recognized `Range` constructor (D5): the call lowers directly to
    /// `MakeRange` — no runtime call at all (the `Ok`/`Err` mechanism
    /// extended from constructors to a table entry).
    MakeRange,
}

/// Signature of a builtin function.
#[derive(Clone)]
pub struct BuiltinSig {
    pub name: &'static str,
    pub param: BuiltinParam,
    pub ret: Type,
    pub lowering: BuiltinLowering,
}

/// The builtin function table.
///
/// 0.0.2: `print` was the only entry — `(string...) -> unit`.
/// 0.0.3 V06 adds VM intrinsics and schematic std helpers.
///
/// Intrinsics (VM-implemented, not Fleen source), by qualified module path:
/// - `std.io.println`: `(string...) -> unit` — print + trailing newline
/// - `std.io.read_line`: `() -> string` — read one line from stdin
/// - `std.str.len`: `(string) -> int`
/// - `std.str.concat`: `(string, string) -> string`
/// - `std.str.split`: `(string, string) -> [string]` — split on a separator
/// - `std.num.sqrt`: `(float) -> float` — square root
/// - `std.num.floor`: `(float) -> int` — floor to integer
/// - `std.os.args`: `() -> [string]` — command-line arguments (without argv[0])
/// - `std.os.exit`: `(int) -> unit` — terminate with the given code
/// - `std.time.now_millis`: `() -> int` — wall-clock milliseconds
///
/// Schematic (type-variable) std helpers — resolved at call site:
/// - `std.arr.len`: `[T] -> int`
/// - `std.arr.get`: `[T], int -> Result<T, string>`
/// - `std.arr.set`: `[T], int, T -> [T]`  (returns a new array)
/// - `std.arr.push`: `[T], T -> [T]`  (returns a new array)
///
/// `print`: `(string...) -> unit` — 0.0.2 strict string check (PLAN §6).
/// Non-string arguments report `ArgTypeMismatch` with help `x as string`
/// (F8, U13). Arity is unrestricted (`print()` prints an empty line).
///
/// `range`/`range_from` (D5): `Range` values are constructed only through
/// these recognized names (also reachable qualified as
/// `std.collection.range.range`/`..._from`); both lower to `MakeRange`.
pub fn builtins() -> Vec<BuiltinSig> {
    let string = || Type::String;
    let int = || Type::Int;
    let float = || Type::Float;
    let unit = || Type::Unit;
    let range = || Type::Range;
    let strings = || Type::Array(Box::new(string()));
    let t0 = || Type::Var(0);
    let arr_t0 = || Type::Array(Box::new(t0()));
    vec![
        // The prelude (`print`): bare name, visible without any import.
        BuiltinSig {
            name: "print",
            param: BuiltinParam::Variadic(vec![string()]),
            ret: unit(),
            lowering: BuiltinLowering::Intrinsic,
        },
        // std.io — VM I/O intrinsics.
        BuiltinSig {
            name: "std.io.println",
            param: BuiltinParam::Variadic(vec![string()]),
            ret: unit(),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.io.read_line",
            param: BuiltinParam::Fixed(vec![]),
            ret: string(),
            lowering: BuiltinLowering::Intrinsic,
        },
        // std.str — no Fleen syntax for these (PLAN §3.5.2: intrinsic).
        BuiltinSig {
            name: "std.str.len",
            param: BuiltinParam::Fixed(vec![string()]),
            ret: int(),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.str.concat",
            param: BuiltinParam::Fixed(vec![string(), string()]),
            ret: string(),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.str.split",
            param: BuiltinParam::Fixed(vec![string(), string()]),
            ret: strings(),
            lowering: BuiltinLowering::Intrinsic,
        },
        // std.num — math intrinsics (`abs`/`min`/`max` are Fleen source).
        BuiltinSig {
            name: "std.num.sqrt",
            param: BuiltinParam::Fixed(vec![float()]),
            ret: float(),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.num.floor",
            param: BuiltinParam::Fixed(vec![float()]),
            ret: int(),
            lowering: BuiltinLowering::Intrinsic,
        },
        // std.os / std.time.
        BuiltinSig {
            name: "std.os.args",
            param: BuiltinParam::Fixed(vec![]),
            ret: strings(),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.os.exit",
            param: BuiltinParam::Fixed(vec![int()]),
            ret: unit(),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.time.now_millis",
            param: BuiltinParam::Fixed(vec![]),
            ret: int(),
            lowering: BuiltinLowering::Intrinsic,
        },
        // std.arr — schematic `[T]` helpers (the bounded polymorphism of
        // 0.0.3; see the module doc comment).
        BuiltinSig {
            name: "std.arr.len",
            param: BuiltinParam::Schematic {
                params: vec![arr_t0()],
                ret: int(),
            },
            ret: int(),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.arr.get",
            param: BuiltinParam::Schematic {
                params: vec![arr_t0(), int()],
                ret: Type::Result(Box::new(t0()), Box::new(string())),
            },
            ret: Type::Result(Box::new(t0()), Box::new(string())),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.arr.set",
            param: BuiltinParam::Schematic {
                params: vec![arr_t0(), int(), t0()],
                ret: arr_t0(),
            },
            ret: arr_t0(),
            lowering: BuiltinLowering::Intrinsic,
        },
        BuiltinSig {
            name: "std.arr.push",
            param: BuiltinParam::Schematic {
                params: vec![arr_t0(), t0()],
                ret: arr_t0(),
            },
            ret: arr_t0(),
            lowering: BuiltinLowering::Intrinsic,
        },
        // Range constructors (D5) — bare global names, plus the qualified
        // module spellings; both lower to `MakeRange`.
        BuiltinSig {
            name: "range",
            param: BuiltinParam::Fixed(vec![int()]),
            ret: range(),
            lowering: BuiltinLowering::MakeRange,
        },
        BuiltinSig {
            name: "range_from",
            param: BuiltinParam::Fixed(vec![int(), int()]),
            ret: range(),
            lowering: BuiltinLowering::MakeRange,
        },
        BuiltinSig {
            name: "std.collection.range.range",
            param: BuiltinParam::Fixed(vec![int()]),
            ret: range(),
            lowering: BuiltinLowering::MakeRange,
        },
        BuiltinSig {
            name: "std.collection.range.range_from",
            param: BuiltinParam::Fixed(vec![int(), int()]),
            ret: range(),
            lowering: BuiltinLowering::MakeRange,
        },
    ]
}

/// Look up a builtin by name.
pub fn find_builtin(name: &str) -> Option<BuiltinSig> {
    builtins().into_iter().find(|sig| sig.name == name)
}

/// A resolved schematic call: the type variable `T0` has been unified
/// against concrete argument types and substituted into the return type.
pub struct ResolvedCall {
    pub param_types: Vec<Type>,
    pub ret_type: Type,
    pub is_schematic: bool,
    /// The first conflicting binding of a type variable, if any: a
    /// schematic variable was bound to two different concrete types by two
    /// arguments (e.g. `std.arr.push(["a"], 1)` binds `T0` to `string`
    /// through the array and to `int` through the value). Carried out of
    /// the table so typeck can report it against the offending argument.
    pub conflict: Option<VarConflict>,
}

/// A type variable bound to two different types at one call site.
pub struct VarConflict {
    /// The expected type — the binding recorded first.
    pub expected: Type,
    /// The conflicting type — the later binding.
    pub found: Type,
    /// Index of the argument that carried the conflicting type.
    pub arg_index: usize,
}

/// Resolve a builtin call at a given name and argument types.
///
/// For schematic builtins, type variables (`Var(n)`) in the signature are
/// replaced with the concrete types inferred from this call site's
/// arguments. The substitution is one-level: `Var(0) -> T` where `T` is
/// the concrete type of the array element (or of the matching argument).
/// Beyond one-call unification of table variables is 0.1.0 generics scope.
///
/// Returns the concrete parameter types and return type, with all
/// `Var`s substituted. `None` only on an arity mismatch (Fixed).
pub fn resolve_schematic_call(sig: &BuiltinSig, arg_types: &[Type]) -> Option<ResolvedCall> {
    match &sig.param {
        BuiltinParam::Schematic { params, ret } => {
            if params.len() != arg_types.len() {
                return None;
            }
            // Collect type variable bindings from argument types; the first
            // binding of a variable wins, later conflicts are recorded.
            let mut subst: Vec<(u32, Type)> = Vec::new();
            let mut conflict: Option<VarConflict> = None;
            for (i, (param, found)) in params.iter().zip(arg_types.iter()).enumerate() {
                collect_bindings(param, found, i, &mut subst, &mut conflict);
            }
            let param_types: Vec<Type> = params.iter().map(|t| substitute(t, &subst)).collect();
            let ret_type = substitute(ret, &subst);
            Some(ResolvedCall {
                param_types,
                ret_type,
                is_schematic: true,
                conflict,
            })
        }
        BuiltinParam::Fixed(params) => {
            if params.len() == arg_types.len() {
                Some(ResolvedCall {
                    param_types: params.clone(),
                    ret_type: sig.ret.clone(),
                    is_schematic: false,
                    conflict: None,
                })
            } else {
                None
            }
        }
        BuiltinParam::Variadic(_) => Some(ResolvedCall {
            param_types: Vec::new(),
            ret_type: sig.ret.clone(),
            is_schematic: false,
            conflict: None,
        }),
    }
}

/// Walk `schema` (a builtin param type containing `Var` placeholders) and
/// `concrete` (the actual argument type), recording bindings for each `Var`
/// encountered. A `Var(n)` in an `Array(Var(n))` param binds `n` to the
/// element type of the concrete `Array(concrete_elem)`.
fn collect_bindings(
    schema: &Type,
    concrete: &Type,
    arg_index: usize,
    subst: &mut Vec<(u32, Type)>,
    conflict: &mut Option<VarConflict>,
) {
    match (schema, concrete) {
        (Type::Var(n), _) => {
            match subst.iter().find(|(id, _)| id == n) {
                // Already bound: a different second binding is a conflict
                // (`push(["a"], 1)`); the same type again is fine.
                Some((_, existing)) if existing != concrete => {
                    if conflict.is_none() {
                        *conflict = Some(VarConflict {
                            expected: existing.clone(),
                            found: concrete.clone(),
                            arg_index,
                        });
                    }
                }
                Some(_) => {}
                None => subst.push((*n, concrete.clone())),
            }
        }
        (Type::Array(schema_elem), Type::Array(concrete_elem)) => {
            collect_bindings(schema_elem, concrete_elem, arg_index, subst, conflict);
        }
        (Type::Result(schema_ok, schema_err), Type::Result(c_ok, c_err)) => {
            collect_bindings(schema_ok, c_ok, arg_index, subst, conflict);
            collect_bindings(schema_err, c_err, arg_index, subst, conflict);
        }
        (Type::Func(schema_params, schema_ret), Type::Func(c_params, c_ret)) => {
            for (sp, cp) in schema_params.iter().zip(c_params.iter()) {
                collect_bindings(sp, cp, arg_index, subst, conflict);
            }
            collect_bindings(schema_ret, c_ret, arg_index, subst, conflict);
        }
        _ => {}
    }
}

/// Substitute all `Var(n)` occurrences in `ty` using `subst`.
fn substitute(ty: &Type, subst: &[(u32, Type)]) -> Type {
    match ty {
        Type::Var(n) => subst
            .iter()
            .find(|(id, _)| *id == *n)
            .map(|(_, t)| t.clone())
            .unwrap_or_else(|| Type::Var(*n)),
        Type::Func(params, ret) => Type::Func(
            params.iter().map(|t| substitute(t, subst)).collect(),
            Box::new(substitute(ret, subst)),
        ),
        Type::Result(ok, err) => Type::Result(
            Box::new(substitute(ok, subst)),
            Box::new(substitute(err, subst)),
        ),
        Type::Array(elem) => Type::Array(Box::new(substitute(elem, subst))),
        Type::Box(inner) => Type::Box(Box::new(substitute(inner, subst))),
        Type::Ref(inner) => Type::Ref(Box::new(substitute(inner, subst))),
        Type::Struct(info) => Type::Struct(info.clone()),
        Type::Int
        | Type::Float
        | Type::Bool
        | Type::String
        | Type::Unit
        | Type::Range
        | Type::Unsupported(_) => ty.clone(),
    }
}
