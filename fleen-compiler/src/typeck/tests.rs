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
            structs: Vec::new(),
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

// 0.0.3 V06: `std.io.println` shares the `(string...) -> unit` signature
// with `print` (D8): non-string args are rejected at compile time.
#[test]
fn check_builtin_println_wrong_arg() {
    let result =
        crate::compile("import std.io;\nfunc main(): int {\n    std.io.println(1);\n    0\n}\n");
    assert!(
        result.is_err(),
        "std.io.println with non-string arg should fail to compile"
    );
}

// ========== Cast: UnsupportedCast (0.0.2 U13, PLAN §8.1) ==========
//
// The 0.0.2 whitelist is scalar (int/float/bool) → string only; everything
// else — including identity casts and box values — is `UnsupportedCast`.

#[test]
fn check_unsupported_cast_identity_rejected() {
    let errs = compile_err("x = 42 as int;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::UnsupportedCast { .. })),
        "expected UnsupportedCast, got {errs:?}"
    );
}

#[test]
fn check_unsupported_cast_fields_pin_from_and_to() {
    let errs = compile_err("x = 42 as float;");
    assert!(
        err_kinds(&errs).iter().any(|k| matches!(
            k,
            TypeckErrorKind::UnsupportedCast {
                from: Type::Int,
                to: Type::Float
            }
        )),
        "expected UnsupportedCast {{ int -> float }}, got {errs:?}"
    );
}

#[test]
fn check_unsupported_cast_box_to_string_rejected() {
    let errs = compile_err("func f(b: box<int>) {\n    x = b as string;\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::UnsupportedCast { .. })),
        "expected UnsupportedCast, got {errs:?}"
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
    // 0.0.3 V03: indexing a non-array value is an InvalidOperand for `index`.
    let errs = compile_err("x = 1;\nx[0];");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { op, .. } if op == "index")),
        "expected InvalidOperand(index), got {errs:?}"
    );
}

#[test]
fn check_field_access_on_non_struct() {
    // 0.0.3 V02: field access on a non-struct value is an InvalidOperand.
    let errs = compile_err("x = 1;\nx.foo;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { .. })),
        "expected InvalidOperand, got {errs:?}"
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
    // 0.0.3 V02: the struct verdict is the cached D2 flag.
    let copy_info = Type::Struct(Box::new(StructInfo {
        id: StructId(0),
        name: "Copy".into(),
        copy: true,
    }));
    let owned_info = Type::Struct(Box::new(StructInfo {
        id: StructId(1),
        name: "Owned".into(),
        copy: false,
    }));
    assert!(copy_info.is_copy());
    assert!(!copy_info.is_owned());
    assert!(!owned_info.is_copy());
    assert!(owned_info.is_owned());
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
    // 0.0.3 V03 (D3): arrays are owned for every element type.
    assert!(Type::Array(Box::new(Type::Int)).is_owned());
    assert!(Type::Array(Box::new(Type::String)).is_owned());
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

// ========== 0.0.3 V03: arrays ==========

#[test]
fn infer_array_literal_type() {
    let hir = compile_ok("[1, 2, 3];");
    assert_eq!(only_expr_ty(&hir), Type::Array(Box::new(Type::Int)));
}

#[test]
fn infer_nested_array_literal_type() {
    let hir = compile_ok("[[1], [2, 3]];");
    assert_eq!(
        only_expr_ty(&hir),
        Type::Array(Box::new(Type::Array(Box::new(Type::Int))))
    );
}

#[test]
fn array_literal_element_uniformity() {
    let errs = compile_err("[1, \"x\"];");
    assert!(
        err_kinds(&errs).iter().any(
            |k| matches!(k, TypeckErrorKind::ArrayTypeMismatch { expected, found }
                if *expected == Type::Int && *found == Type::String)
        ),
        "expected ArrayTypeMismatch, got {errs:?}"
    );
}

#[test]
fn array_literal_annotated_empty_ok() {
    let hir = compile_ok("empty: [int] = [];");
    match &hir.items[0] {
        TypedHirItem::Decl(TypedDeclHir::Var(v)) => {
            assert_eq!(v.ty, Some(Type::Array(Box::new(Type::Int))));
        }
        other => panic!("expected var binding, got {other:?}"),
    }
}

#[test]
fn array_literal_unannotated_empty_is_error() {
    let errs = compile_err("e = [];");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::CannotInferArrayType)),
        "expected CannotInferArrayType, got {errs:?}"
    );
}

