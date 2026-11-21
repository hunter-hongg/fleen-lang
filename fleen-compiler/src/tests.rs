//! Unit tests for the top-level `compile` / `Session` pipeline entry points.

use super::{CompileError, FileId, Session, compile};
use crate::codegen;
use crate::lexer::{TokenKind, tokenize};
use crate::parser::ParseErrorKind;
use crate::resolver::error::ResolveErrorKind;
use crate::resolver::modules::build_module_table;
use crate::resolver::{CrateDef, ExportKind, STD_CRATE_NAME, STD_RESULT_MODULE_NAME};
use crate::source_map::SourceMap;
use crate::typeck::error::TypeckErrorKind;

#[test]
fn compiles_valid_source() {
    let module = compile("func main(): int = 42").unwrap();
    assert_eq!(module.version, codegen::MODULE_VERSION);
    assert!(module.functions.iter().any(|f| f.params == 0));
}

// ========== 0.0.10 T01: the std `Result` declaration module (D1) ==========

/// `import std.result;` compiles end to end through the legacy user-side
/// path (`Ok` / `Err` constructors, `Result<T, E>` in type position, the
/// attempt-chain `choose`), and the compiled module carries **no
/// `enum_defs` entry** — the declaration is declaration-only (D1): the
/// compiler maps `std.result.Result` onto its internal builtin
/// representation and the runtime keeps the 0.0.2 encoding. Recorded
/// expectation (0.0.9 T05 posture, not a hash gate): std today declares no
/// enums anywhere, so an std-using module's `enum_defs` stays empty; std's
/// other modules' bytecode being unchanged is pinned by the e2e/std stdout
/// fixtures, which must stay green throughout.
#[test]
fn std_result_import_compiles_with_no_enum_defs() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        r#"
import std.result;
import std.io;

func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func main(): int {
    r = choose div(10, 2) {
        when Ok(v) { v }
        otherwise { 0 - 1 }
    };
    std.io.println(r as string);
    0
}
"#,
    );
    let module = session.compile(root).expect("should compile");
    assert!(
        module.enum_defs.is_empty(),
        "std.result.Result must not reach enum_defs (D1 declaration-only)"
    );
    assert_eq!(
        module.version,
        codegen::MODULE_VERSION,
        "no bytecode change (D7): the module stays at the content-driven version"
    );
}

/// An `enum Result` in a user project is rejected at compile time (D2
/// groundwork): only `std/result.fln` may declare it.
#[test]
fn user_enum_named_result_is_a_compile_error() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "enum Result<T, E> { Ok(T), Err(E) }\nfunc main(): int = 0\n",
    );
    let err = session
        .compile(root)
        .expect_err("an `enum Result` outside std.result must be rejected");
    assert!(matches!(
        err,
        CompileError::Resolve(ref errors) if errors.iter().any(|e| matches!(
            &e.kind,
            ResolveErrorKind::ReservedBuiltinName { name } if name == "Result"
        ))
    ));
}

/// The T01 ticket's "yields the `Ok` / `Err` / `Result` names" pin, at the
/// table level: the export `import std.result;` hands the importer carries
/// the `pub` enum `Result` with its variant names travelling alongside
/// (0.0.6 T03 export machinery) and the fully-applied-use-site arity
/// (0.0.7 D1). The Session test above exercises the same names only end to
/// end — there `Ok` / `Err` / `Result` would resolve through the builtin
/// parser paths even without the import, so this is the assertion that
/// actually ties the export to the import.
#[test]
fn std_result_module_exports_result_with_ok_err_variants() {
    let mut sources = SourceMap::new();
    let root = sources.add_file("src/main.fln", "import std.result;\nfunc main(): int = 0\n");
    let sources = super::with_std_sources(&sources).expect("std sources resolve");
    let asts = super::parse_all(&sources).expect("std sources parse");
    let std_root = super::std_crate_root(&sources).expect("std sources appended");
    let crates = vec![
        CrateDef::root_crate(root),
        CrateDef::dependency(STD_CRATE_NAME, std_root),
    ];
    let table = crate::resolver::modules::build_module_table(&crates, &sources, &asts)
        .expect("module table builds");
    let result_module = table
        .find(&[
            STD_CRATE_NAME.to_string(),
            STD_RESULT_MODULE_NAME.to_string(),
        ])
        .expect("std.result is in the module table");
    let export = table.modules[result_module]
        .items
        .get("Result")
        .expect("`import std.result;` yields the `Result` name");
    assert!(export.is_pub, "`pub enum Result` is importable (D1)");
    assert!(
        matches!(export.kind, ExportKind::Enum),
        "the export is the enum declaration"
    );
    assert_eq!(
        export.enum_variants,
        ["Ok", "Err"],
        "the import yields the `Ok` / `Err` names"
    );
    assert_eq!(
        export.type_param_count, 2,
        "the fully-applied-use-site arity (0.0.7 D1)"
    );
}

