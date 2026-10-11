//! v1 compatibility (TICKETS U11, BYTECODE.md §10).
//!
//! `tests/fixtures/v1/fib.flnc` was produced by the **0.0.1 toolchain**
//! (git tag `v0.0.1`) from the 0.0.1-era `fib.fln` (`print(fib(x))`, before
//! `as`/strict-string print existed). v2 must keep decoding, verifying and
//! executing it with unchanged output — the "v2 does not break v1" promise,
//! locked as a committed binary fixture.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

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
