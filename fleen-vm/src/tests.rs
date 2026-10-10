//! Unit tests for the VM: hand-written bytecode modules.

use super::error::RuntimeError;
use super::value::Value;
use super::vm::Vm;
use fleen_compiler::codegen::{Const, ConstId, Func, FuncId, Module, Opcode};

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
    assert_eq!(Vm::new(m).run().unwrap_err(), RuntimeError::DivisionByZero);
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
        Vm::new(m).run().unwrap_err(),
        RuntimeError::ImmutableGlobal(fleen_compiler::codegen::GlobalId(0))
    );
}

// ---------------------------------------------------------------------------
// 0.0.2 U07: v2 opcodes are *encoded* (U07) but not *executed* (U09).
// These tests lock that behavior so the catch-all cannot silently regress.
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
fn v2_opcode_fails_loudly_until_u09() {
    // A v2 module whose body contains only v1 instructions still runs:
    // `run` accepts module versions {1, 2}.
    let m = v2_module(enc(&[(Opcode::Unit, vec![]), (Opcode::Return, vec![])]));
    assert_eq!(Vm::new(m).run(), Ok(Value::Unit));

    // The first v2 opcode encountered must fail with `InvalidOpcode`
    // (not a silent no-op, which would corrupt the depth invariants
    // `fleen-verify` reasons about). U09 replaces these with real dispatch.
    for (op, byte) in [
        (Opcode::AllocBox, 0x80u8),
        (Opcode::DerefBox, 0x81),
        (Opcode::StoreDerefBox, 0x82),
        (Opcode::MakeRefLocal, 0x83),
        (Opcode::DupDeep, 0x90),
        (Opcode::MoveLocal, 0x91),
        (Opcode::CloneLocal, 0x92),
        (Opcode::CloneGlobal, 0x93),
        (Opcode::PackOk, 0xA0),
        (Opcode::PackErr, 0xA1),
        (Opcode::IsErr, 0xA2),
        (Opcode::UnwrapOk, 0xA3),
        (Opcode::UnwrapErr, 0xA4),
    ] {
        // The u16 operand only exists on the four slot/global opcodes;
        // either way the dispatch fails before an operand is read.
        let operand = if matches!(
            op,
            Opcode::MakeRefLocal | Opcode::MoveLocal | Opcode::CloneLocal | Opcode::CloneGlobal
        ) {
            u16b(0)
        } else {
            Vec::new()
        };
        let m = v2_module(enc(&[(op, operand), (Opcode::Return, vec![])]));
        assert_eq!(
            Vm::new(m).run(),
            Err(RuntimeError::InvalidOpcode(byte)),
            "opcode 0x{byte:02x} must fail with InvalidOpcode until U09"
        );
    }

    // `ToStr` is U13 and *is* executed, so it is not part of the
    // fail-loudly set.
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
    assert_eq!(Vm::new(m).run(), Err(RuntimeError::UnsupportedVersion(3)));
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
        RuntimeError::ResultMismatch.to_string(),
        "unwrap opcode applied to mismatched Result variant"
    );
    assert_eq!(
        RuntimeError::BorrowOutOfRange.to_string(),
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
