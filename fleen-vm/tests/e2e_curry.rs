//! End-to-end coverage for the `#feature curry` flag (0.0.8 T09 / F10): the
//! harness runs the currying feature matrix in **both modes** — every
//! pre-0.0.8 fixture is a flag-off file (the existing discovery-based
//! `e2e` module compiles and runs them unchanged, which is the
//! byte-identical / non-breaking gate, D16), and the fixtures added here
//! carry `#feature curry` and exercise the flag-on mode plus the project-mode
//! assertions the feature needs (per-module version, the `PartialApply`
//! opcode, and std's flag-off content identity).
//!
//! The harness drives the real `fleen-vm` binary (same shape as
//! `fleen-vm/tests/e2e.rs`); the compiler-level assertions (`Session`) use
//! the `fleen-compiler` crate directly.
//!
//! Fixtures live in the repo-root `tests/e2e/` directory:
//!
//! - `valid/curry_demo.fln` + `.expected`: flag-on demo (two-part
//!   declarations, curried arrow types, partial application, partial flowing
//!   through a flag-off std higher-order helper).
//! - `valid/partial_flow.fln` + `.expected`: a partial produced in a flag-on
//!   file is called inside flag-off code (std's inline `map` loop) — T08's
//!   copy-soundness property, end to end.
//! - `invalid/curry_*.fln` + `.exit` + `.stderr`: the per-mode error shapes
//!   (flag-off files keep today's errors; flag-on files get the currying
//!   rules and keep the shared ones).
//! - `projects/curry_v7` / `projects/curry_v8`: the per-module bytecode
//!   version rule (D13), compiled in-process and run end-to-end here.
//! - `projects/curry_import` / `projects/curry_main`: cross-module curried
//!   exports (D6) — run by `tests/e2e.rs` (T04); `curry_flagoff_import`
//!   here pins the error side of the same rule.
//!
//! Exit code contract (`fleen-vm` main.rs): 0 success, 1 compile / decode /
//! verify / runtime failure, 2 usage / IO error.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use fleen_compiler::{FileId, Session};

/// Path to the `fleen-vm` binary built for this test run.
const VM: &str = env!("CARGO_BIN_EXE_fleen-vm");

/// `tests/e2e/<sub>` next to the repo root.
fn e2e_dir(sub: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/e2e")
        .join(sub)
}

/// `tests/e2e/projects/<name>`.
fn project_dir(name: &str) -> PathBuf {
    e2e_dir("projects").join(name)
}

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

// ---------------------------------------------------------------------------
// Project-mode assertions (D6, D13): compile the project in-process and check
// the module table, version, and instruction stream.
// ---------------------------------------------------------------------------

/// Compile the `tests/e2e/projects/<name>` tree in-process (the same
/// load-and-merge shape the VM's project driver uses) and return the module.
fn compile_project(name: &str) -> fleen_compiler::codegen::Bytecode {
    let dir = project_dir(name);
    let mut files: Vec<PathBuf> = Vec::new();
    collect_fln(&dir.join("src"), &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "project `{name}` has no .fln files under src/"
    );
    let mut session = Session::new();
    let mut root = None;
    for path in &files {
        let rel = path
            .strip_prefix(&dir)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let source = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
        let id = session.add_file(rel.clone(), source);
        if rel == "src/main.fln" || rel == "main.fln" {
            root = Some(id);
        }
    }
    let root = root.unwrap_or_else(|| panic!("project `{name}` has no src/main.fln"));
    session
        .compile_project(root)
        .unwrap_or_else(|e| panic!("project `{name}` failed to compile: {e}"))
}

fn collect_fln(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("read {dir:?}: {e}"));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("readdir {dir:?}: {e}"));
        let path = entry.path();
        if path.is_dir() {
            collect_fln(&path, out);
        } else if path.extension().map(|e| e == "fln").unwrap_or(false) {
            out.push(path);
        }
    }
}

/// Compile a project in-process, then also run the real VM on it and compare
/// stdout with the fixture's `.expected` — a v7/v8 module must decode, verify
/// and execute, not merely compile. Returns the compiled module.
fn compile_and_run_project(name: &str) -> fleen_compiler::codegen::Bytecode {
    let module = compile_project(name);
    let dir = project_dir(name);
    let expected =
        fs::read_to_string(dir.join(".expected")).unwrap_or_else(|e| panic!("read .expected: {e}"));
    let (code, stdout, stderr) = run_vm_args(&[Path::new("--project"), &dir]);
    assert_eq!(
        code, 0,
        "`{name}` failed to run: {stderr} (stdout:\n{stdout})"
    );
    assert_eq!(stdout, expected, "`{name}` stdout mismatch");
    module
}

