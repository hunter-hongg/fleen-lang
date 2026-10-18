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

#[test]
fn compile_project_merges_modules() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import net.tcp;\nfunc main() { net.tcp.say(); }",
    );
    session.add_file("src/net/tcp.fln", "pub func say() { print(\"hi\"); }");
    let module = session.compile_project(root).unwrap();
    assert_eq!(module.version, 3);
}

#[test]
fn compile_project_reads_imported_owned_global() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import m;\n\
         func main() {\n\
             print(m.greeting);\n\
             n = 0;\n\
             while n < 2 and m.items[0] < 100 {\n\
                 n = n + 1;\n\
             };\n\
             print(n as string);\n\
         }",
    );
    session.add_file(
        "src/m/mod.fln",
        "pub const greeting: string = \"hello\";\n\
         pub items: [int] = [10, 20, 30];\n",
    );
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn compile_project_cross_module_global_assignment() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import m;\nfunc main() { m.items = [10, 20]; print(m.items[0] as string); }\n",
    );
    session.add_file("src/m/mod.fln", "pub items: [int] = [1, 2, 3];\n");
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn compile_project_struct_return_type_is_qualified() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import net.tcp;\nfunc main() { p = net.tcp.make(); print(p.x as string); }\n",
    );
    session.add_file(
        "src/net/tcp.fln",
        "pub struct Point { x: int, y: int }\n\
         pub func make(): Point = Point { x: 7, y: 8 }\n",
    );
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn compile_project_rejects_move_of_imported_global() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import m;\nfunc main() {\n    move m.s;\n}\n",
    );
    session.add_file("src/m/mod.fln", "pub const s: string = \"hello\";\n");
    assert!(matches!(
        session.compile_project(root),
        Err(CompileError::Typeck(_))
    ));
}

#[test]
fn compile_project_reports_cycle() {
    let mut session = Session::new();
    let root = session.add_file("src/main.fln", "import net.a;\nfunc main(): int = 0\n");
    session.add_file("src/net/a.fln", "import net.b;\npub func af(): int = 0\n");
    session.add_file("src/net/b.fln", "import net.a;\npub func bf(): int = 0\n");
    assert!(matches!(
        session.compile_project(root),
        Err(CompileError::Resolve(_))
    ));
}

#[test]
fn compile_project_rejects_private_func() {
    let mut session = Session::new();
    let root = session.add_file("src/main.fln", "import m;\nfunc main() { m.hidden(); }\n");
    session.add_file("src/m/mod.fln", "func hidden() { print(\"x\"); }\n");
    assert!(matches!(
        session.compile_project(root),
        Err(CompileError::Resolve(_))
    ));
}

#[test]
fn compile_project_rejects_private_const() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import m;\nfunc main() { print(m.hidden); }\n",
    );
    session.add_file("src/m/mod.fln", "const hidden: string = \"x\";\n");
    assert!(matches!(
        session.compile_project(root),
        Err(CompileError::Resolve(_))
    ));
}

#[test]
fn compile_project_rejects_private_global() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import m;\nfunc main() { print(m.hidden); }\n",
    );
    session.add_file("src/m/mod.fln", "hidden: string = \"x\";\n");
    assert!(matches!(
        session.compile_project(root),
        Err(CompileError::Resolve(_))
    ));
}

#[test]
fn compile_project_rejects_private_struct_type() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import m;\nfunc main() { x: m.Hidden = 0; }\n",
    );
    session.add_file("src/m/mod.fln", "struct Hidden { a: int }\n");
    assert!(matches!(
        session.compile_project(root),
        Err(CompileError::Resolve(_))
    ));
}
