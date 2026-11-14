//! Typed HIR: HIR with type information attached.
//!
//! Every expression node in TypedHir carries its inferred `Type`.
//! The type checker produces this structure, which is then consumed
//! by the lowering and codegen stages.

use crate::lexer::Span;
use crate::parser::ast::ResultCtor;
use crate::resolver::hir::*;

/// Unique ID of a struct declaration (0.0.3 V02). Allocated contiguously
/// from 0 in declaration order; feeds `StructNew(id)` lowering and the
/// bytecode `struct_defs` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

/// Unique ID of an enum declaration (0.0.6 T04). Allocated contiguously
/// from 0 in declaration order; feeds `EnumNew(id)` lowering and the
/// bytecode `enum_defs` table (T06).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnumId(pub u32);

/// Shared per-declaration enum data carried inside `Type::Enum`
/// (0.0.6 T04). Mirrors [`StructInfo`]: `id` is what lowering emits;
/// `name` renders error messages without access to the enum table; `copy`
/// is the cached D2 verdict (an enum is Copy iff every variant payload is
/// Copy) so the pure `Type::is_copy` stays total.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumInfo {
    pub id: EnumId,
    pub name: Box<str>,
    pub copy: bool,
    /// Number of declaration-local type parameters (0.0.7 T04/D1): 0 for a
    /// monomorphic enum, which never appears as [`Type::App`].
    pub type_param_count: usize,
    /// Per-variant payload skeletons in declaration order (0.0.7 T04): the
    /// declaration's own payload types with each type parameter represented
    /// by [`Type::Param(i)`](Type::Param). `Type::is_copy` / `is_owned` on a
    /// [`Type::App`] substitute the use-site arguments into these skeletons
    /// and re-run the rule (D2); for a monomorphic enum the skeletons are
    /// simply the concrete payload types.
    pub payloads: Vec<Vec<Type>>,
}

/// Shared per-declaration struct data carried inside `Type::Struct`
/// (0.0.3 V02). `id` is what lowering emits; `name` renders error messages
/// without access to the struct table; `copy` is the cached D2 verdict
/// (a struct is Copy iff every field is Copy) so the pure `Type::is_copy`
/// stays total. Built once by the struct-table pass, in dependency order.
#[derive(Debug, Clone, PartialEq)]
pub struct StructInfo {
    pub id: StructId,
    pub name: Box<str>,
    pub copy: bool,
}

/// A type in the Fleen type system (0.0.2).
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// 64-bit integer.
    Int,
    /// 64-bit float.
    Float,
    /// Boolean.
    Bool,
    /// UTF-8 string.
    String,
    /// Unit type (no value).
    Unit,
    /// Function type: (param_types) -> return_type.
    Func(Vec<Type>, Box<Type>),
    /// Result type: Result[T, E].
    Result(Box<Type>, Box<Type>),
    /// Array type (0.0.3 V03). Always an owned type (D3) — even `[int]`:
    /// array identity is heap state, and a "Copy array" would silently fork
    /// on write. Element reads are bounds-checked copies.
    Array(Box<Type>),
    /// Box type (0.0.2: typeck-supported ownership; lowering lands in U06).
    Box(Box<Type>),
    /// Ref type (0.0.2: typeck-supported read-only borrow; lowering in U06).
    Ref(Box<Type>),
    /// User-defined struct type (0.0.3 V02).
    Struct(Box<StructInfo>),
    /// User-defined enum type (0.0.6 T04). Monomorphic enums (arity 0)
    /// only; a parameterized enum never appears in this bare form at a use
    /// site (D1: use sites must be fully applied).
    Enum(Box<EnumInfo>),
    /// A fully applied parameterized enum type (0.0.7 T04/D1): `Option<int>`
    /// is `App(option_info, [Int])`. The argument count always equals the
    /// declaration's `type_param_count` (checked at every application). The
    /// `EnumInfo` carries the same `EnumId` as the bare form, so lowering
    /// and codegen treat both alike.
    App(Box<EnumInfo>, Vec<Type>),
    /// A declaration-local type parameter (0.0.7 T04/D1): the `T` of
    /// `enum Option<T> { Some(T) }`, identified by declaration order.
    /// Deliberately distinct from [`Type::Var(u32)`](Type::Var) (schematic
    /// std intrinsic variables). Appears only inside `EnumInfo.payloads`
    /// skeletons; use-site substitution removes every `Param` before a type
    /// reaches lowering or codegen.
    Param(usize),
    /// `Range` — builtin iterable type (0.0.3 V04). Constructed via std
    /// (`range`/`range_from`); iterates `start..end` (end-exclusive). Copy.
    Range,
    /// `Future<T>` (0.0.4 F3): a suspendable coroutine frame yielding `T`.
    /// Non-Copy — it owns its suspended state and is driven once by `await`.
    Future(Box<Type>),
    /// `channel<T>` (0.0.4 F9, D3/D6): a message-passing channel.
    /// Non-Copy, move-only; holds a reference to a shared buffer.
    /// The element type `T` is checked at run time as a defensive backstop.
    Channel(Box<Type>),
    /// `ptr<T>` (0.0.5 T02, D1/D2): a raw pointer. Non-Copy, move-only;
    /// produced only by the unsafe op `addr`. Not a storage type — it
    /// cannot sit in a struct field, array element, box cell or global
    /// (`PtrNotStorable`), because every one of those reads is a safe-code
    /// Value-level copy that would hand out a pointer without `unsafe`.
    Ptr(Box<Type>),
    /// Type variable (0.0.3 V06): appears **only** in the schematic
    /// signatures of std intrinsics — never in user-visible syntax. A
    /// fresh `Var(n)` is created per call site, unified against the
    /// argument types, then substituted in the return type.
    Var(u32),
    /// Unsupported type (for features not yet implemented).
    Unsupported(String),
}

