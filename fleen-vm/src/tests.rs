//! Unit tests for the VM: hand-written bytecode modules.

use super::diag::line_col;
use super::error::{RuntimeError, RuntimeErrorKind};
use super::value::{PayloadKind, Value};
use super::vm::Vm;
use fleen_compiler::codegen::{Const, ConstId, Func, FuncId, Module, Opcode, SpanEntry};

fn enc(ops: &[(Opcode, Vec<u8>)]) -> Box<[u8]> {
    let mut code = Vec::new();
    for (op, args) in ops {
        code.push(*op as u8);
        code.extend_from_slice(args);
    }
    code.into_boxed_slice()
}

fn u16b(v: u16) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn u32b(v: u32) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}

fn func(code: Box<[u8]>, params: u16, locals: u16) -> Func {
    Func {
        name: ConstId(0),
        params,
        locals,
        code,
        span_map: Box::new([]),
        is_builtin: false,
    }
}

fn module_with_entry(entry_code: Box<[u8]>) -> Module {
    Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Str("main".into())],
        functions: vec![func(entry_code, 0, 0)],
        globals: vec![],
        entry: FuncId(0),
    }
}

#[test]
fn returns_const() {
    let m = Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Int(42)],
        functions: vec![func(
            enc(&[(Opcode::Const, u32b(0)), (Opcode::Return, vec![])]),
            0,
            0,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(42));
}

#[test]
fn int_add_and_store_local() {
    let m = Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Int(3)],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(0)),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::LoadLocal, u16b(0)),
                (Opcode::Const, u32b(0)),
                (Opcode::IAdd, vec![]),
                (Opcode::Return, vec![]),
            ]),
            0,
            1,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(6));
}

#[test]
fn division_by_zero() {
    let m = module_with_entry(enc(&[
        (Opcode::Const, u32b(0)),
        (Opcode::Const, u32b(1)),
        (Opcode::IDiv, vec![]),
        (Opcode::Return, vec![]),
    ]));
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Int(1), Const::Int(0)],
        ..m
    };
    assert_eq!(
        Vm::new(m).run().unwrap_err().kind,
        RuntimeErrorKind::DivisionByZero
    );
}

#[test]
fn call_and_return() {
    let add = func(
        enc(&[
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::LoadLocal, u16b(1)),
            (Opcode::IAdd, vec![]),
            (Opcode::Return, vec![]),
        ]),
        2,
        2,
    );
    let main = func(
        enc(&[
            (Opcode::Const, u32b(0)),
            (Opcode::Const, u32b(1)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        0,
        0,
    );
    let m = Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Int(4), Const::Int(5)],
        functions: vec![main, add],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(9));
}

#[test]
fn indirect_call() {
    let double = func(
        enc(&[
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::IAdd, vec![]),
            (Opcode::Return, vec![]),
        ]),
        1,
        1,
    );
    let main = func(
        enc(&[
            (Opcode::LoadFunc, u16b(1)),
            (Opcode::Const, u32b(0)),
            (Opcode::CallValue, vec![1]),
            (Opcode::Return, vec![]),
        ]),
        0,
        0,
    );
    let m = Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Int(7)],
        functions: vec![main, double],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(14));
}

#[test]
fn jump_if_false() {
    let mut code = Vec::new();
    code.push(Opcode::False as u8);
    code.push(Opcode::JumpIfFalse as u8);
    code.extend_from_slice(&12u16.to_le_bytes()); // → Const 2
    code.push(Opcode::Const as u8);
    code.extend_from_slice(&u32b(0));
    code.push(Opcode::Jump as u8);
    code.extend_from_slice(&17u16.to_le_bytes()); // → Return
    code.push(Opcode::Const as u8); // offset 12:
    code.extend_from_slice(&u32b(1));
    code.push(Opcode::Return as u8);
    let m = Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Int(1), Const::Int(2)],
        functions: vec![func(code.into_boxed_slice(), 0, 0)],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(2));
}

#[test]
fn immutable_global_rejected() {
    let m = Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Int(1)],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(0)),
                (Opcode::StoreGlobal, u16b(0)),
                (Opcode::Unit, vec![]),
                (Opcode::Return, vec![]),
            ]),
            0,
            0,
        )],
        globals: vec![fleen_compiler::codegen::Global {
            name: ConstId(0),
            mutable: false,
        }],
        entry: FuncId(0),
    };
    assert_eq!(
        Vm::new(m).run().unwrap_err().kind,
        RuntimeErrorKind::ImmutableGlobal(fleen_compiler::codegen::GlobalId(0))
    );
}

// ---------------------------------------------------------------------------
// 0.0.2 U07/U09: v2 opcodes. U07 locked their loud failure while only encoded;
// U09 replaces that with real dispatch — the per-opcode behavior now lives in
// the dedicated execution tests below. What remains here is the module-level
// gate: version 2 runs v1 instruction streams, unknown versions are rejected.
// ---------------------------------------------------------------------------

fn v2_module(code: Box<[u8]>) -> Module {
    Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into())],
        functions: vec![func(code, 0, 0)],
        globals: vec![],
        entry: FuncId(0),
    }
}

#[test]
fn v2_module_gate() {
    // A v2 module whose body contains only v1 instructions still runs:
    // `run` accepts module versions {1, 2}.
    let m = v2_module(enc(&[(Opcode::Unit, vec![]), (Opcode::Return, vec![])]));
    assert_eq!(Vm::new(m).run(), Ok(Value::Unit));

    // `ToStr` (0xA5, U13) is executed.
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Int(7)],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (Opcode::ToStr, vec![]),
                (Opcode::Return, vec![]),
            ]),
            0,
            0,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert!(matches!(Vm::new(m).run(), Ok(Value::Str(_))));

    // Unknown module versions are rejected before execution.
    let m = Module {
        struct_defs: Vec::new(),
        version: 5,
        ..v2_module(enc(&[(Opcode::Return, vec![])]))
    };
    assert_eq!(
        Vm::new(m).run(),
        Err(RuntimeError::new(RuntimeErrorKind::UnsupportedVersion(5)))
    );
}

// ---------------------------------------------------------------------------
// 0.0.2 U09: box / ref execution (BYTECODE.md §5.9).
// ---------------------------------------------------------------------------

#[test]
fn box_alloc_and_deref_roundtrip() {
    // box 42; deref b → 42 (the box stays below the pushed copy).
    let m = v2_module(enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::AllocBox, vec![]),
        (Opcode::DerefBox, vec![]),
        (Opcode::Return, vec![]),
    ]));
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(42)],
        ..m
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(42));
}

#[test]
fn box_deref_read_leaves_box_on_stack() {
    // `b → b v`: after DerefBox the box is still underneath, so a second
    // read works — pinning the peek (not pop) contract.
    let m = v2_module(enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::AllocBox, vec![]),
        (Opcode::DerefBox, vec![]),
        (Opcode::Pop, vec![]),
        (Opcode::DerefBox, vec![]),
        (Opcode::Return, vec![]),
    ]));
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(9)],
        ..m
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(9));
}

#[test]
fn nested_box_deref_reads_through() {
    // box box 42 → two DerefBox reach 42; nested boxes clone deeply.
    let m = v2_module(enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::AllocBox, vec![]),
        (Opcode::AllocBox, vec![]),
        (Opcode::DerefBox, vec![]),
        (Opcode::DerefBox, vec![]),
        (Opcode::Return, vec![]),
    ]));
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(42)],
        ..m
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(42));
}

#[test]
fn store_deref_box_writes_through_handle() {
    // MakeRefLocal + StoreDerefBox: the write lands in the handle's target
    // slot — the only form of `StoreDerefBox` with persistent effect.
    // (The compiler's own deref-write lowering emits AllocBox+StoreLocal;
    // this pins the opcode's value-level contract for hand-written modules.)
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Int(1), Const::Int(7)],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (Opcode::AllocBox, vec![]),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::MakeRefLocal, u16b(0)),
                (Opcode::Const, u32b(2)),
                (Opcode::StoreDerefBox, vec![]),
                (Opcode::LoadLocal, u16b(0)),
                (Opcode::DerefBox, vec![]),
                (Opcode::Return, vec![]),
            ]),
            0,
            1,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(7));
}

#[test]
fn clone_box_is_independent_after_write() {
    // b = box 1; c = clone b; write b (AllocBox+StoreLocal, the compiler's
    // deref-write shape); deref c must still be 1.
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Int(1), Const::Int(99)],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (Opcode::AllocBox, vec![]),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::CloneLocal, u16b(0)),
                (Opcode::StoreLocal, u16b(1)),
                (Opcode::Const, u32b(2)),
                (Opcode::AllocBox, vec![]),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::LoadLocal, u16b(1)),
                (Opcode::DerefBox, vec![]),
                (Opcode::Return, vec![]),
            ]),
            0,
            2,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(1));
}

#[test]
fn clone_box_copies_pointee_deeply() {
    // c = clone b must deep-copy: after the clone, writing b's box replaces
    // only b's slot — and the cloned box holds the same pointee value.
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Int(5)],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (Opcode::AllocBox, vec![]),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::CloneLocal, u16b(0)),
                (Opcode::StoreLocal, u16b(1)),
                // Write through b's slot: replace the box wholesale.
                (Opcode::Const, u32b(1)),
                (Opcode::AllocBox, vec![]),
                (Opcode::StoreLocal, u16b(0)),
                // Read c.
                (Opcode::LoadLocal, u16b(1)),
                (Opcode::DerefBox, vec![]),
                (Opcode::Return, vec![]),
            ]),
            0,
            2,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(5));
}

// ---------------------------------------------------------------------------
// 0.0.2 U09: move / clone execution (BYTECODE.md §5.10).
// ---------------------------------------------------------------------------

#[test]
fn move_local_returns_value() {
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Int(5)],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::MoveLocal, u16b(0)),
                (Opcode::Return, vec![]),
            ]),
            0,
            1,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(5));
}

#[test]
fn move_local_clears_slot_to_unit() {
    // After MoveLocal the slot reads Unit — the value's release is not
    // deferred to frame destruction.
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Int(5)],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::MoveLocal, u16b(0)),
                (Opcode::Pop, vec![]),
                (Opcode::LoadLocal, u16b(0)),
                (Opcode::Return, vec![]),
            ]),
            0,
            1,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Unit);
}

#[test]
fn clone_global_reads_global_slot() {
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Str("owned".into())],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (Opcode::StoreGlobal, u16b(0)),
                (Opcode::CloneGlobal, u16b(0)),
                (Opcode::Return, vec![]),
            ]),
            0,
            0,
        )],
        globals: vec![fleen_compiler::codegen::Global {
            name: ConstId(0),
            mutable: true,
        }],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Str("owned".into()));
}

// ---------------------------------------------------------------------------
// 0.0.2 U09: ref handles — forwarding, dereferencing at consumption points.
// ---------------------------------------------------------------------------

#[test]
fn ref_handle_forwards_through_call_chain() {
    // main passes a handle to f; f's slot holds the handle, and f passing it
    // on must FORWARD the original (MakeRefLocal on a slot that already
    // holds one re-pushes it) — g's CloneLocal deep-copies the ultimate
    // target, not an intermediate handle.
    let f = func(
        enc(&[
            (Opcode::MakeRefLocal, u16b(0)),
            (Opcode::Call, u16b(2)),
            (Opcode::Return, vec![]),
        ]),
        1,
        1,
    );
    let g = func(
        enc(&[(Opcode::CloneLocal, u16b(0)), (Opcode::Return, vec![])]),
        1,
        1,
    );
    let main = func(
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::MakeRefLocal, u16b(0)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        0,
        1,
    );
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Str("hi".into())],
        functions: vec![main, f, g],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Str("hi".into()));
}

#[test]
fn eq_dereferences_borrow_handles() {
    // `s == "hi"` with s a borrowed parameter: Eq must compare the target,
    // not the handles (derived PartialEq on Ref is tests-only).
    let f = func(
        enc(&[
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::Const, u32b(1)),
            (Opcode::Eq, vec![]),
            (Opcode::Return, vec![]),
        ]),
        1,
        1,
    );
    let main = func(
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::MakeRefLocal, u16b(0)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        0,
        1,
    );
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Str("hi".into())],
        functions: vec![main, f],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Bool(true));
}

