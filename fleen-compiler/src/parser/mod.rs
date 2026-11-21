//! Parser module: converts a token stream into an AST.
//!
//! Uses recursive descent + precedence climbing for expression parsing.

pub mod asi;
pub mod ast;
pub mod error;
pub mod expr;
#[cfg(test)]
#[path = "tests.rs"]
pub mod tests_mod;

pub use ast::{Feature, FeatureSet};
pub use error::{ParseError, ParseErrorKind};

use crate::lexer::{Span, Token};
use crate::source_map::FileId;
use ast::{Ast, Expr, SignatureModifiers};

/// One parsed `#feature` directive: the feature it names and the span of the
/// name token (for the `DuplicateFeature` diagnostic).
struct FeatureDirective {
    feature: Feature,
    name_span: Span,
}

/// Keywords that cannot be used as identifiers.
const KEYWORDS: &[&str] = &[
    "func",
    "const",
    "if",
    "elif",
    "else",
    "while",
    "choose",
    "when",
    "otherwise",
    "import",
    "struct",
    "true",
    "false",
    "int",
    "float",
    "bool",
    "string",
    "unit",
    "Result",
    "box",
    "ref",
    "or",
    "and",
    "not",
    "as",
];

/// Parse a token stream into an AST.
///
/// The stream first goes through the ASI pass (`asi::insert_semis`), which
/// consumes `Newline` tokens and inserts explicit `Semi` tokens at statement
/// boundaries (0.0.2, docs/0.0.2/ASI.md) — the parser proper sees a
/// 0.0.1-shaped stream.
///
/// # Arguments
/// - `tokens`: the token stream produced by the lexer
///
/// # Returns
/// - `Ok(Ast)`: parsing succeeded
/// - `Err(ParseError)`: parsing failed, with a Span and an error message
///
/// # Example
/// ```
/// use fleen_compiler::lexer::tokenize;
/// use fleen_compiler::parser::parse;
///
/// let tokens = tokenize("x = 42;").unwrap();
/// let ast = parse(tokens).unwrap();
/// ```
pub fn parse(tokens: Vec<Token>) -> Result<Ast, ParseError> {
    let tokens = asi::insert_semis(tokens)?;
    let mut parser = Parser::new(tokens);
    parser.parse_program()
}

/// Canonicalize the return side of an arrow into a parameter chain
/// (0.0.8 T03, D4): `a -> (b -> c)` becomes `Func([a, b], c)` — the arrow
/// chain is parse sugar over one flat `Type::Func`, never representational
/// currying. The merge only takes a return-side `Func` that itself carries
/// parameters: a zero-parameter function `() -> T` is a genuine function
/// *value* (`f: int -> (() -> int)` is a unary function over functions),
/// never a segment of the enclosing chain, so it stays nested.
fn merge_arrow_return(mut params: Vec<ast::Type>, ret: ast::Type) -> ast::Type {
    match ret {
        ast::Type::Func(mut rest, r) if !rest.is_empty() => {
            params.append(&mut rest);
            ast::Type::Func(params, r)
        }
        other => ast::Type::Func(params, Box::new(other)),
    }
}

/// The spelling of a keyword the parser accepts in a name position — the
/// enum-name position for `Result` (0.0.10 T01, D1) and the namespace
/// segments for `async` / `await` / `channel` (0.0.4). `TokenKind::
/// keyword_str` is the single spelling source; only keyword kinds reach
/// this helper, and `keyword_str` maps every keyword kind to `Some`.
fn keyword_name(kind: crate::lexer::TokenKind) -> String {
    match kind.keyword_str() {
        Some(name) => name.to_string(),
        None => unreachable!(
            "only keyword kinds are passed, and keyword_str maps every keyword kind to Some"
        ),
    }
}

/// Recursive descent parser with precedence climbing for expressions.
pub struct Parser {
    tokens: Vec<Token>,
    /// File every token in the stream was lexed from (0.0.3 V01); spans
    /// built by the parser carry it.
    file: FileId,
    pos: usize,
    /// Whether `ident {` may parse as a struct literal (0.0.3 V02).
    ///
    /// Disabled while parsing block-head expressions (`if`/`while`
    /// conditions, `choose` scrutinee, arm guards): there `ident {` must be
    /// the construct's block, never a literal — the Rust-2020 ambiguity.
    /// Block bodies always re-enable it.
    allow_struct_lit: bool,
    /// The file's `#feature` set (0.0.8 T01, D1), recorded while parsing the
    /// header region and consulted by the flag-gated productions (T02's
    /// two-part curried declarations; T03's arrow types). `default` until
    /// the header is parsed — every pre-0.0.8 file stays `default`.
    features: FeatureSet,
}

impl Parser {
    /// Create a new parser from a token stream. The stream's file id is
    /// taken from its first token (the lexer always emits at least EOF;
    /// the fallback only guards hand-built empty streams in tests).
    pub fn new(tokens: Vec<Token>) -> Self {
        let file = tokens.first().map_or(FileId(0), |t| t.span.file);
        Self {
            tokens,
            file,
            pos: 0,
            allow_struct_lit: true,
            features: FeatureSet::default(),
        }
    }

    /// Build a span in the parsed file from raw byte offsets.
    fn span(&self, start: u32, end: u32) -> Span {
        Span::new(self.file, start, end)
    }