/// The `PartialApply` opcode byte (0.0.8 T07/D13, `V8_OPCODE_BASE`): the
/// encoder emits exactly one opcode byte followed by one `u8` operand, so a
/// flat scan of each function's code stream detects the instruction.
const PARTIAL_APPLY_OPCODE: u8 = fleen_compiler::codegen::bytecode::V8_OPCODE_BASE;

fn emits_partial_apply(module: &fleen_compiler::codegen::Bytecode) -> bool {
    module
        .functions
        .iter()
        .any(|f| f.code.contains(&PARTIAL_APPLY_OPCODE))
}

/// A flag-on project with no partial call emits bytecode v7 (D13: the bump is
/// content-driven, not flag-driven). The real VM run pins the v7 module as
/// decodable and executable, not merely compilable.
#[test]
fn project_curry_without_partials_stays_v7() {
    let module = compile_and_run_project("curry_v7");
    assert_eq!(
        module.version,
        fleen_compiler::codegen::MODULE_VERSION,
        "a flag-on module with no `PartialApply` stays at v7"
    );
    assert!(
        !emits_partial_apply(&module),
        "no `PartialApply` was emitted for a full-arity-only project"
    );
}

/// A flag-on project with one partial call emits bytecode v8 (D13). The real
/// VM run pins the v8 module as decodable and executable end to end.
#[test]
fn project_curry_with_partial_emits_v8() {
    let module = compile_and_run_project("curry_v8");
    assert_eq!(
        module.version,
        fleen_compiler::codegen::MODULE_VERSION_V8,
        "a `PartialApply` instruction lifts the module to v8"
    );
    assert!(
        emits_partial_apply(&module),
        "the partial call site encoded a `PartialApply`"
    );
}

// ---------------------------------------------------------------------------
// std identity (D16): the std sources are flag-off, so the feature table
// records no directive for any of them and std cannot produce a partial.
// ---------------------------------------------------------------------------

/// Every std file is flag-off: the feature table reports `curry == false` for
/// it (T01: only files carrying a directive are flag-on). Std has no `main`,
/// so the flag-off version / `PartialApply` assertions live in
/// `project_curry_without_partials_stays_v7` and the pre-0.0.8 e2e suite.
#[test]
fn std_sources_are_flag_off() {
    let mut session = Session::new();
    let ids: Vec<(String, FileId)> = fleen_compiler::std_src::files()
        .into_iter()
        .map(|(path, source)| {
            let id = session.add_file(path, source);
            (path.to_string(), id)
        })
        .collect();
    let table = session
        .feature_table()
        .unwrap_or_else(|e| panic!("std failed to parse: {e}"));
    for (path, id) in &ids {
        assert!(
            !table.curry(*id),
            "std file `{path}` must not be curried (flag-off)"
        );
    }
}

// ---------------------------------------------------------------------------
// Binary-mode e2e: flag-on valid fixtures, per-mode invalid fixtures, and the
// project-mode programs (cross-module exports + version shapes).
// ---------------------------------------------------------------------------

/// `valid/curry_demo.fln`: two-part declarations, a curried arrow type, an
/// implicit partial, and a partial flowing through a flag-off std helper.
#[test]
fn e2e_curry_demo() {
    let dir = e2e_dir("valid");
    let (code, stdout, stderr) = run_vm(&dir.join("curry_demo.fln"));
    assert_eq!(code, 0, "curry_demo failed: {stderr} (stdout:\n{stdout})");
    let expected =
        fs::read_to_string(dir.join("curry_demo.expected")).expect("read curry_demo.expected");
    assert_eq!(stdout, expected, "curry_demo stdout mismatch");
}

/// `valid/partial_flow.fln`: a partial produced in a flag-on file is called
/// inside flag-off std code (T08 copy-soundness, end to end).
#[test]
fn e2e_partial_flow() {
    let dir = e2e_dir("valid");
    let (code, stdout, stderr) = run_vm(&dir.join("partial_flow.fln"));
    assert_eq!(code, 0, "partial_flow failed: {stderr} (stdout:\n{stdout})");
    let expected =
        fs::read_to_string(dir.join("partial_flow.expected")).expect("read partial_flow.expected");
    assert_eq!(stdout, expected, "partial_flow stdout mismatch");
}