#[test]
fn eq_compares_box_pointees() {
    // Two separately allocated boxes with equal pointees compare equal
    // (BYTECODE.md §3.2), exercising DupDeep on a Boxed operand.
    let m = v2_module(enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::AllocBox, vec![]),
        (Opcode::DupDeep, vec![]),
        (Opcode::Eq, vec![]),
        (Opcode::Return, vec![]),
    ]));
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(3)],
        ..m
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Bool(true));
}

#[test]
fn print_builtin_dereferences_handle_argument() {
    // print(s) with a borrowed argument must print the target, not `<ref>`.
    let print_fn = Func {
        name: ConstId(0),
        params: 1,
        locals: 1,
        code: Box::new([]),
        span_map: Box::new([]),
        is_builtin: true,
    };
    let main = func(
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::MakeRefLocal, u16b(0)),
            (Opcode::Call, u16b(1)),
            (Opcode::Const, u32b(2)),
            (Opcode::Return, vec![]),
        ]),
        0,
        1,
    );
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![
            Const::Str("print".into()),
            Const::Str("x".into()),
            Const::Int(0),
        ],
        functions: vec![main, print_fn],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(0));
}

// ---------------------------------------------------------------------------
// 0.0.2 U09: Result execution (BYTECODE.md §5.11). The `?` / choose-Result
// lowering shape is `<r> Dup/DupDeep IsErr JumpIfTrue L_err` — IsErr pops the
// duplicate, the matching Unwrap consumes the original on each branch.
// ---------------------------------------------------------------------------

/// `<payload>; Pack<k>; DupDeep; IsErr; JumpIfTrue L; Unwrap<other>; Return;
/// L: Unwrap<k>; Return` — round-trips the payload through the branch shape.
fn result_branch_module(payload: Const, pack: Opcode, unwrap_err_branch: Opcode) -> Module {
    let consts = vec![Const::Str("main".into()), payload];
    Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: consts,
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (pack, vec![]),
                (Opcode::DupDeep, vec![]),
                (Opcode::IsErr, vec![]),
                // Jump target 13 = the L_err `Unwrap<other>` below.
                (Opcode::JumpIfTrue, u16b(13)),
                (Opcode::UnwrapOk, vec![]),
                (Opcode::Return, vec![]),
                (unwrap_err_branch, vec![]),
                (Opcode::Return, vec![]),
            ]),
            0,
            0,
        )],
        globals: vec![],
        entry: FuncId(0),
    }
}

#[test]
fn result_ok_roundtrip_through_branch_shape() {
    let m = result_branch_module(Const::Int(5), Opcode::PackOk, Opcode::UnwrapErr);
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(5));
}

#[test]
fn result_err_roundtrip_through_branch_shape() {
    let m = result_branch_module(
        Const::Str("boom".into()),
        Opcode::PackErr,
        Opcode::UnwrapErr,
    );
    assert_eq!(Vm::new(m).run().unwrap(), Value::Str("boom".into()));
}

#[test]
fn unwrap_mismatch_is_defensive_error() {
    // UnwrapOk on Err / UnwrapErr on Ok fire ResultMismatch — the branch
    // jumps make this unreachable from compiled code, the VM must not
    // silently degrade.
    let ok_then_unwrap_err = enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::PackOk, vec![]),
        (Opcode::UnwrapErr, vec![]),
        (Opcode::Return, vec![]),
    ]);
    let m = v2_module(ok_then_unwrap_err);
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(5)],
        ..m
    };
    assert_eq!(
        Vm::new(m).run().unwrap_err().kind,
        RuntimeErrorKind::ResultMismatch
    );

    let err_then_unwrap_ok = enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::PackErr, vec![]),
        (Opcode::UnwrapOk, vec![]),
        (Opcode::Return, vec![]),
    ]);
    let m = v2_module(err_then_unwrap_ok);
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(5)],
        ..m
    };
    assert_eq!(
        Vm::new(m).run().unwrap_err().kind,
        RuntimeErrorKind::ResultMismatch
    );
}

#[test]
fn is_err_classifies_variants() {
    let ok_case = enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::PackOk, vec![]),
        (Opcode::IsErr, vec![]),
        (Opcode::Return, vec![]),
    ]);
    let m = v2_module(ok_case);
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(5)],
        ..m
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Bool(false));

    let err_case = enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::PackErr, vec![]),
        (Opcode::IsErr, vec![]),
        (Opcode::Return, vec![]),
    ]);
    let m = v2_module(err_case);
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(5)],
        ..m
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Bool(true));
}

#[test]
fn borrow_out_of_range_is_defensive() {
    // A handle whose target left the live stack fires BorrowOutOfRange at
    // the consumption point. The compiler cannot produce this (typeck keeps
    // handles below the current frame and they cannot be stored), so it is
    // built by hand: `f` stores a handle to its own slot into a global and
    // returns — the frame truncation destroys the target, and `main`'s
    // later DupDeep dereference must fail loudly instead of reading garbage.
    let f = func(
        enc(&[
            (Opcode::MakeRefLocal, u16b(2)),
            (Opcode::StoreGlobal, u16b(0)),
            (Opcode::Unit, vec![]),
            (Opcode::Return, vec![]),
        ]),
        0,
        3,
    );
    let main = func(
        enc(&[
            (Opcode::Call, u16b(1)),
            (Opcode::CloneGlobal, u16b(0)),
            (Opcode::DupDeep, vec![]),
            (Opcode::Return, vec![]),
        ]),
        0,
        0,
    );
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into())],
        functions: vec![main, f],
        globals: vec![fleen_compiler::codegen::Global {
            name: ConstId(0),
            mutable: true,
        }],
        entry: FuncId(0),
    };
    assert_eq!(
        Vm::new(m).run().unwrap_err().kind,
        RuntimeErrorKind::BorrowOutOfRange
    );
}

#[test]
fn dup_deep_on_handle_copies_target() {
    // DupDeep with a handle on top must push a copy of the handle's target,
    // never the handle itself (the clone of a borrowed value). Returns the
    // copied target directly — a returned handle would read as `<ref>`.
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Str("hi".into())],
        functions: vec![func(
            enc(&[
                (Opcode::Const, u32b(1)),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::MakeRefLocal, u16b(0)),
                (Opcode::DupDeep, vec![]),
                (Opcode::Return, vec![]),
            ]),
            0,
            1,
        )],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Str("hi".into()));
}

#[test]
fn ord_dereferences_borrow_handles() {
    // `s < "hi"` with s a borrowed parameter: Lt must compare the target
    // strings, same discipline as Eq.
    let f = func(
        enc(&[
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::Const, u32b(1)),
            (Opcode::Lt, vec![]),
            (Opcode::Return, vec![]),
        ]),
        1,
        1,
    );
    let main = func(
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::MakeRefLocal, u16b(0)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        0,
        1,
    );
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![Const::Str("main".into()), Const::Str("hi".into())],
        functions: vec![main, f],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Bool(false));
}

#[test]
fn eq_ok_vs_err_is_false() {
    // Ok(5) and Err(5) are never equal, even with identical payloads
    // (BYTECODE.md §3.2: the variant is part of the value's identity).
    let m = v2_module(enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::PackOk, vec![]),
        (Opcode::Const, u32b(1)),
        (Opcode::PackErr, vec![]),
        (Opcode::Eq, vec![]),
        (Opcode::Return, vec![]),
    ]));
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(5)],
        ..m
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Bool(false));
}

#[test]
fn store_deref_box_consumes_its_box_operand() {
    // Plain-Boxed path (no handle): StoreDerefBox writes the pointee of the
    // box it pops and then *discards that box* — the original box below is
    // untouched. The compiler no longer emits this opcode, but the
    // value-level contract must hold for hand-written modules.
    let m = v2_module(enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::AllocBox, vec![]),
        // DupDeep clones the box; the write below lands in the clone.
        (Opcode::DupDeep, vec![]),
        (Opcode::Const, u32b(2)),
        (Opcode::StoreDerefBox, vec![]),
        // The operand was discarded; the original still reads 1.
        (Opcode::DerefBox, vec![]),
        (Opcode::Return, vec![]),
    ]));
    let m = Module {
        struct_defs: Vec::new(),
        constants: vec![Const::Str("main".into()), Const::Int(1), Const::Int(7)],
        ..m
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(1));
}

#[test]
fn handle_discipline_enforced_at_runtime() {
    // Boxing or packing a borrow handle fires BorrowOutOfRange in *any*
    // build (unreachable from compiled code; the backstop exists because
    // `fleen-verify` has no value-type view of the stack).
    for (op, name) in [
        (Opcode::AllocBox, "AllocBox"),
        (Opcode::PackOk, "PackOk"),
        (Opcode::PackErr, "PackErr"),
    ] {
        let code = enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::MakeRefLocal, u16b(0)),
            (op, vec![]),
            (Opcode::Return, vec![]),
        ]);
        let m = Module {
            struct_defs: Vec::new(),
            version: 2,
            constants: vec![Const::Str("main".into()), Const::Str("hi".into())],
            functions: vec![func(code, 0, 1)],
            globals: vec![],
            entry: FuncId(0),
        };
        assert_eq!(
            Vm::new(m).run().unwrap_err().kind,
            RuntimeErrorKind::BorrowOutOfRange,
            "{name} on a borrow handle must fail loudly"
        );
    }
}

// ---------------------------------------------------------------------------
// 0.0.2 U08: Value ownership representation (BYTECODE.md §3.2). Strings are
// exclusively owned now; `Boxed`/`Ref`/`Ok`/`Err` are defined ahead of U09's
// instruction dispatch, which is the only place that can produce them.
// ---------------------------------------------------------------------------

#[test]
fn value_display_new_variants() {
    // `Boxed` prints the inner value; `Ref` is opaque; Result payloads are
    // wrapped in `Ok(...)` / `Err(...)` (TICKETS U08).
    assert_eq!(Value::Boxed(Box::new(Value::Int(3))).to_string(), "3");
    assert_eq!(Value::Ref { base: 0, slot: 1 }.to_string(), "<ref>");
    assert_eq!(Value::Ok(Box::new(Value::Int(1))).to_string(), "Ok(1)");
    assert_eq!(
        Value::Err(Box::new(Value::Str("boom".into()))).to_string(),
        "Err(boom)"
    );
}

#[test]
fn value_partial_eq_and_clone_semantics() {
    // `Str` still compares by content under `Box<str>` — the switch from
    // shared-reference strings must not change v1 equality behavior.
    assert_eq!(Value::Str("a".into()), Value::Str("a".into()));
    // `Boxed` compares the inner value (Rust `Box: PartialEq`), matching
    // the `Eq` semantics specified in BYTECODE.md §3.2.
    assert_eq!(
        Value::Boxed(Box::new(Value::Int(1))),
        Value::Boxed(Box::new(Value::Int(1)))
    );
    // Nested boxes clone deeply and stay structurally equal (mutation paths
    // such as `StoreDerefBox` arrive with U09; this pins the derive
    // behavior the `Eq` instruction will build on).
    let nested = Value::Boxed(Box::new(Value::Boxed(Box::new(Value::Str("deep".into())))));
    assert_eq!(nested, nested.clone());
}

#[test]
fn reserved_error_display_messages() {
    // U08 pre-reserves these two variants for U09's dispatch; pin their
    // user-facing text so a wording change is a deliberate act. The
    // `ResultMismatch` message stays variant-symmetric: U09 fires it for
    // `UnwrapErr` on `Ok` too.
    assert_eq!(
        RuntimeError::new(RuntimeErrorKind::ResultMismatch).to_string(),
        "unwrap opcode applied to mismatched Result variant"
    );
    assert_eq!(
        RuntimeError::new(RuntimeErrorKind::BorrowOutOfRange).to_string(),
        "borrow handle out of range"
    );
}

#[test]
fn string_return_through_call() {
    // Strings now flow through `Return` under exclusive ownership; the old
    // return plumbing cloned the value on every call, which would have
    // become a hidden deep copy. Behavior is unchanged — this pins it.
    let f = func(
        enc(&[(Opcode::Const, u32b(1)), (Opcode::Return, vec![])]),
        0,
        0,
    );
    let main = func(
        enc(&[(Opcode::Call, u16b(1)), (Opcode::Return, vec![])]),
        0,
        0,
    );
    let m = Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Str("main".into()), Const::Str("hi".into())],
        functions: vec![main, f],
        globals: vec![],
        entry: FuncId(0),
    };
    assert_eq!(Vm::new(m).run().unwrap(), Value::Str("hi".into()));
}

