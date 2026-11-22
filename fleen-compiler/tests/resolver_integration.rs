//! Integration tests for the resolver.
//!
//! Two layers:
//! 1. Inline unit/integration tests for specific behaviors
//! 2. Directory-based tests: every `.fln` file in `tests/resolver/valid/` must
//!    resolve successfully, every file in `tests/resolver/invalid/` must fail.

use fleen_compiler::lexer::tokenize;
use fleen_compiler::parser::parse;
use fleen_compiler::resolver::error::ResolveErrorKind;
use fleen_compiler::resolver::hir::DeclHir;
use fleen_compiler::resolver::resolve;
use fleen_compiler::resolver::resolve_module_with_table;
use std::fs;
use std::path::Path;

fn resolve_str(
    src: &str,
) -> Result<fleen_compiler::resolver::hir::Hir, Vec<fleen_compiler::resolver::error::ResolveError>>
{
    let tokens = tokenize(src).expect("lexer should succeed");
    let ast = parse(tokens).expect("parser should succeed");
    resolve(ast)
}

// ========== Binding & Assignment ==========

#[test]
fn resolve_basic_binding() {
    let hir = resolve_str("x = 42; y = x;").expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_assignment_after_binding() {
    // x = 42; x = 43; → second is assignment to the same binding
    let hir = resolve_str("x = 42; x = 43;").expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_const_binding() {
    let hir = resolve_str("const x = 42; y = x;").expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_typed_binding() {
    let hir = resolve_str("x: int = 42; const y: string = \"hi\";").expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_binding_uses_outer_in_init() {
    // init expression may reference outer variables
    let hir = resolve_str("a = 1; b = a + 1;").expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_self_reference_in_init_is_error() {
    // x = x + 1; → x used before binding in its own initializer
    let errs = resolve_str("x = x + 1;").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. }))
    );
}

// ========== Scopes ==========

#[test]
fn resolve_block_scope_hides_binding() {
    // binding inside block not visible outside
    let errs = resolve_str(
        r#"
func main() {
    if true { x = 1; };
    print(x);
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. }))
    );
}

