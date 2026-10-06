//! CLI: read a `.flnc` module, verify it, exit 0/1.

use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <file.flnc>", args[0]);
        process::exit(2);
    }
    let bytes = match fs::read(&args[1]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Error reading {}: {}", args[1], e);
            process::exit(2);
        }
    };
    let module = match fleen_compiler::codegen::from_bytes(&bytes) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Decode error: {e}");
            process::exit(1);
        }
    };
    match fleen_verify::verify(&module) {
        Ok(()) => println!("verify: PASS"),
        Err(e) => {
            eprintln!("verify: FAIL: {e}");
            process::exit(1);
        }
    }
}
