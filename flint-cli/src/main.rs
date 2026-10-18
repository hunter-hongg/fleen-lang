//! flint — the Fleen package manager (0.0.3 V08, F7).
//!
//! A thin workspace crate that drives `fleen-compiler` and `fleen-vm` as
//! libraries (D9). `flint.toml` declares git-only dependencies pinned by a
//! committed `Flint.lock`; `flint build/run/test/check` compile the whole
//! program (project + dependencies + std) in V05 project mode.
//!
//! Commands (PLAN §3.6.2):
//! - `flint new <name>`  — scaffold a project
//! - `flint build`       — resolve → compile → write `<name>.flnc`
//! - `flint run`         — build + execute (trailing args pass to the program)
//! - `flint test`        — run `tests/*.fln` e2e cases (`.expected` contract)
//! - `flint check`       — resolve + compile + verify, no execution
//!
//! Exit codes mirror `fleen-vm`: 0 success, 1 compile/resolve/runtime failure,
//! 2 usage / IO error.

mod build;
mod lock;
mod manifest;
mod resolve;

use std::env;
use std::path::{Path, PathBuf};
use std::process;

use fleen_compiler::codegen::Module;
use fleen_vm::value::Value;
use fleen_vm::vm::Vm;

use crate::build::BuildError;
use crate::lock::{read_lock, write_lock};
use crate::manifest::Manifest;
use crate::resolve::{FlintHome, ResolveError, resolve};

const USAGE: &str = "Usage: flint <new <name> | build | run | test | check>";

fn main() {
    let args: Vec<String> = env::args().collect();
    let code = match dispatch(&args[1..]) {
        Ok(code) => code,
        Err(AppError::Usage(msg)) => {
            eprintln!("{msg}");
            eprintln!("{USAGE}");
            2
        }
        Err(AppError::Fatal(msg)) => {
            eprintln!("error: {msg}");
            1
        }
    };
    process::exit(code);
}

enum AppError {
    Usage(String),
    Fatal(String),
}

impl From<BuildError> for AppError {
    fn from(e: BuildError) -> Self {
        AppError::Fatal(e.0)
    }
}

impl From<ResolveError> for AppError {
    fn from(e: ResolveError) -> Self {
        AppError::Fatal(e.0)
    }
}

fn dispatch(args: &[String]) -> Result<i32, AppError> {
    let Some(cmd) = args.first() else {
        return Err(AppError::Usage("no command given".into()));
    };
    match cmd.as_str() {
        "new" => cmd_new(&args[1..]),
        "build" => {
            let (module, _) = cmd_toolchain()?;
            write_flnc(&module)?;
            Ok(0)
        }
        "check" => {
            // Build + verify, no execution (write_flnc is skipped).
            cmd_toolchain()?;
            Ok(0)
        }
        "run" => {
            let (module, _) = cmd_toolchain()?;
            cmd_run(module, &args[1..])
        }
        "test" => cmd_test(&args[1..]),
        // Hidden: execute a single `.flnc` (used by `flint test` to capture
        // a test's stdout out-of-process).
        "--exec" => {
            let flnc = args
                .get(1)
                .ok_or_else(|| AppError::Usage("`--exec` needs a file".into()))?;
            let bytes =
                std::fs::read(flnc).map_err(|e| AppError::Fatal(format!("reading {flnc}: {e}")))?;
            let module = fleen_compiler::codegen::from_bytes(&bytes)
                .map_err(|e| AppError::Fatal(format!("decode {flnc}: {e}")))?;
            cmd_run(module, &[])
        }
        other => Err(AppError::Usage(format!("unknown command `{other}`"))),
    }
}

/// Resolve → compile → verify. Returns the module and the project directory.
fn cmd_toolchain() -> Result<(Module, PathBuf), AppError> {
    let project = env::current_dir()
        .map_err(|e| AppError::Fatal(format!("cannot determine working directory: {e}")))?;
    let manifest = build::load_manifest(&project)?;
    let mut home = FlintHome::from_env();
    let lock = read_lock(&project).unwrap_or_default();
    let crates = resolve(&manifest, &lock, &mut home)?;
    // Write back the lock so pins are committed even across an early failure.
    if resolve::lock_is_dirty(&lock, &manifest) || !crates.is_empty() {
        write_lock(&project, &manifest.name, &crates);
    }
    let module = build::build_project(&project, &crates)?;
    if let Err(e) = fleen_verify::verify(&module) {
        return Err(AppError::Fatal(format!("verify: FAIL: {e}")));
    }
    Ok((module, project))
}