#[test]
fn resolve_nested_blocks() {
    let hir = resolve_str(
        r#"
func main() {
    x = 1;
    {
        y = 2;
        {
            z = x + y;
        };
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_shadow_same_mutability() {
    // mutable shadows mutable: OK
    let hir = resolve_str(
        r#"
func main() {
    x = 42;
    {
        x = 10;
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_shadow_const_with_const() {
    // const shadows const: OK
    let hir = resolve_str(
        r#"
func main() {
    const x = 42;
    {
        const x = 10;
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn error_shadow_mutable_with_const() {
    // mutable outer, const inner: ERROR
    let errs = resolve_str(
        r#"
func main() {
    x = 42;
    {
        const x = 10;
    };
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::ShadowingMutabilityMismatch { .. }))
    );
}

#[test]
fn error_shadow_const_with_mutable() {
    // const outer, mutable inner: ERROR
    let errs = resolve_str(
        r#"
func main() {
    const x = 42;
    {
        x = 10;
    };
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::ShadowingMutabilityMismatch { .. }))
    );
}

#[test]
fn resolve_function_scope_is_independent() {
    // function body does not participate in shadowing rules:
    // outer mutable x, inner const x inside function is legal
    let hir = resolve_str(
        r#"
x = 42;
func foo() {
    const x = 10;
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_function_param_shadows_outer() {
    // parameter named like an outer variable is a fresh binding
    let hir = resolve_str(
        r#"
x = 42;
func foo(x: int) {
    print(x);
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

// ========== Functions ==========

#[test]
fn resolve_function_call() {
    let hir = resolve_str(
        r#"
func add(a: int, b: int): int = a + b
result = add(1, 2);
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_forward_function_reference() {
    // function called before its declaration
    let hir = resolve_str(
        r#"
func main() {
    print(add(1, 2));
}
func add(a: int, b: int): int = a + b
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_recursion() {
    let hir = resolve_str(
        r#"
func fib(n: int): int {
    if n < 2 { n } else { fib(n - 1) + fib(n - 2) }
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_function_as_value() {
    // function name used as a first-class value
    let hir = resolve_str(
        r#"
func add(a: int, b: int): int = a + b
func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)
result = apply(add, 1, 2);
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 3);
}

#[test]
fn error_call_undeclared_function() {
    let errs = resolve_str(
        r#"
func main() {
    print(missing(1));
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. }))
    );
}

#[test]
fn resolve_nested_function_decl() {
    // nested func declaration inside a function body
    let hir = resolve_str(
        r#"
func main() {
    func inner(a: int): int = a * 2
    print(inner(21));
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// ========== Control flow ==========

#[test]
fn resolve_if_expr() {
    let hir = resolve_str(
        r#"
func main() {
    x = 10;
    if x > 5 { "big" } elif x > 0 { "small" } else { "zero" };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_while_expr() {
    let hir = resolve_str(
        r#"
func main() {
    x = 0;
    while x < 10 { x = x + 1; };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_while_condition_sees_outer_binding() {
    let hir = resolve_str(
        r#"
func main() {
    x = 0;
    while x < 10 { x = x + 1; };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_choose_expr() {
    let hir = resolve_str(
        r#"
func main() {
    x = 1;
    choose x {
        when 1 { "one" }
        when 2 { "two" }
        otherwise { "other" }
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_choose_pattern_binding() {
    // `when y { ... }` binds y in the arm scope
    let hir = resolve_str(
        r#"
func main() {
    x = 42;
    choose x {
        when y { print(y) }
        otherwise { print("other") }
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_choose_guard_uses_pattern_binding() {
    // guard `if n > 10` can reference the pattern binding n
    let hir = resolve_str(
        r#"
func main() {
    x = 15;
    choose x {
        when n if n > 10 { print("big") }
        otherwise { print("small") }
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_choose_pattern_binding_not_visible_outside_arm() {
    // pattern binding must not leak into the enclosing scope
    let errs = resolve_str(
        r#"
func main() {
    x = 42;
    choose x {
        when y { print(y) }
        otherwise { print("other") }
    };
    print(y);
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. }))
    );
}

#[test]
fn resolve_negative_pattern() {
    let hir = resolve_str(
        r#"
func main() {
    x = 0 - 1;
    choose x {
        when -1 { print("neg") }
        otherwise { print("other") }
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// ========== Scope leakage on the error path (regression) ==========

/// After a function body fails to resolve, its scope must be left anyway:
/// a name declared only inside that body must not leak out.
#[test]
fn error_in_function_body_does_not_leak_scope() {
    let errs = resolve_str(
        r#"
func bad() {
    inner = 1;
    nosuchvar
}
print(inner);
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter().any(
            |e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "inner")
        ),
        "expected `inner` to be undeclared outside the function body, got {errs:?}"
    );
}

/// A nested `func` declaration inside a body must not destroy the enclosing
/// function's shadow barrier ("functions have no effect").
#[test]
fn resolve_nested_func_preserves_shadow_barrier() {
    let hir = resolve_str(
        r#"
x = 42;
func outer() {
    func inner() { }
    func also_inner() { }
    const x = 1;
}
"#,
    )
    .expect("nested functions must not shadow-check against globals");
    assert_eq!(hir.items.len(), 2);
}

// ========== Unified `=` semantics ==========

/// Expression-position `=` must never write through a function boundary.
#[test]
fn error_expr_assign_cannot_cross_function_boundary() {
    let errs = resolve_str(
        r#"
x = 1;
func f() { y = (x = 2); }
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. }))
    );
}

/// ... but within the same function it is a plain assignment.
#[test]
fn resolve_expr_assign_within_function() {
    let hir = resolve_str("func f() { x = 1; y = (x = 2); }").expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

/// Chained assignment requires the target to already exist; the RHS never
/// binds a fresh name (expression `=` is assignment-only).
#[test]
fn error_chain_assign_to_undeclared() {
    let errs = resolve_str("a = b = 5;").expect_err("should fail");
    assert!(
        errs.iter().any(
            |e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "b")
        )
    );
}

#[test]
fn resolve_chain_assign_to_declared() {
    let hir = resolve_str("b = 5;\na = b = 3;").expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

// ========== Loop-body assignment (DESIGN.md §4.5) ==========

/// The counter idiom must ASSIGN the outer binding, not shadow it.
#[test]
fn resolve_loop_body_assigns_outer_binding() {
    use fleen_compiler::resolver::hir::{DeclHir, ExprHir, FuncBodyHir, HirItem, StmtHir};

    let hir = resolve_str(
        r#"
func main() {
    x = 0;
    while x < 10 {
        x = x + 1;
    };
}
"#,
    )
    .expect("should resolve");

    let HirItem::Decl(DeclHir::Func(func)) = &hir.items[0] else {
        panic!("expected a function")
    };
    let FuncBodyHir::Block(body) = &func.body else {
        panic!("expected a block body")
    };
    let StmtHir::Decl(DeclHir::Var(outer)) = &body.stmts[0] else {
        panic!("expected outer binding")
    };
    let StmtHir::Expr(while_expr, _) = &body.stmts[1] else {
        panic!("expected while")
    };
    let ExprHir::While(while_expr) = while_expr.as_ref() else {
        panic!("expected while")
    };
    let StmtHir::Decl(DeclHir::Var(inner)) = &while_expr.body.stmts[0] else {
        panic!("expected loop-body binding")
    };

    assert_eq!(
        outer.binding_id, inner.binding_id,
        "loop body must assign the outer binding, not shadow it"
    );
}

#[test]
fn resolve_loop_body_assigns_outer_global() {
    let hir = resolve_str("x = 0;\nwhile x < 10 { x = x + 1; };").expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn error_loop_assign_to_const() {
    let errs = resolve_str(
        r#"
func main() {
    const x = 0;
    while x < 3 { x = 1; };
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::AssignToImmutable { .. }))
    );
}

#[test]
fn error_loop_assign_to_parameter() {
    let errs =
        resolve_str("func f(x: int) { while x > 0 { x = x - 1; }; }").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::AssignToImmutable { .. }))
    );
}

/// Loop scope is inherited by nested blocks inside the body.
#[test]
fn resolve_loop_scope_extends_into_nested_blocks() {
    for src in [
        "func main() { x = 0;\n while x < 3 { { x = x + 1; }; }; }",
        "func main() { x = 0;\n while x < 3 { if true { x = x + 1; }; }; }",
    ] {
        resolve_str(src).unwrap_or_else(|e| panic!("should resolve `{src}`, got {e:?}"));
    }
}

#[test]
fn error_type_annotation_on_loop_assignment() {
    let errs = resolve_str("func main() { x = 0;\n while x < 3 { x: int = 1; }; }")
        .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::TypeAnnotationOnAssignment))
    );
}

/// Non-loop blocks keep plain shadow semantics.
#[test]
fn error_if_block_shadows_rather_than_assigns() {
    let errs = resolve_str(
        r#"
func main() {
    x = 1;
    if true { const x = 2; print(x); };
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::ShadowingMutabilityMismatch { .. }))
    );
}

// ========== Parameter shadowing ==========

#[test]
fn resolve_param_rebind_in_body() {
    let hir = resolve_str("func f(x: int): int { x = 2; x }").expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_param_const_shadow_in_body() {
    let hir = resolve_str("func f(x: int): int { const x = 2; x }").expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// ========== Duplicate declarations ==========

#[test]
fn error_duplicate_top_level_functions() {
    let errs =
        resolve_str("func f(): int = 1\nfunc f(): int = 2\nr = f();").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. }))
    );
}

#[test]
fn error_duplicate_parameters() {
    let errs = resolve_str("func f(a: int, a: int): int = a").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. }))
    );
}

#[test]
fn error_function_name_clashes_with_variable() {
    let errs = resolve_str("x = 1;\nfunc x(): int = 2\nr = x();").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. }))
    );
}

#[test]
fn error_duplicate_const() {
    let errs = resolve_str("const x = 1;\nconst x = 2;").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. }))
    );
}

// ========== Builtin shadowing ==========

#[test]
fn resolve_user_binding_shadows_builtin() {
    resolve_str("print = 42;").expect("user binding may shadow the builtin");
    resolve_str("const print = 42;").expect("user const may shadow the builtin");
}

#[test]
fn resolve_builtin_still_callable() {
    resolve_str("print(\"hello\");").expect("builtin remains available");
}

// ========== Error spans ==========

#[test]
fn errors_carry_real_spans() {
    let src = "x = 42;\n{ const x = 10; };\nprint(y);";
    let errs = resolve_str(src).expect_err("should fail");

    let slice = |s: fleen_compiler::lexer::Span| {
        let start = (s.start as usize).min(src.len());
        let end = src.len().min(s.end as usize).max(start);
        start..end
    };

    let shadow = errs
        .iter()
        .find(|e| matches!(e.kind, ResolveErrorKind::ShadowingMutabilityMismatch { .. }))
        .expect("shadow mismatch");
    assert_eq!(
        &src[slice(shadow.span)],
        "const x = 10;",
        "shadow error must point at the declaration"
    );

    let undeclared = errs
        .iter()
        .find(|e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "y"))
        .expect("undeclared y");
    assert_eq!(
        &src[slice(undeclared.span)],
        "y",
        "error must point at the use site"
    );
}

#[test]
fn error_duplicate_binding_reports_first_span() {
    let src = "func f(a: int, a: int): int = a";
    let errs = resolve_str(src).expect_err("should fail");
    let dup = errs
        .iter()
        .find(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. }))
        .expect("duplicate binding");
    let first = &src[(dup.span.start as usize)..(dup.span.end as usize)];
    assert!(
        first.contains("a"),
        "duplicate error should point at the offending parameter, got `{first}`"
    );
}

// ========== SPEC.md §14 flagship examples ==========

#[test]
fn resolve_spec_md_examples() {
    resolve_str(
        r#"
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0;
    const limit = 10;
    while x < limit {
        print(fib(x));
        x = x + 1;
    };
    0
}
"#,
    )
    .expect("SPEC.md §14 fib + counter loop must resolve");
}

// ========== Errors ==========

#[test]
fn error_const_reassignment() {
    let errs = resolve_str("const x = 42; x = 10;").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::AssignToImmutable { .. }))
    );
}

#[test]
fn error_const_reassignment_in_block() {
    let errs = resolve_str(
        r#"
func main() {
    const x = 1;
    x = 2;
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::AssignToImmutable { .. }))
    );
}

#[test]
fn error_undeclared_variable() {
    let errs = resolve_str(
        r#"
func main() {
    print(y);
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. }))
    );
}

#[test]
fn error_assign_undeclared_in_function() {
    // First occurrence `y = 10` in a scope is a binding, not an error.
    // To test "assign to undeclared", we need a second occurrence without a prior binding.
    // But that's actually a binding too. The real "undeclared" case is using a variable
    // in an expression context, not as LHS of assignment.
    // So this test is removed; the test `error_undeclared_variable` covers the real case.
    let hir = resolve_str(
        r#"
func main() {
    y = 10;
}
"#,
    )
    .expect("first occurrence is binding");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn error_use_before_binding_same_scope() {
    let errs = resolve_str(
        r#"
func main() {
    print(x);
    x = 42;
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. }))
    );
}

#[test]
fn error_invalid_assignment_target() {
    // `f() = 1` — LHS is a call, not an identifier
    let errs = resolve_str(
        r#"
func f(): int = 1
f() = 2;
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::InvalidAssignmentTarget))
    );
}

#[test]
fn error_deref_assign_undeclared() {
    // 0.0.2 U02: `deref b = v` requires `b` to be a declared binding
    let errs = resolve_str("deref b = 1;").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. }))
    );
}

#[test]
fn error_deref_assign_non_ident_target() {
    // 0.0.2 U02: only `deref <ident> = v` is an assignment target;
    // `deref b.c` is not (use was never assigned a binding)
    let errs = resolve_str("b = box 1; deref b.c = 2;").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::InvalidAssignmentTarget))
    );
}

#[test]
fn resolve_deref_assign_target() {
    // 0.0.2 U02: `deref b = v` resolves to AssignDeref against binding `b`
    let hir = resolve_str("b = box 1; v = 2; deref b = v;").expect("should resolve");
    assert_eq!(hir.items.len(), 3);
    let fleen_compiler::resolver::hir::HirItem::Expr(
        fleen_compiler::resolver::hir::ExprHir::AssignDeref { name, .. },
    ) = &hir.items[2]
    else {
        panic!("expected AssignDeref expr");
    };
    assert_eq!(name, "b");
}

#[test]
fn errors_are_collected_not_fail_fast() {
    // multiple errors reported together
    let errs = resolve_str(
        r#"
func main() {
    print(a);
    print(b);
}
"#,
    )
    .expect_err("should fail");
    assert!(errs.len() >= 2);
}

// ========== Misc ==========

#[test]
fn resolve_import() {
    let hir = resolve_str("import std.io;").expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// ========== 0.0.10 T01: the std `Result` declaration module (D1/D2) ==========

/// `Result` names the std.result builtin type (D1). The declaration in
/// `std/result.fln` is the only legal one; a user module's `enum Result` is
/// rejected (pre-0.0.10 parity rationale: canonical note at `collect_enums`
/// in `resolver/mod.rs`).
#[test]
fn resolve_rejects_user_enum_named_result() {
    let errs = resolve_str("enum Result<T, E> { Ok(T), Err(E) }\n").expect_err("should fail");
    assert!(errs.iter().any(
        |e| matches!(e.kind, ResolveErrorKind::ReservedBuiltinName { ref name } if name == "Result")
    ));
}

#[test]
fn resolve_rejects_non_parameterized_user_enum_named_result() {
    let errs = resolve_str("enum Result { Something }\n").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::ReservedBuiltinName { .. }))
    );
}

