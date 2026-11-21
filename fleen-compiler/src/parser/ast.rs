//! AST node definitions for the Fleen parser.
//!
//! Nodes are designed to match the EBNF grammar in `SYNTAX.ebnf`,
//! with naming per `SPEC.md §3`: `ExprIf` for `if_expr`, `StmtLet` for `let_stmt`, etc.

use crate::lexer::{Span, TokenKind};
use std::fmt;

/// A feature name a `#feature` directive may name (0.0.8 T01, D1).
///
/// `curry` is the only registered feature in 0.0.8; adding a second feature
/// later is additive — a new variant here, an arm in
/// [`FeatureSet::enable`], new gates where its effects live — and never a
/// syntax change to the directive itself. The enum (not a bare `&str`) keeps
/// the registered set, the spellings, and the per-feature effects from
/// drifting apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Feature {
    /// `#feature curry` (0.0.8): the curried two-part declaration form and
    /// implicit partial application.
    Curry,
}

impl Feature {
    /// Every registered feature, in registration order (drives the
    /// `UnknownFeature` diagnostic's registered-features list).
    pub const KNOWN: &'static [Feature] = &[Feature::Curry];

    /// The feature named by `name`, or `None` if it is not registered. The
    /// parser rejects an unknown name as a hard error (`UnknownFeature`) —
    /// a typoed feature must fail loudly, never be ignored.
    pub fn from_name(name: &str) -> Option<Feature> {
        match name {
            "curry" => Some(Feature::Curry),
            _ => None,
        }
    }

    /// The source spelling of the feature.
    pub fn name(self) -> &'static str {
        match self {
            Feature::Curry => "curry",
        }
    }
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Per-file feature set (0.0.8 T01, D1): which `#feature` directives the
/// file's header region declared.
///
/// The directive is unconditional grammar — the *effects* of a feature are
/// gated per file. The parser collects the set while parsing the header
/// region (the directive always precedes every item, so no pre-scan is
/// needed) and records it on [`Ast::features`]; later compiler stages consult
/// the current file's set through the `FileId → FeatureSet` table built from
/// the parse results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FeatureSet {
    /// `#feature curry` (0.0.8): the file opted into the curried two-part
    /// declaration form and implicit partial application.
    pub curry: bool,
}

impl FeatureSet {
    /// Record the feature into the set. The parser only calls this with a
    /// [`Feature`] it resolved from a directive; the exhaustive match makes
    /// adding a feature a compile error until its storage is added, so the
    /// registered names and the recorded effects cannot drift apart.
    pub fn enable(&mut self, feature: Feature) {
        match feature {
            Feature::Curry => self.curry = true,
        }
    }

    /// Whether `feature` was recorded in the set — the duplicate-directive
    /// check in the header region queries this before enabling.
    pub fn contains(&self, feature: Feature) -> bool {
        match feature {
            Feature::Curry => self.curry,
        }
    }
}

/// Top-level program.
#[derive(Debug, Clone, PartialEq)]
pub struct Ast {
    pub items: Vec<Item>,
    /// The file's `#feature` set (0.0.8 T01, D1) — empty (`default`) when
    /// the file carries no directive (every pre-0.0.8 file).
    pub features: FeatureSet,
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
    /// Top-level `break` statement (0.0.9 T03, D4). Legal to *parse* at the
    /// top level; typeck rejects it (`BreakOutsideLoop`) because no loop can
    /// enclose a top-level item.
    Break(Span),
    /// Top-level `continue` statement (0.0.9 T03, D4). Same shape as `Item::Break`.
    Continue(Span),
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
    /// Signature half of the two-part curried declaration (0.0.8 T02, D2):
    /// `add: (int, int) -> int` — recognized only in a `#feature curry` file.
    CurriedSignature(CurriedSignature),
    /// Body half of the two-part curried declaration (0.0.8 T02, D2):
    /// `add(a, b) { a + b }` — recognized only in a `#feature curry` file.
    CurriedBody(CurriedBody),
    /// `x: int = 42;` or `x = 42;`
    Var(VarBinding),
    /// `const y: int = 42;`
    Const(ConstDecl),
    /// `struct Point { x: int, y: int }` (0.0.3 V02)
    Struct(StructDecl),
    /// `enum Shape { Point, Circle(float) }` (0.0.6 T02)
    Enum(EnumDecl),
}

