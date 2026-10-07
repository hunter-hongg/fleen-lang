//! Expression parsing using recursive descent + precedence climbing.
//!
//! Grammar (from SPEC.md §14 and SYNTAX.ebnf v0.0.2, precedence low to high):
//! ```text
//! expr        = if_expr | while_expr | choose_expr | assign_expr
//! assign_expr   = logic_or [ "=" assign_expr ]     (right-assoc)
//! logic_or      = logic_and { "or"  logic_and }
//! logic_and     = equality  { "and" equality  }
//! equality      = comparison { ("==" | "!=") comparison }
//! comparison    = additive   { ("<" | ">" | "<=" | ">=") additive }
//! additive      = multiplicative { ("+" | "-") multiplicative }
//! multiplicative= cast       { ("*" | "/" | "%") cast }
//! cast          = unary    { "as" type }           (0.0.2 U13, F8)
//! unary         = unary_prefix unary | postfix     (prefixes right-assoc)
//! unary_prefix  = "-" | "!" | "box" | "deref" | "move" | "clone"
//! postfix       = primary { call | index | field | "?" }
//! primary       = literal | ident | "(" expr ")" | block | choose_result_expr
//! ```
//!
//! 0.0.2: `box` / `deref` / `move` / `clone` are prefix keyword expressions at
//! the unary level (right-associative, so `clone deref b` = `clone(deref(b))`
//! and `box box 1` = `box(box(1))`); `?` is a postfix suffix binding tighter
//! than any prefix (`deref b?` = `deref(b?)`). The parser only recognizes
//! forms — operand legality (e.g. `move`'s place) is checked by typeck.
//!
//! 0.0.2 U13 (F8): `as` is a cast layer between `unary` and binary operators.
//! `box` is special: its operand is parsed at the `cast` level (so
//! `box 1 as string` = `box (1 as string)`), while all other prefixes
//! (`deref`/`move`/`clone`/`-`/`!`) take a `unary` operand (so
//! `deref b as string` = `(deref b) as string`).

use crate::lexer::Span;
use crate::lexer::TokenKind;
use crate::parser::Parser;
use crate::parser::ast::{Expr, ExprChoose, ExprIf, ExprWhile, Pattern, ResultCtor};
use crate::parser::error::{ParseError, ParseErrorKind};

/// Parse an expression starting from the current position.
pub fn parse_expr(parser: &mut Parser) -> Result<Expr, ParseError> {
    parser.parse_expr_or()
}

/// Binary operator precedence levels, lowest first (SPEC.md §14,
/// SYNTAX.ebnf v0.0.2): 0 `or`, 1 `and`, 2 equality, 3 comparison,
/// 4 additive, 5 multiplicative. All levels are left-associative.
const LEVEL_OR: usize = 0;
const LEVEL_MUL: usize = 5;

/// Constructor for one binary operator node.
type BinCtor = fn(Box<Expr>, Box<Expr>) -> Expr;

/// Precedence level and AST constructor of a binary operator token,
/// or `None` if the token does not continue a binary expression.
fn binop_level(kind: &TokenKind) -> Option<(usize, BinCtor)> {
    let (level, ctor): (usize, BinCtor) = match kind {
        TokenKind::Or => (LEVEL_OR, |a, b| Expr::Or(a, b)),
        TokenKind::And => (1, |a, b| Expr::And(a, b)),
        TokenKind::Eq => (2, |a, b| Expr::Eq(a, b)),
        TokenKind::Ne => (2, |a, b| Expr::Ne(a, b)),
        TokenKind::Lt => (3, |a, b| Expr::Lt(a, b)),
        TokenKind::Gt => (3, |a, b| Expr::Gt(a, b)),
        TokenKind::Le => (3, |a, b| Expr::Le(a, b)),
        TokenKind::Ge => (3, |a, b| Expr::Ge(a, b)),
        TokenKind::Plus => (4, |a, b| Expr::Add(a, b)),
        TokenKind::Minus => (4, |a, b| Expr::Sub(a, b)),
        TokenKind::Star => (LEVEL_MUL, |a, b| Expr::Mul(a, b)),
        TokenKind::Slash => (LEVEL_MUL, |a, b| Expr::Div(a, b)),
        TokenKind::Percent => (LEVEL_MUL, |a, b| Expr::Mod(a, b)),
        _ => return None,
    };
    Some((level, ctor))
}