/// `import std.result;` resolves; the module's only export is the enum.
/// The export's contents — the `Result` name with the `Ok` / `Err` variant
/// names and the type-parameter arity — are pinned at the table level by
/// `std_result_module_exports_result_with_ok_err_variants` in `src/tests.rs`
/// (this crate-external test cannot reach the module table).
#[test]
fn resolve_import_std_result() {
    let hir = resolve_str("import std.result;").expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

/// The prelude trio stays untouched at T01: `Ok` / `Err` still resolve
/// through the parser's Result constructors, `Result<T, E>` still resolves
/// through the builtin type — a program shaped like every pre-0.0.10
/// Result user compiles unchanged.
#[test]
fn resolve_result_programs_unchanged_at_t01() {
    let hir = resolve_str(
        r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func main(): int {
    r = choose div(1, 0) {
        when Ok(v) { v }
        otherwise { 0 - 1 }
    };
    0
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn resolve_empty_program() {
    let hir = resolve_str("").expect("should resolve");
    assert_eq!(hir.items.len(), 0);
}

#[test]
fn resolve_fib_program() {
    let hir = resolve_str(
        r#"
func fib(n: int): int {
    if n < 2 { n } elif n == 2 { 1 } else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0;
    const limit = 10;

    while x < limit {
        print(fib(x));
        x = x + 1;
    };

    0
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 2);
}

// ========== Additional coverage tests ==========

// --- choose without otherwise ---
#[test]
fn resolve_choose_without_otherwise() {
    // DESIGN.md says "otherwise must be present (or the compiler can prove
    // exhaustiveness)"
    // but resolver does not enforce this; it should still resolve
    let hir = resolve_str(
        r#"
func main() {
    x = 1;
    choose x {
        when 1 { "one" }
    };
}
"#,
    )
    .expect("choose without otherwise should resolve (resolver does not enforce exhaustiveness)");
    assert_eq!(hir.items.len(), 1);
}

// --- choose literal pattern ---
#[test]
fn resolve_choose_literal_pattern() {
    let hir = resolve_str(
        r#"
func main() {
    x = 42;
    choose x {
        when 0 { "zero" }
        when 1 { "one" }
        otherwise { "other" }
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- choose guard with undeclared variable ---
#[test]
fn error_choose_guard_undeclared_variable() {
    let errs = resolve_str(
        r#"
func main() {
    x = 15;
    choose x {
        when n if n > unknown_var { "big" }
        otherwise { "small" }
    };
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "unknown_var"))
    );
}

// --- while condition with undeclared variable ---
#[test]
fn error_while_condition_undeclared_variable() {
    let errs = resolve_str(
        r#"
func main() {
    while missing_condition { x = 1; };
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "missing_condition"))
    );
}

// --- if condition with undeclared variable ---
#[test]
fn error_if_condition_undeclared_variable() {
    let errs = resolve_str(
        r#"
func main() {
    if missing_condition { x = 1; };
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "missing_condition"))
    );
}

// --- nested control flow ---
#[test]
fn resolve_nested_control_flow() {
    let hir = resolve_str(
        r#"
func main() {
    x = 0;
    while x < 10 {
        if x > 5 {
            choose x {
                when 6 { print("six") }
                when 7 { print("seven") }
                otherwise { print("other") }
            };
        };
        x = x + 1;
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- nested while loops ---
#[test]
fn resolve_nested_while_loops() {
    let hir = resolve_str(
        r#"
func main() {
    i = 0;
    while i < 3 {
        j = 0;
        while j < 3 {
            j = j + 1;
        };
        i = i + 1;
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- function call with undeclared variable ---
#[test]
fn error_function_call_undeclared_variable() {
    let errs = resolve_str(
        r#"
func add(a: int, b: int): int = a + b
func main() {
    print(add(unknown_var, 2));
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "unknown_var"))
    );
}

// --- index access with undeclared variable ---
#[test]
fn error_index_access_undeclared_variable() {
    let errs = resolve_str(
        r#"
func main() {
    arr = 42;
    print(arr[unknown_index]);
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "unknown_index"))
    );
}

// --- field access with undeclared variable ---
#[test]
fn error_field_access_undeclared_variable() {
    let errs = resolve_str(
        r#"
func main() {
    print(unknown_obj.field);
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "unknown_obj"))
    );
}

// --- global loop body assignment binding_id verification ---
#[test]
fn resolve_loop_body_assigns_outer_global_binding_id() {
    use fleen_compiler::resolver::hir::{ExprHir, HirItem, StmtHir};

    let hir = resolve_str("x = 0;\nwhile x < 10 { x = x + 1; };").expect("should resolve");

    // items[0] = outer binding, items[1] = while loop
    let HirItem::Decl(DeclHir::Var(outer)) = &hir.items[0] else {
        panic!("expected outer binding")
    };
    let HirItem::Expr(ExprHir::While(while_expr)) = &hir.items[1] else {
        panic!("expected while")
    };
    let StmtHir::Decl(DeclHir::Var(inner)) = &while_expr.body.stmts[0] else {
        panic!("expected loop body binding")
    };

    assert_eq!(
        outer.binding_id, inner.binding_id,
        "loop body must assign the outer binding, not shadow it"
    );
}

// --- error recovery: subsequent statements still resolve ---
#[test]
fn error_recovery_subsequent_statements_resolve() {
    let hir = resolve_str(
        r#"
func main() {
    print(a);
    x = 42;
    print(x);
}
"#,
    );
    // Should fail because of `a`, but `x = 42` should still be resolved
    assert!(hir.is_err());
    let errs = hir.unwrap_err();
    assert_eq!(errs.len(), 1, "only `a` should be undeclared");
    assert!(
        errs.iter().any(
            |e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "a")
        )
    );
}

// --- Span correctness for various error types ---
#[test]
fn error_spans_are_correct_for_various_errors() {
    let src = "const x = 42;\nx = 10;\nprint(y);";
    let errs = resolve_str(src).expect_err("should fail");

    let slice = |s: fleen_compiler::lexer::Span| {
        let start = (s.start as usize).min(src.len());
        let end = src.len().min(s.end as usize).max(start);
        start..end
    };

    // AssignToImmutable span should point at `x = 10`
    let assign_err = errs
        .iter()
        .find(|e| matches!(e.kind, ResolveErrorKind::AssignToImmutable { .. }))
        .expect("assign to immutable");
    assert_eq!(
        &src[slice(assign_err.span)],
        "x = 10;",
        "assign error must point at the assignment"
    );

    // UndeclaredVariable span should point at `y`
    let undeclared_err = errs
        .iter()
        .find(|e| matches!(&e.kind, ResolveErrorKind::UndeclaredVariable { name } if name == "y"))
        .expect("undeclared y");
    assert_eq!(
        &src[slice(undeclared_err.span)],
        "y",
        "error must point at the use site"
    );
}

// --- import interaction with other statements ---
#[test]
fn resolve_import_with_other_statements() {
    let hir = resolve_str(
        r#"
import std.io;
x = 42;
print(x);
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 3);
}

// --- choose arm shadowing ---
#[test]
fn error_choose_arm_shadowing_mutability_mismatch() {
    // const x shadows mutable x: mutability mismatch error
    let errs = resolve_str(
        r#"
func main() {
    x = 42;
    choose x {
        when 1 { const x = 10; print(x); }
        otherwise { print("other") }
    };
}
"#,
    )
    .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::ShadowingMutabilityMismatch { .. }))
    );
}

#[test]
fn resolve_choose_arm_shadowing_same_mutability() {
    // mutable x shadows mutable x: OK
    let hir = resolve_str(
        r#"
func main() {
    x = 42;
    choose x {
        when 1 { x = 10; print(x); }
        otherwise { print("other") }
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- function body const shadowing parameter ---
#[test]
fn resolve_function_body_const_shadows_parameter() {
    let hir = resolve_str("func f(x: int): int { const x = 2; x }").expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- nested function with loop body assignment ---
#[test]
fn resolve_nested_function_with_loop_body_assignment() {
    let hir = resolve_str(
        r#"
func outer() {
    x = 0;
    while x < 10 {
        x = x + 1;
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- choose arm with loop body assignment ---
#[test]
fn resolve_choose_arm_with_loop_body_assignment() {
    let hir = resolve_str(
        r#"
func main() {
    x = 0;
    choose x {
        when 1 {
            while x < 10 {
                x = x + 1;
            };
        }
        otherwise { print("other") }
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- if arm with loop body assignment ---
#[test]
fn resolve_if_arm_with_loop_body_assignment() {
    let hir = resolve_str(
        r#"
func main() {
    x = 0;
    if x < 5 {
        while x < 10 {
            x = x + 1;
        };
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- nested choose ---
#[test]
fn resolve_nested_choose() {
    let hir = resolve_str(
        r#"
func main() {
    x = 1;
    choose x {
        when 1 {
            choose x {
                when 1 { print("one") }
                otherwise { print("other") }
            };
        }
        otherwise { print("other") }
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// --- nested if ---
#[test]
fn resolve_nested_if() {
    let hir = resolve_str(
        r#"
func main() {
    x = 10;
    if x > 5 {
        if x > 8 {
            print("big");
        };
    };
}
"#,
    )
    .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// ========== 0.0.8 T04: two-part curried declarations (D3) ==========

#[test]
fn resolve_curried_two_part_declaration() {
    // The signature carries the types/modifiers, the body the names — the
    // resolver synthesizes the ordinary `FuncDecl` typeck/lower/codegen see.
    use fleen_compiler::parser::ast::{BaseType, Type};
    use fleen_compiler::resolver::hir::{FuncBodyHir, HirItem};

    let hir = resolve_str("#feature curry\nadd: int -> int -> int\nadd(a, b) { a + b }")
        .expect("two-part declaration should resolve");
    // One function declaration — the signature contributes no item of its own.
    assert_eq!(hir.items.len(), 1);
    let HirItem::Decl(DeclHir::Func(func)) = &hir.items[0] else {
        panic!("expected a function declaration")
    };
    assert_eq!(func.name, "add");
    assert_eq!(func.params.len(), 2);
    assert_eq!(func.params[0].name, "a");
    assert_eq!(func.params[0].ty, Type::Base(BaseType::Int));
    assert_eq!(func.params[1].name, "b");
    assert_eq!(func.params[1].ty, Type::Base(BaseType::Int));
    let Some(ret) = &func.ret_type else {
        panic!("the signature's return type must travel")
    };
    assert_eq!(*ret, Type::Base(BaseType::Int));
    assert!(matches!(func.body, FuncBodyHir::Block(_)));
}

#[test]
fn resolve_curried_single_expr_body_and_modifiers() {
    // `= expr` body form; modifiers (`async`) and `pub` come from the
    // signature line (D3).
    let hir = resolve_str("#feature curry\npub async half: int -> int\nhalf(n) = n / 2")
        .expect("curried `= expr` body with modifiers should resolve");
    let fleen_compiler::resolver::hir::HirItem::Decl(DeclHir::Func(func)) = &hir.items[0] else {
        panic!("expected a function declaration")
    };
    assert!(func.is_async, "`async` on the signature must travel");
    assert_eq!(func.name, "half");
    assert_eq!(func.params.len(), 1);
}

#[test]
fn resolve_curried_zero_param() {
    // `() -> int` is a zero-parameter function (D4); `unit -> int` is unary.
    let hir = resolve_str(
        "#feature curry\nping: () -> int\nping() { 42 }\nunitf: unit -> int\nunitf(u) { 7 }",
    )
    .expect("zero-param and unit-param curried declarations should resolve");
    assert_eq!(hir.items.len(), 2);
    let fleen_compiler::resolver::hir::HirItem::Decl(DeclHir::Func(zero)) = &hir.items[0] else {
        panic!("expected a function declaration")
    };
    assert!(zero.params.is_empty());
}

#[test]
fn resolve_curried_recursion() {
    // Signatures register before any body resolves, so recursion works
    // unchanged (T04 card).
    let hir = resolve_str(
        "#feature curry\nfib: int -> int\nfib(n) {\n    if n < 2 { n }\n    else { fib(n - 1) + fib(n - 2) }\n}",
    )
    .expect("recursive two-part declaration should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_curried_forward_body_reference() {
    // A call may appear before the callee's signature/body lines (the same
    // forward-reference rule top-level `func` declarations follow).
    let hir = resolve_str(
        "#feature curry\nmain: () -> unit\nmain() { twice(21) }\ntwice: int -> int\ntwice(n) { n * 2 }",
    )
    .expect("forward-referenced two-part declaration should resolve");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn error_curried_body_without_signature() {
    let errs = resolve_str("#feature curry\nadd(a, b) { a + b }").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::BodyWithoutSignature { .. })),
        "expected BodyWithoutSignature, got {errs:?}"
    );
}

#[test]
fn error_curried_signature_without_body() {
    let errs = resolve_str("#feature curry\nadd: int -> int -> int").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::SignatureWithoutBody { .. })),
        "expected SignatureWithoutBody, got {errs:?}"
    );
}

#[test]
fn error_curried_body_before_signature() {
    // Source order is enforced (D3): even though the signature exists in the
    // module, a body may only match a signature that *precedes* it.
    let errs = resolve_str("#feature curry\nadd(a, b) { a + b }\nadd: int -> int -> int")
        .expect_err("body-before-signature must fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::BodyWithoutSignature { .. })),
        "expected BodyWithoutSignature, got {errs:?}"
    );
}

#[test]
fn error_curried_arity_mismatch() {
    // The arrow chain's parameter count must equal the body's parameter count.
    let errs = resolve_str("#feature curry\nadd: int -> int\nadd(a, b) { a + b }")
        .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::SignatureArityMismatch { .. })),
        "expected SignatureArityMismatch, got {errs:?}"
    );
}

#[test]
fn error_curried_duplicate_signature() {
    let errs = resolve_str("#feature curry\nf: int -> int\nf: int -> int\nf(x) { x }")
        .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
        "expected DuplicateBinding for the repeated signature, got {errs:?}"
    );
}

#[test]
fn error_curried_duplicate_body() {
    // A second body line for an already-matched signature is a duplicate
    // declaration, not a fresh orphan-body case.
    let errs = resolve_str("#feature curry\nf: int -> int\nf(x) { x }\nf(x) { x }")
        .expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
        "expected DuplicateBinding for the second body, got {errs:?}"
    );
}

#[test]
fn error_curried_body_clashes_with_global_var() {
    // The signature registers the name in the flat top-level namespace, so a
    // later binding of the same name is the usual name clash (and vice versa:
    // a body line before a `var` of the same name is an orphan body).
    let errs =
        resolve_str("#feature curry\nf: int -> int\nf(x) { x }\nf = 3;").expect_err("should fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
        "expected DuplicateBinding, got {errs:?}"
    );
}

#[test]
fn resolve_curried_import_cross_module() {
    // 0.0.8 T04 (D6): a curried declaration exports exactly like `func` — a
    // flag-off importer sees the signature and calls it (T04 only resolves;
    // the call's arity rule is typeck's, T05).
    use fleen_compiler::lexer::Span;
    use fleen_compiler::parser::ast::ImportDecl;
    use fleen_compiler::resolver::modules::{CrateDef, ModuleDef, ModuleTable};
    use fleen_compiler::source_map::FileId;
    use std::collections::HashMap;

    let mut table = ModuleTable::default();
    table.crates.push(CrateDef::root_crate(FileId(0)));
    table.modules.push(ModuleDef {
        file: FileId(0),
        path: Vec::new(),
        crate_id: 0,
        is_crate_root: true,
        imports: vec![ImportDecl {
            path: vec!["lib".to_string()],
            span: Span::SYNTHETIC,
        }],
        items: HashMap::new(),
    });
    table.modules.push(ModuleDef {
        file: FileId(1),
        path: vec!["lib".to_string()],
        crate_id: 0,
        is_crate_root: false,
        imports: Vec::new(),
        items: {
            // What `exports_of_ast` builds for the flag-on lib module: the
            // signature half exports as `Func`, `pub` from the signature.
            let mut items = HashMap::new();
            items.insert(
                "add".to_string(),
                fleen_compiler::resolver::modules::ExportDef {
                    name: "add".to_string(),
                    is_pub: true,
                    span: Span::SYNTHETIC,
                    kind: fleen_compiler::resolver::modules::ExportKind::Func,
                    enum_variants: Vec::new(),
                    type_param_count: 0,
                },
            );
            items
        },
    });

    // The importing file carries no `#feature` directive (flag-off, D6): it
    // sees the curried declaration as an ordinary function and uses the
    // legacy `func` form it always had.
    let main_ast = parse(tokenize("import lib;\nfunc main() { lib.add(1, 2) }").unwrap())
        .expect("parser should succeed");
    let (main_hir, _, _) =
        resolve_module_with_table(main_ast, &table, 0, 0, 0).expect("should resolve");
    // One import item + the main function declaration.
    assert_eq!(main_hir.items.len(), 2);

    // The exporting side resolves its two-part declaration into an ordinary
    // function the merged HIR carries under its mangled name.
    let lib_ast =
        parse(tokenize("#feature curry\npub add: int -> int -> int\nadd(a, b) { a + b }").unwrap())
            .expect("parser should succeed");
    let (lib_hir, _, _) = resolve_module_with_table(lib_ast, &table, 1, 0, 0)
        .expect("the flag-on lib module should resolve");
    let fleen_compiler::resolver::hir::HirItem::Decl(DeclHir::Func(func)) = &lib_hir.items[0]
    else {
        panic!("expected the curried declaration to resolve into a function")
    };
    assert_eq!(func.name, "lib.add");
    assert_eq!(func.params.len(), 2);
}

// ========== 0.0.9 T01: zero-payload variant patterns are variant references (BUG-2, D1) ==========

#[test]
fn resolve_zero_payload_variant_pattern_repeats() {
    // BUG-2 regression (docs/0.0.7/BUGS.md repro, resolver level): a bare
    // zero-payload variant name inside a payload pattern is a variant
    // reference, not a binding — `when P(None, None)` must not trip
    // `DuplicateBinding` (PLAN D1).
    let hir = resolve_str(
        r#"
enum Option<T> { Some(T), None }
enum Pair<A, B> { P(A, B) }

func main(): int {
    p: Pair<Option<int>, Option<int>> = Pair.P(Option.None, Option.None);
    choose p {
        when P(None, None) { print("empty"); }
        when P(Some(z), Some(w)) { print("both"); }
        when P(Some(z2), None) { print(z2 as string); }
        when P(None, Some(w2)) { print(w2 as string); }
    }
    0
}
"#,
    )
    .expect("zero-payload variant references may repeat within one pattern");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_zero_payload_variant_in_or_pattern() {
    // D1 / PLAN §3.1: or-pattern alternatives compose; `None | Some(0)`
    // binds nothing and resolves.
    let hir = resolve_str(
        r#"
enum Option<T> { Some(T), None }

func main(): int {
    o = Option.Some(1);
    choose o {
        when None | Some(0) { print("zero-ish"); }
        otherwise { print("other"); }
    };
    0
}
"#,
    )
    .expect("or-pattern over a variant reference and a payloadful variant resolves");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn error_repeated_binding_pattern_name_with_local() {
    // D1 lookup order: a local binding of the name wins over the variant, so
    // both occurrences take the binding path and the repetition still trips
    // `DuplicateBinding` (PLAN §9 shadowing fixture).
    let errs = resolve_str(
        r#"
enum Pair<A, B> { P(A, B) }

func main(): int {
    x = 1;
    choose x {
        when P(x, x) { print("same"); }
        otherwise { print("other"); }
    };
    0
}
"#,
    )
    .expect_err("repeated binding names still collide");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
        "expected DuplicateBinding, got {errs:?}"
    );
}

#[test]
fn error_repeated_fresh_binding_pattern_name() {
    // Same rule without a local: the first `z` binds, the second is a
    // duplicate declaration.
    let errs = resolve_str(
        r#"
enum Pair<A, B> { P(A, B) }

func main(): int {
    p = Pair.P(1, 2);
    choose p {
        when P(z, z) { print(z as string); }
        otherwise { print("other"); }
    };
    0
}
"#,
    )
    .expect_err("repeated binding names still collide");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
        "expected DuplicateBinding, got {errs:?}"
    );
}

#[test]
fn resolve_local_shadows_zero_payload_variant_in_pattern() {
    // D1 order: an enclosing local named like a zero-payload variant wins —
    // the pattern takes the binding path (shadowing the local) instead of
    // becoming a variant reference.
    let hir = resolve_str(
        r#"
enum Option<T> { Some(T), None }

func main(): int {
    None = 1;
    choose None {
        when None { print(None as string); }
        otherwise { print("other"); }
    };
    0
}
"#,
    )
    .expect("a visible local binding beats the variant classification");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn error_const_local_shadow_of_variant_name_in_pattern() {
    // The binding path engages the shadow checks a variant reference skips:
    // a `const` local beaten only by the pattern's mutable binding is the
    // usual mutability mismatch (proves the local really won the lookup).
    let errs = resolve_str(
        r#"
enum Option<T> { Some(T), None }

func main(): int {
    const None = 1;
    choose None {
        when None { print("hit"); }
        otherwise { print("other"); }
    };
    0
}
"#,
    )
    .expect_err("const outer + mutable pattern binding is a shadow mismatch");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::ShadowingMutabilityMismatch { .. })),
        "expected ShadowingMutabilityMismatch, got {errs:?}"
    );
}

// ========== Directory-based tests ==========

#[test]
fn resolver_valid_files() {
    let valid_dir = Path::new("../tests/resolver/valid");
    if !valid_dir.exists() {
        return;
    }

    let mut checked = 0;
    for entry in fs::read_dir(valid_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "fln") {
            let source = fs::read_to_string(&path).unwrap();
            let tokens = tokenize(&source).expect("lexer should succeed");
            let ast = parse(tokens).expect("parser should succeed");
            let result = resolve(ast);
            assert!(
                result.is_ok(),
                "Failed to resolve valid file {}: {:?}",
                path.display(),
                result.err()
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no valid test files found");
}

#[test]
fn resolver_invalid_files() {
    let invalid_dir = Path::new("../tests/resolver/invalid");
    if !invalid_dir.exists() {
        return;
    }

    let mut checked = 0;
    for entry in fs::read_dir(invalid_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "fln") {
            let source = fs::read_to_string(&path).unwrap();
            let tokens = tokenize(&source).expect("lexer should succeed");
            let ast = parse(tokens).expect("parser should succeed");
            let result = resolve(ast);
            assert!(
                result.is_err(),
                "Expected resolve error for invalid file {} but got HIR: {:?}",
                path.display(),
                result.ok()
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no invalid test files found");
}

// ========== 0.0.2 U05: `ref` is restricted to parameter position ==========

fn resolve_err_kinds(src: &str) -> Vec<ResolveErrorKind> {
    let tokens = tokenize(src).expect("lexer should succeed");
    let ast = parse(tokens).expect("parser should succeed");
    resolve(ast)
        .expect_err("resolve should fail")
        .into_iter()
        .map(|e| e.kind)
        .collect()
}

#[test]
fn resolve_ref_param_ok() {
    // `ref T` is only allowed in parameter position (PLAN §3.3.1).
    let hir = resolve_str("func shout(s: ref string): int {\n    print(s);\n    42\n}")
        .expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

#[test]
fn resolve_ref_param_nested_rejected() {
    let kinds = resolve_err_kinds("func f(s: ref ref int) {\n}");
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, ResolveErrorKind::RefNotAllowedHere { .. })),
        "expected RefNotAllowedHere, got {kinds:?}"
    );
}

#[test]
fn resolve_ref_local_var_rejected() {
    let kinds = resolve_err_kinds("func go() {\n    x: ref int = 1;\n}");
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, ResolveErrorKind::RefNotAllowedHere { .. })),
        "expected RefNotAllowedHere, got {kinds:?}"
    );
}

#[test]
fn resolve_ref_global_var_rejected() {
    let kinds = resolve_err_kinds("g: ref int = 1;");
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, ResolveErrorKind::RefNotAllowedHere { .. })),
        "expected RefNotAllowedHere, got {kinds:?}"
    );
}

#[test]
fn resolve_ref_return_rejected() {
    let kinds = resolve_err_kinds("func f(): ref int {\n    1\n}");
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, ResolveErrorKind::RefNotAllowedHere { .. })),
        "expected RefNotAllowedHere, got {kinds:?}"
    );
}

// --- ref parameters are read-only: re-declaring the name is an error ---

#[test]
fn resolve_ref_param_rebind_rejected() {
    // `s = "x"` at a `ref` parameter `s`: parameters are immutable, so the
    // resolver would shadow the borrow with a fresh local. DESIGN §10.4
    // forbids assigning to a borrow, so flag it instead (0.0.2 U05).
    let kinds = resolve_err_kinds("func f(s: ref string) {\n    s = \"x\";\n}");
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, ResolveErrorKind::AssignToRefParam { .. })),
        "expected AssignToRefParam, got {kinds:?}"
    );
}

#[test]
fn resolve_ref_param_pattern_shadow_rejected() {
    // A `when s …` pattern binding shadows the borrow the same way.
    let kinds = resolve_err_kinds(
        "func f(s: ref string) {\n    choose s {\n        when s { print(s) }\n        otherwise { print(\"x\") }\n    };\n}",
    );
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, ResolveErrorKind::AssignToRefParam { .. })),
        "expected AssignToRefParam, got {kinds:?}"
    );
}

#[test]
fn resolve_non_ref_param_rebind_ok() {
    // Non-`ref` parameters keep the documented shadow semantics
    // (DESIGN.md §4.3): `x = 2; x` is a fresh local (see the valid fixture
    // `param_shadow_in_body.fln`).
    let hir = resolve_str("func f(x: int): int { x = 2; x }").expect("should resolve");
    assert_eq!(hir.items.len(), 1);
}

// ========== 0.0.6 T03: enum declarations + constructor paths ==========

#[test]
fn enum_decl_collects_into_hir_table() {
    let hir =
        resolve_str("enum Color { Red, Green }\nfunc main(): int = 0").expect("should resolve");
    assert_eq!(hir.enums.len(), 1);
    assert_eq!(hir.enums[0].name, "Color");
    assert_eq!(hir.enums[0].variants.len(), 2);
    assert_eq!(hir.enums[0].variants[0].name, "Red");
    assert_eq!(hir.enums[0].variants[1].name, "Green");
    // Enums are a type-table input; every Decl in `items` is a Func/Var/Const
    // (there is no enum decl variant in DeclHir at all).
    assert!(hir.items.iter().all(|i| match i {
        fleen_compiler::resolver::hir::HirItem::Decl(d) => {
            matches!(d, DeclHir::Func(_) | DeclHir::Var(_) | DeclHir::Const(_))
        }
        _ => true,
    }));
}

#[test]
fn enum_ctor_path_resolves_same_module() {
    let hir =
        resolve_str("enum Color { Red, Green }\nfunc main(): int {\n    x = Color.Red;\n    0\n}")
            .expect("should resolve");
    let mut found = false;
    for item in &hir.items {
        if let fleen_compiler::resolver::hir::HirItem::Decl(decl) = item
            && let DeclHir::Func(f) = decl
            && let fleen_compiler::resolver::hir::FuncBodyHir::Block(b) = &f.body
            && let Some(fleen_compiler::resolver::hir::StmtHir::Decl(d)) = b.stmts.first()
            && let DeclHir::Var(v) = d
            && let ExprHir::EnumCtor {
                enum_name,
                variant_name,
                arity,
                ..
            } = &*v.init
        {
            assert_eq!(enum_name, "Color");
            assert_eq!(variant_name, "Red");
            assert_eq!(*arity, 0);
            found = true;
        }
    }
    assert!(found, "expected an EnumCtor init for Color.Red");
}

#[test]
fn enum_ctor_call_resolves_with_payload_arity() {
    // `Circle(1.5)` parses as a call whose callee is the constructor path.
    let hir = resolve_str(
        "enum Shape { Circle(float) }\nfunc main(): float {\n    c = Shape.Circle(1.5);\n    0.0\n}",
    )
    .expect("should resolve");
    let mut found = false;
    for item in &hir.items {
        if let fleen_compiler::resolver::hir::HirItem::Decl(decl) = item
            && let DeclHir::Func(f) = decl
            && let fleen_compiler::resolver::hir::FuncBodyHir::Block(b) = &f.body
            && let Some(fleen_compiler::resolver::hir::StmtHir::Decl(d)) = b.stmts.first()
            && let DeclHir::Var(v) = d
            && let ExprHir::Call(callee, _) = &*v.init
            && let ExprHir::EnumCtor {
                enum_name,
                variant_name,
                arity,
                ..
            } = &**callee
        {
            assert_eq!(enum_name, "Shape");
            assert_eq!(variant_name, "Circle");
            assert_eq!(*arity, 1);
            found = true;
        }
    }
    assert!(found, "expected Call(EnumCtor(Shape.Circle), ...)");
}

#[test]
fn enum_unknown_variant_is_error() {
    let errs = resolve_str("enum Color { Red }\nfunc main(): int {\n    x = Color.Nope;\n    0\n}")
        .expect_err("unknown variant must fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::UndeclaredVariable { .. })),
        "expected UndeclaredVariable for Color.Nope, got {errs:?}"
    );
}

#[test]
fn enum_duplicate_name_is_error() {
    let errs = resolve_str("enum E { A }\nenum E { B }\nfunc main(): int = 0")
        .expect_err("duplicate enum name must fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
        "expected DuplicateBinding, got {errs:?}"
    );
}

#[test]
fn enum_duplicate_variant_is_error() {
    let errs = resolve_str("enum E { A, A }\nfunc main(): int = 0")
        .expect_err("duplicate variant must fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
        "expected DuplicateBinding for variant A, got {errs:?}"
    );
}

#[test]
fn cross_enum_variant_name_reuse_is_allowed_but_first_enum_wins_bare() {
    // 0.0.6 T04 (D3): two enums may reuse a variant name (uniqueness is only
    // per-enum), so bare `X` is ambiguous by construction. The
    // `variant_to_enum` map is first-wins by declaration order — matching the
    // flat-namespace rule — so the unqualified spelling resolves to `A`, while
    // the qualified spellings `A.X` / `B.X` stay unambiguous.
    // (1) reuse itself is not a DuplicateBinding;
    resolve_str("enum A { X }\nenum B { X }\nfunc main(): int = 0")
        .expect("cross-enum variant-name reuse must be allowed");
    // (2) a bare `X` resolves to the first-declared enum (`A`).
    let hir = resolve_str("enum A { X }\nenum B { X }\nfunc main(): int {\n    x = X;\n    0\n}")
        .expect("bare variant `X` must resolve");
    let mut found = None;
    for item in &hir.items {
        if let fleen_compiler::resolver::hir::HirItem::Decl(decl) = item
            && let DeclHir::Func(f) = decl
            && let fleen_compiler::resolver::hir::FuncBodyHir::Block(b) = &f.body
            && let Some(fleen_compiler::resolver::hir::StmtHir::Decl(d)) = b.stmts.first()
            && let DeclHir::Var(v) = d
            && let ExprHir::EnumCtor {
                enum_name,
                variant_name,
                ..
            } = &*v.init
        {
            assert_eq!(variant_name, "X");
            found = Some(enum_name.clone());
        }
    }
    assert_eq!(
        found.as_deref(),
        Some("A"),
        "bare `X` must resolve to the first-declared enum, got {found:?}"
    );
}

#[test]
fn enum_name_clashes_with_function_is_error() {
    // D3: variant/enum names share the flat identifier namespace with
    // functions — `enum E { A }` after `func E()` is a duplicate.
    let errs = resolve_str("func E(): int = 0\nenum E { A }")
        .expect_err("enum/function name collision must fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
        "expected DuplicateBinding, got {errs:?}"
    );
}

#[test]
fn enum_name_clashes_with_struct_is_error() {
    // D3 / T03 card: struct and enum names share the flat identifier
    // namespace, so an enum may not be named like a struct — in either
    // declaration order. (The module item map is last-wins, so only checking
    // it lets one of the two orders slip through.)
    for src in [
        "struct S { a: int }\nenum S { A }\nfunc main(): int = 0",
        "enum S { A }\nstruct S { a: int }\nfunc main(): int = 0",
    ] {
        let errs = resolve_str(src).expect_err("enum/struct name collision must fail");
        assert!(
            errs.iter()
                .any(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. })),
            "expected DuplicateBinding for {src:?}, got {errs:?}"
        );
    }
}

#[test]
fn enum_forward_reference_resolves() {
    // The constructor path resolves even when the enum is declared after
    // its use (same forward-reference rule as top-level functions).
    let hir =
        resolve_str("func main(): int {\n    x = Color.Red;\n    0\n}\nenum Color { Red, Green }")
            .expect("forward-referenced enum must resolve");
    assert_eq!(hir.enums.len(), 1);
}

#[test]
fn enum_payload_ref_type_is_error() {
    let errs = resolve_str("enum E { A(ref int) }\nfunc main(): int = 0")
        .expect_err("`ref` payload must fail");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::RefNotAllowedHere { .. })),
        "expected RefNotAllowedHere, got {errs:?}"
    );
}

