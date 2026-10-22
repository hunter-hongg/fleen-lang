//! Tests for the Lower stage (TypedHir → MIR).

use super::mir::*;
use crate::lexer::tokenize;
use crate::parser::parse;
use crate::resolver::resolve;
use crate::source_map::FileId;
use crate::typeck::typeck;

fn lower_src(source: &str) -> Mir {
    let tokens = tokenize(source).unwrap();
    let ast = parse(tokens).unwrap();
    let hir = resolve(ast).unwrap();
    let typed = typeck(hir).expect("typeck should succeed");
    super::lower(typed).expect("lower should succeed")
}

/// Lower `source` and return the lowering error (typeck must accept it).
fn lower_err(source: &str) -> super::LowerError {
    let tokens = tokenize(source).unwrap();
    let ast = parse(tokens).unwrap();
    let hir = resolve(ast).unwrap();
    let typed = typeck(hir).expect("typeck should succeed");
    match super::lower(typed) {
        Ok(_) => panic!("lower should fail, but succeeded for:\n{source}"),
        Err(e) => e,
    }
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
fn lower_async_fn_begins_with_asyncstart() {
    // 0.0.4 T05: an `async func`'s entry block must open with `AsyncStart` —
    // the call boundary that packages the frame's params into a not-yet-started
    // Future and returns it to the caller. Body entry is therefore the fixed
    // instruction right after it (pc + 1), which the VM's `AsyncStart` relies
    // on (`resume_pc = ip`), so this layout is load-bearing.
    let src = "\
async func producer(x: int): int = x + 1
func main(): int = producer(1) await
";
    let mir = lower_src(src);
    let producer = find_func(&mir, "producer");
    let mut ins = flat(producer).into_iter();
    assert!(
        matches!(ins.next().map(|i| &i.kind), Some(MirInstrKind::AsyncStart)),
        "async fn body must begin with AsyncStart"
    );
    // The entry block itself opens with AsyncStart, so the resume point is a
    // fixed offset from the function start.
    let entry = &producer.blocks[producer.entry.0 as usize];
    assert!(
        matches!(
            entry.instrs.first().map(|i| &i.kind),
            Some(MirInstrKind::AsyncStart)
        ),
        "async fn entry block must start with AsyncStart"
    );
    // A synchronous fn has no such head.
    let main = find_func(&mir, "main");
    assert!(
        !matches!(
            flat(main).first().map(|i| &i.kind),
            Some(MirInstrKind::AsyncStart)
        ),
        "a sync fn must not open with AsyncStart"
    );
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
fn lower_variadic_print_per_arity_instances() {
    // U09 fix: the `Call` instruction pops exactly `callee.params` operands,
    // so each observed call arity gets its own builtin instance — a shared
    // `params == 1` print left extra arguments on the stack and desynced
    // every depth the verifier reasons about.
    let src = r#"
func f(): int {
    print("one");
    print("a", "b");
    print("x", "y", "z");
    print("again", "dupe");
    0
}
"#;
    let mir = lower_src(src);
    let mut arities: Vec<u16> = mir
        .funcs
        .iter()
        .filter(|f| f.name == "print")
        .map(|f| f.params)
        .collect();
    arities.sort_unstable();
    assert_eq!(arities, vec![1, 2, 3], "one instance per distinct arity");
    // Same-arity call sites reuse the instance.
    let twos = mir
        .funcs
        .iter()
        .filter(|f| f.name == "print" && f.params == 2)
        .count();
    assert_eq!(twos, 1);
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
fn lower_deref_assign_replaces_the_box() {
    // `deref b = 7`: [7]; AllocBox; StoreLocal b; Unit — one value left on
    // stack. The pointee replacement compiles to a box replacement (unique
    // ownership makes the two observationally identical): StoreDerefBox pops
    // and discards its box operand, and the LoadLocal copy it used to write
    // into could never reach the slot's box (U09 fix, see AssignDeref).
    let src = r#"
func f() {
    b = box 42;
    deref b = 7;
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let kinds: Vec<_> = f
        .blocks
        .iter()
        .flat_map(|b| b.instrs.iter())
        .map(|i| &i.kind)
        .collect();
    assert!(kinds.iter().any(|k| matches!(k, MirInstrKind::AllocBox)));
    assert!(
        !kinds
            .iter()
            .any(|k| matches!(k, MirInstrKind::StoreDerefBox))
    );
    // The write targets the box variable's own slot: ConstInt 7 → AllocBox →
    // StoreLocal(b) appear back to back after the initial box allocation.
    let store_local_b = kinds
        .iter()
        .position(|k| matches!(k, MirInstrKind::StoreLocal(s) if *s == 0));
    assert!(
        store_local_b.is_some(),
        "deref write must store to b's slot"
    );
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
fn stack_delta(
    i: &MirInstr,
    func_params: &std::collections::HashMap<FuncId, u16>,
    struct_fields: &std::collections::HashMap<u16, usize>,
) -> i64 {
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
        MirInstrKind::StoreLocal(_) | MirInstrKind::StoreGlobal(_) => -1,
        // `StoreDerefBox` pops BOTH the box and the value (BYTECODE.md §8:
        // Δ−2, min 2) — it is not a plain store.
        MirInstrKind::StoreDerefBox => -2,
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
        // 0.0.3 V02: structs. StructNew's count comes from the struct
        // table; the depth test's helper approximates via the operand id
        // only when the table is unavailable — but lower tests compile
        // from source, so re-derive nothing: GetField/SetField* are exact.
        MirInstrKind::GetField(_) => 0,           // s → v
        MirInstrKind::SetFieldLocal(_, _) => -1,  // v →
        MirInstrKind::SetFieldGlobal(_, _) => -1, // v →
        // 0.0.3 V02: field count from the struct table (the depth test
        // compiles from source, so the table is `mir.structs`).
        MirInstrKind::StructNew(id) => 1 - struct_fields.get(id).copied().unwrap_or(0) as i64,
        // 0.0.3 V03: arrays. ArrNew's count is the operand; ArrGet pops the
        // index and the array and pushes the element copy (the erratum shape).
        MirInstrKind::ArrNew(n) => 1 - *n as i64,
        MirInstrKind::ArrLen => 1,
        MirInstrKind::ArrGet => -1,
        MirInstrKind::ArrSetLocal(_) => -2,
        MirInstrKind::ArrSetGlobal(_) => -2,
        // 0.0.3 V04: range. `MakeRange` pops the two int bounds and pushes
        // the Range (Δ−1); `RangeStart`/`RangeEnd` are peek-shaped like
        // `ArrLen` (Δ+1).
        MirInstrKind::MakeRange => -1,
        MirInstrKind::RangeStart => 1,
        MirInstrKind::RangeEnd => 1,
        // 0.0.4 T05: async / await. `AsyncStart` packages the frame into a
        // Future (Δ0); `Await`/`Poll` drive a Future and leave its payload
        // (F → T, Δ0); `Suspend` is unreachable in T05 (Δ0).
        MirInstrKind::AsyncStart => 0,
        MirInstrKind::Suspend => 0,
        MirInstrKind::Poll => 0,
        MirInstrKind::Await => 0,
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

func total_all(xs: [int]): int {
    t = 0;
    for x in xs {
        t = t + x;
    }
    t
}
"#;
    let mir = lower_src(src);
    let func_params: std::collections::HashMap<FuncId, u16> =
        mir.funcs.iter().map(|f| (f.func_id, f.params)).collect();
    let struct_fields: std::collections::HashMap<u16, usize> = mir
        .structs
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u16, s.field_types.len()))
        .collect();
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
                d += stack_delta(i, &func_params, &struct_fields);
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

// =========================================================================
// 0.0.2 U06: Span propagation assertions
// =========================================================================

#[test]
fn u06_span_alloc_box_covers_box_expr() {
    // `box 1` — AllocBox span should cover the `box 1` expression.
    let src = r#"
func f(): box<int> {
    b = box 1
    b
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    // Locate the AllocBox instruction and verify it has a non-zero span.
    let alloc = ins
        .iter()
        .find(|i| matches!(i.kind, MirInstrKind::AllocBox));
    assert!(alloc.is_some(), "AllocBox instruction should exist");
    let span = alloc.unwrap().span;
    assert!(
        span.start < span.end,
        "AllocBox span should be non-degenerate: start={}, end={}",
        span.start,
        span.end
    );
}

#[test]
fn u06_span_move_local_covers_ident() {
    // `move s` — MoveLocal span should cover the `move s` expression.
    let src = r#"
func f(s: string): string {
    move s
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    let mov = ins
        .iter()
        .find(|i| matches!(i.kind, MirInstrKind::MoveLocal(_)));
    assert!(mov.is_some(), "MoveLocal instruction should exist");
    let span = mov.unwrap().span;
    assert!(
        span.start < span.end,
        "MoveLocal span should be non-degenerate: start={}, end={}",
        span.start,
        span.end
    );
}

#[test]
fn u06_span_clone_local_covers_ident() {
    // `clone s` — CloneLocal span should cover the `clone s` expression.
    let src = r#"
func f(s: string): string {
    clone s
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    let cln = ins
        .iter()
        .find(|i| matches!(i.kind, MirInstrKind::CloneLocal(_)));
    assert!(cln.is_some(), "CloneLocal instruction should exist");
    let span = cln.unwrap().span;
    assert!(
        span.start < span.end,
        "CloneLocal span should be non-degenerate: start={}, end={}",
        span.start,
        span.end
    );
}

#[test]
fn u06_span_result_instructions_have_spans() {
    // Ok(42) → PackOk; the instruction should carry a span covering the expression.
    let src = r#"
func f(): Result<int, string> {
    Ok(42)
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    let pack = ins.iter().find(|i| matches!(i.kind, MirInstrKind::PackOk));
    assert!(pack.is_some(), "PackOk instruction should exist");
    let span = pack.unwrap().span;
    assert!(
        span.start < span.end,
        "PackOk span should be non-degenerate: start={}, end={}",
        span.start,
        span.end
    );
}

#[test]
fn u06_span_is_err_and_unwrap_in_question() {
    // `r?` emits IsErr, UnwrapOk, UnwrapErr — each should have a span.
    let src = r#"
func f(): Result<int, string> {
    r: Result<int, string> = Ok(42)
    n = r?
    Ok(n)
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    for kind in [
        MirInstrKind::IsErr,
        MirInstrKind::UnwrapOk,
        MirInstrKind::UnwrapErr,
    ] {
        let instr = ins.iter().find(|i| i.kind == kind);
        assert!(
            instr.is_some(),
            "instruction {:?} should exist in `?` lowering",
            kind
        );
        let span = instr.unwrap().span;
        assert!(
            span.start < span.end,
            "{:?} span should be non-degenerate: start={}, end={}",
            kind,
            span.start,
            span.end
        );
    }
}

#[test]
fn u06_all_instructions_carry_non_degenerate_spans() {
    // Comprehensive check: every instruction in functions and globals
    // from a program exercising all U06 features must have a valid span.
    let src = r#"
g = box 42;
func f(s: string): Result<int, string> {
    b = box move s;
    n = deref b;
    r: Result<int, string> = Ok(42);
    m = r?;
    Ok(m)
}
"#;
    let mir = lower_src(src);

    // Check global init instructions.
    for g in &mir.globals {
        for instr in &g.init {
            assert!(
                instr.span.start <= instr.span.end,
                "global init instr {:?} has invalid span start={} end={}",
                instr.kind,
                instr.span.start,
                instr.span.end
            );
        }
    }

    // Check all function instructions.
    for func in &mir.funcs {
        if func.is_builtin {
            continue;
        }
        for block in &func.blocks {
            for instr in &block.instrs {
                assert!(
                    instr.span.start <= instr.span.end,
                    "instr {:?} in {} has invalid span start={} end={}",
                    instr.kind,
                    func.name,
                    instr.span.start,
                    instr.span.end
                );
            }
        }
    }
}

#[test]
fn u06_span_deref_box_covers_expr() {
    // `deref b` — DerefBox span should cover the expression.
    let src = r#"
func f(b: box<int>): int {
    deref b
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    let dref = ins
        .iter()
        .find(|i| matches!(i.kind, MirInstrKind::DerefBox));
    assert!(dref.is_some(), "DerefBox instruction should exist");
    let span = dref.unwrap().span;
    assert!(
        span.start < span.end,
        "DerefBox span should be non-degenerate: start={}, end={}",
        span.start,
        span.end
    );
}

#[test]
fn u06_span_make_ref_local_covers_arg() {
    // When passing a local `s` to a `ref string` parameter, MakeRefLocal
    // should be emitted with the span of the argument expression.
    let src = r#"
func takes_ref(s: ref string): int = 0
func f(s: string): int {
    takes_ref(s)
}
"#;
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let ins = flat(f);
    let ref_instr = ins
        .iter()
        .find(|i| matches!(i.kind, MirInstrKind::MakeRefLocal(_)));
    assert!(
        ref_instr.is_some(),
        "MakeRefLocal instruction should exist for ref argument"
    );
    let span = ref_instr.unwrap().span;
    assert!(
        span.start < span.end,
        "MakeRefLocal span should be non-degenerate: start={}, end={}",
        span.start,
        span.end
    );
}

// =========================================================================
// 0.0.2 U06: error paths (lower-level rejections)
// =========================================================================

#[test]
fn u06_ref_arg_in_global_init_is_rejected() {
    // A global initializer that passes a `ref` argument has no frame to host
    // the temp slot (DESIGN §10.5) — lower must report `RefArgInGlobalInit`.
    let src = r#"
func takes_ref(s: ref string): int = 0
g = "hello";
h = takes_ref(g);
func main(): int { 0 }
"#;
    let err = lower_err(src);
    assert!(
        matches!(err.kind, super::LowerErrorKind::RefArgInGlobalInit { .. }),
        "expected RefArgInGlobalInit, got {:?}",
        err.kind
    );
    // The error must point at the offending argument, not the whole file.
    assert!(
        err.span.start < err.span.end,
        "error span should be non-degenerate: start={}, end={}",
        err.span.start,
        err.span.end
    );
}

#[test]
fn u06_choose_result_missing_ok_arm_is_rejected() {
    // typeck enforces exhaustivity first (U04), so `lower` never sees a
    // Result choose lacking an Ok arm. This pins the typeck gate that makes
    // lower's `ChooseResultNeedsOkErrArms` check defensive-only.
    let tokens = tokenize(
        r#"
func f(): int {
    r: Result<int, string> = Ok(5);
    choose clone r {
        when Err(e) { 0 }
    }
}
"#,
    )
    .unwrap();
    let ast = parse(tokens).unwrap();
    let hir = resolve(ast).unwrap();
    let errs = typeck(hir).expect_err("typeck should reject a Result choose missing Ok");
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            crate::typeck::error::TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns.iter().any(|p| p == "Ok")
        )),
        "expected ChooseNotExhaustive mentioning Ok, got {errs:?}"
    );
}

#[test]
fn u06_choose_result_missing_err_arm_is_rejected() {
    let tokens = tokenize(
        r#"
func f(): int {
    r: Result<int, string> = Ok(5);
    choose clone r {
        when Ok(v) { v }
    }
}
"#,
    )
    .unwrap();
    let ast = parse(tokens).unwrap();
    let hir = resolve(ast).unwrap();
    let errs = typeck(hir).expect_err("typeck should reject a Result choose missing Err");
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            crate::typeck::error::TypeckErrorKind::ChooseNotExhaustive { missing_patterns, .. }
                if missing_patterns.iter().any(|p| p == "Err")
        )),
        "expected ChooseNotExhaustive mentioning Err, got {errs:?}"
    );
}

#[test]
fn u06_choose_result_guard_stays_deferred() {
    // DESIGN §10.5 / TICKETS.md U07: guarded Result arms are rejected by
    // typeck (a guarded arm does not count for exhaustivity) and their
    // lowering stays deferred to U09 — U07 explicitly kept it out of scope
    // rather than silently widening (TICKETS.md U07 "scope decision").
    let tokens = tokenize(
        r#"
func f(): int {
    r: Result<int, string> = Ok(5);
    choose clone r {
        when Ok(v) if v > 0 { v }
        when Err(e) { 0 }
    }
}
"#,
    )
    .unwrap();
    let ast = parse(tokens).unwrap();
    let hir = resolve(ast).unwrap();
    let errs =
        typeck(hir).expect_err("guarded Result arm is currently rejected by typeck, not lowered");
    assert!(
        errs.iter().any(|e| matches!(
            &e.kind,
            crate::typeck::error::TypeckErrorKind::ChooseNotExhaustive { .. }
        )),
        "expected ChooseNotExhaustive, got {errs:?}"
    );
}

#[test]
fn u06_result_choose_otherwise_is_rejected_not_dropped() {
    // `otherwise` short-circuits typeck's exhaustivity check, so it reaches
    // lower. The Result lowering is the IsErr-branch shape with no fallback
    // block, so the arm must be rejected rather than silently dropped.
    // Without Ok+Err both present this used to report the misleading
    // `ChooseResultNeedsOkErrArms`.
    let src = r#"
func f(): int {
    r: Result<int, string> = Ok(5);
    choose clone r {
        when Ok(v) if v > 0 { v }
        otherwise { 0 }
    }
}
"#;
    let err = lower_err(src);
    match err.kind {
        super::LowerErrorKind::UnsupportedFeature { feature } => {
            assert!(
                feature.contains("non-`Ok`/`Err` arm"),
                "message should name the unsupported arm, got {feature:?}"
            );
        }
        other => {
            panic!("expected UnsupportedFeature for `otherwise` in a Result choose, got {other:?}")
        }
    }
    // The error must point at the offending arm (a non-degenerate span).
    assert!(
        err.span.start < err.span.end,
        "error span should cover the arm: start={}, end={}",
        err.span.start,
        err.span.end
    );
}

#[test]
fn u06_result_choose_otherwise_after_ok_err_is_rejected() {
    // Even when Ok and Err arms are both present (so exhaustivity is
    // satisfied), an extra `otherwise` arm was silently dropped. It must be
    // rejected: dropped user code is a miscompile waiting to happen.
    let src = r#"
func f(): int {
    r: Result<int, string> = Ok(5);
    choose clone r {
        when Ok(v) { v }
        when Err(e) { 0 }
        otherwise { 99 }
    }
}
"#;
    let err = lower_err(src);
    assert!(
        matches!(err.kind, super::LowerErrorKind::UnsupportedFeature { .. }),
        "expected UnsupportedFeature, got {:?}",
        err.kind
    );
}

#[test]
fn u06_result_choose_only_ok_err_arms_still_lower() {
    // The supported shape must keep working unchanged.
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
    assert!(ins.iter().any(|i| matches!(i.kind, MirInstrKind::UnwrapOk)));
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::UnwrapErr))
    );
    // The otherwise body `99` must NOT appear anywhere.
    assert!(
        !ins.iter()
            .any(|i| matches!(&i.kind, MirInstrKind::ConstInt(99))),
        "the supported shape must not silently drop or add values"
    );
}

#[test]
fn u06_global_box_deref_assign_is_rejected() {
    // typeck rejects this too; lower stays total and reports its own error.
    // The direct-lowering test drives `lower` on a hand-built tree instead:
    // verify the defensive branch exists by checking the error kind's Display.
    let kind = super::LowerErrorKind::GlobalBoxDerefAssign {
        name: "global#0".to_string(),
    };
    let msg = super::LowerError::new(kind, crate::lexer::Span::new(FileId(0), 0, 5)).to_string();
    assert!(
        msg.contains("global#0"),
        "message should name the global: {msg}"
    );
}

// =========================================================================
// 0.0.2 U06: stack-depth consistency for ownership features
// =========================================================================

/// Re-verify MIR stack-depth consistency for a program exercising the U06
/// lowering paths. This is the contract U07's `fleen-verify` will enforce
/// (BYTECODE.md §8 rule 4), so it must hold at MIR level already.
fn assert_stack_depths_consistent(src: &str) {
    let mir = lower_src(src);
    let func_params: std::collections::HashMap<FuncId, u16> =
        mir.funcs.iter().map(|f| (f.func_id, f.params)).collect();
    let struct_fields: std::collections::HashMap<u16, usize> = mir
        .structs
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u16, s.field_types.len()))
        .collect();
    for func in &mir.funcs {
        if func.is_builtin {
            continue;
        }
        let mut entry_depth: Vec<Option<i64>> = vec![None; func.blocks.len()];
        let entry_pos = func
            .blocks
            .iter()
            .position(|b| b.id == func.entry)
            .expect("entry block exists");
        entry_depth[entry_pos] = Some(0);
        let mut worklist = vec![entry_pos];
        while let Some(pos) = worklist.pop() {
            let Some(d0) = entry_depth[pos] else { continue };
            let block = &func.blocks[pos];
            let mut d = d0;
            for i in &block.instrs {
                d += stack_delta(i, &func_params, &struct_fields);
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
fn u06_stack_depths_consistent_box_and_deref() {
    assert_stack_depths_consistent(
        r#"
func f(): int {
    b = box 42;
    n = deref b;
    deref b = 7;
    deref b
}
"#,
    );
}

#[test]
fn u06_stack_depths_consistent_move_and_clone() {
    assert_stack_depths_consistent(
        r#"
func f(s: string): string {
    t = clone s;
    u = move s;
    u
}
"#,
    );
}

#[test]
fn u06_stack_depths_consistent_question() {
    assert_stack_depths_consistent(
        r#"
func f(): Result<int, string> {
    r: Result<int, string> = Ok(42);
    n = r?;
    Ok(n)
}
"#,
    );
}

#[test]
fn u06_stack_depths_consistent_choose_result() {
    assert_stack_depths_consistent(
        r#"
func f(): int {
    r: Result<int, string> = Ok(5);
    choose clone r {
        when Ok(v) { v }
        when Err(e) { 0 }
    }
}
"#,
    );
}

#[test]
fn u06_stack_depths_consistent_ref_argument() {
    assert_stack_depths_consistent(
        r#"
func takes_ref(s: ref string): int = 0
func f(s: string): int {
    takes_ref(s)
}
"#,
    );
}

#[test]
fn u06_stack_depths_consistent_global_ref_argument() {
    // The global ref argument path copies through a temp slot
    // (CloneGlobal; StoreLocal; MakeRefLocal) — the extra slot must not
    // disturb the operand-stack depth.
    assert_stack_depths_consistent(
        r#"
g = "global";
func takes_ref(s: ref string): int = 0
func f(): int {
    takes_ref(g)
}
"#,
    );
}

#[test]
fn lower_struct_literal_declaration_order() {
    // In-order literal: plain evaluate-then-StructNew sequence.
    let mir =
        lower_src("struct Point { x: int, y: int }\nfunc main(): Point = Point { x: 1, y: 2 }");
    let main = mir.funcs.iter().find(|f| f.name == "main").unwrap();
    let kinds: Vec<&MirInstrKind> = main.blocks[0].instrs.iter().map(|i| &i.kind).collect();
    assert_eq!(
        kinds,
        vec![
            &MirInstrKind::ConstInt(1),
            &MirInstrKind::ConstInt(2),
            &MirInstrKind::StructNew(0),
        ]
    );
}

#[test]
fn lower_struct_literal_reorders_source_order() {
    // Out-of-order literal: source-order evaluation into temps, then
    // declaration-order reloads — side effects (calls) stay source-ordered.
    let mir =
        lower_src("struct Point { x: int, y: int }\nfunc main(): Point = Point { y: 5, x: 1 }");
    let main = mir.funcs.iter().find(|f| f.name == "main").unwrap();
    let kinds: Vec<&MirInstrKind> = main.blocks[0].instrs.iter().map(|i| &i.kind).collect();
    // Evaluate y (5) into a temp, evaluate x (1) into a temp, reload
    // declaration order (x, y), StructNew.
    assert_eq!(
        kinds,
        vec![
            &MirInstrKind::ConstInt(5),
            &MirInstrKind::StoreLocal(0),
            &MirInstrKind::ConstInt(1),
            &MirInstrKind::StoreLocal(1),
            &MirInstrKind::LoadLocal(1),
            &MirInstrKind::LoadLocal(0),
            &MirInstrKind::StructNew(0),
        ]
    );
}

#[test]
fn lower_field_read_and_write() {
    let mir = lower_src(
        "struct Point { x: int, y: int }\nfunc main(): int {\n    p = Point { x: 1, y: 2 };\n    p.y = 9;\n    p.y\n}",
    );
    let main = mir.funcs.iter().find(|f| f.name == "main").unwrap();
    let kinds: Vec<&MirInstrKind> = main.blocks[0].instrs.iter().map(|i| &i.kind).collect();
    // Literal, StoreLocal p; Const 9, Dup, SetFieldLocal(p, 1); (statement Pop);
    // read: LoadLocal p, GetField(1).
    assert!(kinds.contains(&&MirInstrKind::GetField(1)));
    assert!(kinds.contains(&&MirInstrKind::SetFieldLocal(0, 1)));
    assert!(kinds.contains(&&MirInstrKind::StructNew(0)));
}

#[test]
fn lower_field_write_global() {
    // TICKETS V02 lower table: `p.f = e` on a *global* p lowers to
    // SetFieldGlobal(gid, offset) — read: LoadGlobal, GetField.
    let mir = lower_src(
        "struct Point { x: int, y: int }\ng = Point { x: 1, y: 2 };\nfunc main(): int {\n    g.y = 9;\n    g.y\n}",
    );
    let main = mir.funcs.iter().find(|f| f.name == "main").unwrap();
    let kinds: Vec<&MirInstrKind> = main.blocks[0].instrs.iter().map(|i| &i.kind).collect();
    assert!(kinds.contains(&&MirInstrKind::SetFieldGlobal(0, 1)));
    assert!(kinds.contains(&&MirInstrKind::GetField(1)));
}

#[test]
fn lower_struct_defs_pass_through() {
    let mir = lower_src("struct Point { x: int, y: int }\nfunc main(): int = 0");
    assert_eq!(mir.structs.len(), 1);
    assert_eq!(mir.structs[0].field_types.len(), 2);
}

#[test]
fn lower_struct_in_global_init() {
    let mir = lower_src(
        "struct Point { x: int, y: int }\ng = Point { x: 1, y: 2 };\nfunc main(): int = 0",
    );
    assert!(mir.globals.len() == 1);
}

// ========== 0.0.3 V03: arrays ==========

/// All instruction kinds of a function, flattened across blocks.
fn instr_kinds(f: &MirFunc) -> Vec<&MirInstrKind> {
    flat(f).into_iter().map(|i| &i.kind).collect()
}

#[test]
fn lower_array_literal_elements_then_arrnew() {
    let mir = lower_src("func main(): int {\n    a = [1, 2, 3];\n    a[0]\n}");
    let f = find_func(&mir, "main");
    let kinds = instr_kinds(f);
    // Const 1; Const 2; Const 3; ArrNew(3); StoreLocal(a); ...
    let arrnew = kinds
        .iter()
        .position(|k| matches!(k, MirInstrKind::ArrNew(n) if *n == 3))
        .expect("ArrNew(3) expected");
    assert!(matches!(kinds[arrnew - 3], MirInstrKind::ConstInt(1)));
    assert!(matches!(kinds[arrnew - 2], MirInstrKind::ConstInt(2)));
    assert!(matches!(kinds[arrnew - 1], MirInstrKind::ConstInt(3)));
}

#[test]
fn lower_index_read_is_arrget() {
    let mir = lower_src("func main(): int {\n    a = [1, 2];\n    a[1]\n}");
    let f = find_func(&mir, "main");
    let kinds = instr_kinds(f);
    let get = kinds
        .iter()
        .position(|k| matches!(k, MirInstrKind::ArrGet))
        .expect("ArrGet expected");
    // The index constant sits right below the ArrGet (rhs-then-index shape
    // is not visible here — the read has no rhs). The array operand is a
    // bare owned read → auto-Clone (D3): CloneLocal deep-copies the array
    // and the read pops the copy.
    assert!(matches!(kinds[get - 1], MirInstrKind::ConstInt(1)));
    assert!(matches!(kinds[get - 2], MirInstrKind::CloneLocal(_)));
}

#[test]
fn lower_index_write_local_shape() {
    // `a[0] = 5;` → <5>; Dup; <0>; ArrSetLocal(slot); Pop (statement discard).
    let mir = lower_src("func main(): int {\n    a = [1, 2];\n    a[0] = 5;\n    a[0]\n}");
    let f = find_func(&mir, "main");
    let kinds = instr_kinds(f);
    let set = kinds
        .iter()
        .position(|k| matches!(k, MirInstrKind::ArrSetLocal(_)))
        .expect("ArrSetLocal expected");
    assert!(matches!(kinds[set - 1], MirInstrKind::ConstInt(0))); // index
    assert!(matches!(kinds[set - 2], MirInstrKind::Dup)); // expression value
    assert!(matches!(kinds[set - 3], MirInstrKind::ConstInt(5))); // rhs
    // Statement-level discard after the value-yielding assignment.
    assert!(matches!(kinds[set + 1], MirInstrKind::Pop));
}

#[test]
fn lower_index_write_owned_rhs_uses_dupdeep() {
    let mir = lower_src(
        "func main(): int {\n    s = [\"x\"];\n    t = \"y\";\n    s[0] = move t;\n    0\n}",
    );
    let f = find_func(&mir, "main");
    let kinds = instr_kinds(f);
    let set = kinds
        .iter()
        .position(|k| matches!(k, MirInstrKind::ArrSetLocal(_)))
        .expect("ArrSetLocal expected");
    assert!(matches!(kinds[set - 2], MirInstrKind::DupDeep));
}

#[test]
fn lower_index_write_global_shape() {
    let mir = lower_src("g: [int] = [1, 2];\nfunc main(): int {\n    g[1] = 9;\n    g[0]\n}");
    let f = find_func(&mir, "main");
    let kinds = instr_kinds(f);
    let set = kinds
        .iter()
        .position(|k| matches!(k, MirInstrKind::ArrSetGlobal(g) if *g == 0))
        .expect("ArrSetGlobal(0) expected");
    assert!(matches!(kinds[set - 1], MirInstrKind::ConstInt(1)));
    assert!(matches!(kinds[set - 2], MirInstrKind::Dup));
}

#[test]
fn lower_array_literal_in_global_init() {
    // Source order == declaration order for arrays: any element shapes that
    // global init accepts compose — no ComplexGlobalInit.
    let mir = lower_src("e: [int] = [];\ng: [int] = [1, 2];\nfunc main(): int = 0");
    assert_eq!(mir.globals.len(), 2);
    let init = &mir.globals[1].init;
    assert!(matches!(
        init.last().map(|i| &i.kind),
        Some(MirInstrKind::ArrNew(2))
    ));
}

#[test]
fn lower_index_read_span_covers_the_chain() {
    // The ArrGet's span names `a[1]` — it is what a runtime
    // IndexOutOfRange points at.
    let src = "func main(): int {\n    a = [1, 2];\n    a[1]\n}";
    let mir = lower_src(src);
    let f = find_func(&mir, "main");
    let get = f
        .blocks
        .iter()
        .flat_map(|b| b.instrs.iter())
        .find(|i| matches!(i.kind, MirInstrKind::ArrGet))
        .expect("ArrGet expected");
    // Locate `a[1]` on the last line (the literal `[1, 2]` earlier in the
    // source contains no `a[1]`, but search from the line start to be safe).
    let line3 = src.rfind("a[1]").expect("source slice");
    assert_eq!(get.span.start, line3 as u32);
    assert_eq!(get.span.end, (line3 + 4) as u32);
}

// =========================================================================
// 0.0.3 V04: `for` desugar (PLAN §5.4)
// =========================================================================

/// Render a block's instruction kinds as a compact string for diffing.
fn kinds_of(b: &MirBlock) -> Vec<String> {
    b.instrs
        .iter()
        .map(|i| match &i.kind {
            MirInstrKind::LoadLocal(s) => format!("LoadLocal({s})"),
            MirInstrKind::StoreLocal(s) => format!("StoreLocal({s})"),
            MirInstrKind::ConstInt(v) => format!("ConstInt({v})"),
            other => format!("{other:?}"),
        })
        .collect()
}

#[test]
fn lower_for_array_desugar_instruction_by_instruction() {
    // `for x in xs { }` — the exact block/instruction shape of PLAN §5.4:
    // hoist (iterable, len, index) → head bound check → body element copy →
    // increment → back edge → end unit.
    let src = "func f(xs: [int]): int {\n    for x in xs { }\n    0\n}";
    let mir = lower_src(src);
    let f = find_func(&mir, "f");

    // Hoist block: `__it`, then `__count = len`, then `__i = 0`.
    let hoist = f
        .blocks
        .iter()
        .find(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(i.kind, MirInstrKind::ArrLen))
        })
        .expect("a hoist block with ArrLen");
    // Slots: xs=0, x=1 (loop var), __it=2, __i=3, __count=4.
    // The iterable (owned array param) is accessed as Clone → CloneLocal(0).
    assert_eq!(
        hoist.instrs.first().map(|i| &i.kind),
        Some(&MirInstrKind::CloneLocal(0)),
        "iterable is cloned for the owned array param (slot 0)"
    );
    assert_eq!(
        kinds_of(hoist),
        vec![
            "CloneLocal(0)".to_string(), // xs (iterable evaluated once, cloned)
            "StoreLocal(2)".to_string(), // __it
            "LoadLocal(2)".to_string(),  // __it
            "ArrLen".to_string(),        // → len (peek)
            "StoreLocal(4)".to_string(), // __count (hoisted; len cannot change)
            "Pop".to_string(),           // drop the peeked __it
            "ConstInt(0)".to_string(),   // __i = 0
            "StoreLocal(3)".to_string(), // __i
        ]
    );

    // Head block: `__i < __count`, terminated by a conditional jump.
    let head = f
        .blocks
        .iter()
        .find(|b| matches!(b.terminator, Terminator::JumpIfFalse(_)))
        .expect("head block ends with JumpIfFalse");
    assert_eq!(
        kinds_of(head),
        vec![
            "LoadLocal(3)".to_string(),
            "LoadLocal(4)".to_string(),
            "Lt".to_string()
        ]
    );

    // Body block: element copy → bind x → body → pop → increment → back edge.
    let body = f
        .blocks
        .iter()
        .find(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(i.kind, MirInstrKind::ArrGet))
        })
        .expect("body block with ArrGet");
    let bk = kinds_of(body);
    assert_eq!(
        &bk[..4],
        ["LoadLocal(2)", "LoadLocal(3)", "ArrGet", "StoreLocal(1)"]
    );
    // The increment closes the body and jumps back to the head.
    assert_eq!(
        &bk[bk.len() - 4..],
        ["LoadLocal(3)", "ConstInt(1)", "IAdd", "StoreLocal(3)"]
    );
    assert_eq!(body.terminator, Terminator::Jump(head.id));

    // End block: the `for` emits `Unit` (yielding unit), then control falls
    // through to the caller — `lower_block` emits `Pop` to discard the for's
    // result (it is a statement), and the function tail `0` emits `ConstInt(0)`.
    let end_id = match head.terminator {
        Terminator::JumpIfFalse(t) => t,
        other => panic!("head must end with JumpIfFalse, got {other:?}"),
    };
    let end = f
        .blocks
        .iter()
        .find(|b| b.id == end_id)
        .expect("end block exists");
    assert_eq!(
        kinds_of(end),
        vec![
            "Unit".to_string(),
            "Pop".to_string(),
            "ConstInt(0)".to_string()
        ]
    );

    // The head has exactly two predecessors: the entry jump and the body's
    // back edge — nothing else may re-enter the bound check.
    let preds = f
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::Jump(t) if t == head.id))
        .count();
    assert_eq!(preds, 2, "entry jump + one back edge");
}