/// Recorded expectation for the T01 ticket's "no `enum_defs` growth occurs
/// anywhere" (0.0.9 T05 posture, not a hash gate), at the source level: no
/// std module other than `std/result.fln` declares an enum at all, and the
/// result module declares exactly the one `Result` enum. A lexer-level scan
/// is exact — comments and string literals never produce an `enum` token.
/// The Session test above pins the pipeline end (the merged compilation's
/// `enum_defs` stays empty for a std-importing program); this pins the
/// invariant for every shipped std module, imported or not.
#[test]
fn std_declares_no_enums_outside_result() {
    for (name, source) in crate::std_src::files() {
        let tokens =
            tokenize(source).unwrap_or_else(|e| panic!("{name} must lex (std gate): {e:?}"));
        let enum_decls = tokens.iter().filter(|t| t.kind == TokenKind::Enum).count();
        if name == "std/result.fln" {
            assert_eq!(enum_decls, 1, "{name}: exactly the Result declaration (D1)");
        } else {
            assert!(
                enum_decls == 0,
                "{name}: std declares no enums outside std.result (D1); \
                 a new std enum must revisit the no-enum_defs-growth posture"
            );
        }
    }
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
    assert_eq!(session.compile(a).unwrap().version, codegen::MODULE_VERSION);
    assert_eq!(session.compile(b).unwrap().version, codegen::MODULE_VERSION);
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
    assert_eq!(module.version, codegen::MODULE_VERSION);
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
fn compile_project_rejects_async_write_of_imported_global() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import m;\nasync func bump() { m.answer = 42; }\nfunc main() { print(\"ok\"); }\n",
    );
    session.add_file("src/m/mod.fln", "pub answer: int = 0;\n");
    match session.compile_project(root) {
        Err(CompileError::Typeck(errs)) => assert!(
            errs.iter().any(|e| matches!(
                &e.kind,
                TypeckErrorKind::AsyncGlobalWrite { name } if name == "m.answer"
            )),
            "expected AsyncGlobalWrite, got {errs:?}"
        ),
        other => panic!("expected typeck error, got {other:?}"),
    }
}

#[test]
fn compile_project_allows_async_read_of_imported_global() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import m;\nasync func read(): int = m.answer;\nfunc main() { print(\"ok\"); }\n",
    );
    session.add_file("src/m/mod.fln", "pub answer: int = 0;\n");
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

// -------------------------------------------------------------------------
// Dependency crates (the addressing mechanism `flint` needs)
// -------------------------------------------------------------------------

/// A session with a `jsonlite` dependency registered from a realistic cache
/// path, so the module path is derived from the *real* file layout rather
/// than a synthesized name. `extra` is appended to the dependency's `util`
/// module so each test can vary what it does.
fn session_with_jsonlite(util_body: &str, main_body: &str) -> (Session, FileId) {
    let mut session = Session::new();
    let root = session.add_file("src/main.fln", main_body);
    let dep_root = session.add_file(
        "/home/u/.flint/cache/jsonlite-abc123/src/mod.fln",
        "pub func dep_root_fn(): int = 0\n",
    );
    session.add_file(
        "/home/u/.flint/cache/jsonlite-abc123/src/util.fln",
        util_body,
    );
    session.add_dependency("jsonlite", dep_root);
    (session, root)
}

#[test]
fn dependency_module_is_addressable_by_crate_name() {
    // The whole point of the mechanism: a module whose real path lives
    // outside the project's `src/` is reachable as `jsonlite.util`.
    let (session, root) = session_with_jsonlite(
        "pub func answer(): int = 42\n",
        "import jsonlite.util;\nfunc main(): int = jsonlite.util.answer()\n",
    );
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn dependency_crate_root_is_addressable() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import jsonlite;\nfunc main(): int = jsonlite.greet()\n",
    );
    let dep_root = session.add_file(
        "/home/u/.flint/cache/jsonlite-abc/src/mod.fln",
        "pub func greet(): int = 7\n",
    );
    session.add_dependency("jsonlite", dep_root);
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn dependency_cannot_import_the_depending_project() {
    // A dependency is read-only to the build: it must not reach back into
    // the project that pulls it in.
    let (mut session, root) = session_with_jsonlite(
        "import net.tcp;\npub func answer(): int = 1\n",
        "import net.tcp;\nimport jsonlite.util;\n\
         func main(): int = jsonlite.util.answer() + net.tcp.n()\n",
    );
    session.add_file("src/net/tcp.fln", "pub func n(): int = 1\n");
    let err = session
        .compile_project(root)
        .expect_err("back-edge must be rejected");
    let CompileError::Resolve(errors) = err else {
        panic!("expected a resolve error, got a different stage");
    };
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::CrossCrateImport { path, .. } if path == "net.tcp")),
        "expected a cross-crate import error, got {errors:?}"
    );
}