#[test]
fn array_literal_nested_empty_with_annotation_ok() {
    // The annotation flows down: the inner `[]` infers `[int]` from the
    // outer `[[int]]` element hint.
    compile_ok("m: [[int]] = [[]];");
}

#[test]
fn array_is_never_copy() {
    // D3: arrays are owned for every element type — even `[int]`.
    assert!(!Type::Array(Box::new(Type::Int)).is_copy());
    assert!(!Type::Array(Box::new(Type::Bool)).is_copy());
    assert!(Type::Array(Box::new(Type::Int)).is_owned());
    // And a struct with an array field is therefore owned (D2 falls out).
    assert!(!Type::Array(Box::new(Type::Array(Box::new(Type::Int)))).is_copy());
}

#[test]
fn struct_with_array_field_is_owned() {
    let hir = compile_ok("struct W { labels: [int] }\nw = W { labels: [1] };");
    assert_eq!(hir.items.len(), 1); // struct decls are not items
    match &hir.items[0] {
        TypedHirItem::Decl(TypedDeclHir::Var(v)) => {
            let ty = v.init.ty();
            assert!(ty.is_owned(), "W with an array field must be owned");
            assert!(!ty.is_copy());
        }
        other => panic!("expected var binding, got {other:?}"),
    }
}

#[test]
fn array_index_read_type() {
    let hir = compile_ok("a = [1, 2];\na[0];");
    match &hir.items[1] {
        TypedHirItem::Expr(e) => assert_eq!(e.ty(), Type::Int),
        other => panic!("expected expression, got {other:?}"),
    }
}

#[test]
fn array_index_not_int() {
    let errs = compile_err("a = [1];\na[\"x\"];");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::IndexNotInt { found } if *found == Type::String)),
        "expected IndexNotInt, got {errs:?}"
    );
}

#[test]
fn move_out_of_array_element() {
    let errs = compile_err("s = [\"a\"];\nt = move s[0];");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MoveOutOfArray { path } if path == "s[0]")),
        "expected MoveOutOfArray, got {errs:?}"
    );
}

#[test]
fn clone_array_element_folds_to_read() {
    // D3: an element read is already a copy, so `clone a[i]` is the read.
    let hir = compile_ok("s = [\"a\"];\nt = clone s[0];");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn move_whole_array_is_legal() {
    // Arrays are owned, so the whole array transfers with `move`. (Function
    // locals: globals are never moved, D5.)
    compile_ok("func main(): int {\n    a = [1, 2];\n    b = move a;\n    b[0]\n}");
}

#[test]
fn array_element_write_typechecks() {
    compile_ok("a = [1, 2];\na[1] = 10;");
}

#[test]
fn array_element_write_owned_rhs_requires_move() {
    // Function locals: a global would read as a deep copy (D5) and not
    // exercise the transfer rule.
    let errs = compile_err(
        "func main(): int {\n    s = [\"x\"];\n    t = \"y\";\n    s[0] = t;\n    0\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
    compile_ok(
        "func main(): int {\n    s = [\"x\"];\n    t = \"y\";\n    s[0] = move t;\n    0\n}",
    );
}

#[test]
fn ragged_rows_are_legal() {
    compile_ok("m = [[1, 2], [3]];");
}

#[test]
fn eq_on_array_rejected() {
    let errs = compile_err("a = [1];\nb = [1];\na == b;");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { op, .. } if op == "==")),
        "expected InvalidOperand(==), got {errs:?}"
    );
}