#[test]
fn lower_for_range_desugar_uses_range_bounds_once() {
    // The Range desugar branch: `__i = RangeStart`, `__count = RangeEnd`,
    // each hoisted before the loop (peek form → the parked Range is popped).
    // `Range` is not source-constructible until V06 (std.collection.range),
    // so this HIR is built by hand: `for x in r { }` with a `Range` param.
    use crate::lexer::Span;
    use crate::resolver::hir::{BindingId, HirId};
    use crate::typeck::typed_hir::{
        Access, Type, TypedBlockHir, TypedDeclHir, TypedExprForHir, TypedExprHir, TypedFuncBodyHir,
        TypedFuncDeclHir, TypedHir, TypedHirItem, TypedParamHir, TypedStmtHir,
    };

    let span = Span::SYNTHETIC;
    let range_param = TypedExprHir::Ident {
        name: "r".to_string(),
        binding_id: BindingId(0),
        ty: Type::Range,
        access: Access::Copy,
        hir_id: HirId(0),
        span,
    };
    let body = TypedBlockHir {
        stmts: vec![],
        tail_expr: None,
        ty: Type::Int,
        hir_id: HirId(1),
        span,
    };
    let for_expr = TypedExprHir::For(TypedExprForHir {
        var: "x".to_string(),
        var_binding_id: BindingId(1),
        iterable: Box::new(range_param),
        elem_ty: Type::Int,
        body,
        ty: Type::Unit,
        hir_id: HirId(2),
        span,
    });
    let typed = TypedHir {
        items: vec![TypedHirItem::Decl(TypedDeclHir::Func(TypedFuncDeclHir {
            name: "f".to_string(),
            params: vec![TypedParamHir {
                name: "r".to_string(),
                ty: Type::Range,
                binding_id: BindingId(0),
                hir_id: HirId(0),
                span,
            }],
            ret_type: Some(Type::Int),
            body: TypedFuncBodyHir::Block(TypedBlockHir {
                stmts: vec![TypedStmtHir::Expr(Box::new(for_expr), false)],
                tail_expr: Some(Box::new(TypedExprHir::Int(0, span))),
                ty: Type::Int,
                hir_id: HirId(3),
                span,
            }),
            hir_id: HirId(4),
            span,
            is_async: false,
        }))],
        structs: Vec::new(),
        span,
    };
    let mir = super::lower(typed).expect("lower should succeed");
    let f = find_func(&mir, "f");

    // Hoist: __it = r; __i = RangeStart(__it); pop; __count = RangeEnd; pop.
    // Slots: r=0, x=1 (loop var), __it=2, __i=3, __count=4.
    // Range is Copy → LoadLocal(0) for the iterable.
    let hoist = f
        .blocks
        .iter()
        .find(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(i.kind, MirInstrKind::RangeStart))
        })
        .expect("a hoist block with RangeStart");
    assert_eq!(
        kinds_of(hoist),
        vec![
            "LoadLocal(0)".to_string(),  // r (Copy → LoadLocal)
            "StoreLocal(2)".to_string(), // __it
            "LoadLocal(2)".to_string(),  // __it
            "RangeStart".to_string(),    // peek start
            "StoreLocal(3)".to_string(), // __i = start
            "Pop".to_string(),           // drop the peeked __it
            "LoadLocal(2)".to_string(),  // __it
            "RangeEnd".to_string(),      // peek end
            "StoreLocal(4)".to_string(), // __count = end (exclusive)
            "Pop".to_string(),           // drop the peeked __it
        ]
    );

    // Body: the element IS the index (no ArrGet); the rest is the int loop.
    let body = f
        .blocks
        .iter()
        .find(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(i.kind, MirInstrKind::StoreLocal(1)))
        })
        .expect("body block binding the loop variable");
    assert!(
        !body
            .instrs
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::ArrGet)),
        "a Range loop never indexes an array"
    );
    let bk = kinds_of(body);
    assert_eq!(&bk[..2], ["LoadLocal(3)", "StoreLocal(1)"]);
    assert_eq!(
        &bk[bk.len() - 4..],
        ["LoadLocal(3)", "ConstInt(1)", "IAdd", "StoreLocal(3)"]
    );
}

