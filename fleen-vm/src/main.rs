//! fleen-vm CLI: run a Fleen program from source or bytecode.
//!
//! - `<file.fln>`: compile → verify → execute (one-command driver)
//! - `<file.flnc>`: decode → verify → execute
//!
//! Exit codes: 0 success, 1 compile/decode/verify/runtime failure,
//! 2 usage / IO error.

use std::{env, fs, path::Path, process};

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
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <file.fln | file.flnc>", args[0]);
        process::exit(2);
    }
    let path = Path::new(&args[1]);

    let (module, source) = match load_module(path) {
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
    match Vm::new(module).run() {
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
            eprintln!("{}", render_runtime_error(path, source.as_deref(), &e));
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
