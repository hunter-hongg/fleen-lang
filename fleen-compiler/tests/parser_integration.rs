//! Integration tests for the parser using test files.

use fleen_compiler::lexer::tokenize;
use fleen_compiler::parser::parse;
use std::fs;
use std::path::Path;

#[test]
fn parser_valid_files() {
    let valid_dir = Path::new("../tests/parser/valid");
    if !valid_dir.exists() {
        return;
    }

    for entry in fs::read_dir(valid_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "fln") {
            let source = fs::read_to_string(&path).unwrap();
            let tokens = tokenize(&source).expect("lexer should succeed");
            let result = parse(tokens);
            assert!(
                result.is_ok(),
                "Failed to parse valid file {}: {:?}",
                path.display(),
                result.err()
            );
        }
    }
}

#[test]
fn parser_invalid_files() {
    let invalid_dir = Path::new("../tests/parser/invalid");
    if !invalid_dir.exists() {
        return;
    }

    for entry in fs::read_dir(invalid_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "fln") {
            let source = fs::read_to_string(&path).unwrap();
            let tokens = tokenize(&source).expect("lexer should succeed");
            let result = parse(tokens);
            assert!(
                result.is_err(),
                "Expected error for invalid file {} but got AST: {:?}",
                path.display(),
                result.ok()
            );
            // Pin the error kind: an optional `<stem>.err` companion holds a
            // distinctive substring of the expected error's Display output
            // (mirrors the e2e invalid fixtures' `.exit`/`.stderr` pattern),
            // so a fixture only passes with the *intended* error.
            let err = result.err().unwrap();
            let err_path = path.with_extension("err");
            if err_path.exists() {
                let expected = fs::read_to_string(&err_path).unwrap();
                assert!(
                    format!("{err}").contains(expected.trim()),
                    "Error for {} does not match {}: got {}",
                    path.display(),
                    err_path.display(),
                    err
                );
            }
        }
    }
}