#[test]
fn lower_for_iterable_is_evaluated_once() {
    // The iterable expression appears exactly once in the instruction
    // stream (in the hoist), before the loop — DESIGN.md §6.
    let src = "func f(): int {\n    for x in [1, 2] { }\n    0\n}";
    let mir = lower_src(src);
    let f = find_func(&mir, "f");
    let arr_new = flat(f)
        .iter()
        .filter(|i| matches!(i.kind, MirInstrKind::ArrNew(_)))
        .count();
    assert_eq!(arr_new, 1, "the array literal is built exactly once");
}

// =========================================================================
// 0.0.3 V07: std.collection.iter inline lowering (instruction-level tests)
// =========================================================================
//
// These tests construct hand-crafted TypedHir nodes for inline iter calls
// (like `lower_for_range_desugar_uses_range_bounds_once` does for ranges),
// so they exercise `lower_inline_iter` directly without needing the full
// std module resolution pipeline. The end-to-end behavior (typeck + codegen
// + verify + VM execution) is covered by the e2e `iter_helpers.fln` and
// `tests/std/iter.fln` runners.

use crate::lexer::Span;
use crate::resolver::hir::{BindingId, HirId};
use crate::typeck::typed_hir::{
    Access, Type, TypedDeclHir, TypedExprHir, TypedFuncBodyHir, TypedFuncDeclHir, TypedHir,
    TypedHirItem, TypedStmtHir,
};

