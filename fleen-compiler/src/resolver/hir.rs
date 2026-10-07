//! HIR (High-level Intermediate Representation) for Fleen.
//!
//! The HIR is the output of the Resolver: AST with all names resolved to binding IDs.
//! Every identifier/reference is linked to its declaration via `BindingId`.

use crate::lexer::Span;
use crate::parser::ast::*;
use std::fmt;

/// Unique ID for a HIR node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirId(pub u32);

impl fmt::Display for HirId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "hir#{}", self.0)
    }
}

/// Unique ID for a binding (variable, const, function, parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BindingId(pub u32);

impl fmt::Display for BindingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bind#{}", self.0)
    }
}

/// Top-level HIR program.
#[derive(Debug, Clone, PartialEq)]
pub struct Hir {
    pub items: Vec<HirItem>,
    pub span: Span,
}

/// Top-level item in HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum HirItem {
    Import(ImportDeclHir),
    Decl(DeclHir),
    Expr(ExprHir),
}

/// Import declaration in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDeclHir {
    pub path: Vec<String>,
    pub span: Span,
}

/// Declaration in HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum DeclHir {
    Func(FuncDeclHir),
    Var(VarBindingHir),
    Const(ConstDeclHir),
}

/// Function declaration in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncDeclHir {
    pub name: String,
    pub params: Vec<ParamHir>,
    pub ret_type: Option<Type>,
    pub body: FuncBodyHir,
    pub hir_id: HirId,
    pub span: Span,
}

/// Function parameter in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamHir {
    pub name: String,
    pub ty: Type,
    pub binding_id: BindingId,
    pub hir_id: HirId,
    pub span: Span,
}

/// Function body in HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum FuncBodyHir {
    SingleExpr(Box<ExprHir>),
    Block(BlockHir),
}

/// Variable binding in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct VarBindingHir {
    pub name: String,
    pub ty: Option<Type>,
    pub init: Box<ExprHir>,
    pub binding_id: BindingId,
    pub hir_id: HirId,
    pub span: Span,
}

/// Const declaration in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDeclHir {
    pub name: String,
    pub ty: Option<Type>,
    pub init: Box<ExprHir>,
    pub binding_id: BindingId,
    pub hir_id: HirId,
    pub span: Span,
}

/// Statement in HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum StmtHir {
    Decl(DeclHir),
    Expr(Box<ExprHir>, bool), // bool = has_semicolon
    /// Error placeholder: the statement failed to resolve (error already recorded).
    /// Keeps positions of later statements stable during error recovery.
    Error,
}

/// Block in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockHir {
    pub stmts: Vec<StmtHir>,
    pub tail_expr: Option<Box<ExprHir>>,
    pub hir_id: HirId,
    pub span: Span,
}

/// Expression in HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum ExprHir {
    /// Assignment: `name = expr` (resolved to binding)
    Assign {
        name: String,
        binding_id: BindingId,
        rhs: Box<ExprHir>,
        hir_id: HirId,
        span: Span,
    },
    /// Assignment through a box pointee: `deref b = expr` (0.0.2).
    /// `binding_id` refers to the box variable `b`; writing the pointee does
    /// not rebind `b` itself. Semantics are enforced by typeck (U04/U05).
    AssignDeref {
        name: String,
        binding_id: BindingId,
        rhs: Box<ExprHir>,
        hir_id: HirId,
        span: Span,
    },
    /// If expression
    If(ExprIfHir),
    /// While expression
    While(ExprWhileHir),
    /// Choose expression
    Choose(ExprChooseHir),
    /// Binary operators
    Or(Box<ExprHir>, Box<ExprHir>),
    And(Box<ExprHir>, Box<ExprHir>),
    Eq(Box<ExprHir>, Box<ExprHir>),
    Ne(Box<ExprHir>, Box<ExprHir>),
    Lt(Box<ExprHir>, Box<ExprHir>),
    Gt(Box<ExprHir>, Box<ExprHir>),
    Le(Box<ExprHir>, Box<ExprHir>),
    Ge(Box<ExprHir>, Box<ExprHir>),
    Add(Box<ExprHir>, Box<ExprHir>),
    Sub(Box<ExprHir>, Box<ExprHir>),
    Mul(Box<ExprHir>, Box<ExprHir>),
    Div(Box<ExprHir>, Box<ExprHir>),
    Mod(Box<ExprHir>, Box<ExprHir>),
    /// Unary operators
    Not(Box<ExprHir>),
    Neg(Box<ExprHir>),
    /// `move <place>` (0.0.2; span covers the whole expression)
    Move(Box<ExprHir>, Span),
    /// `clone <place>` (0.0.2)
    Clone(Box<ExprHir>, Span),
    /// `box <expr>` (0.0.2)
    Box(Box<ExprHir>, Span),
    /// `deref <postfix>` (0.0.2)
    Deref(Box<ExprHir>, Span),
    /// `<expr>?` (0.0.2 Result propagation; span covers operand and `?`)
    Question(Box<ExprHir>, Span),
    /// Function call
    Call(Box<ExprHir>, Vec<ExprHir>),
    /// Index access
    Index(Box<ExprHir>, Box<ExprHir>),
    /// Field access
    Field(Box<ExprHir>, String),
    /// Literals (with their source span)
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Str(String, Span),
    /// Identifier (resolved to binding)
    Ident {
        name: String,
        binding_id: BindingId,
        hir_id: HirId,
        span: Span,
    },
    /// Block expression
    Block(BlockHir),
}