// ---------------------------------------------------------------------------
// 0.0.2 U10: diagnostics — runtime errors carry source location.
// ---------------------------------------------------------------------------

#[test]
fn runtime_error_carries_source_span() {
    let src = "func main(): int {\n    a = 1;\n    b = 0;\n    q = a / b;\n    q\n}\n";
    let err = Vm::new(fleen_compiler::compile(src).unwrap())
        .run()
        .unwrap_err();
    assert_eq!(err.kind, RuntimeErrorKind::DivisionByZero);
    let (start, end) = err.span.expect("span_map must supply a span");
    // Binary expression AST nodes carry no span of their own (`Expr::span`
    // delegates to the lhs), so the `IDiv` instruction records `a`, not the
    // whole `a / b`. Widening that span is a compiler-side change (TICKETS
    // U10 implementation notes); this pins today's contract so widening it
    // later is a deliberate act.
    assert_eq!(&src[start as usize..end as usize], "a");
    // 1-based line/column of the span start (`a` of `a / b` on line 4).
    assert_eq!(line_col(src, start), (4, 9));
    assert_eq!(err.func.as_deref(), Some("main"));
    assert!(err.pc.is_some());
}

#[test]
fn call_path_error_span_points_at_call_site() {
    // `CallDepthExceeded` surfaces before the callee's frame is pushed, and
    // `Call`/`CallValue` rewind the caller's ip on failure, so the location
    // comes from the caller's frame: the deepest recursive call site, not
    // the callee's body and not the instruction after the call.
    let src = concat!(
        "func f(n: int): int {\n",
        "    if n <= 0 { 0 } else { f(n - 1) }\n",
        "}\n",
        "func main(): int {\n",
        "    f(100000)\n",
        "}\n",
    );
    let err = Vm::new(fleen_compiler::compile(src).unwrap())
        .run()
        .unwrap_err();
    assert_eq!(err.kind, RuntimeErrorKind::CallDepthExceeded);
    let (start, end) = err.span.expect("span attached");
    // Same lhs-span convention as `runtime_error_carries_source_span`: the
    // `Call` instruction records the callee expression's head, here `f`.
    assert_eq!(&src[start as usize..end as usize], "f");
    assert_eq!(err.func.as_deref(), Some("f"));
}

#[test]
fn call_path_error_picks_the_call_entry_not_the_next_instruction() {
    // Regression for the U10 review finding: the caller's ip is advanced
    // past the `Call` before `call` runs, so without the rewind the span
    // lookup (`closest entry at or before ip`) would land on the *next*
    // instruction's entry. Synthetic spans make the two entries
    // distinguishable: the call site's span is (7, 8), the following
    // `Const`'s is (20, 21); the error must report the former.
    let builtin = Func {
        name: ConstId(1),
        params: 0,
        locals: 0,
        code: Box::new([]),
        span_map: Box::new([]),
        is_builtin: true,
    };
    let main_code = enc(&[
        (Opcode::Call, u16b(1)),
        (Opcode::Const, u32b(2)),
        (Opcode::Return, vec![]),
    ]);
    let main_fn = Func {
        name: ConstId(0),
        params: 0,
        locals: 0,
        code: main_code,
        span_map: Box::new([
            SpanEntry {
                offset: 0,
                start: 7,
                end: 8,
            },
            SpanEntry {
                offset: 3,
                start: 20,
                end: 21,
            },
        ]),
        is_builtin: false,
    };
    let m = Module {
        struct_defs: Vec::new(),
        version: 2,
        constants: vec![
            Const::Str("main".into()),
            Const::Str("nope".into()),
            Const::Int(1),
        ],
        functions: vec![main_fn, builtin],
        globals: vec![],
        entry: FuncId(0),
    };
    let err = Vm::new(m).run().unwrap_err();
    assert_eq!(err.kind, RuntimeErrorKind::UnknownBuiltin("nope".into()));
    // The call site's span, not the following `Const`'s.
    assert_eq!(err.span, Some((7, 8)));
    // The ip was rewound to the `Call` itself.
    assert_eq!(err.pc, Some(0));
}

#[test]
fn error_without_span_map_entries_degrades() {
    // Hand-written v1-style module (empty span_map): no span, but the
    // function name and pc are still attached and nothing panics.
    let m = module_with_entry(enc(&[
        (Opcode::Const, u32b(1)),
        (Opcode::Const, u32b(2)),
        (Opcode::IDiv, vec![]),
        (Opcode::Return, vec![]),
    ]));
    let m = Module {
        struct_defs: Vec::new(),
        // Const 0 stays the function's name so `func` can be attached.
        constants: vec![Const::Str("main".into()), Const::Int(1), Const::Int(0)],
        ..m
    };
    let err = Vm::new(m).run().unwrap_err();
    assert_eq!(err.kind, RuntimeErrorKind::DivisionByZero);
    assert_eq!(err.span, None);
    assert_eq!(err.func.as_deref(), Some("main"));
    // The failing `IDiv` sits at offset 10 (two 5-byte `Const` instructions).
    assert_eq!(err.pc, Some(10));
}

#[test]
fn unsupported_version_has_no_location() {
    // No frame is ever pushed, so no location can be attached.
    let m = Module {
        struct_defs: Vec::new(),
        version: 5,
        ..v2_module(enc(&[(Opcode::Return, vec![])]))
    };
    let err = Vm::new(m).run().unwrap_err();
    assert_eq!(
        err,
        RuntimeError::new(RuntimeErrorKind::UnsupportedVersion(5))
    );
}

// ---------------------------------------------------------------------------
// 0.0.3 V02: structs. Value::Struct semantics: fields are copied out
// (GetField), overwrites release the old field, deep clone independence,
// and defensive failures for shape violations legal bytecode never emits.
// ---------------------------------------------------------------------------

fn fleenc_struct_def(n: usize) -> fleen_compiler::codegen::StructDefMeta {
    fleen_compiler::codegen::StructDefMeta {
        field_types: vec![fleenc_int_tag(); n].into_boxed_slice(),
    }
}

fn fleenc_int_tag() -> fleen_compiler::codegen::FieldTypeTag {
    fleen_compiler::codegen::FieldTypeTag::Int
}

#[test]
fn struct_field_copy_independence() {
    // p = {1, 2}; q = p (Copy struct → LoadLocal); q.a = 9; p.a must stay 1.
    // main returns p.a via GetField(0).
    let code: Vec<u8> = [
        vec![Opcode::Const as u8],
        u32b(1).to_vec(), // 1
        vec![Opcode::Const as u8],
        u32b(1).to_vec(), // 1
        vec![Opcode::StructNew as u8, 0, 0],
        vec![Opcode::StoreLocal as u8, 0, 0], // p (slot 0)
        vec![Opcode::LoadLocal as u8, 0, 0],  // q = p (Copy load)
        vec![Opcode::StoreLocal as u8, 1, 0], // q (slot 1)
        vec![Opcode::Const as u8],
        u32b(1).to_vec(), // 9
        vec![Opcode::Const as u8],
        u32b(1).to_vec(),
        vec![Opcode::StructNew as u8, 0, 0],  // {9, 9}
        vec![Opcode::StoreLocal as u8, 1, 0], // q = {9, 9}
        vec![Opcode::LoadLocal as u8, 0, 0],  // p
        vec![Opcode::GetField as u8, 0, 0],   // p.a
        vec![Opcode::Return as u8],
    ]
    .concat();
    let m = Module {
        version: 3,
        constants: vec![Const::Str("main".into()), Const::Int(1), Const::Int(9)],
        functions: vec![func(code.into_boxed_slice(), 0, 2)],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: vec![fleenc_struct_def(2)],
    };
    assert_eq!(Vm::new(m).run(), Ok(Value::Int(1)));
}

#[test]
fn struct_setfield_overwrites_field() {
    // p = {1, 2}; p.a = 9; return p.a → 9 (SetFieldLocal path).
    let code: Vec<u8> = [
        vec![Opcode::Const as u8],
        u32b(1).to_vec(),
        vec![Opcode::Const as u8],
        u32b(1).to_vec(),
        vec![Opcode::StructNew as u8, 0, 0],
        vec![Opcode::StoreLocal as u8, 0, 0],
        vec![Opcode::Const as u8],
        u32b(2).to_vec(), // 9
        vec![Opcode::Dup as u8],
        vec![Opcode::SetFieldLocal as u8, 0, 0, 0, 0],
        vec![Opcode::Pop as u8],
        vec![Opcode::LoadLocal as u8, 0, 0],
        vec![Opcode::GetField as u8, 0, 0],
        vec![Opcode::Return as u8],
    ]
    .concat();
    let m = Module {
        version: 3,
        constants: vec![Const::Str("main".into()), Const::Int(1), Const::Int(9)],
        functions: vec![func(code.into_boxed_slice(), 0, 1)],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: vec![fleenc_struct_def(2)],
    };
    assert_eq!(Vm::new(m).run(), Ok(Value::Int(9)));
}

#[test]
fn struct_eq_is_defensive_error() {
    // `Eq` on two struct values is a typeck-level error; a hand-written
    // module must fail loudly (TypeMismatch), not compare.
    let code: Vec<u8> = [
        vec![Opcode::True as u8],
        vec![Opcode::True as u8],
        vec![Opcode::StructNew as u8, 0, 0],
        vec![Opcode::True as u8],
        vec![Opcode::True as u8],
        vec![Opcode::StructNew as u8, 0, 0],
        vec![Opcode::Eq as u8],
        vec![Opcode::Return as u8],
    ]
    .concat();
    let m = Module {
        version: 3,
        constants: vec![Const::Str("main".into())],
        functions: vec![func(code.into_boxed_slice(), 0, 0)],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: vec![fleenc_struct_def(2)],
    };
    assert!(matches!(
        Vm::new(m).run(),
        Err(RuntimeError {
            kind: RuntimeErrorKind::ArithTypeMismatch,
            ..
        })
    ));
}

#[test]
fn struct_new_out_of_range_id() {
    // StructNew(9) with an empty struct table (defensive; verify rejects it
    // before execution — this pins the VM backstop).
    let m = Module {
        version: 3,
        constants: vec![Const::Str("main".into())],
        functions: vec![func(
            enc(&[(Opcode::StructNew, vec![9, 0]), (Opcode::Return, vec![])]),
            0,
            0,
        )],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: Vec::new(),
    };
    assert!(matches!(
        Vm::new(m).run(),
        Err(RuntimeError {
            kind: RuntimeErrorKind::StructOutOfRange(9),
            ..
        })
    ));
}

#[test]
fn struct_getfield_out_of_range_offset() {
    // GetField(5) on a 2-field struct: defensive FieldOutOfRange (verify
    // cannot check offsets statically — the encoding carries no struct id).
    let code: Vec<u8> = [
        vec![Opcode::True as u8],
        vec![Opcode::True as u8],
        vec![Opcode::StructNew as u8, 0, 0],
        vec![Opcode::GetField as u8, 5, 0],
        vec![Opcode::Return as u8],
    ]
    .concat();
    let m = Module {
        version: 3,
        constants: vec![Const::Str("main".into())],
        functions: vec![func(code.into_boxed_slice(), 0, 0)],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: vec![fleenc_struct_def(2)],
    };
    assert!(matches!(
        Vm::new(m).run(),
        Err(RuntimeError {
            kind: RuntimeErrorKind::FieldOutOfRange { index: 5, len: 2 },
            ..
        })
    ));
}

#[test]
fn struct_setfield_out_of_range_offset() {
    // SetFieldLocal(slot 0, offset 5) on a 2-field struct: defensive
    // FieldOutOfRange (verify cannot check offsets statically — the encoding
    // carries no struct id); the GetField sibling is covered above.
    let m = Module {
        version: 3,
        constants: vec![Const::Str("main".into())],
        functions: vec![func(
            enc(&[
                (Opcode::True, vec![]),
                (Opcode::True, vec![]),
                (Opcode::StructNew, u16b(0)),
                (Opcode::StoreLocal, u16b(0)),
                (Opcode::True, vec![]),
                (Opcode::SetFieldLocal, vec![0, 0, 5, 0]),
                (Opcode::Return, vec![]),
            ]),
            0,
            1,
        )],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: vec![fleenc_struct_def(2)],
    };
    assert!(matches!(
        Vm::new(m).run(),
        Err(RuntimeError {
            kind: RuntimeErrorKind::FieldOutOfRange { index: 5, len: 2 },
            ..
        })
    ));
}