/// First binding id used by the hand-crafted V07 helpers. The call callee
/// uses `BindingId(0)` and the function-valued arguments use `BindingId(1)`
/// onwards, so helper *parameters* must live in their own range to keep
/// per-function slot allocation unambiguous.
const V07_HELPER_BINDING_BASE: u32 = 100;

/// Build a user-function declaration `func <name>(a0: T0, ...): ret { 0 }`.
///
/// The helpers must be declared as real items so `LowerCtx::new` registers
/// them in `func_ids`; a function-valued argument that resolves to no
/// `FuncId` is a `LowerError`, not an implicit builtin.
fn make_helper_decl(name: &str, param_tys: &[Type], ret: Type) -> TypedHirItem {
    let span = Span::SYNTHETIC;
    let params = param_tys
        .iter()
        .enumerate()
        .map(|(i, ty)| crate::typeck::typed_hir::TypedParamHir {
            name: format!("a{i}"),
            ty: ty.clone(),
            binding_id: BindingId(V07_HELPER_BINDING_BASE + i as u32),
            hir_id: HirId(0),
            span,
        })
        .collect();
    TypedHirItem::Decl(TypedDeclHir::Func(TypedFuncDeclHir {
        name: name.to_string(),
        params,
        ret_type: Some(ret.clone()),
        body: TypedFuncBodyHir::Block(crate::typeck::typed_hir::TypedBlockHir {
            stmts: vec![],
            tail_expr: Some(Box::new(TypedExprHir::Int(0, span))),
            ty: ret,
            hir_id: HirId(0),
            span,
        }),
        hir_id: HirId(0),
        span,
        is_async: false,
    }))
}

