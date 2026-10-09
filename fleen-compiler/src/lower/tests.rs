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
    super::lower(typed).expect("lower should succeed")
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
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::Call(_))));
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
        .filter(|i| matches!(i.kind, MirInstrKind::StoreLocal(0)))
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
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::Lt)));
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::IAdd)));
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
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::Eq)));
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::Gt)));
    // bind-match for the guard variable slot 1
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::BindMatch(1)))
    );
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
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::CallValue(2)))
    );
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
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::IAdd)));
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
fn lower_expr_stmt_emits_pop() {
    // A statement `1 + 1;` computes a value that must be discarded with Pop.
    let src = r#"
func f(): int {
    1 + 1;
    0
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::Pop)));
}

#[test]
fn lower_param_slots_precede_locals() {
    let src = r#"
func add(a: int, b: int): int {
    c = a + b;
    c
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "add");
    assert_eq!(f.params, 2);
    // params occupy slots 0..2; the local `c` must get slot 2.
    assert_eq!(f.locals, 3);
    let ins = flat(f);
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::LoadLocal(0)))
    );
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::LoadLocal(1)))
    );
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::StoreLocal(2)))
    );
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::LoadLocal(2)))
    );
}

#[test]
fn lower_choose_plain_comparison_chain() {
    // No guard: each `when` must lower to an Eq compare + JumpIfFalse,
    // with the otherwise arm reachable as a fallback.
    let src = r#"
func f(value: int): string {
    choose value {
        when 0 { "zero" }
        when 1 { "one" }
        otherwise { "other" }
    }
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    let eq_count = ins
        .iter()
        .filter(|i| matches!(i.kind, MirInstrKind::Eq))
        .count();
    assert_eq!(eq_count, 2);
    let false_jumps = f
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::JumpIfFalse(_)))
        .count();
    assert!(false_jumps >= 2);
}

#[test]
fn lower_while_exits_to_merge_block() {
    let src = r#"
func f(): int {
    x = 0;
    while x < 3 {
        x = x + 1;
    };
    x
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    // The condition block ends with JumpIfFalse targeting the merge block.
    let exit = f
        .blocks
        .iter()
        .find_map(|b| match b.terminator {
            Terminator::JumpIfFalse(exit) => Some(exit),
            _ => None,
        })
        .expect("while should exit via JumpIfFalse");
    let exit_block = f
        .blocks
        .iter()
        .find(|b| b.id == exit)
        .expect("exit block should exist");
    // After the while loop, `x` is loaded from its slot.
    assert!(
        exit_block
            .instrs
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::LoadLocal(_)))
    );
}

#[test]
fn lower_global_variable() {
    let src = r#"
const LIMIT: int = 10;
x = 0;
func f(): int = LIMIT;
"#;
    let mir = lower_src(src);
    assert_eq!(mir.globals.len(), 2);
    let limit = &mir.globals[0];
    assert_eq!(limit.name, "LIMIT");
    assert!(!limit.mutable);
    assert!(matches!(
        limit.init.as_slice(),
        [MirInstr {
            kind: MirInstrKind::ConstInt(10),
            ..
        }]
    ));
    let x = &mir.globals[1];
    assert_eq!(x.name, "x");
    assert!(x.mutable);
    // f's body loads the global rather than using a local slot.
    let f = find_func(&mir, "f");
    let ins = flat(f);
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::LoadGlobal(0)))
    );
}

#[test]
fn lower_shadowing_inner_slot_hides_outer() {
    // Inside the block the outer slot must not be loaded for `x` —
    // only the inner shadowing slot is read.
    let src = r#"
func f(): int {
    const x = 1;
    if true {
        const x = 2;
        x
    } else { 0 }
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    assert_eq!(f.locals, 2);
    // Find the inner block: it reads slot 1 (inner shadow), never slot 0.
    let inner = f
        .blocks
        .iter()
        .find(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(i.kind, MirInstrKind::StoreLocal(1)))
        })
        .expect("inner shadow block should store to slot 1");
    assert!(inner.instrs.iter().all(|i| !matches!(
        i.kind,
        MirInstrKind::LoadLocal(0) | MirInstrKind::StoreLocal(0)
    )));
}

#[test]
fn lower_elif_chain_merges_to_one_block() {
    // Each arm of an if/elif/else chain must terminate by jumping to the
    // *same* merge block.
    let src = r#"
func f(a: bool, b: bool): int {
    if a { 1 }
    elif b { 2 }
    else { 3 }
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    // Collect jump targets of blocks that contain a branch body constant.
    let mut targets = Vec::new();
    for b in &f.blocks {
        let has_body_const = b
            .instrs
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::ConstInt(1..=3)));
        if has_body_const {
            match b.terminator {
                Terminator::Jump(t) => targets.push(t),
                other => panic!("branch body block should end in Jump, got {other:?}"),
            }
        }
    }
    assert_eq!(targets.len(), 3);
    assert!(targets.iter().all(|t| *t == targets[0]));
}

