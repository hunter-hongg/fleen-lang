//! Integration test: full pipeline from source to bytecode.

use fleen_compiler::codegen::codegen;
use fleen_compiler::lexer::tokenize;
use fleen_compiler::lower::lower;
use fleen_compiler::parser::parse;
use fleen_compiler::resolver::resolve;
use fleen_compiler::typeck::typeck;
use std::path::Path;

#[test]
fn hello_fln_compiles_to_bytecode() {
    let path = Path::new("../examples/hello.fln");
    let src = std::fs::read_to_string(path).expect("read example");
    let tokens = tokenize(&src).expect("lex");
    let ast = parse(tokens).expect("parse");
    let hir = resolve(ast).expect("resolve");
    let typed = typeck(hir).expect("typeck");
    let mir = lower(typed).expect("lower");
    let module = codegen(mir).expect("codegen");

    // Entry must be main (no globals in this example).
    let entry_fn = &module.functions[module.entry.0 as usize];
    let entry_name = &module.constants[entry_fn.name.0 as usize];
    match entry_name {
        fleen_compiler::codegen::Const::Str(s) => assert_eq!(&**s, "main"),
        other => panic!("unexpected entry name {other:?}"),
    }
    assert_eq!(module.version, 1);
    assert!(!module.functions.is_empty());
    // Every function body must end with Return.
    for f in &module.functions {
        assert_eq!(
            *f.code.last().expect("non-empty code"),
            fleen_compiler::codegen::Opcode::Return as u8,
            "function body must end with Return"
        );
    }
}