impl Parser {
    // Main entry: handles if/while/choose or falls through to assign_expr
    fn parse_expr_or(&mut self) -> Result<Expr, ParseError> {
        let head = match self.current_kind() {
            TokenKind::If => self.parse_if()?,
            TokenKind::While => self.parse_while()?,
            TokenKind::Choose => self.parse_choose()?,
            _ => return self.parse_assign(),
        };

        // Block-like heads return before operator climbing resumes, so a
        // following `(`/`[`/`?`/binary operator continues the expression
        // (ASI.md §4 #3: 运算符续接优先，`if c { 1 }` ⏎ `- 2` ⇒ `(if) - 2`).
        if !crate::parser::asi::can_continue_expr(self.current_kind()) {
            return Ok(head);
        }
        let expr = self.parse_postfix_suffixes(head)?;
        self.fold_binary(expr, LEVEL_OR)
    }

    /// Parse a binary expression whose operators bind no looser than
    /// `min_level`: descend to the tightest level, then fold operators
    /// left-associatively. Shared by the main chain (`parse_assign`) and
    /// post-block-head continuation (`fold_binary`), so precedence lives
    /// in exactly one place (`binop_level`).
    fn parse_binary_from(&mut self, min_level: usize) -> Result<Expr, ParseError> {
        let left = if min_level > LEVEL_MUL {
            self.parse_cast()?
        } else {
            self.parse_binary_from(min_level + 1)?
        };
        self.fold_binary(left, min_level)
    }

    /// Fold binary operators of level >= `min_level` onto an
    /// already-parsed `left` operand.
    fn fold_binary(&mut self, mut left: Expr, min_level: usize) -> Result<Expr, ParseError> {
        loop {
            let Some((level, ctor)) = binop_level(self.current_kind()) else {
                return Ok(left);
            };
            if level < min_level {
                return Ok(left);
            }
            self.advance();
            let right = self.parse_binary_from(level + 1)?;
            left = ctor(Box::new(left), Box::new(right));
        }
    }

    /// Parse assignment expression (right-associative).
    fn parse_assign(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_binary_from(LEVEL_OR)?;

        if self.matches(&TokenKind::Assign) {
            // Assignment is right-associative: `a = b = c` → `a = (b = c)`
            let rhs = self.parse_assign()?;
            Ok(Expr::Assign(Box::new(expr), Box::new(rhs)))
        } else {
            Ok(expr)
        }
    }

    /// Parse cast expressions: `unary { "as" type }` (0.0.2 U13, F8).
    ///
    /// Sits between the binary operators (`*`/`/`/`%`) and `unary` in the
    /// precedence chain: `as` is tighter than binary, looser than `unary`.
    /// `box` is special: its operand is parsed at the `cast` level (so
    /// `box 1 as string` = `box (1 as string)`).
    fn parse_cast(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_unary()?;
        while self.matches(&TokenKind::As) {
            let ty = self.parse_type()?;
            let span = Span::new(expr.span().start, self.prev_span_end());
            expr = Expr::Cast(Box::new(expr), ty, span);
        }
        Ok(expr)
    }

