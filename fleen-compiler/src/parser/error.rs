//! Parser error types.

use crate::lexer::{Span, TokenKind};
use std::fmt;

/// Parser error with span information.
#[derive(Debug, Clone, PartialEq)]
pub struct ParseError {
    pub kind: ParseErrorKind,
    pub span: Span,
}

/// Error kinds for parse errors.
#[derive(Debug, Clone, PartialEq)]
pub enum ParseErrorKind {
    /// Expected a specific token but found something else.
    Expected { expected: String, found: TokenKind },
    /// Statement boundary holds a token that can neither continue the
    /// previous statement nor start a new one (0.0.2 ASI, ASI.md §3).
    ExpectedSemiOrNewStmt { found: TokenKind },
    /// A struct field was written without its type (`Point { x, y }` field
    /// init shorthand, or `struct Point { x }`): fields are `name: type`.
    ExpectedFieldColon { field: String },
    /// Reached end of file unexpectedly.
    UnexpectedEof,
    /// `pub` used somewhere other than the top level (0.0.3 V05).
    PubNotAllowedHere,
    /// A `#feature` directive appears after the first item — in file-item
    /// position or inside a block body (0.0.8 T01, D1). The header region
    /// (before the first item) is the only legal placement.
    FeatureDirectiveMisplaced,
    /// `#feature` named a feature that is not registered (0.0.8 T01, D1).
    /// Hard error: a typoed feature name must fail loudly, never be ignored.
    UnknownFeature { name: String },
    /// The same feature was declared twice in one file (0.0.8 T01, D1).
    DuplicateFeature { name: String },
    /// The legacy `func`-keyword form in a `#feature curry` file (0.0.8
    /// T02, D2). The two-part form is the *only* function declaration form
    /// there — strictness by design; nested (block-level) `func` included.
    FuncFormForbiddenInCurryFile,
    /// A top-level `name : type` signature line in a `#feature curry` file
    /// whose type is not a function type (0.0.8 T02). A signature declares
    /// a function's interface; the type must be a function type.
    SignatureNeedsFunctionType { found: String },
    /// `pub` on the body line of a two-part declaration (0.0.8 T02, D3).
    /// Visibility is an interface property — it lives on the signature line.
    PubOnCurriedBody,
    /// `async`/`unsafe`/`trusted` on the body line of a two-part declaration
    /// (0.0.8 T02, D3). Modifiers are interface properties — they live on
    /// the signature line.
    ModifierOnCurriedBody { modifier: String },
    /// The same parameter name appears twice on one body line (0.0.8 T02,
    /// D3). Rejected at parse (the card's choice): the span points at the
    /// second occurrence, and the resolver only ever sees matched halves.
    DuplicateParam { name: String },
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParseErrorKind::Expected { expected, found } => {
                write!(f, "expected {}, found {}", expected, found)
            }
            ParseErrorKind::ExpectedSemiOrNewStmt { found } => {
                write!(
                    f,
                    "expected `;`, a continuation of the previous statement, \
                     or the start of a new statement, found {}",
                    found
                )
            }
            ParseErrorKind::ExpectedFieldColon { field } => {
                write!(
                    f,
                    "expected `:` after field name `{field}` — fields are \
                     `name: type` (no field-init shorthand)"
                )
            }
            ParseErrorKind::UnexpectedEof => {
                write!(f, "unexpected end of input")
            }
            ParseErrorKind::PubNotAllowedHere => {
                write!(
                    f,
                    "`pub` may only mark a top-level declaration (func, const, \
                     struct, or global binding)"
                )
            }
            ParseErrorKind::FeatureDirectiveMisplaced => {
                write!(
                    f,
                    "feature directive (`#feature`) is only allowed at the top \
                     of the file, before the first declaration"
                )
            }
            ParseErrorKind::UnknownFeature { name } => {
                // The registered list is derived from `Feature::KNOWN`, so a
                // newly registered feature advertises itself automatically.
                let registered = crate::parser::ast::Feature::KNOWN
                    .iter()
                    .map(|f| format!("`{f}`"))
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(
                    f,
                    "unknown feature `{name}` (registered features: {registered})"
                )
            }
            ParseErrorKind::DuplicateFeature { name } => {
                write!(f, "feature `{name}` is already enabled in this file")
            }
            ParseErrorKind::FuncFormForbiddenInCurryFile => {
                write!(
                    f,
                    "`func` declarations are not allowed in a `#feature curry` \
                     file — use the two-part form: a `name: type` signature \
                     line followed by a `name(params)` body line"
                )
            }
            ParseErrorKind::SignatureNeedsFunctionType { found } => {
                write!(
                    f,
                    "signature type must be a function type, found `{found}` — \
                     a two-part declaration declares a function's interface"
                )
            }
            ParseErrorKind::PubOnCurriedBody => {
                write!(
                    f,
                    "`pub` belongs on the signature line of a two-part \
                     declaration, not on the body line"
                )
            }
            ParseErrorKind::ModifierOnCurriedBody { modifier } => {
                write!(
                    f,
                    "`{modifier}` belongs on the signature line of a two-part \
                     declaration, not on the body line"
                )
            }
            ParseErrorKind::DuplicateParam { name } => {
                write!(f, "duplicate parameter name `{name}` in the body line")
            }
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "parse error: {} at {}", self.kind, self.span)
    }
}

impl std::error::Error for ParseError {}
