//! CLI-level tests for `fleen-verify` (T07-C): drive the real binary.
//!
//! Exit-code contract: 0 pass, 1 decode/verify failure, 2 usage/IO error.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_fleen-verify");

/// A throwaway working directory for one test.
struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("fleen-verify-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        Temp(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Run the binary with the given args, returning (exit code, stdout, stderr).
fn run(args: &[&str]) -> (i32, String, String) {
    run_env(args, false)
}

/// Like [run], but additionally clears `NO_COLOR` from the child environment
/// when `clear_no_color` is set, so the "color on by default" assertion is
/// independent of the test runner's environment.
fn run_env(args: &[&str], clear_no_color: bool) -> (i32, String, String) {
    let mut cmd = Command::new(BIN);
    cmd.args(args);
    if clear_no_color {
        cmd.env_remove("NO_COLOR");
    }
    let out = cmd.output().unwrap_or_else(|e| panic!("spawn {BIN}: {e}"));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Compile a tiny valid Fleen source and write it as a `.flnc` file.
fn write_valid_flnc(dir: &Path) -> PathBuf {
    let module = fleen_compiler::compile("func main(): int = 42").expect("compile valid source");
    let bytes = fleen_compiler::codegen::to_bytes(&module);
    let path = dir.join("valid.flnc");
    std::fs::write(&path, &bytes).expect("write valid.flnc");
    path
}

#[test]
fn help_exits_zero_and_lists_flags() {
    let (code, stdout, _stderr) = run(&["--help"]);
    assert_eq!(code, 0, "help must exit 0");
    assert!(
        stdout.contains("--project"),
        "help lists --project: {stdout}"
    );
    assert!(stdout.contains("--quiet"), "help lists --quiet: {stdout}");
    assert!(
        stdout.contains("--no-color"),
        "help lists --no-color: {stdout}"
    );
    assert!(
        stdout.contains("Exit codes"),
        "help lists exit codes: {stdout}"
    );
}

#[test]
fn version_exits_zero() {
    let (code, stdout, _stderr) = run(&["--version"]);
    assert_eq!(code, 0, "version must exit 0");
    assert!(
        stdout.contains("fleen-verify"),
        "version names the binary: {stdout}"
    );
}

#[test]
fn missing_file_exits_2() {
    let (code, _stdout, _stderr) = run(&[]);
    assert_eq!(code, 2, "no args is a usage error (exit 2)");
}

#[test]
fn quiet_suppresses_pass_line() {
    let temp = Temp::new("quiet");
    let path = write_valid_flnc(temp.path());
    let file = path.to_str().unwrap();

    let (code, stdout, _stderr) = run(&[file]);
    assert_eq!(code, 0, "valid file verifies");
    assert!(
        stdout.contains("verify: PASS"),
        "default run prints PASS: {stdout}"
    );

    let (code, stdout, _stderr) = run(&["--quiet", file]);
    assert_eq!(code, 0, "valid file verifies with --quiet");
    assert!(
        stdout.is_empty(),
        "--quiet suppresses PASS, got: {stdout:?}"
    );
}

#[test]
fn project_flag_is_accepted_noop() {
    let temp = Temp::new("project");
    let path = write_valid_flnc(temp.path());
    let file = path.to_str().unwrap();

    let (code, stdout, _stderr) = run(&["--project", "/tmp/somewhere", file]);
    assert_eq!(code, 0, "--project is accepted and ignored");
    assert!(
        stdout.contains("verify: PASS"),
        "project run still verifies: {stdout}"
    );
}

#[test]
fn no_color_removes_ansi_on_error() {
    let temp = Temp::new("no-color");
    let path = temp.path().join("bad.flnc");
    std::fs::write(&path, b"garbage").expect("write corrupt flnc");
    let file = path.to_str().unwrap();

    let (code, _stdout, stderr) = run(&["--no-color", file]);
    assert_eq!(code, 1, "corrupt file fails with exit 1");
    assert!(
        !stderr.contains("\x1b["),
        "--no-color removes ANSI: {stderr:?}"
    );

    let (code, _stdout, stderr) = run_env(&[file], true);
    assert_eq!(code, 1, "corrupt file fails with exit 1");
    assert!(
        stderr.contains("\x1b["),
        "color is on by default when NO_COLOR is unset: {stderr:?}"
    );
}