#[test]
fn struct_deep_clone_via_dupdeep() {
    // A struct holding a box: DupDeep must deep-copy (pointee independence).
    // p = {box 1}; q = DupDeep(p); write through q's box; p's box unchanged.
    let code: Vec<u8> = [
        vec![Opcode::Const as u8],
        u32b(1).to_vec(),
        vec![Opcode::AllocBox as u8],
        vec![Opcode::StructNew as u8, 0, 0], // 1-field struct: box
        vec![Opcode::DupDeep as u8],         // q = clone p
        vec![Opcode::StoreLocal as u8, 0, 0], // p (slot 0)
        vec![Opcode::StoreLocal as u8, 1, 0], // q (slot 1)
        vec![Opcode::Const as u8],
        u32b(2).to_vec(), // 9
        vec![Opcode::AllocBox as u8],
        vec![Opcode::SetFieldLocal as u8, 1, 0, 0, 0], // q.field0 = box 9
        // Read p's box pointee: LoadLocal p (slot 0), GetField(0), DerefBox
        // leaves box copy + pointee copy; park the pointee, pop the box.
        vec![Opcode::LoadLocal as u8, 0, 0],
        vec![Opcode::GetField as u8, 0, 0],
        vec![Opcode::DerefBox as u8],
        vec![Opcode::StoreLocal as u8, 2, 0],
        vec![Opcode::Pop as u8],
        vec![Opcode::LoadLocal as u8, 2, 0],
        vec![Opcode::Return as u8],
    ]
    .concat();
    let m = Module {
        version: 3,
        constants: vec![Const::Str("main".into()), Const::Int(1), Const::Int(9)],
        functions: vec![func(code.into_boxed_slice(), 0, 3)],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: vec![fleenc_struct_def(1)],
    };
    // q's write reached only q: p's pointee is still 1.
    assert_eq!(Vm::new(m).run(), Ok(Value::Int(1)));
}

// ---------------------------------------------------------------------------
// 0.0.3 V03: arrays (bytecode v3, 0xB4–0xB8). `ArrGet` bounds failures are a
// real runtime check (D3), not a defensive backstop: `a[i]` is bounds-checked
// at runtime by design. `ArrSet*` take the array from the slot/global, so the
// operand stack only carries the value and the index (value below, index on
// top — the VM pops the index first). `CloneLocal(slot)` reads the *source*
// slot and pushes a deep copy.
// ---------------------------------------------------------------------------

fn v3_array_module(constants: Vec<Const>, code: Box<[u8]>, locals: u16) -> Module {
    Module {
        struct_defs: Vec::new(),
        version: 3,
        constants,
        functions: vec![func(code, 0, locals)],
        globals: vec![],
        entry: FuncId(0),
    }
}

#[test]
fn arr_new_get_set_execution() {
    // a = [10, 20]; a[0] = 99; return a[0] → 99.
    let m = v3_array_module(
        vec![
            Const::Str("main".into()),
            Const::Int(10),
            Const::Int(20),
            Const::Int(99),
            Const::Int(0),
        ],
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::Const, u32b(2)),
            (Opcode::ArrNew, u16b(2)),
            (Opcode::StoreLocal, u16b(0)),
            // a[0] = 99 (array lives in slot 0; stack carries value then index)
            (Opcode::Const, u32b(3)),
            (Opcode::Const, u32b(4)),
            (Opcode::ArrSetLocal, u16b(0)),
            // return a[0]
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::Const, u32b(4)),
            (Opcode::ArrGet, vec![]),
            (Opcode::Return, vec![]),
        ]),
        1,
    );
    assert_eq!(Vm::new(m).run(), Ok(Value::Int(99)));
}

#[test]
fn arr_len_of_new_array() {
    // `[10, 20, 30]` → ArrLen → 3. ArrLen is a peek (`a → a int`): the length
    // is pushed on top of the array, so a second slot stashes the length and
    // `Pop` drops the array below it before Return.
    let m = v3_array_module(
        vec![
            Const::Str("main".into()),
            Const::Int(10),
            Const::Int(20),
            Const::Int(30),
        ],
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::Const, u32b(2)),
            (Opcode::Const, u32b(3)),
            (Opcode::ArrNew, u16b(3)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::ArrLen, vec![]),
            (Opcode::StoreLocal, u16b(1)), // length → slot 1, array remains
            (Opcode::Pop, vec![]),         // drop the array
            (Opcode::LoadLocal, u16b(1)),  // length
            (Opcode::Return, vec![]),
        ]),
        2,
    );
    assert_eq!(Vm::new(m).run(), Ok(Value::Int(3)));
}

#[test]
fn arr_get_negative_index_is_out_of_range() {
    // `a[-1]` must be an out-of-range error, never a huge usize index — the
    // sign check precedes the cast.
    let m = v3_array_module(
        vec![Const::Str("main".into()), Const::Int(10), Const::Int(-1)],
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::ArrNew, u16b(1)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::Const, u32b(2)), // -1
            (Opcode::ArrGet, vec![]),
            (Opcode::Return, vec![]),
        ]),
        1,
    );
    assert_eq!(
        Vm::new(m).run().unwrap_err().kind,
        RuntimeErrorKind::IndexOutOfRange { index: -1, len: 1 }
    );
}

#[test]
fn arr_index_out_of_range_carries_span() {
    // End-to-end: the `IndexOutOfRange` runtime error points at the whole
    // `a[5]` expression (the span now covers the closing `]`), matching the
    // design intent and the lower-span contract.
    let src = "func main(): int {\n    a = [10, 20];\n    a[5]\n}\n";
    let err = Vm::new(fleen_compiler::compile(src).unwrap())
        .run()
        .unwrap_err();
    assert_eq!(
        err.kind,
        RuntimeErrorKind::IndexOutOfRange { index: 5, len: 2 }
    );
    let (start, end) = err.span.expect("span_map must supply a span");
    assert_eq!(&src[start as usize..end as usize], "a[5]");
    assert_eq!(err.func.as_deref(), Some("main"));
    assert!(err.pc.is_some());
}

#[test]
fn arr_deep_clone_independence() {
    // b = clone a (deep copy of the owned array); writing b[0] must not
    // disturb a — the clone copies the element values, not the binding.
    let m = v3_array_module(
        vec![
            Const::Str("main".into()),
            Const::Str("x".into()),
            Const::Str("y".into()),
            Const::Str("z".into()),
            Const::Int(0),
        ],
        enc(&[
            (Opcode::Const, u32b(1)), // "x"
            (Opcode::Const, u32b(2)), // "y"
            (Opcode::ArrNew, u16b(2)),
            (Opcode::StoreLocal, u16b(0)), // a
            (Opcode::CloneLocal, u16b(0)), // push copy of a (slot 0 is source)
            (Opcode::StoreLocal, u16b(1)), // b
            // b[0] = "z"
            (Opcode::Const, u32b(3)), // "z"
            (Opcode::Const, u32b(4)), // index 0
            (Opcode::ArrSetLocal, u16b(1)),
            // return a[0]
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::Const, u32b(4)),
            (Opcode::ArrGet, vec![]),
            (Opcode::Return, vec![]),
        ]),
        2,
    );
    assert_eq!(Vm::new(m).run(), Ok(Value::Str("x".into())));
}

#[test]
fn empty_array_len_is_zero() {
    // `[ ]` (ArrNew 0) has length 0. ArrLen is a peek (`a → a int`): stash the
    // length in a slot and `Pop` the array before Return.
    let m = v3_array_module(
        vec![Const::Str("main".into())],
        enc(&[
            (Opcode::ArrNew, u16b(0)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::ArrLen, vec![]),
            (Opcode::StoreLocal, u16b(1)),
            (Opcode::Pop, vec![]),
            (Opcode::LoadLocal, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        2,
    );
    assert_eq!(Vm::new(m).run(), Ok(Value::Int(0)));
}

#[test]
fn eq_on_two_arrays_is_defensive_error() {
    // `==` on two array values is a typeck-level error; the VM must not
    // silently compare (arrays are not `Eq`-able in Fleen).
    let m = v3_array_module(
        vec![
            Const::Str("main".into()),
            Const::Str("x".into()),
            Const::Str("x".into()),
        ],
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::ArrNew, u16b(1)),
            (Opcode::Const, u32b(2)),
            (Opcode::ArrNew, u16b(1)),
            (Opcode::Eq, vec![]),
            (Opcode::Return, vec![]),
        ]),
        0,
    );
    assert_eq!(
        Vm::new(m).run().unwrap_err().kind,
        RuntimeErrorKind::ArithTypeMismatch
    );
}

#[test]
fn value_arr_display() {
    // `Value::Arr` renders `[a, b]` (deep elements rendered recursively).
    assert_eq!(
        Value::Arr(Box::new([Value::Int(1), Value::Int(2)])).to_string(),
        "[1, 2]"
    );
    assert_eq!(
        Value::Arr(Box::new([Value::Str("x".into())])).to_string(),
        "[x]"
    );
    assert_eq!(Value::Arr(Box::new([])).to_string(), "[]");
}

// ---------------------------------------------------------------------------
// 0.0.3 V06: VM intrinsics (std.io, std.str, std.num, std.os, std.time,
// std.arr).  These exercise `exec_builtin` by name — each test builds a v3
// module with one `is_builtin` function and a `Call` to it, following the
// pattern of `print_builtin_dereferences_handle_argument`.
// ---------------------------------------------------------------------------

/// Build a v3 module whose `main` function (FuncId 0) calls a single builtin
/// (FuncId 1).  `constants[0]` must be the builtin name string; user
/// constants follow at indices 1..  `argc` sets the builtin's `params`
/// field, which `Call` peeks to determine how many stack values to pop.
///
/// The `Call` opcode pops `params` values and reverses them (last-pushed =
/// args[argc-1]), so push arg[0] first (bottom) and arg[argc-1] last (top).
fn builtin_call_module(main_code: Box<[u8]>, argc: u16, constants: Vec<Const>) -> Module {
    Module {
        struct_defs: Vec::new(),
        version: 3,
        constants,
        functions: vec![
            func(main_code, 0, 0),
            Func {
                name: ConstId(0),
                params: argc,
                locals: 0,
                code: Box::new([]),
                span_map: Box::new([]),
                is_builtin: true,
            },
        ],
        globals: vec![],
        entry: FuncId(0),
    }
}

/// Build a module whose `main` calls a zero-arity builtin.
fn builtin_noarg_module(name: &str) -> Module {
    builtin_call_module(
        enc(&[(Opcode::Call, u16b(1)), (Opcode::Return, vec![])]),
        0,
        vec![Const::Str(name.into())],
    )
}

#[test]
fn std_os_exit_propagates_exit_code() {
    // std.os.exit(3) → Exit(3), surfaced as RuntimeError by Vm::run.
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)), // 3
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        1,
        vec![Const::Str("std.os.exit".into()), Const::Int(3)],
    );
    let err = Vm::new(m).run().unwrap_err();
    assert_eq!(err.kind, RuntimeErrorKind::Exit(3));
}

#[test]
fn std_os_exit_non_integer_arg_is_stack_error() {
    // Malformed bytecode: exit given a non-int argument.
    let m = builtin_call_module(
        enc(&[
            (Opcode::Unit, vec![]),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        1,
        vec![Const::Str("std.os.exit".into())],
    );
    let err = Vm::new(m).run().unwrap_err();
    assert_eq!(err.kind, RuntimeErrorKind::StackShape);
}

#[test]
fn std_io_println_returns_unit() {
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)), // "hello"
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        1,
        vec![
            Const::Str("std.io.println".into()),
            Const::Str("hello".into()),
        ],
    );
    assert_eq!(Vm::new(m).run().unwrap(), Value::Unit);
}

#[test]
fn std_str_len_counts_unicode_chars() {
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        1,
        vec![Const::Str("std.str.len".into()), Const::Str("héllo".into())],
    );
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(5));
}

#[test]
fn std_str_len_empty_string_is_zero() {
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        1,
        vec![Const::Str("std.str.len".into()), Const::Str("".into())],
    );
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(0));
}