/// `enum` declaration (0.0.6 T02, extended 0.0.7 T01 D1): ordered variants,
/// each with a name plus 0+ payload types. `type_params` lists declaration-
/// local type parameters (`enum Option<T>` → one `TypeParam` named `T`);
/// empty for a monomorphic enum. Each parameter carries the span of its
/// identifier so registration diagnostics can point at the first occurrence
/// of a duplicate (0.0.7 T03 review).
#[derive(Debug, Clone, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
    pub is_pub: bool,
}

/// One declaration-local type parameter of an `enum` (0.0.7 T01 D1): the
/// parameter name plus the span of the identifier it was parsed from.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeParam {
    pub name: String,
    pub span: Span,
}

/// A single `enum` variant: `name` plus 0+ payload types.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub payload_types: Vec<Type>,
    pub span: Span,
}
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
    /// Whether the declaration was prefixed with `unsafe` (0.0.5 T02): the
    /// body may contain unsafe ops (`addr`, ptr read/write). Consumed by
    /// typeck's context tracking (T03).
    pub is_unsafe: bool,
    /// Whether the declaration was prefixed with `trusted` (0.0.5 T02): the
    /// author attests the body is safe despite containing unsafe ops.
    /// Consumed by typeck's context tracking (T03).
    pub is_trusted: bool,
}

/// Parameter in a function declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: Type,
    pub span: Span,
}

/// The `async`/`unsafe`/`trusted` modifier run (0.0.4 F1, 0.0.5 T02) —
/// shared by `func` declarations and the curried signature line (0.0.8 T02).
/// The order is free (`unsafe async func` == `async unsafe func`) and
/// repeats are rejected by the parser's modifier guard patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SignatureModifiers {
    pub is_async: bool,
    pub is_unsafe: bool,
    pub is_trusted: bool,
    /// The first modifier spelled in the source, for diagnostics that name
    /// the offending modifier (`ModifierOnCurriedBody`). `None` when no
    /// modifier was consumed.
    pub first: Option<&'static str>,
}

/// Signature half of a two-part curried declaration (0.0.8 T02, D2):
/// `[pub] [modifier] name : type` — the interface half. The type must be a
/// function type (`SignatureNeedsFunctionType` otherwise); the arrow-chain
/// notation itself lands with T03, so today the function type is spelled in
/// the legacy tuple form `(int, int) -> int` (and `() -> int` for zero
/// parameters).
///
/// The modifiers and `pub` are interface properties, so they live on this
/// half (D3); the parser rejects them on the body line.
#[derive(Debug, Clone, PartialEq)]
pub struct CurriedSignature {
    pub name: String,
    /// The declared type — always `Type::Func` (enforced at parse time).
    pub ty: Type,
    pub span: Span,
    /// Whether the signature was prefixed with `pub` (0.0.3 V05 shape).
    pub is_pub: bool,
    /// The `async`/`unsafe`/`trusted` modifier run (D3) — one shared struct
    /// with `FuncDecl`'s modifiers instead of a parallel bool clump.
    pub mods: SignatureModifiers,
}

/// One parameter name on the body half of a two-part curried declaration
/// (0.0.8 T02). The T02 card sketched `Vec<String>`; the span is carried so
/// the duplicate-parameter diagnostic can point at the second occurrence.
#[derive(Debug, Clone, PartialEq)]
pub struct CurriedParam {
    pub name: String,
    pub span: Span,
}

/// Body half of a two-part curried declaration (0.0.8 T02, D2):
/// `name ( params ) ( block | = expr )` — the parameter names live here, the
/// types live on the signature half. The body is a normal function body in
/// every other respect (ASI, last-expression return, `?`, `await`).
#[derive(Debug, Clone, PartialEq)]
pub struct CurriedBody {
    pub name: String,
    pub params: Vec<CurriedParam>,
    pub body: FuncBody,
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
    /// `ptr<T>` — a raw pointer (0.0.5 T02, D1/D2). Non-Copy, move-only;
    /// produced only by the unsafe op `addr`. There is no bare-`ptr` type.
    Ptr(Box<Type>),
    /// A named user type (0.0.3 V02), optionally applied to type arguments
    /// (0.0.7 T01, D1/D9). `args` is empty for a monomorphic name; both
    /// `Name<T>` and `Name[T]` spellings are accepted at use sites.
    /// Whether the name resolves (and arity matches) is typeck's business.
    Named {
        name: String,
        args: Vec<Type>,
        span: Span,
    },
}

