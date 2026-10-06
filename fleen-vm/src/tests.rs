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