#[test]
fn index_on_box_rejected_with_help_shape() {
    // `deref big[0]` parses as `deref(big[0])` (postfix binds tighter than
    // the prefix), so the box itself is indexed — an IndexOnBox pointing at
    // the `(deref big)[0]` spelling.
    let errs = compile_err("big = box [1];\nx = deref big[0];");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::IndexOnBox)),
        "expected IndexOnBox, got {errs:?}"
    );
    compile_ok("big = box [1];\nx = (deref big)[0];");
}

#[test]
fn array_in_box_and_result_and_ref_param() {
    // box<[T]> pointee, Result component, and `ref [T]` parameter are all
    // legal containers (D3); a read through the borrow is a copy.
    compile_ok("big = box [1, 2];\nx = (deref big)[1];");
    compile_ok("r: Result<[int], string> = Ok([1, 2]);\nr;");
    compile_ok(
        "func first(a: ref [int]): int = a[0]\nfunc main(): int {\n    xs = [7, 8];\n    first(xs)\n}",
    );
}

#[test]
fn array_literal_elements_are_consuming_positions() {
    let errs = compile_err("func main(): int {\n    s = \"a\";\n    w = [s];\n    0\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
    compile_ok("func main(): int {\n    s = \"a\";\n    w = [clone s];\n    0\n}");
}

#[test]
fn use_after_element_write_resets_state() {
    // Writing an element does not move the array; but the array binding was
    // moved, and the ownership pass reports the write as AssignToMoved (the
    // move-state machinery keys off the binding, unchanged from 0.0.2).
    let errs =
        compile_err("func main(): int {\n    a = [1];\n    b = move a;\n    a[0] = 2;\n    0\n}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::AssignToMoved { name } if name == "a")),
        "expected AssignToMoved, got {errs:?}"
    );
}

// ========== For Expression (0.0.3 V04) ==========

#[test]
fn for_over_array_binds_element_type() {
    let hir = compile_ok("names = [\"a\", \"b\"];\nfor name in names { print(name); };");
    // The `for` statement itself is `unit`.
    let last = hir.items.last().expect("items");
    match last {
        TypedHirItem::Expr(e) => assert_eq!(e.ty(), Type::Unit),
        other => panic!("expected expression item, got {other:?}"),
    }
}

#[test]
fn for_over_non_iterable_is_rejected() {
    let errs = compile_err("for x in 42 { };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::NotIterable { .. })),
        "expected NotIterable, got {errs:?}"
    );
}

#[test]
fn for_loop_variable_is_immutable() {
    // The loop variable is a fresh *immutable* binding (the resolver
    // declares it with `mutable: false`), so assigning to it inside the body
    // is `AssignToImmutable` — not a §4.5 loop-scope reassignment. Checked at
    // the resolver stage, where the diagnostic lives.
    let tokens = tokenize("for x in [1, 2] { x = 3; };").unwrap();
    let ast = parse(tokens).unwrap();
    let err = resolve(ast).expect_err("loop variable must be immutable");
    assert!(
        err.iter().any(|e| matches!(
            &e.kind,
            crate::resolver::error::ResolveErrorKind::AssignToImmutable { name } if name == "x"
        )),
        "expected AssignToImmutable(x), got {err:?}"
    );
}

#[test]
fn for_body_assigns_outer_mutable() {
    // Assigning a *different* (outer) mutable inside the body follows the
    // §4.5 loop-scope rules, exactly like a `while` body.
    compile_ok(
        "func main(): int {\n    total = 0;\n    for n in [1, 2, 3] { total = total + n; }\n    total\n}",
    );
}

#[test]
fn for_element_use_after_body_is_a_fresh_binding() {
    // The loop variable is scoped to the body: reading it after the loop is
    // undefined, not the last iteration's value. The resolver (which owns
    // scoping) reports it before typeck ever runs.
    let tokens = tokenize("for x in [1, 2] { };\nx;").unwrap();
    let ast = parse(tokens).unwrap();
    let err = resolve(ast).expect_err("loop variable must not outlive the body");
    assert!(
        err.iter().any(|e| matches!(
            &e.kind,
            crate::resolver::error::ResolveErrorKind::UndeclaredVariable { name } if name == "x"
        )),
        "expected UndeclaredVariable(x), got {err:?}"
    );
}

#[test]
fn for_empty_array_iterable_is_typed_by_annotation() {
    // An empty literal still needs its element type from the annotation, but
    // once it has one the loop variable is that element type.
    compile_ok("xs: [int] = [];\nfor x in xs { print(x as string); };");
}

#[test]
fn for_over_string_is_not_iterable() {
    // The closed iterable set is `{ [T], Range }`: a string is not iterable.
    let errs = compile_err("s = \"hi\";\nfor c in s { };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::NotIterable { .. })),
        "expected NotIterable, got {errs:?}"
    );
}

// ========== 0.0.3 V06: builtin / schematic call tests ==========

// --- Prelude builtins (range, range_from — no import needed) ---

#[test]
fn range_constructor_returns_range_type() {
    // `range(5)` is a MakeRange call (D5), type-checks as `Range`.
    let hir = compile_ok("func main(): int {\n    range(5);\n    0\n}\n");
    // The top-level func body has the range expr as a statement; we verify
    // by checking the module compiles (no errors).
    let _ = hir;
}

#[test]
fn range_from_constructor_returns_range_type() {
    let hir = compile_ok("func main(): int {\n    range_from(2, 5);\n    0\n}\n");
    let _ = hir;
}

#[test]
fn range_wrong_arg_type_reports_arg_type_mismatch() {
    // `range` expects `int`; passing a float is a type error.
    let errs = compile_err("func main(): int {\n    range(5.0);\n    0\n}\n");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArgTypeMismatch { .. })),
        "expected ArgTypeMismatch, got {errs:?}"
    );
}

