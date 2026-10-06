use crate::verify::VerifyError;
use fleen_compiler::codegen::*;

fn module(constants: Vec<Const>, functions: Vec<Func>, globals: Vec<Global>, entry: u32) -> Module {
    Module {
        version: 1,
        constants,
        functions,
        globals,
        entry: FuncId(entry),
    }
}

fn func(name: ConstId, params: u16, locals: u16, code: Vec<u8>) -> Func {
    Func {
        name,
        params,
        locals,
        code: code.into_boxed_slice(),
        span_map: Box::new([]),
        is_builtin: false,
    }
}

const CONST_INT: u8 = Opcode::Const as u8;
const TRUE: u8 = Opcode::True as u8;
const UNIT: u8 = Opcode::Unit as u8;
const POP: u8 = Opcode::Pop as u8;
const DUP: u8 = Opcode::Dup as u8;
const IADD: u8 = Opcode::IAdd as u8;
const RETURN: u8 = Opcode::Return as u8;
const JUMP: u8 = Opcode::Jump as u8;
const JUMP_IF_FALSE: u8 = Opcode::JumpIfFalse as u8;
const STORE_GLOBAL: u8 = Opcode::StoreGlobal as u8;
const LOAD_LOCAL: u8 = Opcode::LoadLocal as u8;

fn u32le(v: u32) -> [u8; 4] {
    v.to_le_bytes()
}
fn u16le(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

#[test]
fn valid_unit_return() {
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, vec![UNIT, RETURN])],
        vec![],
        0,
    );
    // Unit 使深度 +1，Return 前恰好为 1。
    assert!(crate::verify(&m).is_ok());
}

#[test]
fn valid_iadd() {
    let mut code = vec![CONST_INT];
    code.extend_from_slice(&u32le(1));
    code.push(CONST_INT);
    code.extend_from_slice(&u32le(1));
    code.push(IADD);
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok());
}

#[test]
fn const_index_out_of_range() {
    let mut code = vec![CONST_INT];
    code.extend_from_slice(&u32le(5));
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::ConstIndexOutOfRange { pc: 0, index: 5 })
    );
}

#[test]
fn jump_into_instruction_middle() {
    // pc0: Jump 1 ; pc3: Return —— target 1 在 Const(5B) 内部
    let code = vec![JUMP, 1, 0, RETURN];
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::JumpTargetNotBoundary { pc: 0, target: 1 })
    ));
}

#[test]
fn fall_off_end() {
    let m = module(
        vec![Const::Str("main".into()), Const::Int(0)],
        vec![func(ConstId(0), 0, 0, vec![UNIT])],
        vec![],
        0,
    );
    assert_eq!(crate::verify(&m), Err(VerifyError::FallOffEnd { pc: 0 }));
}

#[test]
fn stack_underflow() {
    let code = vec![IADD, RETURN];
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::StackUnderflow { pc: 0, .. })
    ));
}

#[test]
fn return_with_extra_stack_values() {
    let code = vec![UNIT, UNIT, RETURN];
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::ReturnDepthWrong { pc: 2, depth: 2 })
    ));
}

#[test]
fn stack_depth_mismatch_at_join() {
    // pc0: True        depth 0->1
    // pc1: Dup         1->2
    // pc2: JumpIfFalse 9   2->1: pc5 d=1, pc9 d=1
    // pc5: Dup         1->2
    // pc6: Jump 9      2->2, target pc9 得 2 != 1 → Mismatch
    // pc9: Return      JIF 路径深度 1，先通过 Return；Jump 路径到达时深度不一致
    let code = vec![TRUE, DUP, JUMP_IF_FALSE, 9, 0, DUP, JUMP, 9, 0, RETURN];
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::StackDepthMismatch { pc: 9, .. })
    ));
}

#[test]
fn loop_back_edge_with_consistent_depth() {
    // while 等价结构：header pc5 深度恒为 1。
    // pc0:  Const id1      0->1
    // pc5:  NegI           1->1
    // pc6:  JumpIfFalse 17 1->0: fallthrough pc9 d=0, target pc17 d=0
    // pc9:  Const id1      0->1
    // pc14: Jump 5         1->1  → pc5 前驱深度一致 (1, 1)
    // pc17: Const id1      0->1
    // pc22: Return         depth 1 ✓
    let mut code = vec![CONST_INT];
    code.extend_from_slice(&u32le(1));
    code.push(Opcode::NegI as u8);
    code.push(JUMP_IF_FALSE);
    code.extend_from_slice(&u16le(17));
    code.push(CONST_INT);
    code.extend_from_slice(&u32le(1));
    code.push(JUMP);
    code.extend_from_slice(&u16le(5));
    code.push(CONST_INT);
    code.extend_from_slice(&u32le(1));
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(0)],
        vec![func(ConstId(0), 0, 1, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn store_global_const_rejected() {
    // main 里 StoreGlobal 一个 mutable: false 的全局。
    let code = vec![UNIT, Opcode::StoreGlobal as u8, 0, 0, UNIT, RETURN];
    // 深度: Unit +1, StoreGlobal -1 → 0, Unit +1, Return ✓ 但 StoreGlobal 目标 const。
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, code)],
        vec![Global {
            name: ConstId(0),
            mutable: false,
        }],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::StoreToConstGlobal {
            func: 0,
            global: 0,
            ..
        })
    ));
}

#[test]
fn init_function_may_store_const_globals() {
    // entry = __init__：初始化写 const 全局是允许的。
    let mut init = vec![CONST_INT];
    init.extend_from_slice(&u32le(1));
    init.push(STORE_GLOBAL);
    init.extend_from_slice(&u16le(0));
    init.push(Opcode::Call as u8);
    init.extend_from_slice(&u16le(1));
    init.push(RETURN);
    let m = module(
        vec![
            Const::Str("__init__".into()),
            Const::Str("main".into()),
            Const::Int(0),
        ],
        vec![
            func(ConstId(0), 0, 0, init),
            func(ConstId(1), 0, 0, vec![UNIT, RETURN]),
        ],
        vec![Global {
            name: ConstId(2),
            mutable: false,
        }],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn locals_less_than_params() {
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 2, 1, vec![RETURN])],
        vec![],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::LocalsLessThanParams { func: 0 })
    ));
}

#[test]
fn local_slot_out_of_range() {
    let mut code = vec![Opcode::LoadLocal as u8];
    code.extend_from_slice(&u16le(3));
    code.push(POP);
    code.push(UNIT);
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 1, code)],
        vec![],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::LocalSlotOutOfRange { slot: 3, .. })
    ));
}

#[test]
fn duplicate_constants() {
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, vec![UNIT, RETURN])],
        vec![],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::DuplicateConst { .. })
    ));
}

#[test]
fn call_stack_effect() {
    // main: Call main_like(params=1) → depth 1 → Return。
    let mut caller = vec![CONST_INT];
    caller.extend_from_slice(&u32le(2)); // const arg
    caller.push(Opcode::Call as u8);
    caller.extend_from_slice(&u16le(1));
    caller.push(RETURN);
    let callee = func(ConstId(1), 1, 1, vec![LOAD_LOCAL, 0, 0, POP, UNIT, RETURN]);
    let m = module(
        vec![
            Const::Str("main".into()),
            Const::Str("f".into()),
            Const::Int(2),
        ],
        vec![func(ConstId(0), 0, 0, caller), callee],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}
