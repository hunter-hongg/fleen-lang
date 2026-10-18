//! End-to-end tests for flint (0.0.3 V08): drive the real `flint-cli` binary
//! against a local `file://` git dependency fixture (no network — the CI
//! rule). Fixture repositories are created on the spot by the test setup.
//!
//! The VM stdout is captured by `flint test` and compared against `.expected`
//! files, matching the fleen e2e contract.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_flint-cli");

/// A throwaway working directory for one test.
struct Temp(PathBuf);

impl Temp {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("flint-e2e-{tag}-{}", std::process::id()));
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

fn run_in(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("spawn flint in {}: {e}", dir.display()));
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

/// Write a file and its parent dirs.
fn write(path: &Path, contents: &str) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).expect("create parent");
    }
    std::fs::write(path, contents).expect("write file");
}

/// `git init` + commit a git project root with the given files (map of
/// relative path → contents).
fn git_project(dir: &Path, files: &[(&str, &str)]) {
    for (rel, contents) in files {
        write(&dir.join(rel), contents);
    }
    let ok = Command::new("git")
        .args(["init", "-q", "-b", "main"])
        .current_dir(dir)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if !ok {
        panic!("git init failed (is `git` on PATH?) in {}", dir.display());
    }
    let _ = Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(["add", "-A"])
        .current_dir(dir)
        .status();
    let commit = Command::new("git")
        .args(["-c", "user.email=t@t", "-c", "user.name=t"])
        .args(["commit", "-qm", "init"])
        .current_dir(dir)
        .status();
    assert!(
        commit.map(|s| s.success()).unwrap_or(false),
        "git commit failed"
    );
}

/// A crate's flint.toml writer.
fn crate_toml(name: &str) -> String {
    format!("[package]\nname = {name:?}\nversion = \"0.1.0\"\n\n[dependencies]\n")
}

#[test]
fn flint_new_scaffolds_and_runs_hello() {
    let tmp = Temp::new("new");
    let proj = tmp.path().join("hello_proj");
    let (code, out, err) = run_in(tmp.path(), &["new", "hello_proj"]);
    assert_eq!(code, 0, "flint new exit (stderr: {err})");
    assert!(proj.join("flint.toml").is_file(), "flint.toml scaffolded");
    assert!(
        proj.join("src/main.fln").is_file(),
        "src/main.fln scaffolded"
    );
    let _ = out;

    // Run it with an isolated cache.
    let (code, out, err) = run_in(&proj, &["run"]);
    assert_eq!(code, 0, "hello run (stderr: {err})");
    assert_eq!(out, "Hello, World!\n", "hello stdout");
}