impl Type {
    /// Get a human-readable name for this type (for error messages).
    pub fn name(&self) -> String {
        match self {
            Type::Int => "int".to_string(),
            Type::Float => "float".to_string(),
            Type::Bool => "bool".to_string(),
            Type::String => "string".to_string(),
            Type::Unit => "unit".to_string(),
            Type::Func(params, ret) => {
                let param_names: Vec<String> = params.iter().map(|t| t.name()).collect();
                format!("({}) -> {}", param_names.join(", "), ret.name())
            }
            Type::Result(ok, err) => format!("Result[{}, {}]", ok.name(), err.name()),
            Type::Array(elem) => format!("[{}]", elem.name()),
            Type::Box(inner) => format!("box<{}>", inner.name()),
            Type::Ref(inner) => format!("ref {}", inner.name()),
            Type::Struct(info) => info.name.to_string(),
            Type::Enum(info) => info.name.to_string(),
            Type::App(info, args) => format!(
                "{}<{}>",
                info.name,
                args.iter().map(|a| a.name()).collect::<Vec<_>>().join(", ")
            ),
            // Declaration-local parameter: only visible while a payload
            // skeleton is inspected (error rendering, tests); use-site
            // substitution removes it before codegen.
            Type::Param(i) => format!("?P{}", i),
            Type::Range => "Range".to_string(),
            Type::Future(inner) => format!("Future<{}>", inner.name()),
            Type::Channel(inner) => format!("channel<{}>", inner.name()),
            Type::Ptr(inner) => format!("ptr<{}>", inner.name()),
            Type::Var(id) => format!("?T{}", id),
            Type::Unsupported(name) => name.clone(),
        }
    }

    /// Check if this type holds a `Future` anywhere inside a value-carrying
    /// container (0.0.4 T08): an array of futures is legal *storage* —
    /// `std.async.run` consumes it by moving the frames out wholesale — but
    /// every Value-level *copy* of such an array would duplicate the
    /// suspended frames. Every copy route must therefore be rejected
    /// wherever the array sits, including nested arrays. `Result<T, E>` is
    /// walked too: it carries its payloads by value (`Value::Ok`/`Err` box
    /// them and the derived `Clone` deep-copies), so a `Result<Future, _>`
    /// is exactly as fork-prone as the bare future. Struct fields, box
    /// cells and globals cannot hold a future at all (rejected at
    /// declaration, `FutureNotStorable`).
    pub fn contains_future(&self) -> bool {
        contains_in_value_position(self, &ContainsKind::Future)
    }

    /// Check if this type holds a `channel<T>` anywhere inside a
    /// value-carrying container — the mirror of [`Self::contains_future`]
    /// for the channel fork discipline (0.0.4 T07d): a Value-level copy of
    /// a channel (or of a `Result<channel<_>, _>`) would alias the shared
    /// buffer without the one explicit fork point, `std.concurrent.split`.
    /// A bare `channel<T>` inside an array is rejected at the array literal
    /// (`ChannelNotStorable` — channels have no wholesale array consumer),
    /// and struct fields, box cells and globals are rejected at declaration.
    pub fn contains_channel(&self) -> bool {
        contains_in_value_position(self, &ContainsKind::Channel)
    }

    /// Check if this type holds a `ptr<T>` anywhere inside a value-carrying
    /// container — the mirror of [`Self::contains_future`] /
    /// [`Self::contains_channel`] for the pointer discipline (0.0.5 T02,
    /// D1/D2). A Value-level copy of a pointer hands out aliasing access in
    /// *safe* code, which would break D2's "the entire pointer-exists state
    /// sits behind `unsafe`": creating a pointer is an unsafe op, so every
    /// safe copy route must be sealed. `ptr<T>` is rejected as a struct
    /// field, array element, box cell and global at declaration
    /// (`PtrNotStorable`); `clone` of anything containing a pointer is
    /// rejected (`CloneOfPointer`), and `Result<ptr, _>` payloads ride the
    /// same rejection through this walk.
    ///
    /// The walk deliberately stops at opaque handles (`Channel` / `Future`),
    /// matching `contains_future` / `contains_channel`: a handle does not hold
    /// its payload by value for copy purposes. A `ptr` that would cross a
    /// channel is rejected at the `send` site instead (`PtrAcrossBoundary`,
    /// 0.0.5 T04/D4) — a `channel<ptr<_>>` handle itself never carries a
    /// pointer into the async world, so it is not flagged by this walk.
    pub fn contains_ptr(&self) -> bool {
        contains_in_value_position(self, &ContainsKind::Ptr)
    }

    /// The declaration's [`EnumInfo`] behind either enum-shaped type
    /// (0.0.7 T04): `Enum(info)` for a monomorphic enum, `App(info, args)`
    /// for a fully applied parameterized one. `None` for every other type.
    ///
    /// Every enum rule (copy verdict, variant lookup, exhaustiveness,
    /// lowering's scrutinee check) is about the *declaration*, which both
    /// forms carry identically — the use-site arguments never change it.
    /// Routing those sites through one accessor keeps the two forms from
    /// drifting apart, and keeps the arms that must see the arguments
    /// (substitution, arity) visibly separate.
    pub fn as_enum(&self) -> Option<&EnumInfo> {
        match self {
            Type::Enum(info) | Type::App(info, _) => Some(info),
            _ => None,
        }
    }

