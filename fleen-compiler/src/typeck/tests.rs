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
            enums: Vec::new(),
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
fn check_bare_ok_is_not_a_value() {
    // 0.0.10 T02 parity pin: bare `Ok` in expression position (no call)
    // keeps its rejection — the constructors are not values. The text is
    // unchanged from the pre-0.0.10 pipeline behavior
    // (`UndefinedVariable`; the resolver's builtin binding never becomes a
    // typeck value binding).
    let errs = compile_err("x = Ok;");
    assert!(
        errs.iter().any(
            |e| matches!(e.kind, TypeckErrorKind::UndefinedVariable { ref name } if name == "Ok")
        ),
        "expected UndefinedVariable for `Ok`, got {:?}",
        errs
    );
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
    // 0.0.6 T04: the enum verdict mirrors the struct verdict — the cached
    // D1 flag (Copy iff every variant payload is Copy).
    let copy_enum = Type::Enum(Box::new(EnumInfo {
        id: EnumId(0),
        name: "CopyE".into(),
        copy: true,
        type_param_count: 0,
        payloads: Vec::new(),
    }));
    let owned_enum = Type::Enum(Box::new(EnumInfo {
        id: EnumId(1),
        name: "OwnedE".into(),
        copy: false,
        type_param_count: 0,
        payloads: Vec::new(),
    }));
    assert!(copy_enum.is_copy());
    assert!(!copy_enum.is_owned());
    assert!(!owned_enum.is_copy());
    assert!(owned_enum.is_owned());
    // 0.0.11 T03 D2: a map is ALWAYS owned — never Copy, for every `K` / `V`
    // (the `[T]` posture). A Copy-verdict whitelist key does not flip it.
    assert!(!map_ty(Type::Int, Type::Int).is_copy());
    assert!(!map_ty(Type::Bool, Type::String).is_copy());
    assert!(!map_ty(Type::String, Type::Int).is_copy());
}

#[test]
fn is_owned_exhaustive() {
    assert!(Type::String.is_owned());
    assert!(Type::Box(Box::new(Type::Int)).is_owned());
    assert!(Type::Result(Box::new(Type::String), Box::new(Type::Int)).is_owned());
    // 0.0.11 T03 D2: a map is always an owned, move-only value.
    assert!(map_ty(Type::Int, Type::Int).is_owned());
    assert!(map_ty(Type::Bool, Type::String).is_owned());
    assert!(map_ty(Type::String, Type::Int).is_owned());
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
    let TypedExprHir::Call { args, .. } = &**call else {
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
    let fold = find_builtin("std.collection.iter.foldl").expect("iter.foldl must be a builtin");
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

#[test]
fn ref_clone_in_async_is_error() {
    // `clone` of a captured ref takes a separate typeck path; the borrow
    // itself must still be rejected.
    let errs = compile_err(
        "func outer(s: ref string) {\n    async func inner() {\n        \
         t = clone s;\n        print(t);\n    }\n}\n",
    );
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::RefInAsync]);
}

#[test]
fn ref_param_in_nested_sync_inside_async_is_error() {
    // The ref rule is lexical: a sync helper declared inside an async fn is
    // still inside the async world.
    let errs = compile_err(
        "async func outer() {\n    func helper(t: ref string) {\n        \
         print(t);\n    }\n}\n",
    );
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::RefInAsync]);
}

#[test]
fn async_global_write_in_nested_sync_is_error() {
    let errs = compile_err(
        "struct Point { x: int, }\np = Point { x: 0 };\n\
         async func outer() {\n    func helper() {\n        p.x = 1;\n    }\n}\n",
    );
    assert_eq!(
        err_kinds(&errs),
        vec![&TypeckErrorKind::AsyncGlobalWrite {
            name: "p".to_string()
        }]
    );
}

#[test]
fn async_function_does_not_poison_later_sync_functions() {
    // The async flag is scoped to one function: a later sync fn may take a
    // ref and write a global again.
    compile_ok(
        "struct Point { x: int, }\np = Point { x: 0 };\n\
         async func a() { print(\"a\"); }\n\
         func b(s: ref string) { print(s); }\n\
         func c() { p.x = 1; }\n",
    );
}

#[test]
fn async_error_does_not_poison_later_sync_functions() {
    // Same as above on the error path: the stack is popped before `?`, so a
    // failing async fn must not add spurious boundary errors to later items.
    let errs = compile_err(
        "struct Point { x: int, }\np = Point { x: 0 };\n\
         async func a(s: ref string) { print(s); }\n\
         func b(s: ref string) { print(s); }\n\
         func c() { p.x = 1; }\n",
    );
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::RefInAsync]);
}

#[test]
fn async_deref_global_write_uses_existing_error() {
    // `deref g = v` is rejected in every context; keep the older, stronger
    // error instead of double-reporting an async global write.
    let errs = compile_err("g = box 1;\nasync func f() {\n    deref g = 2;\n}\n");
    assert_eq!(
        err_kinds(&errs),
        vec![&TypeckErrorKind::DerefAssignOfGlobalBox]
    );
}

#[test]
fn async_boundary_error_messages_are_actionable() {
    let errs = compile_err("async func f(s: ref string) { print(s); }\n");
    let msg = format!("{}", errs[0].kind);
    assert!(msg.contains("owned"), "unexpected message: {msg}");

    let errs = compile_err(
        "struct Point { x: int, }\np = Point { x: 0 };\n\
         async func g() { p.x = 1; }\n",
    );
    let msg = format!("{}", errs[0].kind);
    assert!(msg.contains("channel"), "unexpected message: {msg}");
}

#[test]
fn ref_channel_param_is_error() {
    // 0.0.4 T07: a channel handle cannot sit behind a `ref` — send/recv park
    // and resume across the scheduler, carrying the borrowed handle with
    // them (D4). Rejected at the declaration, before the T07a member rewrite
    // could produce a misleading `ChannelMemberNotFound`.
    let errs = compile_err("func borrow(c: ref channel<int>) { }\n");
    assert_eq!(
        err_kinds(&errs),
        vec![&TypeckErrorKind::ChannelNotBorrowable]
    );

    // In an async context both boundary rules fire: the borrow itself
    // (`RefInAsync`) and the channel behind it.
    let errs = compile_err("async func f(c: ref channel<string>) { }\n");
    assert_eq!(
        err_kinds(&errs),
        vec![
            &TypeckErrorKind::RefInAsync,
            &TypeckErrorKind::ChannelNotBorrowable,
        ]
    );
}

#[test]
fn move_of_captured_ref_in_async_is_move_of_borrowed() {
    // `move s` of a captured ref keeps its stronger pre-existing error
    // (`MoveOfBorrowed`) instead of `RefInAsync`, in an async context too.
    let errs = compile_err(
        "func outer(s: ref string) {\n    async func inner() {\n        \
         t = move s;\n    }\n}\n",
    );
    assert_eq!(
        err_kinds(&errs),
        vec![&TypeckErrorKind::MoveOfBorrowed {
            name: "s".to_string()
        }]
    );
}

#[test]
fn struct_field_named_send_resolves_as_field() {
    // T07a: `send`/`recv` are only rewritten to channel ops on a
    // `channel<T>` receiver; a struct with a field literally named `send`
    // still resolves through the ordinary field path.
    let hir = compile_ok(
        "struct S { send: int, recv: int }\n\
         s = S { send: 1, recv: 2 };\nn = s.send + s.recv;\n",
    );
    let last = hir.items.last().expect("items");
    match last {
        TypedHirItem::Expr(e) => assert_eq!(e.ty(), Type::Int),
        TypedHirItem::Decl(TypedDeclHir::Var(v)) => assert_eq!(v.init.ty(), Type::Int),
        other => panic!("expected expression item, got {other:?}"),
    }
}

#[test]
fn local_shadowing_a_function_name_resolves_to_the_local() {
    // `lookup_type` must consult the scope chain before the program-wide
    // function-signature map: a parameter or local that shares a name with a
    // user function keeps the local's type. The resolver resolves the ident
    // against its scopes (innermost wins), so typeck checking function names
    // first disagreed with it — and across modules it broke every std-module
    // body with `a`/`b`-style parameters (`std.num.min`'s `a < b` resolved
    // the operand to a root-crate `async func b`'s type).
    let hir = compile_ok("func b(): int = 1\nfunc f(b: bool): bool = b\n");
    let last = hir.items.last().expect("items");
    let TypedHirItem::Decl(TypedDeclHir::Func(f)) = last else {
        panic!("expected function decl, got {last:?}");
    };
    let TypedFuncBodyHir::SingleExpr(tail) = &f.body else {
        panic!("expected single-expression body");
    };
    assert_eq!(tail.ty(), Type::Bool);

    // The same shadowing in `let` position.
    let hir = compile_ok("func g(): int = 1\nfunc h(): int {\n    g = 2;\n    g\n}\n");
    let last = hir.items.last().expect("items");
    let TypedHirItem::Decl(TypedDeclHir::Func(f)) = last else {
        panic!("expected function decl, got {last:?}");
    };
    let TypedFuncBodyHir::Block(body) = &f.body else {
        panic!("expected block body");
    };
    let tail = body.tail_expr.as_ref().expect("tail expression");
    assert_eq!(tail.ty(), Type::Int);

    // A local may also shadow a *builtin* name in value position.
    compile_ok("func f(print: int): int = print\n");
}

// ========== 0.0.5 T02: ptr / addr / deref_ptr (D1/D2) ==========

#[test]
fn infer_addr_yields_ptr_type() {
    let hir = compile_ok("unsafe func f(): ptr<int> {\n    x = 1;\n    addr x\n}\n");
    let last = hir.items.last().expect("items");
    let TypedHirItem::Decl(TypedDeclHir::Func(f)) = last else {
        panic!("expected function decl");
    };
    let TypedFuncBodyHir::Block(body) = &f.body else {
        panic!("expected block body");
    };
    let tail = body.tail_expr.as_ref().expect("tail expression");
    assert_eq!(tail.ty(), Type::Ptr(Box::new(Type::Int)));
}