#[test]
fn enum_decl_inside_function_is_parser_error() {
    // Top-level only (mirror struct): the parser rejects it, so the resolver
    // never sees a nested enum.
    let src = "func main(): int {\n    enum E { A }\n    0\n}";
    let tokens = fleen_compiler::lexer::tokenize(src).expect("lexer should succeed");
    assert!(
        fleen_compiler::parser::parse(tokens).is_err(),
        "nested enum must be a parser error"
    );
}

// ========== 0.0.7 T03: parameterized enums (type-param scope + arity) ==========

#[test]
fn param_enum_decl_carries_type_param_count() {
    let hir = resolve_str(
        "enum Option<T> { Some(T), None }\n\
         enum Pair<A, B> { Pair(A, B) }\n\
         enum Shape { Point, Circle(float) }",
    )
    .expect("parameterized and monomorphic enums should resolve");
    let find = |name: &str| {
        hir.enums
            .iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} should be registered"))
    };
    assert_eq!(find("Option").type_params.len(), 1);
    assert_eq!(find("Pair").type_params.len(), 2);
    assert_eq!(find("Shape").type_params.len(), 0);
}

#[test]
fn param_enum_payload_param_shadows_module_item() {
    // A module item named `T` must not capture the declaration-local type
    // parameter: inside `enum Box<T> { Just(T) }` the payload stays the bare
    // parameter name (unmangled), not the struct (T03; D1).
    let hir = resolve_str("struct T { x: int }\nenum Box<T> { Just(T) }")
        .expect("the parameter shadows the module item inside the enum body");
    let b = hir
        .enums
        .iter()
        .find(|e| e.name == "Box")
        .expect("Box should be registered");
    assert_eq!(b.type_params.len(), 1);
    let fleen_compiler::parser::ast::Type::Named { name, args, .. } =
        &b.variants[0].payload_types[0]
    else {
        panic!("expected a named payload type, got {:?}", b.variants[0]);
    };
    assert_eq!(name, "T", "payload must stay the bare parameter name");
    assert!(args.is_empty());
}