    /// Whether this type is a parameterized enum applied to at least one
    /// use-site argument (0.0.7 T04/D1).
    pub fn is_enum_app(&self) -> bool {
        matches!(self, Type::App(_, _))
    }

    /// Check if this type is a numeric type (Int or Float).
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }

    /// Check if this type is a Copy type (0.0.2, DESIGN §10.1):
    /// `int`, `float`, `bool`, `unit`, function values, and `Result<T, E>`
    /// whose two payload types are themselves Copy.
    ///
    /// Copy values are consumed freely; ownership rules (U05) never apply
    /// to them, so `move`/`clone` keywords are rejected on Copy types.
    pub fn is_copy(&self) -> bool {
        match self {
            Type::Int | Type::Float | Type::Bool | Type::Unit => true,
            Type::Func(_, _) => true,
            Type::Result(t, e) => t.is_copy() && e.is_copy(),
            // D2: a struct is Copy iff every field is Copy — precomputed
            // per declaration by the struct-table pass (dependency order).
            Type::Struct(info) => info.copy,
            // D2: an enum is Copy iff every variant's payload is Copy —
            // precomputed per declaration by the enum-table pass (0.0.6 T04).
            Type::Enum(info) => info.copy,
            // 0.0.7 D2: a parameterized enum's verdict is a function of the
            // use-site arguments — substitute them into the declaration's
            // payload skeletons and re-run the rule.
            Type::App(info, args) => app_is_copy(info, args),
            // 0.0.7 T04: a declaration-local parameter is only inspected
            // inside a payload skeleton; real verdicts come from
            // argument-substituted types. Conservative owned keeps the
            // match total.
            Type::Param(_) => false,
            Type::Range => true, // D5: Range is Copy (two ints)
            // F3: a Future owns its suspended state — never Copy (driven once).
            // 0.0.5 T02 D1: a ptr is a move-only handle — copying it would
            // alias without the explicit `move` at every boundary.
            Type::Future(_)
            | Type::Channel(_)
            | Type::Ptr(_)
            | Type::Var(_)
            | Type::String
            | Type::Box(_)
            | Type::Array(_)
            | Type::Ref(_)
            | Type::Unsupported(_) => false,
        }
    }

    /// Check if this type owns heap data that a binding can *move* (0.0.2):
    /// `string`, `box<_>`, and `Result<T, E>` containing an owned payload.
    ///
    /// Disjoint from `is_copy`: a type is exactly one of copy / owned /
    /// borrowed (`ref _`), which is what the ownership checker keys on.
    pub fn is_owned(&self) -> bool {
        match self {
            // D3: arrays are owned for every element type (even `[int]`).
            Type::String | Type::Box(_) | Type::Array(_) => true,
            Type::Result(t, e) => t.is_owned() || e.is_owned(),
            // A struct with any non-Copy field is an owned type (D2).
            Type::Struct(info) => !info.copy,
            // An enum with any non-Copy variant payload is an owned type (D2,
            // 0.0.6 T04). Must be listed before the catch-all below: a plain
            // `_ => false` would classify a non-Copy enum as neither copy nor
            // owned, and the ownership checker would then let it be consumed
            // freely without `move`/`clone`.
            Type::Enum(info) => !info.copy,
            // 0.0.7 D2 mirror: a fully applied enum is owned iff it is not
            // Copy under argument substitution (`Option<string>` is owned
            // while `Option<int>` is Copy).
            Type::App(info, args) => !app_is_copy(info, args),
            // F3: a Future owns its suspended coroutine frame; it is moved
            // (e.g. into `std.async.run` or `await`), never shared.
            Type::Future(_) => true,
            // F9: a channel owns its buffer; it is move-only (non-Copy).
            Type::Channel(_) => true,
            // 0.0.5 T02 D1: a ptr is an owned, move-only handle (`move p`).
            Type::Ptr(_) => true,
            _ => false,
        }
    }
}

/// Substitute `Type::Param(i)` occurrences with `args[i]` (0.0.7 T04/D1).
/// Structural: recurses through every composite type so nested payloads
/// like `Option<Pair[A, B]>` substitute at every depth. The caller
/// guarantees `args.len()` equals the declaration's `type_param_count`
/// (arity is checked before any `Type::App` is built), so the `get` fallback
/// only fires for malformed trees and keeps the function total.
pub(crate) fn substitute_params(ty: &Type, args: &[Type]) -> Type {
    rewrite_type(
        ty,
        &|i| {
            args.get(i)
                .cloned()
                .unwrap_or_else(|| Type::Unsupported(format!("?P{}", i)))
        },
        &|v| Type::Var(v),
    )
}

/// D2 verdict for a fully applied enum type: Copy iff every
/// argument-substituted variant payload is Copy.
fn app_is_copy(info: &EnumInfo, args: &[Type]) -> bool {
    info.payloads
        .iter()
        .all(|payload| payload.iter().all(|p| substitute_params(p, args).is_copy()))
}

/// Which handle family [`contains_in_value_position`] looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContainsKind {
    Future,
    Channel,
    Ptr,
}

