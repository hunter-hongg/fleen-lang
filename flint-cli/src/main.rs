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
//! The CLI is clap-derive (0.0.5 D6) with the uniform `--quiet` / `--no-color`
//! flag set. Exit codes mirror `fleen-vm`: 0 success, 1 compile/resolve/
//! runtime failure, 2 usage / IO error.

mod build;
mod lock;
mod manifest;
mod resolve;

use std::env;
use std::path::{Path, PathBuf};
use std::process;

use clap::Parser;
use fleen_compiler::codegen::Module;
use fleen_vm::error::RuntimeErrorKind;
use fleen_vm::value::Value;
use fleen_vm::vm::Vm;

use crate::build::BuildError;
use crate::lock::{read_lock, write_lock};
use crate::manifest::Manifest;
use crate::resolve::{FlintHome, ResolveError, resolve};

#[derive(Parser)]
#[command(
    name = "flint",
    version,
    about = "Fleen package manager",
    arg_required_else_help = true,
    after_help = "Exit codes: 0 success, 1 build/resolve/runtime failure, 2 usage/IO error."
)]
struct Cli {
    /// Suppress informational output and program stdout.
    #[arg(long, global = true)]
    quiet: bool,
    /// Disable ANSI color in diagnostics.
    #[arg(long, global = true)]
    no_color: bool,
    #[command(subcommand)]
    command: Commands,
}

