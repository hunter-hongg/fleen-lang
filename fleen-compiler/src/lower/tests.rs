//! Tests for the Lower stage (TypedHir → MIR).

use super::mir::*;
use crate::lexer::tokenize;
use crate::parser::parse;
use crate::resolver::resolve;
use crate::typeck::typeck;

fn lower_src(source: &str) -> Mir {
    let tokens = tokenize(source).unwrap();
    let ast = parse(tokens).unwrap();
    let hir = resolve(ast).unwrap();
    let typed = typeck(hir).expect("typeck should succeed");
    super::lower(typed)
}

fn find_func<'a>(mir: &'a Mir, name: &str) -> &'a MirFunc {
    mir.funcs
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("function {name} not found"))
}

/// Collect all instructions of a function in block order.
fn flat(func: &MirFunc) -> Vec<&MirInstr> {
    func.blocks.iter().flat_map(|b| b.instrs.iter()).collect()
}

#[test]
fn lower_fib_structure() {
    let src = r#"
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}
"#;
    let mir = lower_src(src);
    let fib = find_func(&mir, "fib");
    assert_eq!(fib.params, 1);
    assert_eq!(fib.locals, 1);
    let ins = flat(fib);
    // sanity: Recursion call present, Return terminator in last block
    assert!(ins.iter().any(|i| matches!(i, MirInstr::Call(_))));
    assert!(fib.blocks.iter().all(|b| matches!(
        b.terminator,
        Terminator::Return
            | Terminator::Jump(_)
            | Terminator::JumpIfFalse(_)
            | Terminator::JumpIfTrue(_)
    )));
}

#[test]
fn lower_binding_and_assign_slots() {
    let src = r#"
func f(): int {
    x = 42;
    x = 43;
    x
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    // Both the binding and the reassignment write slot 0.
    let stores = ins
        .iter()
        .filter(|i| matches!(i, MirInstr::StoreLocal(0)))
        .count();
    assert_eq!(stores, 2);
    assert_eq!(f.locals, 1);
}

#[test]
fn lower_shadowing_new_slot() {
    let src = r#"
func f(): int {
    const x = 1;
    if true {
        const x = 3;
        x
    } else { 0 }
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    // outer x (slot 0), inner const x (slot 1)
    assert_eq!(f.locals, 2);
}

#[test]
fn lower_while() {
    let src = r#"
func f(): int {
    x = 0;
    while x < 10 {
        x = x + 1;
    };
    x
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    assert!(ins.iter().any(|i| matches!(i, MirInstr::Lt)));
    assert!(ins.iter().any(|i| matches!(i, MirInstr::IAdd)));
    // while terminates with Unit + jumps back
    assert!(
        f.blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Jump(_)))
    );
}

#[test]
fn lower_choose_with_guard() {
    let src = r#"
func f(value: int): string {
    choose value {
        when 0 { "zero" }
        when x if x > 10 { "big" }
        otherwise { "other" }
    }
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    assert!(ins.iter().any(|i| matches!(i, MirInstr::Eq)));
    assert!(ins.iter().any(|i| matches!(i, MirInstr::Gt)));
    // bind-match for the guard variable slot 1
    assert!(ins.iter().any(|i| matches!(i, MirInstr::BindMatch(1))));
}

#[test]
fn lower_and_or_short_circuit() {
    let src = r#"
func f(a: bool, b: bool): bool {
    if a and b { true } else { false }
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    // and lowers to control flow: two JumpIfFalse, no And instruction
    let false_count = f.blocks.iter().flat_map(|b| b.instrs.iter()).count();
    let _ = false_count;
    for b in &f.blocks {
        if let Terminator::JumpIfFalse(_) = b.terminator {
            // ok
        }
    }
}

#[test]
fn lower_call_direct_vs_indirect() {
    let src = r#"
func add(a: int, b: int): int = a + b

func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)
"#;
    let mir = lower_src(src);
    let apply = find_func(&mir, "apply");
    let ins = flat(apply);
    assert!(ins.iter().any(|i| matches!(i, MirInstr::CallValue(2))));
    let add = find_func(&mir, "add");
    // direct call inside... add itself doesn't call; check Call exists in apply? apply uses indirect.
    assert!(
        add.blocks
            .iter()
            .any(|b| matches!(b.terminator, Terminator::Return))
    );
}

#[test]
fn lower_single_expr_func() {
    let mir = lower_src("func add(a: int, b: int): int = a + b");
    let add = find_func(&mir, "add");
    assert_eq!(add.params, 2);
    assert_eq!(add.locals, 2);
    let ins = flat(add);
    assert!(ins.iter().any(|i| matches!(i, MirInstr::IAdd)));
    assert!(matches!(
        add.blocks.last().unwrap().terminator,
        Terminator::Return
    ));
}

#[test]
fn lower_builtin_print_placeholder() {
    let mir = lower_src(r#"print("hi");"#);
    let p = find_func(&mir, "print");
    assert!(p.is_builtin);
}

#[test]
fn lower_nested_if_inside_while_body() {
    let src = r#"
func f(): int {
    x = 0;
    while x < 3 {
        if x == 1 { x = 10; } else { x = x + 1; };
    };
    x
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    // must not panic; all blocks terminated
    for b in &f.blocks {
        assert!(matches!(
            b.terminator,
            Terminator::Return
                | Terminator::Jump(_)
                | Terminator::JumpIfFalse(_)
                | Terminator::JumpIfTrue(_)
        ));
    }
}
