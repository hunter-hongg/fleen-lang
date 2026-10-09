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

// 0.0.2 (F6): `print` has a real signature `(string...) -> unit` again —
// strict string checking with variadic arity (PLAN §6).
#[test]
fn check_builtin_print_variadic_strings() {
    compile_ok("print(\"a\", \"b\");");
    compile_ok("print();");
    compile_ok("print(42 as string);");
}

#[test]
fn check_builtin_print_wrong_arg() {
    let errs = compile_err("print(1);");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArgTypeMismatch { .. })),
        "expected ArgTypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_builtin_print_function_value_rejected() {
    let errs = compile_err("func fib(n: int): int = n\nprint(fib);");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArgTypeMismatch { .. })),
        "expected ArgTypeMismatch, got {errs:?}"
    );
}

// ========== Result: Ok/Err Constructors (0.0.2 U04, PLAN §3.4.1/D6) ==========

#[test]
fn check_result_ctor_in_annotated_binding() {
    let hir = compile_ok("res: Result<int, string> = Ok(42);");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn check_result_ctor_payload_checked() {
    let errs = compile_err("res: Result<int, string> = Ok(\"x\");");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArgTypeMismatch { .. })),
        "expected ArgTypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_result_ctor_no_context_rejected() {
    let errs = compile_err("Ok(1);");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::CannotInferResultType { .. })),
        "expected CannotInferResultType, got {errs:?}"
    );
}

#[test]
fn check_result_ctor_no_context_in_func_rejected() {
    let errs = compile_err("func f(): int { Ok(1) }");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::CannotInferResultType { .. })),
        "expected CannotInferResultType, got {errs:?}"
    );
}

#[test]
fn check_result_ctor_arity() {
    let errs = compile_err("res: Result<int, string> = Ok();");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArityMismatch { .. })),
        "expected ArityMismatch, got {errs:?}"
    );
    let errs = compile_err("res: Result<int, string> = Ok(1, 2);");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArityMismatch { .. })),
        "expected ArityMismatch, got {errs:?}"
    );
}

#[test]
fn check_result_ctor_nested_expected_thread() {
    // The payload itself is a Result in expected position: the inner `Ok(1)`
    // must infer through the expected-type thread.
    compile_ok("r: Result<Result<int, string>, string> = Ok(Ok(1));");
}

#[test]
fn check_div_example_from_plan() {
    // PLAN §3.4.1 example: Ok/Err in function body tails infer from the
    // declared Result return type (expected thread, path 1).
    let source = r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
"#;
    compile_ok(source);
}

#[test]
fn check_user_shadows_ok() {
    // §4.3: a user function named `Ok` shadows the builtin constructor;
    // the call is then checked as an ordinary call.
    let source = "func Ok(x: int): int = x + 1\nOk(3);";
    let hir = compile_ok(source);
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn check_local_binding_shadows_ok() {
    // §4.3: a local binding named `Ok` also shadows the builtin
    // constructor; the call is then an ordinary call of that binding.
    let source = "Ok = 42;\nx = Ok;";
    let hir = compile_ok(source);
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn check_param_shadows_ok() {
    // §4.3: a parameter named `Ok` shadows the builtin constructor
    // inside the function body.
    let source = "func f(Ok: int): int = Ok + 1\nf(1);";
    let hir = compile_ok(source);
    assert_eq!(hir.items.len(), 2);
}

// ========== Result: `?` Propagation (0.0.2 U04, PLAN §3.4.3/D6) ==========

#[test]
fn check_question_chain_from_plan() {
    // PLAN §3.4.3 ratio example, verbatim semantics.
    let source = r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
func ratio(a: int, b: int): Result<int, string> {
    q = div(a, b)?;
    r = div(q, 2)?;
    Ok(r)
}
"#;
    compile_ok(source);
}

#[test]
fn check_question_on_non_result() {
    let errs = compile_err("func f(): Result<int, string> = 1?");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::QuestionOnNonResult { .. })),
        "expected QuestionOnNonResult, got {errs:?}"
    );
}