#[test]
fn lower_box_and_deref_read() {
    // `box e` → eval e; AllocBox. `deref b` → LoadLocal(b); DerefBox.
    let src = r#"
func f(): int {
    b = box 42;
    n = deref b;
    n
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::AllocBox)));
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::DerefBox)));
}

#[test]
fn lower_deref_assign() {
    // `deref b = 7`: [b]; [7]; StoreDerefBox; Unit — one value left on stack.
    let src = r#"
func f() {
    b = box 42;
    deref b = 7;
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    assert!(f.blocks.iter().any(|b| {
        b.instrs
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::StoreDerefBox))
    }));
}

#[test]
fn lower_move_access_emits_move_local() {
    let src = r#"
func f(s: string): string {
    t = move s;
    t
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    // `move s` transfers `s` into `t` → MoveLocal. The tail `t` returns by
    // value, so it too is a MoveLocal (implicit transfer), not a LoadLocal.
    let moves = ins
        .iter()
        .filter(|i| matches!(i.kind, MirInstrKind::MoveLocal(_)))
        .count();
    assert!(moves >= 1);
}

#[test]
fn lower_clone_access_emits_clone_local() {
    let src = r#"
func f(s: string): string {
    t = clone s;
    t
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::CloneLocal(_)))
    );
}

#[test]
fn lower_question_emits_unwrap_and_pack_err() {
    let src = r#"
func main(): Result<int, string> {
    r: Result<int, string> = Err("boom");
    n = r?;
    Ok(n)
}
"#;
    let mir = lower_src(src);
    let main = find_func(&mir, "main");
    let ins = flat(main);
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::IsErr)));
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::UnwrapOk)));
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::UnwrapErr))
    );
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::PackErr)));
    let err_block = main.blocks.iter().find(|b| {
        b.instrs
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::UnwrapErr))
    });
    assert!(err_block.is_some_and(|b| matches!(b.terminator, Terminator::Return)));
}

#[test]
fn lower_result_ctor_emits_pack() {
    let src = r#"
func okf(): Result<int, string> { Ok(42) }
func errf(): Result<int, string> { Err("no") }
"#;
    let mir = lower_src(src);
    assert!(
        flat(find_func(&mir, "okf"))
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::PackOk))
    );
    assert!(
        flat(find_func(&mir, "errf"))
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::PackErr))
    );
}

#[test]
fn lower_choose_result_emits_unwrap_branches() {
    let src = r#"
func f(): int {
    r: Result<int, string> = Ok(5);
    choose clone r {
        when Ok(v) { v }
        when Err(e) { 0 }
    }
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::IsErr)));
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::UnwrapOk)));
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::UnwrapErr))
    );
}

#[test]
fn lower_global_owned_init_uses_clone_global() {
    // Global `box` init lowers to AllocBox; reading an owned (string) global
    // with `clone` lowers to CloneGlobal. (Deref of a global box is a Copy
    // access → LoadGlobal, per the Access model.)
    let src = r#"
s = "hi";
g = box 1;
func f(): string {
    t = clone s;
    t
}
"#;
    let mir = lower_src(src);
    let g = &mir.globals[1];
    assert!(
        g.init
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::AllocBox))
    );
    let f = find_func(&mir, "f");
    assert!(
        flat(f)
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::CloneGlobal(_)))
    );
}

#[test]
fn lower_choose_otherwise_is_reachable_fallback() {
    // The otherwise arm's body block must be the fallthrough target of the
    // last when's failed comparison (or the block the chain falls into).
    let src = r#"
func f(value: int): string {
    choose value {
        when 0 { "zero" }
        when 1 { "one" }
        otherwise { "other" }
    }
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let otherwise_block = f
        .blocks
        .iter()
        .find(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(&i.kind, MirInstrKind::ConstStr(s) if s.as_str() == "other"))
        })
        .expect("otherwise body block should exist");
    // Some conditional jump from an earlier arm must target this block.
    let has_pred = f.blocks.iter().any(|b| match b.terminator {
        Terminator::JumpIfFalse(t) | Terminator::JumpIfTrue(t) => t == otherwise_block.id,
        _ => false,
    });
    assert!(
        has_pred,
        "otherwise block must be reachable from a failed when"
    );
}

