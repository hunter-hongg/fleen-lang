//! AST node definitions for the Fleen parser.
//!
//! Nodes are designed to match the EBNF grammar in `SYNTAX.ebnf`,
//! with naming per `SPEC.md §3`: `ExprIf` for `if_expr`, `StmtLet` for `let_stmt`, etc.

use crate::lexer::{Span, TokenKind};
use std::fmt;

/// Top-level program.
#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    pub items: Vec<Item>,
    pub span: Span,
}

/// Top-level item: import, declaration, or expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `import std.io;`
    Import(ImportDecl),
    /// `func`, `const`, or variable binding
    Decl(Decl),
    /// Top-level expression statement
    Expr(Expr),
}

/// Import declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: Vec<String>,
    pub span: Span,
}

/// Declaration: function, variable binding, const, or struct.
#[derive(Debug, Clone, PartialEq)]
pub enum Decl {
    /// `func add(a: int, b: int): int = a + b`
    Func(FuncDecl),
    /// `x: int = 42;` or `x = 42;`
    Var(VarBinding),
    /// `const y: int = 42;`
    Const(ConstDecl),
    /// `struct Point { x: int, y: int }` (0.0.3 V02)
    Struct(StructDecl),
}

/// Struct declaration (0.0.3 V02). Like `func`, a struct declaration takes
/// no trailing semicolon.
#[derive(Debug, Clone, PartialEq)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<StructField>,
    pub span: Span,
    /// Whether the declaration was prefixed with `pub` (0.0.3 V05).
    pub is_pub: bool,
}

/// One `name: type` entry in a struct declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct StructField {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

/// Function declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret_type: Option<Type>,
    pub body: FuncBody,
    pub span: Span,
    /// Whether the declaration was prefixed with `pub` (0.0.3 V05).
    pub is_pub: bool,
    /// Whether the declaration was prefixed with `async` (0.0.4 F1): the
    /// function's *caller* sees `Future<T>` while the body still returns `T`.
    pub is_async: bool,
}

/// Parameter in a function declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

/// Function body: single expression or block.
#[derive(Debug, Clone, PartialEq)]
pub enum FuncBody {
    /// `func name(params): type = expr`
    SingleExpr(Box<Expr>),
    /// `func name(params): type { ... }`
    Block(Block),
}

/// Variable binding.
#[derive(Debug, Clone, PartialEq)]
pub struct VarBinding {
    pub name: String,
    pub ty: Option<Type>,
    pub init: Box<Expr>,
    pub span: Span,
    /// Whether the declaration was prefixed with `pub` (0.0.3 V05).
    pub is_pub: bool,
}

/// Const declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: String,
    pub ty: Option<Type>,
    pub init: Box<Expr>,
    pub span: Span,
    /// Whether the declaration was prefixed with `pub` (0.0.3 V05).
    pub is_pub: bool,
}

/// Type system (0.0.1).
#[derive(Debug, Clone, PartialEq)]
pub enum Type {
    /// `int`, `float`, `bool`, `string`, `unit`
    Base(BaseType),
    /// `[T]`
    Array(Box<Type>),
    /// `box<T>`
    Box(Box<Type>),
    /// `ref T`
    Ref(Box<Type>),
    /// `Result<T, E>`
    Result(Box<Type>, Box<Type>),
    /// `(T, U) -> R`
    Func(Vec<Type>, Box<Type>),
    /// `Range` — builtin iterable type (0.0.3 V04). Constructed via std
    /// (`range`/`range_from`); iterates `start..end` (end-exclusive).
    Range,
    /// `Future<T>` — the type of a suspended async coroutine (0.0.4 F3).
    /// Non-Copy; owns the suspend state; driven once by `await`.
    Future(Box<Type>),
    /// `channel<T>` — a message-passing channel (0.0.4 F9, D3/D6).
    /// Non-Copy, move-only; holds a reference to a shared buffer.
    /// The element type `T` is checked at run time as a defensive backstop.
    Channel(Box<Type>),
    /// A named user type (a struct, 0.0.3 V02). Whether the name actually
    /// refers to a declared struct is resolved by typeck.
    Named(String),
}

/// Base types.
#[derive(Debug, Clone, PartialEq)]
pub enum BaseType {
    Int,
    Float,
    Bool,
    String,
    Unit,
}

impl BaseType {
    /// Convert a TokenKind to a BaseType if it's a type keyword.
    pub fn from_token(kind: &TokenKind) -> Option<Self> {
        Some(match kind {
            TokenKind::IntType => BaseType::Int,
            TokenKind::FloatType => BaseType::Float,
            TokenKind::BoolType => BaseType::Bool,
            TokenKind::StringType => BaseType::String,
            TokenKind::UnitType => BaseType::Unit,
            _ => return None,
        })
    }
}