#[test]
fn infer_deref_ptr_read_yields_pointee() {
    let hir = compile_ok("unsafe func f(p: ptr<int>): int {\n    deref_ptr p\n}\n");
    let last = hir.items.last().expect("items");
    let TypedHirItem::Decl(TypedDeclHir::Func(f)) = last else {
        panic!("expected function decl");
    };
    let TypedFuncBodyHir::Block(body) = &f.body else {
        panic!("expected block body");
    };
    let tail = body.tail_expr.as_ref().expect("tail expression");
    assert_eq!(tail.ty(), Type::Int);
}

#[test]
fn typeck_rejects_clone_of_pointer() {
    let errs = compile_err(
        "unsafe func f(): int {\n    x = 1;\n    p = addr x;\n    q = clone p;\n    deref_ptr q\n}\n",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::CloneOfPointer))
    );
}

#[test]
fn typeck_rejects_ptr_struct_field() {
    let errs = compile_err("struct Holder {\n    p: ptr<int>,\n}\n");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PtrNotStorable))
    );
}

#[test]
fn typeck_rejects_ptr_global() {
    let errs = compile_err(
        "g: ptr<int> = make();\n\ntrusted func make(): ptr<int> {\n    x = 1;\n    addr x\n}\n",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PtrNotStorable))
    );
}

#[test]
fn typeck_rejects_ptr_array_element() {
    let errs = compile_err("unsafe func f(): int {\n    x = 1;\n    a = [addr x];\n    a[0]\n}\n");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PtrNotStorable))
    );
}

#[test]
fn typeck_rejects_ptr_in_box() {
    let errs =
        compile_err("unsafe func f(): int {\n    x = 1;\n    b = box addr x;\n    deref b\n}\n");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PtrNotStorable))
    );
}

#[test]
fn typeck_rejects_deref_ptr_on_box() {
    // `deref` stays box-only and `deref_ptr` stays ptr-only (PLAN §3.2 L2).
    let errs = compile_err("unsafe func f(b: box<int>): int {\n    deref_ptr b\n}\n");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { .. }))
    );
}

#[test]
fn typeck_rejects_deref_ptr_write_mismatch() {
    let errs = compile_err(
        "unsafe func f(): int {\n    x = 1;\n    p = addr x;\n    deref_ptr p = \"no\";\n    0\n}\n",
    );
    assert!(!errs.is_empty(), "write-through type mismatch must fail");
}

#[test]
fn typeck_ptr_is_not_copy_and_moves_explicitly() {
    // D1: `move p` transfers the handle; a bare pass to an owned position
    // demands the explicit move (same rule as channel/future).
    compile_ok(
        "unsafe func g(p: ptr<int>): int {\n    deref_ptr p\n}\n\nunsafe func f(): int {\n    x = 1;\n    p = addr x;\n    g(move p)\n}\n",
    );
    let errs = compile_err(
        "unsafe func g(p: ptr<int>): int {\n    deref_ptr p\n}\n\nunsafe func f(): int {\n    x = 1;\n    p = addr x;\n    g(p)\n}\n",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. }))
    );
}

// ========== 0.0.5 T04: pointer boundary rules (D4) ==========

#[test]
fn ptr_param_in_async_is_error() {
    // D4: a pointer parameter of an async fn is captured into the task's
    // coroutine frame; the referent may not survive the scheduler's frame
    // relocation. Reported once at the declaration, like a ref param.
    let errs = compile_err("async unsafe func task(p: ptr<int>) {\n    q = deref_ptr p;\n}\n");
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::PtrAcrossBoundary]);
}

#[test]
fn ptr_use_in_async_is_error() {
    // A pointer created inside an async body and read after an await is the
    // "captured across await" shape. The lexical rule fires at the use, even
    // though the pointer never leaves the function.
    let errs = compile_err(
        "async func helper(): int = 1\n\n\
         async unsafe func f(): int {\n    x = 1;\n    p = addr x;\n    \
         h = helper() await;\n    deref_ptr p\n}\n",
    );
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::PtrAcrossBoundary]);
}

#[test]
fn ptr_return_in_async_is_error() {
    // The future's result carries a pointer across the await boundary; the
    // referent lived in (or was promoted from) the coroutine frame.
    let errs = compile_err("async unsafe func make(): ptr<int> {\n    x = 1;\n    addr x\n}\n");
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::PtrAcrossBoundary]);
}

#[test]
fn ptr_channel_send_is_error() {
    // A pointer payload never crosses a channel (D4).
    let errs =
        compile_err("func relay(ch: channel<ptr<int>>, p: ptr<int>) {\n    ch.send(p);\n}\n");
    assert_eq!(err_kinds(&errs), vec![&TypeckErrorKind::PtrAcrossBoundary]);
}

#[test]
fn ptr_through_result_container_is_boundary_error() {
    // The rule sees through value-carrying containers, like RefInAsync.
    let errs = compile_err(
        "func relay(ch: channel<Result<ptr<int>, string>>, p: ptr<int>) {\n    \
         r: Result<ptr<int>, string> = Ok(move p);\n    ch.send(move r);\n}\n",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PtrAcrossBoundary))
    );
}

#[test]
fn unsafe_async_call_from_unsafe_is_ok() {
    // D4: `unsafe async func` is legal and may be called/awaited from an
    // unsafe (or trusted) context.
    compile_ok(
        "async unsafe func gen(x: int): int = x + 1\nunsafe func use(): int {\n    \
         gen(1) await\n}\n",
    );
}

#[test]
fn unsafe_async_call_from_safe_is_error() {
    // Calling an unsafe async fn from a plain fn is the same static gate as a
    // sync unsafe fn (T03).
    let errs = compile_err(
        "async unsafe func gen(x: int): int = x + 1\nfunc use(): int {\n    \
         gen(1) await\n}\n",
    );
    assert_eq!(
        err_kinds(&errs),
        vec![&TypeckErrorKind::UnsafeCallOutsideUnsafe {
            func_name: "gen".to_string()
        }]
    );
}

// ---- 0.0.7 T05: recursive pattern typecheck + exhaustiveness ----

#[test]
fn t05_multi_payload_enum_pattern_binds_every_payload() {
    // T05 lifts the T02 staging gate (D4): `when Pair(a, b)` names every
    // payload in declaration order, and each binding carries the type of
    // its payload position — `a` is int, `b` is string.
    compile_ok(
        "enum Pair3 { Pair(int, string), }\n\
         func f(): int {\n    choose Pair(1, \"x\") {\n        when Pair(a, b) { a + 1 }\n    }\n}",
    );
    // Using payload 0's binding as a string is a type error, proving the
    // positional typing landed.
    let errs = compile_err(
        "enum Pair4 { Pair(int, string), }\n\
         func f(): int {\n    choose Pair(1, \"x\") {\n        when Pair(a, b) { a + \"s\" }\n    }\n}",
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::TypeMismatch { .. })),
        "expected payload 0 binding to be int, got {errs:?}"
    );
}

#[test]
fn t05_single_payload_enum_pattern_still_compiles() {
    // The 0.0.6 single-payload form keeps working; a two-payload variant
    // now requires every payload named (D4 strict arity).
    compile_ok(
        "enum Shape2 { Point, Circle(float), Rect(float, float), }\n\
         func f(s: Shape2): int {\n    choose s {\n        when Point { 0 }\n        when Circle(r) { 1 }\n        when Rect(a, b) { 2 }\n    }\n}",
    );
}

#[test]
fn t05_pattern_arity_mismatch_is_reported() {
    // D4: a variant pattern must name exactly as many sub-patterns as the
    // variant declares payloads — the 0.0.6 "first payload only" leniency
    // (`when Rect(a)`) is now `ArityMismatchInPattern`.
    let errs = compile_err(
        "enum Rect1 { Rect(float, float), }\n\
         func f(s: Rect1): int {\n    choose s {\n        when Rect(a) { 0 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ArityMismatchInPattern {
                expected: 2,
                found: 1,
                ..
            }
        )),
        "expected ArityMismatchInPattern 2 vs 1, got {errs:?}"
    );
    // Zero-payload variant given a sub-pattern position is also an arity
    // mismatch (`Point` matches bare — D4).
    let errs = compile_err(
        "enum Shape { Point, Circle(float), }\n\
         func f(s: Shape): int {\n    choose s {\n        when Point(x) { 0 }\n        when Circle(r) { 1 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ArityMismatchInPattern {
                expected: 0,
                found: 1,
                ..
            }
        )),
        "expected ArityMismatchInPattern 0 vs 1 for `Point(x)`, got {errs:?}"
    );
}

#[test]
fn t05_nested_pattern_ok_some_binds_inner_type() {
    // D5: `Ok(Some(x))` on `Result[Option<int>, string]` — the ResultCtor
    // payload is itself a full pattern, and the inner binding `x` takes the
    // *substituted* payload type `int`.
    compile_ok(
        "enum Option<T> { Some(T), None, }\n\
         func f(r: Result[Option<int>, string]): int {\n    choose move r {\n        when Ok(Some(x)) { x }\n        when Ok(None) { 0 - 1 }\n        when Err(e) { 0 - 2 }\n    }\n}",
    );
    // The inner binding is `int`: using it as a string fails.
    let errs = compile_err(
        "enum Option<T> { Some(T), None, }\n\
         func f(r: Result[Option<int>, string]): int {\n    choose move r {\n        when Ok(Some(x)) { x + \"s\" }\n        when Ok(None) { 0 - 1 }\n        when Err(e) { 0 - 2 }\n    }\n}",
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::TypeMismatch { .. })),
        "expected nested binding x to be int, got {errs:?}"
    );
}

#[test]
fn t05_nested_pattern_constructor_mismatch() {
    // D5: the inner pattern's constructor must match the payload type at
    // that position — the payload here is `Option<int>` and `Ok` is a
    // Result constructor, so the nested mismatch is `PatternTypeMismatch`
    // (an inner pattern naming a foreign *enum* variant is
    // `EnumPatternMismatch`; D5 names both diagnostics).
    let errs = compile_err(
        "enum Option<T> { Some(T), None, }\n\
         func f(r: Result[Option<int>, string]): int {\n    choose move r {\n        when Ok(Ok(x)) { x }\n        when Ok(None) { 0 - 1 }\n        when Err(e) { 0 - 2 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::PatternTypeMismatch { pattern: "Ok", .. }
        )),
        "expected PatternTypeMismatch for the nested constructor, got {errs:?}"
    );
    // The enum-variant flavor: `Some(D(a, b))` on an `Option<int>`
    // scrutinee — the inner constructor pattern names a variant, but the
    // payload type `int` is not that enum.
    let errs = compile_err(
        "enum Option<T> { Some(T), None, }\n\
         enum Dup { D(int, int), }\n\
         func f(o: Option<int>): int {\n    choose o {\n        when Some(D(a, b)) { 0 }\n        when None { 1 }\n    }\n}",
    );
    assert!(
        errs.iter()
            .any(|e| matches!(&e.kind, TypeckErrorKind::EnumPatternMismatch { .. })),
        "expected EnumPatternMismatch for the foreign inner variant, got {errs:?}"
    );
}