#[test]
fn check_question_outside_result_fn() {
    let errs =
        compile_err("func g(): Result<int, string> { Err(\"x\") }\nfunc main(): int { g()?; 0 }");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::QuestionOutsideResultFn)),
        "expected QuestionOutsideResultFn, got {errs:?}"
    );
}

#[test]
fn check_question_error_type_mismatch() {
    let errs = compile_err(
        "func g(): Result<int, string> { Err(\"x\") }\nfunc f(): Result<int, int> { q = g()?; Ok(q) }",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::QuestionTypeMismatch { .. })),
        "expected QuestionTypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_question_in_main_with_result_ret() {
    let source = r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
func main(): Result<int, string> {
    q = div(10, 2)?;
    Ok(q)
}
"#;
    compile_ok(source);
}

// ========== Result: choose patterns (0.0.2 U04, PLAN §3.4.2/D7) ==========

#[test]
fn check_choose_result_pattern_exhaustive() {
    let source = r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
res = choose div(10, 2) {
    when Ok(v) { v }
    when Err(e) {
        print("error: ", e);
        0 - 1
    }
};
"#;
    let hir = compile_ok(source);
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn check_choose_result_pattern_payload_typed() {
    // `v` binds as the Ok payload (int): `v + 1` typechecks, `v and true` doesn't.
    let source = r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
r = choose div(10, 2) { when Ok(v) { v + 1 } when Err(e) { 0 - 1 } };
"#;
    compile_ok(source);
    let errs = compile_err(
        "func div(a: int, b: int): Result<int, string> { if b == 0 { Err(\"x\") } else { Ok(a / b) } }\nr = choose div(10, 2) { when Ok(v) { v and true } when Err(e) { 0 - 1 } };",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

#[test]
fn check_choose_result_pattern_not_exhaustive() {
    let source = r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
r = choose div(10, 2) { when Ok(v) { v } };
"#;
    let errs = compile_err(source);
    assert!(
        err_kinds(&errs).iter().any(
            |k| matches!(k, TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns.iter().any(|m| m == "Err"))
        ),
        "expected ChooseNotExhaustive missing `Err`, got {errs:?}"
    );
}

#[test]
fn check_choose_result_guarded_arm_does_not_cover() {
    let source = r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
r = choose div(10, 2) { when Ok(v) if v > 0 { v } when Err(e) { 0 - 1 } };
"#;
    let errs = compile_err(source);
    assert!(
        err_kinds(&errs).iter().any(
            |k| matches!(k, TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns.iter().any(|m| m == "Ok"))
        ),
        "expected ChooseNotExhaustive missing `Ok`, got {errs:?}"
    );
}

#[test]
fn check_choose_result_pattern_on_non_result_scrutinee() {
    let errs = compile_err("choose 1 { when Ok(v) { 1 } otherwise { 0 } };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PatternTypeMismatch { .. })),
        "expected PatternTypeMismatch, got {errs:?}"
    );
}

// ========== Result: UnhandledResult (0.0.2 U04, PLAN §3.4.3) ==========

#[test]
fn check_unhandled_result_rejected() {
    let errs = compile_err(
        "func div(a: int, b: int): Result<int, string> { if b == 0 { Err(\"x\") } else { Ok(a / b) } }\nfunc main(): int { div(10, 2); 0 }",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::UnhandledResult { .. })),
        "expected UnhandledResult, got {errs:?}"
    );
}

#[test]
fn check_question_statement_is_handled() {
    // `div(10, 2)?;` unwraps to int before the statement is discarded —
    // no UnhandledResult.
    let source = r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
func main(): Result<int, string> {
    div(10, 2)?;
    Ok(0)
}
"#;
    compile_ok(source);
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

// ========== 0.0.2 U05: type classification (is_copy / is_owned) ==========

#[test]
fn is_copy_exhaustive() {
    assert!(Type::Int.is_copy());
    assert!(Type::Float.is_copy());
    assert!(Type::Bool.is_copy());
    assert!(Type::Unit.is_copy());
    assert!(Type::Func(vec![Type::Int], Box::new(Type::Unit)).is_copy());
    assert!(Type::Result(Box::new(Type::Int), Box::new(Type::Bool)).is_copy());
    // owned components
    assert!(!Type::String.is_copy());
    assert!(!Type::Box(Box::new(Type::Int)).is_copy());
    assert!(!Type::Array(Box::new(Type::Int)).is_copy());
    assert!(!Type::Ref(Box::new(Type::Int)).is_copy());
    assert!(!Type::Unsupported("x".to_string()).is_copy());
    assert!(!Type::Result(Box::new(Type::String), Box::new(Type::Int)).is_copy());
}

#[test]
fn is_owned_exhaustive() {
    assert!(Type::String.is_owned());
    assert!(Type::Box(Box::new(Type::Int)).is_owned());
    assert!(Type::Result(Box::new(Type::String), Box::new(Type::Int)).is_owned());
    assert!(
        Type::Result(
            Box::new(Type::Int),
            Box::new(Type::Box(Box::new(Type::Int)))
        )
        .is_owned()
    );
    // copy / neutral components
    assert!(!Type::Int.is_owned());
    assert!(!Type::Float.is_owned());
    assert!(!Type::Bool.is_owned());
    assert!(!Type::Unit.is_owned());
    assert!(!Type::Func(vec![], Box::new(Type::Int)).is_owned());
    assert!(!Type::Array(Box::new(Type::Int)).is_owned());
    assert!(!Type::Ref(Box::new(Type::Int)).is_owned());
    assert!(!Type::Unsupported("x".to_string()).is_owned());
    assert!(!Type::Result(Box::new(Type::Int), Box::new(Type::Bool)).is_owned());
}

// ========== 0.0.2 U05: ownership — valid programs ==========

#[test]
fn ownership_move_clone_basic() {
    // Top-level bindings are globals (never moved); the real transfer rules
    // play out on locals.
    compile_ok("func go() {\n    s = \"hello\";\n    t = move s;\n    u = clone t;\n}");
}

#[test]
fn ownership_clone_does_not_consume() {
    // `clone` leaves the slot live: the value is still usable afterwards.
    compile_ok("func go() {\n    s = \"hello\";\n    u = clone s;\n    t = move s;\n}");
}

#[test]
fn ownership_if_branch_transfer_no_use_after() {
    // Conditional transfer (PLAN §3.1.3 rule 3): each branch moves its own
    // value; using neither afterwards is fine.
    compile_ok(
        "func pick(c: bool, a: string, b: string): string {\n    if c { a }\n    else { b }\n}",
    );
}

#[test]
fn ownership_owned_global_read() {
    // Owned globals read as deep copies (D5) — producer tails included.
    compile_ok("const greeting = \"hi\";\nfunc show(): string {\n    greeting\n}");
}

#[test]
fn ownership_clone_global() {
    compile_ok("const msg = \"hi\";\nf = clone msg;");
}

#[test]
fn ownership_move_in_loop_no_use_after() {
    // A move inside a `while` body is legal when the value is not read
    // after the loop (exit state is merely `MaybeMoved`).
    compile_ok(
        "func drain() {\n    i = 0;\n    msg = \"loop\";\n    while i < 1 {\n        out = move msg;\n        i = i + 1;\n    };\n}",
    );
}

#[test]
fn ownership_const_move_at_tail() {
    // `const` bindings can be moved at a producing tail (PLAN §3.1.3 rule 6).
    compile_ok("func get(): string {\n    const s = \"hi\";\n    s\n}");
}

#[test]
fn ownership_box_deref_roundtrip() {
    compile_ok(
        "func go() {\n    b = box 42;\n    n = deref b;\n    deref b = n + 1;\n    c = clone b;\n    s = box \"heap\";\n    w = clone deref s;\n}",
    );
}

#[test]
fn ownership_ref_param_read() {
    // `ref` parameter used read-only (PLAN §3.3.1, DESIGN §10.4 example).
    compile_ok("func shout(s: ref string): int {\n    print(s);\n    42\n}");
}

#[test]
fn ownership_ref_param_clone() {
    // `clone s` of a `ref T` parameter yields an owned `T` (PLAN §3.3.1).
    compile_ok("func copy_out(s: ref string): string {\n    clone s\n}");
}

#[test]
fn ownership_ref_handle_flow() {
    // A borrow forwards to the next `ref` parameter (handle flow, zero copy).
    compile_ok(
        "func inner(s: ref string): int {\n    print(s);\n    0\n}\nfunc outer(t: ref string) {\n    inner(t);\n}",
    );
}

#[test]
fn ownership_ref_param_borrow_from_local() {
    compile_ok(
        "func show(s: ref string) {\n    print(s);\n}\nname = \"fleen\";\nshow(name);\nshow(name);",
    );
}

// ========== 0.0.2 U05: ownership — one failure per rule ==========

#[test]
fn ownership_use_after_move() {
    let errs = compile_err("func go() {\n    s = \"a\";\n    t = move s;\n    u = move s;\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::UseAfterMove { .. })),
        "expected UseAfterMove, got {errs:?}"
    );
}

#[test]
fn ownership_maybe_moved_after_branch() {
    // `s` moves in one branch only → any later use is `MaybeMoved`.
    let errs = compile_err(
        "func go() {\n    s = \"a\";\n    t = \"b\";\n    r = if true { move s }\n    else { move t };\n    print(clone s);\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MaybeMovedAfterBranch { .. })),
        "expected MaybeMovedAfterBranch, got {errs:?}"
    );
}

#[test]
fn ownership_assign_to_moved() {
    let errs = compile_err("func go() {\n    x = \"a\";\n    x = move x;\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::AssignToMoved { .. })),
        "expected AssignToMoved, got {errs:?}"
    );
}

#[test]
fn ownership_move_of_copy_type() {
    let errs = compile_err("func go() {\n    n = 42;\n    m = move n;\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MoveOfCopyType { .. })),
        "expected MoveOfCopyType, got {errs:?}"
    );
}

#[test]
fn ownership_move_out_of_box() {
    let errs = compile_err("func go() {\n    b = box 42;\n    c = move deref b;\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MoveOutOfBox)),
        "expected MoveOutOfBox, got {errs:?}"
    );
}

#[test]
fn ownership_move_out_of_global() {
    let errs = compile_err("g = \"hi\";\nh = move g;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MoveOutOfGlobal { .. })),
        "expected MoveOutOfGlobal, got {errs:?}"
    );
}

#[test]
fn ownership_move_of_borrowed() {
    let errs = compile_err("func f(s: ref string) {\n    move s;\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MoveOfBorrowed { .. })),
        "expected MoveOfBorrowed, got {errs:?}"
    );
}

#[test]
fn ownership_invalid_clone_place() {
    let errs = compile_err("x = clone 42;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidClonePlace)),
        "expected InvalidClonePlace, got {errs:?}"
    );
}

#[test]
fn ownership_owned_arg_requires_move() {
    // A bare owned local in a consuming position (binding RHS) needs
    // `move` / `clone` (PLAN §3.1.3 rule 2).
    let errs = compile_err("func go() {\n    s = \"a\";\n    t = s;\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
}

#[test]
fn ownership_borrow_arg_with_move() {
    let errs = compile_err(
        "func f(s: ref string) {\n    print(s);\n}\nfunc go() {\n    a = \"x\";\n    f(move a);\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::BorrowArgWithMove { .. })),
        "expected BorrowArgWithMove, got {errs:?}"
    );
}

#[test]
fn ownership_deref_assign_of_global_box() {
    let errs = compile_err("g = box 42;\nderef g = 1;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::DerefAssignOfGlobalBox)),
        "expected DerefAssignOfGlobalBox, got {errs:?}"
    );
}

#[test]
fn ownership_deref_invalid_operand() {
    let errs = compile_err("n = 42;\nx = deref n;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { .. })),
        "expected InvalidOperand, got {errs:?}"
    );
}

#[test]
fn ownership_deref_assign_invalid_operand() {
    let errs = compile_err("n = 42;\nderef n = 1;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { .. })),
        "expected InvalidOperand, got {errs:?}"
    );
}

#[test]
fn ownership_while_back_edge_guard() {
    // The guard reads an owned local the body may move → back-edge use is
    // `MaybeMovedAfterBranch` (PLAN §3.1.3, conservative `while` rule).
    let errs = compile_err(
        "func ready(s: ref string): bool {\n    false\n}\nfunc go() {\n    i = 0;\n    s = \"a\";\n    while ready(s) {\n        t = move s;\n        i = i + 1;\n    }\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MaybeMovedAfterBranch { .. })),
        "expected MaybeMovedAfterBranch, got {errs:?}"
    );
}

#[test]
fn ownership_state_does_not_cross_function_boundary() {
    // Rule 8: a move inside a nested function is invisible to the outer
    // scope — the outer use stays legal.
    compile_ok(
        "func outer() {\n    s = \"a\";\n    func inner() {\n        t = move s;\n    }\n    u = move s;\n}",
    );
}

// ========== 0.0.2 U05: `ref` parameter discipline (DESIGN §10.4) ==========

#[test]
fn ownership_borrow_of_box_interior() {
    // `deref b` is not a variable: 0.0.2 cannot borrow a box's pointee
    // (DESIGN §10.4).
    let errs = compile_err(
        "func show(s: ref string) {\n    print(s);\n}\nb = box \"hi\";\nshow(deref b);",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::BorrowOfBoxInterior)),
        "expected BorrowOfBoxInterior, got {errs:?}"
    );
}

#[test]
fn ownership_ref_arg_not_a_variable() {
    // A `ref` argument must be a variable (local, global, or another `ref`
    // parameter) — a temporary has no slot to point at (DESIGN §10.4).
    let errs = compile_err("func show(s: ref string) {\n    print(s);\n}\nshow(\"lit\");");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::RefArgNotAVariable)),
        "expected RefArgNotAVariable, got {errs:?}"
    );

    // `clone s` yields a fresh value, not a place: rejected too.
    let errs = compile_err(
        "func show(s: ref string) {\n    print(s);\n}\nfunc go() {\n    s = \"a\";\n    show(clone s);\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::RefArgNotAVariable)),
        "expected RefArgNotAVariable, got {errs:?}"
    );
}