#[test]
fn param_enum_duplicate_type_param_is_error() {
    let kinds = resolve_err_kinds("enum Pair<T, T> { Pair(T, T) }");
    assert!(
        kinds
            .iter()
            .any(|k| matches!(k, ResolveErrorKind::DuplicateBinding { .. })),
        "duplicate type parameter must be DuplicateBinding, got {kinds:?}"
    );
}

#[test]
fn param_enum_duplicate_type_param_points_at_first_occurrence() {
    // The duplicate diagnostic names the FIRST occurrence's span (the
    // parameter identifier), not the whole enum head (0.0.7 T03 review).
    let errs = resolve_str("enum Pair<T, T> { Pair(T, T) }")
        .expect_err("duplicate type parameter must be reported");
    let dup = errs
        .iter()
        .find(|e| matches!(e.kind, ResolveErrorKind::DuplicateBinding { .. }))
        .expect("duplicate type parameter must be reported");
    let ResolveErrorKind::DuplicateBinding { first_span, .. } = &dup.kind else {
        unreachable!("matched above")
    };
    assert!(
        first_span.end > first_span.start,
        "first_span must be the first parameter's identifier, not a zero-width or enum-head span"
    );
    assert_ne!(
        *first_span, dup.span,
        "first_span must differ from the duplicate's own span"
    );
}

