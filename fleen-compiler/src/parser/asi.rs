//! ASI (automatic semicolon insertion) pass — 0.0.2, F5.
//!
//! Runs between Lex and Parse (docs/0.0.2/ASI.md). Semantics are
//! newline-sensitive: the lexer preserves line breaks as `Newline` tokens;
//! this pass consumes them, inserts explicit `Semi` tokens at statement
//! boundaries, and rejects tokens that can neither continue the previous
//! statement nor start a new one. The output stream is isomorphic to a
//! 0.0.1 token stream (semicolons everywhere the parser expects them), so
//! the parser keeps its original shape with only a few adaptations
//! (see ASI.md §4).

use crate::lexer::{Span, Token, TokenKind};
use crate::parser::error::{ParseError, ParseErrorKind};
use crate::source_map::FileId;

/// Whether this token can start a new statement (used at block starts;
/// includes `(` `-` `!`/`not` `{` and, since 0.0.3 V03, `[` — an array
/// literal statement like `[1, 2];`).
///
/// `not` and `!` are two spellings of the same operator, so the set
/// includes both; `{` is for block-expression statements (`{ 42 }`); `[`
/// is for array-literal expression statements (V03).
pub(crate) fn can_start_stmt(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Func
            | TokenKind::Const
            | TokenKind::If
            | TokenKind::While
            | TokenKind::For
            | TokenKind::Choose
            | TokenKind::Import
            | TokenKind::Pub
            | TokenKind::Struct
            // 0.0.4 F1: `async` opens an `async func` declaration.
            | TokenKind::Async
            | TokenKind::Ident(_)
            | TokenKind::IntLit(_)
            | TokenKind::FloatLit(_)
            | TokenKind::StringLit(_)
            | TokenKind::True
            | TokenKind::False
            | TokenKind::BoxType
            | TokenKind::Deref
            | TokenKind::Move
            | TokenKind::Clone
            | TokenKind::Not
            | TokenKind::Bang
            | TokenKind::Minus
            | TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::LBrace
    )
}

/// After a statement is complete, whether this token implies an "implicit
/// end" (the pass inserts a semicolon based on this).
///
/// The start set ∖ {`(`, `-`, `[`, `{`} ∪ {`}`, EOF} — `(` and `-` are also
/// in the continuation set, and continuation wins, so a statement cannot end
/// on them (`x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`); `[` stays in the continuation
/// set so `f()` ⏎ `[0]` remains an index chain (V03: the same token starts a
/// literal only after an explicit `;`/`}` boundary — the token-based ASI
/// cannot tell index from literal, and the `;`-always-legal escape hatch
/// covers the literal case); `{` is not included so that Allman style
/// (`if c` ⏎ `{`) does not split a statement (ASI.md §3.2, §5 #4).
pub(crate) fn implies_stmt_end(kind: &TokenKind) -> bool {
    match kind {
        TokenKind::Minus | TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => false,
        TokenKind::RBrace | TokenKind::Eof => true,
        other => can_start_stmt(other),
    }
}

/// Whether this token continues the current expression (binary operator /
/// `?` / `.` / `[` / `(` / `as`).
pub(crate) fn can_continue_expr(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::Eq
            | TokenKind::Ne
            | TokenKind::Lt
            | TokenKind::Gt
            | TokenKind::Le
            | TokenKind::Ge
            | TokenKind::And
            | TokenKind::Or
            | TokenKind::Question
            | TokenKind::Dot
            | TokenKind::LBracket
            | TokenKind::LParen
            | TokenKind::As
            | TokenKind::In
    )
}

/// Whether this token can be the last token of a statement (the `p` side
/// check of pass rule 3).
///
/// 0.0.2 U13: type keywords are included here too — the statement
/// `e as string` ends with `string`; without them no semicolon could be
/// inserted at EOF/newline, and the error would move earlier to the parser
/// instead of typeck.
fn can_end_stmt(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Ident(_)
            | TokenKind::IntLit(_)
            | TokenKind::FloatLit(_)
            | TokenKind::StringLit(_)
            | TokenKind::True
            | TokenKind::False
            | TokenKind::RParen
            | TokenKind::RBracket
            | TokenKind::RBrace
            | TokenKind::Question
            | TokenKind::IntType
            | TokenKind::FloatType
            | TokenKind::BoolType
            | TokenKind::StringType
            | TokenKind::UnitType
    )
}

