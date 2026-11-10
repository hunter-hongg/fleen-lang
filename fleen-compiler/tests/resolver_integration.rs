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

use fleen_compiler::resolver::hir::ExprHir;

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
    assert_eq!(find("Option").type_param_count, 1);
    assert_eq!(find("Pair").type_param_count, 2);
    assert_eq!(find("Shape").type_param_count, 0);
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
    assert_eq!(b.type_param_count, 1);
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
    assert_eq!(w.type_param_count, 1, "arity travels with the declaration");
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