#[test]
fn param_type_param_not_in_binding_scope() {
    // A type parameter is not a value: after `enum Option<T>`, using `T`
    // as an expression must be an undeclared variable — the parameter
    // never enters the binding scope (0.0.7 T03).
    let kinds = resolve_err_kinds("enum Option<T> { Some(T), None }\nfunc main(): int = T");
    assert!(
        kinds.iter().any(|k| matches!(
            k,
            ResolveErrorKind::UndeclaredVariable { name } if name == "T"
        )),
        "a type parameter must not become a value binding, got {kinds:?}"
    );
}

#[test]
fn param_enum_dotted_ctor_resolves() {
    let hir = resolve_str(
        "enum Option<T> { Some(T), None }\n\
         func main(): int {\n    o = Option.Some(1);\n    0\n}",
    )
    .expect("dotted constructor path on a parameterized enum should resolve");
    assert_eq!(hir.items.len(), 1);
}

/// Two-module table (root + `lib`) with one parameterized enum, for the
/// cross-module arity / visibility tests below.
fn param_enum_table(lib_pub: bool) -> fleen_compiler::resolver::modules::ModuleTable {
    use fleen_compiler::lexer::Span;
    use fleen_compiler::parser::ast::ImportDecl;
    use fleen_compiler::resolver::modules::{
        CrateDef, ExportDef, ExportKind, ModuleDef, ModuleTable,
    };
    use fleen_compiler::source_map::FileId;
    use std::collections::HashMap;

    let mut table = ModuleTable::default();
    table.crates.push(CrateDef::root_crate(FileId(0)));
    table.modules.push(ModuleDef {
        file: FileId(0),
        path: Vec::new(),
        crate_id: 0,
        is_crate_root: true,
        imports: vec![ImportDecl {
            path: vec!["lib".to_string()],
            span: Span::SYNTHETIC,
        }],
        items: HashMap::new(),
    });
    let mut items = HashMap::new();
    items.insert(
        "Wrapper".to_string(),
        ExportDef {
            name: "Wrapper".to_string(),
            is_pub: lib_pub,
            span: Span::SYNTHETIC,
            kind: ExportKind::Enum,
            enum_variants: vec!["Just".to_string()],
            type_param_count: 1,
        },
    );
    table.modules.push(ModuleDef {
        file: FileId(1),
        path: vec!["lib".to_string()],
        crate_id: 0,
        is_crate_root: false,
        imports: Vec::new(),
        items,
    });
    table
}

