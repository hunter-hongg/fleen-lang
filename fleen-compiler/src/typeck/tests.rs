//! Tests for the type checker.

use crate::lexer::tokenize;
use crate::parser::parse;
use crate::resolver::resolve;
use crate::typeck::error::{TypeckError, TypeckErrorKind};
use crate::typeck::typeck;
use crate::typeck::typed_hir::*;

/// Helper: compile source to TypedHir.
fn compile_to_typed(source: &str) -> Result<TypedHir, Vec<TypeckError>> {
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
fn compile_err(source: &str) -> Vec<TypeckError> {
    compile_to_typed(source).expect_err("typeck should fail")
}

/// Extract the single top-level expression's inferred type.
fn only_expr_ty(hir: &TypedHir) -> Type {
    assert_eq!(hir.items.len(), 1, "expected one item: {hir:?}");
    match &hir.items[0] {
        TypedHirItem::Expr(e) => e.ty(),
        other => panic!("expected expression item, got {other:?}"),
    }
}

fn err_kinds(errs: &[TypeckError]) -> Vec<&TypeckErrorKind> {
    errs.iter().map(|e| &e.kind).collect()
}

// ========== Literal Type Tests ==========

#[test]
fn infer_int_literal() {
    let hir = compile_ok("42;");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

#[test]
fn infer_float_literal() {
    let hir = compile_ok("3.14;");
    assert_eq!(only_expr_ty(&hir), Type::Float);
}

#[test]
fn infer_bool_literal() {
    let hir = compile_ok("true;");
    assert_eq!(only_expr_ty(&hir), Type::Bool);
}

#[test]
fn infer_string_literal() {
    let hir = compile_ok("\"hello\";");
    assert_eq!(only_expr_ty(&hir), Type::String);
}

// ========== Binary Operator Tests ==========

#[test]
fn check_arithmetic_int() {
    let hir = compile_ok("1 + 2;");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

#[test]
fn check_arithmetic_float() {
    let hir = compile_ok("1.0 + 2.0;");
    assert_eq!(only_expr_ty(&hir), Type::Float);
}

#[test]
fn check_arithmetic_type_mismatch() {
    let errs = compile_err("1 + 2.0;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_mod_int() {
    let hir = compile_ok("1 % 2;");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

#[test]
fn check_mod_float_rejected() {
    // BYTECODE.md defines IMod but no FMod; float % must fail at typeck,
    // not later in lower.
    let errs = compile_err("1.0 % 2.0;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { .. })),
        "expected InvalidOperand, got {errs:?}"
    );
}

#[test]
fn check_comparison_int() {
    let hir = compile_ok("1 < 2;");
    assert_eq!(only_expr_ty(&hir), Type::Bool);
}

#[test]
fn check_comparison_type_mismatch() {
    let errs = compile_err("1 < 2.0;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_logical_bool() {
    let hir = compile_ok("true and false;");
    assert_eq!(only_expr_ty(&hir), Type::Bool);
}

#[test]
fn check_logical_type_mismatch() {
    let errs = compile_err("true and 1;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

// ========== Unary Operator Tests ==========

#[test]
fn check_neg_int() {
    let hir = compile_ok("-42;");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

#[test]
fn check_neg_float() {
    let hir = compile_ok("-3.14;");
    assert_eq!(only_expr_ty(&hir), Type::Float);
}

#[test]
fn check_neg_type_mismatch() {
    let errs = compile_err("-true;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { .. })),
        "expected InvalidOperand, got {errs:?}"
    );
}

#[test]
fn check_not_bool() {
    let hir = compile_ok("!true;");
    assert_eq!(only_expr_ty(&hir), Type::Bool);
}

#[test]
fn check_not_type_mismatch() {
    let errs = compile_err("!1;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { .. })),
        "expected InvalidOperand, got {errs:?}"
    );
}

// ========== If Expression Tests ==========

#[test]
fn check_if_same_branch_types() {
    let hir = compile_ok("if true { 1 } else { 2 };");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

#[test]
fn check_if_different_branch_types() {
    let errs = compile_err("if true { 1 } else { \"hello\" };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_if_condition_not_bool() {
    let errs = compile_err("if 1 { 2 } else { 3 };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ConditionNotBool { .. })),
        "expected ConditionNotBool, got {errs:?}"
    );
}

#[test]
fn check_if_elif_else() {
    let hir = compile_ok("if true { 1 } elif false { 2 } else { 3 };");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

// ========== While Expression Tests ==========

#[test]
fn check_while_valid() {
    let hir = compile_ok("while true { 1 + 1; };");
    assert_eq!(only_expr_ty(&hir), Type::Unit);
}

#[test]
fn check_while_condition_not_bool() {
    let errs = compile_err("while 1 { 2 + 2; };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ConditionNotBool { .. })),
        "expected ConditionNotBool, got {errs:?}"
    );
}

// ========== Choose Expression Tests ==========

#[test]
fn check_choose_with_otherwise() {
    let hir = compile_ok("choose 1 { when 0 { \"zero\" } otherwise { \"other\" } };");
    assert_eq!(only_expr_ty(&hir), Type::String);
}

#[test]
fn check_choose_bool_exhaustive() {
    let hir = compile_ok("choose true { when true { 1 } when false { 0 } };");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

#[test]
fn check_choose_bool_not_exhaustive() {
    let errs = compile_err("choose true { when true { 1 } };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ChooseNotExhaustive { .. })),
        "expected ChooseNotExhaustive, got {errs:?}"
    );
}

#[test]
fn check_choose_int_not_exhaustive() {
    let errs = compile_err("choose 1 { when 0 { \"zero\" } };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ChooseNotExhaustive { .. })),
        "expected ChooseNotExhaustive, got {errs:?}"
    );
}

#[test]
fn check_choose_guarded_bool_arm_does_not_cover() {
    // `when true if g` may never fire, so true is NOT covered.
    let errs = compile_err("choose true { when true if 1 < 2 { 1 } when false { 0 } };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. } if missing_patterns.iter().any(|m| m == "true"))),
        "expected ChooseNotExhaustive missing `true`, got {errs:?}"
    );
}

#[test]
fn check_choose_result_requires_otherwise() {
    // Result is not enumerable and 0.0.1 has no Ok/Err pattern syntax:
    // an `otherwise` arm is required.
    let errs = compile_err("x = 1;\nchoose x { when 0 { 1 } };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ChooseNotExhaustive { .. })),
        "expected ChooseNotExhaustive, got {errs:?}"
    );
}

#[test]
fn check_choose_pattern_binding_is_typed() {
    // `when n { ... }` binds n with the scrutinee's type (int here).
    let hir = compile_ok("choose 1 { when n { n + 1 } otherwise { 0 } };");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

#[test]
fn check_choose_pattern_binding_wrong_usage() {
    let errs = compile_err("choose 1 { when n { n and true } otherwise { 0 } };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

// ========== Function Tests ==========

#[test]
fn check_func_decl() {
    let hir = compile_ok("func add(a: int, b: int): int = a + b");
    assert_eq!(hir.items.len(), 1);
    let TypedHirItem::Decl(TypedDeclHir::Func(f)) = &hir.items[0] else {
        panic!("expected func decl");
    };
    assert_eq!(f.ret_type, Some(Type::Int));
}

#[test]
fn check_func_call() {
    let hir = compile_ok("func add(a: int, b: int): int = a + b\nadd(1, 2);");
    assert_eq!(
        only_expr_ty(&TypedHir {
            items: vec![hir.items[1].clone()],
            span: hir.span
        }),
        Type::Int
    );
}

#[test]
fn check_func_call_arity_mismatch() {
    let errs = compile_err("func add(a: int, b: int): int = a + b\nadd(1);");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArityMismatch { .. })),
        "expected ArityMismatch, got {errs:?}"
    );
}

#[test]
fn check_func_call_type_mismatch() {
    let errs = compile_err("func add(a: int, b: int): int = a + b\nadd(1, \"hello\");");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArgTypeMismatch { .. })),
        "expected ArgTypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_func_return_type_mismatch() {
    let errs = compile_err("func add(a: int, b: int): int = \"hello\"");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_func_no_ret_type_means_unit() {
    let errs = compile_err("func f() { 1 }");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_func_as_value_call() {
    let hir = compile_ok(
        "func add(a: int, b: int): int = a + b\nfunc apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)\napply(add, 1, 2);",
    );
    assert_eq!(hir.items.len(), 3);
}

#[test]
fn check_call_non_function() {
    let errs = compile_err("x = 1;\nx(2);");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::NotCallable { .. })),
        "expected NotCallable, got {errs:?}"
    );
}

#[test]
fn check_builtin_print() {
    let hir = compile_ok("print(\"hi\");");
    assert_eq!(only_expr_ty(&hir), Type::Unit);
}

// HACK: 0.0.1 放宽了 `print` 的类型检查（接受任意参数），见 infer.rs。
// 0.0.2 恢复后此测试应回退为期望 ArgTypeMismatch。
#[test]
fn check_builtin_print_wrong_arg() {
    let hir = compile_ok("print(1);");
    assert_eq!(only_expr_ty(&hir), Type::Unit);
}

// ========== Variable Binding Tests ==========

#[test]
fn check_var_binding() {
    let hir = compile_ok("x = 42;");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn check_var_binding_with_type() {
    let hir = compile_ok("x: int = 42;");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn check_var_binding_type_mismatch() {
    let errs = compile_err("x: int = \"hello\";");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_const_binding() {
    let hir = compile_ok("const x = 42;");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn check_const_binding_with_type() {
    let hir = compile_ok("const x: int = 42;");
    assert_eq!(hir.items.len(), 1);
}

// ========== Assignment Tests ==========

#[test]
fn check_assignment() {
    let hir = compile_ok("x = 42;\nx = 43;");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn check_assignment_type_mismatch() {
    let errs = compile_err("x = 42;\nx = \"hello\";");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::AssignTypeMismatch { .. })),
        "expected AssignTypeMismatch, got {errs:?}"
    );
}

// ========== Block Tests ==========

#[test]
fn check_block() {
    let hir = compile_ok("{ 1; 2; 3 }");
    assert_eq!(only_expr_ty(&hir), Type::Int);
}

#[test]
fn check_block_unit_when_no_tail() {
    let hir = compile_ok("{ 1; }");
    assert_eq!(only_expr_ty(&hir), Type::Unit);
}

// ========== Unsupported Feature Tests ==========

#[test]
fn check_index_on_non_array() {
    let errs = compile_err("x = 1;\nx[0];");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_field_access_unsupported() {
    let errs = compile_err("x = 1;\nx.foo;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::UnsupportedFeature { .. })),
        "expected UnsupportedFeature, got {errs:?}"
    );
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
    assert_eq!(hir.items.len(), 1);
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
    assert_eq!(hir.items.len(), 1);
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
