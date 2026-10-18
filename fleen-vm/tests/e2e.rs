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

/// Run the VM binary with arbitrary arguments (e.g. `--project <dir>`).
fn run_vm_args(args: &[&Path]) -> (i32, String, String) {
    let output = Command::new(VM)
        .args(args)
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

#[test]
fn project_demo_prints_expected_output() {
    let project = e2e_dir("projects/netdemo");
    let expected = fs::read_to_string(project.join(".expected")).expect("read .expected");
    let (code, stdout, stderr) = run_vm_args(&[Path::new("--project"), project.as_path()]);
    assert_eq!(code, 0, "exit code (stderr: {stderr})");
    assert_eq!(stdout, expected, "stdout mismatch for project mode");
}

#[test]
fn project_imported_globals_prints_expected_output() {
    let project = e2e_dir("projects/globaldemo");
    let expected = fs::read_to_string(project.join(".expected")).expect("read .expected");
    let (code, stdout, stderr) = run_vm_args(&[Path::new("--project"), project.as_path()]);
    assert_eq!(code, 0, "exit code (stderr: {stderr})");
    assert_eq!(stdout, expected, "stdout mismatch for imported globals");
}

#[test]
fn project_cycle_reports_error() {
    let project = e2e_dir("projects/circular");
    let (code, _stdout, stderr) = run_vm_args(&[Path::new("--project"), project.as_path()]);
    assert_eq!(code, 1, "exit code (stderr: {stderr})");
    assert!(stderr.contains("circular import"), "stderr: {stderr}");
}

/// Run the VM with a `.fln` file followed by trailing program arguments
/// (0.0.3 V06: `std.os.args` receives these without argv[0]).
fn run_vm_with_program_args(file: &Path, program_args: &[&str]) -> (i32, String, String) {
    let mut cmd = Command::new(VM);
    cmd.arg(file);
    for a in program_args {
        cmd.arg(a);
    }
    let output = cmd.output().unwrap_or_else(|e| panic!("spawn {}: {e}", VM));
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// std.os.args receives trailing CLI args (no argv[0]).
#[test]
fn std_os_args_receives_program_arguments() {
    // Write a temp .fln that prints args[0], then invoke with a trailing arg.
    let dir = std::env::temp_dir();
    let src = "import std.io;\nimport std.os;\nfunc main() {\n    args = std.os.args();\n    std.io.println(args[0]);\n}\n";
    let input = dir.join("fleen-v06-std-os-args.fln");
    fs::write(&input, src).expect("write temp fixture");
    let (code, stdout, stderr) = run_vm_with_program_args(&input, &["alpha"]);
    let _ = fs::remove_file(&input);
    assert_eq!(code, 0, "exit code (stderr: {stderr})");
    assert_eq!(stdout, "alpha\n", "stdout mismatch");
}

/// std.os.exit(3) passes the code through as the process exit code.
#[test]
fn std_os_exit_passes_through_exit_code() {
    let input = e2e_dir("invalid").join("std_os_exit.fln");
    let (code, _stdout, stderr) = run_vm(&input);
    assert_eq!(code, 3, "exit code (stderr: {stderr})");
    assert!(stderr.is_empty(), "stderr should be empty for clean exit");
}

/// `tests/std/` next to the repo root (0.0.3 V06: std exercised directly).
fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/std")
}

/// 0.0.3 V06: std library exercised directly as `.fln` + `.expected` fixtures.
///
/// Each `.fln` file in `tests/std/` is compiled, verified, and executed as a
/// standalone program; its stdout must match the `.expected` file exactly.
/// This ensures std cannot rot — it is compiled and run on every `cargo test`.
#[test]
fn std_library_tests_pass() {
    let dir = std_dir();
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