#[test]
fn flint_resolves_a_file_git_dependency_and_runs() {
    let tmp = Temp::new("dep");
    let dep = tmp.path().join("depdemo_lib");
    // The dependency exposes a `util` submodule whose `helper` is called across
    // the crate boundary as `depdemo_lib.util.helper` (the ticket's contract).
    git_project(
        &dep,
        &[
            ("flint.toml", &crate_toml("depdemo_lib")),
            // A non-empty crate root; the `util` submodule sits alongside it.
            ("src/main.fln", "pub func root(): int = 0\n"),
            (
                "src/util.fln",
                "import std.io;\nimport std.str;\n\
                 pub func helper(n: string): string = \
                 std.str.concat(\"Hi \", std.str.concat(n, \"!\"))\n",
            ),
        ],
    );

    let proj = tmp.path().join("app");
    write(
        &proj.join("flint.toml"),
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             depdemo_lib = {{ git = \"file://{}\" }}\n",
            dep.display()
        ),
    );
    write(
        &proj.join("src/main.fln"),
        "import std.io;\nimport depdemo_lib.util;\nfunc main() {\n    \
         std.io.println(depdemo_lib.util.helper(\"flint\"));\n}\n",
    );

    let (code, out, err) = run_in(&proj, &["run"]);
    assert_eq!(code, 0, "dep run (stderr: {err})");
    assert_eq!(out, "Hi flint!\n", "dep output");

    // A lockfile is written and pins the resolved commit.
    let lock = std::fs::read_to_string(proj.join("Flint.lock")).expect("Flint.lock written");
    assert!(lock.contains("depdemo_lib"), "lock names the dep: {lock}");
    assert!(lock.contains("commit"), "lock pins a commit: {lock}");

    // Second run reuses the cache without network (kept fast by the cache).
    let (code, out, err) = run_in(&proj, &["run"]);
    assert_eq!(code, 0, "dep run (second) (stderr: {err})");
    assert_eq!(out, "Hi flint!\n", "dep output (cached)");
}

#[test]
fn flint_private_dependency_item_is_rejected_cross_crate() {
    let tmp = Temp::new("priv");
    let dep = tmp.path().join("shylib");
    git_project(
        &dep,
        &[
            ("flint.toml", &crate_toml("shylib")),
            // NOT `pub` — must be invisible across the crate boundary.
            ("src/main.fln", "func hidden(): int = 42\n"),
        ],
    );

    let proj = tmp.path().join("app");
    write(
        &proj.join("flint.toml"),
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             shylib = {{ git = \"file://{}\" }}\n",
            dep.display()
        ),
    );
    write(
        &proj.join("src/main.fln"),
        "import std.io;\nimport shylib;\nfunc main() {\n    \
         std.io.println(shylib.hidden() as string);\n}\n",
    );

    let (code, _out, err) = run_in(&proj, &["run"]);
    assert_ne!(code, 0, "private access must fail");
    assert!(
        err.contains("is private to module `shylib`"),
        "stderr names the private item: {err}"
    );
}

#[test]
fn flint_missing_dependency_module_reports_tried_paths() {
    let tmp = Temp::new("missing");
    let dep = tmp.path().join("slimlib");
    git_project(
        &dep,
        &[
            ("flint.toml", &crate_toml("slimlib")),
            ("src/main.fln", "pub func hello(): int = 1\n"),
        ],
    );

    let proj = tmp.path().join("app");
    write(
        &proj.join("flint.toml"),
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             slimlib = {{ git = \"file://{}\" }}\n",
            dep.display()
        ),
    );
    write(
        &proj.join("src/main.fln"),
        "import slimlib.nosuch;\nfunc main() {}\n",
    );

    let (code, _out, err) = run_in(&proj, &["run"]);
    assert_ne!(code, 0, "missing module must fail");
    assert!(err.contains("nosuch"), "stderr names the module: {err}");
    assert!(
        err.contains("tried") || err.contains("nosuch.fln"),
        "stderr suggests tried paths: {err}"
    );
}

#[test]
fn flint_test_compares_against_expected() {
    let tmp = Temp::new("test");
    let proj = tmp.path().join("app");
    write(
        &proj.join("flint.toml"),
        "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n",
    );
    write(
        &proj.join("src/main.fln"),
        "import std.io;\nfunc main() { std.io.println(\"hello\"); }\n",
    );
    write(
        &proj.join("tests/greet.fln"),
        "import std.io;\nfunc main() { std.io.println(\"greet\"); }\n",
    );
    write(&proj.join("tests/greet.expected"), "greet\n");

    let (code, out, err) = run_in(&proj, &["test"]);
    assert_eq!(code, 0, "flint test pass (stderr: {err})");
    assert!(out.contains("1 passed, 0 failed"), "summary: {out}");
}

#[test]
fn flint_dependency_graph_cycle_is_reported() {
    let tmp = Temp::new("cycle");
    // A depends on B, and B depends on A — a cycle that must be reported.
    let a = tmp.path().join("crate_a");
    git_project(
        &a,
        &[
            (
                "flint.toml",
                &dependency_toml("crate_a", "crate_b", &tmp.path().join("crate_b")),
            ),
            ("src/main.fln", "pub func a(): int = 1\n"),
        ],
    );
    let b = tmp.path().join("crate_b");
    git_project(
        &b,
        &[
            ("flint.toml", &dependency_toml("crate_b", "crate_a", &a)),
            ("src/main.fln", "pub func b(): int = 2\n"),
        ],
    );

    let proj = tmp.path().join("app");
    write(
        &proj.join("flint.toml"),
        &format!(
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n[dependencies]\n\
             crate_a = {{ git = \"file://{}\" }}\n",
            a.display()
        ),
    );
    write(
        &proj.join("src/main.fln"),
        "import crate_a;\nfunc main() {}\n",
    );

    let (code, _out, err) = run_in(&proj, &["run"]);
    assert_ne!(code, 0, "a dependency cycle must fail the build");
    assert!(
        err.contains("dependency cycle"),
        "stderr reports the cycle: {err}"
    );
}

/// A flint.toml with one git dependency.
fn dependency_toml(name: &str, dep: &str, dep_dir: &Path) -> String {
    format!(
        "[package]\nname = {name:?}\nversion = \"0.1.0\"\n\n[dependencies]\n\
         {dep} = {{ git = \"file://{}\" }}\n",
        dep_dir.display()
    )
}
