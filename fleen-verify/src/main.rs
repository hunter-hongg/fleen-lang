//! CLI: read a `.flnc` module, verify it, exit 0/1.

use std::{fs, path::PathBuf, process};

use clap::Parser;

/// Command-line options for `fleen-verify`.
#[derive(Parser)]
#[command(
    name = "fleen-verify",
    version,
    about = "Statically verify a Fleen bytecode module",
    after_help = "Exit codes: 0 pass, 1 decode/verify failure, 2 usage/IO error."
)]
struct Cli {
    /// Path to the `.flnc` bytecode module to verify.
    file: PathBuf,

    /// Accepted for CLI compatibility; does not change verification.
    ///
    /// The other Fleen drivers accept `--project <dir>` to verify a whole
    /// project tree. This driver always verifies the single `.flnc` module
    /// named by `file`, so the project root is intentionally a no-op.
    #[arg(long, value_name = "DIR")]
    project: Option<PathBuf>,

    /// Suppress the "verify: PASS" success line.
    #[arg(long)]
    quiet: bool,

    /// Disable ANSI color in error output.
    #[arg(long)]
    no_color: bool,
}

fn main() {
    let cli = Cli::parse();

    // `--project` is accepted for CLI compatibility; verification always
    // operates on the single `.flnc` module named by `file`.
    let _ = &cli.project;

    let color = fleen_compiler::diag::color_enabled(cli.no_color);

    let bytes = match fs::read(&cli.file) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error reading {}: {}", cli.file.display(), e);
            process::exit(2);
        }
    };
    let module = match fleen_compiler::codegen::from_bytes(&bytes) {
        Ok(m) => m,
        Err(e) => {
            let msg = format!("Decode error: {e}");
            eprintln!("{}", fleen_compiler::diag::colorize(&msg, color));
            process::exit(1);
        }
    };
    match fleen_verify::verify(&module) {
        Ok(()) => {
            if !cli.quiet {
                println!("verify: PASS");
            }
        }
        Err(e) => {
            let msg = format!("verify: FAIL: {e}");
            eprintln!("{}", fleen_compiler::diag::colorize(&msg, color));
            process::exit(1);
        }
    }
}