    /// Parse unary expressions.
    ///
    /// Prefixes are right-associative and recurse into `parse_unary`, so
    /// `clone deref b` = `clone(deref(b))` and `box box 1` = `box(box(1))`.
    /// The parser accepts any operand form here; typeck validates that
    /// `move`/`clone` operands are legal places (U05).
    ///
    /// 0.0.2 U13: `box` is special — its operand is parsed at the `cast`
    /// level (so `box 1 as string` = `box (1 as string)`). All other prefixes
    /// (`deref`/`move`/`clone`/`-`/`!`) take a `unary` operand, so
    /// `deref b as string` = `(deref b) as string`.
    fn parse_unary(&mut self) -> Result<Expr, ParseError> {
        let span = self.current().span;

        if self.matches(&TokenKind::Minus) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Neg(Box::new(operand)));
        }

        // `!` (Bang) or `not` keyword both mean logical NOT
        if self.matches(&TokenKind::Not) || self.matches(&TokenKind::Bang) {
            let operand = self.parse_unary()?;
            return Ok(Expr::Not(Box::new(operand)));
        }

        // 0.0.2: `box` — operand is a full cast expression (tighter binding),
        // so `box 1 as string` = `box (1 as string)`.
        if matches!(self.current_kind(), TokenKind::BoxType) {
            self.advance();
            let operand = self.parse_cast()?;
            let span = Span::new(span.start, self.prev_span_end());
            return Ok(Expr::Box(Box::new(operand), span));
        }

        // 0.0.2 prefix keyword expressions (non-box). `deref`/`move`/`clone`
        // take a `unary` operand, so `deref b as string` = `(deref b) as string`.
        if !matches!(
            self.current_kind(),
            TokenKind::Deref | TokenKind::Move | TokenKind::Clone
        ) {
            return self.parse_postfix();
        }
        let kind = self.current_kind().clone();
        self.advance();
        let operand = self.parse_unary()?;
        let span = Span::new(span.start, self.prev_span_end());
        let expr = match kind {
            TokenKind::Deref => Expr::Deref(Box::new(operand), span),
            TokenKind::Move => Expr::Move(Box::new(operand), span),
            TokenKind::Clone => Expr::Clone(Box::new(operand), span),
            // SAFETY: the kind was filtered by the `matches!` above, so no
            // other TokenKind variant can reach this arm.
            _ => unreachable!("prefix keyword was matched before advancing"),
        };
        Ok(expr)
    }

    /// Parse postfix expressions: function calls, indexing, field access, `?`.
    fn parse_postfix(&mut self) -> Result<Expr, ParseError> {
        let expr = self.parse_primary()?;
        self.parse_postfix_suffixes(expr)
    }

    /// Attach postfix suffixes (`(`/`[`/`.`/`?`) to an already-parsed
    /// primary or block-like head.
    fn parse_postfix_suffixes(&mut self, mut expr: Expr) -> Result<Expr, ParseError> {
        loop {
            if self.matches(&TokenKind::LParen) {
                // Function call
                expr = self.parse_call_args(expr)?;
            } else if self.matches(&TokenKind::LBracket) {
                // Index access
                let index = self.parse_expr()?;
                self.expect(&TokenKind::RBracket, "expected `]` after index")?;
                expr = Expr::Index(Box::new(expr), Box::new(index));
            } else if self.matches(&TokenKind::Dot) {
                // Field access
                let field = self.parse_ident()?;
                expr = Expr::Field(Box::new(expr), field);
            } else if self.matches(&TokenKind::Question) {
                // 0.0.2: Result propagation suffix; binds tighter than any
                // prefix (`deref b?` = `deref(b?)`), chains left-to-right
                // (`f(x)??` = `(f(x)?)?`)
                let end = self.prev_span_end();
                let start = expr.span().start;
                expr = Expr::Question(Box::new(expr), Span::new(start, end));
            } else {
                break;
            }
        }

        Ok(expr)
    }

    /// Parse the argument list of a function call.
    /// Caller must have consumed the opening `(`.
    fn parse_call_args(&mut self, func: Expr) -> Result<Expr, ParseError> {
        let mut args = Vec::new();

        if !self.check(&TokenKind::RParen) {
            loop {
                let arg = self.parse_expr()?;
                args.push(arg);
                if !self.matches(&TokenKind::Comma) {
                    break;
                }
            }
        }

        self.expect(&TokenKind::RParen, "expected `)` after arguments")?;

        Ok(Expr::Call(Box::new(func), args))
    }

    /// Parse a primary expression.
    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let span = self.current().span;
        let kind = self.current_kind().clone();

        match &kind {
            TokenKind::IntLit(n) => {
                self.advance();
                Ok(Expr::Int(*n, span))
            }
            TokenKind::FloatLit(n) => {
                self.advance();
                Ok(Expr::Float(*n, span))
            }
            TokenKind::True => {
                self.advance();
                Ok(Expr::Bool(true, span))
            }
            TokenKind::False => {
                self.advance();
                Ok(Expr::Bool(false, span))
            }
            TokenKind::StringLit(s) => {
                let val = s.clone();
                self.advance();
                Ok(Expr::Str(val, span))
            }
            TokenKind::Ident(name) => {
                let name = name.clone();
                self.advance();
                // Check for struct literal: ident { ... }  -- not in 0.0.1
                // But identifiers can be followed by ( for calls, [ for index, . for field (handled in postfix)
                Ok(Expr::Ident(name, span))
            }
            TokenKind::LParen => {
                // Parenthesized expression
                self.advance();
                let expr = self.parse_expr()?;
                self.expect(&TokenKind::RParen, "expected `)`")?;
                Ok(expr)
            }
            TokenKind::LBrace => {
                // Block expression
                let block = self.parse_block()?;
                Ok(Expr::Block(block))
            }
            _ => Err(ParseError {
                kind: ParseErrorKind::Expected {
                    expected: "expression".to_string(),
                    found: kind,
                },
                span,
            }),
        }
    }

    /// Parse an if expression.
    fn parse_if(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span.start;

        self.expect(&TokenKind::If, "expected `if`")?;
        let condition = Box::new(self.parse_expr()?);
        let then_branch = self.parse_block()?;

        let mut elif_branches = Vec::new();
        let mut else_branch = None;

        loop {
            if self.matches(&TokenKind::Elif) {
                let elif_cond = Box::new(self.parse_expr()?);
                let elif_block = self.parse_block()?;
                elif_branches.push((elif_cond, elif_block));
            } else if self.matches(&TokenKind::Else) {
                let else_block = self.parse_block()?;
                else_branch = Some(else_block);
                break;
            } else {
                break;
            }
        }

        let end = self.prev_span_end();
        Ok(Expr::If(ExprIf {
            condition,
            then_branch,
            elif_branches,
            else_branch,
            span: Span::new(start, end),
        }))
    }

    /// Parse a while expression.
    fn parse_while(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span.start;

        self.expect(&TokenKind::While, "expected `while`")?;
        let condition = Box::new(self.parse_expr()?);
        let body = self.parse_block()?;

        let end = self.prev_span_end();
        Ok(Expr::While(ExprWhile {
            condition,
            body,
            span: Span::new(start, end),
        }))
    }

    /// Parse a choose expression.
    fn parse_choose(&mut self) -> Result<Expr, ParseError> {
        let start = self.current().span.start;

        self.expect(&TokenKind::Choose, "expected `choose`")?;
        let scrutinee = Box::new(self.parse_expr()?);

        self.expect(&TokenKind::LBrace, "expected `{` after choose expression")?;

        let mut arms = Vec::new();

        // Parse when clauses
        while self.matches(&TokenKind::When) {
            let pattern = self.parse_pattern()?;
            // A `;` between pattern and guard/body is an ASI-pass insertion
            // (`when Ok(v)` ⏎ `if guard {`, `If` ∈ start set): no source form
            // puts one here, so it is always an artifact — skip it (ASI.md §4).
            let _ = self.matches(&TokenKind::Semi);
            let guard = if self.matches(&TokenKind::If) {
                Some(Box::new(self.parse_expr()?))
            } else {
                None
            };
            let body = self.parse_block()?;
            arms.push(crate::parser::ast::ChooseArm {
                pattern,
                guard,
                body,
            });
        }

        // Parse optional otherwise clause
        if self.matches(&TokenKind::Otherwise) {
            let body = self.parse_block()?;
            arms.push(crate::parser::ast::ChooseArm {
                pattern: Pattern::Ident("_".to_string(), self.prev_span()), // wildcard pattern
                guard: None,
                body,
            });
        }

        self.expect(&TokenKind::RBrace, "expected `}` after choose arms")?;

        let end = self.prev_span_end();
        Ok(Expr::Choose(ExprChoose {
            scrutinee,
            arms,
            span: Span::new(start, end),
        }))
    }

    /// Parse a pattern in a choose arm.
    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        // P5: Support negated numeric patterns (e.g., `when -1 { ... }`)
        let is_neg = self.matches(&TokenKind::Minus);
        let kind = self.current_kind().clone();
        let span = self.current().span;

        macro_rules! reject_neg {
            () => {{
                if is_neg {
                    return Err(ParseError {
                        kind: ParseErrorKind::Expected {
                            expected: "numeric literal after `-`".to_string(),
                            found: kind.clone(),
                        },
                        span,
                    });
                }
            }};
        }

        let pat = match &kind {
            TokenKind::IntLit(n) => {
                let val = if is_neg { -*n } else { *n };
                self.advance();
                Pattern::Literal(Expr::Int(val, span))
            }
            TokenKind::FloatLit(n) => {
                let val = if is_neg { -*n } else { *n };
                self.advance();
                Pattern::Literal(Expr::Float(val, span))
            }
            TokenKind::True => {
                reject_neg!();
                self.advance();
                Pattern::Literal(Expr::Bool(true, span))
            }
            TokenKind::False => {
                reject_neg!();
                self.advance();
                Pattern::Literal(Expr::Bool(false, span))
            }
            TokenKind::StringLit(s) => {
                reject_neg!();
                let val = s.clone();
                self.advance();
                Pattern::Literal(Expr::Str(val, span))
            }
            TokenKind::Ident(name) => {
                reject_neg!();
                // 0.0.2: `Ok(ident)` / `Err(ident)` result patterns. `Ok` /
                // `Err` are plain Ident tokens — only the syntactic form
                // matters here; typeck binds the semantics (U04). Without a
                // following `(` they stay ordinary identifier patterns.
                if (name == "Ok" || name == "Err")
                    && matches!(self.peek_kind(1), Some(TokenKind::LParen))
                {
                    let ctor = if name == "Ok" {
                        ResultCtor::Ok
                    } else {
                        ResultCtor::Err
                    };
                    return self.parse_result_pattern(ctor, span);
                }
                let name = name.clone();
                self.advance();
                Pattern::Ident(name, self.prev_span())
            }
            _ => {
                return Err(ParseError {
                    kind: ParseErrorKind::Expected {
                        expected: "pattern".to_string(),
                        found: kind,
                    },
                    span,
                });
            }
        };

        Ok(pat)
    }

    /// Parse `Ok(ident)` / `Err(ident)` after the constructor token is
    /// recognized. Caller has not consumed the `Ok`/`Err` identifier yet.
    fn parse_result_pattern(
        &mut self,
        ctor: ResultCtor,
        ctor_span: Span,
    ) -> Result<Pattern, ParseError> {
        self.advance(); // consume `Ok` / `Err`
        self.expect(&TokenKind::LParen, "expected `(` after `Ok`/`Err` pattern")?;
        let binding = self.parse_ident()?;
        self.expect(&TokenKind::RParen, "expected `)` after pattern binding")?;
        let end = self.prev_span_end();
        Ok(Pattern::ResultCtor {
            ctor,
            binding,
            span: Span::new(ctor_span.start, end),
        })
    }
}
