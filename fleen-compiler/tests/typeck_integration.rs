//! Integration tests for the type checker.
//!
//! Every `.fln` file in `tests/typeck/valid/` must typeck successfully,
//! every file in `tests/typeck/invalid/` must fail.

use fleen_compiler::lexer::tokenize;
use fleen_compiler::parser::parse;
use fleen_compiler::resolver::resolve;
use fleen_compiler::typeck::typeck;
use std::fs;
use std::path::Path;

fn typeck_str(
    src: &str,
) -> Result<
    fleen_compiler::typeck::typed_hir::TypedHir,
    Vec<fleen_compiler::typeck::error::TypeckError>,
> {
    let tokens = tokenize(src).expect("lexer should succeed");
    let ast = parse(tokens).expect("parser should succeed");
    let hir = resolve(ast).expect("resolver should succeed");
    typeck(hir)
}

#[test]
fn typeck_valid_files() {
    let valid_dir = Path::new("../tests/typeck/valid");
    assert!(valid_dir.exists(), "missing {}", valid_dir.display());

    let mut checked = 0;
    for entry in fs::read_dir(valid_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "fln") {
            let source = fs::read_to_string(&path).unwrap();
            let result = typeck_str(&source);
            assert!(
                result.is_ok(),
                "Failed to typeck valid file {}: {:?}",
                path.display(),
                result.err()
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no valid test files found");
}

#[test]
fn typeck_invalid_files() {
    let invalid_dir = Path::new("../tests/typeck/invalid");
    assert!(invalid_dir.exists(), "missing {}", invalid_dir.display());

    let mut checked = 0;
    for entry in fs::read_dir(invalid_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "fln") {
            let source = fs::read_to_string(&path).unwrap();
            let errs = match typeck_str(&source) {
                Ok(_) => panic!(
                    "Expected typeck error for invalid file {} but got TypedHir",
                    path.display()
                ),
                Err(errs) => errs,
            };
            if let Some(expected) = expected_error_kind(&source) {
                assert!(
                    errs.iter()
                        .any(|e| format!("{:?}", e.kind).starts_with(expected)),
                    "Expected {expected} for {} but got {errs:?}",
                    path.display()
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no invalid test files found");
}

/// Optional `// expect-typeck: <Kind>` directive; when present, at least one
/// reported error must be that `TypeckErrorKind` variant (matched by its
/// `Debug` name). Fixtures without the directive only assert failure.
fn expected_error_kind(source: &str) -> Option<&str> {
    source.lines().find_map(|line| {
        line.trim()
            .strip_prefix("// expect-typeck:")
            .map(str::trim)
            .filter(|kind| !kind.is_empty())
    })
}

#[test]
fn enum_constructor_types_against_the_declaration() {
    // 0.0.6 T04: the enum table is live, so a dotted constructor path
    // (`Color.Red`) types to the enum itself; the T03-era
    // `UnsupportedFeature` stub is retired.
    typeck_str("enum Color { Red, Green }\nfunc main(): Color {\n    Color.Red\n}")
        .expect("a zero-payload dotted constructor must type to the enum");
}

#[test]
fn enum_constructor_payload_variant_used_bare_is_arity_error() {
    // 0.0.6 T04 (D3): a payload variant without call parentheses cannot
    // produce a value — the payload form is constructed through a call.
    let errs =
        typeck_str("enum Shape { Point, Circle(float) }\nfunc main(): Shape {\n    Circle\n}")
            .expect_err("a payload variant used bare must be an arity error");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            fleen_compiler::typeck::error::TypeckErrorKind::ArityMismatch { .. }
        )),
        "expected ArityMismatch for bare `Circle`, got {errs:?}"
    );
}

#[test]
fn enum_constructor_payload_type_mismatch_is_rejected() {
    // 0.0.6 T04: payload types unify exactly against the declaration.
    let errs =
        typeck_str("enum Shape { Point, Circle(float) }\nfunc main(): Shape {\n    Circle(1)\n}")
            .expect_err("an int payload against a float variant must fail");
    assert!(
        !errs.is_empty(),
        "expected a type error for Circle(1), got none"
    );
}

#[test]
fn recursive_enum_is_rejected_directly() {
    // 0.0.6 T04 (D2): a payload that reaches the enum being declared is a
    // `RecursiveEnum` error carrying the full path.
    let errs = typeck_str("enum E { A, B(E) }\nfunc main(): int = 0")
        .expect_err("a directly recursive enum must be rejected");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            fleen_compiler::typeck::error::TypeckErrorKind::RecursiveEnum { .. }
        )),
        "expected RecursiveEnum, got {errs:?}"
    );
}

