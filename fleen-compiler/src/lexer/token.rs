//! Token definitions for the Fleen lexer.

use std::fmt;

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

/// Source code span (byte offset range).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
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
    /// `choose` keyword
    Choose,
    /// `when` keyword
    When,
    /// `otherwise` keyword
    Otherwise,
    /// `import` keyword
    Import,
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
                | TokenKind::Choose
                | TokenKind::When
                | TokenKind::Otherwise
                | TokenKind::Import
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
            TokenKind::Choose => "choose",
            TokenKind::When => "when",
            TokenKind::Otherwise => "otherwise",
            TokenKind::Import => "import",
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
            _ => return None,
        })
    }
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Func => write!(f, "func"),
            TokenKind::Const => write!(f, "const"),
            TokenKind::If => write!(f, "if"),
            TokenKind::Elif => write!(f, "elif"),
            TokenKind::Else => write!(f, "else"),
            TokenKind::While => write!(f, "while"),
            TokenKind::Choose => write!(f, "choose"),
            TokenKind::When => write!(f, "when"),
            TokenKind::Otherwise => write!(f, "otherwise"),
            TokenKind::Import => write!(f, "import"),
            TokenKind::True => write!(f, "true"),
            TokenKind::False => write!(f, "false"),
            TokenKind::IntType => write!(f, "int"),
            TokenKind::FloatType => write!(f, "float"),
            TokenKind::BoolType => write!(f, "bool"),
            TokenKind::StringType => write!(f, "string"),
            TokenKind::UnitType => write!(f, "unit"),
            TokenKind::ResultType => write!(f, "Result"),
            TokenKind::BoxType => write!(f, "box"),
            TokenKind::RefType => write!(f, "ref"),
            TokenKind::Or => write!(f, "or"),
            TokenKind::And => write!(f, "and"),
            TokenKind::Not => write!(f, "not"),
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