#[test]
fn t05_multi_binding_pattern_payloads_are_positional() {
    // D4: `when Pair(a, b)` binds both payloads in declaration order; the
    // second binding is the `string` payload.
    compile_ok(
        "enum Pair5 { Pair(int, string), }\n\
         func f(): int {\n    choose Pair(7, \"x\") {\n        when Pair(a, b) { a + 1 }\n    }\n}",
    );
    let errs = compile_err(
        "enum Pair6 { Pair(int, string), }\n\
         func f(): int {\n    choose Pair(7, \"x\") {\n        when Pair(a, b) { b + 1 }\n    }\n}",
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::TypeMismatch { .. })),
        "expected payload 1 binding to be string, got {errs:?}"
    );
}

#[test]
fn t05_or_pattern_same_binding_types_is_ok() {
    // D6: `Some(v) | Other(v)` — both alternatives bind `v` with the same
    // type; the arm body may use it.
    compile_ok(
        "enum Two { Some(int), Other(int), None2, }\n\
         func f(t: Two): int {\n    choose t {\n        when Some(v) | Other(v) { v }\n        when None2 { 0 - 1 }\n    }\n}",
    );
    // A literal or-pattern is also legal (it just never helps enum
    // exhaustiveness on its own).
    compile_ok(
        "func f(x: int): int {\n    choose x {\n        when 1 | 2 { 0 }\n        otherwise { 1 }\n    }\n}",
    );
}

#[test]
fn t05_or_pattern_missing_binding_is_mismatch() {
    // D6: `Some(v) | None` — the second alternative does not bind `v`;
    // reported as `OrPatternBindingMismatch` (missing side shows as unit).
    let errs = compile_err(
        "enum Opt { Some(int), None2, }\n\
         func f(o: Opt): int {\n    choose o {\n        when Some(v) | None2 { v }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::OrPatternBindingMismatch { name, .. } if name == "v"
        )),
        "expected OrPatternBindingMismatch for v, got {errs:?}"
    );
}

#[test]
fn t05_or_pattern_binding_type_mismatch() {
    // D6: same names but different types across alternatives is also a
    // mismatch (`Some(v)` binds int, `Other(v)` binds string).
    let errs = compile_err(
        "enum Two2 { Some(int), Other(string), None2, }\n\
         func f(t: Two2): int {\n    choose move t {\n        when Some(v) | Other(v) { 0 }\n        when None2 { 1 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::OrPatternBindingMismatch { name, .. } if name == "v"
        )),
        "expected OrPatternBindingMismatch for v, got {errs:?}"
    );
}

#[test]
fn t05_or_pattern_missing_binding_in_first_alternative_is_mismatch() {
    // D6, symmetric direction: the binding is missing from the FIRST
    // alternative — the mismatch must still be reported (the check walks
    // every alternative against the first in both directions).
    let errs = compile_err(
        "enum Opt2b { Some(int), None2, }\n\
         func f(o: Opt2b): int {\n    choose o {\n        when None2 | Some(v) { v }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::OrPatternBindingMismatch { name, .. } if name == "v"
        )),
        "expected OrPatternBindingMismatch for v, got {errs:?}"
    );
}

#[test]
fn t05_or_pattern_union_covers_exhaustively() {
    // F7: an or-pattern covers the union of its alternatives' variants —
    // `Some(v) | Other(v)` + `None2` covers all three, so no `otherwise`.
    // (Every alternative must bind the same names — D6 — so the union arm
    // cannot pair `Some(v)` with the binding-free `None2`.)
    compile_ok(
        "enum Two3 { Some(int), Other(int), None2, }\n\
         func f(t: Two3): int {\n    choose t {\n        when Some(v) | Other(v) { v }\n        when None2 { 0 }\n    }\n}",
    );
    // But a partial union still names what is missing: `Some(v) | Other(v)`
    // alone leaves `None2` uncovered.
    let errs = compile_err(
        "enum Two4 { Some(int), Other(int), None2, }\n\
         func f(t: Two4): int {\n    choose t {\n        when Some(v) | Other(v) { v }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns == &["None2".to_string()]
        )),
        "expected ChooseNotExhaustive missing None2, got {errs:?}"
    );
}

#[test]
fn t05_type_guard_on_ident_binding() {
    // D7: `when v: int` — a typed binding; the ascription must equal the
    // scrutinee type and the binding carries that type.
    compile_ok(
        "func f(x: int): int {\n    choose x {\n        when v: int { v }\n        otherwise { 0 - 1 }\n    }\n}",
    );
    // A type guard ascribing the wrong type is a compile-time mismatch —
    // no runtime type test exists (D7).
    let errs = compile_err(
        "func f(x: int): int {\n    choose x {\n        when v: string { 0 }\n        otherwise { 1 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::PatternTypeMismatch {
                pattern: "type-guard",
                ..
            }
        )),
        "expected PatternTypeMismatch for the wrong ascription, got {errs:?}"
    );
}

#[test]
fn t05_type_guard_on_constructor_pattern() {
    // D7: an ascription on a constructor pattern must name the scrutinee's
    // type (mostly a documentation / exhaustiveness aid).
    compile_ok(
        "enum Opt2 { Some(int), None2, }\n\
         func f(o: Opt2): int {\n    choose o {\n        when Some(v): Opt2 { v }\n        when None2 { 0 - 1 }\n    }\n}",
    );
    let errs = compile_err(
        "enum Opt3 { Some(int), None2, }\n\
         func f(o: Opt3): int {\n    choose o {\n        when Some(v): int { 0 }\n        when None2 { 1 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::PatternTypeMismatch {
                pattern: "type-guard",
                ..
            }
        )),
        "expected PatternTypeMismatch for the constructor ascription, got {errs:?}"
    );
}

#[test]
fn t05_result_or_pattern_covers_one_side() {
    // F7 on Result: ident-payload alternatives cover the Ok side; Err is
    // still missing.
    let errs = compile_err(
        "func f(r: Result[int, string]): int {\n    choose move r {\n        when Ok(v) | Ok(v) { v }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns == &["Err".to_string()]
        )),
        "expected ChooseNotExhaustive missing Err, got {errs:?}"
    );
}

// ---- 0.0.7 T06: payload-aware exhaustiveness ----

#[test]
fn t06_literal_payload_arm_does_not_cover_variant() {
    // Payload-aware exhaustiveness: `P(1, x)` matches one value of the
    // payload, so `P` is not covered — a `P(2, 9)` value would otherwise
    // fall into lower's defensive tail at runtime (the crash this
    // tightening prevents).
    let errs = compile_err(
        "enum Pair { P(int, int), Single(int), }\n\
         func f(p: Pair): int {\n    choose p {\n        when P(1, x) { x }\n        when Single(x) { x }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns == &["P".to_string()]
        )),
        "expected ChooseNotExhaustive missing P, got {errs:?}"
    );
}

#[test]
fn t06_result_literal_or_alternatives_do_not_cover_ok() {
    // `Ok(1) | Ok(2)` no longer covers the Ok side — `Ok(3)` would fall
    // through — while ident payloads (`Ok(v)`) still do.
    let errs = compile_err(
        "func f(r: Result[int, string]): int {\n    choose move r {\n        when Ok(1) | Ok(2) { 0 }\n        when Err(e) { 0 - 1 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns == &["Ok".to_string()]
        )),
        "expected ChooseNotExhaustive missing Ok, got {errs:?}"
    );
}

#[test]
fn t06_nested_ctor_payloads_cover_ctor_side_jointly() {
    // `Ok(Some(x))` + `Ok(None2)` jointly cover the Ok side: coverage is
    // per-variant across the group of guard-free Ok-arm payloads.
    compile_ok(
        "enum Opt { Some(int), None2, }\n\
         func f(r: Result<Opt, int>): int {\n    choose r {\n        when Ok(Some(x)) { x }\n        when Ok(None2) { 0 }\n        when Err(e) { e }\n    }\n}",
    );
}

#[test]
fn t06_nested_ctor_payload_partially_covered_is_not_exhaustive() {
    // `Ok(Some(x))` alone leaves the `None2` side of the Ok payload
    // uncovered, so the Result scrutinee is not exhaustive without it.
    let errs = compile_err(
        "enum Opt { Some(int), None2, }\n\
         func f(r: Result<Opt, int>): int {\n    choose r {\n        when Ok(Some(x)) { x }\n        when Err(e) { e }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns == &["Ok".to_string()]
        )),
        "expected ChooseNotExhaustive missing Ok, got {errs:?}"
    );
}

// ---- 0.0.6 T05: choose on enums + exhaustiveness (D3) ----

#[test]
fn t05_choose_enum_exhaustive_is_ok() {
    // Guard-free arms covering every variant are exhaustive; no `otherwise`.
    compile_ok(
        "enum Shape { Point, Circle(float), Rect(float, float), }\n\
         func f(s: Shape): int {\n    choose s {\n        when Point { 0 }\n        when Circle(r) { 1 }\n        when Rect(a, b) { 2 }\n    }\n}",
    );
}

#[test]
fn t05_choose_enum_missing_variant_names_it() {
    let errs = compile_err(
        "enum Color { Red, Green, Blue, }\n\
         func f(c: Color): int {\n    choose c {\n        when Red { 0 }\n        when Green { 1 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns == &["Blue"]
        )),
        "expected ChooseNotExhaustive missing Blue, got {errs:?}"
    );
}