#[test]
fn two_dependencies_may_not_import_each_other() {
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import alib.util;\nimport blib.util;\nfunc main(): int = alib.util.a() + blib.util.b()\n",
    );
    let a_root = session.add_file("/c/alib/src/mod.fln", "");
    session.add_file(
        "/c/alib/src/util.fln",
        "import blib.util;\npub func a(): int = 1\n",
    );
    let b_root = session.add_file("/c/blib/src/mod.fln", "");
    session.add_file(
        "/c/blib/src/util.fln",
        "import alib.util;\npub func b(): int = 1\n",
    );
    session.add_dependency("alib", a_root);
    session.add_dependency("blib", b_root);
    let err = session
        .compile_project(root)
        .expect_err("mutual deps must be rejected");
    let CompileError::Resolve(errors) = err else {
        panic!("expected a resolve error, got a different stage");
    };
    assert!(
        errors
            .iter()
            .any(|e| matches!(&e.kind, ResolveErrorKind::CrossCrateImport { .. })),
        "expected a cross-crate import error, got {errors:?}"
    );
}

#[test]
fn a_dependency_may_import_its_own_dependency() {
    // Transitive dependencies are legal: `jsonlite` importing `serde` is a
    // forward edge, not a cycle.
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import jsonlite.util;\nfunc main(): int = jsonlite.util.answer()\n",
    );
    let jl_root = session.add_file("/c/jsonlite/src/mod.fln", "");
    session.add_file(
        "/c/jsonlite/src/util.fln",
        "import serde.core;\npub func answer(): int = serde.core.base()\n",
    );
    let sd_root = session.add_file("/c/serde/src/mod.fln", "");
    session.add_file("/c/serde/src/core.fln", "pub func base(): int = 1\n");
    session.add_dependency("jsonlite", jl_root);
    session.add_dependency("serde", sd_root);
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn dependency_private_items_stay_private() {
    let (session, root) = session_with_jsonlite(
        "func secret(): int = 1\npub func answer(): int = secret()\n",
        "import jsonlite.util;\nfunc main(): int = jsonlite.util.secret()\n",
    );
    let err = session
        .compile_project(root)
        .expect_err("private access must be rejected");
    assert!(matches!(
        err,
        CompileError::Resolve(ref errors) if errors.iter().any(|e| matches!(
            &e.kind,
            ResolveErrorKind::PrivateAccess { item, .. } if item == "secret"
        ))
    ));
}

