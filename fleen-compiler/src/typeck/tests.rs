//! Tests for the type checker.

use crate::lexer::tokenize;
use crate::parser::parse;
use crate::resolver::resolve;
use crate::typeck::typeck;
use crate::typeck::typed_hir::*;

/// Helper: compile source to TypedHir.
fn compile_to_typed(source: &str) -> Result<TypedHir, Vec<crate::typeck::error::TypeckError>> {
    let tokens = tokenize(source).unwrap();
    let ast = parse(tokens).unwrap();
    let hir = resolve(ast).unwrap();
    typeck(hir)
}

/// Helper: compile source and expect success.
fn compile_ok(source: &str) -> TypedHir {
    compile_to_typed(source).expect("typeck should succeed")
}

/// Helper: compile source and expect failure.
fn compile_err(source: &str) -> Vec<crate::typeck::error::TypeckError> {
    compile_to_typed(source).expect_err("typeck should fail")
}

// ========== Literal Type Tests ==========

#[test]
fn infer_int_literal() {
    let hir = compile_ok("42;");
    // Just check it compiles
    assert!(!hir.items.is_empty());
}

#[test]
fn infer_float_literal() {
    let hir = compile_ok("3.14;");
    assert!(!hir.items.is_empty());
}

#[test]
fn infer_bool_literal() {
    let hir = compile_ok("true;");
    assert!(!hir.items.is_empty());
}

#[test]
fn infer_string_literal() {
    let hir = compile_ok("\"hello\";");
    assert!(!hir.items.is_empty());
}

// ========== Binary Operator Tests ==========