/// The one traversal behind [`Type::contains_future`] /
/// [`Type::contains_channel`] / [`Type::contains_ptr`]. The three rules
/// agree on the whole walk — the matching leaf, the `Array` / `Result`
/// descent, the `App` use-site-argument descent (0.0.7 T04), and stopping
/// at opaque handles — and differ only in which variant counts as a hit,
/// so the shape lives here once and each public predicate is one line.
fn contains_in_value_position(ty: &Type, kind: &ContainsKind) -> bool {
    match ty {
        Type::Future(_) => *kind == ContainsKind::Future,
        Type::Channel(_) => *kind == ContainsKind::Channel,
        Type::Ptr(_) => *kind == ContainsKind::Ptr,
        Type::Array(elem) => contains_in_value_position(elem, kind),
        Type::Result(t, e) => {
            contains_in_value_position(t, kind) || contains_in_value_position(e, kind)
        }
        Type::App(_, args) => args.iter().any(|a| contains_in_value_position(a, kind)),
        _ => false,
    }
}

/// Visit every [`Type::Param`] slot in `ty` (0.0.7 T04), left to right.
/// The single traversal that both `Param`-related rules share — deciding
/// whether a skeleton is fully determined ([`substitute_params`] callers)
/// and collecting the parameters a constructor site left open — so the
/// composite-type arm list lives in one place instead of three.
pub(crate) fn visit_params(ty: &Type, f: &mut impl FnMut(usize)) {
    match ty {
        Type::Param(i) => f(*i),
        Type::Func(ps, r) => {
            for p in ps {
                visit_params(p, f);
            }
            visit_params(r, f);
        }
        Type::Result(a, b) => {
            visit_params(a, f);
            visit_params(b, f);
        }
        Type::Array(e)
        | Type::Box(e)
        | Type::Ref(e)
        | Type::Future(e)
        | Type::Channel(e)
        | Type::Ptr(e) => visit_params(e, f),
        Type::App(_, args) => {
            for a in args {
                visit_params(a, f);
            }
        }
        _ => {}
    }
}

/// Rewrite a type as seen from a use site (0.0.7 T04): every composite
/// arm rebuilds with the same shape, so the "recurse into the children"
/// pattern is expressed once instead of once per rule. `Param` and `Var`
/// are leaves the caller handles (`substitute_params` binds them,
/// `collect_free_vars` records them).
///
/// Free-variable collection (used to decide which parameters a
/// constructor site left undetermined) is expressed with it rather than as
/// a fourth hand-written `Type` match.
pub(crate) fn rewrite_type(
    ty: &Type,
    on_param: &impl Fn(usize) -> Type,
    on_var: &impl Fn(u32) -> Type,
) -> Type {
    match ty {
        Type::Param(i) => on_param(*i),
        Type::Var(v) => on_var(*v),
        Type::Func(ps, r) => Type::Func(
            ps.iter()
                .map(|p| rewrite_type(p, on_param, on_var))
                .collect(),
            Box::new(rewrite_type(r, on_param, on_var)),
        ),
        Type::Result(a, b) => Type::Result(
            Box::new(rewrite_type(a, on_param, on_var)),
            Box::new(rewrite_type(b, on_param, on_var)),
        ),
        Type::Array(e) => Type::Array(Box::new(rewrite_type(e, on_param, on_var))),
        Type::Box(e) => Type::Box(Box::new(rewrite_type(e, on_param, on_var))),
        Type::Ref(e) => Type::Ref(Box::new(rewrite_type(e, on_param, on_var))),
        Type::Future(e) => Type::Future(Box::new(rewrite_type(e, on_param, on_var))),
        Type::Channel(e) => Type::Channel(Box::new(rewrite_type(e, on_param, on_var))),
        Type::Ptr(e) => Type::Ptr(Box::new(rewrite_type(e, on_param, on_var))),
        Type::App(info, args) => Type::App(
            info.clone(),
            args.iter()
                .map(|a| rewrite_type(a, on_param, on_var))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The ownership access form of a single `Ident` use (0.0.2 U05).
///
/// Filled in by the ownership checker (`typeck/ownership.rs`); the lowering
/// stage (U06) picks `MoveLocal` / `CloneLocal` / `LoadLocal` from it:
/// - `Copy` — plain slot load: Copy types, `ref` parameters, and boxes
///   loaded through `deref` (the box itself stays in its slot).
/// - `Move` — `move x`, or an implicit transfer at a value-producing tail
///   position (function/block/branch tail, `box e` inner).
/// - `Clone` — `clone x`, and any read-only use of an owned value
///   (comparisons, `print`, ...): the value is deep-copied, slot untouched.
///
/// Owned *global* bindings are annotated `Clone` too: lower rewrites them
/// to `CloneGlobal` (reads never disturb the global).
///
/// Note: `Ok`/`Err` payloads are **not** implicit-move positions; they are
/// walked as read-only (producer=false) by the ownership checker, so a bare
/// owned local in `Ok(x)` is auto-cloned. This matches the bytecode lowering
/// where the payload is cloned into the Result via PackOk/PackErr.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    /// Plain slot load (Copy semantics).
    Copy,
    /// `move x`: consumes the slot; the binding is moved-from.
    Move,
    /// `clone x` / read-only owned use: deep copy, slot untouched.
    Clone,
}

/// Typed HIR program.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedHir {
    pub items: Vec<TypedHirItem>,
    /// The struct table, in declaration order (0.0.3 V02). Index =
    /// [`StructId`]; consumed by lowering to emit `StructNew(id)` and by
    /// codegen for the `struct_defs` table.
    pub structs: Vec<TypedStructDef>,
    /// The enum table, in declaration order (0.0.6 T04/T06). Index =
    /// [`EnumId`]; consumed by lowering to emit `EnumNew(id)` and by
    /// codegen for the `enum_defs` table.
    pub enums: Vec<TypedEnumDef>,
    pub span: Span,
}

/// One entry of the struct table (0.0.3 V02): the field types in
/// declaration order. Codegen derives the `struct_defs` type tags from it;
/// lowering only needs the field count via `StructNew(id)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedStructDef {
    pub field_types: Vec<Type>,
}