#[test]
fn std_str_concat_joins_strings() {
    // Push arg[0] first (bottom), arg[1] second (top) → after pop+reverse
    // args=[arg0, arg1] = ["foo", "bar"], VM does foo.push_str(bar).
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)), // "foo" (arg 0, bottom)
            (Opcode::Const, u32b(2)), // "bar" (arg 1, top)
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        2,
        vec![
            Const::Str("std.str.concat".into()),
            Const::Str("foo".into()),
            Const::Str("bar".into()),
        ],
    );
    assert_eq!(Vm::new(m).run().unwrap(), Value::Str("foobar".into()));
}

#[test]
fn std_str_split_keeps_empty_segments() {
    // split(subject, sep) = split("a,,b", ",") → ["a", "", "b"]
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)), // "a,,b" (subject, arg 0, bottom)
            (Opcode::Const, u32b(2)), // "," (separator, arg 1, top)
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        2,
        vec![
            Const::Str("std.str.split".into()),
            Const::Str("a,,b".into()),
            Const::Str(",".into()),
        ],
    );
    let result = Vm::new(m).run().unwrap();
    assert_eq!(
        result,
        Value::Arr(Box::new([
            Value::Str("a".into()),
            Value::Str("".into()),
            Value::Str("b".into()),
        ]))
    );
}

#[test]
fn std_num_sqrt_of_225_is_1_5() {
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        1,
        vec![Const::Str("std.num.sqrt".into()), Const::Float(2.25)],
    );
    assert_eq!(Vm::new(m).run().unwrap(), Value::Float(1.5));
}

#[test]
fn std_num_floor_drops_fractional_part() {
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        1,
        vec![Const::Str("std.num.floor".into()), Const::Float(3.7)],
    );
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(3));
}

#[test]
fn std_os_args_returns_program_args_as_array() {
    let m = builtin_noarg_module("std.os.args");
    let mut vm = Vm::with_args(m, vec!["alpha".to_string(), "beta".to_string()]);
    assert_eq!(
        vm.run().unwrap(),
        Value::Arr(Box::new([
            Value::Str("alpha".into()),
            Value::Str("beta".into()),
        ]))
    );
}

#[test]
fn std_os_args_empty_when_no_program_args() {
    let m = builtin_noarg_module("std.os.args");
    assert_eq!(Vm::new(m).run().unwrap(), Value::Arr(Box::new([])));
}

#[test]
fn std_time_now_millis_is_positive() {
    let m = builtin_noarg_module("std.time.now_millis");
    let result = Vm::new(m).run().unwrap();
    let Value::Int(t) = result else {
        panic!("expected Int, got {result:?}");
    };
    assert!(t > 0, "now_millis should be positive, got {t}");
}

#[test]
fn std_arr_len_returns_array_length() {
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)), // 10
            (Opcode::Const, u32b(2)), // 20
            (Opcode::Const, u32b(3)), // 30
            (Opcode::ArrNew, u16b(3)),
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        1,
        vec![
            Const::Str("std.arr.len".into()),
            Const::Int(10),
            Const::Int(20),
            Const::Int(30),
        ],
    );
    assert_eq!(Vm::new(m).run().unwrap(), Value::Int(3));
}

#[test]
fn std_arr_get_in_bounds_returns_ok_value() {
    // get([10, 20, 30], 1) → Ok(20)
    // Push array (arg 0, bottom) then index (arg 1, top).
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),  // 10
            (Opcode::Const, u32b(2)),  // 20
            (Opcode::Const, u32b(3)),  // 30
            (Opcode::ArrNew, u16b(3)), // array (arg 0)
            (Opcode::Const, u32b(4)),  // 1 (index, arg 1, top)
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        2,
        vec![
            Const::Str("std.arr.get".into()),
            Const::Int(10),
            Const::Int(20),
            Const::Int(30),
            Const::Int(1),
        ],
    );
    assert_eq!(
        Vm::new(m).run().unwrap(),
        Value::Ok(Box::new(Value::Int(20)))
    );
}

#[test]
fn std_arr_get_out_of_bounds_returns_err_value() {
    // get([10, 20, 30], 9) → Err("index 9 out of range (array length 3)")
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),  // 10
            (Opcode::Const, u32b(2)),  // 20
            (Opcode::Const, u32b(3)),  // 30
            (Opcode::ArrNew, u16b(3)), // array (arg 0)
            (Opcode::Const, u32b(4)),  // 9 (index, arg 1, top)
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        2,
        vec![
            Const::Str("std.arr.get".into()),
            Const::Int(10),
            Const::Int(20),
            Const::Int(30),
            Const::Int(9),
        ],
    );
    let result = Vm::new(m).run().unwrap();
    let Value::Err(e) = result else {
        panic!("expected Err, got {result:?}");
    };
    let Value::Str(s) = *e else {
        panic!("expected Str payload, got {e:?}");
    };
    assert_eq!(&*s, "index 9 out of range (array length 3)");
}

#[test]
fn std_arr_set_out_of_bounds_is_runtime_error() {
    // set([10, 20], 5, 99) → IndexOutOfRange (NOT an Err value).
    // Push array (arg 0), index (arg 1), value (arg 2, top).
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),  // 10
            (Opcode::Const, u32b(2)),  // 20
            (Opcode::ArrNew, u16b(2)), // array (arg 0)
            (Opcode::Const, u32b(4)),  // 5 (index, arg 1)
            (Opcode::Const, u32b(3)),  // 99 (value, arg 2, top)
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        3,
        vec![
            Const::Str("std.arr.set".into()),
            Const::Int(10),
            Const::Int(20),
            Const::Int(99),
            Const::Int(5),
        ],
    );
    let err = Vm::new(m).run().unwrap_err();
    assert_eq!(
        err.kind,
        RuntimeErrorKind::IndexOutOfRange { index: 5, len: 2 }
    );
}

#[test]
fn std_arr_set_in_bounds_returns_new_array() {
    // set([10, 20, 30], 1, 99) → [10, 99, 30]
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),  // 10
            (Opcode::Const, u32b(2)),  // 20
            (Opcode::Const, u32b(3)),  // 30
            (Opcode::ArrNew, u16b(3)), // array (arg 0)
            (Opcode::Const, u32b(5)),  // 1 (index, arg 1)
            (Opcode::Const, u32b(4)),  // 99 (value, arg 2, top)
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        3,
        vec![
            Const::Str("std.arr.set".into()),
            Const::Int(10),
            Const::Int(20),
            Const::Int(30),
            Const::Int(99),
            Const::Int(1),
        ],
    );
    assert_eq!(
        Vm::new(m).run().unwrap(),
        Value::Arr(Box::new([Value::Int(10), Value::Int(99), Value::Int(30),]))
    );
}

#[test]
fn std_arr_push_appends_element() {
    // push([10, 20], 30) → [10, 20, 30]
    // Push array (arg 0) then element (arg 1, top).
    let m = builtin_call_module(
        enc(&[
            (Opcode::Const, u32b(1)),  // 10
            (Opcode::Const, u32b(2)),  // 20
            (Opcode::ArrNew, u16b(2)), // array (arg 0)
            (Opcode::Const, u32b(3)),  // 30 (element, arg 1, top)
            (Opcode::Call, u16b(1)),
            (Opcode::Return, vec![]),
        ]),
        2,
        vec![
            Const::Str("std.arr.push".into()),
            Const::Int(10),
            Const::Int(20),
            Const::Int(30),
        ],
    );
    assert_eq!(
        Vm::new(m).run().unwrap(),
        Value::Arr(Box::new([Value::Int(10), Value::Int(20), Value::Int(30),]))
    );
}

// ---------------------------------------------------------------------------
// 0.0.4 T04 (D1): coroutine frame save/restore.
//
// No opcode drives these yet — `Suspend` lands with T05 — so the tests call
// `suspend_top_frame` / `resume_frame` directly, exactly as the T05 dispatch
// and the T06 executor will.
// ---------------------------------------------------------------------------

/// Byte offset of the instruction after the suspension point in
/// `async_worker_func` — the `Const 1` that follows the first `IAdd`.
const WORKER_RESUME_PC: usize = 9;

/// The suspendable body the coroutine tests snapshot: with `params = 1`,
/// `locals = 2` it computes `arg + 5` (the suspension point), then `+ 1`, then
/// returns.
///
/// ```text
/// 0: LoadLocal 0
/// 3: Const 5
/// 8: IAdd        <- suspend here, resume_pc = 9
/// 9: Const 1
/// 14: IAdd
/// 15: Return
/// ```
fn async_worker_func() -> Func {
    func(
        enc(&[
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::Const, u32b(1)),
            (Opcode::IAdd, vec![]),
            (Opcode::Const, u32b(2)),
            (Opcode::IAdd, vec![]),
            (Opcode::Return, vec![]),
        ]),
        1,
        2,
    )
}

/// `locals = 1`, `params = 0`: loads slot 0 and returns it.  Used to prove a
/// nested `Value::Future` travels through a snapshot untouched.
fn async_host_func() -> Func {
    // T08: the host reads its slot back out with a *move* — a plain load
    // would clone the `Value`, which the nested-future test parks in the
    // slot, and cloning a Future is the backstopped fork path.
    func(
        enc(&[(Opcode::MoveLocal, u16b(0)), (Opcode::Return, vec![])]),
        0,
        1,
    )
}

/// `[0] = unused entry, [1] = worker, [2] = host`.
fn async_fixture() -> Module {
    Module {
        struct_defs: Vec::new(),
        version: 1,
        constants: vec![Const::Str("main".into()), Const::Int(5), Const::Int(1)],
        functions: vec![
            func(
                enc(&[(Opcode::Unit, vec![]), (Opcode::Return, vec![])]),
                0,
                0,
            ),
            async_worker_func(),
            async_host_func(),
        ],
        globals: vec![],
        entry: FuncId(0),
    }
}

/// Run the live frames until the innermost one returns.
fn drive_frames(vm: &mut Vm) -> Result<(), RuntimeErrorKind> {
    while !vm.frames.is_empty() {
        vm.step()?;
    }
    Ok(())
}

#[test]
fn suspend_snapshots_locals_and_operands_and_detaches_frame() {
    let mut vm = Vm::new(async_fixture());
    vm.stack.push(Value::Int(10));
    vm.call(FuncId(1), 1).expect("worker frame");
    vm.step().unwrap(); // LoadLocal 0
    vm.step().unwrap(); // Const 5
    vm.step().unwrap(); // IAdd -> operand 15

    let frame = vm.suspend_top_frame(WORKER_RESUME_PC).unwrap();

    assert_eq!(frame.func, FuncId(1));
    assert_eq!(frame.resume_pc, WORKER_RESUME_PC);
    // The local area (slot 0 = the argument) and the operand suffix are both
    // captured, in their own slots.
    assert_eq!(frame.locals, vec![Value::Int(10), Value::Unit]);
    assert_eq!(frame.operands, vec![Value::Int(15)]);
    assert_eq!(frame.state, super::frame::FutureState::Resumable);
    // Moved, not copied: the VM stack no longer holds the frame's region.
    assert!(vm.stack.is_empty(), "frame region must leave the stack");
    assert!(vm.frames.is_empty());
}

#[test]
fn resume_rebuilds_frame_at_a_different_stack_depth() {
    let mut vm = Vm::new(async_fixture());
    vm.stack.push(Value::Int(10));
    vm.call(FuncId(1), 1).unwrap();
    for _ in 0..3 {
        vm.step().unwrap();
    }
    let frame = vm.suspend_top_frame(WORKER_RESUME_PC).unwrap();

    // Pollute the stack so the resume cannot land at the old base: the frame
    // stores no `stack_base`, so slot addressing must be recomputed.
    vm.stack.push(Value::Str("poller depth".into()));
    vm.resume_frame(frame).unwrap();
    assert_eq!(vm.frames.len(), 1);
    assert_eq!(vm.frames[0].stack_base, 1);
    assert_eq!(vm.frames[0].ip, WORKER_RESUME_PC);

    drive_frames(&mut vm).unwrap();
    // (10 + 5) + 1 — the value computed before the suspend survived it.
    assert_eq!(vm.stack.pop().unwrap(), Value::Int(16));
}

