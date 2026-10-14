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
    /// `Range` — builtin iterable type (0.0.3 V04). Constructed via std
    /// (`range`/`range_from`); iterates `start..end` (end-exclusive). Copy.
    Range,
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
            Type::Range => "Range".to_string(),
            Type::Unsupported(name) => name.clone(),
        }
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
            Type::Range => true, // D5: Range is Copy (two ints)
            Type::String | Type::Box(_) | Type::Array(_) | Type::Ref(_) | Type::Unsupported(_) => {
                false
            }
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
            _ => false,
        }
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
    pub span: Span,
}

/// One entry of the struct table (0.0.3 V02): the field types in
/// declaration order. Codegen derives the `struct_defs` type tags from it;
/// lowering only needs the field count via `StructNew(id)`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedStructDef {
    pub field_types: Vec<Type>,
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
    /// `Ok(value)` / `Err(value)` constructor (0.0.2 U04; `ty` is the full
    /// `Result<T, E>`; `value` is the payload moved into the Result)
    ResultCtor {
        ctor: ResultCtor,
        value: Box<TypedExprHir>,
        ty: Type,
        span: Span,
    },
    /// `box <expr>` (0.0.2 U05/U06: heap allocation; type is `Box<inner>`)
    Box(Box<TypedExprHir>, Span),
    /// `deref <box>` (0.0.2 U05/U06: read the pointee copy; type is the
    /// pointee type, the box itself is untouched)
    Deref(Box<TypedExprHir>, Span),
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
            TypedExprHir::ResultCtor { ty, .. } => ty.clone(),
            TypedExprHir::Box(inner, _) => Type::Box(Box::new(inner.ty())),
            TypedExprHir::Deref(inner, _) => match inner.ty() {
                // typeck guarantees the deref operand is a `box<T>` (U05);
                // the fallback keeps `ty()` total for error-recovery trees.
                Type::Box(pointee) => *pointee,
                other => other,
            },
            TypedExprHir::AssignDeref { .. } => Type::Unit,
            TypedExprHir::AssignField { .. } => Type::Unit,
            TypedExprHir::AssignIndex { .. } => Type::Unit,
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
            TypedExprHir::ResultCtor { span, .. } => *span,
            TypedExprHir::Box(_, s) | TypedExprHir::Deref(_, s) => *s,
            TypedExprHir::AssignDeref { span, .. } => *span,
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

/// Pattern in Typed HIR.
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
    /// `Ok(v)` / `Err(e)` (0.0.2 U04): binds the payload `v: T` / `e: E`
    /// of a `Result<T, E>` scrutinee; the binding is a move (D7).
    ResultCtor {
        ctor: ResultCtor,
        name: String,
        binding_id: BindingId,
        ty: Type,
        hir_id: HirId,
        span: Span,
    },
    Error,
}