impl Type {
    /// Render this type with the spelling the file's feature flag selects
    /// (0.0.8 T03, D4): a `#feature curry` file renders function types as
    /// right-associative arrow chains (`int -> int -> int`), a flag-off file
    /// keeps the tuple spelling (`Display`, `(int, int) -> int`). Only the
    /// spelling differs — both canonicalize to the same `Type::Func` (D4).
    ///
    /// `curry = false` is byte-identical to `Display`, so every existing
    /// error text is unchanged; the curry spelling is opt-in at each
    /// rendering site.
    pub fn render(&self, curry: bool) -> String {
        if !curry {
            return self.to_string();
        }
        match self {
            Type::Func(params, ret) => {
                let ret_atom = chain_atom(matches!(&**ret, Type::Func(..)), ret.render(true));
                let ps: Vec<String> = params
                    .iter()
                    .map(|p| chain_atom(matches!(p, Type::Func(..)), p.render(true)))
                    .collect();
                render_chain(&ps, ret_atom)
            }
            // Containers render their inner types with the same spelling so
            // a nested chain reads uniformly (`Result[int -> int, string]`).
            // The hardcoded `true` is safe: this arm is only reachable when
            // the `curry = false` early return above did not fire, so the
            // curry spelling is already the selected one.
            Type::Array(t) => format!("[{}]", t.render(true)),
            Type::Box(t) => format!("box<{}>", t.render(true)),
            Type::Ref(t) => format!("ref {}", t.render(true)),
            Type::Result(ok, err) => {
                format!("Result[{}, {}]", ok.render(true), err.render(true))
            }
            Type::Future(t) => format!("Future<{}>", t.render(true)),
            Type::Channel(t) => format!("channel<{}>", t.render(true)),
            Type::Ptr(t) => format!("ptr<{}>", t.render(true)),
            Type::Named { name, args, .. } => {
                if args.is_empty() {
                    name.clone()
                } else {
                    let a: Vec<String> = args.iter().map(|t| t.render(true)).collect();
                    format!("{name}<{}>", a.join(", "))
                }
            }
            // `Base` / `Range` have one spelling either way.
            other => other.to_string(),
        }
    }
}

/// Render a right-associative arrow chain (0.0.8 T03, D4): zero parameters
/// take the explicit `() -> T` spelling (never `unit -> T`), one or more
/// parameters join with ` -> `. Shared by `ast::Type::render` and
/// `typeck::Type::name_with` — the parser and typeck carry distinct IR
/// types, so each renders its own atoms and hands the strings here.
pub(crate) fn render_chain(params: &[String], ret_atom: String) -> String {
    if params.is_empty() {
        format!("() -> {}", ret_atom)
    } else {
        format!("{} -> {}", params.join(" -> "), ret_atom)
    }
}

