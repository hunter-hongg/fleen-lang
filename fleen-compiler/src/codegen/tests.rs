use super::*;
use crate::lexer::Span;
use crate::lower::mir::*;
use crate::typeck::typed_hir::Type;

fn instr(kind: MirInstrKind) -> MirInstr {
    MirInstr::new(kind, Span::new(0, 0))
}

fn main_func(blocks: Vec<MirBlock>) -> MirFunc {
    MirFunc {
        func_id: FuncId(0),
        name: "main".to_string(),
        params: 0,
        locals: 1,
        entry: BlockId(0),
        blocks,
        is_builtin: false,
        ret_type: Type::Int,
    }
}

#[test]
fn consts_interned_and_deduplicated() {
    let mut pool = ConstPool::new();
    let a = pool.intern(Const::Int(42));
    let b = pool.intern(Const::Int(42));
    let c = pool.intern(Const::Float(1.5));
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(pool.items.len(), 2);
}

#[test]
fn encode_single_block_function() {
    let f = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![instr(MirInstrKind::ConstInt(42))],
        terminator: Terminator::Return,
    }]);
    let mir = Mir {
        funcs: vec![f],
        globals: vec![],
    };
    let module = codegen(mir).expect("codegen");
    assert_eq!(module.version, 2);
    assert_eq!(module.entry, FuncId(0));
    let code = &module.functions[0].code;
    // Const <id0> ; Return
    assert_eq!(code[0], Opcode::Const as u8);
    // ConstId 0 is the function name "main"; 42 is ConstId 1.
    assert_eq!(&code[1..5], &1u32.to_le_bytes());
    assert_eq!(code[5], Opcode::Return as u8);
    assert!(matches!(module.constants[1], Const::Int(42)));
}

#[test]
fn jump_targets_are_absolute_offsets() {
    // Block0: ConstInt; JumpIfFalse -> block2; block1: Unit; Return; block2: ConstInt; Return
    let f = MirFunc {
        func_id: FuncId(0),
        name: "main".to_string(),
        params: 0,
        locals: 1,
        entry: BlockId(0),
        blocks: vec![
            MirBlock {
                id: BlockId(0),
                instrs: vec![instr(MirInstrKind::True)],
                terminator: Terminator::JumpIfFalse(BlockId(2)),
            },
            MirBlock {
                id: BlockId(1),
                instrs: vec![instr(MirInstrKind::ConstInt(1))],
                terminator: Terminator::Return,
            },
            MirBlock {
                id: BlockId(2),
                instrs: vec![instr(MirInstrKind::ConstInt(2))],
                terminator: Terminator::Return,
            },
        ],
        is_builtin: false,
        ret_type: Type::Int,
    };
    let mir = Mir {
        funcs: vec![f],
        globals: vec![],
    };
    let module = codegen(mir).expect("codegen");
    let code = &module.functions[0].code;
    // Block0: True(1) + JumpIfFalse(3) at offset 0..4
    assert_eq!(code[0], Opcode::True as u8);
    assert_eq!(code[1], Opcode::JumpIfFalse as u8);
    let target = u16::from_le_bytes([code[2], code[3]]);
    // Block1 starts at offset 4: Const(5) + Return(1) = 6 bytes; block2 at 10.
    assert_eq!(target, 10);
    assert_eq!(code[4], Opcode::Const as u8);
    assert_eq!(code[9], Opcode::Return as u8);
    assert_eq!(code[10], Opcode::Const as u8);
}

#[test]
fn entry_is_init_when_globals_present() {
    let main = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![instr(MirInstrKind::ConstInt(7))],
        terminator: Terminator::Return,
    }]);
    let g = MirGlobal {
        global_id: GlobalId(0),
        name: "g".to_string(),
        mutable: false,
        ty: Type::Int,
        init: vec![instr(MirInstrKind::ConstInt(7))],
    };
    let mir = Mir {
        funcs: vec![main],
        globals: vec![g],
    };
    let module = codegen(mir).expect("codegen");
    assert_eq!(module.entry, FuncId(1));
    assert_eq!(module.functions.len(), 2);
    assert!(matches!(
        module.constants[module.functions[1].name.0 as usize],
        Const::Str(ref s) if &**s == "__init__"
    ));
    let init_code = &module.functions[1].code;
    // Const <7> ; StoreGlobal 0 ; Call main ; Return
    assert_eq!(init_code[0], Opcode::Const as u8);
    assert_eq!(init_code[5], Opcode::StoreGlobal as u8);
    // sanity: constant pool contains the global init value 7.
    assert!(module.constants.iter().any(|c| matches!(c, Const::Int(7))));
    assert_eq!(init_code[8], Opcode::Call as u8);
    assert_eq!(init_code[11], Opcode::Return as u8);
}

