//! V09: the whole-repo **examples sweep**.
//!
//! Every example under `examples/` (0.0.1, 0.0.2 and the 0.0.3 project)
//! must compile, verify and execute on every `cargo test` — an example that
//! rots fails loudly instead of silently drifting out of date.
//!
//! Two kinds of example are exercised:
//!   - single-file `.fln` examples: run directly. They must exit 0 with an
//!     empty stderr, unless they opt into the e2e invalid contract via a
//!     sibling `.exit`/`.stderr` file (the `err_exit.fln` demo).
//!   - project examples: a dir with `main.fln` / `src/main.fln` is run in
//!     `--project` mode.
//!
//! This is also the V09 **reserved-word sweep**: all examples are compiled
//! here, so any accidental use of `struct` / `for` / `in` / `pub` as an
//! *identifier* would fail to compile and surface as a test failure.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Path to the `fleen-vm` binary built for this test run.
const VM: &str = env!("CARGO_BIN_EXE_fleen-vm");

fn examples_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples")
}

/// A directory is a project root if running it needs `--project` mode.
fn is_project(dir: &Path) -> bool {
    dir.join("main.fln").is_file() || dir.join("src").join("main.fln").is_file()
}

#[test]
fn single_file_examples_compile_verify_and_run() {
    let mut ran = 0;
    collect_single(&examples_root(), &mut ran);
    assert!(ran > 0, "no single-file examples found");
}

fn collect_single(dir: &Path, ran: &mut usize) {
    for entry in fs::read_dir(dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            if is_project(&path) {
                continue; // handled by project_example_compiles_verify_and_run
            }
            if path.file_name().and_then(|s| s.to_str()) == Some("src") {
                continue; // module tree of a project above
            }
            collect_single(&path, ran);
        } else if path.extension().is_some_and(|ext| ext == "fln") {
            run_single(&path);
            *ran += 1;
        }
    }
}

/// Run one standalone `.fln`, honoring the optional `.exit`/`.stderr`
/// contract (the e2e invalid convention, reused so an intentionally failing
/// example like `err_exit.fln` is legal).
fn run_single(path: &Path) {
    let output = Command::new(VM)
        .arg(path)
        .output()
        .unwrap_or_else(|e| panic!("spawn {VM}: {e}"));
    let code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

    let exit_path = path.with_extension("exit");
    let stderr_path = path.with_extension("stderr");
    if exit_path.is_file() {
        let expected: i32 = fs::read_to_string(&exit_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", exit_path.display()))
            .trim()
            .parse()
            .expect("bad .exit code");
        assert_eq!(
            code,
            expected,
            "{}: exit code (stderr: {stderr})",
            path.display()
        );
        if stderr_path.is_file() {
            let needle = fs::read_to_string(&stderr_path)
                .unwrap_or_else(|e| panic!("read {}: {e}", stderr_path.display()))
                .trim()
                .to_owned();
            assert!(
                stderr.contains(&needle),
                "{}: stderr {stderr:?} does not contain {needle:?}",
                path.display()
            );
        }
    } else {
        assert_eq!(code, 0, "{}: exit code (stderr: {stderr})", path.display());
        assert!(
            stderr.is_empty(),
            "{}: unexpected stderr: {stderr}",
            path.display()
        );
    }
}

#[test]
fn project_examples_compile_verify_and_run() {
    let mut projects = Vec::new();
    collect_projects(&examples_root(), &mut projects);
    assert!(!projects.is_empty(), "no example projects found");
    for proj in projects {
        let output = Command::new(VM)
            .arg("--project")
            .arg(&proj)
            .output()
            .unwrap_or_else(|e| panic!("spawn {VM}: {e}"));
        let code = output.status.code().unwrap_or(-1);
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert_eq!(code, 0, "{}: exit code (stderr: {stderr})", proj.display());
        assert!(
            stderr.is_empty(),
            "{}: unexpected stderr: {stderr}",
            proj.display()
        );
    }
}

fn collect_projects(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if !path.is_dir() {
            continue;
        }
        if is_project(&path) {
            out.push(path);
        } else {
            collect_projects(&path, out);
        }
    }
}