/// Write the compiled module to `<project>/<name>.flnc`.
fn write_flnc(module: &Module) -> Result<(), AppError> {
    // The caller knows the project dir, but the module itself carries no
    // name — reconstruct from the current dir's basename.
    let cwd = env::current_dir().map_err(|e| AppError::Fatal(e.to_string()))?;
    let name = cwd
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "app".to_string());
    let path = cwd.join(format!("{name}.flnc"));
    let bytes = fleen_compiler::codegen::to_bytes(module);
    std::fs::write(&path, bytes)
        .map_err(|e| AppError::Fatal(format!("writing {}: {e}", path.display())))?;
    println!("built {}", path.display());
    Ok(())
}

/// Execute a compiled module, passing trailing program args.
fn cmd_run(module: Module, args: &[String]) -> Result<i32, AppError> {
    // args[0] == "run", args[1..] are the Fleen program's own args.
    let program_args = args.get(1..).unwrap_or(&[]).to_vec();
    let cwd = env::current_dir().map_err(|e| AppError::Fatal(e.to_string()))?;
    match Vm::with_args(module, program_args).run() {
        Ok(Value::Err(e)) => {
            eprintln!("error: {e}");
            Ok(1)
        }
        Ok(_) => Ok(0),
        Err(e) => {
            if let fleen_vm::error::RuntimeErrorKind::Exit(code) = e.kind {
                return Ok(code as i32);
            }
            let (rendered, _) = render_runtime(&cwd, &e);
            eprintln!("{}", rendered);
            Ok(1)
        }
    }
}

/// Render a runtime error. Multi-file programs embed the file in the span's
/// source map indirectly; we look the source up from the module's globals —
/// simplest is to render without source (the error carries func/pc).
fn render_runtime(_cwd: &Path, e: &fleen_vm::error::RuntimeError) -> (String, Option<String>) {
    // `render_runtime_error` needs a single source string; for a project the
    // runtime error's span_map points at the instruction, and we do not have
    // per-file sources here. Degrade to the message + function/pc form, which
    // is what `.flnc` execution always does.
    let s = fleen_vm::diag::render_runtime_error(Path::new("<project>"), None, e);
    (s, None)
}

/// `flint new <name>` — scaffold a project and (best-effort) git init.
/// `flint new <name-or-path>` — scaffold a project and (best-effort) git
/// init. `name` may be a bare identifier (created in the current directory)
/// or a path (`new /tmp/flint_smoke`); the final path component becomes the
/// package name and must be a valid identifier (a project name is its crate's
/// module root, so hyphens are rejected).
fn cmd_new(args: &[String]) -> Result<i32, AppError> {
    let arg = args
        .first()
        .ok_or_else(|| AppError::Usage("`new` needs a name".into()))?;
    let dir = PathBuf::from(arg);
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::Usage(format!("invalid project path `{arg}`")))?;
    if !manifest::is_valid_package_name(name) {
        return Err(AppError::Usage(format!(
            "invalid project name `{name}` (a project name is its crate's module \
             root: lowercase letters, digits, underscores; not a reserved word)"
        )));
    }
    if dir.exists() {
        return Err(AppError::Fatal(format!(
            "`{}` already exists",
            dir.display()
        )));
    }
    let src = dir.join("src");
    std::fs::create_dir_all(&src).map_err(|e| AppError::Fatal(format!("mkdir: {e}")))?;
    let toml = format!("[package]\nname = {name:?}\nversion = \"0.1.0\"\n\n[dependencies]\n");
    std::fs::write(dir.join("flint.toml"), toml).map_err(|e| AppError::Fatal(e.to_string()))?;
    let main_fln = "import std.io;\n\nfunc main() {\n    std.io.println(\"Hello, World!\");\n}\n";
    std::fs::write(src.join("main.fln"), main_fln).map_err(|e| AppError::Fatal(e.to_string()))?;
    // Best-effort git init; failure (no git, already a repo) is only a note.
    let _ = process::Command::new("git")
        .args(["init"])
        .current_dir(&dir)
        .status();
    println!(
        "Created `{}` (cd {} && cargo run -q -p flint-cli -- run)",
        dir.display(),
        name
    );
    Ok(0)
}

