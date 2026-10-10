use crate::verify::VerifyError;
use fleen_compiler::codegen::*;

/// A v2 module (bytecode version 2, as emitted by the 0.0.2 compiler).
fn module(constants: Vec<Const>, functions: Vec<Func>, globals: Vec<Global>, entry: u32) -> Module {
    Module {
        version: 2,
        constants,
        functions,
        globals,
        entry: FuncId(entry),
    }
}

/// A v1 module (bytecode version 1, as emitted by the 0.0.1 compiler):
/// must keep verifying even though v2 opcodes exist.
fn module_v1(
    constants: Vec<Const>,
    functions: Vec<Func>,
    globals: Vec<Global>,
    entry: u32,
) -> Module {
    Module {
        version: 1,
        ..module(constants, functions, globals, entry)
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
const TO_STR: u8 = Opcode::ToStr as u8;
// 0.0.2 U07 opcodes.
const ALLOC_BOX: u8 = Opcode::AllocBox as u8;
const STORE_DEREF_BOX: u8 = Opcode::StoreDerefBox as u8;
const MAKE_REF_LOCAL: u8 = Opcode::MakeRefLocal as u8;
const DUP_DEEP: u8 = Opcode::DupDeep as u8;
const MOVE_LOCAL: u8 = Opcode::MoveLocal as u8;
const CLONE_LOCAL: u8 = Opcode::CloneLocal as u8;
const CLONE_GLOBAL: u8 = Opcode::CloneGlobal as u8;
const PACK_OK: u8 = Opcode::PackOk as u8;
const PACK_ERR: u8 = Opcode::PackErr as u8;
const IS_ERR: u8 = Opcode::IsErr as u8;
const UNWRAP_OK: u8 = Opcode::UnwrapOk as u8;
const UNWRAP_ERR: u8 = Opcode::UnwrapErr as u8;

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

#[test]
fn valid_to_str_cast() {
    // 0.0.2 U13：Const 压标量，ToStr 换成新鲜 string（Δ0，min 1），Return 前深度仍为 1。
    let mut code = vec![CONST_INT];
    code.extend_from_slice(&u32le(1));
    code.push(TO_STR);
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(42)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn to_str_on_empty_stack() {
    // ToStr min_depth_before = 1：空栈上执行须报 StackUnderflow。
    let code = vec![TO_STR, RETURN];
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::StackUnderflow {
            pc: 0,
            needed: 1,
            actual: 0
        })
    );
}

// ---------------------------------------------------------------------------
// 0.0.2 U07: bytecode v2 instruction verification
// ---------------------------------------------------------------------------

#[test]
fn v1_module_with_v1_instructions_still_verifies() {
    // Backwards compatibility: a 0.0.1-shaped module keeps verifying on the
    // 0.0.2 verifier (BYTECODE.md §10).
    let m = module_v1(
        vec![Const::Str("main".into()), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, vec![CONST_INT, 1, 0, 0, 0, RETURN])],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn v2_opcode_in_v1_module_is_rejected() {
    // The version gate is one comparison against V2_OPCODE_BASE: a v2
    // instruction must never sneak into a v1 module (BYTECODE.md §4.2).
    let m = module_v1(
        vec![Const::Str("main".into()), Const::Int(42)],
        vec![func(
            ConstId(0),
            0,
            0,
            vec![CONST_INT, 1, 0, 0, 0, TO_STR, RETURN],
        )],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::V2OpcodeInV1Module {
            pc: 5,
            byte: TO_STR,
        })
    );
}

#[test]
fn valid_box_read_write_sequence() {
    // Const; AllocBox; Const; StoreDerefBox; Unit; Return
    // depths: 1 → 1 (v → b) → 2 → 0 (b v →) → 1; Return needs exactly 1.
    let mut code = vec![CONST_INT];
    code.extend_from_slice(&u32le(1));
    code.push(ALLOC_BOX);
    code.push(CONST_INT);
    code.extend_from_slice(&u32le(2));
    code.push(STORE_DEREF_BOX);
    code.push(UNIT);
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1), Const::Int(2)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn store_deref_box_needs_two_values() {
    // StoreDerefBox min_depth_before = 2: running it with one value on the
    // stack is an underflow (the box plus the value to store).
    let mut code = vec![CONST_INT];
    code.extend_from_slice(&u32le(1));
    code.push(ALLOC_BOX);
    code.push(STORE_DEREF_BOX);
    code.push(UNIT);
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::StackUnderflow {
            pc: 6,
            needed: 2,
            actual: 1
        })
    );
}