/// Render one atom of an arrow chain: a function type needs parentheses in
/// parameter/return position (`(int -> int) -> int`), every other type
/// renders bare. Shared by the parser and typeck arrow-chain renderers.
pub(crate) fn chain_atom(is_func: bool, atom: String) -> String {
    if is_func { format!("({})", atom) } else { atom }
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
            Type::Named { name, args, .. } => {
                if args.is_empty() {
                    write!(f, "{name}")
                } else {
                    let arg_strs: Vec<String> = args.iter().map(|t| t.to_string()).collect();
                    write!(f, "{name}<{}>", arg_strs.join(", "))
                }
            }
            Type::Func(params, ret) => {
                let ps: Vec<String> = params.iter().map(|t| t.to_string()).collect();
                write!(f, "({},) -> {}", ps.join(", "), ret)
            }
            Type::Range => write!(f, "Range"),
            Type::Future(inner) => write!(f, "Future<{}>", inner),
            Type::Channel(inner) => write!(f, "channel<{}>", inner),
            Type::Ptr(inner) => write!(f, "ptr<{}>", inner),
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
    /// `break` statement (0.0.9 T03, D4): exit the innermost enclosing loop.
    /// A keyword statement, never an expression (`x = break` is a parse
    /// error); the span points at the keyword. The trailing `;` is optional
    /// (ASI) and carries no meaning — no value is discarded.
    Break(Span),
    /// `continue` statement (0.0.9 T03, D4): jump to the innermost loop's
    /// next-iteration edge. Same shape as [`Stmt::Break`].
    Continue(Span),
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
    /// `addr <place>` (0.0.5 T02: take the address of a place — an unsafe
    /// op, D2; the place shapes are validated by the resolver, the types by
    /// typeck). Operand parses at the `unary` level, so `addr p.f`,
    /// `addr a[i]` and `addr deref b` chain naturally.
    Addr(Box<Expr>, Span),
    /// `deref_ptr <unary>` (0.0.5 T02: read through a `ptr<T>`; the same
    /// spelling in assignment-target position writes through it). Kept
    /// distinct from `deref`, which stays box-only (PLAN §3.2).
    DerefPtr(Box<Expr>, Span),
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
            | Expr::Addr(_, span)
            | Expr::DerefPtr(_, span)
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
    /// Span of the loop-variable identifier itself: the diagnostic location
    /// for a reserved loop-variable name (0.0.10 T02 review — the
    /// whole-`for` span points too wide for a name-level error).
    pub var_span: Span,
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

/// Which Result constructor a pattern or constructor expression names.
///
/// 0.0.10 T02 (D1): `Ok` / `Err` are prelude variant-constructor names of
/// `std.result.Result`. The parser no longer produces Result shapes —
/// `Ok(p)` / `Err(p)` parse as ordinary constructor patterns (`Pattern::
/// Enum`) and `Ok(...)` / `Err(...)` stay ordinary calls — and the resolver
/// classifies them back onto `ResultCtor`-tagged nodes. The tag therefore
/// survives as the shared HIR / typed-HIR constructor marker (`PatternHir::
/// ResultCtor`, `TypedPatternHir::ResultCtor`, `TypedExprHir::ResultCtor`);
/// only the parser's dedicated `Pattern::ResultCtor` production is retired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultCtor {
    Ok,
    Err,
}

/// Pattern in a `choose` arm.
///
/// 0.0.7 T02: patterns become recursive. `Enum` payloads are full
/// `Pattern`s (nested, multi-binding); `Or` and `TypeGuard` are new.
/// 0.0.10 T02 (D1): `Ok(p)` / `Err(p)` parse through the ordinary `Enum`
/// constructor-pattern path — the resolver maps them back onto the
/// `ResultCtor`-tagged HIR nodes.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// Literal pattern (int, float, bool, string).
    Literal(Expr),
    /// Identifier pattern (binds variable, or `_` wildcard). Carries the pattern's span.
    Ident(String, Span),

    /// `Variant(p₁, …, pₙ)` enum-variant pattern (0.0.6; 0.0.7: payloads are full patterns).
    /// Zero-payload variants written bare are represented as `Ident` (parser), not `Enum`.
    Enum {
        name: String,
        payloads: Vec<Pattern>,
        span: Span,
    },
    /// `p₁ | p₂` (0.0.7 D6). Alternatives are full patterns.
    Or(Vec<Pattern>, Span),
    /// `pattern : T` (0.0.7 D7). Compile-time type ascription.
    TypeGuard {
        pattern: Box<Pattern>,
        ty: Type,
        span: Span,
    },
}

impl Pattern {
    /// The source span of this pattern.
    pub fn span(&self) -> Span {
        match self {
            Pattern::Literal(e) => e.span(),
            Pattern::Ident(_, s) => *s,
            Pattern::Enum { span, .. } => *span,
            Pattern::Or(_, span) => *span,
            Pattern::TypeGuard { span, .. } => *span,
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
            Expr::Addr(e, _) => write!(f, "(addr {})", e),
            Expr::DerefPtr(e, _) => write!(f, "(deref_ptr {})", e),
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