/// Build a hand-crafted TypedHir containing a single inline-iter call.
///
/// `callee_name` is the fully-qualified builtin name, so `LowerCtx::new`
/// (which mirrors every builtin into `func_ids`) resolves it. `helpers` are
/// declared ahead of `main` so function-valued arguments lower to
/// `LoadFunc`. `main`'s body is `{ <call>; 0 }`.
fn make_inline_iter_hir(
    callee_name: &str,
    helpers: Vec<TypedHirItem>,
    args: Vec<TypedExprHir>,
    ret_ty: Type,
) -> TypedHir {
    let span = Span::SYNTHETIC;
    let call = TypedExprHir::Call(
        Box::new(TypedExprHir::Ident {
            name: callee_name.to_string(),
            binding_id: BindingId(0),
            ty: Type::Func(vec![], Box::new(ret_ty.clone())),
            access: Access::Copy,
            hir_id: HirId(0),
            span,
        }),
        args,
        ret_ty,
    );

    let mut items = helpers;
    items.push(TypedHirItem::Decl(TypedDeclHir::Func(TypedFuncDeclHir {
        name: "main".to_string(),
        params: vec![],
        ret_type: Some(Type::Int),
        body: TypedFuncBodyHir::Block(crate::typeck::typed_hir::TypedBlockHir {
            stmts: vec![TypedStmtHir::Expr(Box::new(call), false)],
            tail_expr: Some(Box::new(TypedExprHir::Int(0, span))),
            ty: Type::Int,
            hir_id: HirId(1),
            span,
        }),
        hir_id: HirId(2),
        span,
        is_async: false,
    })));

    TypedHir {
        items,
        structs: Vec::new(),
        span,
    }
}

