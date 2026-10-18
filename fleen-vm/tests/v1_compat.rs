//! v1 compatibility (TICKETS U11, BYTECODE.md §10).
//!
//! `tests/fixtures/v1/fib.flnc` is a genuine **0.0.1 bytecode module**,
//! regenerated from the archived 0.0.1 release
//! (`release/0.0.1/fleen-0.0.1-x86_64-linux.tar.gz` → `fleen-compiler`) using
//! the 0.0.1-era `fib.fln` from `release/0.0.1/RELEASE_NOTES.md` (the
//! `print(fib(x))` form, before `as`/strict-string print existed). The 0.0.3
//! verifier must decode, verify and execute it with unchanged output — the
//! "a later version does not break v1" promise, locked as a committed binary
//! fixture. Reproduce with:
//!   `./fleen-compiler fib_v1.fln -o fib.flnc`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use fleen_compiler::codegen::{Const, ConstId, Func, FuncId, Module, Opcode, to_bytes};

/// Path to the `fleen-vm` binary built for this test run.
const VM: &str = env!("CARGO_BIN_EXE_fleen-vm");

/// Repo-root-relative path helper (`tests/...` next to the workspace).
fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../{rel}"))
}

#[test]
fn v001_fib_fixture_verifies_and_executes() {
    let path = repo_path("tests/fixtures/v1/fib.flnc");
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

    // Static verification accepts the 0.0.1 module untouched.
    let module = fleen_compiler::codegen::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()));
    assert_eq!(module.version, 1, "fixture must be a v1 module");
    fleen_verify::verify(&module).unwrap_or_else(|e| panic!("verify {}: {e}", path.display()));

    // Execution output is byte-identical to the 0.0.1 regression baseline.
    let expected_path = repo_path("tests/e2e/valid/fib.expected");
    let expected = fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("read {expected_path:?}: {e}"));
    let output = Command::new(VM)
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("spawn {VM}: {e}"));
    assert_eq!(
        output.status.code(),
        Some(0),
        "exit code (stderr: {})",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        expected,
        "v1 fib output drifted"
    );
}

#[test]
fn v001_runtime_error_degrades_to_func_and_pc() {
    // A v1 module carries no usable source and (here) no span_map entries,
    // so a runtime error must degrade to `message (in `func`, pc 0x..)`
    // and exit 1 — no source location, no panic (TICKETS U10).
    let m = Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Str("main".into()), Const::Int(1), Const::Int(0)],
        functions: vec![Func {
            name: ConstId(0),
            params: 0,
            locals: 0,
            code: Box::new([
                Opcode::Const as u8,
                1,
                0,
                0,
                0, // Const 1
                Opcode::Const as u8,
                2,
                0,
                0,
                0, // Const 0
                Opcode::IDiv as u8,
                Opcode::Return as u8,
            ]),
            span_map: Box::new([]),
            is_builtin: false,
        }],
        globals: vec![],
        entry: FuncId(0),
    };

    let path = std::env::temp_dir().join("fleen_v1_runtime_error.flnc");
    fs::write(&path, to_bytes(&m)).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));

    let output = Command::new(VM)
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("spawn {VM}: {e}"));
    assert_eq!(output.status.code(), Some(1), "exit code must stay 1");
    let stderr = String::from_utf8_lossy(&output.stderr);
    // pc 0xa: the failing `IDiv` sits at byte 10 (two 5-byte `Const`s).
    assert_eq!(
        stderr, "runtime error: division by zero (in `main`, pc 0xa)\n",
        "degraded report must name the function and offset"
    );

    fs::remove_file(&path).unwrap_or_else(|e| panic!("cleanup {}: {e}", path.display()));
}
