use crate::verify;
use crate::verify::VerifyError;
use fleen_compiler::codegen::*;

/// A v2 module (bytecode version 2, as emitted by the 0.0.2 compiler).
fn module(constants: Vec<Const>, functions: Vec<Func>, globals: Vec<Global>, entry: u32) -> Module {
    Module {
        struct_defs: Vec::new(),
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
        struct_defs: Vec::new(),
        version: 1,
        ..module(constants, functions, globals, entry)
    }
}

/// A v3 module (bytecode version 3, as emitted by the 0.0.3 compiler):
/// structs + arrays opcodes are valid here.
fn v3_module(
    constants: Vec<Const>,
    functions: Vec<Func>,
    globals: Vec<Global>,
    entry: u32,
) -> Module {
    Module {
        struct_defs: Vec::new(),
        version: 3,
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
// 0.0.3 V03 array opcodes.
const ARR_NEW: u8 = Opcode::ArrNew as u8;
const ARR_LEN: u8 = Opcode::ArrLen as u8;
const ARR_GET: u8 = Opcode::ArrGet as u8;
const ARR_SET_LOCAL: u8 = Opcode::ArrSetLocal as u8;
const ARR_SET_GLOBAL: u8 = Opcode::ArrSetGlobal as u8;

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
    // Unit makes depth +1; exactly 1 before Return.
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
    // pc0: Jump 1 ; pc3: Return — target 1 is inside Const (5 bytes)
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
    // pc6: Jump 9      2->2, target pc9 arrives at 2 != 1 → Mismatch
    // pc9: Return      the JIF path has depth 1 and passes Return first; the Jump path arrives with a mismatched depth
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
    // while-equivalent structure: header pc5 always has depth 1.
    // pc0:  Const id1      0->1
    // pc5:  NegI           1->1
    // pc6:  JumpIfFalse 17 1->0: fallthrough pc9 d=0, target pc17 d=0
    // pc9:  Const id1      0->1
    // pc14: Jump 5         1->1  → pc5 predecessors agree on depth (1, 1)
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
    // main does a StoreGlobal on a global with mutable: false.
    let code = vec![UNIT, Opcode::StoreGlobal as u8, 0, 0, UNIT, RETURN];
    // Depths: Unit +1, StoreGlobal -1 → 0, Unit +1, Return ✓ but the
    // StoreGlobal target is const.
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
    // entry = __init__: writing a const global during initialization is allowed.
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
    // main: Call main_like(params=1) → depth 1 → Return.
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
    // 0.0.2 U13: Const pushes a scalar, ToStr swaps in a fresh string
    // (Δ0, min 1), depth is still 1 before Return.
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
    // ToStr min_depth_before = 1: executing on an empty stack must report
    // StackUnderflow.
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
    assert_eq!(module.version, 4);
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

// ------------------- 0.0.3 V02: structs (bytecode v3) -------------------

use fleen_compiler::codegen::{FieldTypeTag, StructDefMeta, V3_OPCODE_BASE};

/// A v3 module with one struct (two int fields) and a valid
/// `True; True; StructNew(0); GetField(0); Return` sequence in main.
fn v3_struct_module() -> Module {
    Module {
        version: 3,
        constants: vec![Const::Str("main".into())],
        functions: vec![Func {
            name: ConstId(0),
            params: 0,
            locals: 0,
            code: Box::new([
                Opcode::True as u8,
                Opcode::True as u8,
                Opcode::StructNew as u8,
                0,
                0,
                Opcode::GetField as u8,
                0,
                0,
                Opcode::Return as u8,
            ]),
            span_map: Box::new([]),
            is_builtin: false,
        }],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: vec![StructDefMeta {
            field_types: Box::new([FieldTypeTag::Int, FieldTypeTag::Int]),
        }],
    }
}

#[test]
fn v3_struct_instructions_verify() {
    // StructNew(2) pops both field values, so two `True` pushes precede it.
    let m = v3_struct_module();
    assert_eq!(verify::verify(&m), Ok(()));
}

#[test]
fn v3_struct_new_bad_id_rejected() {
    let mut m = v3_struct_module();
    m.functions[0].code[3] = 9; // StructNew(9): struct table has 1 entry
    assert_eq!(
        verify::verify(&m),
        Err(VerifyError::StructIndexOutOfRange { pc: 2, index: 9 })
    );
}

#[test]
fn v3_struct_new_count_cross_check() {
    // StructNew(0) with only ONE value on the stack underflows: the table
    // says the struct has 2 fields (operand-derived, static effect).
    let mut m = v3_struct_module();
    m.functions[0].code = Box::new([
        Opcode::True as u8,
        Opcode::StructNew as u8,
        0,
        0,
        Opcode::GetField as u8,
        0,
        0,
        Opcode::Return as u8,
    ]);
    assert!(matches!(
        verify::verify(&m),
        Err(VerifyError::StackUnderflow { .. })
    ));
}

#[test]
fn v3_opcode_in_v2_module_rejected() {
    // Explicit gate: a 0xB0 opcode in a v2 module is rejected, not "naturally
    // fine" (the 0.0.2 U07 lesson).
    let mut m = v3_struct_module();
    m.version = 2;
    assert_eq!(
        verify::verify(&m),
        Err(VerifyError::V3OpcodeInV1V2Module {
            pc: 2,
            byte: Opcode::StructNew as u8
        })
    );
}

#[test]
fn v3_compiled_struct_program_verifies() {
    // Full-chain smoke (standing policy): the compiler's own output for a
    // struct program passes verify.
    let src = r#"
struct Point { x: int, y: int }
struct Owner { name: string, pos: Point }
func main(): int {
    p = Point { x: 1, y: 2 };
    o = Owner { name: "a", pos: clone p };
    o.name = "b";
    print(o.name);
    0
}
"#;
    let module = fleen_compiler::compile(src).expect("compile");
    assert!(module.struct_defs.len() >= 2);
    assert_eq!(
        verify::verify(&module),
        Ok(()),
        "{:?}",
        verify::verify(&module)
    );
}

#[test]
fn v3_opcode_base_is_struct_group() {
    // Lock the numbering: struct group starts at 0xB0 (BYTECODE v3).
    assert_eq!(V3_OPCODE_BASE, 0xB0);
    assert_eq!(Opcode::StructNew as u8, 0xB0);
    assert_eq!(Opcode::GetField as u8, 0xB1);
    assert_eq!(Opcode::SetFieldLocal as u8, 0xB2);
    assert_eq!(Opcode::SetFieldGlobal as u8, 0xB3);
}

// ------------------- 0.0.3 V03: arrays (bytecode v3) --------------------

/// A small constant pool with int interning. `Const::Int` values must be
/// unique (BYTECODE.md §8 rejects duplicates), so repeated pushes of the
/// same value share one pool entry.
struct ConstPool {
    vec: Vec<Const>,
    ints: std::collections::HashMap<i64, u32>,
}

impl ConstPool {
    fn new() -> Self {
        // Index 0 is always the main name, mirroring the helper builders.
        ConstPool {
            vec: vec![Const::Str("main".into())],
            ints: std::collections::HashMap::new(),
        }
    }
    fn int(&mut self, v: i64) -> u32 {
        if let Some(&i) = self.ints.get(&v) {
            return i;
        }
        let i = self.vec.len() as u32;
        self.vec.push(Const::Int(v));
        self.ints.insert(v, i);
        i
    }
    /// Emit `Const Int(k)` (the operand is a pool index, not the value).
    fn push_int(&mut self, code: &mut Vec<u8>, v: i64) {
        let idx = self.int(v);
        code.push(CONST_INT);
        code.extend_from_slice(&u32le(idx));
    }
    fn build(self) -> Vec<Const> {
        self.vec
    }
}

/// Build a single-function v3 module whose main is the given code.
fn v3_array_module(
    constants: Vec<Const>,
    code: Vec<u8>,
    locals: u16,
    globals: Vec<Global>,
) -> Module {
    v3_module(
        constants,
        vec![func(ConstId(0), 0, locals, code)],
        globals,
        0,
    )
}

#[test]
fn arr_new_depth_matches_operand() {
    // `ArrNew(n)` pops n values and pushes the array (Δ = 1 − n, min n).
    // Verify the operand-derived effect for the boundary cases n = 0/1/3.
    for n in [0u16, 1, 3] {
        let mut pool = ConstPool::new();
        let mut code = Vec::new();
        for k in 0..n {
            pool.push_int(&mut code, k as i64);
        }
        code.push(ARR_NEW);
        code.extend_from_slice(&u16le(n));
        code.push(RETURN);
        let m = v3_array_module(pool.build(), code, 0, vec![]);
        assert!(
            verify::verify(&m).is_ok(),
            "n = {n}: {:?}",
            verify::verify(&m)
        );
    }
}

#[test]
fn arr_len_keeps_array_and_pushes_int() {
    // `[e]; ArrLen` → array + length (Δ+1, min 1): depth 2 before Return,
    // pop one to land at exactly 1.
    let mut pool = ConstPool::new();
    let mut code = Vec::new();
    pool.push_int(&mut code, 7);
    code.push(ARR_NEW);
    code.extend_from_slice(&u16le(1));
    code.push(ARR_LEN);
    code.push(POP);
    code.push(RETURN);
    let m = v3_array_module(pool.build(), code, 0, vec![]);
    assert!(verify::verify(&m).is_ok(), "{:?}", verify::verify(&m));
}

#[test]
fn arr_len_needs_an_array() {
    // ArrLen min_depth_before = 1: empty stack underflows.
    let m = v3_array_module(
        vec![Const::Str("main".into())],
        vec![ARR_LEN, RETURN],
        0,
        vec![],
    );
    assert_eq!(
        verify::verify(&m),
        Err(VerifyError::StackUnderflow {
            pc: 0,
            needed: 1,
            actual: 0
        })
    );
}

#[test]
fn arr_get_pop_form_balances() {
    // `a i → v` (Δ−1, min 2): the index and the array are popped, the
    // element pushed — as-built (V02 GetField erratum shape), not the
    // PLAN peek form.
    let mut pool = ConstPool::new();
    let mut code = Vec::new();
    pool.push_int(&mut code, 7);
    code.push(ARR_NEW);
    code.extend_from_slice(&u16le(1));
    pool.push_int(&mut code, 0); // index
    code.push(ARR_GET);
    code.push(RETURN);
    let m = v3_array_module(pool.build(), code, 0, vec![]);
    assert!(verify::verify(&m).is_ok(), "{:?}", verify::verify(&m));
}

#[test]
fn arr_get_needs_two_values() {
    // ArrGet min_depth_before = 2: a lone array (depth 1) underflows.
    let m = v3_array_module(
        vec![Const::Str("main".into())],
        vec![ARR_NEW, 0, 0, ARR_GET, RETURN],
        0,
        vec![],
    );
    assert_eq!(
        verify::verify(&m),
        Err(VerifyError::StackUnderflow {
            pc: 3,
            needed: 2,
            actual: 1
        })
    );
}

#[test]
fn arr_set_local_pops_two() {
    // `v i →` (Δ−2, min 2): value and index leave the stack. The array lives
    // in the slot, not on the stack.
    let mut pool = ConstPool::new();
    let mut code = Vec::new();
    pool.push_int(&mut code, 7);
    code.push(ARR_NEW);
    code.extend_from_slice(&u16le(1));
    code.push(Opcode::StoreLocal as u8);
    code.extend_from_slice(&u16le(0));
    code.push(Opcode::LoadLocal as u8);
    code.extend_from_slice(&u16le(0));
    pool.push_int(&mut code, 0); // index
    pool.push_int(&mut code, 5); // value
    code.push(ARR_SET_LOCAL);
    code.extend_from_slice(&u16le(0));
    code.push(RETURN);
    let m = v3_array_module(pool.build(), code, 1, vec![]);
    assert!(verify::verify(&m).is_ok(), "{:?}", verify::verify(&m));
}

#[test]
fn arr_set_local_slot_out_of_range() {
    // The slot operand is range-checked against `locals` like the other
    // slot instructions (here `locals = 0`).
    let m = v3_array_module(
        vec![Const::Str("main".into())],
        vec![ARR_SET_LOCAL, 0, 0, RETURN],
        0,
        vec![],
    );
    assert_eq!(
        verify::verify(&m),
        Err(VerifyError::LocalSlotOutOfRange {
            pc: 0,
            slot: 0,
            locals: 0,
        })
    );
}

#[test]
fn arr_set_global_gid_out_of_range() {
    // `ArrSetGlobal` writes an *element* (not the global binding), so it
    // skips the `mutable` gate but still range-checks the gid. No globals
    // are declared here.
    let m = v3_array_module(
        vec![Const::Str("main".into())],
        vec![ARR_SET_GLOBAL, 0, 0, RETURN],
        0,
        vec![],
    );
    assert_eq!(
        verify::verify(&m),
        Err(VerifyError::GlobalIndexOutOfRange { pc: 0, index: 0 })
    );
}

#[test]
fn v3_array_opcode_in_v1v2_module_rejected() {
    // Explicit gate: a 0xB4 opcode in a v2 module is rejected, the same
    // way a struct opcode is.
    let mut m = v3_array_module(
        vec![Const::Str("main".into())],
        vec![ARR_NEW, 0, 0, RETURN],
        0,
        vec![],
    );
    m.version = 2;
    assert_eq!(
        verify::verify(&m),
        Err(VerifyError::V3OpcodeInV1V2Module {
            pc: 0,
            byte: ARR_NEW
        })
    );
}

#[test]
fn v3_opcode_base_extends_to_arrays() {
    // Lock the numbering: the array group follows the struct group at 0xB4.
    assert_eq!(Opcode::ArrNew as u8, 0xB4);
    assert_eq!(Opcode::ArrLen as u8, 0xB5);
    assert_eq!(Opcode::ArrGet as u8, 0xB6);
    assert_eq!(Opcode::ArrSetLocal as u8, 0xB7);
    assert_eq!(Opcode::ArrSetGlobal as u8, 0xB8);
}

// ---------------------------------------------------------------------------
// 0.0.4 T05 async opcodes (bytecode v4, 0xC0–0xC3).
// ---------------------------------------------------------------------------

/// A hand-built v4 module exercising the async opcodes in a depth-balanced
/// way: `main` pushes a Future (via `__await`-style `Await` at min depth 1).
fn v4_async_module() -> Module {
    // constants: [0]=main, [1]=Int(1).
    // `main`: Const Int(1) (a stand-in Future operand) ; Await ; Return.
    // Depth: Const(+1)→1, Await(Δ0,min1)→1, Return(needs 1) — balanced.
    Module {
        struct_defs: Vec::new(),
        version: 4,
        constants: vec![Const::Str("main".into()), Const::Int(1)],
        functions: vec![Func {
            name: ConstId(0),
            params: 0,
            locals: 0,
            code: Box::new([
                Opcode::Const as u8,
                1,
                0,
                0,
                0, // const idx 1 (Int(1))
                Opcode::Await as u8,
                Opcode::Return as u8,
            ]),
            span_map: Box::new([]),
            is_builtin: false,
        }],
        globals: vec![],
        entry: FuncId(0),
    }
}

#[test]
fn v4_async_program_verifies() {
    // A v4 module reaching `Await` at a valid depth passes verify.
    let m = v4_async_module();
    assert_eq!(m.version, 4);
    assert_eq!(verify::verify(&m), Ok(()));
}

#[test]
fn v4_async_opcode_in_v3_module_rejected() {
    // Explicit gate (the 0.0.2 U07 lesson): a 0xCx opcode in a v1–v3 module
    // is rejected — the same explicit comparison as the v2/v3 gates, not a
    // "naturally fine" side effect.
    let mut m = v4_async_module();
    m.version = 3;
    assert_eq!(
        verify::verify(&m),
        Err(VerifyError::V4OpcodeInV1V2V3Module {
            pc: 5,
            byte: Opcode::Await as u8
        })
    );
}

#[test]
fn v4_await_underflow_is_rejected() {
    // `Await` has min depth 1 (it pops a Future). A module that hits it with
    // an empty operand stack fails the depth analysis.
    let mut m = v4_async_module();
    m.functions[0].code = Box::new([Opcode::Await as u8, Opcode::Return as u8]);
    assert!(matches!(
        verify::verify(&m),
        Err(VerifyError::StackUnderflow { .. })
    ));
}

/// A v4 module whose `main` is `Const Int(1) ; <op> ; Return` — used to
/// exercise the async opcodes that are reachable from hand-written bytecode
/// but not from 0.0.4 T05 lowering (`Poll` / `Suspend`).
fn v4_async_single_op(op: Opcode) -> Module {
    Module {
        struct_defs: Vec::new(),
        version: 4,
        constants: vec![Const::Str("main".into()), Const::Int(1)],
        functions: vec![Func {
            name: ConstId(0),
            params: 0,
            locals: 0,
            code: Box::new([
                Opcode::Const as u8,
                1,
                0,
                0,
                0, // const idx 1 (Int(1))
                op as u8,
                Opcode::Return as u8,
            ]),
            span_map: Box::new([]),
            is_builtin: false,
        }],
        globals: vec![],
        entry: FuncId(0),
    }
}

#[test]
fn v4_poll_stack_effect_verifies() {
    // `Poll` pops the Future and pushes the payload (Δ0, min1) like `Await`:
    // Const→1, Poll(0,1)→1, Return→0 — balanced at min depth 1.
    let m = v4_async_single_op(Opcode::Poll);
    assert_eq!(verify::verify(&m), Ok(()));
}

#[test]
fn v4_suspend_stack_effect_verifies() {
    // `Suspend` is a yield boundary: net zero and touches no operand
    // (Δ0, min0). Const→1, Suspend(0,0)→1, Return→0 — balanced.
    let m = v4_async_single_op(Opcode::Suspend);
    assert_eq!(verify::verify(&m), Ok(()));
}

#[test]
fn v4_async_opcode_numbers_are_locked() {
    // 0.0.4 uses the reserved 0xBC–0xFF region; async lands at 0xC0–0xC3.
    assert_eq!(Opcode::AsyncStart as u8, 0xC0);
    assert_eq!(Opcode::Suspend as u8, 0xC1);
    assert_eq!(Opcode::Poll as u8, 0xC2);
    assert_eq!(Opcode::Await as u8, 0xC3);
}

// ---------------------------------------------------------------------------
// 0.0.4 T07: channels (0xC4–0xC7)
// ---------------------------------------------------------------------------

/// A v4 module whose `main` is `<setup> ; <op> ; Return` for the channel
/// opcodes. `setup` is expressed as const pushes (`count` of const idx 1).
fn v4_channel_module(op: Opcode, setup_pushes: usize) -> Module {
    let mut code = Vec::new();
    for _ in 0..setup_pushes {
        code.extend_from_slice(&[
            Opcode::Const as u8,
            1,
            0,
            0,
            0, // const idx 1 (Int(1)) — stand-in operand
        ]);
    }
    code.push(op as u8);
    code.push(Opcode::Return as u8);
    Module {
        struct_defs: Vec::new(),
        version: 4,
        constants: vec![Const::Str("main".into()), Const::Int(1)],
        functions: vec![Func {
            name: ConstId(0),
            params: 0,
            locals: 0,
            code: code.into_boxed_slice(),
            span_map: Box::new([]),
            is_builtin: false,
        }],
        globals: vec![],
        entry: FuncId(0),
    }
}

#[test]
fn v4_channel_opcode_numbers_are_locked() {
    // Channels land directly after the async group, still in the reserved
    // 0xBC–0xFF region (BYTECODE.md §4.2 compat promise).
    assert_eq!(Opcode::ChannelNew as u8, 0xC4);
    assert_eq!(Opcode::ChannelSend as u8, 0xC5);
    assert_eq!(Opcode::ChannelRecv as u8, 0xC6);
    assert_eq!(Opcode::ChannelSplit as u8, 0xC7);
}

#[test]
fn v4_channel_stack_effects_verify() {
    // `ChannelNew`: cap → ch (Δ0, min1): Const→1, New→1, Return→0.
    assert_eq!(
        verify::verify(&v4_channel_module(Opcode::ChannelNew, 1)),
        Ok(())
    );
    // `ChannelSend`: ch v → unit (Δ-1, min2): Const,Const→2, Send→1, Return→0.
    assert_eq!(
        verify::verify(&v4_channel_module(Opcode::ChannelSend, 2)),
        Ok(())
    );
    // `ChannelRecv`: ch → v (Δ0, min1): Const→1, Recv→1, Return→0.
    assert_eq!(
        verify::verify(&v4_channel_module(Opcode::ChannelRecv, 1)),
        Ok(())
    );
    // `ChannelSplit`: ch → ch2 (Δ0, min1): Const→1, Split→1, Return→0.
    assert_eq!(
        verify::verify(&v4_channel_module(Opcode::ChannelSplit, 1)),
        Ok(())
    );
}

#[test]
fn v4_channel_min_depths_are_enforced() {
    // `ChannelSend` needs both the handle and the payload (min 2): one
    // operand is an underflow.
    assert!(matches!(
        verify::verify(&v4_channel_module(Opcode::ChannelSend, 1)),
        Err(VerifyError::StackUnderflow { .. })
    ));
    // `ChannelRecv` needs the handle (min 1): an empty stack underflows.
    assert!(matches!(
        verify::verify(&v4_channel_module(Opcode::ChannelRecv, 0)),
        Err(VerifyError::StackUnderflow { .. })
    ));
}

#[test]
fn v4_channel_opcode_in_v3_module_rejected() {
    // The v4 gate covers the channel group too.
    let mut m = v4_channel_module(Opcode::ChannelNew, 1);
    m.version = 3;
    assert!(matches!(
        verify::verify(&m),
        Err(VerifyError::V4OpcodeInV1V2V3Module { .. })
    ));
}