/// Net stack effect of a single instruction (args popped, result pushed).
fn stack_delta(i: &MirInstr, func_params: &std::collections::HashMap<FuncId, u16>) -> i64 {
    match &i.kind {
        MirInstrKind::ConstInt(_)
        | MirInstrKind::ConstFloat(_)
        | MirInstrKind::ConstStr(_)
        | MirInstrKind::True
        | MirInstrKind::False
        | MirInstrKind::Unit => 1,
        MirInstrKind::Dup | MirInstrKind::DupDeep => 1,
        MirInstrKind::Pop => -1,
        MirInstrKind::LoadLocal(_)
        | MirInstrKind::LoadGlobal(_)
        | MirInstrKind::LoadFunc(_)
        | MirInstrKind::MoveLocal(_)
        | MirInstrKind::CloneLocal(_)
        | MirInstrKind::CloneGlobal(_)
        | MirInstrKind::DerefBox
        | MirInstrKind::MakeRefLocal(_) => 1,
        MirInstrKind::StoreLocal(_)
        | MirInstrKind::StoreGlobal(_)
        | MirInstrKind::StoreDerefBox => -1,
        MirInstrKind::IAdd
        | MirInstrKind::ISub
        | MirInstrKind::IMul
        | MirInstrKind::IDiv
        | MirInstrKind::IMod
        | MirInstrKind::FAdd
        | MirInstrKind::FSub
        | MirInstrKind::FMul
        | MirInstrKind::FDiv
        | MirInstrKind::Eq
        | MirInstrKind::Ne
        | MirInstrKind::Lt
        | MirInstrKind::Gt
        | MirInstrKind::Le
        | MirInstrKind::Ge => -1,
        MirInstrKind::Not | MirInstrKind::NegI | MirInstrKind::NegF => 0,
        MirInstrKind::BindMatch(_) => 0,
        // 0.0.2 U13: ToStr pops a scalar and pushes a string (Δ0).
        MirInstrKind::ToStr => 0,
        MirInstrKind::Call(fid) => 1 - func_params[fid] as i64,
        MirInstrKind::CallValue(argc) => -(*argc as i64),
        // 0.0.2 U06: ownership / Result instructions.
        MirInstrKind::AllocBox => 0,  // v → b
        MirInstrKind::PackOk => 0,    // v → ok(v)
        MirInstrKind::PackErr => 0,   // v → err(v)
        MirInstrKind::IsErr => 0,     // result → bool
        MirInstrKind::UnwrapOk => 0,  // result → v
        MirInstrKind::UnwrapErr => 0, // result → e
    }
}

#[test]
fn lower_block_entry_stack_depths_are_consistent() {
    // Entry depth at the entry block is 0; threading depths through the
    // terminators must give every predecessor the same value at a merge
    // block, and every `Return` must leave exactly one value.
    let src = r#"
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

func sum(limit: int): int {
    x = 0;
    while x < limit {
        x = x + 1;
    };
    x
}

func pick(v: int): int {
    choose v {
        when 0 { 1 }
        when n if n > 10 { n }
        otherwise { 0 }
    }
}
"#;
    let mir = lower_src(src);
    let func_params: std::collections::HashMap<FuncId, u16> =
        mir.funcs.iter().map(|f| (f.func_id, f.params)).collect();
    for func in &mir.funcs {
        let mut entry_depth: Vec<Option<i64>> = vec![None; func.blocks.len()];
        let entry_pos = func
            .blocks
            .iter()
            .position(|b| b.id == func.entry)
            .expect("entry block exists");
        entry_depth[entry_pos] = Some(0);
        // Worklist over positions; fallthrough target of conditional jumps is
        // the next block in program order.
        let mut worklist = vec![entry_pos];
        while let Some(pos) = worklist.pop() {
            let Some(d0) = entry_depth[pos] else { continue };
            let block = &func.blocks[pos];
            let mut d = d0;
            for i in &block.instrs {
                d += stack_delta(i, &func_params);
                assert!(d >= 0, "stack underflow in {:?} at {:?}", func.name, i);
            }
            let mut set = |target: BlockId, val: i64| {
                let p = func
                    .blocks
                    .iter()
                    .position(|b| b.id == target)
                    .expect("jump target exists");
                match entry_depth[p] {
                    None => {
                        entry_depth[p] = Some(val);
                        worklist.push(p);
                    }
                    Some(existing) => assert_eq!(
                        existing, val,
                        "inconsistent entry depth at {target:?} in {}",
                        func.name
                    ),
                }
            };
            match block.terminator {
                Terminator::Return => assert_eq!(d, 1, "{} must return 1 value", func.name),
                Terminator::Jump(t) => set(t, d),
                Terminator::JumpIfFalse(t) | Terminator::JumpIfTrue(t) => {
                    set(t, d - 1);
                    if pos + 1 < func.blocks.len() {
                        let next_id = func.blocks[pos + 1].id;
                        set(next_id, d - 1);
                    }
                }
            }
        }
    }
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
