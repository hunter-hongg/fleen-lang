//! End-to-end tests (TICKETS.md T10): drive the real `fleen-vm` binary.
//!
//! Fixtures live in the repo-root `tests/e2e/` directory:
//!
//! - `valid/<name>.fln` + `<name>.expected`: the program must compile,
//!   verify, execute and its stdout must match the expected file exactly.
//! - `invalid/<name>.fln` + `<name>.exit` + `<name>.stderr`: the program
//!   must fail at compile time or runtime with the given exit code and a
//!   stderr message containing the given needle.
//!
//! Exit code contract (`fleen-vm` main.rs): 0 success, 1 compile / decode /
//! verify / runtime failure, 2 usage / IO error.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the `fleen-vm` binary built for this test run.
const VM: &str = env!("CARGO_BIN_EXE_fleen-vm");

/// Run the VM binary on `input`, returning (exit code, stdout, stderr).
fn run_vm(input: &Path) -> (i32, String, String) {
    let output = Command::new(VM)
        .arg(input)
        .output()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", VM));
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// `tests/e2e/<sub>` next to the repo root.
fn e2e_dir(sub: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/e2e")
        .join(sub)
}

/// All `.fln` fixtures in `dir`, sorted for deterministic failure output.
fn fln_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "fln"))
        .collect();
    files.sort();
    files
}

#[test]
fn valid_programs_print_expected_output() {
    let dir = e2e_dir("valid");
    let mut ran = 0;
    for path in fln_files(&dir) {
        let expected_path = path.with_extension("expected");
        let expected = fs::read_to_string(&expected_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", expected_path.display()));

        let (code, stdout, stderr) = run_vm(&path);
        assert_eq!(code, 0, "{}: exit code (stderr: {stderr})", path.display());
        assert!(
            stderr.is_empty(),
            "{}: unexpected stderr: {stderr}",
            path.display()
        );
        assert_eq!(stdout, expected, "{}: stdout mismatch", path.display());
        ran += 1;
    }
    assert!(ran > 0, "no .fln fixtures found in {}", dir.display());
}

#[test]
fn invalid_programs_fail_with_expected_error() {
    let dir = e2e_dir("invalid");
    let mut ran = 0;
    for path in fln_files(&dir) {
        let exit_path = path.with_extension("exit");
        let expected_code: i32 = fs::read_to_string(&exit_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", exit_path.display()))
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("{}: bad exit code: {e}", exit_path.display()));
        let stderr_path = path.with_extension("stderr");
        let needle = fs::read_to_string(&stderr_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", stderr_path.display()))
            .trim()
            .to_owned();

        let (code, _stdout, stderr) = run_vm(&path);
        assert_eq!(
            code,
            expected_code,
            "{}: exit code (stderr: {stderr})",
            path.display()
        );
        assert!(
            stderr.contains(&needle),
            "{}: stderr {stderr:?} does not contain {needle:?}",
            path.display()
        );
        ran += 1;
    }
    assert!(ran > 0, "no .fln fixtures found in {}", dir.display());
}

#[test]
fn compiled_bytecode_roundtrip_prints_expected_output() {
    // Same program, but through the `.flnc` decode path instead of the
    // `.fln` compile path: source → compile → serialize → decode → execute.
    let source = fs::read_to_string(e2e_dir("valid").join("fib.fln")).expect("read fib.fln");
    let module = fleen_compiler::compile(&source).expect("fib.fln should compile");
    let out = std::env::temp_dir().join("fleen-e2e-fib.flnc");
    fs::write(&out, fleen_compiler::codegen::to_bytes(&module)).expect("write temp .flnc");

    let (code, stdout, stderr) = run_vm(&out);
    let _ = fs::remove_file(&out);

    let expected =
        fs::read_to_string(e2e_dir("valid").join("fib.expected")).expect("read fib.expected");
    assert_eq!(code, 0, "exit code (stderr: {stderr})");
    assert_eq!(stdout, expected, "stdout mismatch via .flnc path");
}

#[test]
fn corrupt_bytecode_is_rejected() {
    let out = std::env::temp_dir().join("fleen-e2e-corrupt.flnc");
    fs::write(&out, b"definitely not fleen bytecode").expect("write temp .flnc");

    let (code, _stdout, stderr) = run_vm(&out);
    let _ = fs::remove_file(&out);

    assert_eq!(code, 1, "exit code (stderr: {stderr})");
    assert!(stderr.contains("Decode error"), "stderr: {stderr}");
}

#[test]
fn missing_input_file_exits_2() {
    let (code, _stdout, stderr) = run_vm(Path::new("/nonexistent/fleen-e2e-missing.fln"));
    assert_eq!(code, 2, "exit code (stderr: {stderr})");
    assert!(stderr.contains("Error reading"), "stderr: {stderr}");
}

#[test]
fn unsupported_extension_exits_2() {
    let out = std::env::temp_dir().join("fleen-e2e-note.txt");
    fs::write(&out, "hello").expect("write temp file");

    let (code, _stdout, stderr) = run_vm(&out);
    let _ = fs::remove_file(&out);

    assert_eq!(code, 2, "exit code (stderr: {stderr})");
    assert!(stderr.contains("Unsupported input"), "stderr: {stderr}");
}

#[test]
fn no_arguments_exits_2() {
    let output = Command::new(VM).output().expect("spawn fleen-vm");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Usage"), "stderr: {stderr}");
}