#[test]
fn range_from_wrong_arity_reports_arity_mismatch() {
    // `range_from(a, b)` takes 2 args; passing 1 is an arity error.
    let errs = compile_err("func main(): int {\n    range_from(2);\n    0\n}\n");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArityMismatch { .. })),
        "expected ArityMismatch, got {errs:?}"
    );
}

// --- print prelude (variadic, string-only) ---

#[test]
fn print_with_string_literal_is_ok() {
    let hir = compile_ok("func main(): int {\n    print(\"hi\");\n    0\n}\n");
    let _ = hir;
}

#[test]
fn print_with_int_arg_reports_arg_type_mismatch() {
    // 0.0.2 U13: `print` requires string arguments.
    let errs = compile_err("func main(): int {\n    print(42);\n    0\n}\n");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArgTypeMismatch { .. })),
        "expected ArgTypeMismatch, got {errs:?}"
    );
}

// --- Schematic std.arr builtins (tested via resolve_schematic_call) ---

#[test]
fn schematic_arr_len_resolves_element_type() {
    // std.arr.len([int]) → int
    use crate::typeck::builtins::{find_builtin, resolve_schematic_call};
    use crate::typeck::typed_hir::Type;

    let sig = find_builtin("std.arr.len").expect("std.arr.len must be a builtin");
    let resolved =
        resolve_schematic_call(&sig, &[Type::Array(Box::new(Type::Int))]).expect("arity match");
    assert_eq!(resolved.ret_type, Type::Int);
    assert!(resolved.is_schematic);
    assert!(resolved.conflict.is_none());
}

#[test]
fn schematic_arr_get_resolves_element_type() {
    // std.arr.get([string], int) → Result<string, string>
    use crate::typeck::builtins::{find_builtin, resolve_schematic_call};
    use crate::typeck::typed_hir::Type;

    let sig = find_builtin("std.arr.get").expect("std.arr.get must be a builtin");
    let resolved = resolve_schematic_call(&sig, &[Type::Array(Box::new(Type::String)), Type::Int])
        .expect("arity match");
    assert_eq!(
        resolved.ret_type,
        Type::Result(Box::new(Type::String), Box::new(Type::String))
    );
    assert!(resolved.conflict.is_none());
}