#[test]
fn valid_move_and_clone_local() {
    // MoveLocal / CloneLocal are Δ+1 with no minimum: both can run on an
    // empty stack (they read a slot, not the operand stack).
    let code = vec![MOVE_LOCAL, 0, 0, POP, CLONE_LOCAL, 0, 0, RETURN];
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 1, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn slot_operand_out_of_range_for_v2_slot_instructions() {
    // MakeRefLocal / MoveLocal / CloneLocal share LoadLocal's slot rule
    // (BYTECODE.md §8): slot < locals. `locals = 0` here.
    for op in [MAKE_REF_LOCAL, MOVE_LOCAL, CLONE_LOCAL] {
        let m = module(
            vec![Const::Str("main".into())],
            vec![func(ConstId(0), 0, 0, vec![op, 0, 0, RETURN])],
            vec![],
            0,
        );
        assert_eq!(
            crate::verify(&m),
            Err(VerifyError::LocalSlotOutOfRange {
                pc: 0,
                slot: 0,
                locals: 0,
            }),
            "opcode 0x{op:02x} should reject slot 0 with locals = 0"
        );
    }
}

#[test]
fn clone_global_index_out_of_range() {
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, vec![CLONE_GLOBAL, 0, 0, RETURN])],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::GlobalIndexOutOfRange { pc: 0, index: 0 })
    );
}

#[test]
fn valid_clone_global() {
    let mut code = vec![CLONE_GLOBAL];
    code.extend_from_slice(&u16le(0));
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, code)],
        vec![Global {
            name: ConstId(0),
            mutable: true,
        }],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn result_instructions_need_a_value() {
    // PackOk / PackErr / IsErr / UnwrapOk / UnwrapErr are Δ0, min 1.
    for (i, op) in [PACK_OK, PACK_ERR, IS_ERR, UNWRAP_OK, UNWRAP_ERR]
        .into_iter()
        .enumerate()
    {
        let m = module(
            vec![Const::Str("main".into())],
            vec![func(ConstId(0), 0, 0, vec![op, RETURN])],
            vec![],
            0,
        );
        assert_eq!(
            crate::verify(&m),
            Err(VerifyError::StackUnderflow {
                pc: 0,
                needed: 1,
                actual: 0
            }),
            "opcode 0x{op:02x} should require one operand (case {i})"
        );
    }
}

