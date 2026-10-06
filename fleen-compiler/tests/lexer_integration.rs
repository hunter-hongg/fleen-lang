//! Integration tests for the lexer using test files.

use fleen_compiler::lexer::tokenize;
use std::fs;
use std::path::Path;

#[test]
fn lexer_valid_files() {
    let valid_dir = Path::new("../tests/lexer/valid");
    if !valid_dir.exists() {
        return;
    }

    for entry in fs::read_dir(valid_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "fln") {
            let source = fs::read_to_string(&path).unwrap();
            let result = tokenize(&source);
            assert!(
                result.is_ok(),
                "Failed to tokenize valid file {}: {:?}",
                path.display(),
                result.err()
            );
        }
    }
}

#[test]
fn lexer_invalid_files() {
    let invalid_dir = Path::new("../tests/lexer/invalid");
    if !invalid_dir.exists() {
        return;
    }

    for entry in fs::read_dir(invalid_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "fln") {
            let source = fs::read_to_string(&path).unwrap();
            let result = tokenize(&source);
            assert!(
                result.is_err(),
                "Expected error for invalid file {} but got tokens: {:?}",
                path.display(),
                result.ok()
            );
        }
    }
}
