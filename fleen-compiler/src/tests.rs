//! Unit tests for the top-level `compile` pipeline entry point.

use super::{CompileError, compile};

#[test]
fn compiles_valid_source() {
    let module = compile("func main(): int = 42").unwrap();
    assert_eq!(module.version, 2);
    assert!(module.functions.iter().any(|f| f.params == 0));
}

#[test]
fn stops_at_first_failing_stage() {
    // Lex failure: an invalid character aborts before parsing.
    assert!(matches!(
        compile("func main(): int = 1 $ 2"),
        Err(CompileError::Lex(_))
    ));

    // Parse failure on well-lexed input.
    assert!(matches!(
        compile("func main(): int {"),
        Err(CompileError::Parse(_))
    ));

    // Resolve failure: use of an undefined name.
    assert!(matches!(
        compile("func main(): int = no_such_name"),
        Err(CompileError::Resolve(_))
    ));

    // Typeck failure: return type mismatch.
    assert!(matches!(
        compile("func main(): int = true"),
        Err(CompileError::Typeck(_))
    ));

    // Codegen failure: no `main` function.
    assert!(matches!(
        compile("func not_main(): int = 1"),
        Err(CompileError::Codegen(_))
    ));
}

#[test]
fn error_display_names_the_stage() {
    let err = compile("func main(): int = true").unwrap_err();
    assert!(err.to_string().starts_with("typeck: "), "got: {err}");

    let err = compile("func main(): int = no_such_name").unwrap_err();
    assert!(err.to_string().starts_with("resolve: "), "got: {err}");
}
