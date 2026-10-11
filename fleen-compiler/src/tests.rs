//! Unit tests for the top-level `compile` / `Session` pipeline entry points.

use super::{CompileError, FileId, Session, compile};

#[test]
fn compiles_valid_source() {
    let module = compile("func main(): int = 42").unwrap();
    assert_eq!(module.version, 3);
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

// ---------------------------------------------------------------------
// Session (0.0.3 V01): multi-file registration, per-file compilation
// ---------------------------------------------------------------------

#[test]
fn session_compiles_each_registered_file() {
    let mut session = Session::new();
    let a = session.add_file("a.fln", "func main(): int = 42");
    let b = session.add_file("b.fln", "func main(): int = 7");
    assert_eq!(session.compile(a).unwrap().version, 3);
    assert_eq!(session.compile(b).unwrap().version, 3);
}

#[test]
fn parse_error_span_names_the_failing_file() {
    // Two files registered; the error span must carry the id of the file
    // that was compiled, not of the first registered one.
    let mut session = Session::new();
    let good = session.add_file("good.fln", "func main(): int = 42");
    let bad = session.add_file("bad.fln", "func main(): int = ");
    assert!(session.compile(good).is_ok());
    match session.compile(bad) {
        Err(CompileError::Parse(e)) => assert_eq!(e.span.file, FileId(1)),
        other => panic!("expected parse error, got {other:?}"),
    }
}

#[test]
fn single_file_compile_registers_file_zero() {
    // The `compile(&str)` convention: the one file is `FileId(0)`, which is
    // what every pre-0.0.3 span (and serialized output) assumes.
    let mut session = Session::new();
    let root = session.add_file("source", "func main(): int = 42");
    assert_eq!(root, FileId(0));
    assert!(session.compile(root).is_ok());
}

#[test]
#[should_panic(expected = "not registered")]
fn compiling_unregistered_root_panics() {
    // Documented panic: an unregistered FileId is an internal invariant
    // violation (spans only ever come from lexing a registered file).
    let session = Session::new();
    let _ = session.compile(FileId(7));
}
