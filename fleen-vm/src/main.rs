//! fleen-vm CLI: run a Fleen program from source or bytecode.
//!
//! - `<file.fln>`: compile → verify → execute (one-command driver)
//! - `<file.flnc>`: decode → verify → execute
//!
//! Exit codes: 0 success, 1 compile/decode/verify/runtime failure,
//! 2 usage / IO error.

use std::{env, fs, path::Path, process};

use fleen_compiler::Session;
use fleen_compiler::codegen::Module;

use fleen_vm::diag::render_runtime_error;
use fleen_vm::value::Value;
use fleen_vm::vm::Vm;

/// How loading the input module failed; each variant maps to an exit code.
enum LoadError {
    /// Input file could not be read (exit code 2).
    Io(std::io::Error),
    /// Input path is not a `.fln`/`.flnc` file (exit code 2).
    Usage,
    /// `.fln` source failed to compile (exit code 1).
    Compile(fleen_compiler::CompileError),
    /// `.flnc` bytecode could not be decoded (exit code 1).
    Decode(fleen_compiler::codegen::FlncError),
}

fn main() {
    // Trailing arguments after the input path (0.0.3 V06) are the Fleen
    // program's own arguments, served by `std.os.args` (without argv[0]).
    let args: Vec<String> = env::args().collect();
    let (path, project, program_args) = if args.len() >= 3 && args[1] == "--project" {
        (Path::new(&args[2]).to_path_buf(), true, args[3..].to_vec())
    } else if args.len() >= 2 {
        (Path::new(&args[1]).to_path_buf(), false, args[2..].to_vec())
    } else {
        eprintln!(
            "Usage: {} [--project <dir>] <file.fln | file.flnc> [program args...]",
            args[0]
        );
        process::exit(2);
    };

    let (module, source) = match if project {
        load_project(&path)
    } else {
        load_module(&path)
    } {
        Ok(m) => m,
        Err(LoadError::Io(e)) => {
            eprintln!("Error reading {}: {}", path.display(), e);
            process::exit(2);
        }
        Err(LoadError::Usage) => {
            eprintln!(
                "Unsupported input: {} (expected .fln or .flnc)",
                path.display()
            );
            process::exit(2);
        }
        Err(LoadError::Compile(e)) => {
            eprintln!("Compile error: {e}");
            process::exit(1);
        }
        Err(LoadError::Decode(e)) => {
            eprintln!("Decode error: {e}");
            process::exit(1);
        }
    };

    // Bytecode is never executed without passing static verification.
    if let Err(e) = fleen_verify::verify(&module) {
        eprintln!("verify: FAIL: {e}");
        process::exit(1);
    }
    match Vm::with_args(module, program_args).run() {
        // 0.0.2 U09 (D8): an entry function returning `Result` propagates a
        // trailing `Err` (via `?` or an explicit return) out of execution.
        // Print the payload to stderr and exit 1; the exit-code contract
        // (0 success / 1 error / 2 usage·IO) is unchanged.
        Ok(Value::Err(e)) => {
            eprintln!("error: {e}");
            process::exit(1);
        }
        // Any other module result keeps the 0.0.1 behavior (silently
        // discarded).
        Ok(_) => {}
        Err(e) => {
            // `std.os.exit(code)` (0.0.3 V06) is not a failure: the intrinsic
            // unwinds through the runtime-error channel, and the CLI passes
            // the code straight through without rendering anything.
            if let fleen_vm::error::RuntimeErrorKind::Exit(code) = e.kind {
                process::exit(code as i32);
            }
            eprintln!("{}", render_runtime_error(&path, source.as_deref(), &e));
            process::exit(1);
        }
    }
}

/// Load a module from `.fln` source (full pipeline) or `.flnc` bytes.
fn load_module(path: &Path) -> Result<(Module, Option<String>), LoadError> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("fln") => {
            let source = fs::read_to_string(path).map_err(LoadError::Io)?;
            let module = fleen_compiler::compile(&source).map_err(LoadError::Compile)?;
            Ok((module, Some(source)))
        }
        Some("flnc") => {
            let bytes = fs::read(path).map_err(LoadError::Io)?;
            let module = fleen_compiler::codegen::from_bytes(&bytes).map_err(LoadError::Decode)?;
            Ok((module, None))
        }
        _ => Err(LoadError::Usage),
    }
}

/// Load a multi-file project: `<dir>/src/main.fln` (or `<dir>/main.fln` as
/// a fallback) is the crate root; other `.fln` files under the same source
/// root are discovered from import declarations by the compiler's module
/// table sub-pass.
fn load_project(project: &Path) -> Result<(Module, Option<String>), LoadError> {
    let source_dir = if project.join("src").join("main.fln").is_file() {
        project.join("src")
    } else if project.join("main.fln").is_file() {
        project.to_path_buf()
    } else {
        return Err(LoadError::Usage);
    };

    let mut files = Vec::new();
    collect_fln_files(&source_dir, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err(LoadError::Usage);
    }

    let mut session = Session::new();
    let mut root = None;
    for path in files {
        let rel = path.strip_prefix(project).unwrap_or(&path);
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let source = fs::read_to_string(&path).map_err(LoadError::Io)?;
        let id = session.add_file(rel_str.clone(), source);
        if rel == Path::new("src/main.fln") || rel == Path::new("main.fln") {
            root = Some(id);
        }
    }

    let root = root.ok_or(LoadError::Usage)?;
    let module = session.compile_project(root).map_err(LoadError::Compile)?;
    Ok((module, None))
}

/// Recursively collect `.fln` files beneath `dir`.
fn collect_fln_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<(), LoadError> {
    let entries = fs::read_dir(dir).map_err(LoadError::Io)?;
    for entry in entries {
        let entry = entry.map_err(LoadError::Io)?;
        let path = entry.path();
        if path.is_dir() {
            collect_fln_files(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("fln") {
            out.push(path);
        }
    }
    Ok(())
}