#[test]
fn t05_choose_enum_guarded_only_arm_is_not_exhaustive() {
    // A guarded arm never counts (same rule as Bool/Result).
    let errs = compile_err(
        "enum Color { Red, Green, }\n\
         func f(c: Color): int {\n    choose c {\n        when Red if true { 0 }\n        when Green { 1 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns == &["Red"]
        )),
        "expected guarded-only Red to be reported missing, got {errs:?}"
    );
}

#[test]
fn t05_choose_enum_otherwise_short_circuits_exhaustiveness() {
    compile_ok(
        "enum Color { Red, Green, Blue, }\n\
         func f(c: Color): int {\n    choose c {\n        when Red { 0 }\n        otherwise { 1 }\n    }\n}",
    );
}

#[test]
fn t05_enum_pattern_payload_binding_takes_declared_type() {
    // `when Circle(r)` binds r: float (index-0 payload); using it as an int
    // must fail to type-check, proving the binding is typed and moved.
    let errs = compile_err(
        "enum Shape { Point, Circle(float), }\n\
         func f(s: Shape): int {\n    choose s {\n        when Point { 0 }\n        when Circle(r) { r + 1 }\n    }\n}",
    );
    assert!(
        !errs.is_empty(),
        "expected a float-vs-int type error, got none"
    );
}

#[test]
fn t05_enum_pattern_mismatch_on_foreign_variant() {
    // `when Blue(x)` on a Color scrutinee that does not declare Blue.
    let errs = compile_err(
        "enum Color { Red, Green, }\n\
         func f(c: Color): int {\n    choose c {\n        when Red { 0 }\n        when Green { 1 }\n        when Blue(x) { 2 }\n    }\n}",
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::EnumPatternMismatch { .. })),
        "expected EnumPatternMismatch, got {errs:?}"
    );
}

#[test]
fn t05_enum_zero_payload_variant_with_binding_is_mismatch() {
    // `Point` declares no payload, so `when Point(x)` has nothing to bind:
    // 0.0.7 D4 strict arity reports `ArityMismatchInPattern` (0 vs 1), which
    // supersedes the 0.0.6 `EnumPatternMismatch` for this shape.
    let errs = compile_err(
        "enum Shape { Point, Circle(float), }\n\
         func f(s: Shape): int {\n    choose s {\n        when Point(x) { 0 }\n        when Circle(r) { 1 }\n    }\n}",
    );
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            TypeckErrorKind::ArityMismatchInPattern {
                expected: 0,
                found: 1,
                ..
            }
        )),
        "expected ArityMismatchInPattern for a zero-payload variant binding, got {errs:?}"
    );
}

#[test]
// 0.0.7 T04/D1: a fully applied parameterized use site now resolves —
// superseding the T01 placeholder that reported "parameterized types not
// yet resolved". `Option<int>` is a legal type; a bare `Option` is not.
fn parameterized_type_use_site_resolves_t04() {
    compile_ok("enum Option<T> { Some(T), None }\nfunc f(x: Option<int>): int { 0 }");
}

#[test]
fn parameterized_type_use_site_bracket_resolves_t04() {
    compile_ok("enum Option<T> { Some(T), None }\nfunc f(x: Option[int]): int { 0 }");
}

// ---- 0.0.7 T04: parameterized types + argument-dependent is_copy ----

fn body_stmts(body: &TypedFuncBodyHir) -> &[TypedStmtHir] {
    match body {
        TypedFuncBodyHir::Block(b) => &b.stmts,
        TypedFuncBodyHir::SingleExpr(_) => &[],
    }
}

/// Helper: the type of the last local binding of `main`.
fn last_binding_ty(hir: &TypedHir) -> Type {
    let mut found: Option<Type> = None;
    for item in &hir.items {
        let TypedHirItem::Decl(TypedDeclHir::Func(f)) = item else {
            continue;
        };
        for stmt in body_stmts(&f.body) {
            if let TypedStmtHir::Decl(TypedDeclHir::Var(binding)) = stmt {
                found = Some(
                    binding
                        .ty
                        .clone()
                        .unwrap_or_else(|| binding.init.ty().clone()),
                );
            }
        }
    }
    found.unwrap_or_else(|| panic!("no binding found in {hir:?}"))
}

#[test]
fn param_option_int_is_copy_flows_free_t04() {
    // D2: `Option<int>` is Copy — an owned binding flows free with no `move`.
    let hir = compile_ok(
        "enum Option<T> { Some(T), None, }\n\
         func f(): int {\n    o = Option.Some(1);\n    0\n}",
    );
    assert!(
        last_binding_ty(&hir).is_copy(),
        "Option<int> should be Copy: {:?}",
        last_binding_ty(&hir)
    );
}

#[test]
fn param_option_string_is_owned_needs_move_t04() {
    // D2: `Option<string>` is owned (string payload) — the use site demands
    // `move` in a consuming position.
    let errs = compile_err(
        "enum Option<T> { Some(T), None, }\n\
         func use_(x: Option<string>): int = 0\n\
         func f(): int {\n    o = Option.Some(\"a\");\n    use_(o)\n}",
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected use-of-moved, got {errs:?}"
    );
}

#[test]
fn param_pair_two_args_owned_t04() {
    let hir = compile_ok(
        "enum Pair<A, B> { P(A, B), }\n\
         func f(): int {\n    p = Pair.P(\"a\", 2);\n    0\n}",
    );
    assert!(
        !last_binding_ty(&hir).is_copy(),
        "Pair[string, int] should be owned: {:?}",
        last_binding_ty(&hir)
    );
}

#[test]
fn param_nested_option_pair_t04() {
    // Nested applications substitute at every depth:
    // `Option<Pair[int, string>>` is owned because Pair carries a string.
    let hir = compile_ok(
        "enum Option<T> { Some(T), None, }\n\
         enum Pair<A, B> { P(A, B), }\n\
         func f(): int {\n    o = Option.Some(Pair.P(1, \"a\"));\n    0\n}",
    );
    assert!(
        !last_binding_ty(&hir).is_copy(),
        "nested owned arg should be owned: {:?}",
        last_binding_ty(&hir)
    );
}

#[test]
fn param_ctor_arg_fixes_param_t04() {
    // `Option.Some(1)` fixes `T` from the argument.
    let hir = compile_ok(
        "enum Option<T> { Some(T), None, }\n\
         func f(): int {\n    o = Option.Some(1);\n    0\n}",
    );
    assert!(
        last_binding_ty(&hir).is_copy(),
        "inferred Option<int> should be Copy: {:?}",
        last_binding_ty(&hir)
    );
}

#[test]
fn param_bare_use_reports_needs_args_t04() {
    let errs = compile_err("enum Option<T> { Some(T), None, }\nfunc f(x: Option): int = 0");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::TypeConstructorNeedsArgs { .. })),
        "expected needs-args, got {errs:?}"
    );
}

#[test]
fn param_too_many_args_t04() {
    let errs =
        compile_err("enum Pair<A, B> { P(A, B), }\nfunc f(x: Pair<int, string, bool>): int = 0");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::ArityMismatch { .. })),
        "expected arity mismatch, got {errs:?}"
    );
}

#[test]
fn param_too_few_args_t04() {
    let errs = compile_err("enum Pair<A, B> { P(A, B), }\nfunc f(x: Pair<int>): int = 0");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            TypeckErrorKind::TypeConstructorNeedsArgs {
                expected: 2,
                found: 1,
                ..
            }
        )),
        "expected needs-args 2 found 1, got {errs:?}"
    );
}

#[test]
fn param_recursive_list_rejected_t04() {
    // D3 carries forward: `List<T>` naming itself through its parameter is
    // still a recursive enum.
    let errs = compile_err("enum List<T> { Cons(T, List<T>), Nil, }");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::RecursiveEnum { .. })),
        "expected recursive enum, got {errs:?}"
    );
}

#[test]
fn param_ptr_payload_rejected_t04() {
    let errs =
        compile_err("enum Option<T> { Some(T), None, }\nfunc f(x: Option<ptr<int>>): int = 0");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::PtrNotStorable)),
        "expected ptr-not-storable, got {errs:?}"
    );
}

#[test]
fn param_ambiguous_param_reports_t04() {
    // `None` carries no payload, so `T` is fixed only by the annotation —
    // absent it, the parameter is ambiguous.
    let errs = compile_err(
        "enum Option<T> { Some(T), None, }\n\
         func f(): int {\n    x = Option.None;\n    0\n}",
    );
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, TypeckErrorKind::TypeParameterAmbiguous { .. })),
        "expected ambiguous param, got {errs:?}"
    );
}

#[test]
fn param_annotation_resolves_ambiguous_none_t04() {
    compile_ok(
        "enum Option<T> { Some(T), None, }\n\
         func f(): int {\n    y: Option<int> = Option.None;\n    0\n}",
    );
}

#[test]
fn func_type_name_with_curry_spelling_renders_chains_t03() {
    // 0.0.8 T03 (D4): `name_with(true)` renders function types as
    // right-associative arrow chains; `name_with(false)` is `name()` — the
    // tuple spelling every existing error text uses.
    use crate::typeck::typed_hir::Type as T;
    let int = || T::Int;
    let two_arg = T::Func(vec![int(), int()], Box::new(int()));
    assert_eq!(two_arg.name_with(true), "int -> int -> int");
    assert_eq!(two_arg.name_with(false), two_arg.name());

    let zero_arg = T::Func(Vec::new(), Box::new(int()));
    assert_eq!(zero_arg.name_with(true), "() -> int");

    let higher_order = T::Func(vec![T::Func(vec![int()], Box::new(int()))], Box::new(int()));
    assert_eq!(higher_order.name_with(true), "(int -> int) -> int");
}

// ========== 0.0.8 T05: implicit partial application (D5–D12) ==========

use crate::typeck::typeck_with_features;

/// Helper: compile source to TypedHir with the file's own `#feature` table
/// (the same wiring `Session::compile` uses; a single file is the whole
/// table, `FileId(0)` per the single-file convention).
fn compile_to_typed_flagged(source: &str) -> Result<TypedHir, Vec<TypeckError>> {
    let tokens = tokenize(source).unwrap();
    let ast = parse(tokens).unwrap();
    let features = crate::FeatureTable::from_asts(std::slice::from_ref(&ast));
    let hir = resolve(ast).unwrap();
    typeck_with_features(hir, &features)
}