impl fmt::Display for BaseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BaseType::Int => write!(f, "int"),
            BaseType::Float => write!(f, "float"),
            BaseType::Bool => write!(f, "bool"),
            BaseType::String => write!(f, "string"),
            BaseType::Unit => write!(f, "unit"),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Base(bt) => write!(f, "{}", bt),
            Type::Array(t) => write!(f, "[{}]", t),
            Type::Box(t) => write!(f, "box<{}>", t),
            Type::Ref(t) => write!(f, "ref {}", t),
            Type::Result(ok, err) => write!(f, "Result<{}, {}>", ok, err),
            Type::Named(name) => write!(f, "{name}"),
            Type::Func(params, ret) => {
                let ps: Vec<String> = params.iter().map(|t| t.to_string()).collect();
                write!(f, "({},) -> {}", ps.join(", "), ret)
            }
            Type::Range => write!(f, "Range"),
            Type::Future(inner) => write!(f, "Future<{}>", inner),
            Type::Channel(inner) => write!(f, "channel<{}>", inner),
        }
    }
}

/// Block: sequence of statements with optional tail expression.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    /// Trailing expression without semicolon (becomes return value if in function).
    pub tail_expr: Option<Box<Expr>>,
    pub span: Span,
}

/// Statement.
#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    /// Declaration (func, const, var)
    Decl(Decl),
    /// Expression statement.
    /// The boolean indicates whether a semicolon was present (true = discard value, false = tail expression).
    Expr(Box<Expr>, bool),
}

/// Expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// `lhs = rhs` (assignment, expression position: never binds a new name)
    Assign(Box<Expr>, Box<Expr>),
    /// `if cond { ... } elif cond { ... } else { ... }`
    If(ExprIf),
    /// `while cond { ... }`
    While(ExprWhile),
    /// `for x in expr { ... }` (0.0.3 V04)
    For(ExprFor),
    /// `choose expr { when pat [if guard] { body } ... }`
    Choose(ExprChoose),
    /// `lhs or rhs` (short-circuit)
    Or(Box<Expr>, Box<Expr>),
    /// `lhs and rhs` (short-circuit)
    And(Box<Expr>, Box<Expr>),
    /// `lhs == rhs`
    Eq(Box<Expr>, Box<Expr>),
    /// `lhs != rhs`
    Ne(Box<Expr>, Box<Expr>),
    /// `lhs < rhs`
    Lt(Box<Expr>, Box<Expr>),
    /// `lhs > rhs`
    Gt(Box<Expr>, Box<Expr>),
    /// `lhs <= rhs`
    Le(Box<Expr>, Box<Expr>),
    /// `lhs >= rhs`
    Ge(Box<Expr>, Box<Expr>),
    /// `lhs + rhs`
    Add(Box<Expr>, Box<Expr>),
    /// `lhs - rhs`
    Sub(Box<Expr>, Box<Expr>),
    /// `lhs * rhs`
    Mul(Box<Expr>, Box<Expr>),
    /// `lhs / rhs`
    Div(Box<Expr>, Box<Expr>),
    /// `lhs % rhs`
    Mod(Box<Expr>, Box<Expr>),
    /// `!` (logical not)
    Not(Box<Expr>),
    /// Unary negation
    Neg(Box<Expr>),
    /// `move <place>` (0.0.2: ownership transfer; place validated by typeck)
    Move(Box<Expr>, Span),
    /// `clone <place>` (0.0.2: deep copy; place validated by typeck)
    Clone(Box<Expr>, Span),
    /// `box <expr>` (0.0.2: heap allocation)
    Box(Box<Expr>, Span),
    /// `deref <unary>` (0.0.2: box pointee read)
    Deref(Box<Expr>, Span),
    /// `<expr> as <type>` (0.0.2 U13: type cast; 0.0.2 whitelist: scalar → string only)
    Cast(Box<Expr>, Type, Span),
    /// `<expr>?` (0.0.2: Result propagation suffix)
    Question(Box<Expr>, Span),
    /// `<expr> await` (0.0.4 F2: postfix future drive; `e: Future<T>` is
    /// consumed and yields `T`). Binds with the tightest suffix group
    /// (`() [i] .f await ?`), chaining left-to-right.
    Await(Box<Expr>, Span),
    /// `channel<T>(capacity)` (0.0.4 T07: channel constructor).
    /// `ty` is the element type `T`, `capacity` is the buffer size expression.
    ChannelNew {
        ty: Type,
        capacity: Box<Expr>,
        span: Span,
    },
    /// `func(args)`
    Call(Box<Expr>, Vec<Expr>),
    /// `arr[idx]` — span covers the whole `arr[idx]`, including the `]`.
    Index(Box<Expr>, Box<Expr>, Span),
    /// `[e1, e2, ...]` — array literal (0.0.3 V03). Parsed at the primary
    /// position; the postfix-position `[` is an index, distinguished by
    /// parse position alone (the same predicate ASI uses).
    ArrayLit(ExprArrayLit),
    /// `obj.field`
    Field(Box<Expr>, String),
    /// `Name { field: expr, ... }` — struct literal (0.0.3 V02; the
    /// `struct_literal` grammar rule reserved since 0.0.1). Initializers
    /// keep source order; field-order rules are typeck's business.
    StructLit(ExprStructLit),
    /// Integer literal
    Int(i64, Span),
    /// Float literal
    Float(f64, Span),
    /// Boolean literal
    Bool(bool, Span),
    /// String literal
    Str(String, Span),
    /// Identifier (with its usage span, so resolver errors can point at the use site)
    Ident(String, Span),
    /// Block expression `{ stmts... }`
    Block(Block),
}