#[test]
fn param_enum_import_resolves_cross_module() {
    use fleen_compiler::resolver::modules::ROOT_CRATE;
    use fleen_compiler::resolver::resolve_module_with_table;

    let table = param_enum_table(true);
    let main_ast = parse(tokenize("import lib;\nfunc f(w: lib.Wrapper<int>): int = 0").unwrap())
        .expect("parser should succeed");
    let lib_ast =
        parse(tokenize("pub enum Wrapper<T> { Just(T) }").unwrap()).expect("parser should succeed");

    let (main_hir, _, _) = resolve_module_with_table(main_ast, &table, 0, 0, 0)
        .expect("a qualified parameterized-enum use site should resolve");
    // One import item + one function declaration.
    assert_eq!(main_hir.items.len(), 2);

    let (lib_hir, _, _) = resolve_module_with_table(lib_ast, &table, ROOT_CRATE + 1, 0, 0)
        .expect("the lib module should resolve");
    let w = lib_hir
        .enums
        .iter()
        .find(|e| e.name == "lib.Wrapper")
        .expect("Wrapper should be registered");
    assert_eq!(w.type_params.len(), 1, "arity travels with the declaration");
    let fleen_compiler::parser::ast::Type::Named { name, .. } = &w.variants[0].payload_types[0]
    else {
        panic!("expected a named payload type");
    };
    assert_eq!(name, "T", "payload parameter stays declaration-local");
}

#[test]
fn param_enum_private_cross_module_is_error() {
    use fleen_compiler::resolver::resolve_module_with_table;

    let table = param_enum_table(false);
    let main_ast = parse(tokenize("import lib;\nfunc f(w: lib.Wrapper<int>): int = 0").unwrap())
        .expect("parser should succeed");
    let errs = resolve_module_with_table(main_ast, &table, 0, 0, 0)
        .expect_err("a private parameterized enum must not be usable cross-module");
    assert!(
        errs.iter()
            .any(|e| matches!(e.kind, ResolveErrorKind::PrivateAccess { .. })),
        "expected PrivateAccess, got {errs:?}"
    );
}

#[test]
fn resolve_literal_payload_arms_bind_no_names() {
    // T01 checklist: literal payload arms (`when P(1, 1)`) never enter the
    // binding namespace — no `DuplicateBinding`, no bindings — and are
    // untouched by the variant-reference classification, which only
    // considers bare identifiers (PLAN D1).
    let hir = resolve_str(
        r#"
enum Pair<A, B> { P(A, B) }

func main(): int {
    p = Pair.P(1, 1);
    choose p {
        when P(1, 1) { print("ones"); }
        otherwise { print("other"); }
    };
    0
}
"#,
    )
    .expect("literal payload arms bind no names");
    assert_eq!(hir.items.len(), 1);
}

// ========== 0.0.10 T02: the prelude trio (D1/D2) ==========

