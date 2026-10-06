use std::env;
use std::fs;
use std::process;

/// CLI: compile a `.fln` source file to a `.flnc` bytecode module.
///
/// Usage: `fleen-compiler <file.fln> [-o <out.flnc>]`
///
/// `-o` selects the output path; by default the bytecode is written next to
/// the source with a `.flnc` extension.
///
/// Exit codes: 0 success, 1 compile failure, 2 usage / IO error.
fn main() {
    let mut file_path: Option<String> = None;
    let mut out_path: Option<String> = None;

    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-o" => match args.next() {
                Some(path) => out_path = Some(path),
                None => usage("Error: -o requires a path argument"),
            },
            other => {
                if file_path.replace(other.to_string()).is_some() {
                    usage("Error: only one input file is allowed");
                }
            }
        }
    }

    let file_path = match file_path {
        Some(p) => p,
        None => usage("Error: missing input file"),
    };

    let source = match fs::read_to_string(&file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file {}: {}", file_path, e);
            process::exit(2);
        }
    };

    // Lex
    println!("=== Lexing ===");
    let tokens = match fleen_compiler::lexer::tokenize(&source) {
        Ok(t) => t,
        Err(errors) => {
            eprintln!("Lexing errors:");
            for err in errors {
                eprintln!("  {:?}", err);
            }
            process::exit(1);
        }
    };
    println!("Successfully tokenized {} tokens", tokens.len());

    // Parse
    println!("\n=== Parsing ===");
    let ast = match fleen_compiler::parser::parse(tokens) {
        Ok(ast) => ast,
        Err(err) => {
            eprintln!("Parsing error: {:?}", err);
            process::exit(1);
        }
    };
    println!("Successfully parsed {} items", ast.items.len());

    // Print AST summary
    println!("\n=== AST Summary ===");
    for (i, item) in ast.items.iter().enumerate() {
        println!("Item {}: {:?}", i, item);
    }

    println!("\n=== Success ===");

    // Resolve
    let hir = match fleen_compiler::resolver::resolve(ast) {
        Ok(h) => h,
        Err(errors) => {
            eprintln!("Resolve errors:");
            for err in errors {
                eprintln!("  {:?}", err);
            }
            process::exit(1);
        }
    };

    // Typeck
    let typed = match fleen_compiler::typeck::typeck(hir) {
        Ok(t) => t,
        Err(errors) => {
            eprintln!("Type errors:");
            for err in errors {
                eprintln!("  {:?}", err);
            }
            process::exit(1);
        }
    };

    // Lower
    let mir = match fleen_compiler::lower::lower(typed) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("Lower error: {:?}", err);
            process::exit(1);
        }
    };

    // Codegen
    let module = match fleen_compiler::codegen::codegen(mir) {
        Ok(m) => m,
        Err(err) => {
            eprintln!("Codegen error: {:?}", err);
            process::exit(1);
        }
    };

    // Write .flnc
    let out = out_path.unwrap_or_else(|| {
        std::path::Path::new(&file_path)
            .with_extension("flnc")
            .display()
            .to_string()
    });
    let bytes = fleen_compiler::codegen::to_bytes(&module);
    if let Err(e) = fs::write(&out, &bytes) {
        eprintln!("Error writing {}: {}", out, e);
        process::exit(2);
    }
    println!("\nWrote {} ({} bytes)", out, bytes.len());
}

/// Print usage (with an optional reason) and exit with the usage code.
fn usage(message: &str) -> ! {
    if !message.is_empty() {
        eprintln!("{message}");
    }
    eprintln!("Usage: fleen-compiler <file.fln> [-o <out.flnc>]");
    process::exit(2);
}
