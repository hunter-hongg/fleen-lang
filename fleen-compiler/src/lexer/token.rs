//! Token definitions for the Fleen lexer.

use std::fmt;

use crate::source_map::FileId;

/// A token with its kind and source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// True only for the terminators the ASI pass *inserts* (0.0.8 T01).
    /// The inserted `Semi` is zero-width, but telling source and inserted
    /// apart by that span invariant leaks ASI's representation into the
    /// parser — this flag carries the distinction explicitly (e.g. the
    /// `#feature` directive accepts only the inserted terminator, never a
    /// source `;`).
    pub inserted_by_asi: bool,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self {
            kind,
            span,
            inserted_by_asi: false,
        }
    }

    /// A terminator the ASI pass inserted (0.0.8 T01): same shape as a
    /// source token, marked so the parser can tell it from the source's
    /// own `;`.
    pub fn asi_inserted(kind: TokenKind, span: Span) -> Self {
        Self {
            kind,
            span,
            inserted_by_asi: true,
        }
    }
}

/// Source code span (file id + byte offset range).
///
/// 0.0.3 V01: the span carries the file it belongs to
/// ([`FileId`]), so diagnostics can name their
/// file once programs span multiple files. The `Display` output stays
/// `start..end` on purpose — error-message formats are locked byte-for-byte
/// by the e2e fixtures; file names are resolved through a
/// [`SourceMap`](crate::source_map::SourceMap) at rendering time, never
/// inside `Display`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub file: FileId,
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        Self { file, start, end }
    }

    /// Sentinel span for synthesized code that has no source location
    /// (e.g. the glue instructions codegen builds around a synthesized
    /// `__init__`, builtin declarations).
    ///
    /// `u32::MAX` cannot be a real byte offset in a source file, so this
    /// is distinguishable from every parser-produced span. Consumers that
    /// map positions back to text must skip such spans rather than render
    /// them.
    pub const SYNTHETIC: Span = Span {
        file: FileId::SYNTHETIC,
        start: u32::MAX,
        end: u32::MAX,
    };

    /// True for [`Span::SYNTHETIC`] — a span with no source location. The
    /// file field is irrelevant for the check (`u32::MAX` cannot be a real
    /// byte offset).
    pub fn is_synthetic(&self) -> bool {
        self.start == u32::MAX
    }

    pub fn len(&self) -> u32 {
        self.end - self.start
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Deliberately file-less: see the type-level doc comment.
        write!(f, "{}..{}", self.start, self.end)
    }
}

/// One part of a string literal containing interpolation (0.0.10 T04, D4).
///
/// A string literal that contains the sequence `${` is scanned into parts:
/// literal chunks and embedded-expression token runs. Adjacent literal
/// chunks never occur — a chunk is flushed exactly when `${` or the closing
/// quote is reached. A string without `${` stays a plain
/// [`TokenKind::StringLit`], keeping its pre-0.0.10 compile path
/// byte-identical (PLAN D6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StringPart {
    /// A literal chunk: escapes already resolved (`\$` folded to `$`, D4).
    Lit(String),
    /// An embedded expression `${...}`: the tokens between the delimiters
    /// (delimiters excluded, brace depth resolved by the lexer), each
    /// carrying its absolute file span. `span` covers `${` through the
    /// matching `}` inclusive, so diagnostics can point at the whole
    /// interpolation even when the run is empty (`"${}"`).
    Expr { tokens: Vec<Token>, span: Span },
}