#[test]
fn schematic_arr_push_conflict_on_mismatched_element_type() {
    // std.arr.push(["a"], 1) binds T0 to string (from array) and int (from
    // value) → ArrayTypeMismatch conflict on arg index 1.
    use crate::typeck::builtins::{find_builtin, resolve_schematic_call};
    use crate::typeck::typed_hir::Type;

    let sig = find_builtin("std.arr.push").expect("std.arr.push must be a builtin");
    let resolved = resolve_schematic_call(&sig, &[Type::Array(Box::new(Type::String)), Type::Int])
        .expect("arity match");
    let conflict = resolved.conflict.expect("expected a type conflict");
    assert_eq!(conflict.expected, Type::String);
    assert_eq!(conflict.found, Type::Int);
    assert_eq!(conflict.arg_index, 1);
}

#[test]
fn schematic_arr_set_arity_mismatch_returns_none() {
    // std.arr.set takes 3 args; passing 2 → None (handled as ArityMismatch
    // by typeck).
    use crate::typeck::builtins::{find_builtin, resolve_schematic_call};
    use crate::typeck::typed_hir::Type;

    let sig = find_builtin("std.arr.set").expect("std.arr.set must be a builtin");
    let result = resolve_schematic_call(&sig, &[Type::Array(Box::new(Type::Int)), Type::Int]);
    assert!(result.is_none(), "2-arg call to 3-arg builtin should fail");
}

// --- End-to-end: std.arr through the full compile pipeline (with imports) ---

#[test]
fn std_arr_len_compiles_through_full_pipeline() {
    // The single-file `resolve` doesn't load std modules, so go through the
    // public `fleen_compiler::compile` entry which wires up std sources.
    let bc =
        crate::compile("import std.arr;\nfunc main(): int {\n    std.arr.len([1, 2]);\n    0\n}\n")
            .expect("std.arr.len should compile through the full pipeline");
    let _ = bc;
}

#[test]
fn std_arr_set_type_mismatch_does_not_compile() {
    // set([string], int, int) — the value arg has wrong element type.
    // Through the full pipeline, typeck should reject this.
    let result = crate::compile(
        "import std.arr;\nfunc main(): int {\n    std.arr.set([\"a\", \"b\"], 0, 1);\n    0\n}\n",
    );
    assert!(
        result.is_err(),
        "std.arr.set with mismatched element type should fail to compile"
    );
}

#[test]
fn std_arr_push_type_mismatch_does_not_compile() {
    // push(["a"], 1) — element type mismatch.
    let result = crate::compile(
        "import std.arr;\nfunc main(): int {\n    std.arr.push([\"a\"], 1);\n    0\n}\n",
    );
    assert!(
        result.is_err(),
        "std.arr.push with mismatched element type should fail to compile"
    );
}

#[test]
fn iter_helpers_two_type_variables_substitute_independently() {
    // 0.0.3 V07: `map`/`fold` carry *two* type variables (T0, T1) — the
    // schematic mechanism's first multi-variable users. Each substitutes
    // independently at one call site.
    use crate::typeck::builtins::{find_builtin, resolve_schematic_call};
    use crate::typeck::typed_hir::Type;

    let sig = find_builtin("std.collection.iter.map").expect("iter.map must be a builtin");
    // map([int], (int) -> string) ->> [string]: T=Var(0)=int, U=Var(1)=string.
    let resolved = resolve_schematic_call(
        &sig,
        &[
            Type::Array(Box::new(Type::Int)),
            Type::Func(vec![Type::Int], Box::new(Type::String)),
        ],
    )
    .expect("map resolves arity-wise");
    assert_eq!(
        resolved.ret_type,
        Type::Array(Box::new(Type::String)),
        "map([int], (int)->string) must return [string]"
    );
    assert!(resolved.conflict.is_none(), "no conflict expected");

    // fold([string], "", (string, string) -> string) ->> string.
    let fold = find_builtin("std.collection.iter.fold").expect("iter.fold must be a builtin");
    let resolved = resolve_schematic_call(
        &fold,
        &[
            Type::Array(Box::new(Type::String)),
            Type::String,
            Type::Func(vec![Type::String, Type::String], Box::new(Type::String)),
        ],
    )
    .expect("fold resolves arity-wise");
    assert_eq!(
        resolved.ret_type,
        Type::String,
        "fold returns its accumulator type"
    );
    assert!(resolved.conflict.is_none(), "no conflict expected");
}

