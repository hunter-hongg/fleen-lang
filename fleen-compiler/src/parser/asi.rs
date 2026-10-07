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

/// 该 token 能否开启一条新语句（块起始位置用，含 `(` `-` `!`/`not` `{`）。
///
/// `not` 与 `!` 是同一运算符的两种拼写，集合同时收录两者；
/// `{` 用于块表达式语句（`{ 42 }`）。
pub(crate) fn can_start_stmt(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Func
            | TokenKind::Const
            | TokenKind::If
            | TokenKind::While
            | TokenKind::Choose
            | TokenKind::Import
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
            | TokenKind::LBrace
    )
}

/// 语句完成后，该 token 是否意味着"隐式结束"（pass 据此插入分号）。
///
/// 起始集 ∖ {`(`, `-`, `{`} ∪ {`}`, EOF}——`(` 与 `-` 同属续接集，续接优先，
/// 不可据此结束语句（`x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`）；`{` 不在其中，
/// 使 Allman 风格（`if c` ⏎ `{`）不断句（ASI.md §3.2、§5 #4）。
pub(crate) fn implies_stmt_end(kind: &TokenKind) -> bool {
    match kind {
        TokenKind::Minus | TokenKind::LParen | TokenKind::LBrace => false,
        TokenKind::RBrace | TokenKind::Eof => true,
        other => can_start_stmt(other),
    }
}

/// 该 token 是否续接当前表达式（二元运算符 / `?` / `.` / `[` / `(` / `as`）。
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
    )
}

/// 该 token 能否作为一条语句的最后一个 token（pass 规则 3 的 p 侧判定）。
///
/// 0.0.2 U13：类型关键字也在此处——`e as string` 的语句以 `string` 结尾，
/// 不收录则 EOF/换行处补不上分号，错误会前移到 parser 而不是 typeck。
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

/// 未闭合括号种类（pass 的栈元素）。
#[derive(Copy, Clone, PartialEq, Eq)]
enum OpenBracket {
    Paren,
    Bracket,
    Brace,
}

/// 换行边界处的动作。
enum Boundary {
    /// 丢弃换行，不断句。
    Drop,
    /// 插入一个显式 `Semi`。
    InsertSemi,
}

/// ASI pass：消费全部 `Newline`，在语句边界插入显式 `Semi`。
///
/// 判定规则（ASI.md §3.1，换行或 EOF 处，`p` 前一 token、`n` 后一 token）：
/// 1. 最内层未闭合括号是 `(` 或 `[` → 丢弃（`{` 内 ASI 照常生效）；
/// 2. `n` 是 `}` → 丢弃（永不在 `}` 前插——尾表达式 vs 绑定由 parser 结构区分）；
/// 3. `p` 不能结尾语句 → 丢弃（跨行续接）；
/// 4. `n` ∈ 隐式结束集 → 插入 `Semi`；
/// 5. `n` ∈ 续接集 ∪ 构造延续集 ∪ {`;`} → 丢弃（续接优先）；
/// 6. 其余 → 报 `ExpectedSemiOrNewStmt`。
pub(crate) fn insert_semis(tokens: Vec<Token>) -> Result<Vec<Token>, ParseError> {
    let mut out: Vec<Token> = Vec::with_capacity(tokens.len());
    let mut stack: Vec<OpenBracket> = Vec::new();
    let mut iter = tokens.into_iter().peekable();

    while let Some(tok) = iter.next() {
        if matches!(tok.kind, TokenKind::Newline) {
            // 折叠连续换行（lexer 已折叠空白，防御性再兜一层），
            // 决策所用的 `n` 是换行后第一个真实 token。
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
                    out.push(Token::new(TokenKind::Semi, Span::new(end, end)));
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
            // 不匹配的闭括号不碰栈——parser 随后会在正确的位置报错
            TokenKind::Eof => {
                // EOF 是触发点：`x = 1` EOF 也需要补分号。out 为空（空文件）
                // 或前一 token 是 `;` 时自然落入 Drop 分支。
                match boundary(&stack, out.last(), Some(&tok))? {
                    Boundary::Drop => {}
                    Boundary::InsertSemi => {
                        let end = out.last().map_or(0, |t| t.span.end);
                        out.push(Token::new(TokenKind::Semi, Span::new(end, end)));
                    }
                }
            }
            _ => {}
        }
        out.push(tok);
    }

    Ok(out)
}