#[test]
fn cold_frame_starts_at_pc_zero_with_padded_locals() {
    let mut vm = Vm::new(async_fixture());
    let frame = super::frame::FutureFrame::new(FuncId(1), vec![Value::Int(2)], 2);
    assert_eq!(frame.resume_pc, 0);
    assert_eq!(frame.locals, vec![Value::Int(2), Value::Unit]);
    assert!(frame.operands.is_empty());

    vm.resume_frame(frame).unwrap();
    drive_frames(&mut vm).unwrap();
    assert_eq!(vm.stack.pop().unwrap(), Value::Int(8));
}

#[test]
fn nested_future_survives_an_outer_snapshot() {
    let inner = super::frame::FutureFrame::new(FuncId(1), vec![Value::Int(3)], 2);
    let mut vm = Vm::new(async_fixture());
    vm.call(FuncId(2), 0).unwrap(); // host frame, locals = [Unit]
    vm.stack[0] = Value::Future(Box::new(inner.clone()));

    let outer = vm.suspend_top_frame(0).unwrap();
    assert_eq!(outer.locals, vec![Value::Future(Box::new(inner.clone()))]);

    // Resume into a fresh VM at a different depth and read the inner future
    // back out through the host body.
    let mut vm2 = Vm::new(async_fixture());
    vm2.stack.push(Value::Int(-1)); // depth offset
    vm2.resume_frame(outer).unwrap();
    drive_frames(&mut vm2).unwrap();
    assert_eq!(
        vm2.stack.pop().unwrap(),
        Value::Future(Box::new(inner)),
        "the inner frame must be restored exactly, not reconstructed"
    );
}

#[test]
fn suspend_without_a_live_frame_is_an_error() {
    let mut vm = Vm::new(async_fixture());
    assert_eq!(
        vm.suspend_top_frame(0).unwrap_err(),
        RuntimeErrorKind::StackUnderflow
    );
}

#[test]
fn resume_validates_the_snapshot_against_the_function() {
    use super::frame::{FutureFrame, FutureState};
    let mut vm = Vm::new(async_fixture());
    let bad_locals = FutureFrame {
        func: FuncId(1),
        resume_pc: 0,
        locals: vec![Value::Unit], // worker has 2 local slots
        operands: vec![],
        state: FutureState::Resumable,
        task_id: 0,
    };
    assert_eq!(
        vm.resume_frame(bad_locals).unwrap_err(),
        RuntimeErrorKind::StackShape
    );

    let bad_func = FutureFrame {
        func: FuncId(99),
        resume_pc: 0,
        locals: vec![],
        operands: vec![],
        state: FutureState::Resumable,
        task_id: 0,
    };
    assert_eq!(
        vm.resume_frame(bad_func).unwrap_err(),
        RuntimeErrorKind::FuncOutOfRange(FuncId(99))
    );
}

#[test]
fn resume_counts_toward_the_call_depth_limit() {
    let mut vm = Vm::new(async_fixture());
    vm.frames = vec![
        super::frame::CallFrame {
            func: FuncId(0),
            ip: 0,
            stack_base: 0,
            is_future: false,
            task_id: 0,
        };
        super::vm::MAX_CALL_DEPTH
    ];
    assert_eq!(
        vm.resume_frame(super::frame::FutureFrame::new(
            FuncId(1),
            vec![Value::Int(1)],
            2
        ))
        .unwrap_err(),
        RuntimeErrorKind::CallDepthExceeded
    );
}

#[test]
fn suspend_rejects_a_frame_region_beyond_the_stack() {
    let mut vm = Vm::new(async_fixture());
    // A frame whose local area (2 slots) is not actually on the stack —
    // hand-written or corrupted bytecode only.
    vm.frames.push(super::frame::CallFrame {
        func: FuncId(1),
        ip: 0,
        stack_base: 0,
        is_future: false,
        task_id: 0,
    });
    assert_eq!(
        vm.suspend_top_frame(0).unwrap_err(),
        RuntimeErrorKind::StackShape
    );
}

#[test]
fn resume_pc_beyond_the_code_is_caught_by_the_dispatch() {
    let mut vm = Vm::new(async_fixture());
    let frame = super::frame::FutureFrame {
        func: FuncId(1),
        resume_pc: 999,
        locals: vec![Value::Unit, Value::Unit],
        operands: vec![],
        state: super::frame::FutureState::Resumable,
        task_id: 0,
    };
    // `resume_frame` does not re-validate the pc (the code length lives with
    // the function, same trust model as a hand-written jump); the existing
    // `ip >= code.len()` guard in `step` is the backstop.
    vm.resume_frame(frame).unwrap();
    assert_eq!(vm.step().unwrap_err(), RuntimeErrorKind::BadJumpTarget);
}

#[test]
fn future_value_is_opaque_and_does_not_grow_value() {
    let v = Value::Future(Box::new(super::frame::FutureFrame::new(
        FuncId(1),
        vec![Value::Int(1)],
        2,
    )));
    assert_eq!(v.to_string(), "<future>");
    // The frame lives behind a Box: the snapshot must not inflate every operand
    // slot, array element and struct field the VM touches.
    assert!(
        std::mem::size_of::<Value>() <= 24,
        "Value grew to {} bytes — FutureFrame must stay boxed",
        std::mem::size_of::<Value>()
    );
}

// ---------------------------------------------------------------------------
// 0.0.4 T05: AsyncStart / Suspend / Poll / Await opcode dispatch (bytecode v4).
//
// `run_src` compiles a `.fln` source through the full pipeline (now emitting
// v4) and executes it as the module's entry — the same path `cargo fln` uses.
// ---------------------------------------------------------------------------

fn run_src(src: &str) -> Result<Value, RuntimeError> {
    let module = fleen_compiler::compile(src).expect("compile async source");
    assert_eq!(module.version, 4, "compiler must emit bytecode v4 now");
    Vm::new(module).run()
}

#[test]
fn async_fn_call_returns_a_future_and_await_unwraps_it() {
    // `gen(1)` is an async call, so it yields `Future<int>`; `await` drives it
    // to completion and leaves the payload `2` on the stack.
    let src = "async func gen(x: int): int = x + 1\nfunc main(): int = gen(1) await\n";
    let v = run_src(src).expect("runs");
    assert_eq!(v, Value::Int(2));
}

#[test]
fn await_chains_nested_async_calls() {
    // `outer` awaits `gen` internally; the module drives `outer` to completion
    // through two resume/Return cycles.
    let src = "\
async func gen(x: int): int = x + 1
async func outer(): int { gen(2) await }
func main(): int = outer() await
";
    let v = run_src(src).expect("runs");
    assert_eq!(v, Value::Int(3));
}

#[test]
fn await_involving_locals_survives_the_drive() {
    // A local bound to the awaited value must be a slot (part of the frame's
    // local area), so it survives being resumed at a different stack depth.
    let src = "\
async func gen(x: int): int = x * 2
func main(): int {
    v = gen(21) await;
    v + 1
}
";
    let v = run_src(src).expect("runs");
    assert_eq!(v, Value::Int(43));
}

#[test]
fn await_on_an_async_fn_that_returns_unit() {
    let src = "\
async func tick(n: int) { print(n as string); }
func main(): int {
    tick(7) await;
    0
}
";
    let v = run_src(src).expect("runs");
    assert_eq!(v, Value::Int(0));
}

#[test]
fn await_emitted_by_lower_produces_v4_and_still_verifies() {
    // The compiler emits version 4 with the Await opcode; the VM must run it,
    // and the whole module must still pass fleen-verify's v4 gate.
    let src = "async func f(): int = 5\nfunc main(): int = f() await\n";
    let module = fleen_compiler::compile(src).expect("compile");
    assert_eq!(module.version, 4);
    fleen_verify::verify(&module).expect("v4 async module verifies");
    assert_eq!(Vm::new(module).run().unwrap(), Value::Int(5));
}

// Hand-written v4 module exercising the raw opcodes directly.
fn async_opcode_module() -> Module {
    // constants: [0]=main, [1]=gen, [2]=Int(1), [3]=Int(40).
    // function 0 = `main` (entry): pushes the arg `40`, calls the async fn
    //   `gen` (func 1), which via its head `AsyncStart` returns a `Future`,
    //   then `Await` drives it to `41`.
    // function 1 = `gen(x)`: `AsyncStart` (packages the frame's param into a
    //   not-yet-started coroutine frame and returns the Future), then the
    //   body `LoadLocal 0, Const Int(1), IAdd → x+1`, Return — the same
    //   single-function layout the real lowerer produces.
    let main = enc(&[
        (Opcode::Const, u32b(3)), // Int(40): the arg to gen
        (Opcode::Call, u16b(1)),  // call async fn -> Future on stack
        (Opcode::Await, vec![]),  // drive -> 41 on stack
        (Opcode::Return, vec![]),
    ]);
    let gen_func = enc(&[
        (Opcode::AsyncStart, vec![]),
        (Opcode::LoadLocal, u16b(0)),
        (Opcode::Const, u32b(2)), // Int(1)
        (Opcode::IAdd, vec![]),
        (Opcode::Return, vec![]),
    ]);
    Module {
        struct_defs: Vec::new(),
        version: 4,
        constants: vec![
            Const::Str("main".into()),
            Const::Str("gen".into()),
            Const::Int(1),
            Const::Int(40),
        ],
        functions: vec![func(main, 0, 0), func(gen_func, 1, 1)],
        globals: vec![],
        entry: FuncId(0),
    }
}

#[test]
fn raw_asyncstart_delivers_a_future_and_await_drives_it() {
    // `AsyncStart` at the head of `gen` packages the frame's param into a
    // not-yet-started coroutine frame and returns the Future; `Await` in
    // `main` resumes it, `gen` runs `40 + 1` → Return delivers `41`.
    let mut vm = Vm::new(async_opcode_module());
    assert_eq!(vm.run().unwrap(), Value::Int(41));
}

#[test]
fn raw_poll_delivers_a_future_and_drives_it() {
    // 0.0.4 T05 `Poll` drives a Future to completion exactly like `Await`
    // (the Ready/Pending protocol is wired by the T06 executor), so swapping
    // the awaited opcode in `main` for `Poll` must yield the same 41. This
    // exercises the `Opcode::Poll` → resume_frame dispatch that lowering does
    // not yet emit.
    let mut module = async_opcode_module();
    module.functions[0].code = enc(&[
        (Opcode::Const, u32b(3)), // Int(40): the arg to gen
        (Opcode::Call, u16b(1)),  // call async fn -> Future on stack
        (Opcode::Poll, vec![]),   // drive -> 41 on stack
        (Opcode::Return, vec![]),
    ]);
    let mut vm = Vm::new(module);
    assert_eq!(vm.run().unwrap(), Value::Int(41));
}

#[test]
fn await_on_a_non_future_is_future_expected() {
    // typeck rejects `5 await`, but hand-written bytecode can still reach the
    // opcode with a non-Future operand; the VM reports it defensively.
    let mut module = async_opcode_module();
    module.functions[0].code = enc(&[
        (Opcode::Const, u32b(2)), // Int(1): not a Future
        (Opcode::Await, vec![]),
        (Opcode::Return, vec![]),
    ]);
    let mut vm = Vm::new(module);
    assert_eq!(vm.run().unwrap_err().kind, RuntimeErrorKind::FutureExpected);
}

#[test]
fn suspend_outside_a_future_context_is_a_defensive_error() {
    // A verified 0.0.4 program only ever reaches `Suspend` from inside an async
    // body (the binary always emits it after a suspend-source builtin there, T06);
    // hand-written bytecode that fires it from a synchronous frame is malformed
    // and reports `StackShape` (no future-marked frame to detach).
    let mut module = async_opcode_module();
    module.functions[0].code = enc(&[(Opcode::Suspend, vec![]), (Opcode::Return, vec![])]);
    let mut vm = Vm::new(module);
    assert_eq!(vm.run().unwrap_err().kind, RuntimeErrorKind::StackShape);
}

// ---------------------------------------------------------------------------
// 0.0.4 T08: D4 wiring — defensive backstops for a misused Future/Channel.
// None of these paths is reachable from compiled source: typeck/ownership
// reject every copy route (`FutureNotClonable` / `FutureNotStorable` /
// `FutureIndexRead`, `ChannelNotClonable` / `ChannelNotStorable`), so these
// hand-written modules exercise the malformed-bytecode defense only.
// ---------------------------------------------------------------------------