impl Expr {
    /// The source span of this expression.
    ///
    /// Binary/unary nodes derive their span from the leftmost operand;
    /// sufficient for error reporting without storing redundant spans.
    pub fn span(&self) -> Span {
        match self {
            Expr::Assign(l, _)
            | Expr::Or(l, _)
            | Expr::And(l, _)
            | Expr::Eq(l, _)
            | Expr::Ne(l, _)
            | Expr::Lt(l, _)
            | Expr::Gt(l, _)
            | Expr::Le(l, _)
            | Expr::Ge(l, _)
            | Expr::Add(l, _)
            | Expr::Sub(l, _)
            | Expr::Mul(l, _)
            | Expr::Div(l, _)
            | Expr::Mod(l, _) => l.span(),
            Expr::Not(e) | Expr::Neg(e) => e.span(),
            Expr::Move(_, span)
            | Expr::Clone(_, span)
            | Expr::Box(_, span)
            | Expr::Deref(_, span)
            | Expr::Cast(_, _, span)
            | Expr::Question(_, span)
            | Expr::Await(_, span)
            | Expr::ChannelNew { span, .. } => *span,
            Expr::Call(f, _) => f.span(),
            Expr::Index(_, _, s) => *s,
            Expr::ArrayLit(l) => l.span,
            Expr::Field(o, _) => o.span(),
            Expr::StructLit(l) => l.span,
            Expr::Int(_, s)
            | Expr::Float(_, s)
            | Expr::Bool(_, s)
            | Expr::Str(_, s)
            | Expr::Ident(_, s) => *s,
            Expr::If(e) => e.span,
            Expr::While(e) => e.span,
            Expr::For(e) => e.span,
            Expr::Choose(e) => e.span,
            Expr::Block(b) => b.span,
        }
    }
}

/// Struct literal: `Name { field: expr, ... }` (0.0.3 V02).
#[derive(Debug, Clone, PartialEq)]
pub struct ExprStructLit {
    pub name: String,
    pub inits: Vec<(String, Expr)>,
    pub span: Span,
}

/// Array literal: `[e1, e2, ...]` (0.0.3 V03). Elements keep source order;
/// an empty literal needs a type annotation (typeck: `CannotInferArrayType`).
#[derive(Debug, Clone, PartialEq)]
pub struct ExprArrayLit {
    pub elems: Vec<Expr>,
    pub span: Span,
}

/// `if` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprIf {
    pub condition: Box<Expr>,
    pub then_branch: Block,
    pub elif_branches: Vec<(Box<Expr>, Block)>,
    pub else_branch: Option<Block>,
    pub span: Span,
}

/// `while` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprWhile {
    pub condition: Box<Expr>,
    pub body: Block,
    pub span: Span,
}

/// `for` expression (0.0.3 V04): `for var in iterable { body }`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprFor {
    /// Loop variable name (bound immutable, scoped to `body`).
    pub var: String,
    /// The iterable expression (evaluated once, before the loop).
    pub iterable: Box<Expr>,
    pub body: Block,
    pub span: Span,
}

/// `choose` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ExprChoose {
    pub scrutinee: Box<Expr>,
    pub arms: Vec<ChooseArm>,
    pub span: Span,
}

/// A `when` arm in a `choose` expression.
#[derive(Debug, Clone, PartialEq)]
pub struct ChooseArm {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Block,
}