/// Kind of unclosed bracket (the pass's stack element).
#[derive(Copy, Clone, PartialEq, Eq)]
enum OpenBracket {
    Paren,
    Bracket,
    Brace,
}

/// Action at a newline boundary.
enum Boundary {
    /// Discard the newline; do not split.
    Drop,
    /// Insert one explicit `Semi`.
    InsertSemi,
}

/// ASI pass: consumes all `Newline` tokens and inserts explicit `Semi` at
/// statement boundaries.
///
/// Decision rules (ASI.md §3.1; at a newline or EOF, `p` is the previous
/// token and `n` the next):
/// 1. innermost unclosed bracket is `(` or `[` → drop (ASI still applies
///    inside `{`);
/// 2. `n` is `}` → drop (never insert before `}` — trailing expression vs
///    binding is distinguished structurally by the parser);
/// 3. `p` cannot end a statement → drop (continuation across lines);
/// 4. `n` ∈ implicit-end set → insert `Semi`;
/// 5. `n` ∈ continuation set ∪ construct-continuation set ∪ {`;`} → drop
///    (continuation wins);
/// 6. otherwise → report `ExpectedSemiOrNewStmt`.
pub(crate) fn insert_semis(tokens: Vec<Token>) -> Result<Vec<Token>, ParseError> {
    // The pass consumes one token stream = one file; inserted `Semi` spans
    // belong to that file. (Empty streams never reach an insertion point,
    // so the fallback is purely defensive.)
    let file = tokens.first().map_or(FileId(0), |t| t.span.file);
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut stack: Vec<OpenBracket> = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    while let Some(tok) = iter.next() {
        if matches!(tok.kind, TokenKind::Newline) {
            // Collapse runs of newlines (the lexer already collapses
            // whitespace; this is a defensive second layer). The `n` used
            // for the decision is the first real token after the newline.
            let next = loop {
                match iter.peek() {
                    Some(t) if matches!(t.kind, TokenKind::Newline) => {
                        iter.next();
                    }
                    other => break other,
                }
            };
            match boundary(&stack, out.last(), next)? {
                Boundary::Drop => {}
                Boundary::InsertSemi => {
                    let end = out.last().map_or(0, |t| t.span.end);
                    out.push(Token::new(TokenKind::Semi, Span::new(file, end, end)));
                }
            }
            continue;
        }

        match tok.kind {
            TokenKind::LParen => stack.push(OpenBracket::Paren),
            TokenKind::LBracket => stack.push(OpenBracket::Bracket),
            TokenKind::LBrace => stack.push(OpenBracket::Brace),
            TokenKind::RParen if stack.last() == Some(&OpenBracket::Paren) => {
                stack.pop();
            }
            TokenKind::RBracket if stack.last() == Some(&OpenBracket::Bracket) => {
                stack.pop();
            }
            TokenKind::RBrace if stack.last() == Some(&OpenBracket::Brace) => {
                stack.pop();
            }
            // Mismatched closing brackets leave the stack untouched — the
            // parser reports the error at the right place afterwards
            TokenKind::Eof => {
                // EOF is a trigger point: `x = 1` EOF also needs a semicolon.
                // With `out` empty (empty file), or the previous token being
                // `;`, it naturally falls into the Drop branch.
                match boundary(&stack, out.last(), Some(&tok))? {
                    Boundary::Drop => {}
                    Boundary::InsertSemi => {
                        let end = out.last().map_or(0, |t| t.span.end);
                        out.push(Token::new(TokenKind::Semi, Span::new(file, end, end)));
                    }
                }
            }
            _ => {}
        }
        out.push(tok);
    }

    Ok(out)
}