#[test]
fn missing_main_is_error() {
    let mir = Mir {
        funcs: vec![],
        globals: vec![],
    };
    assert!(codegen(mir).is_err());
}

#[test]
fn builtin_keeps_placeholder_body() {
    let print = MirFunc {
        func_id: FuncId(0),
        name: "print".to_string(),
        params: 1,
        locals: 1,
        entry: BlockId(0),
        blocks: vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr(MirInstrKind::Unit)],
            terminator: Terminator::Return,
        }],
        is_builtin: true,
        ret_type: Type::Unit,
    };
    let main = MirFunc {
        func_id: FuncId(1),
        name: "main".to_string(),
        params: 0,
        locals: 0,
        entry: BlockId(0),
        blocks: vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr(MirInstrKind::Unit)],
            terminator: Terminator::Return,
        }],
        is_builtin: false,
        ret_type: Type::Unit,
    };
    let module = codegen(Mir {
        funcs: vec![print, main],
        globals: vec![],
    })
    .expect("codegen");
    assert!(module.functions[0].is_builtin);
    assert_eq!(
        &*module.functions[0].code,
        &[Opcode::Unit as u8, Opcode::Return as u8]
    );
}

/// Every stack-effect-free instruction maps to the expected opcode byte and
/// fixed operand width. Guards the `mir_opcode` table and `operand_len`.
#[test]
fn all_instructions_encode_with_expected_opcode_and_width() {
    let cases: Vec<(MirInstr, Opcode, usize)> = vec![
        (instr(MirInstrKind::ConstInt(1)), Opcode::Const, 4),
        (instr(MirInstrKind::ConstFloat(1.5)), Opcode::Const, 4),
        (instr(MirInstrKind::ConstStr("s".into())), Opcode::Const, 4),
        (instr(MirInstrKind::True), Opcode::True, 0),
        (instr(MirInstrKind::False), Opcode::False, 0),
        (instr(MirInstrKind::Unit), Opcode::Unit, 0),
        (instr(MirInstrKind::Pop), Opcode::Pop, 0),
        (instr(MirInstrKind::Dup), Opcode::Dup, 0),
        (instr(MirInstrKind::LoadLocal(3)), Opcode::LoadLocal, 2),
        (instr(MirInstrKind::StoreLocal(3)), Opcode::StoreLocal, 2),
        (instr(MirInstrKind::LoadGlobal(2)), Opcode::LoadGlobal, 2),
        (instr(MirInstrKind::StoreGlobal(2)), Opcode::StoreGlobal, 2),
        (instr(MirInstrKind::IAdd), Opcode::IAdd, 0),
        (instr(MirInstrKind::ISub), Opcode::ISub, 0),
        (instr(MirInstrKind::IMul), Opcode::IMul, 0),
        (instr(MirInstrKind::IDiv), Opcode::IDiv, 0),
        (instr(MirInstrKind::IMod), Opcode::IMod, 0),
        (instr(MirInstrKind::FAdd), Opcode::FAdd, 0),
        (instr(MirInstrKind::FSub), Opcode::FSub, 0),
        (instr(MirInstrKind::FMul), Opcode::FMul, 0),
        (instr(MirInstrKind::FDiv), Opcode::FDiv, 0),
        (instr(MirInstrKind::Eq), Opcode::Eq, 0),
        (instr(MirInstrKind::Ne), Opcode::Ne, 0),
        (instr(MirInstrKind::Lt), Opcode::Lt, 0),
        (instr(MirInstrKind::Gt), Opcode::Gt, 0),
        (instr(MirInstrKind::Le), Opcode::Le, 0),
        (instr(MirInstrKind::Ge), Opcode::Ge, 0),
        (instr(MirInstrKind::Not), Opcode::Not, 0),
        (instr(MirInstrKind::NegI), Opcode::NegI, 0),
        (instr(MirInstrKind::NegF), Opcode::NegF, 0),
        (instr(MirInstrKind::Call(FuncId(1))), Opcode::Call, 2),
        (
            instr(MirInstrKind::LoadFunc(FuncId(1))),
            Opcode::LoadFunc,
            2,
        ),
        (instr(MirInstrKind::CallValue(2)), Opcode::CallValue, 1),
        (instr(MirInstrKind::BindMatch(1)), Opcode::BindMatch, 2),
        (instr(MirInstrKind::ToStr), Opcode::ToStr, 0),
        // 0.0.2 U07: v2 instructions.
        (instr(MirInstrKind::AllocBox), Opcode::AllocBox, 0),
        (instr(MirInstrKind::DerefBox), Opcode::DerefBox, 0),
        (instr(MirInstrKind::StoreDerefBox), Opcode::StoreDerefBox, 0),
        (
            instr(MirInstrKind::MakeRefLocal(1)),
            Opcode::MakeRefLocal,
            2,
        ),
        (instr(MirInstrKind::DupDeep), Opcode::DupDeep, 0),
        (instr(MirInstrKind::MoveLocal(1)), Opcode::MoveLocal, 2),
        (instr(MirInstrKind::CloneLocal(1)), Opcode::CloneLocal, 2),
        (instr(MirInstrKind::CloneGlobal(1)), Opcode::CloneGlobal, 2),
        (instr(MirInstrKind::PackOk), Opcode::PackOk, 0),
        (instr(MirInstrKind::PackErr), Opcode::PackErr, 0),
        (instr(MirInstrKind::IsErr), Opcode::IsErr, 0),
        (instr(MirInstrKind::UnwrapOk), Opcode::UnwrapOk, 0),
        (instr(MirInstrKind::UnwrapErr), Opcode::UnwrapErr, 0),
    ];
    for (instr, expected_op, operand_len) in cases {
        let f = main_func(vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr],
            terminator: Terminator::Return,
        }]);
        let module = codegen(Mir {
            funcs: vec![f],
            globals: vec![],
        })
        .expect("codegen");
        let code = &module.functions[0].code;
        assert_eq!(
            code[0], expected_op as u8,
            "wrong opcode for {expected_op:?}"
        );
        assert_eq!(
            code.len(),
            1 + operand_len + 1, // instr + Return
            "wrong operand width for {expected_op:?}"
        );
    }
}

