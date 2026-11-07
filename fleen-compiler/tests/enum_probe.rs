//! Enum support tests (0.0.6 T02/T03).
//!
//! Originally a probe that printed the *current* behavior before the enum
//! work landed; now a regression suite. The resolver (T03) resolves enum
//! declarations and constructor paths — the typing of a constructor is
//! T04's job, so the assertions here are about **resolution**: which
//! programs reach the typeck stage and which are rejected by the resolver.

use fleen_compiler::Session;

fn compile_sources(sources: &[(&str, &str)]) -> Result<(), String> {
    let mut session = Session::new();
    let mut root = None;
    for (name, src) in sources {
        let id = session.add_file(*name, *src);
        if root.is_none() {
            root = Some(id);
        }
    }
    let root = root.expect("at least one source");
    session
        .compile_project(root)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// The error text of a failed compilation, or `None` when it compiled.
fn error_text(sources: &[(&str, &str)]) -> Option<String> {
    compile_sources(sources).err()
}

#[test]
fn same_module_enum_variant_resolves() {
    // `Color.Red` resolves to the registered variant (the failure, if any,
    // is the typeck stage — T04 — never a resolver "undeclared variable").
    let err = error_text(&[(
        "main.fln",
        r#"enum Color { Red, Green }
func main(): int {
    x = Color.Red;
    0
}"#,
    )]);
    assert!(
        !err.as_deref()
            .unwrap_or_default()
            .contains("undeclared variable `Color`"),
        "the enum name must resolve, got: {err:?}"
    );
}

#[test]
fn duplicate_enum_and_struct_name_is_rejected() {
    // `enum E { A }` + `struct E {}` share the flat identifier namespace
    // (D3): the collision is a compile error at registration.
    let err = error_text(&[(
        "main.fln",
        r#"enum E { A }
struct E { x: int }
func main(): int { 0 }"#,
    )]);
    assert!(
        err.as_deref().unwrap_or_default().contains("duplicate"),
        "expected a duplicate-definition error, got: {err:?}"
    );
}

#[test]
fn variant_named_ok_is_allowed() {
    // A variant may reuse a builtin name (`Ok`) — the variant namespace is
    // per-enum, so it does not shadow the Result constructor.
    let err = error_text(&[(
        "main.fln",
        r#"enum F { Ok }
func main(): int {
    x = F.Ok;
    0
}"#,
    )]);
    assert!(
        !err.as_deref()
            .unwrap_or_default()
            .contains("undeclared variable `F`"),
        "the enum name must resolve, got: {err:?}"
    );
}

#[test]
fn enum_payload_bare_types_resolve() {
    // Payload types go through the resolver's type resolution.
    let err = error_text(&[(
        "main.fln",
        r#"enum G { A(string), B(int) }
func main(): int {
    x = G.A;
    0
}"#,
    )]);
    assert!(
        !err.as_deref()
            .unwrap_or_default()
            .contains("undeclared variable `G`"),
        "the enum name must resolve, got: {err:?}"
    );
}

#[test]
fn cross_module_enum_variant_resolves() {
    // `pub enum` is importable cross-module; `shapes.Color.Red` resolves.
    let err = error_text(&[
        (
            "main.fln",
            r#"import shapes;
func main(): int {
    x = shapes.Color.Red;
    0
}"#,
        ),
        ("shapes/mod.fln", r#"pub enum Color { Red, Green }"#),
    ]);
    assert!(
        !err.as_deref()
            .unwrap_or_default()
            .contains("undeclared variable `shapes`"),
        "the cross-module enum path must resolve, got: {err:?}"
    );
}

#[test]
fn cross_module_private_enum_is_rejected() {
    // A non-`pub` enum is module-local: cross-module access is a
    // `PrivateAccess` error.
    let err = error_text(&[
        (
            "main.fln",
            r#"import shapes;
func main(): int {
    x = shapes.Color.Red;
    0
}"#,
        ),
        ("shapes/mod.fln", r#"enum Color { Red, Green }"#),
    ]);
    assert!(
        err.as_deref().unwrap_or_default().contains("private"),
        "expected a private-access error, got: {err:?}"
    );
}

#[test]
fn enum_pattern_in_choose_is_still_parser_rejected() {
    // `when Color.Red { … }` — the pattern grammar does not accept a dotted
    // spelling yet (T05's scope is the pattern *matching*, not the syntax).
    // What matters here is that it is not silently accepted as something
    // else.
    let err = error_text(&[(
        "main.fln",
        r#"enum Color { Red, Green }
func main(): int {
    choose Color.Red {
        when Color.Red { 1 }
        when Color.Green { 2 }
    }
}"#,
    )]);
    assert!(err.is_some(), "the dotted pattern must not compile yet");
}
