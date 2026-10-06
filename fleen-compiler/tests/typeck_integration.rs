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
            let result = typeck_str(&source);
            assert!(
                result.is_err(),
                "Expected typeck error for invalid file {} but got TypedHir",
                path.display()
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no invalid test files found");
}