#[test]
fn float_and_str_constants_load() {
    let f = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![
            instr(MirInstrKind::ConstFloat(2.5)),
            instr(MirInstrKind::ConstStr("hello".into())),
            instr(MirInstrKind::Pop),
            instr(MirInstrKind::Pop),
        ],
        terminator: Terminator::Return,
    }]);
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    assert_eq!(module.constants[0], Const::Str("main".into()));
    assert!(
        module
            .constants
            .iter()
            .any(|c| matches!(c, Const::Float(f) if *f == 2.5))
    );
    assert!(
        module
            .constants
            .iter()
            .any(|c| matches!(c, Const::Str(s) if &**s == "hello"))
    );
}

#[test]
fn float_neg_zero_and_zero_are_distinct_constants() {
    let mut pool = ConstPool::new();
    let a = pool.intern(Const::Float(0.0));
    let b = pool.intern(Const::Float(-0.0));
    assert_ne!(a, b);
    // Same bits intern to the same id.
    let c = pool.intern(Const::Float(0.0f64.to_bits() as f64));
    assert_eq!(a, c);
}

#[test]
fn jump_and_jump_if_true_targets_resolve() {
    // block0: Jump -> block2 (skips block1); block1: ConstInt; Return;
    // block2: True; JumpIfTrue -> block0-target… use separate func instead:
    // simpler: block0: True; JumpIfTrue -> block2; block1: Const; Return; block2: Const; Jump -> block1? cycles fine.
    let f = MirFunc {
        func_id: FuncId(0),
        name: "main".to_string(),
        params: 0,
        locals: 1,
        entry: BlockId(0),
        blocks: vec![
            MirBlock {
                id: BlockId(0),
                instrs: vec![instr(MirInstrKind::True)],
                terminator: Terminator::JumpIfTrue(BlockId(2)),
            },
            MirBlock {
                id: BlockId(1),
                instrs: vec![instr(MirInstrKind::ConstInt(1))],
                terminator: Terminator::Return,
            },
            MirBlock {
                id: BlockId(2),
                instrs: vec![instr(MirInstrKind::ConstInt(2))],
                terminator: Terminator::Jump(BlockId(1)),
            },
        ],
        is_builtin: false,
        ret_type: Type::Int,
    };
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    let code = &module.functions[0].code;
    // block0: True(1) JumpIfTrue(3) = 4 bytes at [0..4); block1: Const(5)+Return(1)=6 at [4..10);
    // block2 at [10..): Const(5) at [10..15), Jump(3) at [15..18).
    let jt = u16::from_le_bytes([code[2], code[3]]);
    assert_eq!(jt, 10, "JumpIfTrue target should be block2 start");
    let j = u16::from_le_bytes([code[16], code[17]]);
    assert_eq!(j, 4, "Jump target should be block1 start");
}

