//! CLI-level integration tests for the fleen-vm binary (T07-B).
//!
//! These drive the real `fleen-vm` executable and lock the 0.0.5 uniform
//! flag set: --help/--version, the required positional, --quiet, --no-color,
//! program args after `--`, and project mode.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the `fleen-vm` binary built for this test run.
const VM: &str = env!("CARGO_BIN_EXE_fleen-vm");

/// Run the VM binary with args, with NO_COLOR removed so color behavior is
/// deterministic. Returns (exit code, stdout, stderr).
fn run_vm(args: &[&str]) -> (i32, String, String) {
    let output = Command::new(VM)
        .args(args)
        .env_remove("NO_COLOR")
        .output()
        .unwrap_or_else(|e| panic!("spawn {}: {e}", VM));
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Write a unique temp `.fln` fixture and return its path.
fn temp_file(name: &str, source: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("fleen-vm-cli-{}-{name}.fln", std::process::id()));
    fs::write(&path, source).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
    path
}

#[test]
fn help_exits_zero_and_lists_flags() {
    let (code, stdout, _stderr) = run_vm(&["--help"]);
    assert_eq!(code, 0, "help should exit 0");
    assert!(stdout.contains("--project"), "help stdout: {stdout}");
    assert!(stdout.contains("--quiet"), "help stdout: {stdout}");
    assert!(stdout.contains("--no-color"), "help stdout: {stdout}");
    assert!(stdout.contains("Exit codes"), "help stdout: {stdout}");
}

#[test]
fn version_exits_zero() {
    let (code, stdout, _stderr) = run_vm(&["--version"]);
    assert_eq!(code, 0, "version should exit 0");
    assert!(stdout.contains("fleen-vm"), "version stdout: {stdout}");
}

#[test]
fn no_args_exits_2() {
    let (code, _stdout, _stderr) = run_vm(&[]);
    assert_eq!(code, 2, "no args should be a usage error");
}

#[test]
fn quiet_suppresses_program_stdout() {
    let input = temp_file(
        "quiet",
        "import std.io; func main() { std.io.println(\"hello\"); }",
    );
    let input_str = input.to_string_lossy().into_owned();

    let (code, stdout, stderr) = run_vm(&["--quiet", &input_str]);
    assert_eq!(code, 0, "quiet run should succeed (stderr: {stderr})");
    assert_eq!(stdout, "", "quiet run should suppress stdout");

    let (code, stdout, _stderr) = run_vm(&[&input_str]);
    assert_eq!(code, 0, "normal run should succeed");
    assert_eq!(stdout, "hello\n", "normal run should print hello");

    let _ = fs::remove_file(&input);
}

#[test]
fn no_color_removes_ansi_from_runtime_error() {
    let input = temp_file("no-color", "func main() { x = 1; y = x / 0; }");
    let input_str = input.to_string_lossy().into_owned();

    let (code, _stdout, stderr) = run_vm(&["--no-color", &input_str]);
    assert_eq!(code, 1, "runtime error should exit 1 (stderr: {stderr})");
    assert!(
        stderr.contains("runtime error"),
        "stderr should be a runtime error: {stderr:?}"
    );
    assert!(
        !stderr.contains("\x1b["),
        "stderr should be plain: {stderr:?}"
    );

    let (code, _stdout, stderr) = run_vm(&[&input_str]);
    assert_eq!(code, 1, "runtime error should exit 1 (stderr: {stderr})");
    assert!(
        stderr.contains("\x1b["),
        "default color should emit ANSI: {stderr:?}"
    );

    let _ = fs::remove_file(&input);
}

#[test]
fn program_args_after_dashdash() {
    let input = temp_file(
        "args",
        "import std.io; import std.os; func main() { args = std.os.args(); std.io.println(args[0]); }",
    );
    let input_str = input.to_string_lossy().into_owned();

    let (code, stdout, stderr) = run_vm(&[&input_str, "--", "alpha"]);
    assert_eq!(code, 0, "args run should succeed (stderr: {stderr})");
    assert_eq!(stdout, "alpha\n", "program should see args[0]");

    let (code, _stdout, _stderr) = run_vm(&[&input_str, "alpha"]);
    assert_eq!(code, 2, "program args without -- should be a usage error");

    let _ = fs::remove_file(&input);
}

#[test]
fn project_mode_still_works() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/e2e/projects/netdemo");
    let project_str = project.to_string_lossy().into_owned();
    let (code, _stdout, stderr) = run_vm(&["--project", &project_str]);
    assert_eq!(code, 0, "project mode should succeed (stderr: {stderr})");
}
