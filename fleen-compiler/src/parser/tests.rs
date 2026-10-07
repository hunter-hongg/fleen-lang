//! Unit tests for the parser.

#[cfg(test)]
mod tests {
    use crate::lexer::tokenize;
    use crate::parser::ast::*;
    use crate::parser::{ParseError, ParseErrorKind, parse};

    /// Helper: tokenize and parse, asserting success.
    fn parse_str(src: &str) -> Ast {
        let tokens = tokenize(src).expect("lexer should succeed");
        parse(tokens).expect("parser should succeed")
    }

    /// Helper: tokenize and parse, asserting error.
    fn parse_err(src: &str) -> ParseError {
        let tokens = tokenize(src).expect("lexer should succeed");
        parse(tokens).expect_err("parser should fail")
    }

    // ========== Literals ==========

    #[test]
    fn parse_int_literal() {
        let ast = parse_str("42; // comment");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Int(42, _))));
    }

    #[test]
    fn parse_float_literal() {
        let ast = parse_str("3.1;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Float(f, _)) if (f - 3.1f64).abs() < 0.005));
    }

    #[test]
    fn parse_bool_true() {
        let ast = parse_str("true;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Bool(true, _))));
    }

    #[test]
    fn parse_bool_false() {
        let ast = parse_str("false;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Bool(false, _))));
    }

    #[test]
    fn parse_string_literal() {
        let ast = parse_str(r#""hello world";"#);
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Str(s, _)) if s == "hello world"));
    }

    // ========== Identifiers ==========

    #[test]
    fn parse_identifier() {
        let ast = parse_str("my_var;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Ident(s, _)) if s == "my_var"));
    }

    // ========== Variable Binding ==========

    #[test]
    fn parse_var_binding_simple() {
        // At top level, x = 42; is parsed as a VarBinding declaration
        // The resolver determines if this is first-use (binding) or reuse (assignment).
        let ast = parse_str("x = 42;");
        let item = &ast.items[0];
        match item {
            Item::Decl(Decl::Var(v)) => {
                assert_eq!(v.name, "x");
                assert_eq!(v.ty, None);
                assert!(matches!(&*v.init, Expr::Int(42, _)));
            }
            _ => panic!("expected Var binding, got {:?}", item),
        }
    }

    #[test]
    fn parse_var_binding_typed() {
        // x: int = 42; — typed binding at top level
        let ast = parse_str("x: int = 42;");
        let item = &ast.items[0];
        match item {
            Item::Decl(Decl::Var(v)) => {
                assert_eq!(v.name, "x");
                assert_eq!(v.ty, Some(Type::Base(BaseType::Int)));
                assert!(matches!(&*v.init, Expr::Int(42, _)));
            }
            _ => panic!("expected Var binding, got {:?}", item),
        }
    }

    #[test]
    fn parse_assignment_in_block() {
        // x = 42; in a block is parsed as VarBinding (binding/assignment unified at parse time)
        // The resolver determines semantics.
        let ast = parse_str("func main() { x = 42; }");
        match &ast.items[0] {
            Item::Decl(Decl::Func(f)) => {
                if let FuncBody::Block(block) = &f.body {
                    assert_eq!(block.stmts.len(), 1);
                    assert!(matches!(block.stmts[0], Stmt::Decl(Decl::Var(_))));
                }
            }
            _ => panic!("expected func"),
        }
    }

    // ========== Const Declaration ==========

    #[test]
    fn parse_const_decl_simple() {
        let ast = parse_str("const y = 42;");
        let item = &ast.items[0];
        if let Item::Decl(Decl::Const(c)) = item {
            assert_eq!(c.name, "y");
            assert_eq!(c.ty, None);
            assert!(matches!(*c.init, Expr::Int(42, _)));
        } else {
            panic!("expected Const decl, got {:?}", item);
        }
    }

    #[test]
    fn parse_const_decl_typed() {
        let ast = parse_str("const y: int = 42;");
        let item = &ast.items[0];
        if let Item::Decl(Decl::Const(c)) = item {
            assert_eq!(c.name, "y");
            assert_eq!(c.ty, Some(Type::Base(BaseType::Int)));
            assert!(matches!(*c.init, Expr::Int(42, _)));
        } else {
            panic!("expected Const decl, got {:?}", item);
        }
    }

    // ========== Function Declaration ==========

    #[test]
    fn parse_func_single_line() {
        let ast = parse_str("func add(a: int, b: int): int = a + b");
        let item = &ast.items[0];
        if let Item::Decl(Decl::Func(f)) = item {
            assert_eq!(f.name, "add");
            assert_eq!(f.params.len(), 2);
            assert_eq!(f.params[0].name, "a");
            assert_eq!(f.ret_type, Some(Type::Base(BaseType::Int)));
            assert!(matches!(f.body, FuncBody::SingleExpr(_)));
        } else {
            panic!("expected Func decl, got {:?}", item);
        }
    }

    #[test]
    fn parse_func_no_params() {
        let ast = parse_str("func main() { }");
        let item = &ast.items[0];
        if let Item::Decl(Decl::Func(f)) = item {
            assert_eq!(f.name, "main");
            assert_eq!(f.params.len(), 0);
            assert!(matches!(f.body, FuncBody::Block(_)));
        } else {
            panic!("expected Func decl, got {:?}", item);
        }
    }

    #[test]
    fn parse_func_multi_line() {
        let ast = parse_str("func fib(n: int): int {\n    if n < 2 { n } else { 0 }\n}");
        let item = &ast.items[0];
        if let Item::Decl(Decl::Func(f)) = item {
            assert_eq!(f.name, "fib");
            assert!(matches!(f.body, FuncBody::Block(_)));
        } else {
            panic!("expected Func decl, got {:?}", item);
        }
    }

    // ========== Import ==========

    #[test]
    fn parse_import() {
        let ast = parse_str("import std.io;");
        let item = &ast.items[0];
        if let Item::Import(import) = item {
            assert_eq!(import.path, vec!["std".to_string(), "io".to_string()]);
        } else {
            panic!("expected Import, got {:?}", item);
        }
    }

    #[test]
    fn parse_import_nested() {
        let ast = parse_str("import a.b.c;");
        let item = &ast.items[0];
        if let Item::Import(import) = item {
            assert_eq!(import.path, vec!["a", "b", "c"]);
        } else {
            panic!("expected Import, got {:?}", item);
        }
    }

    // ========== Binary Operators ==========

    #[test]
    fn parse_addition() {
        let ast = parse_str("1 + 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Add(_, _))));
    }

    #[test]
    fn parse_subtraction() {
        let ast = parse_str("1 - 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Sub(_, _))));
    }

    #[test]
    fn parse_multiplication() {
        let ast = parse_str("3 * 4;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Mul(_, _))));
    }

    #[test]
    fn parse_division() {
        let ast = parse_str("10 / 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Div(_, _))));
    }

    #[test]
    fn parse_modulo() {
        let ast = parse_str("10 % 3;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Mod(_, _))));
    }

    #[test]
    fn parse_comparison_ops() {
        let ast = parse_str("1 < 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Lt(_, _))));

        let ast = parse_str("1 > 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Gt(_, _))));

        let ast = parse_str("1 <= 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Le(_, _))));

        let ast = parse_str("1 >= 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Ge(_, _))));
    }

    #[test]
    fn parse_equality_ops() {
        let ast = parse_str("1 == 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Eq(_, _))));

        let ast = parse_str("1 != 2;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Ne(_, _))));
    }

    #[test]
    fn parse_logical_ops() {
        let ast = parse_str("true or false;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Or(_, _))));

        let ast = parse_str("true and false;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::And(_, _))));
    }

    #[test]
    fn parse_unary_ops() {
        let ast = parse_str("-5;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Neg(_))));

        let ast = parse_str("!false;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Not(_))));
    }

    #[test]
    fn parse_not_keyword() {
        let ast = parse_str("not false;");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::Not(_))));
    }

    #[test]
    fn parse_precedence_ordering() {
        // * binds tighter than +
        let ast = parse_str("1 + 2 * 3;");
        let item = &ast.items[0];
        // Should be: 1 + (2 * 3)
        if let Item::Expr(Expr::Add(lhs, rhs)) = item {
            assert!(matches!(**lhs, Expr::Int(1, _)));
            assert!(matches!(**rhs, Expr::Mul(_, _)));
        } else {
            panic!("expected Add as top-level, got {:?}", item);
        }
    }

    #[test]
    fn parse_right_associative_assignment() {
        // a = b = 5; parses as: a is a Var binding with init = Assign(b, 5)
        // This is because `a` followed by `=` triggers binding detection
        let ast = parse_str("a = b = 5;");
        let item = &ast.items[0];
        match item {
            Item::Decl(Decl::Var(v)) => {
                assert_eq!(v.name, "a");
                // The init should be an Assign expression: b = 5
                assert!(matches!(&*v.init, Expr::Assign(_, _)));
            }
            _ => panic!("expected Var binding, got {:?}", item),
        }
    }

    // ========== If Expression ==========

    #[test]
    fn parse_if_simple() {
        let ast = parse_str("if true { 1 } else { 2 };");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::If(_))));
    }

    #[test]
    fn parse_if_with_elif() {
        let ast = parse_str("if x > 0 { 1 } elif x < 0 { 2 } else { 3 };");
        let item = &ast.items[0];
        if let Item::Expr(Expr::If(if_expr)) = item {
            assert_eq!(if_expr.elif_branches.len(), 1);
            assert!(if_expr.else_branch.is_some());
        } else {
            panic!("expected If expr, got {:?}", item);
        }
    }

    #[test]
    fn parse_if_no_else() {
        let ast = parse_str("if true { 1 };");
        let item = &ast.items[0];
        if let Item::Expr(Expr::If(if_expr)) = item {
            assert!(if_expr.else_branch.is_none());
        } else {
            panic!("expected If expr, got {:?}", item);
        }
    }

    // ========== While Expression ==========

    #[test]
    fn parse_while() {
        let ast = parse_str("while x < 10 { x = x + 1; };");
        let item = &ast.items[0];
        assert!(matches!(item, Item::Expr(Expr::While(_))));
    }

    // ========== Choose Expression ==========

    #[test]
    fn parse_choose_simple() {
        let ast = parse_str(
            "choose x { when 1 { \"one\" } when 2 { \"two\" } otherwise { \"other\" } };",
        );
        let item = &ast.items[0];
        if let Item::Expr(Expr::Choose(ch)) = item {
            assert_eq!(ch.arms.len(), 3);
        } else {
            panic!("expected Choose expr, got {:?}", item);
        }
    }

    #[test]
    fn parse_choose_with_guard() {
        let ast = parse_str("choose x { when n if n > 10 { \"big\" } otherwise { \"small\" } };");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Choose(ch)) = item {
            assert_eq!(ch.arms.len(), 2);
            assert!(ch.arms[0].guard.is_some());
            assert!(ch.arms[1].guard.is_none());
        } else {
            panic!("expected Choose expr, got {:?}", item);
        }
    }

    // ========== Function Call ==========

    #[test]
    fn parse_function_call() {
        let ast = parse_str("foo(1, 2);");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Call(func, args)) = item {
            assert!(matches!(**func, Expr::Ident(ref s, _) if s == "foo"));
            assert_eq!(args.len(), 2);
        } else {
            panic!("expected Call, got {:?}", item);
        }
    }

    #[test]
    fn parse_nested_call() {
        let ast = parse_str("foo(bar(1));");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Call(outer_func, outer_args)) = item {
            assert!(matches!(**outer_func, Expr::Ident(ref s, _) if s == "foo"));
            assert_eq!(outer_args.len(), 1);
            assert!(matches!(outer_args[0], Expr::Call(_, _)));
        } else {
            panic!("expected nested Call, got {:?}", item);
        }
    }

    // ========== Field Access and Indexing ==========

    #[test]
    fn parse_field_access() {
        let ast = parse_str("obj.field;");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Field(expr, field)) = item {
            assert!(matches!(**expr, Expr::Ident(ref s, _) if s == "obj"));
            assert_eq!(field, "field");
        } else {
            panic!("expected Field, got {:?}", item);
        }
    }

    #[test]
    fn parse_index_access() {
        let ast = parse_str("arr[0];");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Index(arr, idx)) = item {
            assert!(matches!(**arr, Expr::Ident(ref s, _) if s == "arr"));
            assert!(matches!(**idx, Expr::Int(0, _)));
        } else {
            panic!("expected Index, got {:?}", item);
        }
    }

    // ========== Parenthesized Expressions ==========

    #[test]
    fn parse_parens() {
        let ast = parse_str("(1 + 2) * 3;");
        let item = &ast.items[0];
        // Should be: (1 + 2) * 3 → Mul(Add(1,2), 3), NOT Add(1, 2*3)
        if let Item::Expr(Expr::Mul(lhs, rhs)) = item {
            assert!(matches!(**lhs, Expr::Add(_, _)));
            assert!(matches!(**rhs, Expr::Int(3, _)));
        } else {
            panic!("expected Mul as top-level, got {:?}", item);
        }
    }

    // ========== Block Expressions ==========

    #[test]
    fn parse_block_expr() {
        let ast = parse_str("{ x = 1; y = 2; x + y };");
        let item = &ast.items[0];
        match item {
            Item::Expr(Expr::Block(block)) => {
                assert_eq!(block.stmts.len(), 2);
                assert!(block.tail_expr.is_some());
            }
            _ => panic!("expected Block expr, got {:?}", item),
        }
    }

    #[test]
    fn parse_empty_block() {
        let ast = parse_str("{};");
        // Empty block at top level is an expression that returns void
        // Per the grammar: block is a primary expression
        let item = &ast.items[0];
        match item {
            Item::Expr(Expr::Block(block)) => {
                assert_eq!(block.stmts.len(), 0);
                assert!(block.tail_expr.is_none());
            }
            _ => panic!("expected Block expr, got {:?}", item),
        }
    }

    // ========== Complex Expressions ==========

    #[test]
    fn parse_complex_expression() {
        let ast = parse_str("add(a + b * 2, fib(n - 1));");
        let item = &ast.items[0];
        match item {
            Item::Expr(Expr::Call(_func, args)) => {
                assert_eq!(args.len(), 2);
                // First arg: a + b * 2
                assert!(matches!(args[0], Expr::Add(_, _)));
                // Second arg: fib(n - 1)
                assert!(matches!(args[1], Expr::Call(_, _)));
            }
            _ => panic!("expected Call, got {:?}", item),
        }
    }

    // ========== Error Cases ==========

    #[test]
    fn asi_bare_expr_stmt_at_eof() {
        // 0.0.2 ASI: `42` at EOF gets an inserted `;` — valid since ASI.
        // (Was a missing-semicolon error in 0.0.1.)
        let ast = parse_str("42");
        assert!(matches!(ast.items[0], Item::Expr(Expr::Int(_, _))));
    }

    #[test]
    fn error_missing_rbrace_in_block() {
        // 0.0.2 ASI: the inserted `;` lets the binding parse; the unclosed
        // brace then surfaces as UnexpectedEof.
        let err = parse_err("func main() { x = 42 ");
        assert!(matches!(err.kind, ParseErrorKind::UnexpectedEof));
    }

    #[test]
    fn error_expected_expression() {
        // A stray `;` at statement position is rejected by the statement-
        // entry guard (ASI.md §4 #4).
        let err = parse_err(";");
        assert!(matches!(
            err.kind,
            ParseErrorKind::ExpectedSemiOrNewStmt { .. }
        ));
    }

    #[test]
    fn error_unexpected_token() {
        let err = parse_err("42 + ;");
        assert!(matches!(err.kind, ParseErrorKind::Expected { .. }));
    }

    // ========== Full Program Tests ==========

    #[test]
    fn parse_fib_program() {
        // The fib function uses if/elif/else as statements inside blocks
        // Note: in fleen 0.0.1, if/elif/else inside a block needs semicolons
        // unless they're the tail expression. For multi-line functions
        // with if/elif/else, the branches return values.
        let src = r#"
func fib(n: int): int {
    if n < 2 { n } elif n == 2 { 1 } else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0;
    const limit = 10;

    while x < limit {
        print(fib(x));
        x = x + 1;
    }

    0
}
"#;
        let ast = parse_str(src);
        assert_eq!(ast.items.len(), 2);

        // Check fib function
        match &ast.items[0] {
            Item::Decl(Decl::Func(f)) => {
                assert_eq!(f.name, "fib");
                assert_eq!(f.params.len(), 1);
                match &f.params[0].ty {
                    Type::Base(BaseType::Int) => {}
                    _ => panic!("expected int param type"),
                }
                assert!(matches!(f.ret_type, Some(Type::Base(BaseType::Int))));

                // Check body has if expression as tail
                if let FuncBody::Block(block) = &f.body {
                    assert!(block.tail_expr.is_some()); // if/elif/else is tail expression
                }
            }
            _ => panic!("expected func fib, got {:?}", ast.items[0]),
        }

        // Check main function
        match &ast.items[1] {
            Item::Decl(Decl::Func(f)) => {
                assert_eq!(f.name, "main");
                if let FuncBody::Block(block) = &f.body {
                    assert!(block.tail_expr.is_some());
                    assert!(matches!(
                        **block.tail_expr.as_ref().unwrap(),
                        Expr::Int(0, _)
                    ));
                }
            }
            _ => panic!("expected func main, got {:?}", ast.items[1]),
        }
    }

    #[test]
    fn parse_all_keywords_file() {
        // Keywords should be parseable as identifiers in expressions
        let src = r#"
func main() {
    const x = 1;
}
"#;
        let ast = parse_str(src);
        assert!(!ast.items.is_empty());
    }

    #[test]
    fn parse_operators_file() {
        let src = r#"
func main() {
    a = 1 + 2 - 3 * 4 / 5 % 6;
    b = a == 10;
    c = a != 20;
    d = a < 30;
    e = a > 40;
    f = a <= 50;
    g = a >= 60;
    h = true and false;
    i = true or false;
    j = not true;
    k = -5;
    l = !false;
}
"#;
        let ast = parse_str(src);
        match &ast.items[0] {
            Item::Decl(Decl::Func(f)) => {
                if let FuncBody::Block(block) = &f.body {
                    assert_eq!(block.stmts.len(), 12);
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn parse_control_flow_file() {
        let src = r#"
func main() {
    if true {
        x = 1;
    } elif false {
        x = 2;
    } else {
        x = 3;
    };

    while x < 10 {
        x = x + 1;
    };

    choose x {
        when 1 { "one" }
        when 2 { "two" }
        otherwise { "other" }
    };
}
"#;
        let ast = parse_str(src);
        match &ast.items[0] {
            Item::Decl(Decl::Func(f)) => {
                if let FuncBody::Block(block) = &f.body {
                    // 3 statements: if-expr, while-stmt, choose-expr
                    assert_eq!(block.stmts.len(), 3);
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn parse_function_types_file() {
        let src = r#"
func add(a: int, b: int): int = a + b

func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)
"#;
        let ast = parse_str(src);
        assert_eq!(ast.items.len(), 2);

        // Check apply has function type parameter
        match &ast.items[1] {
            Item::Decl(Decl::Func(f)) => {
                assert_eq!(f.name, "apply");
                match &f.params[0].ty {
                    Type::Func(_, _) => {}
                    _ => panic!("expected func type for parameter 'f'"),
                }
            }
            _ => panic!("expected func apply"),
        }
    }

    // ========== Type Tests ==========

    #[test]
    fn parse_type_annotations() {
        let ast = parse_str("x: int = 0;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                assert!(matches!(v.ty, Some(Type::Base(BaseType::Int))));
            }
            _ => panic!("expected Var decl"),
        }
    }

    #[test]
    fn parse_result_type() {
        // P4: Result<T, E> with angle brackets (per EBNF)
        let ast = parse_str("const r: Result<int, string> = 42;");
        match &ast.items[0] {
            Item::Decl(Decl::Const(c)) => {
                assert!(matches!(c.ty, Some(Type::Result(_, _))));
            }
            _ => panic!("expected Const decl"),
        }
    }

    #[test]
    fn parse_result_type_square_brackets_invalid() {
        // P4: Result[int, string] (square brackets) should now be invalid
        let err = parse_err("const r: Result[int, string] = 42;");
        assert!(matches!(err.kind, ParseErrorKind::Expected { .. }));
    }

    #[test]
    fn parse_array_type() {
        let ast = parse_str("const arr: [int] = 42;");
        match &ast.items[0] {
            Item::Decl(Decl::Const(c)) => {
                assert!(matches!(c.ty, Some(Type::Array(_))));
            }
            _ => panic!("expected Const decl"),
        }
    }

    #[test]
    fn parse_box_type() {
        let ast = parse_str("const b: box<int> = 42;");
        match &ast.items[0] {
            Item::Decl(Decl::Const(c)) => {
                assert!(matches!(c.ty, Some(Type::Box(_))));
            }
            _ => panic!("expected Const decl"),
        }
    }

    #[test]
    fn parse_ref_type() {
        let ast = parse_str("func foo(r: ref int) { }");
        match &ast.items[0] {
            Item::Decl(Decl::Func(f)) => match &f.params[0].ty {
                Type::Ref(_) => {}
                _ => panic!("expected ref type"),
            },
            _ => panic!("expected Func decl"),
        }
    }

    // ========== Integration with lexer tests ==========

    #[test]
    fn parse_single_tokens() {
        // Each token type as expression
        let ast = parse_str("(true); (false); (42); (\"hello\");");
        assert_eq!(ast.items.len(), 4);
    }

    #[test]
    fn parse_complex_expressions_in_blocks() {
        let src = r#"
func main() {
    const a = 1 + 2 * 3;
    const b = (1 + 2) * 3;
    x = foo(a, bar(b));
}
"#;
        let ast = parse_str(src);
        match &ast.items[0] {
            Item::Decl(Decl::Func(f)) => {
                if let FuncBody::Block(block) = &f.body {
                    assert_eq!(block.stmts.len(), 3);
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn parse_nested_blocks() {
        let src = r#"
func main() {
    if x > 0 {
        if y > 0 {
            z = 1;
        } else {
            z = 2;
        };
    } else {
        z = 3;
    };
}
"#;
        let ast = parse_str(src);
        match &ast.items[0] {
            Item::Decl(Decl::Func(f)) => {
                if let FuncBody::Block(block) = &f.body {
                    assert_eq!(block.stmts.len(), 1); // if statement
                }
            }
            _ => panic!("expected function"),
        }
    }

    // ========== P5: choose pattern extensions ==========

    #[test]
    fn parse_choose_negative_pattern() {
        let ast = parse_str("choose x { when -1 { \"neg\" } otherwise { \"o\" } };");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Choose(ch)) = item {
            assert_eq!(ch.arms.len(), 2);
            assert!(matches!(
                ch.arms[0].pattern,
                Pattern::Literal(Expr::Int(-1, _))
            ));
        } else {
            panic!("expected Choose expr, got {:?}", item);
        }
    }

    #[test]
    fn parse_choose_float_pattern() {
        let ast = parse_str("choose y { when 1.5 { \"half\" } otherwise { \"o\" } };");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Choose(ch)) = item {
            assert_eq!(ch.arms.len(), 2);
            assert!(matches!(
                ch.arms[0].pattern,
                Pattern::Literal(Expr::Float(1.5, _))
            ));
        } else {
            panic!("expected Choose expr, got {:?}", item);
        }
    }

    #[test]
    fn parse_choose_string_pattern() {
        let ast = parse_str("choose s { when \"hello\" { 1 } otherwise { 0 } };");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Choose(ch)) = item {
            assert_eq!(ch.arms.len(), 2);
            assert!(
                matches!(ch.arms[0].pattern, Pattern::Literal(Expr::Str(ref s, _)) if s == "hello")
            );
        } else {
            panic!("expected Choose expr, got {:?}", item);
        }
    }

    #[test]
    fn parse_choose_bool_pattern() {
        let ast = parse_str("choose b { when true { 1 } when false { 0 } };");
        let item = &ast.items[0];
        if let Item::Expr(Expr::Choose(ch)) = item {
            assert_eq!(ch.arms.len(), 2);
            assert!(matches!(
                ch.arms[0].pattern,
                Pattern::Literal(Expr::Bool(true, _))
            ));
            assert!(matches!(
                ch.arms[1].pattern,
                Pattern::Literal(Expr::Bool(false, _))
            ));
        } else {
            panic!("expected Choose expr, got {:?}", item);
        }
    }

    // ========== 0.0.2 U02: prefix keywords, `?`, Result patterns, deref assign ==========

    #[test]
    fn parse_move_expression() {
        let ast = parse_str("y = move x;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Move(inner, _) = &*v.init else {
                    panic!("expected Move expr")
                };
                assert!(matches!(&**inner, Expr::Ident(n, _) if n == "x"));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_clone_deref_chain() {
        // `clone deref b` = clone(deref(b)): prefix recursion, right-assoc
        let ast = parse_str("y = clone deref b;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Clone(inner, _) = &*v.init else {
                    panic!("expected Clone expr")
                };
                assert!(matches!(&**inner, Expr::Deref(_, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_box_nested() {
        // `box box 1` = box(box(1))
        let ast = parse_str("y = box box 1;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Box(inner, _) = &*v.init else {
                    panic!("expected Box expr")
                };
                assert!(matches!(&**inner, Expr::Box(_, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_question_suffix_chain() {
        // `x??` = (x?)? — postfix loops left-to-right
        let ast = parse_str("q = x??;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Question(inner, _) = &*v.init else {
                    panic!("expected Question expr")
                };
                assert!(matches!(&**inner, Expr::Question(_, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_question_binds_tighter_than_prefix() {
        // `deref b?` = deref(b?): `?` is postfix, binds tighter than prefix
        let ast = parse_str("y = deref b?;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Deref(inner, _) = &*v.init else {
                    panic!("expected Deref expr")
                };
                assert!(matches!(&**inner, Expr::Question(_, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_prefix_expression_span_covers_keyword() {
        // Span of a prefix expression starts at the keyword, not the operand
        let ast = parse_str("y = move x;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                if let Expr::Move(_, span) = &*v.init {
                    let src = "y = move x;";
                    assert_eq!(&src[span.start as usize..span.end as usize], "move x");
                } else {
                    panic!("expected Move expr");
                }
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_deref_assign_target() {
        // `deref b = v;` parses as an assignment with a Deref target
        let ast = parse_str("deref b = v;");
        match &ast.items[0] {
            Item::Expr(Expr::Assign(lhs, rhs)) => {
                let Expr::Deref(inner, _) = &**lhs else {
                    panic!("expected Deref target")
                };
                assert!(matches!(&**inner, Expr::Ident(n, _) if n == "b"));
                assert!(matches!(&**rhs, Expr::Ident(n, _) if n == "v"));
            }
            other => panic!("expected Assign expr, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_patterns() {
        let ast = parse_str("choose r { when Ok(v) { 1 } when Err(e) { 2 } } ;");
        match &ast.items[0] {
            Item::Expr(Expr::Choose(ch)) => {
                assert_eq!(ch.arms.len(), 2);
                assert!(matches!(
                    ch.arms[0].pattern,
                    Pattern::ResultCtor { ctor: ResultCtor::Ok, ref binding, .. } if binding == "v"
                ));
                assert!(matches!(
                    ch.arms[1].pattern,
                    Pattern::ResultCtor { ctor: ResultCtor::Err, ref binding, .. } if binding == "e"
                ));
            }
            other => panic!("expected Choose expr, got {:?}", other),
        }
    }

    #[test]
    fn parse_ok_err_without_paren_is_ident_pattern() {
        // `Ok` without `(` stays an ordinary identifier pattern
        let ast = parse_str("choose r { when Ok { 1 } } ;");
        match &ast.items[0] {
            Item::Expr(Expr::Choose(ch)) => assert!(matches!(
                ch.arms[0].pattern,
                Pattern::Ident(ref n, _) if n == "Ok"
            )),
            other => panic!("expected Choose expr, got {:?}", other),
        }
    }

    #[test]
    fn parse_move_clone_as_call_args() {
        let ast = parse_str("a = f(move x, clone y);");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                if let Expr::Call(_, args) = &*v.init {
                    assert!(matches!(args[0], Expr::Move(_, _)));
                    assert!(matches!(args[1], Expr::Clone(_, _)));
                } else {
                    panic!("expected Call expr");
                }
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn reject_move_as_binding_name() {
        // 0.0.2: `move` is reserved; using it as a binding name reports the
        // keyword, not a generic "expected expression"
        let err = parse_err("move = 1;");
        if let ParseErrorKind::Expected { expected, .. } = err.kind {
            assert!(expected.contains("keyword `move`"), "got: {expected}");
        } else {
            panic!("expected Expected error, got {:?}", err.kind);
        }
    }

    #[test]
    fn reject_dangling_question() {
        let err = parse_err("q = ?;");
        assert!(matches!(err.kind, ParseErrorKind::Expected { .. }));
    }

    #[test]
    fn reject_move_without_operand() {
        let err = parse_err("y = move;");
        assert!(matches!(err.kind, ParseErrorKind::Expected { .. }));
    }

    #[test]
    fn reject_deref_without_operand() {
        let err = parse_err("y = deref;");
        assert!(matches!(err.kind, ParseErrorKind::Expected { .. }));
    }

    #[test]
    fn reject_deref_assign_with_type_annotation() {
        // `deref b` cannot carry a type annotation (DESIGN.md §3.4); the
        // error must name the problem, not a generic "expected `;`"
        let err = parse_err("deref b: int = 2;");
        if let ParseErrorKind::Expected { expected, .. } = err.kind {
            assert!(
                expected.contains("cannot carry a type annotation"),
                "got: {expected}"
            );
        } else {
            panic!("expected Expected error, got {:?}", err.kind);
        }
    }

    #[test]
    fn parse_prefix_binds_tighter_than_additive() {
        // `deref b + 1` = (deref b) + 1: prefix binds tighter than `+`,
        // so `n = deref b + 1;` assigns `Add(Deref(b), 1)` to `n`
        let ast = parse_str("n = deref b + 1;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Add(lhs, _) = &*v.init else {
                    panic!("expected Add expr")
                };
                assert!(
                    matches!(&**lhs, Expr::Deref(inner, _) if matches!(&**inner, Expr::Ident(n, _) if n == "b"))
                );
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_question_binds_tighter_than_clone() {
        // `clone b?` = clone(b?): `?` is postfix, binds tighter than prefix
        let ast = parse_str("y = clone b?;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Clone(inner, _) = &*v.init else {
                    panic!("expected Clone expr")
                };
                assert!(matches!(&**inner, Expr::Question(_, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_prefix_takes_unary_operand() {
        // Prefixes recurse into the unary level (SYNTAX.ebnf v0.0.2:
        // unary = unary_prefix unary), so `deref -x` = deref(neg(x))
        let ast = parse_str("y = deref -x;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Deref(inner, _) = &*v.init else {
                    panic!("expected Deref expr")
                };
                assert!(
                    matches!(&**inner, Expr::Neg(operand) if matches!(&**operand, Expr::Ident(n, _) if n == "x"))
                );
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_prefix_stacks_on_itself() {
        // `move move x` = move(move(x)): same recursion, non-`box` prefix
        let ast = parse_str("y = move move x;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Move(inner, _) = &*v.init else {
                    panic!("expected Move expr")
                };
                assert!(matches!(&**inner, Expr::Move(_, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn reject_box_as_binding_name() {
        // 0.0.2: `box` is reserved in both type and expression position;
        // `box = 1` hits the BoxType branch of reject_keyword_binding_name
        let err = parse_err("box = 1;");
        if let ParseErrorKind::Expected { expected, .. } = err.kind {
            assert!(expected.contains("keyword `box`"), "got: {expected}");
        } else {
            panic!("expected Expected error, got {:?}", err.kind);
        }
    }

    #[test]
    fn reject_result_pattern_without_binding() {
        // `Ok()` — parse_result_pattern requires exactly one payload binding
        let err = parse_err("choose r { when Ok() { 0 } }");
        assert!(matches!(err.kind, ParseErrorKind::Expected { .. }));
    }

    // ========== ASI（0.0.2，见 docs/0.0.2/ASI.md §5.1 陷阱表） ==========

    #[test]
    fn asi_two_stmts_across_newline() {
        // `x = 1` ⏎ `y = 2` → 两条语句
        let ast = parse_str("x = 1\ny = 2\n");
        assert_eq!(ast.items.len(), 2);
    }

    #[test]
    fn asi_binding_before_rbrace() {
        // `{ x = 1 }` → 绑定贴 `}` 免分号，无尾表达式
        let ast = parse_str("{ x = 1 }");
        let Item::Expr(Expr::Block(block)) = &ast.items[0] else {
            panic!("expected block expr");
        };
        assert_eq!(block.stmts.len(), 1);
        assert!(block.tail_expr.is_none());
    }

    #[test]
    fn asi_const_before_rbrace() {
        let ast = parse_str("{ const y = 2 }");
        let Item::Expr(Expr::Block(block)) = &ast.items[0] else {
            panic!("expected block expr");
        };
        assert!(matches!(&block.stmts[0], Stmt::Decl(Decl::Const(_))));
    }

    #[test]
    fn asi_tail_expr_preserved() {
        // `{ 42 }` → 尾表达式（pass 不在 `}` 前插分号）
        let ast = parse_str("{ 42 }");
        let Item::Expr(Expr::Block(block)) = &ast.items[0] else {
            panic!("expected block expr");
        };
        assert!(block.stmts.is_empty());
        assert!(matches!(
            block.tail_expr.as_deref().expect("tail"),
            Expr::Int(_, _)
        ));
    }

    #[test]
    fn asi_explicit_semi_still_discards() {
        // `{ f(); }` → 语句（has_semi），块值 unit
        let ast = parse_str("{ f(); }");
        let Item::Expr(Expr::Block(block)) = &ast.items[0] else {
            panic!("expected block expr");
        };
        assert!(matches!(&block.stmts[0], Stmt::Expr(_, true)));
        assert!(block.tail_expr.is_none());
    }

    #[test]
    fn asi_cross_line_if_else_is_one_expr() {
        // `if c { 1 }` ⏎ `else { 2 }` → 一个 if-else 表达式
        let ast = parse_str("r = if c { 1 }\nelse { 2 };\n");
        let Item::Decl(Decl::Var(binding)) = &ast.items[0] else {
            panic!("expected binding");
        };
        assert!(matches!(*binding.init, Expr::If(_)));
    }

    #[test]
    fn asi_allman_braces() {
        // Allman 风格：`{` 单独成行不断句
        let ast = parse_str("r = if c\n{ 1 }\nelse\n{ 2 };\n");
        let Item::Decl(Decl::Var(binding)) = &ast.items[0] else {
            panic!("expected binding");
        };
        assert!(matches!(*binding.init, Expr::If(_)));
    }

    #[test]
    fn asi_if_stmt_then_binding() {
        // `if c { 1 }` ⏎ `y = 2` → if 语句完结，pass 插分号
        let ast = parse_str("if c { 1 }\ny = 2\n");
        assert_eq!(ast.items.len(), 2);
    }

    #[test]
    fn asi_else_after_terminated_if_rejected() {
        // `if c { 1 };` ⏎ `else { 2 }` → else 不续接已完结的 if 语句
        let err = parse_err("if c { 1 };\nelse { 2 }\n");
        assert!(matches!(
            err.kind,
            ParseErrorKind::ExpectedSemiOrNewStmt { .. }
        ));
    }

    #[test]
    fn asi_same_line_juxtaposition_rejected() {
        // `x = 1 y = 2`（同行拼接）→ 语法错误
        let err = parse_err("x = 1 y = 2\n");
        assert!(matches!(
            err.kind,
            ParseErrorKind::ExpectedSemiOrNewStmt { .. }
        ));
    }

    #[test]
    fn asi_block_expr_stmt_needs_leading_semi() {
        // `x = 1` ⏎ `{ print(1) }` → 块表达式语句需前导分号
        // （`{` ∉ 隐式结束集，pass 不断句，绑定收尾报错）
        let err = parse_err("x = 1\n{ print(1) }\n");
        assert!(matches!(
            err.kind,
            ParseErrorKind::ExpectedSemiOrNewStmt { .. }
        ));
    }

    #[test]
    fn asi_double_semicolon_rejected() {
        let err = parse_err("x = 1;;\n");
        assert!(matches!(
            err.kind,
            ParseErrorKind::ExpectedSemiOrNewStmt { .. }
        ));
    }

    #[test]
    fn asi_binary_continues_across_lines() {
        // `x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`
        let ast = parse_str("x = 1\n- 2\n");
        let Item::Decl(Decl::Var(binding)) = &ast.items[0] else {
            panic!("expected binding");
        };
        assert!(matches!(*binding.init, Expr::Sub(_, _)));
    }

    #[test]
    fn asi_if_head_binary_climb() {
        // `if c { 1 }` ⏎ `- 2` ⇒ `(if-expr) - 2`（block-like 头续接爬升）
        let ast = parse_str("if c { 1 }\n- 2\n");
        assert!(matches!(ast.items[0], Item::Expr(Expr::Sub(_, _))));
    }

    #[test]
    fn asi_call_chain_across_lines() {
        // `f()` ⏎ `(g())` ⇒ `f()(g())`
        let ast = parse_str("f()\n(g())\n");
        let Item::Expr(Expr::Call(func, _)) = &ast.items[0] else {
            panic!("expected call");
        };
        assert!(matches!(**func, Expr::Call(_, _)));
    }

    #[test]
    fn asi_choose_arms_across_lines() {
        // `}` ⏎ `when` —— 臂延续不断句
        let ast = parse_str("choose x {\nwhen 0 { 1 }\nwhen 1 { 2 }\n}\n");
        let Item::Expr(Expr::Choose(choose)) = &ast.items[0] else {
            panic!("expected choose");
        };
        assert_eq!(choose.arms.len(), 2);
    }

    #[test]
    fn asi_when_guard_across_lines() {
        // `when 0` ⏎ `if guard { … }` —— guard 可跨行（pass 插入分号被跳过）
        let ast = parse_str("choose x {\nwhen 0\nif x > 1 { 1 }\n}\n");
        let Item::Expr(Expr::Choose(choose)) = &ast.items[0] else {
            panic!("expected choose");
        };
        assert!(choose.arms[0].guard.is_some());
    }

    #[test]
    fn asi_func_decl_then_binding() {
        // `func f() { 1 }` ⏎ `x = 2` → 两个 item（pass 在 `}` 后插分号）
        let ast = parse_str("func f() { 1 }\nx = 2\n");
        assert_eq!(ast.items.len(), 2);
    }

    #[test]
    fn asi_binding_in_when_arm_before_rbrace() {
        // when 臂内绑定贴 `}` 免分号
        let ast = parse_str("choose x {\nwhen 0 { y = 1 }\n}\n");
        let Item::Expr(Expr::Choose(choose)) = &ast.items[0] else {
            panic!("expected choose");
        };
        assert!(choose.arms[0].body.tail_expr.is_none());
    }

    #[test]
    fn asi_assign_shape_before_rbrace_is_statement() {
        // `deref b = v` 贴 `}` 按语句处理，不作尾表达式（ASI.md §4 #2：
        // Assign 的 lower 栈残留与 Unit 类型错位的防御）
        let ast = parse_str("func f() {\nderef b = 3\n}");
        let Item::Decl(Decl::Func(func)) = &ast.items[0] else {
            panic!("expected func decl");
        };
        let FuncBody::Block(block) = &func.body else {
            panic!("expected block body");
        };
        assert_eq!(block.stmts.len(), 1);
        assert!(matches!(
            &block.stmts[0],
            Stmt::Expr(e, false) if matches!(**e, Expr::Assign(_, _))
        ));
        assert!(block.tail_expr.is_none());
    }

    #[test]
    fn asi_block_with_explicit_semi_binding_is_statement() {
        // `{ x = 1; }` —— 显式分号：绑定按语句处理，块值 unit（ASI.md §5.1）
        let ast = parse_str("{ x = 1; }");
        let Item::Expr(Expr::Block(block)) = &ast.items[0] else {
            panic!("expected block expression");
        };
        assert_eq!(block.stmts.len(), 1);
        assert!(block.tail_expr.is_none());
    }

    // ========== 0.0.2 U13: `as` casts ==========

    #[test]
    fn parse_cast_simple() {
        let ast = parse_str("y = 1 as string;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Cast(inner, ty, _) = &*v.init else {
                    panic!("expected Cast expr")
                };
                assert!(matches!(&**inner, Expr::Int(1, _)));
                assert_eq!(*ty, Type::Base(BaseType::String));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_cast_tighter_than_binary() {
        // `1 + 2 as string` = `1 + (2 as string)`
        let ast = parse_str("y = 1 + 2 as string;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Add(lhs, rhs) = &*v.init else {
                    panic!("expected Add expr")
                };
                assert!(matches!(&**lhs, Expr::Int(1, _)));
                assert!(matches!(&**rhs, Expr::Cast(_, _, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_cast_deref_operand_unary_first() {
        // `deref b as string` = `(deref b) as string`
        let ast = parse_str("y = deref b as string;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Cast(inner, _, _) = &*v.init else {
                    panic!("expected Cast expr")
                };
                assert!(matches!(&**inner, Expr::Deref(_, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_cast_box_operand_includes_cast() {
        // `box 1 as string` = `box (1 as string)` — box parses its operand
        // at the cast level (unlike deref/move/clone which take unary).
        let ast = parse_str("y = box 1 as string;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Box(inner, _) = &*v.init else {
                    panic!("expected Box expr")
                };
                assert!(matches!(&**inner, Expr::Cast(_, _, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_cast_call_operand() {
        // `f(x) as string`
        let ast = parse_str("y = f(x) as string;");
        match &ast.items[0] {
            Item::Decl(Decl::Var(v)) => {
                let Expr::Cast(inner, _, _) = &*v.init else {
                    panic!("expected Cast expr")
                };
                assert!(matches!(&**inner, Expr::Call(_, _)));
            }
            other => panic!("expected Var binding, got {:?}", other),
        }
    }

    #[test]
    fn parse_cast_missing_type_is_error() {
        let err = parse_err("y = 42 as;");
        assert!(matches!(err.kind, ParseErrorKind::Expected { .. }));
    }

    #[test]
    fn parse_as_as_binding_name_is_error() {
        // `as` is a reserved keyword (0.0.2 U13). The statement-entry guard
        // rejects it before the keyword-binding check (unlike move/clone/
        // deref, `as` cannot start a statement).
        let err = parse_err("as = 1;");
        assert!(matches!(
            err.kind,
            ParseErrorKind::ExpectedSemiOrNewStmt { .. }
        ));
    }
}