/// Boundary decision at a newline (or EOF). `prev` is the current last
/// token of the pass's output stream, `next` the first token after the
/// boundary (`Some(&Eof)` for the EOF trigger point).
fn boundary(
    stack: &[OpenBracket],
    prev: Option<&Token>,
    next: Option<&Token>,
) -> Result<Boundary, ParseError> {
    // Rule 1: no statement boundary inside `(`/`[` (braces are statement
    // context and are not suppressed by this rule).
    if matches!(
        stack.last(),
        Some(OpenBracket::Paren) | Some(OpenBracket::Bracket)
    ) {
        return Ok(Boundary::Drop);
    }
    let next = match next {
        Some(t) => t,
        // No token after the newline — malformed stream; let the parser
        // report UnexpectedEof.
        None => return Ok(Boundary::Drop),
    };
    // Rule 2: never insert before `}` (trailing expression `{ 42 }` vs
    // binding `{ x = 1 }`).
    if matches!(next.kind, TokenKind::RBrace) {
        return Ok(Boundary::Drop);
    }
    // Rule 3: the previous token cannot end a statement — the expression is
    // incomplete, continue across the line break.
    if !prev.is_some_and(|t| can_end_stmt(&t.kind)) {
        return Ok(Boundary::Drop);
    }
    // Rule 4: implicit-end set — insert an explicit semicolon.
    if implies_stmt_end(&next.kind) {
        return Ok(Boundary::InsertSemi);
    }
    // Rule 5: continuation wins; `else`/`elif`/`when`/`otherwise` are
    // construct continuations (`}` ⏎ `else`, `}` ⏎ `when` must not split);
    // block-expression statements starting with `{` / Allman-style
    // constructs must not split; an explicit `;` splits on its own.
    if can_continue_expr(&next.kind)
        || matches!(
            next.kind,
            TokenKind::Else
                | TokenKind::Elif
                | TokenKind::When
                | TokenKind::Otherwise
                | TokenKind::LBrace
        )
        || matches!(next.kind, TokenKind::Semi)
    {
        return Ok(Boundary::Drop);
    }
    // Rule 6: neither a continuation nor a start — report an error.
    Err(ParseError {
        kind: ParseErrorKind::ExpectedSemiOrNewStmt {
            found: next.kind.clone(),
        },
        span: next.span,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn kinds(src: &str) -> Vec<TokenKind> {
        let tokens = tokenize(src).expect("lexer should succeed");
        insert_semis(tokens)
            .expect("ASI pass should succeed")
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    fn kinds_err(src: &str) -> ParseError {
        let tokens = tokenize(src).expect("lexer should succeed");
        insert_semis(tokens).expect_err("ASI pass should fail")
    }

    fn semi_count(kinds: &[TokenKind]) -> usize {
        kinds
            .iter()
            .filter(|k| matches!(k, TokenKind::Semi))
            .count()
    }

    fn has_no_newline(kinds: &[TokenKind]) -> bool {
        !kinds.iter().any(|k| matches!(k, TokenKind::Newline))
    }

    // ========== Predicate functions: decision table for every TokenKind variant ==========

    #[test]
    fn can_start_stmt_table() {
        let yes: &[TokenKind] = &[
            TokenKind::Func,
            TokenKind::Const,
            TokenKind::If,
            TokenKind::While,
            TokenKind::For,
            TokenKind::Choose,
            TokenKind::Import,
            TokenKind::Pub,
            // 0.0.4 F1: `async` opens an `async func` declaration.
            TokenKind::Async,
            TokenKind::Ident(String::new()),
            TokenKind::IntLit(0),
            TokenKind::FloatLit(0.0),
            TokenKind::StringLit(String::new()),
            TokenKind::True,
            TokenKind::False,
            TokenKind::BoxType,
            TokenKind::Deref,
            TokenKind::Move,
            TokenKind::Clone,
            TokenKind::Not,
            TokenKind::Bang,
            TokenKind::Minus,
            TokenKind::LParen,
            TokenKind::LBracket,
            TokenKind::LBrace,
        ];
        for k in yes {
            assert!(can_start_stmt(k), "{k} should be able to start a stmt");
        }
        let no: &[TokenKind] = &[
            TokenKind::Elif,
            TokenKind::Else,
            TokenKind::When,
            TokenKind::Otherwise,
            TokenKind::IntType,
            TokenKind::FloatType,
            TokenKind::BoolType,
            TokenKind::StringType,
            TokenKind::UnitType,
            TokenKind::ResultType,
            TokenKind::RefType,
            TokenKind::Or,
            TokenKind::And,
            TokenKind::Assign,
            TokenKind::Eq,
            TokenKind::Ne,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::Le,
            TokenKind::Ge,
            TokenKind::Plus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::Question,
            TokenKind::As,
            TokenKind::Newline,
            TokenKind::Dot,
            TokenKind::Comma,
            TokenKind::Colon,
            TokenKind::Semi,
            TokenKind::RParen,
            TokenKind::RBrace,
            TokenKind::RBracket,
            TokenKind::Arrow,
            TokenKind::Eof,
            TokenKind::In,
            // 0.0.4: `await` / `channel` are reserved words that cannot start
            // a statement (non-expression-starting; rejected by the
            // statement-entry guard, same as `as` / `ref`). `async` *can* —
            // it opens an `async func` declaration (F1).
            TokenKind::Await,
            TokenKind::Channel,
        ];
        for k in no {
            assert!(!can_start_stmt(k), "{k} should not start a stmt");
        }
    }

    #[test]
    fn implies_stmt_end_table() {
        // Every token in the start set implies an end, except `(` and `-`
        // (continuation wins).
        assert!(implies_stmt_end(&TokenKind::Func));
        assert!(implies_stmt_end(&TokenKind::Ident(String::new())));
        assert!(implies_stmt_end(&TokenKind::IntLit(0)));
        assert!(implies_stmt_end(&TokenKind::Bang));
        assert!(implies_stmt_end(&TokenKind::Not));
        assert!(implies_stmt_end(&TokenKind::Deref));
        assert!(implies_stmt_end(&TokenKind::BoxType));
        assert!(implies_stmt_end(&TokenKind::Move));
        assert!(implies_stmt_end(&TokenKind::Clone));
        assert!(implies_stmt_end(&TokenKind::True));
        assert!(implies_stmt_end(&TokenKind::If));
        assert!(implies_stmt_end(&TokenKind::While));
        assert!(implies_stmt_end(&TokenKind::Choose));
        assert!(implies_stmt_end(&TokenKind::Import));
        assert!(implies_stmt_end(&TokenKind::Const));
        assert!(!implies_stmt_end(&TokenKind::Minus));
        assert!(!implies_stmt_end(&TokenKind::LParen));
        // V03: `[` can start a statement but never ends one — it is in the
        // continuation set, so `f()` ⏎ `[0]` keeps joining as an index chain.
        assert!(!implies_stmt_end(&TokenKind::LBracket));
        assert!(!implies_stmt_end(&TokenKind::LBrace));
        // Terminator set.
        assert!(implies_stmt_end(&TokenKind::RBrace));
        assert!(implies_stmt_end(&TokenKind::Eof));
        // Neither start nor terminator → no.
        assert!(!implies_stmt_end(&TokenKind::Plus));
        assert!(!implies_stmt_end(&TokenKind::Else));
        assert!(!implies_stmt_end(&TokenKind::Semi));
        assert!(!implies_stmt_end(&TokenKind::Colon));
    }

    #[test]
    fn can_continue_expr_table() {
        let yes: &[TokenKind] = &[
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Percent,
            TokenKind::Eq,
            TokenKind::Ne,
            TokenKind::Lt,
            TokenKind::Gt,
            TokenKind::Le,
            TokenKind::Ge,
            TokenKind::And,
            TokenKind::Or,
            TokenKind::Question,
            TokenKind::Dot,
            TokenKind::LBracket,
            TokenKind::LParen,
            TokenKind::As,
            TokenKind::In,
        ];
        for k in yes {
            assert!(can_continue_expr(k), "{k} should continue an expr");
        }
        let no: &[TokenKind] = &[
            TokenKind::Func,
            TokenKind::If,
            TokenKind::Else,
            TokenKind::Ident(String::new()),
            TokenKind::IntLit(0),
            TokenKind::Assign,
            TokenKind::Comma,
            TokenKind::Colon,
            TokenKind::Semi,
            TokenKind::RBrace,
            TokenKind::Newline,
            TokenKind::Eof,
        ];
        for k in no {
            assert!(!can_continue_expr(k), "{k} should not continue an expr");
        }
    }

    #[test]
    fn can_end_stmt_boundary() {
        // Endable set: a statement may finish here.
        assert!(can_end_stmt(&TokenKind::Ident(String::new())));
        assert!(can_end_stmt(&TokenKind::IntLit(0)));
        assert!(can_end_stmt(&TokenKind::StringLit(String::new())));
        assert!(can_end_stmt(&TokenKind::True));
        assert!(can_end_stmt(&TokenKind::RParen));
        assert!(can_end_stmt(&TokenKind::RBracket));
        assert!(can_end_stmt(&TokenKind::RBrace));
        assert!(can_end_stmt(&TokenKind::Question));
        // 0.0.2 U13: cast expressions end with a type keyword, which must be
        // able to close a statement.
        assert!(can_end_stmt(&TokenKind::StringType));
        assert!(can_end_stmt(&TokenKind::IntType));
        // Not endable: the expression is necessarily incomplete.
        assert!(!can_end_stmt(&TokenKind::Plus));
        assert!(!can_end_stmt(&TokenKind::Assign));
        assert!(!can_end_stmt(&TokenKind::Move));
        assert!(!can_end_stmt(&TokenKind::BoxType));
        assert!(!can_end_stmt(&TokenKind::LParen));
        assert!(!can_end_stmt(&TokenKind::LBrace));
        assert!(!can_end_stmt(&TokenKind::Semi));
        assert!(!can_end_stmt(&TokenKind::Else));
    }

    // ========== Pass: golden cases (ASI.md §5.1 table) ==========

    #[test]
    fn inserts_semi_between_stmts() {
        // `x = 1` ⏎ `y = 2` → two statements: one Semi inserted at the newline.
        let ks = kinds("x = 1\ny = 2\n");
        assert_eq!(semi_count(&ks), 2);
        assert!(has_no_newline(&ks));
    }

    #[test]
    fn keeps_explicit_semis() {
        // Explicit semicolons are always legal; the pass does not insert
        // duplicates (a newline after `;` does not split, EOF adds nothing).
        let ks = kinds("x = 1;\ny = 2;\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn no_semi_before_closing_brace() {
        // `{ x = 1` ⏎ `}` — never insert before `}` (parser-side binding
        // tolerance is the safety net).
        let ks = kinds("func f() {\nx = 1\n}\n");
        // Only 1 added at EOF.
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn no_semi_inside_parens() {
        // No statement boundary inside parens: multi-line arguments do not split.
        let ks = kinds("f(a,\nb)\n");
        assert_eq!(semi_count(&ks), 1); // only the one at trailing EOF
    }

    #[test]
    fn semi_inside_braces_in_parens() {
        // ASI still applies inside `{`, even when the outer delimiter is `(`
        // (ASI.md §3.1 rule 1).
        let ks = kinds("f({\nx = 1\ny = 2\n})\n");
        // One inserted before `y = 2`; none before `}`; 1 added at EOF after `)`.
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn join_binary_operator_across_lines() {
        // `x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`: `-` continues, no split.
        let ks = kinds("x = 1\n- 2\n");
        assert_eq!(semi_count(&ks), 1); // only at EOF
    }

    #[test]
    fn join_call_paren_across_lines() {
        // `f()` ⏎ `(g())` ⇒ call chain.
        let ks = kinds("f()\n(g())\n");
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn join_as_cast_across_lines() {
        // `x = 42` ⏎ `as string` ⇒ `x = 42 as string` (`as` continues, no split).
        let ks = kinds("x = 42\nas string\n");
        assert_eq!(semi_count(&ks), 1); // only the one at trailing EOF
    }

    #[test]
    fn join_index_across_lines() {
        // `f()` ⏎ `[0]` ⇒ `f()[0]` (ASI.md §5.1 table: `[` ∈ continuation set).
        let ks = kinds("f()\n[0]\n");
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn array_literal_continues_binding_across_lines() {
        // 0.0.3 V03: `x =` ⏎ `[1, 2]` — the binding is incomplete, so the
        // array literal joins across the line break (PLAN §3.2.2 trap case).
        let ks = kinds("x =\n[1, 2]\n");
        assert_eq!(semi_count(&ks), 1); // only at EOF
    }

    #[test]
    fn array_literal_statement_after_explicit_semi() {
        // 0.0.3 V03: `[1, 2];` starts a new statement after an explicit
        // `;` boundary — the token-based ASI cannot tell index from literal,
        // so a literal statement requires the boundary (the `;` escape hatch).
        let ks = kinds("x = 1;\n[1, 2];\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn complete_stmt_joins_bracket_as_index_trap() {
        // 0.0.3 V03 documented trap: `x = 1` ⏎ `[1, 2]` joins as
        // `x = 1[1, 2]` — an index on the literal, which the parser rejects.
        // The ASI layer is token-based and cannot see the difference; the
        // `;`-always-legal escape hatch is the fix (PLAN §3.2.2).
        let ks = kinds("x = 1\n[1, 2]\n");
        assert_eq!(semi_count(&ks), 1); // joined, no split inserted
    }

    #[test]
    fn split_before_bang() {
        // `x = 1` ⏎ `!flag` → two statements (`!` is not a continuation).
        let ks = kinds("x = 1\n!flag\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn no_semi_before_else() {
        // `if c { 1 }` ⏎ `else { 2 }` is one if-else: construct continuation
        // does not split.
        let ks = kinds("if c { 1 }\nelse { 2 }\n");
        assert_eq!(semi_count(&ks), 1); // only where the if-else statement ends
    }

    #[test]
    fn no_semi_before_when_arm() {
        // `}` ⏎ `when` — choose arm continuation does not split.
        let ks = kinds("choose x {\nwhen 0 { a }\nwhen 1 { b }\n}\n");
        // Only one, where the choose statement ends (EOF).
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn semi_after_if_stmt_followed_by_stmt() {
        // `if c { 1 }` ⏎ `y = 2` — a semicolon is inserted after the if
        // statement completes.
        let ks = kinds("if c { 1 }\ny = 2\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn semi_at_eof() {
        // EOF is a trigger point: `x = 1` EOF adds a semicolon; `x = 1;` EOF
        // does not duplicate.
        assert_eq!(semi_count(&kinds("x = 1")), 1);
        assert_eq!(semi_count(&kinds("x = 1;")), 1);
    }

    #[test]
    fn prefix_keyword_joins_across_lines() {
        // `move` / `!` and other prefixes cannot end a statement, so the
        // operand joins across the line break.
        let ks = kinds("y = move\nx\n");
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn struct_literal_joins_across_lines() {
        // 0.0.3 V02: `p = Point` ⏎ `{ x: 1 }` — `{` is a construct
        // continuation, so the struct literal joins across the line break.
        let ks = kinds("p = Point\n{ x: 1, y: 2 }\n");
        assert_eq!(semi_count(&ks), 1); // at EOF; nothing splits the literal
    }

    #[test]
    fn struct_decl_allman_joins_across_lines() {
        // `struct Point` ⏎ `{` — same construct-continuation rule; the
        // Allman-style declaration is one statement.
        let ks = kinds("struct Point\n{ x: int }\n");
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn blank_lines_collapse() {
        let ks = kinds("x = 1\n\n\n\ny = 2\n");
        assert_eq!(semi_count(&ks), 2);
        assert!(has_no_newline(&ks));
    }

    #[test]
    fn for_header_joins_across_lines() {
        // 0.0.3 V04: `for x` ⏎ `in names` ⏎ `{ … }` — `in` is a continuation
        // token, so the whole header is one construct; the body block follows.
        let ks = kinds("for x\nin names\n{\nprint(x)\n}\n");
        assert_eq!(semi_count(&ks), 1); // where the for statement ends
        assert!(has_no_newline(&ks));
    }

    #[test]
    fn for_stmt_boundary_after_block() {
        // `for x in a { }` ⏎ `print(1)` — the statement boundary after the
        // closing brace inserts a semicolon, exactly like `if`/`while`.
        let ks = kinds("for x in a { }\nprint(1)\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn rejects_boundary_garbage() {
        // `x = 1` ⏎ `)` — neither a continuation nor a start.
        let err = kinds_err("x = 1\n)");
        assert!(matches!(
            err.kind,
            ParseErrorKind::ExpectedSemiOrNewStmt { .. }
        ));
    }

    #[test]
    fn double_semicolon_passes_through_to_parser() {
        // Both semicolons of `;;` are preserved; the error is raised by the
        // parser's statement-entry guard.
        let ks = kinds("x = 1;;\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn empty_and_trivial_inputs() {
        // Empty file, single newline: nothing inserted, no error.
        assert!(has_no_newline(&kinds("")));
        assert_eq!(semi_count(&kinds("\n")), 0);
    }
}