/// One entry of the enum table (0.0.6 T06): the variants in declaration
/// order, with their payload types.
///
/// 0.0.7 T04: `type_params` / `payloads` (the declaration's own
/// payload skeletons, `Param(i)` placeholders intact) are the type-checker's
/// view and are dropped before codegen (`MirEnumDef` carries only what the
/// emitted table holds: the payload *shape*, one tag per variant slot).
#[derive(Debug, Clone, PartialEq)]
pub struct TypedEnumDef {
    /// Declared type parameters in declaration order (0.0.7 T04/D1).
    pub type_params: Vec<String>,
    /// Payload *skeletons* per variant, in declaration order (0.0.7 T04):
    /// the declaration's own types, with [`Type::Param(i)`] where a
    /// parameter is named. `is_copy` / `is_owned` on a [`Type::App`]
    /// substitute the use-site arguments into these and re-run the rule.
    /// A `Param` cannot reach codegen: MIR keeps only the per-variant
    /// shape.
    pub payloads: Vec<Vec<Type>>,
    /// The emitted shape, one entry per variant in declaration order. Each
    /// entry carries the variant's payload list (the runtime representation
    /// is one `Value::Enum` per variant with a payload *per declared slot*,
    /// 0.0.6 T06; 0.0.7 T04 keeps the declared count).
    pub variants: Vec<TypedEnumVariantDef>,
}

/// One emitted enum-variant entry (0.0.6 T06): the payload kinds in
/// declaration order. 0.0.7 T04: a parameterized payload stays `Param(i)`
/// here — the declaration site has no concrete kind to record; codegen
/// maps it to the shape-only `FieldTypeTag::Unit` placeholder.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedEnumVariantDef {
    pub payload_types: Vec<Type>,
}

/// Top-level item in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedHirItem {
    Import(ImportDeclHir),
    Decl(TypedDeclHir),
    Expr(TypedExprHir),
}

/// Declaration in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedDeclHir {
    Func(TypedFuncDeclHir),
    Var(TypedVarBindingHir),
    Const(TypedConstDeclHir),
}

/// Function declaration in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedFuncDeclHir {
    pub name: String,
    pub params: Vec<TypedParamHir>,
    pub ret_type: Option<Type>,
    pub body: TypedFuncBodyHir,
    pub hir_id: HirId,
    pub span: Span,
    /// `async func` (0.0.4 F1): **callers** see `Future<T>`; the body and
    /// `ret_type` still describe the body's own `T`.
    pub is_async: bool,
}

/// Function parameter in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedParamHir {
    pub name: String,
    pub ty: Type,
    pub binding_id: BindingId,
    pub hir_id: HirId,
    pub span: Span,
}

/// Function body in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedFuncBodyHir {
    SingleExpr(Box<TypedExprHir>),
    Block(TypedBlockHir),
}

/// Variable binding in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedVarBindingHir {
    pub name: String,
    pub ty: Option<Type>,
    pub init: Box<TypedExprHir>,
    pub binding_id: BindingId,
    pub hir_id: HirId,
    pub span: Span,
}

/// Const declaration in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedConstDeclHir {
    pub name: String,
    pub ty: Option<Type>,
    pub init: Box<TypedExprHir>,
    pub binding_id: BindingId,
    pub hir_id: HirId,
    pub span: Span,
}

/// Statement in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedStmtHir {
    Decl(TypedDeclHir),
    Expr(Box<TypedExprHir>, bool), // bool = has_semicolon
    Error,
}

/// Block in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedBlockHir {
    pub stmts: Vec<TypedStmtHir>,
    pub tail_expr: Option<Box<TypedExprHir>>,
    pub ty: Type,
    pub hir_id: HirId,
    pub span: Span,
}