fn compile_ok_flagged(source: &str) -> TypedHir {
    compile_to_typed_flagged(source).expect("typeck should succeed")
}

fn compile_err_flagged(source: &str) -> Vec<TypeckError> {
    compile_to_typed_flagged(source).expect_err("typeck should fail")
}

/// The flag-on declarations shared by the partial tests: a two-part
/// `add: int -> int -> int`.
const ADD_DECL: &str = "add: int -> int -> int\nadd(a, b) { a + b }\n";

/// A top-level partial application is itself a discarded value (D10 applies
/// to the top-level statement list too), so the valid tests observe the
/// partial's type through an annotated binding — which simultaneously pins
/// D6's exact-match rule at a use site.
fn binding_init(hir: &TypedHir) -> &TypedExprHir {
    match hir.items.last().expect("at least one item") {
        TypedHirItem::Decl(TypedDeclHir::Var(v)) => &v.init,
        other => panic!("expected a top-level binding, got {other:?}"),
    }
}

#[test]
fn partial_of_two_param_function_has_the_remaining_type() {
    // D5: `add(1)` over `add: int -> int -> int` is `int -> int` — literally
    // the rest of the signature — and the node carries the partial marker.
    let hir = compile_ok_flagged(&format!(
        "#feature curry\n{ADD_DECL}x: int -> int = add(1);\n"
    ));
    match binding_init(&hir) {
        TypedExprHir::Call { ty, partial, .. } => {
            assert_eq!(*ty, Type::Func(vec![Type::Int], Box::new(Type::Int)));
            let captured = partial.as_ref().expect("call must be marked partial");
            assert_eq!(captured, &vec![Type::Int]);
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

#[test]
fn partial_of_function_value_callee_has_the_remaining_type() {
    // The one-code-path claim: an indirect (function-value) callee
    // partializes by the same D5 rule — participation is type-level only
    // (D9's documented hole: `unsafe`/`trusted` is unknowable for a value).
    let hir = compile_ok_flagged(&format!(
        "#feature curry\n{ADD_DECL}f: int -> int -> int = add;\nx: int -> int = f(1);\n"
    ));
    match binding_init(&hir) {
        TypedExprHir::Call { ty, partial, .. } => {
            assert_eq!(*ty, Type::Func(vec![Type::Int], Box::new(Type::Int)));
            let captured = partial.as_ref().expect("call must be marked partial");
            assert_eq!(captured, &vec![Type::Int]);
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

#[test]
fn full_call_type_and_marker_unchanged_under_flag() {
    // D5: `k = n` is a full call — same type, no marker.
    let hir = compile_ok_flagged(&format!("#feature curry\n{ADD_DECL}x: int = add(1, 2);\n"));
    match binding_init(&hir) {
        TypedExprHir::Call { ty, partial, .. } => {
            assert_eq!(*ty, Type::Int);
            assert!(partial.is_none(), "a full call must not carry the marker");
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

#[test]
fn degenerate_zero_arg_partial_yields_the_function_itself() {
    // D5: `k = 0` is the degenerate partial — uniformity, no special case.
    let hir = compile_ok_flagged(&format!(
        "#feature curry\n{ADD_DECL}x: int -> int -> int = add();\n"
    ));
    match binding_init(&hir) {
        TypedExprHir::Call { ty, partial, .. } => {
            assert_eq!(
                *ty,
                Type::Func(vec![Type::Int, Type::Int], Box::new(Type::Int))
            );
            let captured = partial.as_ref().expect("degenerate partial is partial");
            assert!(captured.is_empty());
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

#[test]
fn partial_of_fixed_builtin_is_the_remaining_function() {
    // D5/D8: `range_from(0)` — a Fixed table builtin — is the remaining
    // `int -> Range` function (T06 reroutes its lowering).
    let hir = compile_ok_flagged("#feature curry\nr = range_from(0);\n");
    match binding_init(&hir) {
        TypedExprHir::Call { ty, partial, .. } => {
            assert_eq!(*ty, Type::Func(vec![Type::Int], Box::new(Type::Range)));
            assert!(partial.is_some());
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

// Note: a partial of a *schematic* builtin (`std.arr.get(arr)` binding `T0`
// from the applied argument) takes the same rule through
// `resolve_schematic_partial`; it needs the import machinery, so it is
// pinned end-to-end by T09's cross-flag fixtures rather than this
// single-file harness.

#[test]
fn async_partial_keeps_the_future_at_the_final_call() {
    // D12: `half(5)` is `int -> Future<Result[int, string]>` — the future
    // exists only at the final call; no new boundary rule. The body line
    // carries no modifier (T02/D3) — the signature holds it.
    let hir = compile_ok_flagged(
        "#feature curry\nasync half: int -> int -> Result[int, string]\nhalf(n, d) {\n    if d == 0 { Err(\"zero\") } else { Ok(n / d) }\n}\nr = half(5);\n",
    );
    match binding_init(&hir) {
        TypedExprHir::Call { ty, partial, .. } => {
            assert_eq!(
                *ty,
                Type::Func(
                    vec![Type::Int],
                    Box::new(Type::Future(Box::new(Type::Result(
                        Box::new(Type::Int),
                        Box::new(Type::String)
                    ))))
                )
            );
            assert!(partial.is_some());
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

#[test]
fn partial_flows_into_a_higher_order_parameter_and_is_called_there() {
    // D6/D15: a partial value is an ordinary function value — it satisfies
    // an exact-match `int -> int` parameter and is called at its remaining
    // arity (the flag-blind cross-file shape, pinned end-to-end by T09).
    let hir = compile_ok_flagged(&format!(
        "#feature curry\n{ADD_DECL}apply1: (int -> int, int) -> int\napply1(f, x) {{ f(x) }}\nx: int = apply1(add(1), 10);\n"
    ));
    assert_eq!(binding_init(&hir).ty(), Type::Int);
}

#[test]
fn flagoff_too_few_args_keeps_the_arity_error() {
    // D5's lock: the relaxation replaces the error *in flag-on files only*.
    let errs = compile_err("func add(a: int, b: int): int = a + b\n\nadd(1)\n");
    assert!(
        err_kinds(&errs).iter().any(|k| matches!(
            k,
            TypeckErrorKind::ArityMismatch {
                expected: 2,
                found: 1
            }
        )),
        "expected ArityMismatch, got {errs:?}"
    );
}

#[test]
fn too_many_args_stays_arity_mismatch_under_flag() {
    // D5: `k > n` is never a partial.
    let errs = compile_err_flagged(&format!("#feature curry\n{ADD_DECL}add(1, 2, 3)\n"));
    assert!(
        err_kinds(&errs).iter().any(|k| matches!(
            k,
            TypeckErrorKind::ArityMismatch {
                expected: 2,
                found: 3
            }
        )),
        "expected ArityMismatch, got {errs:?}"
    );
}

#[test]
fn partial_of_trusted_function_is_rejected() {
    // D9: `trusted` functions are exempt — the short-arity call keeps
    // today's `ArityMismatch` (the static unsafe-context check stays total).
    let errs = compile_err_flagged(
        "#feature curry\ntrusted add: int -> int -> int\nadd(a, b) { a + b }\nmain: () -> int\nmain() { add(1); 0 }\n",
    );
    assert!(
        err_kinds(&errs).iter().any(|k| matches!(
            k,
            TypeckErrorKind::ArityMismatch {
                expected: 2,
                found: 1
            }
        )),
        "expected ArityMismatch, got {errs:?}"
    );
}

#[test]
fn partial_of_ref_param_function_is_rejected() {
    // D9: a partially-applied borrow would have to outlive arbitrary
    // control flow — `shout()` (k = 0 < n = 1) keeps the arity error.
    let errs = compile_err_flagged(
        "#feature curry\nshout: (ref string) -> int\nshout(s) { 42 }\nmain: () -> int\nmain() { shout(); 0 }\n",
    );
    assert!(
        err_kinds(&errs).iter().any(|k| matches!(
            k,
            TypeckErrorKind::ArityMismatch {
                expected: 1,
                found: 0
            }
        )),
        "expected ArityMismatch, got {errs:?}"
    );
}

#[test]
fn partial_of_ptr_param_function_is_rejected() {
    // D9: the capture list would be a new pointer storage location —
    // `PtrNotStorable`'s ban list grows it. The arity error fires before
    // the arguments are typed, so the argument here is a plain int.
    let errs = compile_err_flagged(
        "#feature curry\nread_at: ptr<int> -> int -> int\nread_at(p, i) { 42 }\nread_at(1)\n",
    );
    assert!(
        err_kinds(&errs).iter().any(|k| matches!(
            k,
            TypeckErrorKind::ArityMismatch {
                expected: 2,
                found: 1
            }
        )),
        "expected ArityMismatch, got {errs:?}"
    );
}

#[test]
fn discarded_partial_is_an_error() {
    // D10: the anti-typo guard — `add(1);` as a statement is the classic
    // currying typo and is rejected.
    let errs = compile_err_flagged(&format!(
        "#feature curry\n{ADD_DECL}main: () -> int\nmain() {{\n    add(1);\n    0\n}}\n"
    ));
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PartialApplicationDiscarded { .. })),
        "expected PartialApplicationDiscarded, got {errs:?}"
    );
}

#[test]
fn discarded_partial_error_renders_the_arrow_spelling() {
    // T05's rendering wire-up: in a flag-on file the error's function type
    // is drawn as an arrow chain (T03's renderer); flag-off text is
    // byte-identical to the pre-0.0.8 output.
    let errs = compile_err_flagged(&format!(
        "#feature curry\n{ADD_DECL}main: () -> int\nmain() {{\n    add(1);\n    0\n}}\n"
    ));
    let err = errs
        .iter()
        .find(|e| matches!(e.kind, TypeckErrorKind::PartialApplicationDiscarded { .. }))
        .expect("PartialApplicationDiscarded");
    assert!(err.curry, "the error records the file's flag");
    assert!(err.to_string().contains("int -> int"), "got: {err}");
}

#[test]
fn genuine_function_value_statement_stays_legal() {
    // D10's negative: only the *marker* decides — a genuine unary function
    // value as a statement is as legal as it was before 0.0.8.
    let hir = compile_ok_flagged(&format!(
        "#feature curry\n{ADD_DECL}main: () -> int\nmain() {{\n    f = add(1);\n    f;\n    0\n}}\n"
    ));
    let Some(TypedHirItem::Decl(TypedDeclHir::Func(f))) = hir.items.last() else {
        panic!("expected the main function item");
    };
    assert_eq!(f.ret_type, Some(Type::Int));
}

#[test]
fn owned_arg_without_move_still_rejected_for_a_partial() {
    // D5: argument ownership is the full-call rule set — a bare owned
    // local read into an owned parameter needs `move`/`clone`, partial
    // application or not.
    let errs = compile_err_flagged(
        "#feature curry\nwrap: string -> string -> string\nwrap(a, b) { a }\nmain: () -> string\nmain() {\n    s = \"hi\";\n    wrap(s, \"x\")\n}\n",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
}

#[test]
fn variadic_print_is_never_a_partial() {
    // D7: a variadic's arity is unrestricted, so every call is complete —
    // `print("a")` under the flag is `unit`, no marker.
    let hir = compile_ok_flagged("#feature curry\nu: unit = print(\"a\");\n");
    match binding_init(&hir) {
        TypedExprHir::Call { ty, partial, .. } => {
            assert_eq!(*ty, Type::Unit);
            assert!(
                partial.is_none(),
                "a variadic call never carries the marker"
            );
        }
        other => panic!("expected a call, got {other:?}"),
    }
}

// ========== 0.0.9 T03: `break` / `continue` loop context (D4) ==========

#[test]
fn break_in_while_body_is_legal() {
    let _ = compile_ok(
        "func f(): int { \
         x = 0; \
         while x < 10 { x = x + 1; if x > 5 { break; } }; \
         x \
         }",
    );
}

#[test]
fn break_in_for_over_array_is_legal() {
    let _ = compile_ok(
        "func f(): int { \
         total = 0; \
         for v in [1, 2, 3, 4] { if v == 3 { break; }; total = total + v; }; \
         total \
         }",
    );
}

#[test]
fn continue_in_for_over_range_is_legal() {
    let _ = compile_ok(
        "import std.collection.range; \
         func f(): int { \
         total = 0; \
         for i in range_from(0, 10) { if i % 2 == 0 { continue; }; total = total + i; }; \
         total \
         }",
    );
}

#[test]
fn continue_in_while_body_is_legal() {
    let _ = compile_ok(
        "func f(): int { \
         x = 0; seen = 0; \
         while x < 10 { x = x + 1; if x % 2 == 0 { continue; }; seen = seen + 1; }; \
         seen \
         }",
    );
}

#[test]
fn break_binds_innermost_of_nested_loops() {
    let _ = compile_ok(
        "func f(): int { \
         x = 0; \
         while x < 3 { \
         y = 0; \
         while y < 3 { y = y + 1; if y == 2 { break; } }; \
         x = x + 1; \
         }; \
         x \
         }",
    );
}

#[test]
fn break_inside_loop_inside_choose_arm_is_legal() {
    // D4 blocks the `choose`-arm boundary only for jumps that would cross
    // it: a loop *inside* the arm re-establishes the loop context, so its
    // own `break` binds to that loop.
    let _ = compile_ok(
        "func f(n: int): int { \
         x = 0; \
         while x < n { \
         y = choose x { when 1 { while true { break; }; 0 } otherwise { 2 } }; \
         x = x + 1; \
         }; \
         x \
         }",
    );
}

#[test]
fn break_outside_loop_at_top_level_is_rejected() {
    let errs = compile_err("break;");
    assert_eq!(errs.len(), 1, "got {errs:?}");
    assert!(matches!(
        err_kinds(&errs)[0],
        TypeckErrorKind::BreakOutsideLoop
    ));
}

#[test]
fn continue_outside_loop_in_function_is_rejected() {
    let errs = compile_err("func f(): int { continue; 0 }");
    assert_eq!(errs.len(), 1, "got {errs:?}");
    assert!(matches!(
        err_kinds(&errs)[0],
        TypeckErrorKind::ContinueOutsideLoop
    ));
}

#[test]
fn break_in_choose_arm_is_rejected_even_inside_a_loop() {
    // D4: a `choose` arm is a boundary — the jump may not cross it to the
    // enclosing loop, so the arm's `break` reports outside-of-loop despite
    // the `while` around the `choose`.
    let errs = compile_err(
        "func f(n: int): int { \
         x = 0; \
         while x < n { \
         y = choose x { when 1 { break; } otherwise { 2 } }; \
         x = x + 1; \
         }; \
         x \
         }",
    );
    // The jump is a *statement* in the arm block, so the arm block still
    // types `Unit` and the arm unification reports its own mismatch — the
    // jump error does not abort the choose.
    assert_eq!(errs.len(), 2, "got {errs:?}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::BreakOutsideLoop)),
        "got {errs:?}"
    );
}

#[test]
fn continue_in_choose_arm_is_rejected_even_inside_a_loop() {
    let errs = compile_err(
        "func f(n: int): int { \
         x = 0; \
         while x < n { \
         y = choose x { when 1 { continue; } otherwise { 2 } }; \
         x = x + 1; \
         }; \
         x \
         }",
    );
    assert_eq!(errs.len(), 2, "got {errs:?}");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ContinueOutsideLoop)),
        "got {errs:?}"
    );
}

#[test]
fn break_in_nested_function_is_rejected() {
    // D4's lexical containment: the nested declaration is a function
    // boundary — its `break` sees no loop even though the declaration sits
    // inside the outer `while` body.
    let errs = compile_err(
        "func outer(): int { \
         x = 0; \
         while x < 3 { \
         func inner(): int { break; 0 } \
         x = x + 1; \
         }; \
         x \
         }",
    );
    assert_eq!(errs.len(), 1, "got {errs:?}");
    assert!(matches!(
        err_kinds(&errs)[0],
        TypeckErrorKind::BreakOutsideLoop
    ));
}

#[test]
fn break_in_nested_async_function_is_rejected() {
    // Same boundary for `async func` (D4 covers the async case with the
    // same lexical rule).
    let errs = compile_err(
        "func outer(): int { \
         x = 0; \
         while x < 3 { \
         async func inner(): int { break; 0 } \
         x = x + 1; \
         }; \
         x \
         }",
    );
    assert_eq!(errs.len(), 1, "got {errs:?}");
    assert!(matches!(
        err_kinds(&errs)[0],
        TypeckErrorKind::BreakOutsideLoop
    ));
}

#[test]
fn loop_depth_is_not_leaked_across_functions() {
    // Errors are collected, not fail-fast: the first function's loop body
    // errors (its tail), but the depth restore must still run before the
    // `?` propagates — otherwise the second function's bare `break` would
    // compile against the leaked depth.
    let errs = compile_err(
        "func a(): int { \
         x = 0; \
         while x < 3 { 1 + \"s\" }; \
         x \
         } \
         func b(): int { break; }",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected the first function's operand error, got {errs:?}"
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::BreakOutsideLoop)),
        "expected BreakOutsideLoop in the second function, got {errs:?}"
    );
}

#[test]
fn choose_arm_depth_is_restored_after_an_arm_error() {
    // The mirror pin: an arm body error propagates out of the arm's
    // (cleared) depth; the restore must put the enclosing loop's depth
    // back, so the `break` *after* the choose stays legal and only the
    // arm's own error is reported.
    let errs = compile_err(
        "func f(n: int): int { \
         x = 0; \
         while x < n { \
         y = choose x { when 1 { 1 + \"s\" } otherwise { 0 } }; \
         break; \
         x = x + 1; \
         }; \
         x \
         }",
    );
    assert_eq!(errs.len(), 1, "got {errs:?}");
    assert!(matches!(
        err_kinds(&errs)[0],
        TypeckErrorKind::TypeMismatch { .. }
    ));
}

// ========== 0.0.10 T03: the mapped prelude spelling sees the same rules ==========

#[test]
fn result_prelude_is_copy_argument_dependent() {
    // 0.0.7 D2's argument-dependent `is_copy`, pinned at the pipeline level
    // through the 0.0.10 prelude spelling: `Result<int, bool>` is Copy
    // (both arguments Copy) so a second read needs no `move`...
    compile_ok("func go() {\n    r: Result<int, bool> = Ok(1);\n    r2 = r;\n    r3 = r;\n}");
    // ...while `Result<int, string>` is owned (string is not Copy): after
    // the move, a second read is the pre-migration `UseAfterMove`. The
    // ownership rule sees the mapped internal `Type::Result` — no rule
    // change (T03), the resolver mapping is what feeds it (T02). (Inside a
    // function: a top-level binding is a global, and `move` off a global is
    // its own pre-migration rejection — `MoveOutOfGlobal`.)
    let errs = compile_err(
        "func go() {\n    r: Result<int, string> = Ok(1);\n    r2 = move r;\n    r3 = move r;\n}",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::UseAfterMove { .. })),
        "expected UseAfterMove, got {errs:?}"
    );
}

#[test]
fn result_prelude_unify_reports_mapped_type_in_text() {
    // Error-parity item: `unify` (here via a call argument) sees the
    // internal `Type::Result`, so the `ArgTypeMismatch` text names the
    // mapped type exactly as before the migration (typeck's `Type::name`
    // renders the bracket spelling) — `typeck/error.rs` is untouched since
    // the 0.0.10 merge-base (recorded expectation).
    let errs = compile_err("func f(r: Result<int, string>): int = 0\nf(42);");
    let hit = errs.iter().any(|e| {
        matches!(e.kind, TypeckErrorKind::ArgTypeMismatch { index: 0, .. })
            && e.to_string().contains("Result[int, string]")
    });
    assert!(
        hit,
        "expected ArgTypeMismatch naming `Result[int, string]`, got {errs:?}"
    );
}

// ========== 0.0.11 T02: the `map[K, V]` typeck bridge ==========

#[test]
fn map_type_is_never_copy() {
    // 0.0.11 D2: a map is always owned — `is_copy` is false for every
    // key/value pair, regardless of the element types.
    let hir = compile_ok("func f(m: map[int, int]): int { 0 }");
    match &hir.items[0] {
        TypedHirItem::Decl(TypedDeclHir::Func(f)) => {
            assert!(!f.params[0].ty.is_copy(), "map must never be Copy");
        }
        other => panic!("expected Func decl, got {other:?}"),
    }
}

#[test]
fn map_type_annotation_reports_a_readable_type_name() {
    // The bridge degrades a map type to `Unsupported`, but the *name* the
    // user sees in a mismatch must read as a type, not as an internal
    // placeholder. `map[K, V]` (the bridge's raw string) leaked straight
    // into the diagnostic; the user-facing spelling names the element
    // types: `map[int, string]`.
    let errs = compile_err("func f(): map[int, string] { 1 }");
    let hit = errs.iter().any(|e| {
        matches!(e.kind, TypeckErrorKind::TypeMismatch { .. })
            && e.to_string().contains("`map[int, string]`")
    });
    assert!(
        hit,
        "expected TypeMismatch naming `map[int, string]`, got {errs:?}"
    );
}

#[test]
fn two_different_map_types_do_not_unify() {
    // `Type::Map(k, v)` compares structurally (its key and value types), so
    // `map[int, string]` and `map[string, int]` are distinct types and do
    // not unify — the 0.0.3 V03 posture where `Array<int>` ≠ `Array<char>`.
    //
    // The value comes from a parameter (a `map[int, string]`-typed
    // binding); it is passed to a function that wants a
    // `map[string, int]`. Nothing else in the snippet mismatches, so any
    // reported error is the cross-type unification.
    let errs = compile_err(
        "func g(m: map[string, int]): int { 0 }\n\
         func f(m: map[int, string]): int { g(m); 0 }",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ArgTypeMismatch { .. })),
        "expected ArgTypeMismatch between two distinct map types, got {errs:?}"
    );
}

// ========== 0.0.11 T03: the `map` type ==========

/// Build a `Type::Map` for the pure-rule tests (is_copy / is_owned /
/// contains_*). A map is one of the owned types, mirroring `[T]`.
fn map_ty(k: Type, v: Type) -> Type {
    Type::Map(Box::new(k), Box::new(v))
}

#[test]
fn map_is_copy_exhaustive_matrix() {
    // D2/D3: a map is ALWAYS owned — never Copy, for every K / V (the `[T]`
    // posture). The element types span every whitelist key and a spread of
    // owned / Copy / non-Copy values, so the false verdict is not an accident
    // of one shape.
    assert!(!map_ty(Type::Int, Type::Int).is_copy());
    assert!(!map_ty(Type::Bool, Type::Bool).is_copy());
    assert!(!map_ty(Type::String, Type::String).is_copy());
    // owned value inside a Copy key
    assert!(!map_ty(Type::Int, Type::String).is_copy());
    // non-Copy value (a struct payload) inside a whitelist key
    let s = Type::Struct(Box::new(StructInfo {
        id: StructId(0),
        name: "S".into(),
        copy: false,
    }));
    assert!(!map_ty(Type::String, s).is_copy());
    // an enum value inside a whitelist key
    let e = Type::Enum(Box::new(EnumInfo {
        id: EnumId(0),
        name: "E".into(),
        copy: false,
        type_param_count: 0,
        payloads: Vec::new(),
    }));
    assert!(!map_ty(Type::String, e).is_copy());
    // a nested map is still a map — always owned.
    assert!(!map_ty(Type::Int, map_ty(Type::Int, Type::Int)).is_copy());
}

#[test]
fn map_is_owned_for_every_element() {
    // D2/D3: a map is one of the owned types (like `[T]`) — every
    // key / value combination is owned, so the ownership checker requires
    // `move` / `clone` at every consuming use.
    assert!(map_ty(Type::Int, Type::Int).is_owned());
    assert!(map_ty(Type::Bool, Type::String).is_owned());
    assert!(map_ty(Type::String, Type::Int).is_owned());
}

#[test]
fn map_contains_walks_key_and_value() {
    // D6: a map carries its keys and its values by ownership, so the
    // ownership checker's `contains_*` walk descends into BOTH slots. A
    // `map[string, Future<int>]` is exactly as fork-prone as a bare future at
    // every read route (`m[k]` deep-copies the value).
    assert!(map_ty(Type::Int, Type::Future(Box::new(Type::Int))).contains_future());
    assert!(map_ty(Type::Int, Type::Channel(Box::new(Type::Int))).contains_channel());
    assert!(map_ty(Type::Int, Type::Ptr(Box::new(Type::Int))).contains_ptr());
    // a whitelist key never trips the walk (keys are Copy / storable).
    assert!(!map_ty(Type::String, Type::Int).contains_future());
    assert!(!map_ty(Type::String, Type::Int).contains_channel());
    assert!(!map_ty(Type::String, Type::Int).contains_ptr());
}

#[test]
fn map_type_name_renders_user_facing_spelling() {
    // D5: the user-facing spelling is `map[K, V]` (the `[]` form). Error
    // messages that name a map use it, so a mismatch renders the real map
    // rather than an opaque placeholder.
    assert_eq!(map_ty(Type::Int, Type::String).name(), "map[int, string]");
}

#[test]
fn map_key_whitelist_accepts_int_bool_string() {
    // D5: a key must be one of `int`, `bool`, `string` — the `==`-comparable
    // set minus `float` / `unit` / `Range`. Each whitelist key typechecks.
    // The value is `string` so the literal needs no inference help; each key
    // type is spelled in the annotation and exercised by the literal's key.
    for (key_lit, key_ty) in [("0", "int"), ("true", "bool"), ("\"k\"", "string")] {
        compile_ok(&format!(
            "m: map[{key_ty}, string] = map {{ {key_lit} -> \"v\" }};"
        ));
    }
}

#[test]
fn map_key_rejects_outside_whitelist() {
    // D5: `float`, `unit`, `Range`, an array, a box, a Result, a function, a
    // Future, a channel, a ptr — none may be a key. Each reports
    // `MapKeyNotComparable` (named after what a key requires). The map type
    // is used as a parameter annotation, so the whitelist check in
    // `convert_type` is what fires — no literal involved.
    let key_cases = [
        "float",
        "unit",
        "Range",
        "[int]",
        "box<int>",
        "Result[int, string]",
        "(int) -> int",
        "Future<int>",
        "channel<int>",
        "ptr<int>",
    ];
    for key in key_cases {
        let src = format!("func f(m: map[{key}, int]): int {{ 0 }}");
        let errs = compile_err(&src);
        assert!(
            err_kinds(&errs)
                .iter()
                .any(|k| matches!(k, TypeckErrorKind::MapKeyNotComparable { .. })),
            "expected MapKeyNotComparable for key `{key}`, got {errs:?}"
        );
    }
    // A user struct / enum key (declared locally) is also rejected.
    let errs = compile_err(
        "struct S { x: int }\n\
         func f(m: map[S, int]): int { 0 }",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MapKeyNotComparable { .. }))
    );
    let errs = compile_err(
        "enum E { A, B }\n\
         func f(m: map[E, int]): int { 0 }",
    );
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MapKeyNotComparable { .. }))
    );
}

#[test]
fn map_key_not_comparable_message_names_the_whitelist() {
    // The error is actionable (SPEC §4): it names the offending key type and
    // lists the whitelist, so the fix is readable from the message.
    let errs = compile_err("func f(m: map[float, int]): int { 0 }");
    let msg = errs[0].to_string();
    assert!(
        msg.contains("float")
            && msg.contains("map key")
            && msg.contains("int")
            && msg.contains("bool")
            && msg.contains("string"),
        "expected the message to name `float` and the whitelist, got: {msg}"
    );
}

#[test]
fn map_literal_infers_key_and_value_from_first_entry() {
    // The first key/value fix the map's types; the result is a fresh owned
    // `map[K, V]` (D2). (Wrapped in parens so it is an expression item: a
    // bare `map { … }` statement takes no semicolon from ASI's `}` rule.)
    let hir = compile_ok("(map { 1 -> 2 });");
    assert_eq!(only_expr_ty(&hir), map_ty(Type::Int, Type::Int));
    let hir = compile_ok("(map { \"a\" -> true });");
    assert_eq!(only_expr_ty(&hir), map_ty(Type::String, Type::Bool));
}

#[test]
fn map_literal_rejects_inconsistent_keys_and_values() {
    // Later entries must match the first key / first value.
    let errs = compile_err("map { 1 -> 2, 3 -> \"x\" };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MapValueTypeMismatch { .. }))
    );
    let errs = compile_err("map { 1 -> 2, \"x\" -> 3 };");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::MapKeyTypeMismatch { .. }))
    );
}