/// Which Result constructor a `result_pattern` matches (0.0.2).
///
/// `Ok` / `Err` stay plain identifiers at the lexer level; the parser only
/// recognizes the syntactic form, and typeck (U04) binds the semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultCtor {
    Ok,
    Err,
}

/// Pattern in a `choose` arm.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// Literal pattern (int, float, bool, string)
    Literal(Expr),
    /// Identifier pattern (binds variable). Carries the pattern's span.
    Ident(String, Span),
    /// `Ok(binding)` / `Err(binding)` (0.0.2: Result pattern; semantics
    /// resolved by typeck). Carries the whole pattern's span.
    ResultCtor {
        ctor: ResultCtor,
        binding: String,
        span: Span,
    },
}

impl Pattern {
    /// The source span of this pattern.
    pub fn span(&self) -> Span {
        match self {
            Pattern::Literal(e) => e.span(),
            Pattern::Ident(_, s) => *s,
            Pattern::ResultCtor { span, .. } => *span,
        }
    }
}

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Int(n, _) => write!(f, "{}", n),
            Expr::Float(n, _) => write!(f, "{}", n),
            Expr::Bool(b, _) => write!(f, "{}", b),
            Expr::Str(s, _) => write!(f, "\"{}\"", s),
            Expr::Ident(name, _) => write!(f, "{}", name),
            Expr::Add(lhs, rhs) => write!(f, "({} + {})", lhs, rhs),
            Expr::Sub(lhs, rhs) => write!(f, "({} - {})", lhs, rhs),
            Expr::Mul(lhs, rhs) => write!(f, "({} * {})", lhs, rhs),
            Expr::Div(lhs, rhs) => write!(f, "({} / {})", lhs, rhs),
            Expr::Mod(lhs, rhs) => write!(f, "({} % {})", lhs, rhs),
            Expr::Neg(e) => write!(f, "(-{})", e),
            Expr::Not(e) => write!(f, "(!{})", e),
            Expr::Move(e, _) => write!(f, "(move {})", e),
            Expr::Clone(e, _) => write!(f, "(clone {})", e),
            Expr::Box(e, _) => write!(f, "(box {})", e),
            Expr::Deref(e, _) => write!(f, "(deref {})", e),
            Expr::Cast(e, ty, _) => write!(f, "({} as {})", e, ty),
            Expr::Question(e, _) => write!(f, "({}?)", e),
            Expr::Await(e, _) => write!(f, "({} await)", e),
            Expr::ChannelNew { ty, capacity, .. } => write!(f, "channel<{}>({})", ty, capacity),
            Expr::Eq(lhs, rhs) => write!(f, "({} == {})", lhs, rhs),
            Expr::Ne(lhs, rhs) => write!(f, "({} != {})", lhs, rhs),
            Expr::Lt(lhs, rhs) => write!(f, "({} < {})", lhs, rhs),
            Expr::Gt(lhs, rhs) => write!(f, "({} > {})", lhs, rhs),
            Expr::Le(lhs, rhs) => write!(f, "({} <= {})", lhs, rhs),
            Expr::Ge(lhs, rhs) => write!(f, "({} >= {})", lhs, rhs),
            Expr::And(lhs, rhs) => write!(f, "({} and {})", lhs, rhs),
            Expr::Or(lhs, rhs) => write!(f, "({} or {})", lhs, rhs),
            Expr::Assign(lhs, rhs) => write!(f, "({} = {})", lhs, rhs),
            Expr::Call(func, args) => {
                write!(f, "{}({{}})", func)?;
                let args_str: Vec<String> = args.iter().map(|a| format!("{}", a)).collect();
                write!(f, "({})", args_str.join(", "))
            }
            Expr::Field(expr, field) => write!(f, "{}.{}", expr, field),
            Expr::StructLit(l) => write!(f, "{} {{ ... }}", l.name),
            Expr::Index(arr, idx, _) => write!(f, "{}[{}]", arr, idx),
            Expr::ArrayLit(l) => {
                write!(
                    f,
                    "[{}]",
                    l.elems
                        .iter()
                        .map(|e| e.to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            Expr::If(ExprIf { condition, .. }) => write!(f, "if {} {{ ... }}", condition),
            Expr::While(ExprWhile { condition, .. }) => {
                write!(f, "while {} {{ ... }}", condition)
            }
            Expr::For(ExprFor { var, iterable, .. }) => {
                write!(f, "for {} in {} {{ ... }}", var, iterable)
            }
            Expr::Choose(ExprChoose { scrutinee, .. }) => {
                write!(f, "choose {} {{ ... }}", scrutinee)
            }
            Expr::Block(_) => write!(f, "{{ ... }}"),
        }
    }
}