impl ExprHir {
    /// Get the span of this expression.
    pub fn span(&self) -> Span {
        match self {
            ExprHir::Assign { span, .. } => *span,
            ExprHir::AssignDeref { span, .. } => *span,
            ExprHir::If(e) => e.span,
            ExprHir::While(e) => e.span,
            ExprHir::Choose(e) => e.span,
            ExprHir::Or(l, _) => l.span(),
            ExprHir::And(l, _) => l.span(),
            ExprHir::Eq(l, _) => l.span(),
            ExprHir::Ne(l, _) => l.span(),
            ExprHir::Lt(l, _) => l.span(),
            ExprHir::Gt(l, _) => l.span(),
            ExprHir::Le(l, _) => l.span(),
            ExprHir::Ge(l, _) => l.span(),
            ExprHir::Add(l, _) => l.span(),
            ExprHir::Sub(l, _) => l.span(),
            ExprHir::Mul(l, _) => l.span(),
            ExprHir::Div(l, _) => l.span(),
            ExprHir::Mod(l, _) => l.span(),
            ExprHir::Not(e) => e.span(),
            ExprHir::Neg(e) => e.span(),
            ExprHir::Move(_, s)
            | ExprHir::Clone(_, s)
            | ExprHir::Box(_, s)
            | ExprHir::Deref(_, s)
            | ExprHir::Question(_, s) => *s,
            ExprHir::Call(f, _) => f.span(),
            ExprHir::Index(a, _) => a.span(),
            ExprHir::Field(o, _) => o.span(),
            ExprHir::Int(_, s)
            | ExprHir::Float(_, s)
            | ExprHir::Bool(_, s)
            | ExprHir::Str(_, s) => *s,
            ExprHir::Ident { span, .. } => *span,
            ExprHir::Block(b) => b.span,
        }
    }
}

/// If expression in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprIfHir {
    pub condition: Box<ExprHir>,
    pub then_branch: BlockHir,
    pub elif_branches: Vec<(Box<ExprHir>, BlockHir)>,
    pub else_branch: Option<BlockHir>,
    pub hir_id: HirId,
    pub span: Span,
}

/// While expression in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprWhileHir {
    pub condition: Box<ExprHir>,
    pub body: BlockHir,
    pub hir_id: HirId,
    pub span: Span,
}

/// Choose expression in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprChooseHir {
    pub scrutinee: Box<ExprHir>,
    pub arms: Vec<ChooseArmHir>,
    pub hir_id: HirId,
    pub span: Span,
}

/// Choose arm in HIR.
#[derive(Debug, Clone, PartialEq)]
pub struct ChooseArmHir {
    pub pattern: PatternHir,
    pub guard: Option<Box<ExprHir>>,
    pub body: BlockHir,
}

/// Pattern in HIR.
#[derive(Debug, Clone, PartialEq)]
pub enum PatternHir {
    Literal(Box<ExprHir>),
    Ident {
        name: String,
        binding_id: BindingId,
        hir_id: HirId,
        span: Span,
    },
    /// `Ok(binding)` / `Err(binding)` (0.0.2; semantics checked by typeck).
    ResultCtor {
        ctor: ResultCtor,
        name: String,
        binding_id: BindingId,
        hir_id: HirId,
        span: Span,
    },
    /// Placeholder for a pattern that failed to resolve (error already recorded).
    Error,
}