/// 换行（或 EOF）边界判定。`prev` 是 pass 输出流当前的最后一个 token，
/// `next` 是边界后的第一个 token（EOF 触发点时为 `Some(&Eof)`）。
fn boundary(
    stack: &[OpenBracket],
    prev: Option<&Token>,
    next: Option<&Token>,
) -> Result<Boundary, ParseError> {
    // 规则 1：`( `/`[` 内没有语句边界（花括号是语句上下文，不受此条压制）。
    if matches!(
        stack.last(),
        Some(OpenBracket::Paren) | Some(OpenBracket::Bracket)
    ) {
        return Ok(Boundary::Drop);
    }
    let next = match next {
        Some(t) => t,
        // 换行后没有 token——畸形流，交给 parser 报 UnexpectedEof。
        None => return Ok(Boundary::Drop),
    };
    // 规则 2：永不在 `}` 前插（`{ 42 }` 尾表达式 vs `{ x = 1 }` 绑定）。
    if matches!(next.kind, TokenKind::RBrace) {
        return Ok(Boundary::Drop);
    }
    // 规则 3：前一 token 不能结尾语句——表达式未完，跨行续接。
    if !prev.is_some_and(|t| can_end_stmt(&t.kind)) {
        return Ok(Boundary::Drop);
    }
    // 规则 4：隐式结束集——插入显式分号。
    if implies_stmt_end(&next.kind) {
        return Ok(Boundary::InsertSemi);
    }
    // 规则 5：续接优先；`else`/`elif`/`when`/`otherwise` 是构造延续
    // （`}` ⏎ `else`、`}` ⏎ `when` 不能断句）；`{` 起始的块表达式语句
    // / Allman 风格构造体不能断句；显式 `;` 自行断句。
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
    // 规则 6：既不能续接也不能起始——报错。
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

    // ========== 谓词函数：对每个 TokenKind 变体的判定表 ==========

    #[test]
    fn can_start_stmt_table() {
        let yes: &[TokenKind] = &[
            TokenKind::Func,
            TokenKind::Const,
            TokenKind::If,
            TokenKind::While,
            TokenKind::Choose,
            TokenKind::Import,
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
            TokenKind::LBracket,
            TokenKind::RBracket,
            TokenKind::Arrow,
            TokenKind::Eof,
        ];
        for k in no {
            assert!(!can_start_stmt(k), "{k} should not start a stmt");
        }
    }

    #[test]
    fn implies_stmt_end_table() {
        // 起始集内的 token 全部隐式结束，除了 `(` 与 `-`（续接优先）。
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
        assert!(!implies_stmt_end(&TokenKind::LBrace));
        // 终止集。
        assert!(implies_stmt_end(&TokenKind::RBrace));
        assert!(implies_stmt_end(&TokenKind::Eof));
        // 非起始、非终止 → 否。
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
        // 可结尾集：语句可能在此完结。
        assert!(can_end_stmt(&TokenKind::Ident(String::new())));
        assert!(can_end_stmt(&TokenKind::IntLit(0)));
        assert!(can_end_stmt(&TokenKind::StringLit(String::new())));
        assert!(can_end_stmt(&TokenKind::True));
        assert!(can_end_stmt(&TokenKind::RParen));
        assert!(can_end_stmt(&TokenKind::RBracket));
        assert!(can_end_stmt(&TokenKind::RBrace));
        assert!(can_end_stmt(&TokenKind::Question));
        // 0.0.2 U13: cast 表达式以类型关键字结尾，必须能收尾语句。
        assert!(can_end_stmt(&TokenKind::StringType));
        assert!(can_end_stmt(&TokenKind::IntType));
        // 不可结尾：表达式必然未完。
        assert!(!can_end_stmt(&TokenKind::Plus));
        assert!(!can_end_stmt(&TokenKind::Assign));
        assert!(!can_end_stmt(&TokenKind::Move));
        assert!(!can_end_stmt(&TokenKind::BoxType));
        assert!(!can_end_stmt(&TokenKind::LParen));
        assert!(!can_end_stmt(&TokenKind::LBrace));
        assert!(!can_end_stmt(&TokenKind::Semi));
        assert!(!can_end_stmt(&TokenKind::Else));
    }

    // ========== pass：golden 用例（ASI.md §5.1 表） ==========

    #[test]
    fn inserts_semi_between_stmts() {
        // `x = 1` ⏎ `y = 2` → 两条语句：换行处插入一个 Semi。
        let ks = kinds("x = 1\ny = 2\n");
        assert_eq!(semi_count(&ks), 2);
        assert!(has_no_newline(&ks));
    }

    #[test]
    fn keeps_explicit_semis() {
        // 显式分号永远合法，pass 不重复插入（`;` 后换行不断句、EOF 不补）。
        let ks = kinds("x = 1;\ny = 2;\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn no_semi_before_closing_brace() {
        // `{ x = 1` ⏎ `}` —— 永不在 `}` 前插（parser 侧绑定容忍兜底）。
        let ks = kinds("func f() {\nx = 1\n}\n");
        // 仅 EOF 处补 1。
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn no_semi_inside_parens() {
        // 括号内没有语句边界：实参跨行不断句。
        let ks = kinds("f(a,\nb)\n");
        assert_eq!(semi_count(&ks), 1); // 仅末尾 EOF 处一个
    }

    #[test]
    fn semi_inside_braces_in_parens() {
        // `{` 内 ASI 照常生效，即使外层是 `(`（ASI.md §3.1 规则 1）。
        let ks = kinds("f({\nx = 1\ny = 2\n})\n");
        // `y = 2` 前插入一个；`}` 前不插；`)` 后 EOF 处补 1。
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn join_binary_operator_across_lines() {
        // `x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`：`-` 续接优先，不断句。
        let ks = kinds("x = 1\n- 2\n");
        assert_eq!(semi_count(&ks), 1); // 仅 EOF 处
    }

    #[test]
    fn join_call_paren_across_lines() {
        // `f()` ⏎ `(g())` ⇒ 调用链。
        let ks = kinds("f()\n(g())\n");
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn join_as_cast_across_lines() {
        // `x = 42` ⏎ `as string` ⇒ `x = 42 as string`（`as` 续接，不断句）。
        let ks = kinds("x = 42\nas string\n");
        assert_eq!(semi_count(&ks), 1); // 仅末尾 EOF 处一个
    }

    #[test]
    fn join_index_across_lines() {
        // `f()` ⏎ `[0]` ⇒ `f()[0]`（ASI.md §5.1 表：`[` ∈ 续接集）。
        let ks = kinds("f()\n[0]\n");
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn split_before_bang() {
        // `x = 1` ⏎ `!flag` → 两条语句（`!` 非续接）。
        let ks = kinds("x = 1\n!flag\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn no_semi_before_else() {
        // `if c { 1 }` ⏎ `else { 2 }` 是一个 if-else：构造延续不断句。
        let ks = kinds("if c { 1 }\nelse { 2 }\n");
        assert_eq!(semi_count(&ks), 1); // 仅末尾 if-else 语句完结处
    }

    #[test]
    fn no_semi_before_when_arm() {
        // `}` ⏎ `when` —— choose 臂延续不断句。
        let ks = kinds("choose x {\nwhen 0 { a }\nwhen 1 { b }\n}\n");
        // 仅 choose 语句完结处（EOF）一个。
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn semi_after_if_stmt_followed_by_stmt() {
        // `if c { 1 }` ⏎ `y = 2` —— if 语句完结后插分号。
        let ks = kinds("if c { 1 }\ny = 2\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn semi_at_eof() {
        // EOF 是触发点：`x = 1` EOF 补分号；`x = 1;` EOF 不重复。
        assert_eq!(semi_count(&kinds("x = 1")), 1);
        assert_eq!(semi_count(&kinds("x = 1;")), 1);
    }

    #[test]
    fn prefix_keyword_joins_across_lines() {
        // `move` / `!` 等前缀不能结尾语句，跨行连上操作数。
        let ks = kinds("y = move\nx\n");
        assert_eq!(semi_count(&ks), 1);
    }

    #[test]
    fn blank_lines_collapse() {
        let ks = kinds("x = 1\n\n\n\ny = 2\n");
        assert_eq!(semi_count(&ks), 2);
        assert!(has_no_newline(&ks));
    }

    #[test]
    fn rejects_boundary_garbage() {
        // `x = 1` ⏎ `)` —— 既不能续接也不能起始。
        let err = kinds_err("x = 1\n)");
        assert!(matches!(
            err.kind,
            ParseErrorKind::ExpectedSemiOrNewStmt { .. }
        ));
    }

    #[test]
    fn double_semicolon_passes_through_to_parser() {
        // `;;` 的两个分号都保留；报错由 parser 的语句入口 guard 给出。
        let ks = kinds("x = 1;;\n");
        assert_eq!(semi_count(&ks), 2);
    }

    #[test]
    fn empty_and_trivial_inputs() {
        // 空文件、单个换行：不插入、不报错。
        assert!(has_no_newline(&kinds("")));
        assert_eq!(semi_count(&kinds("\n")), 0);
    }
}