/// Token kinds in the Fleen language.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords
    /// `func` keyword
    Func,
    /// `const` keyword
    Const,
    /// `if` keyword
    If,
    /// `elif` keyword
    Elif,
    /// `else` keyword
    Else,
    /// `while` keyword
    While,
    /// `for` keyword (0.0.3 V04: iteration statement)
    For,
    /// `in` keyword (0.0.3 V04: `for x in expr` iterable separator)
    In,
    /// `choose` keyword
    Choose,
    /// `when` keyword
    When,
    /// `otherwise` keyword
    Otherwise,
    /// `import` keyword
    Import,
    /// `struct` keyword (0.0.3: user-defined aggregate types)
    Struct,
    /// `pub` keyword (0.0.3 V05: top-level visibility modifier)
    Pub,
    /// `true` literal
    True,
    /// `false` literal
    False,
    /// `int` type
    IntType,
    /// `float` type
    FloatType,
    /// `bool` type
    BoolType,
    /// `string` type
    StringType,
    /// `unit` type
    UnitType,
    /// `Result` type
    ResultType,
    /// `box` type
    BoxType,
    /// `ref` type
    RefType,
    /// `or` operator
    Or,
    /// `and` operator
    And,
    /// `not` / `!` operator
    Not,

    // Ownership keywords (0.0.2)
    /// `move` keyword (0.0.2: ownership transfer)
    Move,
    /// `clone` keyword (0.0.2: deep copy)
    Clone,
    /// `deref` keyword (0.0.2: box pointee read/write)
    Deref,
    /// `as` keyword (0.0.2: type cast, F8 — scalar → string only in 0.0.2)
    As,

    // Async keywords (0.0.4)
    /// `async` keyword (0.0.4: marks a `func` as producing a `Future<T>`)
    Async,
    /// `await` keyword (0.0.4: postfix operator that drives a `Future<T>`)
    Await,
    /// `channel` keyword (0.0.4: first-class channel type constructor)
    Channel,

    // Pointer / unsafe keywords (0.0.5) — reserved here only; the parser wires
    // `ptr<T>` (T02), the `addr` unary prefix (T02) and the `unsafe`/`trusted`
    // `func` modifiers (T02/T03), so misuse surfaces at the generic
    // statement/expr entry guards until then.
    // T02 memo: wiring `addr` as a unary prefix must also add it to
    // `can_start_stmt` (parser/asi.rs, alongside `box`/`deref`/`move`/`clone`)
    // and update the ASI `can_start_stmt_table` test — otherwise `addr x`
    // after a newline is rejected at the statement entry instead of parsing.
    /// `ptr` keyword (0.0.5: raw pointer type constructor `ptr<T>`; no bare-`ptr` type)
    Ptr,
    /// `addr` keyword (0.0.5: unsafe prefix operator taking the address of a place)
    Addr,
    /// `deref_ptr` keyword (0.0.5 T02: read/write through a `ptr<T>`).
    /// Deliberately *not* an overload of `deref` (which stays box-only, PLAN
    /// §3.2 L2 note): overloading would silently re-type existing safe call
    /// sites that might one day receive a ptr.
    DerefPtr,
    /// `unsafe` keyword (0.0.5: `func` modifier whose body may contain unsafe ops)
    Unsafe,
    /// `trusted` keyword (0.0.5: `func` modifier, author-attested safe wrapper)
    Trusted,

    // Loop-jump keywords (0.0.9 T03, D4) — reserved words, legal only as
    // keyword *statements* inside a loop body (typeck enforces the position;
    // the parser wires the keyword-statement shapes). They are not
    // expressions: `x = break` is a parse error at the expression entry.
    /// `break` keyword (0.0.9 T03, D4: exit the innermost enclosing loop)
    Break,
    /// `continue` keyword (0.0.9 T03, D4: jump to the innermost loop's
    /// next-iteration edge)
    Continue,

    // Enum keyword (0.0.6) — reserved here; the parser wires `enum_decl` and
    // the variant patterns in T02, so until then misuse surfaces at the
    // binding-name rejection (`enum = 1`) or as a plain "expected expression"
    // (`enum S { A }`).
    // Unlike `ptr`/`addr`/`unsafe`/`trusted` (which stayed out of
    // `can_start_stmt` until 0.0.5 T02), `enum` joins the statement-start set
    // immediately: the binding-name rejection
    // (`Parser::reject_keyword_binding_name`) sits *behind* the
    // statement-entry guard, so being statement-starting is what makes
    // `enum = 1` report the keyword instead of a statement-separator error.
    // T02 memo: `enum_decl` needs no further `can_start_stmt` change
    // (already statement-starting); `can_start_stmt_table` and
    // `implies_stmt_end_table` already assert it.
    /// `enum` keyword (0.0.6: tagged-union declaration introducer)
    Enum,

    // Literals
    /// Integer literal (e.g., `42`)
    IntLit(i64),
    /// Float literal (e.g., `3.14`)
    FloatLit(f64),
    /// String literal (e.g., `"hello"`)
    StringLit(String),
    /// String literal with interpolation (0.0.10 T04, D4): produced only
    /// when the literal contains `${`. A `$` in any other position, and a
    /// literal without `${` altogether, stays a plain [`TokenKind::StringLit`].
    StringInterp(Vec<StringPart>),
    /// Identifier (variable/function names)
    Ident(String),

    // Operators and punctuation
    /// `=`
    Assign,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `!`
    Bang,
    /// `?` (0.0.2: Result propagation suffix)
    Question,
    /// `|` (0.0.7 T02: or-patterns in `choose`)
    Pipe,
    /// Line break (0.0.2: consumed by the ASI pass, never reaches the parser;
    /// collapsed runs of blank lines, not emitted at file start or inside
    /// block comments — see docs/0.0.2/ASI.md §2)
    Newline,
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `:`
    Colon,
    /// `;`
    Semi,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `->`
    Arrow,
    /// `#` (0.0.8 T01: `#feature` directive introducer). Punctuation, not a
    /// keyword — `feature` itself lexes as an ordinary identifier, and the
    /// comment syntax (`//`, `/* */`) is untouched.
    Hash,
    /// `<`
    // (handled by Lt)
    /// `>`
    // (handled by Gt)

    // Special
    /// End of file
    Eof,
}