use fleen_compiler::resolver::hir::{ExprHir, FuncBodyHir, HirItem, PatternHir};

#[test]
fn prelude_ctor_patterns_classify_to_result_ctor() {
    // D1: `when Ok(v)` / `when Err(e)` parse as ordinary constructor
    // patterns and the resolver classifies them back onto the
    // `PatternHir::ResultCtor` nodes (resolver/hir.rs) through the 0.0.9 D1
    // variant-reference machinery — typeck/lower churn stays zero.
    let hir = resolve_str(
        r#"
func main(): int {
    choose 1 { when Ok(v) { 1 } when Err(e) { 2 } }
}
"#,
    )
    .expect("prelude ctor patterns resolve");
    let Some(HirItem::Decl(DeclHir::Func(func))) = hir.items.first() else {
        panic!("expected a func item");
    };
    let FuncBodyHir::Block(body) = &func.body else {
        panic!("expected a block body");
    };
    let Some(tail) = body.tail_expr.as_ref() else {
        panic!("expected a choose tail expression");
    };
    let ExprHir::Choose(choose) = tail.as_ref() else {
        panic!("expected a choose expression");
    };
    use fleen_compiler::parser::ast::ResultCtor;
    assert!(
        matches!(
            choose.arms[0].pattern,
            PatternHir::ResultCtor {
                ctor: ResultCtor::Ok,
                ..
            }
        ),
        "arm 0 must classify as the Ok ResultCtor, got {:?}",
        choose.arms[0].pattern
    );
    assert!(
        matches!(
            choose.arms[1].pattern,
            PatternHir::ResultCtor {
                ctor: ResultCtor::Err,
                ..
            }
        ),
        "arm 1 must classify as the Err ResultCtor, got {:?}",
        choose.arms[1].pattern
    );
}

#[test]
fn prelude_result_type_use_maps_to_internal_result() {
    // D1: `Result<T, E>` parses as an ordinary parameterized type use and
    // the resolver maps the prelude name back onto the internal
    // `ast::Type::Result` — typeck/lower/codegen see the pre-migration
    // shape in every type position (annotations, params, returns, struct
    // fields, enum payloads, casts all flow through `resolve_type`).
    use fleen_compiler::parser::ast::{BaseType, Type};
    let hir = resolve_str("func f(a: int): Result<int, string> = Ok(a);")
        .expect("prelude Result type use resolves");
    let Some(HirItem::Decl(DeclHir::Func(func))) = hir.items.first() else {
        panic!("expected a func item");
    };
    match &func.ret_type {
        Some(Type::Result(ok, err)) => {
            assert!(matches!(ok.as_ref(), Type::Base(BaseType::Int)));
            assert!(matches!(err.as_ref(), Type::Base(BaseType::String)));
        }
        other => panic!("expected the internal Type::Result, got {:?}", other),
    }
}

#[test]
fn struct_field_of_result_type_maps_to_internal_result() {
    // D1 mapping coverage: struct field types flow through the same
    // `resolve_type` Named arm (T03's `FieldTypeTag::Result = 6` parity).
    use fleen_compiler::parser::ast::{BaseType, Type};
    let hir = resolve_str(
        r#"
struct Holder {
    r: Result<int, string>,
}
func main(): int { 0 }
"#,
    )
    .expect("struct with a Result field resolves");
    let field = &hir.structs[0].fields[0];
    assert_eq!(field.name, "r");
    match &field.ty {
        Type::Result(ok, err) => {
            assert!(matches!(ok.as_ref(), Type::Base(BaseType::Int)));
            assert!(matches!(err.as_ref(), Type::Base(BaseType::String)));
        }
        other => panic!("expected the internal Type::Result, got {:?}", other),
    }
}

#[test]
fn async_result_return_maps_to_internal_result() {
    // T03 (review follow-up): an **async** function's declared `Result<T, E>`
    // return type also lands as the internal `ast::Type::Result`, so the
    // resolver's return-type `type_contains_ref` guard walks the mapped shape
    // on the async side too (the plain-return and struct-field positions are
    // pinned by the two tests above).
    use fleen_compiler::parser::ast::{BaseType, Type};
    use fleen_compiler::resolver::hir::HirItem;
    let hir = resolve_str(
        r#"
async func afar(n: int): Result<int, string> {
    Ok(n)
}
func main(): int { 0 }
"#,
    )
    .expect("async func with a prelude Result return resolves");
    let afar = hir
        .items
        .iter()
        .filter_map(|item| match item {
            HirItem::Decl(DeclHir::Func(f)) => Some(f),
            _ => None,
        })
        .find(|f| f.is_async)
        .expect("the async func resolves");
    match &afar.ret_type {
        Some(Type::Result(ok, err)) => {
            assert!(matches!(ok.as_ref(), Type::Base(BaseType::Int)));
            assert!(matches!(err.as_ref(), Type::Base(BaseType::String)));
        }
        other => panic!("expected the internal Type::Result, got {:?}", other),
    }
}

#[test]
fn prelude_trio_available_without_import() {
    // D2: `Ok` / `Err` / `Result` are prelude names — available in every
    // file without `import std.result;` (`import std.result;` also works
    // and stays the documented explicit spelling).
    let hir = resolve_str(
        r#"
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("zero") } else { Ok(a / b) }
}
func main(): int {
    q = div(4, 2)?;
    choose div(1, 0) {
        when Ok(v) { v }
        otherwise { 2 }
    };
    q
}
"#,
    )
    .expect("the prelude trio resolves without any import");
    assert_eq!(hir.items.len(), 2);
}

#[test]
fn prelude_ctor_multi_payload_falls_to_enum_path() {
    // 0.0.10 T02 (recorded stage shift): `Ok(a, b)` parses as an ordinary
    // two-payload constructor pattern; the prelude classification takes
    // exactly one payload, so the arm falls through to the generic enum-
    // pattern path and typeck reports the mismatch against the scrutinee.
    // Resolution itself succeeds (the rejection moves to typeck).
    let hir = resolve_str(
        r#"
func main(): int {
    choose 1 { when Ok(a, b) { 1 } otherwise { 2 } }
}
"#,
    )
    .expect("multi-payload Ok resolves to the generic enum-pattern path");
    let Some(HirItem::Decl(DeclHir::Func(func))) = hir.items.first() else {
        panic!("expected a func item");
    };
    let FuncBodyHir::Block(body) = &func.body else {
        panic!("expected a block body");
    };
    let Some(tail) = body.tail_expr.as_ref() else {
        panic!("expected a choose tail expression");
    };
    let ExprHir::Choose(choose) = tail.as_ref() else {
        panic!("expected a choose expression");
    };
    assert!(
        matches!(&choose.arms[0].pattern, PatternHir::Enum { name, .. } if name == "Ok"),
        "a two-payload `Ok` stays the generic Enum pattern, got {:?}",
        choose.arms[0].pattern
    );
}

#[test]
fn reserved_ctor_name_rejections() {
    // D2: declaring a binding named `Ok` / `Err` is a ReservedBuiltinName
    // rejection (behavior flip from the pre-0.0.10 §4.3 shadow — recorded
    // in the T02 card).
    for (src, name) in [
        ("func Ok(x: int): int = x", "Ok"),
        ("Ok = 5;", "Ok"),
        ("const Err = 5;", "Err"),
        ("func f(Ok: int): int = Ok", "Ok"),
        (
            "func main(): int {\n    r = choose 5 { when Ok { 1 } otherwise { 2 } }\n    r\n}",
            "Ok",
        ),
        ("for Ok in [1] { }", "Ok"),
    ] {
        let errs = resolve_str(src).expect_err("the declaration must be rejected");
        assert!(
            errs.iter().any(|e| matches!(
                e.kind,
                ResolveErrorKind::ReservedBuiltinName { name: ref n } if n == name
            )),
            "src {src:?}: expected ReservedBuiltinName({name}), got {errs:?}"
        );
    }
}

#[test]
fn reserved_for_var_error_points_at_the_loop_variable() {
    // 0.0.10 T02 review: `ExprFor.var_span` lets the `ReservedBuiltinName`
    // diagnostic point at the loop variable itself, not the whole `for`
    // expression.
    let src = "for Ok in [1] { }";
    let errs = resolve_str(src).expect_err("the declaration must be rejected");
    let err = errs
        .iter()
        .find(|e| matches!(e.kind, ResolveErrorKind::ReservedBuiltinName { .. }))
        .expect("the reserved-name error must be present");
    assert_eq!(
        (err.span.start, err.span.end),
        (4, 6),
        "the diagnostic must point at the `Ok` identifier itself"
    );
}