/// `projects/curry_import`: a flag-off importer of a flag-on library keeps
/// today's too-few-args error (D5/D6: the relaxation is a call-site rule).
#[test]
fn e2e_project_curry_import() {
    let dir = project_dir("curry_import");
    let (code, stdout, stderr) = run_vm_args(&[Path::new("--project"), &dir]);
    assert_eq!(code, 0, "curry_import failed: {stderr} (stdout:\n{stdout})");
    let expected = fs::read_to_string(dir.join(".expected")).expect("read curry_import/.expected");
    assert_eq!(stdout, expected, "curry_import stdout mismatch");
}

/// `projects/curry_main`: a flag-on project whose declarations use the
/// two-part form and whose `main` is a zero-param signature/body pair.
#[test]
fn e2e_project_curry_main() {
    let dir = project_dir("curry_main");
    let (code, stdout, stderr) = run_vm_args(&[Path::new("--project"), &dir]);
    assert_eq!(code, 0, "curry_main failed: {stderr} (stdout:\n{stdout})");
    let expected = fs::read_to_string(dir.join(".expected")).expect("read curry_main/.expected");
    assert_eq!(stdout, expected, "curry_main stdout mismatch");
}

/// `projects/curry_flagoff_import`: a flag-off importer of a curried export
/// keeps today's `ArityMismatch` on a too-few-args call (D5/D6/D16).
#[test]
fn e2e_project_curry_flagoff_import() {
    let dir = project_dir("curry_flagoff_import");
    let (code, stdout, stderr) = run_vm_args(&[Path::new("--project"), &dir]);
    let expected_exit: i32 = fs::read_to_string(dir.join(".exit"))
        .unwrap_or_else(|e| panic!("read .exit: {e}"))
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("bad exit code in .exit: {e}"));
    assert_eq!(
        code, expected_exit,
        "expected exit {expected_exit}, got {code}"
    );
    assert!(
        stdout.is_empty(),
        "no stdout on a compile error: {stdout:?}"
    );
    let needle = fs::read_to_string(dir.join(".stderr")).expect("read .stderr");
    assert!(
        stderr.contains(needle.trim()),
        "stderr {stderr:?} does not contain {needle:?}"
    );
}

/// The per-mode invalid fixtures: `curry_*.fln` + `.exit` + `.stderr`.
///
/// - flag-off files (`curry_flagoff_*`) prove the pre-0.0.8 behavior is
///   unchanged (D16): too-few-args is still `ArityMismatch`, a stray
///   directive is still a parse error.
/// - flag-on files (`curry_*` without the prefix) prove the currying rules
///   and the shared errors they keep.
#[test]
fn e2e_curry_invalid() {
    let dir = e2e_dir("invalid");
    let mut names: Vec<String> = fs::read_dir(&dir)
        .expect("read tests/e2e/invalid")
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.extension().map(|e| e == "fln").unwrap_or(false)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("curry_"))
                    .unwrap_or(false)
        })
        .map(|p| {
            p.file_stem()
                .expect("fln has a stem")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    assert!(
        !names.is_empty(),
        "no `curry_*` invalid fixtures found under tests/e2e/invalid"
    );
    for name in &names {
        let base = dir.join(name);
        let (code, stdout, stderr) = run_vm(&base.with_extension("fln"));
        let expected_exit: i32 = fs::read_to_string(base.with_extension("exit"))
            .unwrap_or_else(|e| panic!("read {name}.exit: {e}"))
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("bad exit code in {name}.exit: {e}"));
        let needle = fs::read_to_string(base.with_extension("stderr"))
            .unwrap_or_else(|e| panic!("read {name}.stderr: {e}"));
        assert_eq!(
            code, expected_exit,
            "{name}: exit {code} != {expected_exit} (stderr: {stderr})"
        );
        assert!(stdout.is_empty(), "{name}: unexpected stdout {stdout:?}");
        assert!(
            stderr.contains(needle.trim()),
            "{name}: stderr {stderr:?} does not contain {needle:?}"
        );
    }
}