#[test]
fn valid_result_unwrap_sequence() {
    // Const (Ok payload); PackOk; DupDeep; IsErr; JumpIfFalse end;
    // UnwrapOk; Return — mirrors the `?` / Result-choose lowering shape.
    let mut code = vec![CONST_INT];
    code.extend_from_slice(&u32le(1));
    code.push(PACK_OK);
    code.push(DUP_DEEP);
    code.push(IS_ERR);
    // JumpIfFalse over the UnwrapOk (target = pc 11, the `Pop`).
    code.push(JUMP_IF_FALSE);
    code.extend_from_slice(&u16le(11));
    code.push(POP); // pc 11: drop the duplicated Result on the error path
    code.push(UNIT); // pc 12
    code.push(RETURN); // pc 13
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn compiled_ownership_source_verifies() {
    // End-to-end for the verifier: compile a program that exercises every
    // v2 lowering path and check the verifier agrees with Lower's stack
    // assumptions. This is the only test that catches drift between
    // Lower's stack-effect table and `stack_effect` here.
    let src = r#"
g = "global";

func shout(x: ref string): int { 42 }

func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func ratio(a: int): Result<int, string> {
    q = div(a, 2)?;
    Ok(q)
}

func main(): int {
    s = "hello";
    t = move s;
    u = clone t;
    b = box 42;
    n = deref b;
    deref b = 7;
    c = clone b;
    shout(t);
    shout(g);
    r = choose ratio(10) {
        when Ok(v) { v }
        when Err(e) { 0 - 1 }
    };
    print(u);
    print(r as string);
    print(n as string);
    print(deref c as string);
    0
}
"#;
    let module = fleen_compiler::compile(src).expect("compile");
    assert_eq!(module.version, 2);
    assert!(
        crate::verify(&module).is_ok(),
        "{:?}",
        crate::verify(&module)
    );

    // Every non-builtin function must carry a populated span map: that is the
    // data U10 turns into source positions.
    for f in &module.functions {
        if f.is_builtin {
            assert!(f.span_map.is_empty(), "builtin placeholders carry no spans");
            continue;
        }
        assert!(!f.span_map.is_empty(), "span_map must be generated");
        // Entries are ordered by offset and land inside the code.
        let mut prev = 0;
        for e in &f.span_map {
            assert!(e.offset >= prev, "span_map must be ordered");
            assert!(e.start < e.end, "spans must be non-degenerate");
            assert!((e.offset as usize) < f.code.len(), "offset out of range");
            prev = e.offset;
        }
    }
}

// ---------------------------------------------------------------------------
// 0.0.2 U07 review: remaining per-instruction coverage + regression cases
// ---------------------------------------------------------------------------

/// Minimum-encoded shape of a function body: `Const Int(k); Return`.
fn const_int_body(code: &mut Vec<u8>, const_idx: u32) {
    code.push(CONST_INT);
    code.extend_from_slice(&u32le(const_idx));
    code.push(RETURN);
}

#[test]
fn empty_function_body_is_rejected() {
    // Regression: an empty body used to slip past `walk_function` (no
    // instruction to fall off the end of) and then panic inside the stack
    // analysis. It must be a verification failure instead (SPEC §4: no
    // panic on malformed input).
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, vec![])],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::EmptyFunctionBody { func: 0 })
    );
}

#[test]
fn empty_function_body_from_bytes_is_rejected() {
    // Same regression, driven through the `.flnc` decode path: a
    // well-formed file with a zero-length function body must fail
    // verification, not abort. This is the shape of the input that used
    // to crash both the `fleen-verify` CLI and `fleen-vm`.
    let mut b = Vec::new();
    b.extend_from_slice(b"FLNC");
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes()); // constants
    b.extend_from_slice(b"\x02");
    b.extend_from_slice(&4u32.to_le_bytes());
    b.extend_from_slice(b"main");
    b.extend_from_slice(&1u32.to_le_bytes()); // functions
    b.extend_from_slice(&0u32.to_le_bytes()); // name
    b.extend_from_slice(&0u16.to_le_bytes()); // params
    b.extend_from_slice(&0u16.to_le_bytes()); // locals
    b.push(0); // is_builtin
    b.extend_from_slice(&0u32.to_le_bytes()); // code_len = 0
    b.extend_from_slice(&0u32.to_le_bytes()); // span_map_len
    b.extend_from_slice(&0u32.to_le_bytes()); // globals
    b.extend_from_slice(&0u32.to_le_bytes()); // entry
    let m = fleen_compiler::codegen::from_bytes(&b).expect("well-formed bytes");
    assert!(m.functions[0].code.is_empty());
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::EmptyFunctionBody { func: 0 })
    );
}

#[test]
fn alloc_box_replaces_the_top_value_net_zero() {
    // `box e` lowers to `Const; AllocBox` where AllocBox *replaces* the
    // value (net 0, min 1) — not a push (+1). This module is well-formed
    // only under the U07-corrected table: with the old `+1` the Return
    // would see depth 2 and fail.
    let mut code = Vec::new();
    const_int_body(&mut code, 1);
    code.push(ALLOC_BOX);
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn alloc_box_needs_a_value() {
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, vec![ALLOC_BOX, RETURN])],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::StackUnderflow {
            pc: 0,
            needed: 1,
            actual: 0
        })
    );
}