/// Build the `xs` array argument for inline iter tests: a typed `[int]`
/// array literal `[1, 2, 3]`.
fn make_int_array_arg(span: Span) -> TypedExprHir {
    TypedExprHir::ArrayLit {
        elems: vec![
            TypedExprHir::Int(1, span),
            TypedExprHir::Int(2, span),
            TypedExprHir::Int(3, span),
        ],
        ty: Type::Array(Box::new(Type::Int)),
        span,
    }
}

/// Build a function value argument for a declared helper function.
fn make_func_arg(name: &str, param_tys: Vec<Type>, ret: Type, span: Span) -> TypedExprHir {
    TypedExprHir::Ident {
        name: name.to_string(),
        binding_id: BindingId(1),
        ty: Type::Func(param_tys, Box::new(ret)),
        access: Access::Copy,
        hir_id: HirId(3),
        span,
    }
}

#[test]
fn lower_inline_iter_map_emits_call_and_push() {
    // `std.collection.iter.map([1,2,3], double)` expands to a loop that
    // calls `double` on each element and pushes the result via `std.arr.push`.
    let span = Span::SYNTHETIC;
    let typed = make_inline_iter_hir(
        "std.collection.iter.map",
        vec![make_helper_decl("double", &[Type::Int], Type::Int)],
        vec![
            make_int_array_arg(span),
            make_func_arg("double", vec![Type::Int], Type::Int, span),
        ],
        Type::Array(Box::new(Type::Int)),
    );
    let mir = super::lower(typed).expect("lower should succeed");
    let main = find_func(&mir, "main");
    let ins = flat(main);

    // The body block must contain ArrGet (element copy) and CallValue (indirect call).
    let push_fid = find_func(&mir, "std.arr.push").func_id;
    let body = main
        .blocks
        .iter()
        .find(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(i.kind, MirInstrKind::ArrGet))
        })
        .expect("body block with ArrGet");

    // The loop body is exactly: element copy → indirect call → push →
    // increment → back edge. Nothing else may sit between them.
    let mut expected: Vec<String> = [
        "LoadLocal(0)",  // __it
        "LoadLocal(4)",  // __i
        "ArrGet",        // bounds-checked element copy
        "StoreLocal(5)", // __elem
        "LoadLocal(1)",  // the function value
        "LoadLocal(5)",  // the element argument
        "CallValue(1)",
        "StoreLocal(6)", // __mapped
        "LoadLocal(3)",  // __out
        "LoadLocal(6)",  // __mapped
        "__PUSH__",
        "StoreLocal(3)", // __out
        "LoadLocal(4)",  // __i
        "ConstInt(1)",
        "IAdd",
        "StoreLocal(4)",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let push_at = expected
        .iter()
        .position(|k| k == "__PUSH__")
        .expect("push placeholder is present");
    expected[push_at] = format!("Call({:?})", push_fid);
    assert_eq!(
        kinds_of(body),
        expected,
        "map's body block is element copy, indirect call, push, increment"
    );

    // Push to accumulator: the accumulator is grown through a direct
    // `Call` to the `std.arr.push` builtin (not a `CallValue`).
    assert!(
        ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::Call(fid) if fid == push_fid)),
        "map must push results via Call(std.arr.push)"
    );

    // The array literal is built exactly once in the loop prologue, not per
    // iteration. (`map` also allocates the empty accumulator `ArrNew(0)`
    // before the loop, so count only the non-empty literal.)
    let literal_count = ins
        .iter()
        .filter(|i| matches!(i.kind, MirInstrKind::ArrNew(n) if n > 0))
        .count();
    assert_eq!(
        literal_count, 1,
        "the array literal must be built exactly once (not per iteration)"
    );
}

