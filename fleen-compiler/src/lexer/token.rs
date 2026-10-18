//! Token definitions for the Fleen lexer.

use std::fmt;

use crate::source_map::FileId;

/// A token with its kind and source span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
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

    // Literals
    /// Integer literal (e.g., `42`)
    IntLit(i64),
    /// Float literal (e.g., `3.14`)
    FloatLit(f64),
    /// String literal (e.g., `"hello"`)
    StringLit(String),
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
            TokenKind::Ident(s) => s.hash(state),
            _ => {}
        }
    }
}