#[test]
fn deref_box_reads_the_pointee_and_keeps_the_box() {
    // `Const; AllocBox; DerefBox` → depths 1 → 1 → 2; drop the copy, the
    // box remains the return value.
    let mut code = Vec::new();
    code.extend_from_slice(&[CONST_INT]);
    code.extend_from_slice(&u32le(1));
    code.push(ALLOC_BOX);
    code.push(Opcode::DerefBox as u8);
    code.push(POP);
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn deref_box_needs_a_box_on_the_stack() {
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, vec![Opcode::DerefBox as u8, RETURN])],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::StackUnderflow {
            pc: 0,
            needed: 1,
            actual: 0
        })
    );
}

#[test]
fn dup_deep_needs_a_value() {
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 0, vec![DUP_DEEP, RETURN])],
        vec![],
        0,
    );
    assert_eq!(
        crate::verify(&m),
        Err(VerifyError::StackUnderflow {
            pc: 0,
            needed: 1,
            actual: 0
        })
    );
}

#[test]
fn valid_make_ref_local_pushes_a_handle() {
    // MakeRefLocal is Δ+1, min 0: it may run on an empty operand stack.
    let mut code = Vec::new();
    code.push(MAKE_REF_LOCAL);
    code.extend_from_slice(&u16le(0));
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 1, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn result_constructors_and_observers_preserve_depth() {
    // Every Result instruction is Δ0, min 1: a depth-1 value survives all
    // of them in a row; the `DupDeep` bumps to 2 and one `Pop` brings it
    // back before `Return`.
    let mut code = Vec::new();
    code.extend_from_slice(&[CONST_INT]);
    code.extend_from_slice(&u32le(1));
    for op in [PACK_OK, DUP_DEEP, IS_ERR, UNWRAP_OK, UNWRAP_ERR, PACK_ERR] {
        code.push(op);
    }
    code.push(POP);
    code.push(RETURN);
    let m = module(
        vec![Const::Str("main".into()), Const::Int(1)],
        vec![func(ConstId(0), 0, 0, code)],
        vec![],
        0,
    );
    assert!(crate::verify(&m).is_ok(), "{:?}", crate::verify(&m));
}

#[test]
fn truncated_v2_operand_is_rejected() {
    // A 3-byte slot instruction cut off after one operand byte is
    // `TruncatedInstruction`, not a mis-decode of the next byte.
    let m = module(
        vec![Const::Str("main".into())],
        vec![func(ConstId(0), 0, 1, vec![MOVE_LOCAL, 0])],
        vec![],
        0,
    );
    assert!(matches!(
        crate::verify(&m),
        Err(VerifyError::TruncatedInstruction { pc: 0 })
    ));
}

#[test]
fn v2_in_v1_module_rejection_covers_every_v2_byte() {
    // The version gate is `opcode >= V2_OPCODE_BASE`; every v2 opcode in a
    // v1 module must trip it (decoding alone cannot catch it, since
    // `from_byte` knows all v2 numbers).
    for op in [
        ALLOC_BOX,
        Opcode::DerefBox as u8,
        STORE_DEREF_BOX,
        MAKE_REF_LOCAL,
        DUP_DEEP,
        MOVE_LOCAL,
        CLONE_LOCAL,
        CLONE_GLOBAL,
        PACK_OK,
        PACK_ERR,
        IS_ERR,
        UNWRAP_OK,
        UNWRAP_ERR,
        TO_STR,
    ] {
        // The v2 byte sits at pc 5, after a well-formed `Const;`. The four
        // slot/global opcodes carry a u16 operand; the jump-checks never
        // run, but a well-formed instruction body keeps the shape honest.
        let mut body = Vec::new();
        body.extend_from_slice(&[CONST_INT]);
        body.extend_from_slice(&u32le(1));
        body.push(op);
        if matches!(op, MAKE_REF_LOCAL | MOVE_LOCAL | CLONE_LOCAL | CLONE_GLOBAL) {
            body.extend_from_slice(&u16le(0));
        }
        body.push(RETURN);
        let m = module_v1(
            vec![Const::Str("main".into()), Const::Int(42)],
            vec![func(ConstId(0), 0, 0, body)],
            vec![],
            0,
        );
        assert_eq!(
            crate::verify(&m),
            Err(VerifyError::V2OpcodeInV1Module { pc: 5, byte: op }),
            "opcode 0x{op:02x} in a v1 module must be rejected"
        );
    }
}