#[test]
fn backward_jump_offsets_resolve() {
    // Loop shape: block0: condition; JumpIfFalse -> exit; body; block2: Jump -> block0 (back edge).
    let f = MirFunc {
        func_id: FuncId(0),
        name: "main".to_string(),
        params: 0,
        locals: 1,
        entry: BlockId(0),
        blocks: vec![
            MirBlock {
                id: BlockId(0),
                instrs: vec![instr(MirInstrKind::False)],
                terminator: Terminator::JumpIfFalse(BlockId(2)),
            },
            MirBlock {
                id: BlockId(1),
                instrs: vec![instr(MirInstrKind::ConstInt(9))],
                terminator: Terminator::Jump(BlockId(0)), // back edge
            },
            MirBlock {
                id: BlockId(2),
                instrs: vec![instr(MirInstrKind::ConstInt(0))],
                terminator: Terminator::Return,
            },
        ],
        is_builtin: false,
        ret_type: Type::Int,
    };
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    let code = &module.functions[0].code;
    // block0: False(1)+JumpIfFalse(3)=4 @[0..4); block1: Const(5)+Jump(3)=8 @[4..12);
    // block2: Const(5)+Return(1) @[12..18).
    let jf = u16::from_le_bytes([code[2], code[3]]);
    assert_eq!(jf, 12);
    let back = u16::from_le_bytes([code[4 + 5 + 1], code[4 + 5 + 2]]);
    assert_eq!(back, 0, "back edge must target block0 offset 0");
}

#[test]
fn bad_jump_target_is_error() {
    let f = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![],
        terminator: Terminator::Jump(BlockId(7)),
    }]);
    assert!(
        codegen(Mir {
            funcs: vec![f],
            globals: vec![],
        })
        .is_err()
    );
}

#[test]
fn call_with_overflowing_func_id_is_error() {
    let main = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![instr(MirInstrKind::Call(FuncId(u32::from(u16::MAX) + 1)))],
        terminator: Terminator::Return,
    }]);
    let r = codegen(Mir {
        funcs: vec![main],
        globals: vec![],
    });
    assert!(
        r.is_err(),
        "oversized FuncId must not be silently truncated"
    );
}

#[test]
fn constants_deduped_across_functions() {
    let a = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![instr(MirInstrKind::ConstInt(7))],
        terminator: Terminator::Return,
    }]);
    let b = MirFunc {
        func_id: FuncId(1),
        name: "helper".to_string(),
        params: 0,
        locals: 1,
        entry: BlockId(0),
        blocks: vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr(MirInstrKind::ConstInt(7))],
            terminator: Terminator::Return,
        }],
        is_builtin: false,
        ret_type: Type::Int,
    };
    let module = codegen(Mir {
        funcs: vec![a, b],
        globals: vec![],
    })
    .expect("codegen");
    let sevens: Vec<_> = module
        .constants
        .iter()
        .filter(|c| matches!(c, Const::Int(7)))
        .collect();
    assert_eq!(sevens.len(), 1, "Const::Int(7) should intern once");
}

#[test]
fn init_func_emits_globals_in_order_and_calls_main() {
    let main = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![instr(MirInstrKind::ConstInt(0))],
        terminator: Terminator::Return,
    }]);
    let g1 = MirGlobal {
        global_id: GlobalId(0),
        name: "a".into(),
        mutable: true,
        ty: Type::Int,
        init: vec![instr(MirInstrKind::ConstInt(1))],
    };
    let g2 = MirGlobal {
        global_id: GlobalId(1),
        name: "b".into(),
        mutable: false,
        ty: Type::Int,
        init: vec![instr(MirInstrKind::ConstInt(2))],
    };
    let module = codegen(Mir {
        funcs: vec![main],
        globals: vec![g1, g2],
    })
    .expect("codegen");
    assert_eq!(module.globals.len(), 2);
    assert!(module.globals[0].mutable);
    assert!(!module.globals[1].mutable);
    let init = &module.functions[1];
    assert_eq!(init.params, 0);
    assert_eq!(init.locals, 0);
    assert!(!init.is_builtin);
    let code = &init.code;
    // Const 1 ; StoreGlobal 0 ; Const 2 ; StoreGlobal 1 ; Call main ; Return
    assert_eq!(code[0], Opcode::Const as u8);
    assert_eq!(code[5], Opcode::StoreGlobal as u8);
    let sg0 = u16::from_le_bytes([code[6], code[7]]);
    assert_eq!(sg0, 0);
    assert_eq!(code[8], Opcode::Const as u8);
    assert_eq!(code[13], Opcode::StoreGlobal as u8);
    let sg1 = u16::from_le_bytes([code[14], code[15]]);
    assert_eq!(sg1, 1);
    assert_eq!(code[16], Opcode::Call as u8);
    let call_target = u16::from_le_bytes([code[17], code[18]]);
    assert_eq!(call_target, 0, "__init__ must call main");
    assert_eq!(code[19], Opcode::Return as u8);
}

