//! CLI-level tests for flint (0.0.5 D6): clap parsing, the uniform
//! `--quiet` / `--no-color` flag set, and exit-code behavior.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_flint-cli");
/// Isolated cache home so the tests never touch a real `~/.flint`.
const FLINT_HOME: &str = "/tmp/flint-e2e-home";

/// A throwaway working directory for one test.
struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("flint-cli-{tag}-{}", std::process::id()));
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

/// Write a file and its parent dirs.
fn write(path: &Path, contents: &str) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).expect("create parent");
    }
    std::fs::write(path, contents).expect("write file");
}

/// Run `flint` in `dir`, isolating `FLINT_HOME` and clearing `NO_COLOR`
/// so ANSI assertions are deterministic.
fn run_in(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let _ = std::fs::create_dir_all(FLINT_HOME);
    let out = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("FLINT_HOME", FLINT_HOME)
        .env_remove("NO_COLOR")
        .output()
        .unwrap_or_else(|e| panic!("spawn flint in {}: {e}", dir.display()));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// `flint --help` exits 0, lists the public subcommands and the exit-code
/// legend, and does not reveal the hidden `exec` subcommand.
#[test]
fn help_exits_zero_and_lists_subcommands_and_exit_codes() {
    let tmp = Temp::new("help");
    let (code, out, _err) = run_in(tmp.path(), &["--help"]);
    assert_eq!(code, 0, "--help exit");
    for needle in ["new", "build", "run", "test", "check", "Exit codes"] {
        assert!(out.contains(needle), "--help lists {needle}: {out}");
    }
    // The hidden `exec` subcommand must not be listed; the word
    // "execute" legitimately appears in the `run` about, so match on a
    // line that names the subcommand rather than the substring.
    let exec_lines: Vec<&str> = out
        .lines()
        .map(|l| l.trim_start())
        .filter(|l| l.starts_with("exec"))
        .collect();
    assert!(exec_lines.is_empty(), "--help hides exec: {out}");
}

/// `flint --version` exits 0 and names the binary.
#[test]
fn version_exits_zero() {
    let tmp = Temp::new("version");
    let (code, out, _err) = run_in(tmp.path(), &["--version"]);
    assert_eq!(code, 0, "--version exit");
    assert!(out.contains("flint"), "--version names flint: {out}");
}

/// No subcommand is a usage error (exit 2), matching clap's
/// `arg_required_else_help`.
#[test]
fn no_subcommand_exits_2() {
    let tmp = Temp::new("noargs");
    let (code, _out, _err) = run_in(tmp.path(), &[]);
    assert_eq!(code, 2, "no subcommand exits 2");
}

/// Per-subcommand help documents the command and the program-args boundary.
#[test]
fn subcommand_help_is_per_command() {
    let tmp = Temp::new("subhelp");
    let (code, out, _err) = run_in(tmp.path(), &["build", "--help"]);
    assert_eq!(code, 0, "build --help exit");
    assert!(
        out.contains("write the project bytecode") || out.contains("Build"),
        "build help names the subcommand: {out}"
    );
    let (code, out, _err) = run_in(tmp.path(), &["run", "--help"]);
    assert_eq!(code, 0, "run --help exit");
    assert!(
        out.contains("program") || out.contains("after --"),
        "run help documents program args: {out}"
    );
}

/// `run --quiet` suppresses the program's stdout; without it the program
/// output appears normally.
#[test]
fn quiet_suppresses_program_stdout() {
    let tmp = Temp::new("quiet");
    let proj = tmp.path().join("app");
    write(
        &proj.join("flint.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n",
    );
    write(
        &proj.join("src/main.fln"),
        "import std.io;\nfunc main() { std.io.println(\"hello\"); }\n",
    );

    let (code, out, err) = run_in(&proj, &["run", "--quiet"]);
    assert_eq!(code, 0, "quiet run exit (stderr: {err})");
    assert_eq!(out, "", "quiet suppresses program stdout");

    let (code, out, err) = run_in(&proj, &["run"]);
    assert_eq!(code, 0, "run exit (stderr: {err})");
    assert_eq!(out, "hello\n", "program stdout without quiet");
}

/// `--no-color` removes ANSI escapes from a compile error; the default
/// (with `NO_COLOR` unset) renders them.
#[test]
fn no_color_removes_ansi_on_compile_error() {
    let tmp = Temp::new("nocolor");
    let proj = tmp.path().join("app");
    write(
        &proj.join("flint.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n",
    );
    write(&proj.join("src/main.fln"), "func main() { 1 + }\n");

    let (code, _out, err) = run_in(&proj, &["run", "--no-color"]);
    assert_eq!(code, 1, "compile error exit with --no-color");
    assert!(!err.contains("\x1b["), "stderr has no ANSI: {err:?}");

    let (code, _out, err) = run_in(&proj, &["run"]);
    assert_eq!(code, 1, "compile error exit");
    assert!(err.contains("\x1b["), "stderr has ANSI: {err:?}");
}

/// `new` scaffolds a project and `check` compiles it successfully.
#[test]
fn new_and_check_smoke() {
    let tmp = Temp::new("newcheck");
    let (code, out, err) = run_in(tmp.path(), &["new", "hello_proj"]);
    assert_eq!(code, 0, "flint new exit (stderr: {err})");
    let proj = tmp.path().join("hello_proj");
    assert!(proj.join("flint.toml").is_file(), "flint.toml scaffolded");
    assert!(
        proj.join("src/main.fln").is_file(),
        "src/main.fln scaffolded"
    );
    let _ = out;

    let (code, _out, err) = run_in(&proj, &["check"]);
    assert_eq!(code, 0, "flint check exit (stderr: {err})");
}

/// `--quiet` suppresses tool messages from `new`, `build` and `test`.
#[test]
fn quiet_suppresses_tool_messages() {
    let tmp = Temp::new("quiet-tools");
    let (code, out, err) = run_in(tmp.path(), &["new", "--quiet", "hello_proj"]);
    assert_eq!(code, 0, "new --quiet exit (stderr: {err})");
    assert_eq!(out, "", "new --quiet prints nothing");

    let proj = tmp.path().join("hello_proj");
    let (code, out, err) = run_in(&proj, &["build", "--quiet"]);
    assert_eq!(code, 0, "build --quiet exit (stderr: {err})");
    assert_eq!(out, "", "build --quiet prints nothing");

    write(
        &proj.join("tests/greet.fln"),
        "import std.io;\nfunc main() { std.io.println(\"hi\"); }\n",
    );
    write(&proj.join("tests/greet.expected"), "hi\n");
    let (code, out, err) = run_in(&proj, &["test", "--quiet"]);
    assert_eq!(code, 0, "test --quiet exit (stderr: {err})");
    assert_eq!(out, "", "test --quiet prints nothing");
}