    /// Get a reference to the current token.
    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    /// Get the span end of the previous token.
    fn prev_span_end(&self) -> u32 {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span.end
        } else {
            0
        }
    }

    /// Get the full span of the previous token.
    fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            self.span(0, 0)
        }
    }

    /// Get the kind of the current token.
    fn current_kind(&self) -> &crate::lexer::TokenKind {
        &self.current().kind
    }

    /// Get the kind of the token `n` positions ahead of the current one
    /// (0 = current), or `None` past the end of the stream.
    fn peek_kind(&self, n: usize) -> Option<&crate::lexer::TokenKind> {
        self.tokens.get(self.pos + n).map(|t| &t.kind)
    }

    /// Advance to the next token, returning the current token.
    fn advance(&mut self) -> &Token {
        let token = &self.tokens[self.pos];
        if !self.is_at_end() {
            self.pos += 1;
        }
        token
    }

    /// Check if we've reached the end (current token is EOF).
    fn is_at_end(&self) -> bool {
        matches!(self.current_kind(), crate::lexer::TokenKind::Eof)
    }

    /// Check if the current token matches the given kind.
    fn check(&self, kind: &crate::lexer::TokenKind) -> bool {
        match (self.current_kind(), kind) {
            (crate::lexer::TokenKind::Ident(_), crate::lexer::TokenKind::Ident(_)) => true,
            (a, b) => a == b,
        }
    }

    /// If the current token matches, consume it and return true.
    /// Otherwise, return false without consuming.
    fn matches(&mut self, kind: &crate::lexer::TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume the current token if it matches `kind`, otherwise return an error.
    fn expect(&mut self, kind: &crate::lexer::TokenKind, msg: &str) -> Result<(), ParseError> {
        if self.check(kind) {
            self.advance();
            Ok(())
        } else {
            let found = self.current().kind.clone();
            let span = self.current().span;
            Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: msg.to_string(),
                    found,
                },
                span,
            })
        }
    }

    /// Error at the current token: a statement boundary was required here
    /// (explicit or ASI-inserted `;`) but the token is something else.
    fn err_expected_stmt_end(&self) -> ParseError {
        ParseError {
            kind: ParseErrorKind::ExpectedSemiOrNewStmt {
                found: self.current().kind.clone(),
            },
            span: self.current().span,
        }
    }

    /// Check if the given identifier is a keyword.
    fn check_keyword(&self, name: &str) -> bool {
        KEYWORDS.contains(&name)
    }

    /// Reject a reserved keyword used as a binding name at statement start
    /// (e.g. `move = 1;`). 0.0.2 makes `move` / `clone` / `deref` reserved
    /// words; erroring here (instead of falling into prefix-expression
    /// parsing) gives an actionable message instead of "expected expression".
    fn reject_keyword_binding_name(&self) -> Result<(), ParseError> {
        let keyword = match self.current_kind() {
            crate::lexer::TokenKind::Move => "move",
            crate::lexer::TokenKind::Clone => "clone",
            crate::lexer::TokenKind::Deref => "deref",
            crate::lexer::TokenKind::BoxType => "box",
            crate::lexer::TokenKind::RefType => "ref",
            crate::lexer::TokenKind::As => "as",
            // 0.0.5 T02: the new reserved words cannot be binding names
            // either (`addr = 1;`).
            crate::lexer::TokenKind::Ptr => "ptr",
            crate::lexer::TokenKind::Addr => "addr",
            crate::lexer::TokenKind::DerefPtr => "deref_ptr",
            crate::lexer::TokenKind::Unsafe => "unsafe",
            crate::lexer::TokenKind::Trusted => "trusted",
            // 0.0.9 T03 (D4): the loop-jump keywords are reserved words
            // (`break = 1` names the keyword, not a statement-separator
            // error) — the same path as the 0.0.5/0.0.6 words.
            crate::lexer::TokenKind::Break => "break",
            crate::lexer::TokenKind::Continue => "continue",
            // 0.0.6 T01: `enum` joins the same path. It is statement-starting
            // from day one (unlike the 0.0.5 words before their T02), so
            // `enum = 1` reaches this rejection instead of the ASI
            // statement-separator error — the message names the keyword.
            crate::lexer::TokenKind::Enum => "enum",
            _ => return Ok(()),
        };
        let next_starts_binding = matches!(
            self.peek_kind(1),
            Some(crate::lexer::TokenKind::Assign) | Some(crate::lexer::TokenKind::Colon)
        );
        if next_starts_binding {
            Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: format!(
                        "identifier (keyword `{}` cannot be used as identifier)",
                        keyword
                    ),
                    found: self.current().kind.clone(),
                },
                span: self.current().span,
            })
        } else {
            Ok(())
        }
    }

    /// Reject a type annotation on a `deref` / `deref_ptr` assignment target
    /// (`deref b: int = v`, `deref_ptr p: int = v`). Only plain bindings may
    /// be annotated (DESIGN.md §3.4); erroring here gives an actionable
    /// message instead of a generic "expected `;`" at the colon.
    fn reject_deref_assign_type_annot(&self) -> Result<(), ParseError> {
        let (kw, target) = match self.current_kind() {
            crate::lexer::TokenKind::Deref => ("deref", "`b: box<int> = box v`"),
            crate::lexer::TokenKind::DerefPtr => ("deref_ptr", "`p: ptr<int> = addr x`"),
            _ => return Ok(()),
        };
        if matches!(self.peek_kind(1), Some(crate::lexer::TokenKind::Ident(_)))
            && matches!(self.peek_kind(2), Some(crate::lexer::TokenKind::Colon))
        {
            let found = self
                .peek_kind(2)
                .cloned()
                .unwrap_or(crate::lexer::TokenKind::Eof);
            let span = self.tokens[self.pos + 2].span;
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: format!(
                        "`=` (a `{kw}` assignment target cannot carry a type \
                               annotation; annotate the binding instead, e.g. \
                               `{target}`)"
                    ),
                    found,
                },
                span,
            });
        }
        Ok(())
    }

    /// Parse the entire program.
    fn parse_program(&mut self) -> Result<Ast, ParseError> {
        // P1: Handle empty token stream (only EOF)
        if self.tokens.is_empty() || (self.tokens.len() == 1 && self.is_at_end()) {
            return Ok(Ast {
                items: vec![],
                features: FeatureSet::default(),
                span: self.span(0, 0),
            });
        }

        let start = self.current().span.start;

        // 0.0.8 T01 (D1): the header region — `#feature` directives before
        // the first item. Comments and blank lines do not end the region
        // (they produce no item); any item does. A `Hash` seen later is
        // rejected by `parse_item` / `parse_block` as `FeatureDirectiveMisplaced`.
        let mut features = FeatureSet::default();
        while self.check(&crate::lexer::TokenKind::Hash) {
            let directive = self.parse_feature_directive()?;
            // The set itself records what was seen — no parallel "seen"
            // list: `contains` before `enable` is the duplicate check.
            if features.contains(directive.feature) {
                return Err(ParseError {
                    kind: ParseErrorKind::DuplicateFeature {
                        name: directive.feature.name().to_string(),
                    },
                    span: directive.name_span,
                });
            }
            features.enable(directive.feature);
        }
        // The header precedes every item, so from here on the flag-gated
        // productions (T02/T03) can consult the file's set.
        self.features = features;

        let mut items = Vec::new();

        while !self.is_at_end() {
            // Statement-entry guard (ASI.md §4 #4): a token that can neither
            // start a statement nor close anything is reported here instead
            // of surfacing as a generic "expected expression" deep inside
            // primary parsing (e.g. a stray `else`, `;`, or `)`).
            if !asi::can_start_stmt(self.current_kind()) {
                return Err(self.err_expected_stmt_end());
            }
            let item = self.parse_item()?;
            items.push(item);
        }

        let end = self.tokens.last().map(|t| t.span.end).unwrap_or(start);
        Ok(Ast {
            items,
            features,
            span: self.span(start, end),
        })
    }

    /// Parse a `#feature` directive (0.0.8 T01, D1): `# , "feature" , ident`,
    /// newline-terminated. The ASI pass inserts a *zero-width* `Semi` at the
    /// end of the directive line (the name is a statement-ender and the next
    /// line's token a statement-starter), and at EOF; the parser requires it.
    /// A `Semi` with a real span comes from an explicit `;` in the source —
    /// rejected, because "directive lines take no semicolon": only the
    /// newline terminates a directive. A directive joined onto one line with
    /// anything else also fails here. Returns the resolved feature and the
    /// span of its name token (for the `DuplicateFeature` diagnostic).
    fn parse_feature_directive(&mut self) -> Result<FeatureDirective, ParseError> {
        self.advance(); // consume `#`
        // `feature` is not a keyword, and `Parser::check` matches any
        // `Ident` against any `Ident`, so the word must be compared by text.
        let word = self.parse_ident()?;
        if word != "feature" {
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "`feature` after `#`".to_string(),
                    found: crate::lexer::TokenKind::Ident(word),
                },
                span: self.prev_span(),
            });
        }
        let name = self.parse_ident()?;
        let name_span = self.prev_span();
        if self.check(&crate::lexer::TokenKind::Semi) {
            // The ASI pass inserts the line's terminator (`Semi` marked
            // `inserted_by_asi`); a source `;` is never marked — rejected,
            // because "directive lines take no semicolon": only the newline
            // terminates a directive.
            let current = self.current();
            if !current.inserted_by_asi {
                return Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: "a newline after the `#feature` directive \
                                   (a directive takes no semicolon)"
                            .to_string(),
                        found: crate::lexer::TokenKind::Semi,
                    },
                    span: current.span,
                });
            }
            self.advance();
        } else {
            let found = self.current().kind.clone();
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "a newline after the `#feature` directive (a \
                               directive is a whole line)"
                        .to_string(),
                    found,
                },
                span: self.current().span,
            });
        }
        let feature = Feature::from_name(&name).ok_or_else(|| ParseError {
            kind: ParseErrorKind::UnknownFeature { name: name.clone() },
            span: name_span,
        })?;
        Ok(FeatureDirective { feature, name_span })
    }

    /// A `Hash` in item position or inside a block body is a misplaced
    /// `#feature` directive (0.0.8 T01, D1) — shared by `parse_item` and the
    /// block-statement parser, which reject it like they reject `pub`.
    fn err_feature_misplaced(&self) -> ParseError {
        ParseError {
            kind: ParseErrorKind::FeatureDirectiveMisplaced,
            span: self.current().span,
        }
    }

    /// Parse a top-level item: import, declaration, or expression statement.
    fn parse_item(&mut self) -> Result<ast::Item, ParseError> {
        // 0.0.8 T01 (D1): a `#` after the first item is a hard error, never
        // a comment, never a no-op — the "magic statement" rule. The header
        // region (before the first item) is the only legal placement.
        if self.check(&crate::lexer::TokenKind::Hash) {
            return Err(self.err_feature_misplaced());
        }

        // 0.0.3 V05: `pub` is a top-level visibility modifier. It is parsed
        // here, never inside block statements (the block parser rejects it).
        let is_pub = self.matches(&crate::lexer::TokenKind::Pub);

        if self.check(&crate::lexer::TokenKind::Import) {
            if is_pub {
                return Err(ParseError {
                    kind: ParseErrorKind::PubNotAllowedHere,
                    span: self.prev_span(),
                });
            }
            return self.parse_import();
        }

        if self.check(&crate::lexer::TokenKind::Func) {
            // 0.0.8 T02 (D2): in a `#feature curry` file the legacy
            // `func`-keyword form is rejected — the two-part form is the
            // only function declaration form there.
            if self.features.curry {
                return Err(self.err_func_form_forbidden());
            }
            let mut decl = self.parse_func_decl()?;
            decl.is_pub = is_pub;
            return Ok(ast::Item::Decl(ast::Decl::Func(decl)));
        }

        // 0.0.4 F1: `async func` — the `async` modifier introduces a func
        // declaration (so it is statement-startable, unlike `await`).
        // 0.0.5 T02: `unsafe func` / `trusted func` join the same shape.
        if self.check(&crate::lexer::TokenKind::Async)
            || self.check(&crate::lexer::TokenKind::Unsafe)
            || self.check(&crate::lexer::TokenKind::Trusted)
        {
            // 0.0.8 T02 (D3): in a `#feature curry` file the modifiers
            // prefix the *signature* line (`async half: ...`), never the
            // legacy form — the signature parser rejects `modifier func`
            // and `modifier name(...)` with the dedicated diagnostics.
            if self.features.curry {
                return self.parse_curried_signature(is_pub);
            }
            let mut decl = self.parse_func_decl()?;
            decl.is_pub = is_pub;
            return Ok(ast::Item::Decl(ast::Decl::Func(decl)));
        }

        if self.check(&crate::lexer::TokenKind::Const) {
            let mut decl = self.parse_const_decl()?;
            decl.is_pub = is_pub;
            return Ok(ast::Item::Decl(ast::Decl::Const(decl)));
        }

        if self.check(&crate::lexer::TokenKind::Struct) {
            let mut decl = self.parse_struct_decl()?;
            decl.is_pub = is_pub;
            return Ok(ast::Item::Decl(ast::Decl::Struct(decl)));
        }

        if self.check(&crate::lexer::TokenKind::Enum) {
            let mut decl = self.parse_enum_decl()?;
            decl.is_pub = is_pub;
            return Ok(ast::Item::Decl(ast::Decl::Enum(decl)));
        }

        // Check for variable binding: ident followed by : or =
        if self.check(&crate::lexer::TokenKind::Ident(String::new())) {
            let next_kind = self.peek_kind(1);

            // 0.0.8 T02 (D2): in a `#feature curry` file, `ident (` whose
            // matching `)` is followed by `{` or `=` is the *body line* of a
            // two-part declaration. Top-level call statements (`f(1);`) stay
            // legal — the scan-ahead in `curried_body_ahead` keeps the two
            // apart. `pub` on the body line is rejected (D3): visibility
            // lives on the signature line.
            if self.features.curry
                && matches!(next_kind, Some(crate::lexer::TokenKind::LParen))
                && self.curried_body_ahead()
            {
                if is_pub {
                    return Err(ParseError {
                        kind: ParseErrorKind::PubOnCurriedBody,
                        span: self.prev_span(),
                    });
                }
                let decl = self.parse_curried_body()?;
                return Ok(ast::Item::Decl(ast::Decl::CurriedBody(decl)));
            }

            let is_binding = matches!(
                next_kind,
                Some(crate::lexer::TokenKind::Colon) | Some(crate::lexer::TokenKind::Assign)
            );
            // 0.0.8 T02 (D2): in a `#feature curry` file a top-level
            // `ident :` is the *signature line* — unless an `=` follows the
            // type, which keeps it an annotated variable binding. The
            // signature parser reports `SignatureNeedsFunctionType` for a
            // non-function type (D3's strictness: in a flag-on file the
            // bare `name : type` production *is* the signature).
            if self.features.curry && matches!(next_kind, Some(crate::lexer::TokenKind::Colon)) {
                return self.parse_curried_sig_or_binding(is_pub);
            }
            if is_binding {
                // P3: Check if identifier is a keyword
                if let crate::lexer::TokenKind::Ident(name) = self.current_kind()
                    && self.check_keyword(name)
                {
                    let found = self.current().kind.clone();
                    let span = self.current().span;
                    return Err(ParseError {
                        kind: ParseErrorKind::Expected {
                            expected: format!(
                                "identifier (keyword `{}` cannot be used as identifier)",
                                name
                            ),
                            found,
                        },
                        span,
                    });
                }
                let mut decl = self.parse_var_binding()?;
                decl.is_pub = is_pub;
                return Ok(ast::Item::Decl(ast::Decl::Var(decl)));
            }
        }

        // `pub` must be followed by a declaration. Reaching here with the
        // modifier consumed means the next token is not a valid decl start.
        if is_pub {
            let found = self.current().kind.clone();
            let span = self.current().span;
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "declaration after `pub`".to_string(),
                    found,
                },
                span,
            });
        }

        // `break` / `continue` statements (0.0.9 T03, D4): keyword
        // statements, parsed at the top level too. Whether one is *legal*
        // there is typeck's business (`BreakOutsideLoop` /
        // `ContinueOutsideLoop` — no loop can enclose a top-level item), so
        // the parser accepts the shape everywhere statements are accepted.
        if self.check(&crate::lexer::TokenKind::Break)
            || self.check(&crate::lexer::TokenKind::Continue)
        {
            return self.parse_break_like_item();
        }

        // Expression statement at top level.
        // The ASI pass has inserted a `Semi` at every complete statement
        // boundary, so a missing `;` here means same-line juxtaposition
        // (`f() g()`) — rejected per the newline-sensitive semantics.
        self.reject_keyword_binding_name()?;
        self.reject_deref_assign_type_annot()?;
        let expr = self.parse_expr()?;
        if self.matches(&crate::lexer::TokenKind::Semi) {
            Ok(ast::Item::Expr(expr))
        } else {
            Err(self.err_expected_stmt_end())
        }
    }

    /// Parse an import declaration.
    fn parse_import(&mut self) -> Result<ast::Item, ParseError> {
        let start = self.current().span.start;
        self.expect(&crate::lexer::TokenKind::Import, "`import`")?;

        // Parse module path: segment . segment . ... Each segment is an
        // identifier or (0.0.4) a keyword-named module member such as
        // `std.async` (see `parse_namespace_segment`).
        let mut path = Vec::new();
        loop {
            path.push(self.parse_namespace_segment()?);

            if self.check(&crate::lexer::TokenKind::Dot) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(&crate::lexer::TokenKind::Semi, "`;` after import")?;

        let end = self.prev_span_end();
        Ok(ast::Item::Import(ast::ImportDecl {
            path,
            span: self.span(start, end),
        }))
    }

    /// Parse a struct declaration (0.0.3 V02):
    /// `struct Name { field: type, ... }` — top-level only, no trailing
    /// semicolon (same rule as `func`; the ASI pass may insert one after
    /// the closing `}`, which is tolerated here).
    fn parse_struct_decl(&mut self) -> Result<ast::StructDecl, ParseError> {
        let start = self.current().span.start;
        self.expect(&crate::lexer::TokenKind::Struct, "`struct`")?;
        let name = self.parse_ident()?;
        self.expect(&crate::lexer::TokenKind::LBrace, "`{` after struct name")?;

        let mut fields = Vec::new();
        if !self.check(&crate::lexer::TokenKind::RBrace) {
            loop {
                let fstart = self.current().span.start;
                let fname = self.parse_ident()?;
                if !self.matches(&crate::lexer::TokenKind::Colon) {
                    return Err(ParseError {
                        kind: ParseErrorKind::ExpectedFieldColon {
                            field: fname.clone(),
                        },
                        span: self.current().span,
                    });
                }
                let ty = self.parse_type()?;
                let fend = self.prev_span_end();
                fields.push(ast::StructField {
                    name: fname,
                    ty,
                    span: self.span(fstart, fend),
                });
                if !self.matches(&crate::lexer::TokenKind::Comma) {
                    break;
                }
                // Trailing comma allowed (`y: int,}`).
                if self.check(&crate::lexer::TokenKind::RBrace) {
                    break;
                }
            }
        }

        self.expect(&crate::lexer::TokenKind::RBrace, "`}` after struct fields")?;
        let _ = self.matches(&crate::lexer::TokenKind::Semi);

        let end = self.prev_span_end();
        Ok(ast::StructDecl {
            name,
            fields,
            span: self.span(start, end),
            is_pub: false,
        })
    }

    /// Parse an `enum` declaration (0.0.6 T02, extended 0.0.7 T01 D1):
    /// `enum Name [ "<" ident { "," ident } [ "," ] ">" ] "{" variant, ... "}"`
    /// — top-level only. `type_params` are declaration-local identifiers;
    /// empty `<>` is a parse error.
    fn parse_enum_decl(&mut self) -> Result<ast::EnumDecl, ParseError> {
        let start = self.current().span.start;
        self.expect(&crate::lexer::TokenKind::Enum, "`enum`")?;
        // 0.0.10 T01 (D1): `Result` stays a lexer keyword, but the enum-name
        // position accepts it so the std declaration (`enum Result<T, E>` in
        // std/result.fln) parses through the ordinary parameterized-enum
        // frontend. Same shape as the 0.0.4 `parse_namespace_segment`
        // precedent (`async`/`await`/`channel` legal in namespace positions
        // only): every other identifier position still rejects the keyword,
        // and the resolver rejects an `enum Result` outside `std.result`.
        let name = match self.current_kind() {
            crate::lexer::TokenKind::ResultType => {
                self.advance();
                keyword_name(crate::lexer::TokenKind::ResultType)
            }
            _ => self.parse_ident()?,
        };
        let type_params = if self.check(&crate::lexer::TokenKind::Lt) {
            self.parse_enum_type_params()?
        } else {
            Vec::new()
        };
        self.expect(&crate::lexer::TokenKind::LBrace, "`{` after enum name")?;
        let mut variants = Vec::new();
        if !self.check(&crate::lexer::TokenKind::RBrace) {
            loop {
                // The EBNF requires at least one variant
                // (`enum_decl = "enum" ident "{" variant { "," variant } [ "," ] "}"`).
                let vstart = self.current().span.start;
                let vname = self.parse_ident()?;
                let mut payload_types = Vec::new();
                if self.matches(&crate::lexer::TokenKind::LParen) {
                    // `variant = ident, [ "(" , type , { "," , type } , ")" ]`.
                    // Empty parens (`Foo()`) are a grammar violation: the
                    // ident inside `(` `)` is mandatory.
                    if self.check(&crate::lexer::TokenKind::RParen) {
                        return Err(ParseError {
                            kind: ParseErrorKind::Expected {
                                expected: "variant payload".to_string(),
                                found: self.current().kind.clone(),
                            },
                            span: self.current().span,
                        });
                    }
                    loop {
                        let ptype = self.parse_type()?;
                        payload_types.push(ptype);
                        if !self.matches(&crate::lexer::TokenKind::Comma) {
                            break;
                        }
                        // Trailing comma allowed (`A, B,`).
                        if self.check(&crate::lexer::TokenKind::RParen) {
                            break;
                        }
                    }
                    self.expect(
                        &crate::lexer::TokenKind::RParen,
                        "`)` after enum variant payload types",
                    )?;
                }
                let vend = self.prev_span_end();
                variants.push(ast::EnumVariant {
                    name: vname,
                    payload_types,
                    span: self.span(vstart, vend),
                });
                if !self.matches(&crate::lexer::TokenKind::Comma) {
                    break;
                }
                // Trailing comma allowed (`A, B,`).
                if self.check(&crate::lexer::TokenKind::RBrace) {
                    break;
                }
            }
        }
        if variants.is_empty() {
            // The EBNF requires at least one variant; an empty body is not
            // the struct rule (`struct S {}` is legal) but an enum error.
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "variant (an enum needs at least one variant)".to_string(),
                    found: self.current().kind.clone(),
                },
                span: self.current().span,
            });
        }
        self.expect(&crate::lexer::TokenKind::RBrace, "`}` after enum variants")?;
        let _ = self.matches(&crate::lexer::TokenKind::Semi);

        let end = self.prev_span_end();
        Ok(ast::EnumDecl {
            name,
            type_params,
            variants,
            span: self.span(start, end),
            is_pub: false,
        })
    }

    /// Parse `enum` type parameters: `<` ident { "," ident } [ "," ] `>`.
    /// Requires at least one parameter; an empty `<>` is a parse error.
    /// Each parameter records the span of its identifier (0.0.7 T03: the
    /// resolver's duplicate-parameter diagnostic points at the first
    /// occurrence).
    fn parse_enum_type_params(&mut self) -> Result<Vec<ast::TypeParam>, ParseError> {
        self.expect(&crate::lexer::TokenKind::Lt, "`<` in enum type parameters")?;
        if self.check(&crate::lexer::TokenKind::Gt) {
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected:
                        "type parameter (an enum type parameter list needs at least one parameter)"
                            .to_string(),
                    found: self.current().kind.clone(),
                },
                span: self.current().span,
            });
        }
        let mut params = Vec::new();
        loop {
            let pstart = self.current().span.start;
            let p = self.parse_ident()?;
            let pend = self.prev_span_end();
            if matches!(
                p.as_str(),
                "int" | "float" | "bool" | "string" | "unit" | "char"
            ) {
                return Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: "type parameter name (must not shadow a base type name)"
                            .to_string(),
                        found: crate::lexer::TokenKind::Ident(p.clone()),
                    },
                    span: self.current().span,
                });
            }
            params.push(ast::TypeParam {
                name: p,
                span: self.span(pstart, pend),
            });
            if !self.matches(&crate::lexer::TokenKind::Comma) {
                break;
            }
            if self.check(&crate::lexer::TokenKind::Gt) {
                break; // trailing comma
            }
        }
        self.expect(
            &crate::lexer::TokenKind::Gt,
            "`>` after enum type parameters",
        )?;
        Ok(params)
    }

    /// Parse optional type arguments after a named type: `<T, U>` or `[T, U]`
    /// (0.0.7 T01, D1/D9). Returns empty vec if neither delimiter follows.
    fn parse_optional_type_args(&mut self) -> Result<Vec<ast::Type>, ParseError> {
        let (open, close, close_msg) = if self.check(&crate::lexer::TokenKind::Lt) {
            (
                crate::lexer::TokenKind::Lt,
                crate::lexer::TokenKind::Gt,
                "`>` after type arguments",
            )
        } else if self.check(&crate::lexer::TokenKind::LBracket) {
            (
                crate::lexer::TokenKind::LBracket,
                crate::lexer::TokenKind::RBracket,
                "`]` after type arguments",
            )
        } else {
            return Ok(Vec::new());
        };
        self.expect(&open, "type argument list")?;
        if self.check(&close) {
            // Empty `Name<>` / `Name[]` in type position — reject as a
            // parse error (requires at least one type argument).
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "type argument (type argument list needs at least one type)"
                        .to_string(),
                    found: self.current().kind.clone(),
                },
                span: self.current().span,
            });
        }
        let mut args = Vec::new();
        loop {
            args.push(self.parse_type()?);
            if !self.matches(&crate::lexer::TokenKind::Comma) {
                break;
            }
            if self.check(&close) {
                break; // trailing comma
            }
        }
        self.expect(&close, close_msg)?;
        Ok(args)
    }

    /// Parse a const declaration: `const ident [: type] = expr ;`
    fn parse_const_decl(&mut self) -> Result<ast::ConstDecl, ParseError> {
        let start = self.current().span.start;
        self.expect(&crate::lexer::TokenKind::Const, "`const`")?;

        let name = self.parse_ident()?;
        let ty = if self.matches(&crate::lexer::TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(
            &crate::lexer::TokenKind::Assign,
            "`=` after const declaration",
        )?;

        let init = self.parse_expr()?;

        if self.check(&crate::lexer::TokenKind::RBrace) {
            // ASI (ASI.md §3.4): `const` immediately before `}` needs no `;`.
        } else if !self.matches(&crate::lexer::TokenKind::Semi) {
            return Err(self.err_expected_stmt_end());
        }

        let end = self.prev_span_end();
        Ok(ast::ConstDecl {
            name,
            ty,
            init: Box::new(init),
            span: self.span(start, end),
            is_pub: false,
        })
    }

    /// Parse a variable binding: `ident [: type] = expr ;`
    fn parse_var_binding(&mut self) -> Result<ast::VarBinding, ParseError> {
        let start = self.current().span.start;
        let name = self.parse_ident()?;

        let ty = if self.matches(&crate::lexer::TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.finish_var_binding(name, ty, start)
    }

    /// The `= expr` tail of a variable binding, shared by `parse_var_binding`
    /// and the curry-file path (0.0.8 T02): a top-level `name : type` in a
    /// flag-on file parses its type first, then branches on `=` — binding
    /// here, signature line otherwise.
    fn finish_var_binding(
        &mut self,
        name: String,
        ty: Option<ast::Type>,
        start: u32,
    ) -> Result<ast::VarBinding, ParseError> {
        self.expect(&crate::lexer::TokenKind::Assign, "`=` in variable binding")?;

        let init = self.parse_expr()?;

        if self.check(&crate::lexer::TokenKind::RBrace) {
            // ASI (ASI.md §3.4): a binding immediately before `}` needs no
            // `;` — the pass never inserts before a closing brace, and a
            // binding can never be a block tail expression.
        } else if !self.matches(&crate::lexer::TokenKind::Semi) {
            return Err(self.err_expected_stmt_end());
        }

        let end = self.prev_span_end();
        Ok(ast::VarBinding {
            name,
            ty,
            init: Box::new(init),
            span: self.span(start, end),
            is_pub: false,
        })
    }

    /// 0.0.8 T02 (D2/D3): the two-part curried declaration, flag-on files
    /// only. The signature line `[pub] [modifier] ident : type` is the
    /// interface half; the body line `ident ( params ) ( block | = expr )`
    /// is the implementation half.
    ///
    /// Modifiers live on the signature line (D3): in a flag-on file the
    /// `async`/`unsafe`/`trusted` run prefixes the signature. `modifier
    /// func` is the legacy form, rejected (`FuncFormForbiddenInCurryFile`);
    /// `modifier name(...)` is a body line carrying a modifier, rejected
    /// (`ModifierOnCurriedBody`). `pub` is handled by the caller (it is a
    /// top-level item modifier, consumed before dispatch).
    /// The `async`/`unsafe`/`trusted` modifier run (free order, no repeats)
    /// — shared by `parse_func_decl` and `parse_curried_signature`.
    fn parse_modifiers(&mut self) -> SignatureModifiers {
        let mut mods = SignatureModifiers {
            is_async: false,
            is_unsafe: false,
            is_trusted: false,
            first: None,
        };
        loop {
            match self.current_kind() {
                crate::lexer::TokenKind::Async if !mods.is_async => {
                    mods.is_async = true;
                    mods.first.get_or_insert("async");
                }
                crate::lexer::TokenKind::Unsafe if !mods.is_unsafe => {
                    mods.is_unsafe = true;
                    mods.first.get_or_insert("unsafe");
                }
                crate::lexer::TokenKind::Trusted if !mods.is_trusted => {
                    mods.is_trusted = true;
                    mods.first.get_or_insert("trusted");
                }
                _ => break,
            }
            self.advance();
        }
        mods
    }

    /// The legacy `func`-keyword form in a `#feature curry` file (0.0.8 T02,
    /// D2) — one emission point for the discovery sites (top-level `func`,
    /// a modifier run followed by `func`, and a nested declaration in a
    /// block body), so the diagnostic stays identical wherever the form is
    /// found.
    fn err_func_form_forbidden(&self) -> ParseError {
        ParseError {
            kind: ParseErrorKind::FuncFormForbiddenInCurryFile,
            span: self.current().span,
        }
    }

    /// The `: type` tail of a signature line — the caller consumed the `:`;
    /// this parses the type and returns it with its span (which the
    /// `SignatureNeedsFunctionType` diagnostic points at). Shared by the
    /// modifier path and the plain path (0.0.8 T02).
    fn parse_signature_type(&mut self) -> Result<(ast::Type, Span), ParseError> {
        let ty_start = self.current().span.start;
        let ty = self.parse_type()?;
        let ty_span = self.span(ty_start, self.prev_span_end());
        Ok((ty, ty_span))
    }

    fn parse_curried_signature(&mut self, is_pub: bool) -> Result<ast::Item, ParseError> {
        let start = self.current().span.start;

        let mods = self.parse_modifiers();

        if self.check(&crate::lexer::TokenKind::Func) {
            return Err(self.err_func_form_forbidden());
        }

        let name = self.parse_ident()?;

        if !self.check(&crate::lexer::TokenKind::Colon) {
            // `unsafe add(a, b) { ... }` — a modifier on the body line (D3).
            if self.check(&crate::lexer::TokenKind::LParen) {
                // This path is only reached from a modifier-dispatched item,
                // so `mods.first` is always `Some` — the fallback keeps the
                // parser assert-free.
                return Err(ParseError {
                    kind: ParseErrorKind::ModifierOnCurriedBody {
                        modifier: mods.first.unwrap_or("async").to_string(),
                    },
                    span: self.current().span,
                });
            }
            let found = self.current().kind.clone();
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "`:` after the signature name".to_string(),
                    found,
                },
                span: self.current().span,
            });
        }
        self.advance();

        let (ty, ty_span) = self.parse_signature_type()?;
        self.finish_curried_signature(name, ty, ty_span, is_pub, mods, start)
    }

    /// 0.0.8 T02 (D2): a top-level `ident :` in a flag-on file — either the
    /// signature line or (when an `=` follows the type) an annotated
    /// variable binding. The type must be parsed before the two can be told
    /// apart, so the binding tail is shared with `parse_var_binding`.
    fn parse_curried_sig_or_binding(&mut self, is_pub: bool) -> Result<ast::Item, ParseError> {
        let start = self.current().span.start;
        let name = self.parse_ident()?;
        self.expect(&crate::lexer::TokenKind::Colon, "`:` after the name")?;
        let (ty, ty_span) = self.parse_signature_type()?;

        if self.check(&crate::lexer::TokenKind::Assign) {
            // `x: int = 42` — variable bindings keep their flag-off shape
            // in flag-on files (D2 switches only the *function* form).
            let mut binding = self.finish_var_binding(name, Some(ty), start)?;
            binding.is_pub = is_pub;
            return Ok(ast::Item::Decl(ast::Decl::Var(binding)));
        }

        // Signature line (no modifiers reached this path — plain `pub` at
        // most). `finish_curried_signature` rejects a non-function type
        // (D3): in a flag-on file the bare `name : type` production *is*
        // the signature, so there is no other reading of the line.
        self.finish_curried_signature(
            name,
            ty,
            ty_span,
            is_pub,
            SignatureModifiers::default(),
            start,
        )
    }

    /// Complete a signature whose modifiers were already consumed (by
    /// `parse_curried_signature` for the `modifier name : type` shape, by
    /// `parse_curried_sig_or_binding` for the plain shape) — split out so
    /// the node construction stays in one place.
    fn finish_curried_signature(
        &mut self,
        name: String,
        ty: ast::Type,
        ty_span: crate::lexer::Span,
        is_pub: bool,
        mods: SignatureModifiers,
        start: u32,
    ) -> Result<ast::Item, ParseError> {
        if !matches!(ty, ast::Type::Func(..)) {
            return Err(ParseError {
                kind: ParseErrorKind::SignatureNeedsFunctionType {
                    // 0.0.8 T03 (D4): render in the file's spelling — a
                    // flag-on file sees the arrow-chain form.
                    found: ty.render(self.features.curry),
                },
                span: ty_span,
            });
        }
        let sig = ast::CurriedSignature {
            name,
            ty,
            is_pub,
            mods,
            span: self.span(start, self.prev_span_end()),
        };
        // Single-line only (D3): the ASI pass inserts the zero-width `Semi`
        // at the newline (or EOF); a trailing `;` is tolerated, as with
        // `func` declarations' optional trailing semicolon.
        let _ = self.matches(&crate::lexer::TokenKind::Semi);
        Ok(ast::Item::Decl(ast::Decl::CurriedSignature(sig)))
    }

    /// 0.0.8 T02 (D2): the body line `ident ( params ) ( block | = expr )`.
    /// Parameter names are bare identifiers (keyword-protected, as with
    /// `func` parameters); duplicates are rejected here (the card's choice —
    /// parse, not resolve). The body is a normal function body.
    fn parse_curried_body(&mut self) -> Result<ast::CurriedBody, ParseError> {
        let start = self.current().span.start;
        let name = self.parse_ident()?;

        self.expect(&crate::lexer::TokenKind::LParen, "`(` after the body name")?;

        let mut params: Vec<ast::CurriedParam> = Vec::new();
        if !self.check(&crate::lexer::TokenKind::RParen) {
            loop {
                let param_start = self.current().span.start;
                let param_name = self.parse_ident()?;
                let param_span = self.span(param_start, self.prev_span_end());
                if params.iter().any(|p| p.name == param_name) {
                    return Err(ParseError {
                        kind: ParseErrorKind::DuplicateParam { name: param_name },
                        span: param_span,
                    });
                }
                params.push(ast::CurriedParam {
                    name: param_name,
                    span: param_span,
                });
                if !self.matches(&crate::lexer::TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(
            &crate::lexer::TokenKind::RParen,
            "`)` after the body parameters",
        )?;

        let body = if self.matches(&crate::lexer::TokenKind::Assign) {
            // Single-expression body: `add(a, b) = a + b` — same shape as
            // the `func` single-line form (no trailing semicolon required).
            let expr = self.parse_expr()?;
            let _ = self.matches(&crate::lexer::TokenKind::Semi);
            ast::FuncBody::SingleExpr(Box::new(expr))
        } else {
            ast::FuncBody::Block(self.parse_block()?)
        };
        // The ASI pass may insert a `Semi` after the closing `}` when
        // another statement follows (or at EOF); optional, as with `func`.
        let _ = self.matches(&crate::lexer::TokenKind::Semi);

        let end = self.prev_span_end();
        Ok(ast::CurriedBody {
            name,
            params,
            body,
            span: self.span(start, end),
        })
    }

    /// Lookahead for the curried body line (0.0.8 T02, D2): the current
    /// `ident`'s next token is `(`. A body line is `ident ( params )` whose
    /// *matching* `)` is followed by `{` or `=` — a top-level call
    /// statement (`f(1);`) or a call assignment is not. Scans to the
    /// matching close paren (nesting-aware: call args may nest calls and
    /// parenthesized expressions); EOF before it closes means no.
    fn curried_body_ahead(&self) -> bool {
        let mut depth = 0usize;
        let mut i = self.pos + 1; // at the `(` after the name
        while let Some(token) = self.tokens.get(i) {
            match &token.kind {
                crate::lexer::TokenKind::LParen => depth += 1,
                crate::lexer::TokenKind::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            self.tokens.get(i + 1).map(|t| &t.kind),
                            Some(crate::lexer::TokenKind::LBrace)
                                | Some(crate::lexer::TokenKind::Assign)
                        );
                    }
                }
                _ => {}
            }
            i += 1;
        }
        false
    }

    /// Parse a function declaration.
    fn parse_func_decl(&mut self) -> Result<ast::FuncDecl, ParseError> {
        let start = self.current().span.start;

        // 0.0.4 F1: optional `async` modifier. Only `func` may follow it (so
        // a stray `async const`/`async x` errors here, at `expect`, rather
        // than being silently accepted).
        //
        // 0.0.5 T02: `unsafe` / `trusted` join the modifier group. The
        // `async`/`unsafe`/`trusted` order is free (`async unsafe func` and
        // `unsafe async func` are the same declaration state — D3/T04); the
        // flags are consumed by typeck's unsafe-context tracking (T03).
        // 0.0.8 T02: the same run (extracted) serves the curried signature
        // line — `parse_modifiers` keeps the order/repeat rules in one place.
        let mods = self.parse_modifiers();

        self.expect(&crate::lexer::TokenKind::Func, "`func`")?;

        let name = self.parse_ident()?;

        self.expect(&crate::lexer::TokenKind::LParen, "`(` after function name")?;

        let mut params = Vec::new();
        if !self.check(&crate::lexer::TokenKind::RParen) {
            loop {
                let param = self.parse_param()?;
                params.push(param);
                if !self.matches(&crate::lexer::TokenKind::Comma) {
                    break;
                }
            }
        }
        self.expect(&crate::lexer::TokenKind::RParen, "`)`")?;

        let ret_type = if self.matches(&crate::lexer::TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        // Function body: either `= expr` or `{ block }`
        let body = if self.matches(&crate::lexer::TokenKind::Assign) {
            // Single-line: func name(params): type = expr
            // Per EBNF: single-line function body does not require trailing semicolon
            let expr = self.parse_expr()?;
            let _ = self.matches(&crate::lexer::TokenKind::Semi);
            ast::FuncBody::SingleExpr(Box::new(expr))
        } else {
            // Multi-line: func name(params): type { ... }
            ast::FuncBody::Block(self.parse_block()?)
        };
        // The ASI pass inserts a `Semi` after the closing `}` when another
        // statement follows (or at EOF); 0.0.1 never required one here, so
        // it stays optional (ASI.md §4).
        let _ = self.matches(&crate::lexer::TokenKind::Semi);

        let end = self.prev_span_end();
        Ok(ast::FuncDecl {
            name,
            params,
            ret_type,
            body,
            span: self.span(start, end),
            is_pub: false,
            is_async: mods.is_async,
            is_unsafe: mods.is_unsafe,
            is_trusted: mods.is_trusted,
        })
    }

    /// Parse a function parameter: `ident : type`.
    fn parse_param(&mut self) -> Result<ast::Param, ParseError> {
        let start = self.current().span.start;
        let name = self.parse_ident()?;
        self.expect(&crate::lexer::TokenKind::Colon, "`:` in parameter")?;
        let ty = self.parse_type()?;
        let end = self.prev_span_end();
        Ok(ast::Param {
            name,
            ty,
            span: self.span(start, end),
        })
    }

    /// Parse an identifier from the current token.
    fn parse_ident(&mut self) -> Result<String, ParseError> {
        if let crate::lexer::TokenKind::Ident(name) = self.current_kind().clone() {
            // P3: Check if identifier is a keyword
            if self.check_keyword(&name) {
                let found = self.current().kind.clone();
                let span = self.current().span;
                return Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: format!(
                            "identifier (keyword `{}` cannot be used as identifier)",
                            name
                        ),
                        found,
                    },
                    span,
                });
            }
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            let found = self.current().kind.clone();
            let span = self.current().span;
            Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "identifier".to_string(),
                    found,
                },
                span,
            })
        }
    }

    /// Parse a namespace/member segment — an identifier, or one of the
    /// reserved words `async`/`await`/`channel` which are legal as a *name*
    /// in a dotted module path and `.member` selector position (0.0.4: this
    /// is what makes `import std.async` / `std.async.run` parse — the
    /// concurrency executor module is literally named `async`, PLAN §8).
    /// Unlike `parse_ident`, binding names stay keyword-protected; only
    /// namespace/member positions accept a keyword token.
    fn parse_namespace_segment(&mut self) -> Result<String, ParseError> {
        match self.current_kind().clone() {
            crate::lexer::TokenKind::Ident(name) => {
                if self.check_keyword(&name) {
                    let found = self.current().kind.clone();
                    let span = self.current().span;
                    return Err(ParseError {
                        kind: ParseErrorKind::Expected {
                            expected: format!(
                                "identifier (keyword `{}` cannot be used as a name)",
                                name
                            ),
                            found,
                        },
                        span,
                    });
                }
                self.advance();
                Ok(name)
            }
            crate::lexer::TokenKind::Async => {
                self.advance();
                Ok(keyword_name(crate::lexer::TokenKind::Async))
            }
            crate::lexer::TokenKind::Await => {
                self.advance();
                Ok(keyword_name(crate::lexer::TokenKind::Await))
            }
            crate::lexer::TokenKind::Channel => {
                self.advance();
                Ok(keyword_name(crate::lexer::TokenKind::Channel))
            }
            _ => {
                let found = self.current().kind.clone();
                let span = self.current().span;
                Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: "module path member".to_string(),
                        found,
                    },
                    span,
                })
            }
        }
    }

    /// Parse a type annotation.
    ///
    /// Type positions are shared: signature lines, parameter types, return
    /// types, binding annotations, `as` casts, type-guard patterns, and type
    /// arguments all route here. The 0.0.8 T03 arrow chain therefore covers
    /// every type position of a flag-on file — intentional, and ambiguity-
    /// free, because `->` has no expression-level meaning.
    fn parse_type(&mut self) -> Result<ast::Type, ParseError> {
        // 0.0.4 F3: `Future<T>` — the async coroutine type. `Future` is *not*
        // a reserved word (0.0.4 reserves only `async`/`await`/`channel`), so
        // it is recognizable only in constructor position `Future<T>`: a bare
        // `Future` falls through to the ordinary `Named` path (a struct named
        // `Future` stays legal).
        if let crate::lexer::TokenKind::Ident(name) = self.current_kind()
            && name == "Future"
            && matches!(self.peek_kind(1), Some(crate::lexer::TokenKind::Lt))
        {
            self.advance();
            self.expect(&crate::lexer::TokenKind::Lt, "`<` in Future type")?;
            let inner = self.parse_type()?;
            self.expect(&crate::lexer::TokenKind::Gt, "`>` in Future type")?;
            // T03: the Future branch must run the arrow-chain tail too —
            // `Future<int> -> int` (a unary function taking a future) is
            // otherwise left with an unconsumed `->`.
            return self.finish_arrow_chain(ast::Type::Future(Box::new(inner)));
        }

        let ty = match self.current_kind() {
            crate::lexer::TokenKind::IntType => {
                self.advance();
                ast::Type::Base(ast::BaseType::Int)
            }
            crate::lexer::TokenKind::FloatType => {
                self.advance();
                ast::Type::Base(ast::BaseType::Float)
            }
            crate::lexer::TokenKind::BoolType => {
                self.advance();
                ast::Type::Base(ast::BaseType::Bool)
            }
            crate::lexer::TokenKind::StringType => {
                self.advance();
                ast::Type::Base(ast::BaseType::String)
            }
            crate::lexer::TokenKind::UnitType => {
                self.advance();
                ast::Type::Base(ast::BaseType::Unit)
            }
            crate::lexer::TokenKind::Ident(name) => {
                // Named user type (0.0.3 V02), optionally with type args
                // (0.0.7 T01 D1/D9). Dotted module paths (`net.Point`) are
                // accepted; type args apply to the whole path (`net.Option<int>`).
                let start = self.current().span.start;
                let mut name = name.clone();
                self.advance();
                while self.check(&crate::lexer::TokenKind::Dot) {
                    self.advance();
                    name.push('.');
                    let seg = self.parse_namespace_segment()?;
                    name.push_str(&seg);
                }
                let args = self.parse_optional_type_args()?;
                let end = self.prev_span_end();
                ast::Type::Named {
                    name,
                    args,
                    span: self.span(start, end),
                }
            }
            crate::lexer::TokenKind::LParen => {
                // Function type: (T, T, ...) -> T
                self.advance();
                let mut params = Vec::new();
                if !self.check(&crate::lexer::TokenKind::RParen) {
                    loop {
                        params.push(self.parse_type()?);
                        if !self.matches(&crate::lexer::TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&crate::lexer::TokenKind::RParen, "`)` in function type")?;
                self.expect(&crate::lexer::TokenKind::Arrow, "`->` in function type")?;
                let ret = self.parse_type()?;
                // 0.0.8 T03 (D4): a flag-on file canonicalizes the arrow's
                // return side into the param chain — `(int, int) -> (int) ->
                // int` is the same type as `(int, int) -> int -> int` (D4:
                // two spellings, one canonical form, so the legacy sugar
                // merges its return side exactly like the chain tail does).
                // A flag-off file keeps the pre-0.0.8 nesting byte-identical
                // (D16: shared path, no flag-off behavior change). An empty
                // param list never absorbs the return function (`() -> unit
                // -> int` is a zero-arg function returning a unary
                // function), and the merge itself skips zero-parameter
                // return functions — see `merge_arrow_return`.
                if !self.features.curry || params.is_empty() {
                    ast::Type::Func(params, Box::new(ret))
                } else {
                    merge_arrow_return(params, ret)
                }
            }
            crate::lexer::TokenKind::LBracket => {
                // Array type: [T]
                self.advance();
                let inner = self.parse_type()?;
                self.expect(&crate::lexer::TokenKind::RBracket, "`]` in array type")?;
                ast::Type::Array(Box::new(inner))
            }
            crate::lexer::TokenKind::BoxType => {
                // box<T>
                self.advance();
                self.expect(&crate::lexer::TokenKind::Lt, "`<` in box type")?;
                let inner = self.parse_type()?;
                self.expect(&crate::lexer::TokenKind::Gt, "`>` in box type")?;
                ast::Type::Box(Box::new(inner))
            }
            crate::lexer::TokenKind::RefType => {
                // ref T
                self.advance();
                let inner = self.parse_type()?;
                ast::Type::Ref(Box::new(inner))
            }
            crate::lexer::TokenKind::ResultType => {
                // 0.0.10 T02 (D1): `Result<T, E>` / `Result[T, E]` (bracket,
                // 0.0.7 D9) now parses as an ordinary parameterized type use
                // (`Type::Named` named `Result`) — the dedicated builtin
                // shape is retired; the resolver maps the prelude name back
                // onto the internal `Type::Result`. `Result` stays a lexer
                // keyword (PLAN §5: no lexer change this milestone), and the
                // two-argument shape keeps the pre-0.0.10 parse errors
                // byte-for-byte: a bare `Result` and every too-few/too-many
                // argument spelling are rejected at the same stage with the
                // same text as before the migration.
                let start = self.current().span.start;
                self.advance();
                let (close, close_msg) = if self.matches(&crate::lexer::TokenKind::LBracket) {
                    (crate::lexer::TokenKind::RBracket, "`]` in Result type")
                } else {
                    self.expect(&crate::lexer::TokenKind::Lt, "`<` or `[` in Result type")?;
                    (crate::lexer::TokenKind::Gt, "`>` in Result type")
                };
                let ok = self.parse_type()?;
                self.expect(&crate::lexer::TokenKind::Comma, "`,` in Result type")?;
                let err = self.parse_type()?;
                self.expect(&close, close_msg)?;
                let end = self.prev_span_end();
                ast::Type::Named {
                    name: "Result".to_string(),
                    args: vec![ok, err],
                    span: self.span(start, end),
                }
            }
            crate::lexer::TokenKind::Channel => {
                // channel<T> (0.0.4 F9): a message-passing channel type.
                // `channel` is a reserved keyword (TokenKind::Channel).
                self.advance();
                self.expect(&crate::lexer::TokenKind::Lt, "`<` in channel type")?;
                let inner = self.parse_type()?;
                self.expect(&crate::lexer::TokenKind::Gt, "`>` in channel type")?;
                ast::Type::Channel(Box::new(inner))
            }
            crate::lexer::TokenKind::Ptr => {
                // ptr<T> (0.0.5 T02, D1): a raw pointer type. `ptr` is a
                // reserved keyword recognized only in constructor position —
                // there is no bare-`ptr` type (same shape as `channel<T>`).
                self.advance();
                self.expect(&crate::lexer::TokenKind::Lt, "`<` in ptr type")?;
                let inner = self.parse_type()?;
                self.expect(&crate::lexer::TokenKind::Gt, "`>` in ptr type")?;
                ast::Type::Ptr(Box::new(inner))
            }
            _ => {
                let found = self.current().kind.clone();
                let span = self.current().span;
                return Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: "type".to_string(),
                        found,
                    },
                    span,
                });
            }
        };

        self.finish_arrow_chain(ty)
    }

    /// 0.0.8 T03 (D4): the arrow chain tail of a type —
    /// `arrow_type = func_atom , "->" , arrow_type`, right-associative,
    /// accepted only in a `#feature curry` file. `int -> int -> int` parses
    /// as `int -> (int -> int)` and both spellings canonicalize to the same
    /// flat `Type::Func([int, int], int)` (the chain is parse sugar over one
    /// `Type::Func` — no representational currying, D4). A flag-off file
    /// never enters the branch, so a stray `->` keeps today's parse error.
    fn finish_arrow_chain(&mut self, atom: ast::Type) -> Result<ast::Type, ParseError> {
        if !self.features.curry || !self.check(&crate::lexer::TokenKind::Arrow) {
            return Ok(atom);
        }
        self.advance();
        let ret = self.parse_type()?;
        Ok(merge_arrow_return(vec![atom], ret))
    }

    /// Parse a block: `{ stmt* expr? }`
    fn parse_block(&mut self) -> Result<ast::Block, ParseError> {
        let start = self.current().span.start;

        self.expect(&crate::lexer::TokenKind::LBrace, "`{`")?;

        // Block bodies are always struct-literal-allowed contexts (V02):
        // even when a block appears inside an `if`/`while` condition
        // expression, its statements parse with the flag on. (The flag is
        // re-established by every `parse_cond`; on an error return the
        // whole parse aborts, so not restoring on `?` paths is harmless.)
        self.allow_struct_lit = true;

        let mut stmts = Vec::new();
        let mut tail_expr = None;

        loop {
            if self.check(&crate::lexer::TokenKind::RBrace) {
                break;
            }
            if self.is_at_end() {
                return Err(ParseError {
                    kind: ParseErrorKind::UnexpectedEof,
                    span: self.tokens[self.pos - 1].span,
                });
            }
            // Statement-entry guard (ASI.md §4 #4): catches stray `else`,
            // `;`, `)` etc. at statement position with an actionable error
            // instead of a generic "expected expression".
            if !asi::can_start_stmt(self.current_kind()) {
                return Err(self.err_expected_stmt_end());
            }

            // 0.0.8 T01 (D1): `#` inside a block body is the same misplaced
            // directive as at top level after the first item — rejected like
            // `pub` below, with the dedicated diagnostic.
            if self.check(&crate::lexer::TokenKind::Hash) {
                return Err(self.err_feature_misplaced());
            }

            // `pub` is a top-level-only modifier (0.0.3 V05, PLAN D6).
            if self.check(&crate::lexer::TokenKind::Pub) {
                return Err(ParseError {
                    kind: ParseErrorKind::PubNotAllowedHere,
                    span: self.current().span,
                });
            }

            // 0.0.8 T02 (D2): in a `#feature curry` file the legacy `func`
            // form is forbidden everywhere — nested declarations included —
            // and the two-part form is top-level only (declarations inside
            // blocks are top-level-only anyway, as with `struct`/`enum`).
            if self.features.curry {
                if self.check(&crate::lexer::TokenKind::Func) {
                    return Err(self.err_func_form_forbidden());
                }
                if self.check(&crate::lexer::TokenKind::Async)
                    || self.check(&crate::lexer::TokenKind::Unsafe)
                    || self.check(&crate::lexer::TokenKind::Trusted)
                {
                    if matches!(self.peek_kind(1), Some(crate::lexer::TokenKind::Func)) {
                        return Err(self.err_func_form_forbidden());
                    }
                    if matches!(self.peek_kind(1), Some(crate::lexer::TokenKind::Ident(_)))
                        && matches!(self.peek_kind(2), Some(crate::lexer::TokenKind::Colon))
                    {
                        // A two-part *signature* line inside a block —
                        // declarations are top-level only (D2), as with
                        // `struct`/`enum` above.
                        return Err(ParseError {
                            kind: ParseErrorKind::Expected {
                                expected: "statement (two-part curried \
                                           declarations are only allowed at \
                                           the top level)"
                                    .to_string(),
                                found: self.current().kind.clone(),
                            },
                            span: self.current().span,
                        });
                    }
                }
            }

            // Check for declaration
            let is_decl = self.check(&crate::lexer::TokenKind::Const)
                || self.check(&crate::lexer::TokenKind::Func)
                // 0.0.4 F1: `async` at statement start opens an `async func`.
                // 0.0.5 T02: `unsafe` / `trusted` open their func forms too.
                || self.check(&crate::lexer::TokenKind::Async)
                || self.check(&crate::lexer::TokenKind::Unsafe)
                || self.check(&crate::lexer::TokenKind::Trusted)
                || (self.check(&crate::lexer::TokenKind::Ident(String::new())) && {
                    matches!(
                        self.peek_kind(1),
                        Some(crate::lexer::TokenKind::Colon)
                            | Some(crate::lexer::TokenKind::Assign)
                    )
                });

            // Struct declarations are top-level only (0.0.3 D1): inside a
            // block they would need scoped struct tables for no benefit.
            if self.check(&crate::lexer::TokenKind::Struct) {
                return Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: "statement (struct declarations are only \
                                   allowed at the top level)"
                            .to_string(),
                        found: self.current().kind.clone(),
                    },
                    span: self.current().span,
                });
            }
            // `enum` declarations are top-level only (0.0.6 T02): inside a
            // block they would need scoped enum tables for no benefit. The
            // `Enum` keyword is already statement-starting (T01), so this
            // check runs before the expression branch that would otherwise
            // emit the temporary "expected expression, found enum" fallback.
            if self.check(&crate::lexer::TokenKind::Enum) {
                return Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: "statement (enum declarations are only \
                                   allowed at the top level)"
                            .to_string(),
                        found: self.current().kind.clone(),
                    },
                    span: self.current().span,
                });
            }

            // `break` / `continue` statements (0.0.9 T03, D4): keyword
            // statements, never expressions. Whether the position is legal
            // (inside a loop body of the lexically enclosing function) is
            // typeck's business — the parser accepts the shape everywhere.
            if self.check(&crate::lexer::TokenKind::Break)
                || self.check(&crate::lexer::TokenKind::Continue)
            {
                let stmt = self.parse_break_like_stmt()?;
                stmts.push(stmt);
                continue;
            }

            if is_decl {
                // Parse declaration
                if self.check(&crate::lexer::TokenKind::Const) {
                    let decl = self.parse_const_decl()?;
                    stmts.push(ast::Stmt::Decl(ast::Decl::Const(decl)));
                } else if self.check(&crate::lexer::TokenKind::Func)
                    || self.check(&crate::lexer::TokenKind::Async)
                {
                    let decl = self.parse_func_decl()?;
                    stmts.push(ast::Stmt::Decl(ast::Decl::Func(decl)));
                } else {
                    let decl = self.parse_var_binding()?;
                    stmts.push(ast::Stmt::Decl(ast::Decl::Var(decl)));
                }
            } else {
                // Expression: statement or tail expression. The ASI pass
                // guarantees a `Semi` after every complete statement, so a
                // missing one here is same-line juxtaposition or the block
                // tail (ASI.md §4).
                self.reject_keyword_binding_name()?;
                self.reject_deref_assign_type_annot()?;
                let expr = self.parse_expr()?;

                if self.matches(&crate::lexer::TokenKind::Semi) {
                    stmts.push(ast::Stmt::Expr(Box::new(expr), true));
                } else if self.check(&crate::lexer::TokenKind::RBrace) {
                    if matches!(expr, Expr::Assign(_, _)) {
                        // Assignment-shaped expressions (`deref b = v`,
                        // `x.y = v`) cannot be block tails: lowering leaves
                        // the RHS value on the stack while typeck types the
                        // assignment Unit (ASI.md §4 #2). Treat as statement.
                        stmts.push(ast::Stmt::Expr(Box::new(expr), false));
                    } else {
                        tail_expr = Some(Box::new(expr));
                        break;
                    }
                } else {
                    return Err(self.err_expected_stmt_end());
                }
            }
        }

        self.expect(&crate::lexer::TokenKind::RBrace, "`}`")?;
        let end = self.prev_span_end();

        Ok(ast::Block {
            stmts,
            tail_expr,
            span: self.span(start, end),
        })
    }

    /// Parse a `break` / `continue` *statement* (0.0.9 T03, D4):
    /// `break_stmt = "break" , [ ";" ]`. The trailing `;` is optional (the
    /// ASI pass supplies one at line ends; rule 2 never inserts before `}`),
    /// so the accepted boundaries are `;`, `}` and EOF — anything else is
    /// one-line juxtaposition (`break 1`) and reports the statement-separator
    /// error, exactly like the expression-statement path.
    ///
    /// They are statements, not expressions (D4): the grammar has no
    /// `break <value>` form, so `x = break` fails in the expression parser,
    /// and `break = 1` reports the reserved word through
    /// [`Parser::reject_keyword_binding_name`].
    /// The `break` / `continue` keyword-statement shape (0.0.9 T03, D4),
    /// built from the shared [`Parser::parse_break_like`] core.
    fn parse_break_like_stmt(&mut self) -> Result<ast::Stmt, ParseError> {
        let (is_break, span) = self.parse_break_like(true)?;
        Ok(if is_break {
            ast::Stmt::Break(span)
        } else {
            ast::Stmt::Continue(span)
        })
    }

    /// Top-level shape of [`Parser::parse_break_like_stmt`]: an
    /// `Item::Break` / `Item::Continue` (0.0.9 T03, D4). A top-level `break`
    /// is legal to *parse*; typeck rejects it (no loop can enclose a
    /// top-level item).
    fn parse_break_like_item(&mut self) -> Result<ast::Item, ParseError> {
        let (is_break, span) = self.parse_break_like(false)?;
        Ok(if is_break {
            ast::Item::Break(span)
        } else {
            ast::Item::Continue(span)
        })
    }

    /// Shared core of the two `break` / `continue` shapes: reject
    /// binding-name misuse, pick the keyword, advance, then check the
    /// terminator, returning `(is_break, span)` for the wrappers to build
    /// their node. `allow_rbrace` marks the statement form, whose last
    /// statement may end at a block boundary (`{ break }` stays legal);
    /// the item form takes `;` or ends at EOF.
    fn parse_break_like(&mut self, allow_rbrace: bool) -> Result<(bool, Span), ParseError> {
        // `break = 1` names the keyword (same route as `move = 1`); a bare
        // `break` falls through (peek is not `=` / `:`).
        self.reject_keyword_binding_name()?;
        let is_break = self.check(&crate::lexer::TokenKind::Break);
        let span = self.current().span;
        self.advance();
        // Terminator: explicit `;`, or an accepted boundary — EOF, plus a
        // block boundary for the statement form.
        if self.is_at_end() || (allow_rbrace && self.check(&crate::lexer::TokenKind::RBrace)) {
            // no terminator consumed
        } else if !self.matches(&crate::lexer::TokenKind::Semi) {
            return Err(self.err_expected_stmt_end());
        }
        Ok((is_break, span))
    }
}

// Delegate expression parsing to expr module
impl Parser {
    fn parse_expr(&mut self) -> Result<ast::Expr, ParseError> {
        expr::parse_expr(self)
    }
}