#[test]
fn empty_globals_gives_empty_global_table_and_main_entry() {
    let main = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![instr(MirInstrKind::Unit)],
        terminator: Terminator::Return,
    }]);
    let module = codegen(Mir {
        funcs: vec![main],
        globals: vec![],
    })
    .expect("codegen");
    assert!(module.globals.is_empty());
    assert_eq!(module.entry, FuncId(0));
    assert_eq!(module.functions.len(), 1);
}

#[test]
fn flnc_round_trip() {
    let module = codegen(Mir {
        funcs: {
            let mut f = main_func(vec![MirBlock {
                id: BlockId(0),
                instrs: vec![
                    instr(MirInstrKind::ConstStr("hi".into())),
                    instr(MirInstrKind::ConstFloat(1.25)),
                    instr(MirInstrKind::Pop),
                    instr(MirInstrKind::Pop),
                ],
                terminator: Terminator::Return,
            }]);
            f.params = 1;
            f.locals = 3;
            vec![f]
        },
        globals: vec![MirGlobal {
            global_id: GlobalId(0),
            name: "g".into(),
            mutable: true,
            ty: Type::Int,
            init: vec![instr(MirInstrKind::ConstInt(7))],
        }],
    })
    .expect("codegen");
    let bytes = crate::codegen::to_bytes(&module);
    let back = crate::codegen::from_bytes(&bytes).expect("round trip");
    assert_eq!(module, back);
}

#[test]
fn flnc_rejects_bad_magic_and_trailing_bytes() {
    let module = codegen(Mir {
        funcs: vec![main_func(vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr(MirInstrKind::Unit)],
            terminator: Terminator::Return,
        }])],
        globals: vec![],
    })
    .expect("codegen");
    let bytes = crate::codegen::to_bytes(&module);

    let mut bad = bytes.clone();
    bad[0] = b'X';
    assert!(crate::codegen::from_bytes(&bad).is_err());

    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(crate::codegen::from_bytes(&trailing).is_err());
}

fn sample_module() -> Bytecode {
    codegen(Mir {
        funcs: vec![main_func(vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr(MirInstrKind::Unit)],
            terminator: Terminator::Return,
        }])],
        globals: vec![],
    })
    .expect("codegen")
}

#[test]
fn flnc_rejects_bad_version() {
    let mut b = crate::codegen::to_bytes(&sample_module());
    b[4] = 99; // version lo byte
    assert!(crate::codegen::from_bytes(&b).is_err());
}

#[test]
fn flnc_accepts_v1_modules() {
    // A 0.0.1-shaped module (version word 1, empty span_map — the layout is
    // identical apart from the version) must still load (BYTECODE.md §9/§10).
    // U11 additionally keeps a real 0.0.1-built `.flnc` fixture.
    let mut v1 = sample_module();
    v1.version = 1;
    for f in &mut v1.functions {
        f.span_map = Box::new([]);
    }
    let back =
        crate::codegen::from_bytes(&crate::codegen::to_bytes(&v1)).expect("v1 module must load");
    assert_eq!(back, v1);
    assert_eq!(back.version, 1);
}

#[test]
fn flnc_rejects_truncated_input() {
    let b = crate::codegen::to_bytes(&sample_module());
    // Every strict prefix must fail (never panic, never Ok).
    for cut in 0..b.len() {
        assert!(
            crate::codegen::from_bytes(&b[..cut]).is_err(),
            "prefix of len {cut} must be rejected"
        );
    }
}

#[test]
fn flnc_rejects_bad_const_tag() {
    // constants_len = 1 right after the 6-byte header, followed by a bogus tag.
    let mut b = Vec::new();
    b.extend_from_slice(b"FLNC");
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes());
    b.push(9); // invalid tag
    assert!(crate::codegen::from_bytes(&b).is_err());
}

