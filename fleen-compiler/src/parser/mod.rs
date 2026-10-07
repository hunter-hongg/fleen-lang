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

pub use error::{ParseError, ParseErrorKind};

use crate::lexer::{Span, Token};
use ast::{Ast, Expr};

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
/// - `tokens`: 词法分析后的 Token 流
///
/// # 返回
/// - `Ok(Ast)`: 解析成功
/// - `Err(ParseError)`: 解析失败，带 Span 和错误信息
///
/// # 示例
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

/// Recursive descent parser with precedence climbing for expressions.
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    /// Create a new parser from a token stream.
    pub fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
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
            Span::new(0, 0)
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

    /// Reject a type annotation on a `deref` assignment target
    /// (`deref b: int = v`). Only plain bindings may be annotated
    /// (DESIGN.md §3.4); erroring here gives an actionable message
    /// instead of a generic "expected `;`" at the colon.
    fn reject_deref_assign_type_annot(&self) -> Result<(), ParseError> {
        if matches!(self.current_kind(), crate::lexer::TokenKind::Deref)
            && matches!(self.peek_kind(1), Some(crate::lexer::TokenKind::Ident(_)))
            && matches!(self.peek_kind(2), Some(crate::lexer::TokenKind::Colon))
        {
            let found = self
                .peek_kind(2)
                .cloned()
                .unwrap_or(crate::lexer::TokenKind::Eof);
            let span = self.tokens[self.pos + 2].span;
            return Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "`=` (a `deref` assignment target cannot carry a type \
                               annotation; annotate the binding instead, e.g. \
                               `b: box<int> = box v`)"
                        .to_string(),
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
                span: crate::lexer::Span::new(0, 0),
            });
        }

        let start = self.current().span.start;
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
            span: crate::lexer::Span::new(start, end),
        })
    }

    /// Parse a top-level item: import, declaration, or expression statement.
    fn parse_item(&mut self) -> Result<ast::Item, ParseError> {
        if self.check(&crate::lexer::TokenKind::Import) {
            return self.parse_import();
        }

        if self.check(&crate::lexer::TokenKind::Func) {
            return Ok(ast::Item::Decl(ast::Decl::Func(self.parse_func_decl()?)));
        }

        if self.check(&crate::lexer::TokenKind::Const) {
            return Ok(ast::Item::Decl(ast::Decl::Const(self.parse_const_decl()?)));
        }

        // Check for variable binding: ident followed by : or =
        if self.check(&crate::lexer::TokenKind::Ident(String::new())) {
            let next_kind = self.peek_kind(1);
            let is_binding = matches!(
                next_kind,
                Some(crate::lexer::TokenKind::Colon) | Some(crate::lexer::TokenKind::Assign)
            );
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
                return Ok(ast::Item::Decl(ast::Decl::Var(self.parse_var_binding()?)));
            }
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
        self.expect(&crate::lexer::TokenKind::Import, "expected `import`")?;

        // Parse module path: ident . ident . ...
        let mut path = Vec::new();
        loop {
            if let crate::lexer::TokenKind::Ident(name) = self.current_kind().clone() {
                path.push(name.clone());
                self.advance();
            } else {
                let found = self.current().kind.clone();
                let span = self.current().span;
                return Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: "module path".to_string(),
                        found,
                    },
                    span,
                });
            }

            if self.check(&crate::lexer::TokenKind::Dot) {
                self.advance();
            } else {
                break;
            }
        }

        self.expect(&crate::lexer::TokenKind::Semi, "expected `;` after import")?;

        let end = self.prev_span_end();
        Ok(ast::Item::Import(ast::ImportDecl {
            path,
            span: crate::lexer::Span::new(start, end),
        }))
    }

    /// Parse a const declaration: `const ident [: type] = expr ;`
    fn parse_const_decl(&mut self) -> Result<ast::ConstDecl, ParseError> {
        let start = self.current().span.start;
        self.expect(&crate::lexer::TokenKind::Const, "expected `const`")?;

        let name = self.parse_ident()?;
        let ty = if self.matches(&crate::lexer::TokenKind::Colon) {
            Some(self.parse_type()?)
        } else {
            None
        };

        self.expect(
            &crate::lexer::TokenKind::Assign,
            "expected `=` after const declaration",
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
            span: crate::lexer::Span::new(start, end),
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

        self.expect(
            &crate::lexer::TokenKind::Assign,
            "expected `=` in variable binding",
        )?;

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
            span: crate::lexer::Span::new(start, end),
        })
    }

    /// Parse a function declaration.
    fn parse_func_decl(&mut self) -> Result<ast::FuncDecl, ParseError> {
        let start = self.current().span.start;

        self.expect(&crate::lexer::TokenKind::Func, "expected `func`")?;

        let name = self.parse_ident()?;

        self.expect(
            &crate::lexer::TokenKind::LParen,
            "expected `(` after function name",
        )?;

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
        self.expect(&crate::lexer::TokenKind::RParen, "expected `)`")?;

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
            span: crate::lexer::Span::new(start, end),
        })
    }

    /// Parse a function parameter: `ident : type`.
    fn parse_param(&mut self) -> Result<ast::Param, ParseError> {
        let start = self.current().span.start;
        let name = self.parse_ident()?;
        self.expect(&crate::lexer::TokenKind::Colon, "expected `:` in parameter")?;
        let ty = self.parse_type()?;
        let end = self.prev_span_end();
        Ok(ast::Param {
            name,
            ty,
            span: crate::lexer::Span::new(start, end),
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

    /// Parse a type annotation.
    fn parse_type(&mut self) -> Result<ast::Type, ParseError> {
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
                self.expect(
                    &crate::lexer::TokenKind::RParen,
                    "expected `)` in function type",
                )?;
                self.expect(
                    &crate::lexer::TokenKind::Arrow,
                    "expected `->` in function type",
                )?;
                let ret = self.parse_type()?;
                ast::Type::Func(params, Box::new(ret))
            }
            crate::lexer::TokenKind::LBracket => {
                // Array type: [T]
                self.advance();
                let inner = self.parse_type()?;
                self.expect(
                    &crate::lexer::TokenKind::RBracket,
                    "expected `]` in array type",
                )?;
                ast::Type::Array(Box::new(inner))
            }
            crate::lexer::TokenKind::BoxType => {
                // box<T>
                self.advance();
                self.expect(&crate::lexer::TokenKind::Lt, "expected `<` in box type")?;
                let inner = self.parse_type()?;
                self.expect(&crate::lexer::TokenKind::Gt, "expected `>` in box type")?;
                ast::Type::Box(Box::new(inner))
            }
            crate::lexer::TokenKind::RefType => {
                // ref T
                self.advance();
                let inner = self.parse_type()?;
                ast::Type::Ref(Box::new(inner))
            }
            crate::lexer::TokenKind::ResultType => {
                // P4: Result<T, E> with angle brackets (per EBNF)
                self.advance();
                self.expect(&crate::lexer::TokenKind::Lt, "expected `<` in Result type")?;
                let ok = self.parse_type()?;
                self.expect(
                    &crate::lexer::TokenKind::Comma,
                    "expected `,` in Result type",
                )?;
                let err = self.parse_type()?;
                self.expect(&crate::lexer::TokenKind::Gt, "expected `>` in Result type")?;
                ast::Type::Result(Box::new(ok), Box::new(err))
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

        Ok(ty)
    }

    /// Parse a block: `{ stmt* expr? }`
    fn parse_block(&mut self) -> Result<ast::Block, ParseError> {
        let start = self.current().span.start;

        self.expect(&crate::lexer::TokenKind::LBrace, "expected `{`")?;

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

            // Check for declaration
            let is_decl = self.check(&crate::lexer::TokenKind::Const)
                || self.check(&crate::lexer::TokenKind::Func)
                || (self.check(&crate::lexer::TokenKind::Ident(String::new())) && {
                    matches!(
                        self.peek_kind(1),
                        Some(crate::lexer::TokenKind::Colon)
                            | Some(crate::lexer::TokenKind::Assign)
                    )
                });

            if is_decl {
                // Parse declaration
                if self.check(&crate::lexer::TokenKind::Const) {
                    let decl = self.parse_const_decl()?;
                    stmts.push(ast::Stmt::Decl(ast::Decl::Const(decl)));
                } else if self.check(&crate::lexer::TokenKind::Func) {
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

        self.expect(&crate::lexer::TokenKind::RBrace, "expected `}`")?;
        let end = self.prev_span_end();

        Ok(ast::Block {
            stmts,
            tail_expr,
            span: crate::lexer::Span::new(start, end),
        })
    }
}

// Delegate expression parsing to expr module
impl Parser {
    fn parse_expr(&mut self) -> Result<ast::Expr, ParseError> {
        expr::parse_expr(self)
    }
}
