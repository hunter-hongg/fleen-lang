//! Typed HIR: HIR with type information attached.
//!
//! Every expression node in TypedHir carries its inferred `Type`.
//! The type checker produces this structure, which is then consumed
//! by the lowering and codegen stages.

use crate::lexer::Span;
use crate::resolver::hir::*;

/// A type in the Fleen type system (0.0.1).
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
    /// Array type (parsed but unsupported in 0.0.1 codegen).
    Array(Box<Type>),
    /// Box type (parsed but unsupported in 0.0.1 codegen).
    Box(Box<Type>),
    /// Ref type (parsed but unsupported in 0.0.1 codegen).
    Ref(Box<Type>),
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
            Type::Unsupported(name) => name.clone(),
        }
    }

    /// Check if this type is a numeric type (Int or Float).
    pub fn is_numeric(&self) -> bool {
        matches!(self, Type::Int | Type::Float)
    }

    /// Check if this type is a function type.
    pub fn is_func(&self) -> bool {
        matches!(self, Type::Func(_, _))
    }
}

/// Typed HIR program.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedHir {
    pub items: Vec<TypedHirItem>,
    pub span: Span,
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
    /// Function call
    Call(Box<TypedExprHir>, Vec<TypedExprHir>, Type), // Type = return type
    /// Index access
    Index(Box<TypedExprHir>, Box<TypedExprHir>),
    /// Field access
    Field(Box<TypedExprHir>, String),
    /// Literals
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Str(String, Span),
    /// Identifier
    Ident {
        name: String,
        binding_id: BindingId,
        ty: Type,
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
            TypedExprHir::Call(_, _, ret_ty) => ret_ty.clone(),
            TypedExprHir::Index(_, _) => Type::Unsupported("index".to_string()),
            TypedExprHir::Field(_, _) => Type::Unsupported("field".to_string()),
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
            TypedExprHir::Call(f, _, _) => f.span(),
            TypedExprHir::Index(a, _) => a.span(),
            TypedExprHir::Field(o, _) => o.span(),
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
    Error,
}