#[test]
fn ownership_ref_arg_local_and_global_ok() {
    // The supported cases: a local and a global borrowed by a `ref` param.
    compile_ok(
        "func show(s: ref string) {\n    print(s);\n}\nname = \"fleen\";\nshow(name);\nfunc go() {\n    local = \"a\";\n    show(local);\n    show(name);\n}",
    );
}

// ========== 0.0.2 U05: consuming positions (rule 2) across all call sites ==========

#[test]
fn ownership_choose_scrutinee_bare_owned() {
    // A `choose` scrutinee is a consuming position (D7): bare owned → error,
    // `move`/`clone` wrappers are fine.
    let errs = compile_err(
        "func go() {\n    s = \"abc\";\n    r = choose s {\n        when \"abc\" { 1 }\n        otherwise { 0 }\n    };\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
}

#[test]
fn ownership_choose_scrutinee_move() {
    compile_ok(
        "func go() {\n    s = \"abc\";\n    r = choose move s {\n        when \"abc\" { 1 }\n        otherwise { 0 }\n    };\n    print(r as string);\n}",
    );
}

#[test]
fn ownership_maybe_moved_via_choose() {
    // A branch tail of `choose` transfers conditionally: a later use is
    // `MaybeMovedAfterBranch` (DESIGN §10.2 rule 3).
    let errs = compile_err(
        "func go(c: bool) {\n    a = \"a\";\n    b = \"b\";\n    r = choose c {\n        when true { a }\n        otherwise { b }\n    };\n    print(a);\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MaybeMovedAfterBranch { .. })),
        "expected MaybeMovedAfterBranch, got {errs:?}"
    );
}

#[test]
fn ownership_owned_arg_in_call() {
    // A bare owned local passed to an *owned* parameter needs `move`/`clone`.
    let errs = compile_err(
        "func take(s: string) {\n    print(s);\n}\nfunc go() {\n    s = \"a\";\n    take(s);\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
}

#[test]
fn ownership_box_inner_bare_owned() {
    // The inner value of `box` is consumed into the allocation.
    let errs = compile_err("func go() {\n    s = \"a\";\n    b = box s;\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
}

#[test]
fn ownership_box_inner_explicit_transfer_ok() {
    compile_ok("func go() {\n    s = \"a\";\n    b = box move s;\n    c = box \"fresh\";\n}");
}

#[test]
fn ownership_result_payload_bare_owned() {
    // The payload of `Ok`/`Err` is moved into the Result (D7).
    let errs = compile_err("func go() {\n    s = \"a\";\n    res: Result<string, int> = Ok(s);\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
}

// ========== 0.0.2 U05: `Access` annotations (contract for lowering, U06) ==========

/// The function item of a single-item typed program.
fn only_func(hir: &TypedHir) -> &TypedFuncDeclHir {
    match &hir.items[0] {
        TypedHirItem::Decl(TypedDeclHir::Func(f)) => f,
        other => panic!("expected function item, got {other:?}"),
    }
}

#[test]
fn ownership_access_read_clone_for_owned_local() {
    // Read-only use of an owned local auto-clones (D3): the slot is
    // untouched, so a later `move` stays legal.
    let hir = compile_ok("func go() {\n    s = \"a\";\n    print(s);\n    t = move s;\n}");
    let f = only_func(&hir);
    let TypedFuncBodyHir::Block(b) = &f.body else {
        panic!("expected block body");
    };
    let TypedStmtHir::Expr(call, _) = &b.stmts[1] else {
        panic!("expected print statement");
    };
    let TypedExprHir::Call(_, args, _) = &**call else {
        panic!("expected call");
    };
    let TypedExprHir::Ident { access, .. } = &args[0] else {
        panic!("expected identifier argument");
    };
    assert_eq!(*access, Access::Clone);
}

#[test]
fn ownership_access_move_explicit() {
    let hir = compile_ok("func go() {\n    s = \"a\";\n    t = move s;\n}");
    let f = only_func(&hir);
    let TypedFuncBodyHir::Block(b) = &f.body else {
        panic!("expected block body");
    };
    let TypedStmtHir::Decl(TypedDeclHir::Var(v)) = &b.stmts[1] else {
        panic!("expected binding statement");
    };
    let TypedExprHir::Ident { access, .. } = &*v.init else {
        panic!("expected identifier RHS");
    };
    assert_eq!(*access, Access::Move);
}

#[test]
fn ownership_access_tail_implicit_move() {
    // The value-producing tail of a function transfers implicitly: the
    // bare owned local there is annotated `Move` (DESIGN §10.2 rule 3).
    let hir = compile_ok("func go(): string {\n    s = \"a\";\n    s\n}");
    let f = only_func(&hir);
    let TypedFuncBodyHir::Block(b) = &f.body else {
        panic!("expected block body");
    };
    let TypedExprHir::Ident { access, .. } = &**b.tail_expr.as_ref().expect("tail expr") else {
        panic!("expected identifier tail");
    };
    assert_eq!(*access, Access::Move);
}

#[test]
fn ownership_access_deref_loads_box_not_pointee() {
    // `deref b` keeps a plain load of the box slot (lowering emits
    // `LoadLocal`/`CloneGlobal` + `DerefBox`), never a clone of the box.
    let hir = compile_ok("func go(): int {\n    b = box 42;\n    deref b\n}");
    let f = only_func(&hir);
    let TypedFuncBodyHir::Block(b) = &f.body else {
        panic!("expected block body");
    };
    let TypedExprHir::Deref(inner, _) = &**b.tail_expr.as_ref().expect("tail expr") else {
        panic!("expected deref tail");
    };
    let TypedExprHir::Ident { access, .. } = &**inner else {
        panic!("expected box identifier");
    };
    assert_eq!(*access, Access::Copy);
}

// ========== 0.0.2 U05: statement-position tails (DESIGN §10.2 rule 3) ==========

#[test]
fn ownership_stmt_block_tail_produces() {
    // A statement-position block still *produces* its tail value (the
    // value is then discarded): the owned local transfers out and the
    // program is fine as long as nothing uses it afterwards.
    compile_ok("func go() {\n    s = \"a\";\n    { s };\n}");
}

#[test]
fn ownership_stmt_block_tail_use_after() {
    // …but using the value after the discarded tail reports
    // `UseAfterMove` (the conservative, Rust-consistent reading of
    // DESIGN §10.2 rule 3).
    let errs = compile_err("func go() {\n    s = \"a\";\n    { s };\n    print(s);\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::UseAfterMove { .. })),
        "expected UseAfterMove, got {errs:?}"
    );
}