#[test]
fn iter_helpers_two_variable_conflict_is_detected() {
    // Var(0) bound to `int` through the array, then to `string` through the
    // function's parameter: the element type of `f` clashes with the array.
    use crate::typeck::builtins::{find_builtin, resolve_schematic_call};
    use crate::typeck::typed_hir::Type;

    let sig = find_builtin("std.collection.iter.map").expect("iter.map must be a builtin");
    let resolved = resolve_schematic_call(
        &sig,
        &[
            Type::Array(Box::new(Type::Int)),
            Type::Func(vec![Type::String], Box::new(Type::String)),
        ],
    )
    .expect("map resolves arity-wise");
    assert!(
        resolved.conflict.is_some(),
        "map([int], (string)->string) must report a Var(0) conflict"
    );
}

#[test]
fn iter_map_compiles_through_full_pipeline() {
    // A two-variable schematic helper end-to-end: typeck, inline lowering,
    // and codegen all accept it (verify/execution is covered by the e2e
    // `iter_helpers.fln` runner).
    let bc = crate::compile(
        "import std.io;\nimport std.collection.iter;\n\
         func f(x: int): string = \"n\";\n\
         func main(): int {\n    \
             std.io.println(std.collection.iter.map([1, 2], f)[1]);\n    \
         0\n}\n",
    )
    .expect("map([int], (int)->string) should compile through the full pipeline");
    let _ = bc;
}

#[test]
fn iter_map_function_arg_type_mismatch_does_not_compile() {
    // `f` takes a `string`, but the array is `[int]`: Var(0) clashes and
    // typeck rejects the release — the same `ArrayTypeMismatch` a bad
    // `std.arr.push` reports.
    let result = crate::compile(
        "import std.collection.iter;\n\
         func f(s: string): string = \"x\";\n\
         func main(): int {\n    std.collection.iter.map([1, 2], f);\n    0\n}\n",
    );
    assert!(
        result.is_err(),
        "map([int], (string)->string) should fail to compile"
    );
}

#[test]
fn range_wrong_arity_at_typeck_level() {
    let errs = compile_err("func main(): int {\n    range(5, 6);\n    0\n}\n");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArityMismatch { .. })),
        "expected ArityMismatch, got {errs:?}"
    );
}

// ========== 0.0.4 async / await / Future<T> (T02) ==========

#[test]
fn async_func_decl_is_async() {
    // An `async func` is recorded on the typed declaration.
    let hir = compile_ok("async func gen(x: int): int = x + 1\n");
    assert!(only_func(&hir).is_async);

    let hir = compile_ok("func gen(x: int): int = x + 1\n");
    assert!(!only_func(&hir).is_async);
}

#[test]
fn async_fn_call_returns_future() {
    // F1: the *caller* of an async fn sees `Future<T>`.
    let hir = compile_ok("async func gen(x: int): int = x + 1\ngen(1);");
    assert_eq!(hir.items.len(), 2);
    let fut = match &hir.items[1] {
        TypedHirItem::Expr(e) => e.ty(),
        other => panic!("expected expr, got {other:?}"),
    };
    assert_eq!(fut, Type::Future(Box::new(Type::Int)));
}

#[test]
fn async_func_body_still_returns_t() {
    // F1: the async fn's local `ret_type` is `T`, not `Future<T>`.
    let hir = compile_ok("async func gen(x: int): int = x + 1\n");
    assert_eq!(only_func(&hir).ret_type.as_ref(), Some(&Type::Int));
}

#[test]
fn await_unwraps_future() {
    // F3 rule: `e: Future<T> ⇒ e await : T`.
    let hir = compile_ok("async func gen(x: int): int = x + 1\nn = gen(2) await;");
    let init = match &hir.items[1] {
        TypedHirItem::Decl(TypedDeclHir::Var(v)) => &*v.init,
        other => panic!("expected var decl, got {other:?}"),
    };
    assert_eq!(init.ty(), Type::Int);
}