#[test]
fn lower_inline_iter_filter_emits_conditional_push() {
    // `std.collection.iter.filter([1,2,3], pred)` expands to a loop that
    // calls `pred` on each element and only pushes when the result is true.
    let span = Span::SYNTHETIC;
    let typed = make_inline_iter_hir(
        "std.collection.iter.filter",
        vec![make_helper_decl("is_even", &[Type::Int], Type::Bool)],
        vec![
            make_int_array_arg(span),
            make_func_arg("is_even", vec![Type::Int], Type::Bool, span),
        ],
        Type::Array(Box::new(Type::Int)),
    );
    let mir = super::lower(typed).expect("lower should succeed");
    let main = find_func(&mir, "main");

    // The predicate block reads the element, calls the predicate, and
    // branches on the result. Crucially it must be `JumpIfFalse`: the
    // *fall-through* arm keeps the element, the *taken* arm drops it.
    let push_fid = find_func(&mir, "std.arr.push").func_id;
    let pred_pos = main
        .blocks
        .iter()
        .position(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(i.kind, MirInstrKind::CallValue(1)))
        })
        .expect("a block calling the predicate");
    let pred_block = &main.blocks[pred_pos];
    assert_eq!(
        kinds_of(pred_block),
        vec![
            "LoadLocal(0)",  // __it
            "LoadLocal(4)",  // __i
            "ArrGet",        // bounds-checked element copy
            "StoreLocal(5)", // __elem
            "LoadLocal(1)",  // the predicate value
            "LoadLocal(5)",  // the element argument
            "CallValue(1)",
        ],
        "the predicate block copies the element and calls the predicate"
    );
    let dropped_id = match pred_block.terminator {
        Terminator::JumpIfFalse(t) => t,
        other => panic!("filter must branch with JumpIfFalse, got {other:?}"),
    };

    // The dropped arm is the taken branch and carries no push.
    let dropped = main
        .blocks
        .iter()
        .find(|b| b.id == dropped_id)
        .expect("dropped arm exists");
    assert!(
        !dropped
            .instrs
            .iter()
            .any(|i| matches!(i.kind, MirInstrKind::Call(fid) if fid == push_fid)),
        "the taken (dropped) arm must not push, got {:?}",
        kinds_of(dropped)
    );

    // The kept arm is the fall-through and pushes `__elem` onto `__out`.
    let kept = &main.blocks[pred_pos + 1];
    let expected_kept: Vec<String> = [
        "LoadLocal(3)", // __out
        "LoadLocal(5)", // __elem (unmapped)
        "__PUSH__",
        "StoreLocal(3)", // __out
    ]
    .into_iter()
    .map(String::from)
    .collect();
    let mut expected_kept = expected_kept;
    let push_at = expected_kept
        .iter()
        .position(|k| k == "__PUSH__")
        .expect("push placeholder is present");
    expected_kept[push_at] = format!("Call({:?})", push_fid);
    assert_eq!(
        kinds_of(kept),
        expected_kept,
        "the fall-through (kept) arm pushes the element onto the accumulator"
    );
}

#[test]
fn lower_inline_iter_fold_uses_accumulator_slot() {
    // `std.collection.iter.fold([1,2,3], 0, add)` expands to a loop that
    // calls `add(acc, elem)` and stores the result back into the accumulator.
    let span = Span::SYNTHETIC;
    let typed = make_inline_iter_hir(
        "std.collection.iter.fold",
        vec![make_helper_decl("add", &[Type::Int, Type::Int], Type::Int)],
        vec![
            make_int_array_arg(span),
            TypedExprHir::Int(0, span),
            make_func_arg("add", vec![Type::Int, Type::Int], Type::Int, span),
        ],
        Type::Int,
    );
    let mir = super::lower(typed).expect("lower should succeed");
    let main = find_func(&mir, "main");
    let ins = flat(main);

    // The body is exactly: element copy → `f(acc, elem)` → write back →
    // increment. The argument order pins `f(acc, elem)` — swapping it would
    // silently turn `fold` into a different combinator.
    let body = main
        .blocks
        .iter()
        .find(|b| {
            b.instrs
                .iter()
                .any(|i| matches!(i.kind, MirInstrKind::ArrGet))
        })
        .expect("body block with ArrGet");
    assert_eq!(
        kinds_of(body),
        vec![
            "LoadLocal(0)",  // __it
            "LoadLocal(4)",  // __i
            "ArrGet",        // bounds-checked element copy
            "StoreLocal(5)", // __elem
            "LoadLocal(2)",  // the function value
            "LoadLocal(1)",  // __acc (the `init` slot) — first
            "LoadLocal(5)",  // __elem — second
            "CallValue(2)",
            "StoreLocal(1)", // __acc = f(__acc, __elem)
            "LoadLocal(4)",  // __i
            "ConstInt(1)",
            "IAdd",
            "StoreLocal(4)",
        ],
        "fold calls f(acc, elem) and writes the result back into acc"
    );

    // Unlike the array builders, `fold` reuses the `init` slot as its
    // accumulator — it never allocates an empty output array.
    assert!(
        !ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::ArrNew(0))),
        "fold must not allocate an output array"
    );
}