/// Expression in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum TypedExprHir {
    /// Assignment: `name = expr`
    Assign {
        name: String,
        binding_id: BindingId,
        rhs: Box<TypedExprHir>,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    /// If expression
    If(TypedExprIfHir),
    /// While expression
    While(TypedExprWhileHir),
    /// For expression (0.0.3 V04)
    For(TypedExprForHir),
    /// Choose expression
    Choose(TypedExprChooseHir),
    /// Binary operators
    Or(Box<TypedExprHir>, Box<TypedExprHir>),
    And(Box<TypedExprHir>, Box<TypedExprHir>),
    Eq(Box<TypedExprHir>, Box<TypedExprHir>),
    Ne(Box<TypedExprHir>, Box<TypedExprHir>),
    Lt(Box<TypedExprHir>, Box<TypedExprHir>),
    Gt(Box<TypedExprHir>, Box<TypedExprHir>),
    Le(Box<TypedExprHir>, Box<TypedExprHir>),
    Ge(Box<TypedExprHir>, Box<TypedExprHir>),
    Add(Box<TypedExprHir>, Box<TypedExprHir>),
    Sub(Box<TypedExprHir>, Box<TypedExprHir>),
    Mul(Box<TypedExprHir>, Box<TypedExprHir>),
    Div(Box<TypedExprHir>, Box<TypedExprHir>),
    Mod(Box<TypedExprHir>, Box<TypedExprHir>),
    /// Unary operators
    Not(Box<TypedExprHir>),
    Neg(Box<TypedExprHir>),
    /// `expr as string` (0.0.2 U13: type cast; result is a fresh owned string)
    Cast(Box<TypedExprHir>, Span),
    /// `expr?` (0.0.2 U04: Result propagation; `ty` is the unwrapped payload `T`)
    Question {
        operand: Box<TypedExprHir>,
        ty: Type,
        span: Span,
    },
    /// `expr await` (0.0.4 F2/F3: drive a `Future<T>` to completion; `ty` is
    /// the yielded `T`). The operand is consumed (a Future is driven once).
    Await {
        operand: Box<TypedExprHir>,
        ty: Type,
        span: Span,
    },
    /// `Ok(value)` / `Err(value)` constructor (0.0.2 U04; `ty` is the full
    /// `Result<T, E>`; `value` is the payload moved into the Result)
    ResultCtor {
        ctor: ResultCtor,
        value: Box<TypedExprHir>,
        ty: Type,
        span: Span,
    },
    /// Enum variant constructor (0.0.6 T04): `Circle(e1, …, en)` or a
    /// zero-payload `Point`. `variant_idx` is the declaration-order variant
    /// index (feeds `EnumNew`'s `u16` variant operand in T06); `payload_tys`
    /// are the converted payload types in declaration order; `ty` is the
    /// full enum type.
    EnumCtor {
        enum_id: EnumId,
        variant_idx: u16,
        payload_tys: Vec<Type>,
        args: Vec<TypedExprHir>,
        ty: Type,
        span: Span,
    },
    /// `box <expr>` (0.0.2 U05/U06: heap allocation; type is `Box<inner>`)
    Box(Box<TypedExprHir>, Span),
    /// `deref <box>` (0.0.2 U05/U06: read the pointee copy; type is the
    /// pointee type, the box itself is untouched)
    Deref(Box<TypedExprHir>, Span),
    /// `deref_ptr <p>` (0.0.5 T02: read through a `ptr<T>`; type is the
    /// pointee type `T`, the pointer itself is untouched)
    DerefPtr(Box<TypedExprHir>, Span),
    /// `addr <place>` (0.0.5 T02, D2: take the address of a place; type is
    /// `ptr<T>` where `T` is the place type)
    Addr {
        place: Box<TypedExprHir>,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    /// `deref_ptr <p> = <expr>` (0.0.5 T02: write through a `ptr<T>`;
    /// replaces the pointee, old value released; the pointer binding itself
    /// is untouched)
    AssignDerefPtr {
        name: String,
        binding_id: BindingId,
        rhs: Box<TypedExprHir>,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    /// `deref <b> = <expr>` (0.0.2 U05: write a box's pointee; replaces the
    /// pointee, old value released; the box binding itself is untouched)
    AssignDeref {
        name: String,
        binding_id: BindingId,
        rhs: Box<TypedExprHir>,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    /// `p.f = <expr>` (0.0.3 V02: one-level field write; replaces the
    /// field, old value released; the struct binding itself is untouched).
    /// `field_idx` is the declaration-order index into the struct table.
    AssignField {
        obj_name: String,
        obj_binding_id: BindingId,
        field_idx: u16,
        rhs: Box<TypedExprHir>,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    /// `a[i] = <expr>` (0.0.3 V03: one-level element write). Bounds are
    /// checked at runtime (`ArrSet*`); `ty` is the element type, carried so
    /// lowering can pick the `Dup`/`DupDeep` form from the RHS without
    /// re-deriving it.
    AssignIndex {
        name: String,
        binding_id: BindingId,
        index: Box<TypedExprHir>,
        rhs: Box<TypedExprHir>,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    /// `[e1, e2, ...]` (0.0.3 V03). Elements keep source order; `ty` is the
    /// full array type (an empty literal takes it from the annotation).
    ArrayLit {
        elems: Vec<TypedExprHir>,
        ty: Type,
        span: Span,
    },
    /// `Name { field: expr, ... }` (0.0.3 V02). Initializers are in
    /// *source* order paired with their declaration-order field index, so
    /// lowering evaluates source-first and reorders into `StructNew`'s
    /// declaration order.
    StructLit {
        struct_id: StructId,
        inits: Vec<(u16, TypedExprHir)>,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    /// Function call
    Call(Box<TypedExprHir>, Vec<TypedExprHir>, Type), // Type = return type
    /// Index access `a[i]` (0.0.3 V03): a bounds-checked **copy** of the
    /// element (deep for owned element types). `ty` is the element type.
    Index {
        arr: Box<TypedExprHir>,
        idx: Box<TypedExprHir>,
        ty: Type,
        span: Span,
    },
    /// Field access `obj.field` (0.0.3 V02): reads are copies of the field
    /// (deep for owned fields). `field_idx` is the declaration-order index
    /// into the struct table; `ty` is the field's type.
    Field {
        obj: Box<TypedExprHir>,
        field_idx: u16,
        ty: Type,
        span: Span,
    },
    /// Literals
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Str(String, Span),
    /// Identifier (0.0.2: `access` records this use's ownership form)
    Ident {
        name: String,
        binding_id: BindingId,
        ty: Type,
        access: Access,
        hir_id: HirId,
        span: Span,
    },
    /// Block expression
    Block(TypedBlockHir),
    /// `channel<T>(capacity)` constructor (0.0.4 F9, T07): creates a new channel
    /// with the given capacity. `payload_ty` is the element type T.
    ChannelNew {
        capacity: Box<TypedExprHir>,
        payload_ty: Type,
        span: Span,
    },
    /// `channel.send(value)` (0.0.4 F9, T07): sends a value, may suspend if full.
    /// `payload_ty` is the element type T (defensive run-time check).
    ChannelSend {
        channel: Box<TypedExprHir>,
        value: Box<TypedExprHir>,
        payload_ty: Type,
        span: Span,
    },
    /// `channel.recv()` (0.0.4 F9, T07): receives a value, may suspend if empty.
    /// `payload_ty` is the element type T (defensive run-time check).
    ChannelRecv {
        channel: Box<TypedExprHir>,
        payload_ty: Type,
        span: Span,
    },
    /// `split(channel)` (0.0.4 F9, T07d): creates a second independent handle
    /// to the same channel buffer. The original and the split handle are both
    /// move-only; neither is Copy.
    ChannelSplit {
        channel: Box<TypedExprHir>,
        span: Span,
    },
}

impl TypedExprHir {
    /// Get the type of this expression.
    pub fn ty(&self) -> Type {
        match self {
            TypedExprHir::Assign { ty, .. } => ty.clone(),
            TypedExprHir::If(e) => e.ty.clone(),
            TypedExprHir::While(e) => e.ty.clone(),
            TypedExprHir::For(e) => e.ty.clone(),
            TypedExprHir::Choose(e) => e.ty.clone(),
            TypedExprHir::Or(_, _) => Type::Bool,
            TypedExprHir::And(_, _) => Type::Bool,
            TypedExprHir::Eq(_, _) => Type::Bool,
            TypedExprHir::Ne(_, _) => Type::Bool,
            TypedExprHir::Lt(_, _) => Type::Bool,
            TypedExprHir::Gt(_, _) => Type::Bool,
            TypedExprHir::Le(_, _) => Type::Bool,
            TypedExprHir::Ge(_, _) => Type::Bool,
            TypedExprHir::Add(l, _) => l.ty(),
            TypedExprHir::Sub(l, _) => l.ty(),
            TypedExprHir::Mul(l, _) => l.ty(),
            TypedExprHir::Div(l, _) => l.ty(),
            TypedExprHir::Mod(l, _) => l.ty(),
            TypedExprHir::Not(_) => Type::Bool,
            TypedExprHir::Neg(e) => e.ty(),
            TypedExprHir::Cast(_, _) => Type::String,
            TypedExprHir::Question { ty, .. } => ty.clone(),
            TypedExprHir::Await { ty, .. } => ty.clone(),
            TypedExprHir::ResultCtor { ty, .. } => ty.clone(),
            TypedExprHir::EnumCtor { ty, .. } => ty.clone(),
            TypedExprHir::Box(inner, _) => Type::Box(Box::new(inner.ty())),
            TypedExprHir::Deref(inner, _) => match inner.ty() {
                // typeck guarantees the deref operand is a `box<T>` (U05);
                // the fallback keeps `ty()` total for error-recovery trees.
                Type::Box(pointee) => *pointee,
                other => other,
            },
            TypedExprHir::AssignDeref { .. } => Type::Unit,
            TypedExprHir::AssignDerefPtr { .. } => Type::Unit,
            TypedExprHir::AssignField { .. } => Type::Unit,
            TypedExprHir::AssignIndex { .. } => Type::Unit,
            TypedExprHir::DerefPtr(inner, _) => match inner.ty() {
                // typeck guarantees the deref_ptr operand is a `ptr<T>`;
                // the fallback keeps `ty()` total for error-recovery trees.
                Type::Ptr(pointee) => *pointee,
                other => other,
            },
            TypedExprHir::Addr { ty, .. } => ty.clone(),
            TypedExprHir::ArrayLit { ty, .. } => ty.clone(),
            TypedExprHir::StructLit { ty, .. } => ty.clone(),
            TypedExprHir::Call(_, _, ret_ty) => ret_ty.clone(),
            TypedExprHir::Index { ty, .. } => ty.clone(),
            TypedExprHir::Field { ty, .. } => ty.clone(),
            TypedExprHir::Int(_, _) => Type::Int,
            TypedExprHir::Float(_, _) => Type::Float,
            TypedExprHir::Bool(_, _) => Type::Bool,
            TypedExprHir::Str(_, _) => Type::String,
            TypedExprHir::Ident { ty, .. } => ty.clone(),
            TypedExprHir::Block(b) => b.ty.clone(),
            TypedExprHir::ChannelNew { payload_ty, .. } => {
                Type::Channel(Box::new(payload_ty.clone()))
            }
            TypedExprHir::ChannelSend { .. } => Type::Unit,
            TypedExprHir::ChannelRecv { payload_ty, .. } => payload_ty.clone(),
            TypedExprHir::ChannelSplit { channel, .. } => channel.ty(),
        }
    }

    /// Get the span of this expression.
    pub fn span(&self) -> Span {
        match self {
            TypedExprHir::Assign { span, .. } => *span,
            TypedExprHir::If(e) => e.span,
            TypedExprHir::While(e) => e.span,
            TypedExprHir::For(e) => e.span,
            TypedExprHir::Choose(e) => e.span,
            TypedExprHir::Or(l, _) => l.span(),
            TypedExprHir::And(l, _) => l.span(),
            TypedExprHir::Eq(l, _) => l.span(),
            TypedExprHir::Ne(l, _) => l.span(),
            TypedExprHir::Lt(l, _) => l.span(),
            TypedExprHir::Gt(l, _) => l.span(),
            TypedExprHir::Le(l, _) => l.span(),
            TypedExprHir::Ge(l, _) => l.span(),
            TypedExprHir::Add(l, _) => l.span(),
            TypedExprHir::Sub(l, _) => l.span(),
            TypedExprHir::Mul(l, _) => l.span(),
            TypedExprHir::Div(l, _) => l.span(),
            TypedExprHir::Mod(l, _) => l.span(),
            TypedExprHir::Not(e) => e.span(),
            TypedExprHir::Neg(e) => e.span(),
            TypedExprHir::Cast(_, s) => *s,
            TypedExprHir::Question { span, .. } => *span,
            TypedExprHir::Await { span, .. } => *span,
            TypedExprHir::ResultCtor { span, .. } => *span,
            TypedExprHir::EnumCtor { span, .. } => *span,
            TypedExprHir::Box(_, s) | TypedExprHir::Deref(_, s) | TypedExprHir::DerefPtr(_, s) => {
                *s
            }
            TypedExprHir::Addr { span, .. } => *span,
            TypedExprHir::AssignDeref { span, .. } => *span,
            TypedExprHir::AssignDerefPtr { span, .. } => *span,
            TypedExprHir::AssignField { span, .. } => *span,
            TypedExprHir::AssignIndex { span, .. } => *span,
            TypedExprHir::ArrayLit { span, .. } => *span,
            TypedExprHir::StructLit { span, .. } => *span,
            TypedExprHir::Call(f, _, _) => f.span(),
            TypedExprHir::Index { span, .. } => *span,
            TypedExprHir::Field { span, .. } => *span,
            TypedExprHir::Int(_, s)
            | TypedExprHir::Float(_, s)
            | TypedExprHir::Bool(_, s)
            | TypedExprHir::Str(_, s) => *s,
            TypedExprHir::Ident { span, .. } => *span,
            TypedExprHir::ChannelNew { span, .. }
            | TypedExprHir::ChannelSend { span, .. }
            | TypedExprHir::ChannelRecv { span, .. }
            | TypedExprHir::ChannelSplit { span, .. } => *span,
            TypedExprHir::Block(b) => b.span,
        }
    }
}

/// If expression in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedExprIfHir {
    pub condition: Box<TypedExprHir>,
    pub then_branch: TypedBlockHir,
    pub elif_branches: Vec<(Box<TypedExprHir>, TypedBlockHir)>,
    pub else_branch: Option<TypedBlockHir>,
    pub ty: Type,
    pub hir_id: HirId,
    pub span: Span,
}

/// While expression in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedExprWhileHir {
    pub condition: Box<TypedExprHir>,
    pub body: TypedBlockHir,
    pub ty: Type,
    pub hir_id: HirId,
    pub span: Span,
}

/// For expression in Typed HIR (0.0.3 V04).
#[derive(Debug, Clone, PartialEq)]
pub struct TypedExprForHir {
    /// Loop variable name (immutable, scoped to `body`).
    pub var: String,
    /// Resolver-assigned binding for the loop variable.
    pub var_binding_id: BindingId,
    /// Typed iterable expression (evaluated once, before the loop).
    pub iterable: Box<TypedExprHir>,
    /// Element type: the type of the loop variable per iteration.
    pub elem_ty: Type,
    pub body: TypedBlockHir,
    pub ty: Type,
    pub hir_id: HirId,
    pub span: Span,
}

/// Choose expression in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedExprChooseHir {
    pub scrutinee: Box<TypedExprHir>,
    pub arms: Vec<TypedChooseArmHir>,
    pub ty: Type,
    pub hir_id: HirId,
    pub span: Span,
}

/// Choose arm in Typed HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedChooseArmHir {
    pub pattern: TypedPatternHir,
    pub guard: Option<Box<TypedExprHir>>,
    pub body: TypedBlockHir,
}

/// Pattern in Typed HIR (0.0.7 T02: recursive).
#[derive(Debug, Clone, PartialEq)]
pub enum TypedPatternHir {
    Literal(Box<TypedExprHir>),
    Ident {
        name: String,
        binding_id: BindingId,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    /// `Ok(p)` / `Err(p)` (recursive payload). T05 owns full semantics.
    ResultCtor {
        ctor: ResultCtor,
        payload: Box<TypedPatternHir>,
        hir_id: HirId,
        span: Span,
    },
    /// Payload-carrying enum-variant pattern (recursive payloads).
    Enum {
        enum_id: EnumId,
        variant_index: u32,
        name: String,
        payloads: Vec<TypedPatternHir>,
        hir_id: HirId,
        span: Span,
    },
    /// Zero-payload variant (bare). Discriminant match only.
    EnumVariant {
        enum_id: EnumId,
        variant_index: u32,
        name: String,
        span: Span,
    },
    /// 0.0.7 D6.
    Or(Vec<TypedPatternHir>, Span),
    /// 0.0.7 D7. `ty` refers to the shared `typed_hir::Type` enum.
    TypeGuard {
        pattern: Box<TypedPatternHir>,
        ty: Type,
        span: Span,
    },
    Error,
}