#[test]
fn raw_load_local_on_a_future_is_rejected() {
    // A plain load clones the `Value` — for a Future that deep-copies the
    // whole suspended frame (a fork). Ownership rejects every bare read of a
    // future local, so only malformed bytecode reaches this. (A Channel load
    // stays legal: the clone is an Rc bump, the handle-load semantics.)
    let mut module = async_opcode_module();
    module.functions[0] = func(
        enc(&[
            (Opcode::Const, u32b(3)), // Int(40): the arg to gen
            (Opcode::Call, u16b(1)),  // call async fn -> Future on stack
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::LoadLocal, u16b(0)),
            (Opcode::Return, vec![]),
        ]),
        0,
        1,
    );
    let mut vm = Vm::new(module);
    assert_eq!(
        vm.run().unwrap_err().kind,
        RuntimeErrorKind::FutureNotClonable
    );
}

#[test]
fn raw_clone_local_on_a_future_is_rejected() {
    // `clone` lowers to `CloneLocal` for locals; typeck rejects `clone f`, so
    // a Future here is malformed bytecode forking the frame.
    let mut module = async_opcode_module();
    module.functions[0] = func(
        enc(&[
            (Opcode::Const, u32b(3)),
            (Opcode::Call, u16b(1)),
            (Opcode::StoreLocal, u16b(0)),
            (Opcode::CloneLocal, u16b(0)),
            (Opcode::Return, vec![]),
        ]),
        0,
        1,
    );
    let mut vm = Vm::new(module);
    assert_eq!(
        vm.run().unwrap_err().kind,
        RuntimeErrorKind::FutureNotClonable
    );
}

#[test]
fn raw_clone_global_on_a_future_is_rejected() {
    // Globals are read as deep copies (`CloneGlobal`); typeck rejects a
    // Future-typed global (T08), so this is malformed bytecode forking the
    // frame on every read.
    let mut module = async_opcode_module();
    module.globals = vec![fleen_compiler::codegen::Global {
        name: ConstId(0),
        mutable: true,
    }];
    module.functions[0] = func(
        enc(&[
            (Opcode::Const, u32b(3)),
            (Opcode::Call, u16b(1)),
            (Opcode::StoreGlobal, u16b(0)),
            (Opcode::CloneGlobal, u16b(0)),
            (Opcode::Return, vec![]),
        ]),
        0,
        0,
    );
    let mut vm = Vm::new(module);
    assert_eq!(
        vm.run().unwrap_err().kind,
        RuntimeErrorKind::FutureNotClonable
    );
}

#[test]
fn raw_clone_global_on_a_channel_is_rejected() {
    // Same defensive rule for a channel global: the read would hand out an
    // alias without `split`, the one fork point (T07d/T08).
    let mut module = async_opcode_module();
    module.globals = vec![fleen_compiler::codegen::Global {
        name: ConstId(0),
        mutable: true,
    }];
    module.functions[0] = func(
        enc(&[
            (Opcode::Const, u32b(2)), // Int(1): capacity
            (Opcode::ChannelNew, vec![]),
            (Opcode::StoreGlobal, u16b(0)),
            (Opcode::CloneGlobal, u16b(0)),
            (Opcode::Return, vec![]),
        ]),
        0,
        0,
    );
    let mut vm = Vm::new(module);
    assert_eq!(
        vm.run().unwrap_err().kind,
        RuntimeErrorKind::ChannelNotClonable
    );
}

#[test]
fn channel_send_on_a_non_channel_is_channel_expected() {
    // `ChannelSend` with a non-channel under the payload reports
    // `ChannelExpected` (mirroring `FutureExpected`), not a bare StackShape.
    let mut module = async_opcode_module();
    module.functions[0] = func(
        enc(&[
            (Opcode::Const, u32b(2)), // Int(1): the "channel"
            (Opcode::Const, u32b(2)), // Int(1): the payload
            (Opcode::ChannelSend, vec![]),
            (Opcode::Return, vec![]),
        ]),
        0,
        0,
    );
    let mut vm = Vm::new(module);
    assert_eq!(
        vm.run().unwrap_err().kind,
        RuntimeErrorKind::ChannelExpected
    );
}

#[test]
fn channel_send_payload_kind_mismatch_is_rejected() {
    // The channel records the first accepted payload's runtime kind; a later
    // send of a different kind is malformed bytecode (typeck guarantees a
    // uniform static payload type) and fails at the send instead of
    // surfacing delayed at the receiver's next typed instruction.
    let mut module = async_opcode_module();
    module.constants.push(Const::Str("s".into())); // id 4
    module.functions[0] = func(
        enc(&[
            (Opcode::Const, u32b(2)), // Int(1): capacity
            (Opcode::ChannelNew, vec![]),
            (Opcode::Dup, vec![]),         // keep a second handle ([ch, ch])
            (Opcode::Const, u32b(2)),      // Int payload
            (Opcode::ChannelSend, vec![]), // ok: kind fixed to Int ([ch, unit])
            (Opcode::Pop, vec![]),         // drop the unit ([ch])
            (Opcode::Const, u32b(4)),      // Str payload
            (Opcode::ChannelSend, vec![]), // kind Str != Int
            (Opcode::Return, vec![]),
        ]),
        0,
        0,
    );
    let mut vm = Vm::new(module);
    assert_eq!(
        vm.run().unwrap_err().kind,
        RuntimeErrorKind::ChannelPayloadMismatch {
            recorded: PayloadKind::Int,
            found: PayloadKind::Str,
        }
    );
}

#[test]
fn channel_send_accepts_ok_and_err_of_one_result_channel() {
    // `Ok`/`Err` collapse to one `Result` payload kind: a
    // `channel<Result<int, string>>` legitimately carries both constructors.
    let mut module = async_opcode_module();
    module.constants.push(Const::Str("e".into())); // id 4
    module.constants.push(Const::Int(2)); // id 5: capacity
    module.functions[0] = func(
        enc(&[
            (Opcode::Const, u32b(5)), // Int(2): capacity (two payloads)
            (Opcode::ChannelNew, vec![]),
            (Opcode::Dup, vec![]),
            (Opcode::Const, u32b(2)),      // Int(1)
            (Opcode::PackOk, vec![]),      // Ok(1)
            (Opcode::ChannelSend, vec![]), // kind Result ([ch, unit])
            (Opcode::Pop, vec![]),
            (Opcode::Const, u32b(4)),      // Str("e")
            (Opcode::PackErr, vec![]),     // Err("e")
            (Opcode::ChannelSend, vec![]), // Result again — accepted
            (Opcode::Return, vec![]),      // returns the send's unit
        ]),
        0,
        0,
    );
    let mut vm = Vm::new(module);
    assert_eq!(vm.run().unwrap(), Value::Unit);
}

// ---------------------------------------------------------------------------
// 0.0.4 T06: the cooperative executor (F6). `run_src` compiles a `.fln` source
// through the full pipeline; each test drives a small async concurrency program
// and asserts on its printed output / return value. These are deterministic:
// the scheduler round-robins FIFO and `yield_now` re-enqueues, so the
// interleaving is stable across runs.
// ---------------------------------------------------------------------------

#[test]
fn async_run_joins_multiple_roots_to_completion() {
    // `std.async.run` spawns independent roots, runs each to completion (round-
    // robin over its suspension points), then returns `unit` to the caller.
    let v = run_src(
        r#"
import std.io;
import std.async;
async func task(name: string, n: int) {
    for i in range(n) { std.async.yield_now(); }
    std.io.println(name, " done");
}
func main() {
    std.async.run([task("a", 3), task("b", 2)]);
    std.io.println("main after run");
}
"#,
    )
    .expect("executor joins the spawned roots");
    assert_eq!(v, Value::Unit);
}

#[test]
fn top_level_await_block_drives_a_suspending_future() {
    // A bare `main`-level `await` on a root that suspends (here `sleep`) boots
    // a blocking drive that waits on the timer and delivers the payload.
    let v = run_src(
        r#"
import std.io;
import std.async;
async func wait_a_bit(): int { std.io.sleep(5); 42 }
func main(): int { x = wait_a_bit() await; std.io.println("waited: ", x as string); 0 }
"#,
    )
    .expect("blocking top-level await drives the root");
    assert_eq!(v, Value::Int(0));
}

#[test]
fn a_failing_task_re_raises_at_the_join() {
    // A task that panics surfaces as a RuntimeError at the join (`std.async.run`),
    // aborting the whole program — the executor does not swallow it.
    let err = run_src(
        r#"
import std.io;
import std.async;
async func boom() { std.io.println("boom"); n = 1 / 0; std.io.println(n as string); }
func main() { std.async.run([boom()]); std.io.println("unreachable"); }
"#,
    )
    .expect_err("the failing task re-raises at the join");
    assert_eq!(err.kind, RuntimeErrorKind::DivisionByZero);
}

#[test]
fn sync_sleep_and_yield_are_no_ops() {
    // Outside an async body the compiler emits no `Suspend`, so a synchronous
    // caller of `sleep`/`yield_now` simply continues (no scheduler, no park).
    let v = run_src(
        r#"
import std.io;
import std.async;
func main() {
    std.io.sleep(1);
    std.async.yield_now();
    std.io.println("sync ok");
}
"#,
    )
    .expect("sync sleep/yield are no-ops");
    assert_eq!(v, Value::Unit);
}

#[test]
fn a_sync_sleep_does_not_leak_a_deadline_into_a_later_async_suspend() {
    // A synchronous `sleep` is a no-op and must not record a deadline that a
    // later unrelated async suspension would mistake for a timer. Here a huge
    // sync `sleep` is followed by an async task that only `yield_now`s (no
    // sleep): if the stale deadline leaked, the task would be parked on a timer
    // and the program would hang on the join.
    let v = run_src(
        r#"
import std.io;
import std.async;
async func ping() { std.async.yield_now(); }
func main() {
    std.io.sleep(999999);      // sync no-op
    std.async.run([ping()]);   // would hang if deadline leaked
    std.io.println("async ok");
}
"#,
    )
    .expect("the async task is not stuck on a leaked sync sleep deadline");
    assert_eq!(v, Value::Unit);
}

#[test]
fn await_of_async_body_that_calls_run_completes() {
    // Regression (T06 executor): `std.async.run` from inside an async body
    // must not signal the join itself — the compiler emits a `Suspend` right
    // after the call, and only once that Suspend parks the caller may
    // `drive_join` run the spawned tasks. With the join signaled early the
    // tasks completed "into" the still-live caller frame (never counted by
    // the JoinWait), the caller parked forever, and main lost its awaited
    // value (a StackShape at the next store).
    let v = run_src(
        r#"
import std.async;
async func nop() { }
async func orch(): int {
    std.async.run([nop()]);
    5
}
func main(): int {
    v = orch() await;
    v
}
"#,
    )
    .expect("awaiting an async body that runs tasks completes");
    assert_eq!(v, Value::Int(5));
}

#[test]
fn nested_nested_run_releases_every_level_in_order() {
    // Regression (T06 executor): a `std.async.run` inside a task that is
    // itself the join target of an outer `std.async.run` must not overwrite
    // the outer waiter. The old single-slot `waiting_join` dropped the outer
    // waiter's parked chain when the inner run parked, so `top` never resumed
    // and its awaiting site lost its value.
    let v = run_src(
        r#"
import std.async;
async func leaf() { std.async.yield_now(); }
async func mid() { std.async.run([leaf()]); }
async func top(): int {
    std.async.run([mid()]);
    42
}
func main(): int { top() await }
"#,
    )
    .expect("two nested std.async.run levels both release");
    assert_eq!(v, Value::Int(42));
}

#[test]
fn run_with_an_empty_task_list_completes() {
    // Regression (T06 executor): `run([])` in an async body parks the caller
    // with zero tasks to wait for — no completion can ever satisfy a counter
    // of 0, so the old counter-based JoinWait parked the caller forever. An
    // empty pending set re-enqueues the caller immediately.
    let v = run_src(
        r#"
import std.async;
async func worker() { tasks: [Future<unit>] = []; std.async.run(move tasks); }
func main() { std.async.run([worker()]); }
"#,
    )
    .expect("run([]) joins trivially");
    assert_eq!(v, Value::Unit);
}