#[test]
fn dependency_may_use_its_own_private_items() {
    let (session, root) = session_with_jsonlite(
        "func secret(): int = 1\npub func answer(): int = secret()\n",
        "import jsonlite.util;\nfunc main(): int = jsonlite.util.answer()\n",
    );
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn dependency_may_import_std() {
    let (session, root) = session_with_jsonlite(
        "import std.io;\npub func shout(): int { std.io.println(\"from dep\"); 0 }\n",
        "import jsonlite.util;\nfunc main(): int = jsonlite.util.shout()\n",
    );
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn dependencies_with_the_same_module_name_do_not_collide() {
    // `alib.util` and `blib.util` are different modules; the crate name is
    // what disambiguates them.
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import alib.util;\nimport blib.util;\nfunc main(): int = alib.util.f() + blib.util.f()\n",
    );
    let a_root = session.add_file("/c/alib/src/mod.fln", "");
    session.add_file("/c/alib/src/util.fln", "pub func f(): int = 1\n");
    let b_root = session.add_file("/c/blib/src/mod.fln", "");
    session.add_file("/c/blib/src/util.fln", "pub func f(): int = 2\n");
    session.add_dependency("alib", a_root);
    session.add_dependency("blib", b_root);
    assert!(session.compile_project(root).is_ok());
}

#[test]
fn registering_the_same_crate_twice_is_an_error() {
    // Previously this collapsed silently — one module shadowed the other and
    // the build succeeded with the wrong code. It must be a hard error.
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import jsonlite.util;\nfunc main(): int = jsonlite.util.f()\n",
    );
    let first = session.add_file("/c/a/src/mod.fln", "");
    session.add_file("/c/a/src/util.fln", "pub func f(): int = 1\n");
    let second = session.add_file("/c/b/src/mod.fln", "");
    session.add_file("/c/b/src/util.fln", "pub func f(): int = 2\n");
    session.add_dependency("jsonlite", first);
    session.add_dependency("jsonlite", second);
    let err = session
        .compile_project(root)
        .expect_err("duplicate crate must be rejected");
    assert!(
        matches!(
            err,
            CompileError::Resolve(ref errors) if errors.iter().any(|e| matches!(
                &e.kind,
                ResolveErrorKind::DuplicateModule { path, .. } if path == "jsonlite.util"
            ))
        ),
        "expected a duplicate module error, got {err:?}"
    );
}

#[test]
fn a_dependency_cycle_is_reported_even_when_unreachable_from_main() {
    // The project never imports `jsonlite`, but the crate still has to be
    // acyclic on its own — cycle detection starts from every module, not
    // just the crate root.
    let mut session = Session::new();
    let root = session.add_file("src/main.fln", "func main(): int = 0\n");
    let dep_root = session.add_file("/c/lib/src/mod.fln", "");
    session.add_file("/c/lib/src/a.fln", "import lib.b;\npub func a(): int = 1\n");
    session.add_file("/c/lib/src/b.fln", "import lib.a;\npub func b(): int = 1\n");
    session.add_dependency("lib", dep_root);
    let err = session
        .compile_project(root)
        .expect_err("an unimported crate's cycle must still fail");
    assert!(matches!(
        err,
        CompileError::Resolve(ref errors) if errors.iter().any(|e| matches!(
            &e.kind,
            ResolveErrorKind::CircularImport { .. }
        ))
    ));
}

#[test]
fn missing_dependency_module_suggests_a_path_inside_the_crate() {
    let (session, root) = session_with_jsonlite(
        "pub func answer(): int = 1\n",
        "import jsonlite.nosuch;\nfunc main(): int = 0\n",
    );
    let err = session
        .compile_project(root)
        .expect_err("missing module must fail");
    let CompileError::Resolve(errors) = err else {
        panic!("expected a resolve error, got a different stage");
    };
    assert!(
        errors.iter().any(|e| matches!(
            &e.kind,
            ResolveErrorKind::ModuleNotFound { path, tried }
                if path == "jsonlite.nosuch"
                    && tried.contains(&"/home/u/.flint/cache/jsonlite-abc123/src/nosuch.fln".to_string())
        )),
        "expected the suggestion to point inside the dependency's real tree, got {errors:?}"
    );
}