#[test]
fn check_arithmetic_int() {
    let hir = compile_ok("1 + 2;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_arithmetic_float() {
    let hir = compile_ok("1.0 + 2.0;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_arithmetic_type_mismatch() {
    let errs = compile_err("1 + 2.0;");
    assert!(!errs.is_empty());
}

#[test]
fn check_comparison_int() {
    let hir = compile_ok("1 < 2;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_comparison_type_mismatch() {
    let errs = compile_err("1 < 2.0;");
    assert!(!errs.is_empty());
}

#[test]
fn check_logical_bool() {
    let hir = compile_ok("true and false;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_logical_type_mismatch() {
    let errs = compile_err("true and 1;");
    assert!(!errs.is_empty());
}

// ========== Unary Operator Tests ==========

#[test]
fn check_neg_int() {
    let hir = compile_ok("-42;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_neg_float() {
    let hir = compile_ok("-3.14;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_neg_type_mismatch() {
    let errs = compile_err("-true;");
    assert!(!errs.is_empty());
}

#[test]
fn check_not_bool() {
    let hir = compile_ok("!true;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_not_type_mismatch() {
    let errs = compile_err("!1;");
    assert!(!errs.is_empty());
}

// ========== If Expression Tests ==========

#[test]
fn check_if_same_branch_types() {
    let hir = compile_ok("if true { 1 } else { 2 };");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_if_different_branch_types() {
    let errs = compile_err("if true { 1 } else { \"hello\" };");
    assert!(!errs.is_empty());
}

#[test]
fn check_if_condition_not_bool() {
    let errs = compile_err("if 1 { 2 } else { 3 };");
    assert!(!errs.is_empty());
}

#[test]
fn check_if_elif_else() {
    let hir = compile_ok("if true { 1 } elif false { 2 } else { 3 };");
    assert!(!hir.items.is_empty());
}

// ========== While Expression Tests ==========

#[test]
fn check_while_valid() {
    let hir = compile_ok("while true { 1 + 1; };");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_while_condition_not_bool() {
    let errs = compile_err("while 1 { 2 + 2; };");
    assert!(!errs.is_empty());
}

// ========== Choose Expression Tests ==========

#[test]
fn check_choose_with_otherwise() {
    let hir = compile_ok("choose 1 { when 0 { \"zero\" } otherwise { \"other\" } };");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_choose_bool_exhaustive() {
    let hir = compile_ok("choose true { when true { 1 } when false { 0 } };");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_choose_bool_not_exhaustive() {
    let errs = compile_err("choose true { when true { 1 } };");
    assert!(!errs.is_empty());
}

#[test]
fn check_choose_int_not_exhaustive() {
    let errs = compile_err("choose 1 { when 0 { \"zero\" } };");
    assert!(!errs.is_empty());
}

// ========== Function Tests ==========

#[test]
fn check_func_decl() {
    let hir = compile_ok("func add(a: int, b: int): int = a + b");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_func_call() {
    let hir = compile_ok("func add(a: int, b: int): int = a + b\nadd(1, 2);");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_func_call_arity_mismatch() {
    let errs = compile_err("func add(a: int, b: int): int = a + b\nadd(1);");
    assert!(!errs.is_empty());
}

#[test]
fn check_func_call_type_mismatch() {
    let errs = compile_err("func add(a: int, b: int): int = a + b\nadd(1, \"hello\");");
    assert!(!errs.is_empty());
}

#[test]
fn check_func_return_type_mismatch() {
    let errs = compile_err("func add(a: int, b: int): int = \"hello\"");
    assert!(!errs.is_empty());
}

// ========== Variable Binding Tests ==========

#[test]
fn check_var_binding() {
    let hir = compile_ok("x = 42;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_var_binding_with_type() {
    let hir = compile_ok("x: int = 42;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_var_binding_type_mismatch() {
    let errs = compile_err("x: int = \"hello\";");
    assert!(!errs.is_empty());
}

#[test]
fn check_const_binding() {
    let hir = compile_ok("const x = 42;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_const_binding_with_type() {
    let hir = compile_ok("const x: int = 42;");
    assert!(!hir.items.is_empty());
}

// ========== Assignment Tests ==========

#[test]
fn check_assignment() {
    let hir = compile_ok("x = 42;\nx = 43;");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_assignment_type_mismatch() {
    // Typeck correctly catches type mismatch in assignments:
    // x is Int, then x = "hello" assigns String to Int variable.
    let errs = compile_err("x = 42;\nx = \"hello\";");
    assert!(!errs.is_empty());
}

// ========== Block Tests ==========

#[test]
fn check_block() {
    let hir = compile_ok("{ 1; 2; 3 }");
    assert!(!hir.items.is_empty());
}

#[test]
fn check_block_with_tail() {
    let hir = compile_ok("{ 1; 2; 3 }");
    assert!(!hir.items.is_empty());
}

// ========== Complex Tests ==========

#[test]
fn check_fibonacci() {
    let source = r#"
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}
"#;
    let hir = compile_ok(source);
    assert!(!hir.items.is_empty());
}

#[test]
fn check_main_with_loop() {
    let source = r#"
func main(): int {
    x = 0;
    const limit = 10;
    while x < limit {
        x = x + 1;
    };
    0
}
"#;
    let hir = compile_ok(source);
    assert!(!hir.items.is_empty());
}

// ========== Error Recovery Tests ==========

#[test]
fn check_multiple_errors_collected() {
    let errs = compile_err("if true { 1 } else { \"hello\" };\nwhile 1 { 2 + 2; };");
    assert!(errs.len() >= 2);
}

// ========== Type Tests ==========

#[test]
fn type_name_int() {
    assert_eq!(Type::Int.name(), "int");
}

#[test]
fn type_name_float() {
    assert_eq!(Type::Float.name(), "float");
}

#[test]
fn type_name_bool() {
    assert_eq!(Type::Bool.name(), "bool");
}

#[test]
fn type_name_string() {
    assert_eq!(Type::String.name(), "string");
}

#[test]
fn type_name_unit() {
    assert_eq!(Type::Unit.name(), "unit");
}

#[test]
fn type_name_func() {
    let func = Type::Func(vec![Type::Int, Type::Int], Box::new(Type::Int));
    assert_eq!(func.name(), "(int, int) -> int");
}

#[test]
fn type_name_result() {
    let result = Type::Result(Box::new(Type::Int), Box::new(Type::String));
    assert_eq!(result.name(), "Result[int, string]");
}

#[test]
fn type_is_numeric() {
    assert!(Type::Int.is_numeric());
    assert!(Type::Float.is_numeric());
    assert!(!Type::Bool.is_numeric());
    assert!(!Type::String.is_numeric());
}

#[test]
fn type_is_func() {
    let func = Type::Func(vec![], Box::new(Type::Unit));
    assert!(func.is_func());
    assert!(!Type::Int.is_func());
}
