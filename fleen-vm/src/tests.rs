//! Unit tests for the VM: hand-written bytecode modules.

use super::diag::line_col;
use super::error::{RuntimeError, RuntimeErrorKind};
use super::value::Value;
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
        version: 3,
        ..v2_module(enc(&[(Opcode::Return, vec![])]))
    };
    assert_eq!(
        Vm::new(m).run(),
        Err(RuntimeError::new(RuntimeErrorKind::UnsupportedVersion(3)))
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
        version: 3,
        ..v2_module(enc(&[(Opcode::Return, vec![])]))
    };
    let err = Vm::new(m).run().unwrap_err();
    assert_eq!(
        err,
        RuntimeError::new(RuntimeErrorKind::UnsupportedVersion(3))
    );
}