#[test]
fn flnc_rejects_out_of_range_indices() {
    let good = crate::codegen::to_bytes(&sample_module());

    // entry index out of range: entry is the last 4 bytes.
    let mut b = good.clone();
    let n = b.len();
    b[n - 4..].copy_from_slice(&99u32.to_le_bytes());
    assert!(crate::codegen::from_bytes(&b).is_err());

    // func name ConstId out of range: skip header, constants_len, one Int const…
    // Simpler: rebuild a hand-rolled module where constants table is empty but
    // the function's name points at ConstId 5.
    let mut b2 = Vec::new();
    b2.extend_from_slice(b"FLNC");
    b2.extend_from_slice(&1u16.to_le_bytes());
    b2.extend_from_slice(&0u32.to_le_bytes()); // constants_len = 0
    b2.extend_from_slice(&1u32.to_le_bytes()); // functions_len = 1
    b2.extend_from_slice(&5u32.to_le_bytes()); // name ConstId = 5 (invalid)
    b2.extend_from_slice(&0u16.to_le_bytes());
    b2.extend_from_slice(&0u16.to_le_bytes());
    b2.push(0);
    b2.extend_from_slice(&0u32.to_le_bytes());
    b2.extend_from_slice(&0u32.to_le_bytes());
    b2.extend_from_slice(&0u32.to_le_bytes()); // globals_len = 0
    b2.extend_from_slice(&0u32.to_le_bytes()); // entry = 0
    assert!(crate::codegen::from_bytes(&b2).is_err());
}

#[test]
fn flnc_rejects_non_bool_flags() {
    // Build a module with is_builtin = 2 in the func table.
    let mut b = Vec::new();
    b.extend_from_slice(b"FLNC");
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes()); // constants_len = 1
    b.push(2);
    b.extend_from_slice(&4u32.to_le_bytes());
    b.extend_from_slice(b"main");
    b.extend_from_slice(&1u32.to_le_bytes()); // functions_len = 1
    b.extend_from_slice(&0u32.to_le_bytes()); // name ConstId = 0
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.push(2); // is_builtin = 2 → must be rejected
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    assert!(crate::codegen::from_bytes(&b).is_err());
}

#[test]
fn flnc_round_trip_builtin_and_empty_module() {
    let print = MirFunc {
        func_id: FuncId(0),
        name: "print".to_string(),
        params: 1,
        locals: 1,
        entry: BlockId(0),
        blocks: vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr(MirInstrKind::Unit)],
            terminator: Terminator::Return,
        }],
        is_builtin: true,
        ret_type: Type::Unit,
    };
    let main = MirFunc {
        func_id: FuncId(1),
        name: "main".to_string(),
        params: 0,
        locals: 0,
        entry: BlockId(0),
        blocks: vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr(MirInstrKind::Unit)],
            terminator: Terminator::Return,
        }],
        is_builtin: false,
        ret_type: Type::Unit,
    };
    let module = codegen(Mir {
        funcs: vec![print, main],
        globals: vec![],
    })
    .expect("codegen");
    let back = crate::codegen::from_bytes(&crate::codegen::to_bytes(&module)).expect("rt");
    assert_eq!(module, back);
    assert!(back.functions[0].is_builtin);
}

#[test]
fn func_params_and_locals_propagate() {
    let f = MirFunc {
        func_id: FuncId(0),
        name: "main".to_string(),
        params: 2,
        locals: 5,
        entry: BlockId(0),
        blocks: vec![MirBlock {
            id: BlockId(0),
            instrs: vec![instr(MirInstrKind::Unit)],
            terminator: Terminator::Return,
        }],
        is_builtin: false,
        ret_type: Type::Unit,
    };
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    assert_eq!(module.functions[0].params, 2);
    assert_eq!(module.functions[0].locals, 5);
    // The single `Unit` instruction carries the test helper's span, so the
    // span map has exactly that one entry (synthetic spans are skipped).
    assert_eq!(module.functions[0].span_map.len(), 1);
}

// ---------------------------------------------------------------------------
// 0.0.2 U07: v2 instruction encoding and span_map generation
// ---------------------------------------------------------------------------

/// A `main` with an explicit `locals` count for slot-operand instructions.
fn main_func_locals(locals: u16, blocks: Vec<MirBlock>) -> MirFunc {
    MirFunc {
        locals,
        ..main_func(blocks)
    }
}