/// The flint subcommands.
#[derive(clap::Subcommand)]
enum Commands {
    /// Create a new project.
    #[command(about = "Create a new project")]
    New {
        /// Project name (or path); the final component becomes the package name.
        name: String,
    },
    /// Resolve, compile, and write the project bytecode.
    #[command(about = "Resolve, compile, and write the project bytecode")]
    Build,
    /// Build and execute the project.
    #[command(about = "Build and execute the project")]
    Run {
        /// Program arguments passed after `--`.
        #[arg(last = true, value_name = "program")]
        program_args: Vec<String>,
    },
    /// Run the project tests.
    #[command(about = "Run the project tests")]
    Test,
    /// Resolve, compile, and verify without executing.
    #[command(about = "Resolve, compile, and verify without executing")]
    Check {
        /// Report which std copy is in effect and its version.
        #[arg(long)]
        std: bool,
    },
    /// Internal: execute a compiled `.flnc` (used by `flint test`).
    #[command(hide = true)]
    Exec {
        /// The compiled module to execute.
        file: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    let color = fleen_compiler::diag::color_enabled(cli.no_color);
    let code = match dispatch(cli, color) {
        Ok(code) => code,
        Err(AppError::Usage(msg)) => {
            eprintln!("{}", fleen_compiler::diag::colorize(&msg, color));
            2
        }
        Err(AppError::Fatal(msg)) => {
            eprintln!("error: {}", fleen_compiler::diag::colorize(&msg, color));
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

/// Route a parsed [`Cli`] to the matching command handler.
fn dispatch(cli: Cli, color: bool) -> Result<i32, AppError> {
    let quiet = cli.quiet;
    match cli.command {
        Commands::New { name } => cmd_new(&name, quiet, color),
        Commands::Build => cmd_build(quiet),
        Commands::Run { program_args } => {
            let (module, _) = cmd_toolchain()?;
            cmd_run(module, &program_args, quiet, color)
        }
        Commands::Test => cmd_test(quiet, color),
        Commands::Check { std } => cmd_check(std),
        Commands::Exec { file } => {
            let bytes = std::fs::read(&file)
                .map_err(|e| AppError::Fatal(format!("reading {}: {e}", file.display())))?;
            let module = fleen_compiler::codegen::from_bytes(&bytes)
                .map_err(|e| AppError::Fatal(format!("decode {}: {e}", file.display())))?;
            cmd_run(module, &[], quiet, color)
        }
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

/// `flint build` — resolve → compile → write `<name>.flnc`.
fn cmd_build(quiet: bool) -> Result<i32, AppError> {
    let (module, _) = cmd_toolchain()?;
    write_flnc(&module, quiet)?;
    Ok(0)
}

/// `flint check` — resolve → compile → verify, no execution.
fn cmd_check(std_flag: bool) -> Result<i32, AppError> {
    if std_flag {
        return cmd_check_std();
    }
    cmd_toolchain()?;
    Ok(0)
}

/// `flint check --std` — report which std copy is in effect.
fn cmd_check_std() -> Result<i32, AppError> {
    let resolver = fleen_compiler::std_resolver::StdResolver::from_compiler_version();
    match resolver.diagnose() {
        Ok(diag) => {
            println!("Std source: {}", diag.source.description());
            if let Some(path) = diag.path {
                println!("Std path: {}", path.display());
            }
            println!("Std version: {}", diag.version);
            Ok(0)
        }
        Err(e) => {
            eprintln!("error: {}", e);
            Ok(1)
        }
    }
}

/// Write the compiled module to `<project>/<name>.flnc`.
fn write_flnc(module: &Module, quiet: bool) -> Result<(), AppError> {
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
    if !quiet {
        println!("built {}", path.display());
    }
    Ok(())
}

/// Execute a compiled module, passing trailing program args.
fn cmd_run(
    module: Module,
    program_args: &[String],
    quiet: bool,
    color: bool,
) -> Result<i32, AppError> {
    let mut vm = Vm::with_args(module, program_args.to_vec()).with_quiet(quiet);
    match vm.run() {
        Ok(Value::Err(e)) => {
            eprintln!(
                "error: {}",
                fleen_compiler::diag::colorize(&e.to_string(), color)
            );
            Ok(1)
        }
        Ok(_) => Ok(0),
        Err(e) => {
            if let RuntimeErrorKind::Exit(code) = &e.kind {
                return Ok(*code as i32);
            }
            let rendered = fleen_vm::diag::render_runtime_error_colored(
                Path::new("<project>"),
                None,
                &e,
                color,
            );
            eprintln!("{rendered}");
            Ok(1)
        }
    }
}

/// `flint new <name>` — scaffold a project and (best-effort) git init.
/// `name` may be a bare identifier (created in the current directory) or a
/// path (`new /tmp/flint_smoke`); the final path component becomes the
/// package name and must be a valid identifier (a project name is its crate's
/// module root, so hyphens are rejected).
fn cmd_new(name: &str, quiet: bool, _color: bool) -> Result<i32, AppError> {
    let dir = PathBuf::from(name);
    let name = dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::Usage(format!("invalid project path `{name}`")))?;
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
        .stdout(process::Stdio::null())
        .stderr(process::Stdio::null())
        .status();
    if !quiet {
        println!(
            "Created `{}` (cd {} && cargo run -q -p flint-cli -- run)",
            dir.display(),
            name
        );
    }
    Ok(0)
}

/// `flint test` — run the project's `tests/*.fln` e2e cases.
fn cmd_test(quiet: bool, _color: bool) -> Result<i32, AppError> {
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
            run_one_test(
                &project,
                &manifest,
                &crates,
                &path,
                &mut ran,
                &mut failed,
                quiet,
            )?;
        }
    }
    if total == 0 {
        return Err(AppError::Fatal("no `tests/*.fln` cases found".into()));
    }
    if !quiet {
        println!("test result: {ran} passed, {failed} failed");
    }
    if failed == 0 { Ok(0) } else { Ok(1) }
}

fn run_one_test(
    project: &Path,
    manifest: &Manifest,
    _crates: &[resolve::ResolvedCrate],
    test: &Path,
    ran: &mut i32,
    failed: &mut i32,
    quiet: bool,
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
    // Execute a copy of the .flnc through this same binary's `exec` subcommand,
    // capturing stdout so it can be compared against `.expected` without
    // leaking into flint's own logs.
    let flnc = std::env::temp_dir().join(format!("flint-test-{}.flnc", std::process::id()));
    if let Err(e) = std::fs::write(&flnc, fleen_compiler::codegen::to_bytes(&compiled)) {
        *failed += 1;
        println!("test {test_disp} ... FAILED (write temp: {e})");
        return Ok(());
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let output = process::Command::new(&exe).arg("exec").arg(&flnc).output();
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
            if !quiet {
                println!("test {test_disp} ... ok");
            }
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
            if !quiet {
                println!("test {test_disp} ... ok");
            }
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