#[test]
fn map_literal_bad_first_key_yields_one_error() {
    // A non-whitelisted first key aborts the literal, so the single bad key
    // produces exactly ONE `MapKeyNotComparable` — no follow-on key/value
    // mismatch errors against a type that was never accepted.
    let errs = compile_err("func f(): int { m = map { 1.0 -> \"a\", 2.0 -> 3 }; 0 }");
    let map_errs = err_kinds(&errs)
        .iter()
        .filter(|k| matches!(k, TypeckErrorKind::MapKeyNotComparable { .. }))
        .count();
    assert_eq!(
        map_errs, 1,
        "expected exactly one map-key error, got {errs:?}"
    );
    assert!(
        !err_kinds(&errs).iter().any(|k| matches!(
            k,
            TypeckErrorKind::MapKeyTypeMismatch { .. }
                | TypeckErrorKind::MapValueTypeMismatch { .. }
        )),
        "a rejected key must not cascade, got {errs:?}"
    );
}

#[test]
fn map_literal_bad_first_value_yields_one_error() {
    // Symmetric with the key: a non-storable first value aborts the literal,
    // so it produces exactly one seal error and no follow-on mismatches.
    let errs = compile_err(
        "async func t(): int { 0 }\n\
         func f(): int { m = map { 1 -> t(), \"x\" -> 2 }; 0 }",
    );
    let future_errs = err_kinds(&errs)
        .iter()
        .filter(|k| matches!(k, TypeckErrorKind::FutureNotStorable))
        .count();
    assert_eq!(
        future_errs, 1,
        "expected exactly one value error, got {errs:?}"
    );
    assert!(
        !err_kinds(&errs).iter().any(|k| matches!(
            k,
            TypeckErrorKind::MapKeyTypeMismatch { .. }
                | TypeckErrorKind::MapValueTypeMismatch { .. }
        )),
        "a rejected value must not cascade, got {errs:?}"
    );
}

