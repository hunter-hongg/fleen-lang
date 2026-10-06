use std::env;
use std::fs;
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: {} <file.fln>", args[0]);
        process::exit(1);
    }

    let file_path = &args[1];
    let source = match fs::read_to_string(file_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file {}: {}", file_path, e);
            process::exit(1);
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
    let out = std::path::Path::new(file_path).with_extension("flnc");
    let bytes = fleen_compiler::codegen::to_bytes(&module);
    if let Err(e) = fs::write(&out, &bytes) {
        eprintln!("Error writing {}: {}", out.display(), e);
        process::exit(1);
    }
    println!("\nWrote {} ({} bytes)", out.display(), bytes.len());
}