#[test]
fn v2_opcode_bytes_match_the_numbering_plan() {
    // The 0x80–0x83 / 0x90–0x93 / 0xA0–0xA5 ranges from BYTECODE.md §4.2.
    assert_eq!(Opcode::AllocBox as u8, 0x80);
    assert_eq!(Opcode::DerefBox as u8, 0x81);
    assert_eq!(Opcode::StoreDerefBox as u8, 0x82);
    assert_eq!(Opcode::MakeRefLocal as u8, 0x83);
    assert_eq!(Opcode::DupDeep as u8, 0x90);
    assert_eq!(Opcode::MoveLocal as u8, 0x91);
    assert_eq!(Opcode::CloneLocal as u8, 0x92);
    assert_eq!(Opcode::CloneGlobal as u8, 0x93);
    assert_eq!(Opcode::PackOk as u8, 0xA0);
    assert_eq!(Opcode::PackErr as u8, 0xA1);
    assert_eq!(Opcode::IsErr as u8, 0xA2);
    assert_eq!(Opcode::UnwrapOk as u8, 0xA3);
    assert_eq!(Opcode::UnwrapErr as u8, 0xA4);
    assert_eq!(Opcode::ToStr as u8, 0xA5);

    // Round trip through `from_byte` keeps every v2 opcode decodable.
    for op in [
        Opcode::AllocBox,
        Opcode::DerefBox,
        Opcode::StoreDerefBox,
        Opcode::MakeRefLocal,
        Opcode::DupDeep,
        Opcode::MoveLocal,
        Opcode::CloneLocal,
        Opcode::CloneGlobal,
        Opcode::PackOk,
        Opcode::PackErr,
        Opcode::IsErr,
        Opcode::UnwrapOk,
        Opcode::UnwrapErr,
        Opcode::ToStr,
    ] {
        assert_eq!(Opcode::from_byte(op as u8), Some(op));
    }

    // Reserved slots stay unknown.
    assert_eq!(Opcode::from_byte(0x84), None);
    assert_eq!(Opcode::from_byte(0xA6), None);
}

#[test]
fn v2_slot_operands_round_trip_le_u16() {
    let f = main_func_locals(
        3,
        vec![MirBlock {
            id: BlockId(0),
            instrs: vec![
                MirInstr::new(MirInstrKind::CloneGlobal(2), Span::new(0, 1)),
                MirInstr::new(MirInstrKind::MoveLocal(1), Span::new(0, 1)),
                MirInstr::new(MirInstrKind::CloneLocal(0), Span::new(0, 1)),
                MirInstr::new(MirInstrKind::MakeRefLocal(1), Span::new(0, 1)),
            ],
            terminator: Terminator::Return,
        }],
    );
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    let code = &module.functions[0].code;
    assert_eq!(code[0], Opcode::CloneGlobal as u8);
    assert_eq!(&code[1..3], &2u16.to_le_bytes());
    assert_eq!(code[3], Opcode::MoveLocal as u8);
    assert_eq!(&code[4..6], &1u16.to_le_bytes());
    assert_eq!(code[6], Opcode::CloneLocal as u8);
    assert_eq!(&code[7..9], &0u16.to_le_bytes());
    assert_eq!(code[9], Opcode::MakeRefLocal as u8);
    assert_eq!(&code[10..12], &1u16.to_le_bytes());
    assert_eq!(code.len(), 13, "4 x 3-byte instructions + Return");
}

#[test]
fn jump_offsets_account_for_v2_instruction_widths() {
    // block0: MoveLocal 0 (3 B); JumpIfFalse -> block1 (3 B)
    // block1: Return (1 B) → the target must be byte 6, not 5.
    let f = main_func_locals(
        1,
        vec![
            MirBlock {
                id: BlockId(0),
                instrs: vec![MirInstr::new(MirInstrKind::MoveLocal(0), Span::new(0, 1))],
                terminator: Terminator::JumpIfFalse(BlockId(1)),
            },
            MirBlock {
                id: BlockId(1),
                instrs: vec![],
                terminator: Terminator::Return,
            },
        ],
    );
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    let code = &module.functions[0].code;
    assert_eq!(code[0], Opcode::MoveLocal as u8);
    assert_eq!(code[3], Opcode::JumpIfFalse as u8);
    assert_eq!(u16::from_le_bytes([code[4], code[5]]), 6);
    assert_eq!(code[6], Opcode::Return as u8);
}