#[test]
fn map_literal_value_rejects_future_channel_and_pointer() {
    // D6: a value may not carry a Future, a channel, or a ptr (the same
    // contains_* seal a struct field / array element / box cell applies).
    // Each is a separate snippet: the Future value is an awaited async call,
    // the channel value a fresh channel, and the ptr a raw `ptr<int>` inside
    // an unsafe body.
    let future_src = "async func half(n: int): int { n }\n\
                      func f(): int {\n\
                          m = map { 0 -> half(1) };\n\
                          0\n\
                      }";
    let errs = compile_err(future_src);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::FutureNotStorable))
    );

    let channel_src = "func f(): int {\n\
                           m = map { 0 -> channel<int>(1) };\n\
                           0\n\
                       }";
    let errs = compile_err(channel_src);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ChannelNotStorable))
    );

    let ptr_src = "unsafe func f(): int {\n\
                       x = 1;\n\
                       p = addr x;\n\
                       m = map { 0 -> p };\n\
                       0\n\
                   }";
    let errs = compile_err(ptr_src);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PtrNotStorable))
    );
}

#[test]
fn map_has_no_equality() {
    // D5: a map has no `==` / `!=` — deep map equality needs the 0.1.0
    // traits work (the 0.0.6 enums' posture).
    for op in ["==", "!="] {
        let errs = compile_err(&format!("m = map {{ 1 -> 2 }}; m {op} map {{ 1 -> 2 }};"));
        assert!(
            err_kinds(&errs)
                .iter()
                .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { op: _, ty: _ })),
            "expected InvalidOperand for `m {op} …`, got {errs:?}"
        );
    }
}