#[test]
fn dependency_names_are_mangled_under_their_crate() {
    // The mangled symbol must carry the crate name so the merged HIR keeps
    // the two crates' items apart.
    let (session, root) = session_with_jsonlite(
        "pub func answer(): int = 42\n",
        "import jsonlite.util;\nfunc main(): int = jsonlite.util.answer()\n",
    );
    let module = session.compile_project(root).unwrap();
    let names: Vec<String> = module
        .constants
        .iter()
        .filter_map(|c| match c {
            codegen::Const::Str(s) => Some(s.to_string()),
            _ => None,
        })
        .collect();
    assert!(
        names.iter().any(|n| n == "jsonlite.util.answer"),
        "expected `jsonlite.util.answer` in the constant pool, got {names:?}"
    );
}

#[test]
fn a_dependency_nested_inside_the_project_tree_claims_its_own_files() {
    // A vendored dependency living under the project's `src/` must belong to
    // the dependency, not to the root crate — the longest matching source
    // directory wins. Getting this backwards would register
    // `src/vendor/lib/src/util.fln` as the module `vendor.lib.src.util`.
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import lib.util;\nfunc main(): int = lib.util.answer()\n",
    );
    let dep_root = session.add_file("src/vendor/lib/src/mod.fln", "");
    session.add_file(
        "src/vendor/lib/src/util.fln",
        "pub func answer(): int = 42\n",
    );
    session.add_dependency("lib", dep_root);
    assert!(session.compile_project(root).is_ok());
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

// ========== 0.0.6 T03: cross-module enum visibility ==========

#[test]
fn compile_project_imports_pub_enum() {
    // `pub enum` joins the module export/import machinery exactly like
    // `pub struct`: the enum name is visible across the module boundary.
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import shapes;\nfunc main(): int { x = shapes.Color.Red; 0 }\n",
    );
    session.add_file("src/shapes/mod.fln", "pub enum Color { Red, Green }\n");
    // The resolver must resolve the cross-module constructor path. The
    // typeck stage rejects the enum *type* until T04 adds `Type::Enum`, so
    // the assertion is: the failure is NOT a resolver failure.
    if let Err(CompileError::Resolve(errs)) = session.compile_project(root) {
        panic!("resolver rejected the cross-module enum: {errs:?}");
    }
}

#[test]
fn compile_project_rejects_private_enum_access() {
    // A non-`pub` enum stays module-local: reaching it from another module
    // is a `PrivateAccess` error.
    let mut session = Session::new();
    let root = session.add_file(
        "src/main.fln",
        "import shapes;\nfunc main(): int { x = shapes.Color.Red; 0 }\n",
    );
    session.add_file("src/shapes/mod.fln", "enum Color { Red, Green }\n");
    assert!(matches!(
        session.compile_project(root),
        Err(CompileError::Resolve(_))
    ));
}

#[test]
fn feature_sets_table_reflects_directives() {
    // 0.0.8 T01: the `FileId → FeatureSet` table lives on the compilation
    // config (`Session::feature_table`) — a directive in the header enables
    // `curry` for that file only; files without one stay the default
    // (flag-off) set.
    let mut session = Session::new();
    // The flag-on file's content stays outside the function-form switch
    // (bindings only) — the legacy `func` form is forbidden there since T02.
    let on = session.add_file("on.fln", "#feature curry\nx = 1;");
    let off = session.add_file("off.fln", "func main(): int = 42");
    let table = session.feature_table().unwrap();
    assert!(table.get(on).curry);
    assert!(!table.get(off).curry);
}

#[test]
fn directive_file_rejects_legacy_form() {
    // 0.0.8 T02 (D2): the strictness switch landed — in a flag-on file the
    // legacy `func` form is a parse error (`FuncFormForbiddenInCurryFile`).
    // A flag-on file cannot compile end-to-end here yet: its `main` must
    // use the two-part form, and the resolver wiring for that is T04's
    // card — so only the rejection half is pinned at this stage.
    assert!(matches!(
        compile("#feature curry\nfunc main(): int = 42"),
        Err(CompileError::Parse(e))
            if e.kind == ParseErrorKind::FuncFormForbiddenInCurryFile
    ));
    // Flag-off behavior is untouched: the legacy form still compiles.
    let module = compile("func main(): int = 42").unwrap();
    assert_eq!(module.version, codegen::MODULE_VERSION);
}