#[test]
fn recursive_enum_is_rejected_via_array() {
    // 0.0.6 T04 (D2): the cycle check sees through `[T]` nesting.
    let errs = typeck_str("enum E { A, B([E]) }\nfunc main(): int = 0")
        .expect_err("an array-mediated recursive enum must be rejected");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            fleen_compiler::typeck::error::TypeckErrorKind::RecursiveEnum { .. }
        )),
        "expected RecursiveEnum via [T], got {errs:?}"
    );
}

#[test]
fn recursive_enum_is_rejected_via_box() {
    // 0.0.6 T04 (D2): `box` indirection does not lift the ban in 0.0.6.
    let errs = typeck_str("enum E { A, B(box<E>) }\nfunc main(): int = 0")
        .expect_err("a box-mediated recursive enum must be rejected");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            fleen_compiler::typeck::error::TypeckErrorKind::RecursiveEnum { .. }
        )),
        "expected RecursiveEnum via box, got {errs:?}"
    );
}

#[test]
fn ptr_enum_payload_is_not_storable() {
    // 0.0.6 T04 (D1): payload storage follows the struct-field rules — a
    // `ptr<T>` payload is rejected (`PtrNotStorable` family).
    let errs = typeck_str("enum E { A, B(ptr<int>) }\nfunc main(): int = 0")
        .expect_err("a ptr payload must be rejected");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            fleen_compiler::typeck::error::TypeckErrorKind::PtrNotStorable
        )),
        "expected PtrNotStorable, got {errs:?}"
    );
}

#[test]
fn eq_on_enum_values_is_rejected() {
    // 0.0.6 T04 (D3): enums have no `==` — matching via `choose` is the
    // comparison story.
    let errs = typeck_str(
        "enum Color { Red, Green }\nfunc main(): bool {\n    a = Color.Red;\n    b = Color.Green;\n    a == b\n}",
    )
    .expect_err("`==` on enum values must be rejected");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            fleen_compiler::typeck::error::TypeckErrorKind::InvalidOperand { .. }
        )),
        "expected InvalidOperand for `==` on enums, got {errs:?}"
    );
}

#[test]
fn cast_enum_to_string_is_rejected() {
    // 0.0.6 T04: `as string` stays scalar-only; printing an enum requires a
    // `choose` projection.
    let errs = typeck_str(
        "enum Color { Red, Green }\nfunc main() {\n    c = Color.Red;\n    print(c as string);\n}",
    )
    .expect_err("`as string` on an enum must be rejected");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            fleen_compiler::typeck::error::TypeckErrorKind::UnsupportedCast { .. }
        )),
        "expected UnsupportedCast, got {errs:?}"
    );
}

#[test]
fn owned_enum_requires_explicit_transfer() {
    // 0.0.6 T04 (D1): an enum with any owned payload is an owned type — a
    // consuming read needs `move` / `clone`.
    let errs = typeck_str(
        "enum Op { Add, Tag(string) }\nfunc consume(x: Op): int = 0\nfunc main(): int {\n    o = Tag(\"a\");\n    consume(o)\n}",
    )
    .expect_err("a bare owned enum read at a consuming position must fail");
    assert!(
        errs.iter().any(|e| matches!(
            e.kind,
            fleen_compiler::typeck::error::TypeckErrorKind::OwnedArgRequiresMove { .. }
        )),
        "expected OwnedArgRequiresMove, got {errs:?}"
    );
}