impl TokenKind {
    /// Returns true if this token is a keyword.
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenKind::Func
                | TokenKind::Const
                | TokenKind::If
                | TokenKind::Elif
                | TokenKind::Else
                | TokenKind::While
                | TokenKind::For
                | TokenKind::In
                | TokenKind::Choose
                | TokenKind::When
                | TokenKind::Otherwise
                | TokenKind::Import
                | TokenKind::Struct
                | TokenKind::Pub
                | TokenKind::True
                | TokenKind::False
                | TokenKind::IntType
                | TokenKind::FloatType
                | TokenKind::BoolType
                | TokenKind::StringType
                | TokenKind::UnitType
                | TokenKind::ResultType
                | TokenKind::BoxType
                | TokenKind::RefType
                | TokenKind::Or
                | TokenKind::And
                | TokenKind::Not
                | TokenKind::Move
                | TokenKind::Clone
                | TokenKind::Deref
                | TokenKind::As
                | TokenKind::Async
                | TokenKind::Await
                | TokenKind::Channel
                | TokenKind::Ptr
                | TokenKind::Addr
                | TokenKind::DerefPtr
                | TokenKind::Unsafe
                | TokenKind::Trusted
                | TokenKind::Break
                | TokenKind::Continue
                | TokenKind::Enum
        )
    }

    /// Returns the keyword string for this token kind, if it's a keyword.
    pub fn keyword_str(&self) -> Option<&'static str> {
        Some(match self {
            TokenKind::Func => "func",
            TokenKind::Const => "const",
            TokenKind::If => "if",
            TokenKind::Elif => "elif",
            TokenKind::Else => "else",
            TokenKind::While => "while",
            TokenKind::For => "for",
            TokenKind::In => "in",
            TokenKind::Choose => "choose",
            TokenKind::When => "when",
            TokenKind::Otherwise => "otherwise",
            TokenKind::Import => "import",
            TokenKind::Struct => "struct",
            TokenKind::Pub => "pub",
            TokenKind::True => "true",
            TokenKind::False => "false",
            TokenKind::IntType => "int",
            TokenKind::FloatType => "float",
            TokenKind::BoolType => "bool",
            TokenKind::StringType => "string",
            TokenKind::UnitType => "unit",
            TokenKind::ResultType => "Result",
            TokenKind::BoxType => "box",
            TokenKind::RefType => "ref",
            TokenKind::Or => "or",
            TokenKind::And => "and",
            TokenKind::Not => "not",
            TokenKind::Move => "move",
            TokenKind::Clone => "clone",
            TokenKind::Deref => "deref",
            TokenKind::As => "as",
            TokenKind::Async => "async",
            TokenKind::Await => "await",
            TokenKind::Channel => "channel",
            TokenKind::Ptr => "ptr",
            TokenKind::Addr => "addr",
            TokenKind::DerefPtr => "deref_ptr",
            TokenKind::Unsafe => "unsafe",
            TokenKind::Trusted => "trusted",
            TokenKind::Break => "break",
            TokenKind::Continue => "continue",
            TokenKind::Enum => "enum",
            _ => return None,
        })
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::IntLit(n) => write!(f, "{}", n),
            TokenKind::FloatLit(n) => write!(f, "{}", n),
            TokenKind::StringLit(s) => write!(f, "\"{}\"", s),
            // Load-bearing arm (see the `tokenize_enum_keyword` test): the
            // catch-all below renders keywords via `keyword_str` and panics
            // on a non-keyword variant that reaches it.
            // Render a fixed label, not the run contents: the embedded
            // tokens have already been consumed, and diagnostics only need
            // to name the construct ("expected expression, found …").
            TokenKind::StringInterp(_) => write!(f, "interpolated string"),
            TokenKind::Ident(s) => write!(f, "{}", s),
            TokenKind::Assign => write!(f, "="),
            TokenKind::Eq => write!(f, "=="),
            TokenKind::Ne => write!(f, "!="),
            TokenKind::Lt => write!(f, "<"),
            TokenKind::Gt => write!(f, ">"),
            TokenKind::Le => write!(f, "<="),
            TokenKind::Ge => write!(f, ">="),
            TokenKind::Plus => write!(f, "+"),
            TokenKind::Minus => write!(f, "-"),
            TokenKind::Star => write!(f, "*"),
            TokenKind::Slash => write!(f, "/"),
            TokenKind::Percent => write!(f, "%"),
            TokenKind::Bang => write!(f, "!"),
            TokenKind::Question => write!(f, "?"),
            TokenKind::Pipe => write!(f, "|"),
            TokenKind::Newline => write!(f, "newline"),
            TokenKind::Dot => write!(f, "."),
            TokenKind::Comma => write!(f, ","),
            TokenKind::Colon => write!(f, ":"),
            TokenKind::Semi => write!(f, ";"),
            TokenKind::LParen => write!(f, "("),
            TokenKind::RParen => write!(f, ")"),
            TokenKind::LBrace => write!(f, "{{"),
            TokenKind::RBrace => write!(f, "}}"),
            TokenKind::LBracket => write!(f, "["),
            TokenKind::RBracket => write!(f, "]"),
            TokenKind::Arrow => write!(f, "->"),
            TokenKind::Hash => write!(f, "#"),
            TokenKind::Eof => write!(f, "EOF"),
            // Keyword spellings are maintained only in keyword_str(); all
            // non-keyword variants are matched exhaustively above, so any
            // arm falling through here must be a keyword
            other => match other.keyword_str() {
                Some(kw) => f.write_str(kw),
                None => unreachable!("non-keyword variants are all matched above"),
            },
        }
    }
}

impl Eq for TokenKind {}

impl std::hash::Hash for TokenKind {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            TokenKind::IntLit(n) => n.hash(state),
            TokenKind::FloatLit(n) => n.to_bits().hash(state),
            TokenKind::StringLit(s) => s.hash(state),
            TokenKind::StringInterp(parts) => {
                // Hash literal chunks by content but each embedded run only
                // by its span (not its token contents): equal parts always
                // carry equal spans, so this stays Eq-consistent while
                // avoiding a full token-tree walk on every hash.
                for part in parts {
                    match part {
                        StringPart::Lit(s) => s.hash(state),
                        StringPart::Expr { span, .. } => span.hash(state),
                    }
                }
            }
            TokenKind::Ident(s) => s.hash(state),
            _ => {}
        }
    }
}