#[test]
fn map_has_no_as_string() {
    // D5: a map is not `as string`-convertible (the 0.0.6 enums' posture).
    let errs = compile_err("m = map { 1 -> 2 }; print(m as string);");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::UnsupportedCast { .. }))
    );
}

#[test]
fn map_key_whitelist_is_independent_of_annotation() {
    // D5/D1: a bare `map` keyword is not a type; the annotation carries K
    // and V. A map literal annotated `map[int, int]` is not affected by the
    // whitelist — only its *key* type is checked, not its spelling.
    compile_ok("m: map[int, int] = map { 1 -> 2, 3 -> 4 };");
}

#[test]
fn map_is_move_and_clone() {
    // D2/D3: a map is an owned value — `clone m` deep-copies the table and
    // `move m` transfers ownership (the ownership checker consumes the
    // binding). This is a typeck-level assertion: the snippet typechecks. A
    // read route (`m[k]`) is T05's.
    compile_ok("func f(): int { m = map { 1 -> 2 }; c = clone m; t = move m; 0 }");
}

#[test]
fn map_literal_needs_annotation_when_empty() {
    // The array-empty rule: an empty map literal has nothing to fix its key
    // and value types from, so it needs a `map[K, V]` annotation (and that
    // annotation is whitelist / storable-checked).
    let errs = compile_err("func f(): int { empty = map { }; 0 }");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::CannotInferMapType))
    );
    compile_ok("func f(): map[int, string] { map { } }");
}

#[test]
fn map_is_never_inferred_from_an_int_literal() {
    // The `map` keyword is a literal prefix, not a type (D1): a map literal
    // is the ONLY way to introduce a map value; no coercion from another type
    // produces one. A bare map literal fixes its type from its entries.
    let hir = compile_ok("(map { 1 -> 2 });");
    assert_eq!(only_expr_ty(&hir), map_ty(Type::Int, Type::Int));
}

#[test]
fn map_type_annotation_rejects_a_foreign_value_type() {
    // The bridge must still reject a value that is not a map at all —
    // `Unsupported` unifies with `Unsupported` but never with `Int`.
    let errs = compile_err("func f(): map[int, string] { 1 }");
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch, got {errs:?}"
    );
}

// (T02's `map_literal_is_rejected_until_t03` was replaced by the T03 suite
// above — the literal is now a real map at the typeck level, and T05 lands
// the lowering / access opcodes.)

// ========== 0.0.11 T04: `m[k]` access through `index_suffix` ==========

#[test]
fn map_index_hit_returns_result_of_value_and_string() {
    // D3: `m[k]` on a `map[K, V]` base (the `K` subscript unifies with the
    // key type) has type `Result[V, string]` — the missing key is
    // `Err("key not found")` at the runtime, but at the type level the
    // result is always a Result. (No lowering yet, T05: the type only.)
    let src = "func f(m: map[int, int]): Result[int, string] { r = m[1]; r }";
    let hir = compile_ok(src);
    match &hir.items[0] {
        TypedHirItem::Decl(TypedDeclHir::Func(f)) => {
            // The local `r` (the index result) has the Result type; the
            // return is exactly that.
            assert_eq!(
                f.ret_type.as_ref(),
                Some(&Type::Result(Box::new(Type::Int), Box::new(Type::String)))
            );
        }
        other => panic!("expected Func decl, got {other:?}"),
    }
}

#[test]
fn map_index_string_key_works() {
    // D5: `string` keys are in the whitelist; `m["a"]` on a
    // `map[string, int]` base is `Result<int, string>`.
    let src = "func f(m: map[string, int]): Result<int, string> { r = m[\"a\"]; r }";
    compile_ok(src);
}

#[test]
fn map_index_bool_key_works() {
    // D5: `bool` keys are in the whitelist.
    let src = "func f(m: map[bool, int]): Result<int, string> { r = m[true]; r }";
    compile_ok(src);
}

#[test]
fn map_index_result_with_string_value_is_moved_out() {
    // D2: the result of `m[k]` is `Result[V, string]`; when V is `string`
    // the Result is *owned* (the value side is owned). The index read is
    // already a copy out (the D2 read-copies rule), so the read itself
    // does not trip the bare-owned-use gate — that gate fires only on a
    // bare *variable* in a consuming position, and the index expression
    // is not a variable. Instead, the consuming position is the call
    // argument: `g(r)` where `r` holds an owned `Result<string, string>`
    // is flagged `OwnedArgRequiresMove` — the same rule a `string`
    // binding passed to a consuming argument follows. The flow-sensitive
    // pass then sees `r` as moved, so a second use is `UseAfterMove`.
    let src = "func g(r: Result<string, string>): int { 0 }\nfunc f(m: map[string, string]): Result<string, string> { r = m[\"a\"]; g(r); r }";
    let errs = compile_err(src);
    // The `OwnedArgRequiresMove` fires on the `g(r)` argument (the
    // owned `Result` passed by a bare variable). That single error is
    // sufficient to pin the D2 rule: the `Result[V, string]` index
    // result is *owned* when V is `string`, so a bare-variable use in
    // a consuming position is flagged exactly like a `string` binding.
    let kinds = err_kinds(&errs);
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::OwnedArgRequiresMove { .. })),
        "expected OwnedArgRequiresMove for the owned Result argument, got {kinds:?}"
    );
}

#[test]
fn map_index_wrong_key_type_is_type_mismatch() {
    // D4: an `int` key where the map wants a `string` key (or the reverse)
    // is a `TypeMismatch` (the ticket's `ArgTypeMismatch` slot — `unify`
    // reports it against the key type, the index expression's span).
    let src_bad1 = "func f(m: map[string, int]): Result<int, string> { r = m[1]; 0 }";
    let errs = compile_err(src_bad1);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::TypeMismatch { .. })),
        "expected TypeMismatch for the wrong key type, got {errs:?}"
    );
}

#[test]
fn map_index_on_non_map_base_stays_invalid_operand() {
    // A non-array, non-map base (e.g. a scalar `int`) is still the
    // array-path `InvalidOperand(index)` error — the map case is a new
    // base type, not a change to the array path.
    let src = "func f(x: int): int { r = x[1]; 0 }";
    let errs = compile_err(src);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::InvalidOperand { op, .. } if *op == "index")),
        "expected InvalidOperand(index) for a scalar base, got {errs:?}"
    );
}

#[test]
fn map_index_composes_with_question() {
    // D4: `m[k]?` composes with the 0.0.2 `?` propagation rules — the
    // `typeck_question` path is unchanged (it already sees a `Result<V,
    // string>` operand and requires the enclosing function to return
    // `Result<_, string>`).
    let src = "func f(m: map[int, int]): Result<int, string> { v = m[1]?; Ok(v) }";
    compile_ok(src);
}

#[test]
fn map_index_question_outside_result_fn_is_error() {
    // The `?` on a map index is still `QuestionOutsideResultFn` when the
    // enclosing function's return type is not a `Result` with the
    // matching error type (the 0.0.2 rule, applied to the map result).
    let src = "func f(m: map[int, int]): int { v = m[1]?; 0 }";
    let errs = compile_err(src);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::QuestionOutsideResultFn)),
        "expected QuestionOutsideResultFn, got {errs:?}"
    );
}

#[test]
fn map_index_read_seals_future_value_slot() {
    // D6 read-route seal: a map whose value slot is `Future<int>` is
    // rejected at *every* `m[k]` read through the same
    // `check_map_value_storable` walk used at declaration / literal — a
    // deep copy of the index result would fork a suspended frame.
    let src = "func f(m: map[int, Future<int>]): Result[int, string] { r = m[1]; r }";
    let errs = compile_err(src);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::FutureNotStorable)),
        "expected FutureNotStorable for the map index read, got {errs:?}"
    );
}

#[test]
fn map_index_read_seals_channel_and_ptr_value_slots() {
    // D6 read-route seal, the channel / ptr mirrors of the future case.
    let src_ch = "func f(m: map[int, channel<int>]): Result[channel<int>, string] { r = m[1]; r }";
    let errs = compile_err(src_ch);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::ChannelNotStorable)),
        "expected ChannelNotStorable, got {errs:?}"
    );
    let src_ptr = "func f(m: map[int, ptr<int>]): Result[ptr<int>, string] { r = m[1]; r }";
    let errs = compile_err(src_ptr);
    assert!(
        err_kinds(&errs)
            .iter()
            .any(|k| matches!(k, TypeckErrorKind::PtrNotStorable)),
        "expected PtrNotStorable, got {errs:?}"
    );
}