#[test]
fn await_in_async_fn_body() {
    // An async fn may await internally; its body still returns `T`.
    let hir = compile_ok(
        "async func gen(x: int): int = x + 1\n\
         async func outer(): int {\n    gen(1) await\n}\n",
    );
    assert_eq!(only_func(&hir).ret_type.as_ref(), Some(&Type::Int));
    assert!(only_func(&hir).is_async);
}

#[test]
fn await_non_future_is_error() {
    // F3: awaiting a non-Future is `AwaitNonFuture`.
    let errs = compile_err("n = 5 await;");
    let want = TypeckErrorKind::AwaitNonFuture { found: Type::Int };
    assert!(
        errs.iter().any(|e| e.kind == want),
        "expected AwaitNonFuture, got {errs:?}"
    );
}

#[test]
fn future_is_not_copy() {
    // F3: a Future owns its suspend state — never Copy, always owned.
    let fut = Type::Future(Box::new(Type::Int));
    assert!(!fut.is_copy());
    assert!(fut.is_owned());
    // Copy classes are unchanged inside async fn: `int` stays Copy.
    assert!(Type::Int.is_copy());
}

// ========== 0.0.4 async boundaries (T03) ==========

#[test]
fn ref_param_in_async_is_error() {
    // One error per binding: the declaration is reported, later uses are not.
    let errs = compile_err("async func show(s: ref string) {\n    print(s);\n    print(s);\n}\n");
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::RefInAsync]);
}

#[test]
fn ref_captured_by_nested_async_is_error() {
    let errs = compile_err(
        "func outer(s: ref string) {\n    async func inner() {\n        print(s);\n    }\n}\n",
    );
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::RefInAsync]);
}

#[test]
fn async_global_field_write_is_error() {
    let errs = compile_err(
        "struct Point { x: int, }\np = Point { x: 0 };\n\
         async func bump() {\n    p.x = 1;\n}\n",
    );
    assert_eq!(
        err_kinds(&errs),
        vec![&TypeckErrorKind::AsyncGlobalWrite {
            name: "p".to_string()
        }]
    );
}

#[test]
fn async_global_index_write_is_error() {
    let errs = compile_err("xs: [int] = [1];\nasync func bump() {\n    xs[0] = 2;\n}\n");
    assert_eq!(
        err_kinds(&errs),
        vec![&TypeckErrorKind::AsyncGlobalWrite {
            name: "xs".to_string()
        }]
    );
}

#[test]
fn async_const_global_field_write_is_error() {
    // Mutability does not matter: async code may not write any global.
    let errs = compile_err(
        "struct Point { x: int, }\nconst p = Point { x: 0 };\n\
         async func bump() {\n    p.x = 1;\n}\n",
    );
    assert_eq!(
        err_kinds(&errs),
        vec![&TypeckErrorKind::AsyncGlobalWrite {
            name: "p".to_string()
        }]
    );
}

#[test]
fn async_global_read_is_ok() {
    compile_ok(
        "struct Point { x: int, }\np = Point { x: 7 };\n\
         async func read() {\n    print(p.x as string);\n}\n",
    );
}

#[test]
fn async_local_shadow_writes_are_ok() {
    // The local `p` shadows the global: `p = ...` binds locally and `p.x = 1`
    // writes that local, so neither is an async global write.
    compile_ok(
        "struct Point { x: int, }\np = Point { x: 7 };\n\
         async func local() {\n    p = Point { x: 0 };\n    p.x = 1;\n    print(p.x as string);\n}\n",
    );
}

#[test]
fn async_sync_ref_call_is_ok() {
    // A borrow lives only for the synchronous call; no ref is declared or
    // captured by the async fn itself.
    compile_ok(
        "func show(s: ref string) {\n    print(s);\n}\n\
         async func greet() {\n    name = \"fleen\";\n    show(name);\n}\n",
    );
}