#[test]
fn span_map_records_instruction_offsets_and_real_spans() {
    // Const 7 (5 B, span [0,7)); MoveLocal 0 (3 B, span [10,20)); Return
    // (terminators carry no span → no entry).
    let f = main_func_locals(
        1,
        vec![MirBlock {
            id: BlockId(0),
            instrs: vec![
                MirInstr::new(MirInstrKind::ConstInt(7), Span::new(0, 7)),
                MirInstr::new(MirInstrKind::MoveLocal(0), Span::new(10, 20)),
            ],
            terminator: Terminator::Return,
        }],
    );
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    let func = &module.functions[0];
    let entries: Vec<(u32, u32, u32)> = func
        .span_map
        .iter()
        .map(|e| (e.offset, e.start, e.end))
        .collect();
    assert_eq!(entries, vec![(0, 0, 7), (5, 10, 20)]);
    // Each offset really is the instruction's opcode byte.
    assert_eq!(func.code[0], Opcode::Const as u8);
    assert_eq!(func.code[5], Opcode::MoveLocal as u8);
    assert_eq!(&func.code[6..8], &0u16.to_le_bytes());
}

#[test]
fn span_map_skips_synthetic_spans() {
    // Only the instruction with a real span is mapped.
    let f = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![
            MirInstr::new(MirInstrKind::Unit, Span::SYNTHETIC),
            MirInstr::new(MirInstrKind::ConstInt(1), Span::new(3, 9)),
            MirInstr::new(MirInstrKind::Pop, Span::SYNTHETIC),
        ],
        terminator: Terminator::Return,
    }]);
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    let func = &module.functions[0];
    let entries: Vec<(u32, u32, u32)> = func
        .span_map
        .iter()
        .map(|e| (e.offset, e.start, e.end))
        .collect();
    // Unit (1 B) occupies offset 0, so the mapped instruction starts at 1.
    assert_eq!(entries, vec![(1, 3, 9)]);
}

#[test]
fn builtin_span_map_stays_empty() {
    // Builtins run host-side; their placeholder body has no source location.
    let print = MirFunc {
        func_id: FuncId(0),
        name: "print".to_string(),
        params: 1,
        locals: 1,
        entry: BlockId(0),
        blocks: vec![MirBlock {
            id: BlockId(0),
            instrs: vec![],
            terminator: Terminator::Return,
        }],
        is_builtin: true,
        ret_type: Type::Unit,
    };
    let main = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![MirInstr::new(MirInstrKind::Unit, Span::new(0, 1))],
        terminator: Terminator::Return,
    }]);
    let module = codegen(Mir {
        funcs: vec![main, print],
        globals: vec![],
    })
    .expect("codegen");
    let print_fn = module
        .functions
        .iter()
        .find(|f| f.is_builtin)
        .expect("builtin function");
    assert!(print_fn.span_map.is_empty());
}

#[test]
fn init_func_span_map_keeps_initializer_spans_only() {
    // A global initialized with `2 + 3` keeps the initializer's spans; the
    // synthesized `StoreGlobal` / `Call main` glue is synthetic and absent.
    let init = vec![
        MirInstr::new(MirInstrKind::ConstInt(2), Span::new(0, 1)),
        MirInstr::new(MirInstrKind::ConstInt(3), Span::new(4, 5)),
        MirInstr::new(MirInstrKind::IAdd, Span::new(0, 5)),
    ];
    let g = MirGlobal {
        global_id: GlobalId(0),
        name: "g".to_string(),
        mutable: true,
        ty: Type::Int,
        init,
    };
    let main = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![MirInstr::new(MirInstrKind::Unit, Span::new(20, 25))],
        terminator: Terminator::Return,
    }]);
    let module = codegen(Mir {
        funcs: vec![main],
        globals: vec![g],
    })
    .expect("codegen");

    let init_fn = &module.functions[module.entry.0 as usize];
    let name = &module.constants[init_fn.name.0 as usize];
    assert!(matches!(name, Const::Str(s) if &**s == "__init__"));
    let entries: Vec<(u32, u32, u32)> = init_fn
        .span_map
        .iter()
        .map(|e| (e.offset, e.start, e.end))
        .collect();
    // Three initializer instructions (5 + 5 + 1 bytes); the StoreGlobal and
    // Call glue are synthetic, and `Return` is a terminator.
    assert_eq!(entries, vec![(0, 0, 1), (5, 4, 5), (10, 0, 5)]);
}

#[test]
fn span_map_survives_flnc_round_trip() {
    let f = main_func(vec![MirBlock {
        id: BlockId(0),
        instrs: vec![MirInstr::new(MirInstrKind::ConstInt(7), Span::new(2, 6))],
        terminator: Terminator::Return,
    }]);
    let module = codegen(Mir {
        funcs: vec![f],
        globals: vec![],
    })
    .expect("codegen");
    assert!(!module.functions[0].span_map.is_empty());
    let back = crate::codegen::from_bytes(&crate::codegen::to_bytes(&module)).expect("rt");
    assert_eq!(module, back);
    assert_eq!(back.functions[0].span_map.len(), 1);
}