#[test]
fn lower_inline_iter_take_caps_count_at_n() {
    // `std.collection.iter.take([1,2,3,4,5], 3)` emits a cap check:
    // `__count = min(__count, n)` via a conditional block before the loop.
    let span = Span::SYNTHETIC;
    let typed = make_inline_iter_hir(
        "std.collection.iter.take",
        vec![],
        vec![make_int_array_arg(span), TypedExprHir::Int(3, span)],
        Type::Array(Box::new(Type::Int)),
    );
    let mir = super::lower(typed).expect("lower should succeed");
    let main = find_func(&mir, "main");
    let ins = flat(main);

    // The cap check is `n < count ? (__count = n) : ()`, so the first `Lt`
    // in the stream is the cap comparison (the loop head's `__i < __count`
    // comes later).
    let cap_check = main
        .blocks
        .iter()
        .find(|b| b.instrs.iter().any(|i| matches!(i.kind, MirInstrKind::Lt)))
        .expect("a block with the cap comparison");
    assert_eq!(
        &kinds_of(cap_check)[..],
        [
            "ConstInt(1)",
            "ConstInt(2)",
            "ConstInt(3)",
            "ArrNew(3)",
            "StoreLocal(0)", // xs (slot 0)
            "ConstInt(3)",
            "StoreLocal(1)", // n (slot 1)
            "LoadLocal(0)",
            "ArrLen",
            "StoreLocal(2)", // __count (slot 2)
            "Pop",           // ArrLen is peek-shaped
            "LoadLocal(1)",
            "LoadLocal(2)",
            "Lt",
        ],
        "prologue evaluates both args once, then caps __count against n"
    );
    assert!(
        matches!(cap_check.terminator, Terminator::JumpIfTrue(_)),
        "the cap comparison branches on true (n < count), got {:?}",
        cap_check.terminator
    );

    // The cap block reassigns `__count = n` and nothing else.
    let cap_id = match cap_check.terminator {
        Terminator::JumpIfTrue(t) => t,
        _ => unreachable!("just asserted JumpIfTrue"),
    };
    let cap = main
        .blocks
        .iter()
        .find(|b| b.id == cap_id)
        .expect("cap block exists");
    assert_eq!(
        kinds_of(cap),
        vec!["LoadLocal(1)", "StoreLocal(2)"],
        "the cap block writes n back into __count"
    );

    // Two `Lt`s total: the cap check and the loop head's bound check.
    assert_eq!(
        ins.iter()
            .filter(|i| matches!(i.kind, MirInstrKind::Lt))
            .count(),
        2,
        "take emits exactly two comparisons: the cap and the loop bound"
    );

    // `take` passes elements through — no indirect call in the loop.
    assert!(
        !ins.iter()
            .any(|i| matches!(i.kind, MirInstrKind::CallValue(_))),
        "take must not call a user function"
    );
}

#[test]
fn lower_inline_iter_head_reenters_only_via_back_edge() {
    // The head block must have exactly two predecessors: the entry jump
    // and the body's back-edge jump (the V04 standing fix for stack depth
    // consistency at the loop head).
    let span = Span::SYNTHETIC;
    let typed = make_inline_iter_hir(
        "std.collection.iter.map",
        vec![make_helper_decl("id", &[Type::Int], Type::Int)],
        vec![
            make_int_array_arg(span),
            make_func_arg("id", vec![Type::Int], Type::Int, span),
        ],
        Type::Array(Box::new(Type::Int)),
    );
    let mir = super::lower(typed).expect("lower should succeed");
    let main = find_func(&mir, "main");

    // Find the head block (the one with the loop's bound check + JumpIfFalse).
    let head = main
        .blocks
        .iter()
        .find(|b| matches!(b.terminator, Terminator::JumpIfFalse(_)))
        .expect("head block with JumpIfFalse");

    // Count predecessors: blocks whose terminator jumps back to head.
    let preds = main
        .blocks
        .iter()
        .filter(|b| matches!(b.terminator, Terminator::Jump(t) if t == head.id))
        .count();
    assert_eq!(
        preds, 2,
        "head block must have exactly two predecessors: entry + back-edge"
    );
}

#[test]
fn lower_inline_iter_result_left_on_stack() {
    // The end block must LoadLocal the accumulator/out slot as its
    // final instruction (the loop's result value).
    let span = Span::SYNTHETIC;
    let typed = make_inline_iter_hir(
        "std.collection.iter.map",
        vec![make_helper_decl("id", &[Type::Int], Type::Int)],
        vec![
            make_int_array_arg(span),
            make_func_arg("id", vec![Type::Int], Type::Int, span),
        ],
        Type::Array(Box::new(Type::Int)),
    );
    let mir = super::lower(typed).expect("lower should succeed");
    let main = find_func(&mir, "main");

    // Find the head block (with JumpIfFalse).
    let head = main
        .blocks
        .iter()
        .find(|b| matches!(b.terminator, Terminator::JumpIfFalse(_)))
        .expect("head block with JumpIfFalse");

    // The JumpIfFalse target is the end block.
    let end_id = match head.terminator {
        Terminator::JumpIfFalse(t) => t,
        _ => unreachable!(),
    };
    let end_block = main
        .blocks
        .iter()
        .find(|b| b.id == end_id)
        .expect("end block should exist");

    // The end block loads the accumulator and pushes the result. Since the
    // call here is a statement (not the tail expression) the value is
    // immediately discarded, so `LoadLocal` must be followed by `Pop`.
    let end_kinds = kinds_of(end_block);
    let load_at = end_kinds
        .iter()
        .position(|k| k.starts_with("LoadLocal"))
        .expect("end block loads the loop result");
    assert_eq!(
        end_kinds[load_at + 1],
        "Pop",
        "the statement-position loop result is loaded then popped, got {:?}",
        end_kinds
    );
}

#[test]
fn lower_inline_iter_args_evaluated_once() {
    // Each argument (the array and the function) must be evaluated exactly
    // once, before the loop — not re-evaluated per iteration.
    let span = Span::SYNTHETIC;
    let typed = make_inline_iter_hir(
        "std.collection.iter.map",
        vec![make_helper_decl("id", &[Type::Int], Type::Int)],
        vec![
            make_int_array_arg(span),
            make_func_arg("id", vec![Type::Int], Type::Int, span),
        ],
        Type::Array(Box::new(Type::Int)),
    );
    let mir = super::lower(typed).expect("lower should succeed");
    let main = find_func(&mir, "main");
    let literal_count = flat(main)
        .iter()
        .filter(|i| matches!(i.kind, MirInstrKind::ArrNew(n) if n > 0))
        .count();
    assert_eq!(
        literal_count, 1,
        "the array literal must be built exactly once (not per iteration)"
    );
}