#[test]
fn sibling_completion_does_not_release_a_nested_join_early() {
    // Regression (T06 executor): the join completion is matched by spawned
    // task id, not counted. Here `a` joins one spawned task `c`, while its
    // sibling `b` completes first; `b`'s completion must not release `a`.
    // `a` resumes only after `c` finished — observable through the e2e
    // fixture `nested_run_sibling` (println ordering); this unit variant
    // asserts the join still delivers.
    let v = run_src(
        r#"
import std.async;
async func yielder() { std.async.yield_now(); }
async func idle() { }
async func joiner() {
    std.async.run([yielder()]);
}
func main(): int { std.async.run([joiner(), idle()]); 9 }
"#,
    )
    .expect("the sibling does not release a's join");
    assert_eq!(v, Value::Int(9));
}

#[test]
fn await_of_async_body_that_recvs_after_run_completes() {
    // Same regression shape with a channel in the loop: the awaited body
    // joins its spawned sender and then receives the buffered value.
    let v = run_src(
        r#"
import std.async;
import std.concurrent;
async func sender(ch: channel<int>) {
    ch.send(99);
}
async func orch(): int {
    ch = channel<int>(1);
    peer = std.concurrent.split(ch);
    std.async.run([sender(move ch)]);
    peer.recv()
}
func main(): int {
    v = orch() await;
    v
}
"#,
    )
    .expect("awaiting an async body that runs a channel task completes");
    assert_eq!(v, Value::Int(99));
}

// ---------------------------------------------------------------------------
// 0.0.4 T07: channels (F9). Source-level tests through the full pipeline —
// the scheduler round-robin and the channel wake paths are deterministic:
// a task always runs to its next suspension point before yielding.
// ---------------------------------------------------------------------------

#[test]
fn channel_buffered_send_then_recv_is_fifo() {
    // Buffered channel: all sends fit, then all recvs come out in send order.
    let v = run_src(
        r#"
import std.io;
import std.async;
import std.collection.range;
import std.concurrent;
async func producer(ch: channel<int>) {
    for i in range(4) { ch.send(i); }
}
async func consumer(ch: channel<int>) {
    for _ in range(4) { std.io.println(ch.recv() as string); }
}
func main() {
    ch = channel<int>(4);
    peer = std.concurrent.split(ch);
    std.async.run([producer(move ch), consumer(move peer)]);
}
"#,
    )
    .expect("buffered channel round-trips");
    assert_eq!(v, Value::Unit);
    // FIFO order is the observable behavior; captured by the e2e fixture
    // `channel_basic` (stdout comparison) — here only the value is checked.
}

#[test]
fn channel_rendezvous_hands_off_between_parked_tasks() {
    // Rendezvous (cap 0): the receiver parks first, the sender's handoff
    // injects the payload into its resumed frame, and the sender continues.
    let v = run_src(
        r#"
import std.io;
import std.async;
import std.collection.range;
import std.concurrent;
async func sender(ch: channel<int>) {
    for i in range(3) { ch.send(i); }
}
async func receiver(ch: channel<int>) {
    for _ in range(3) { std.io.println(ch.recv() as string); }
}
func main() {
    ch = channel<int>(0);
    peer = std.concurrent.split(ch);
    std.async.run([sender(move ch), receiver(move peer)]);
}
"#,
    )
    .expect("rendezvous handoff completes all three sends");
    assert_eq!(v, Value::Unit);
}

#[test]
fn channel_park_does_not_count_as_a_nested_join_completion() {
    // A nested `std.async.run` whose spawned task parks on a channel must not
    // be counted as completed by the join (the `suspended_on_channel` signal):
    // the waiter would otherwise resume before the channel work finishes.
    let v = run_src(
        r#"
import std.io;
import std.async;
import std.collection.range;
import std.concurrent;
async func worker(ch: channel<int>, n: int) {
    ch.send(n);
}
async func feeder(ch: channel<int>) {
    for i in range(2) {
        std.io.println(ch.recv() as string);
    }
}
async func spawner(ch: channel<int>) {
    peer = std.concurrent.split(ch);
    std.async.run([worker(move ch, 7), worker(move peer, 9)]);
    std.io.println("spawner done");
}
func main() {
    ch = channel<int>(2);
    sub = std.concurrent.split(ch);
    std.async.run([spawner(move ch), feeder(move sub)]);
}
"#,
    )
    .expect("nested run with channel parks completes in order");
    assert_eq!(v, Value::Unit);
}

#[test]
fn sync_recv_on_an_empty_channel_is_channel_deadlock() {
    // T07b: in a synchronous main nothing can ever fill the channel, so the
    // blocking recv is the immediate `ChannelDeadlock` (located at the recv).
    let err = run_src(
        r#"
import std.concurrent;
func main() {
    ch = channel<int>(0);
    ch.recv();
}
"#,
    )
    .expect_err("sync recv on an empty channel deadlocks");
    assert_eq!(err.kind, RuntimeErrorKind::ChannelDeadlock);
}

#[test]
fn all_tasks_blocked_on_channels_is_channel_deadlock() {
    // T07b: two tasks, each waiting for the other's channel — no ready task,
    // no timer, so the scheduler detects the deadlock instead of hanging.
    let err = run_src(
        r#"
import std.async;
import std.concurrent;
async func wait_on(ch: channel<int>) {
    ch.recv();
}
func main() {
    ch = channel<int>(0);
    std.async.run([wait_on(move ch)]);
}
"#,
    )
    .expect_err("a lone receiver with no sender deadlocks");
    assert_eq!(err.kind, RuntimeErrorKind::ChannelDeadlock);
}

#[test]
fn channel_send_result_is_unit_and_recv_yields_the_payload() {
    // `send` has type `unit` (its result can be discarded by the statement
    // wrapper — the ChannelSend opcode leaves `Unit` on the stack); `recv`
    // yields the payload into an expression.
    let v = run_src(
        r#"
import std.concurrent;
func main(): int {
    ch = channel<int>(2);
    u = ch.send(41);
    ch.recv() + 1
}
"#,
    )
    .expect("send returns unit, recv returns the payload");
    assert_eq!(v, Value::Int(42));
}

#[test]
fn negative_channel_capacity_is_invalid_channel_capacity() {
    // T07e: the capacity is an arbitrary int expression; a negative one is
    // `InvalidChannelCapacity` at the ChannelNew instruction.
    let err = run_src(
        r#"
import std.concurrent;
func main() {
    ch = channel<int>(0 - 1);
}
"#,
    )
    .expect_err("a negative capacity is rejected at ChannelNew");
    assert_eq!(err.kind, RuntimeErrorKind::InvalidChannelCapacity);
}

#[test]
fn channel_waiters_registry_is_pruned_after_tasks_unpark() {
    // T07: the waiter registry roots channels that hold parked tasks. Once
    // every parked chain has been woken the roots must go — otherwise one
    // `Rc` per channel ever parked on pins its buffer for the program's
    // lifetime (a leak by construction on long runs).
    let src = r#"
import std.async;
import std.collection.range;
import std.concurrent;
async func producer(ch: channel<int>) {
    for i in range(4) { ch.send(i); }
}
async func consumer(ch: channel<int>) {
    for _ in range(4) { ch.recv(); }
}
func main() {
    ch = channel<int>(2);
    peer = std.concurrent.split(ch);
    std.async.run([producer(move ch), consumer(move peer)]);
}
"#;
    let module = fleen_compiler::compile(src).expect("compile channel source");
    let mut vm = Vm::new(module);
    let v = vm.run().expect("producer/consumer completes");
    assert_eq!(v, Value::Unit);
    assert_eq!(
        vm.channel_waiter_count(),
        0,
        "no channel keeps a waiter root after its tasks were woken"
    );
}

#[test]
fn two_parked_senders_wake_in_fifo_arrival_order() {
    // T07 (FIFO): two producers park on a rendezvous channel *before* the
    // feeder runs (round-robin runs p1, then p2, then the feeder) — they must
    // be woken in arrival order, p1's value before p2's in each round. The
    // feeder folds the stream into one number: p1 sends 10, 11 and p2 sends
    // 20, 21, so arrival order 10, 20, 11, 21 gives 10201121; a LIFO sender
    // queue would wake p2 first (20102111). All three tasks live in one
    // `run` (a join over parked senders with no receiver is a real T07b
    // deadlock); the feeder hands the total back through a second channel.
    let v = run_src(
        r#"
import std.async;
import std.collection.range;
import std.concurrent;
async func p1(ch: channel<int>) {
    ch.send(10);
    ch.send(11);
}
async func p2(ch: channel<int>) {
    ch.send(20);
    ch.send(21);
}
async func feeder(ch: channel<int>, out: channel<int>) {
    total = 0;
    for _ in range(4) {
        total = total * 100 + ch.recv();
    }
    out.send(total);
}
func main(): int {
    ch = channel<int>(0);
    a = std.concurrent.split(ch);
    b = std.concurrent.split(ch);
    // Buffered (cap 1): the feeder's result send must fit without the
    // receiver being parked — `run` joins the feeder, so a rendezvous
    // result-send would deadlock against the post-run recv (T07b correctly
    // flags that shape).
    out = channel<int>(1);
    ra = std.concurrent.split(out);
    std.async.run([p1(move ch), p2(move a), feeder(move b, move ra)]);
    out.recv()
}
"#,
    )
    .expect("parked senders wake in FIFO arrival order");
    assert_eq!(v, Value::Int(10201121));
}

// ---------------------------------------------------------------------------
// Scheduler unit tests (0.0.4 T06)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod scheduler_tests {
    use super::super::executor::{Scheduler, Task};
    use super::super::frame::{FutureFrame, FutureState};
    use super::super::value::Value;
    use fleen_compiler::codegen::FuncId;

    fn dummy_frame() -> FutureFrame {
        FutureFrame {
            func: FuncId(0),
            resume_pc: 0,
            locals: vec![Value::Unit],
            operands: vec![],
            state: FutureState::Resumable,
            task_id: 0,
        }
    }

    fn dummy_task() -> Task {
        Task {
            chain: vec![dummy_frame()],
            ready_at_ms: None,
        }
    }

    fn timer_task(ms: i64) -> Task {
        Task {
            chain: vec![dummy_frame()],
            ready_at_ms: Some(ms),
        }
    }

    #[test]
    fn new_scheduler_is_empty() {
        let s = Scheduler::new();
        assert!(s.is_empty());
    }

    #[test]
    fn push_ready_then_next_task_returns_it() {
        let mut s = Scheduler::new();
        s.push_ready(dummy_task());
        assert!(!s.is_empty());
        let t = s.next_task().expect("task available");
        assert!(s.is_empty());
        assert!(t.ready_at_ms.is_none());
    }

    #[test]
    fn ready_tasks_win_over_timers() {
        let mut s = Scheduler::new();
        // Enqueue a timer task far in the future
        s.enqueue(timer_task(1000));
        // Enqueue a ready task
        s.push_ready(dummy_task());
        // Ready task must come first (round-robin)
        let t = s.next_task().expect("ready task first");
        assert!(t.ready_at_ms.is_none());
        // Timer still in scheduler
        assert!(!s.is_empty());
        // Next is the timer (after waiting, but we use a fake now)
        // We can't easily test the sleep, but the ordering is correct.
    }

    #[test]
    fn timer_task_is_promoted_when_due() {
        let mut s = Scheduler::new();
        // Enqueue a timer with deadline 0 (already due)
        s.enqueue(timer_task(0));
        // next_task will promote it and return it
        let t = s.next_task().expect("timer promoted");
        assert!(t.ready_at_ms.is_none());
        assert!(s.is_empty());
    }

    #[test]
    fn ready_before_timer_even_if_timer_due_sooner() {
        let mut s = Scheduler::new();
        // Timer with very early deadline (but in future), ready task added after
        s.enqueue(timer_task(1000));
        s.push_ready(dummy_task());
        // Ready task wins (next_task promotes timers first, then pops ready)
        let t = s.next_task().expect("ready wins");
        assert!(t.ready_at_ms.is_none());
        // Timer still in scheduler
        assert!(!s.is_empty());
    }

    #[test]
    fn enqueue_routes_by_deadline() {
        let mut s = Scheduler::new();
        s.enqueue(timer_task(100));
        s.push_ready(dummy_task());
        // Both should be in scheduler
        assert!(!s.is_empty());
        // Ready first
        let t1 = s.next_task().expect("first");
        assert!(t1.ready_at_ms.is_none());
        // Timer second
        let t2 = s.next_task().expect("second");
        assert!(t2.ready_at_ms.is_none());
        assert!(s.is_empty());
    }
}