/// `flint test` — run the project's `tests/*.fln` e2e cases.
fn cmd_test(_args: &[String]) -> Result<i32, AppError> {
    let project = env::current_dir().map_err(|e| AppError::Fatal(e.to_string()))?;
    let manifest = build::load_manifest(&project)?;
    let mut home = FlintHome::from_env();
    let lock = read_lock(&project).unwrap_or_default();
    let crates = resolve(&manifest, &lock, &mut home)?;
    let tests_dir = project.join("tests");
    let mut ran = 0;
    let mut failed = 0;
    let mut total = 0;
    if let Ok(read) = std::fs::read_dir(&tests_dir) {
        let mut files: Vec<PathBuf> = read.filter_map(|e| e.ok()).map(|e| e.path()).collect();
        files.sort();
        for path in files {
            if path.extension().and_then(|e| e.to_str()) != Some("fln") {
                continue;
            }
            total += 1;
            run_one_test(&project, &manifest, &crates, &path, &mut ran, &mut failed)?;
        }
    }
    if total == 0 {
        return Err(AppError::Fatal("no `tests/*.fln` cases found".into()));
    }
    println!("test result: {ran} passed, {failed} failed");
    if failed == 0 { Ok(0) } else { Ok(1) }
}

fn run_one_test(
    project: &Path,
    manifest: &Manifest,
    _crates: &[resolve::ResolvedCrate],
    test: &Path,
    ran: &mut i32,
    failed: &mut i32,
) -> Result<(), AppError> {
    // For 0.0.3, a test case is a standalone `.fln` file (dependency crates in
    // `flint test` are a follow-up; the harness contract — `.fln`/`.expected`
    // comparison — is what matters now).
    let _ = project;
    let _ = manifest;
    let source = std::fs::read_to_string(test)
        .map_err(|e| AppError::Fatal(format!("reading {}: {e}", test.display())))?;
    let test_disp = test.display().to_string();
    let expected = test.with_extension("expected");
    let expected_text = std::fs::read_to_string(&expected).ok();

    let compiled = match fleen_compiler::compile(&source) {
        Ok(m) => m,
        Err(e) => {
            *failed += 1;
            println!("test {test_disp} ... FAILED (compile: {e})");
            return Ok(());
        }
    };
    if let Err(e) = fleen_verify::verify(&compiled) {
        *failed += 1;
        println!("test {test_disp} ... FAILED (verify: {e})");
        return Ok(());
    }
    // Execute a copy of the .flnc through this same binary's `--exec` path,
    // capturing stdout so it can be compared against `.expected` without
    // leaking into flint's own logs.
    let flnc = std::env::temp_dir().join(format!("flint-test-{}.flnc", std::process::id()));
    if let Err(e) = std::fs::write(&flnc, fleen_compiler::codegen::to_bytes(&compiled)) {
        *failed += 1;
        println!("test {test_disp} ... FAILED (write temp: {e})");
        return Ok(());
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let output = std::process::Command::new(&exe)
        .arg("--exec")
        .arg(&flnc)
        .output();
    let _ = std::fs::remove_file(&flnc);

    let (ok, got, details) = match output {
        Ok(inner) => (
            inner.status.success(),
            String::from_utf8_lossy(&inner.stdout).into_owned(),
            String::from_utf8_lossy(&inner.stderr).into_owned(),
        ),
        Err(e) => (false, String::new(), format!("{e}")),
    };

    match (ok, expected_text) {
        (true, Some(expected_text)) if got == expected_text => {
            *ran += 1;
            println!("test {test_disp} ... ok");
        }
        (true, Some(expected_text)) => {
            *failed += 1;
            println!(
                "test {test_disp} ... FAILED (output mismatch)\n  expected: {:?}\n  got:      {:?}",
                expected_text, got
            );
        }
        (true, None) => {
            *ran += 1;
            println!("test {test_disp} ... ok");
        }
        (false, _) => {
            *failed += 1;
            let details = if details.is_empty() { got } else { details };
            println!(
                "test {test_disp} ... FAILED (runtime)\n  {}",
                details.trim()
            );
        }
    }
    Ok(())
}
