//! Codegen stage: MIR → Bytecode.

pub mod bytecode;
pub mod encoder;
pub mod flnc;

#[cfg(test)]
mod tests;

pub use bytecode::{
    Bytecode, Const, ConstId, EnumDefMeta, EnumVariantDefMeta, FieldTypeTag, Func, FuncId, Global,
    GlobalId, Module, Opcode, SpanEntry, StructDefMeta, V2_OPCODE_BASE, V3_OPCODE_BASE,
    V4_OPCODE_BASE, V5_OPCODE_BASE, V6_OPCODE_BASE,
};
pub use encoder::{CodegenError, ConstPool, EncodedFunc, encode_func};
pub use flnc::{FlncError, from_bytes, to_bytes};

use crate::lexer::Span;
use crate::lower::mir::{Mir, MirInstr, MirInstrKind};

/// Bytecode version emitted by this compiler.
///
/// Version 1 was 0.0.1; version 2 adds the ownership / Result instructions
/// and a populated `span_map` (BYTECODE.md §10); version 3 (0.0.3 V02) adds
/// the struct instructions 0xB0–0xB3 and the appended `struct_defs` table;
/// version 4 (0.0.4) adds the async/channel instructions 0xC0–0xC7; version
/// 5 (0.0.5) adds the pointer / unsafe instructions 0xC8–0xCA; version 6
/// (0.0.6 T06) adds the enum instructions 0xCB–0xCD and the appended
/// `enum_defs` table; version 7 (0.0.7 T07) adds one `type_param_count:
/// u32` per `enum_defs` entry. No new opcodes, no `FieldTypeTag` change.
/// Readers accept all seven.
pub const MODULE_VERSION: u16 = 7;

/// Lower MIR into a bytecode module.
///
/// The emitted module is bytecode version 7 (BYTECODE.md §10): it carries
/// the struct / async / pointer / enum instructions and the appended
/// `struct_defs` and `enum_defs` tables (the latter with the per-enum
/// `type_param_count` field, 0.0.7 T07/D8).
///
/// # Errors
/// `CodegenError` on missing `main` or malformed MIR (e.g. bad block index).
pub fn codegen(mir: Mir) -> Result<Bytecode, CodegenError> {
    let Mir {
        funcs,
        globals,
        structs,
        enums,
    } = mir;

    let mut pool = ConstPool::new();

    // Function table (order preserved from MIR).
    let main_id = funcs
        .iter()
        .find(|f| f.name == "main")
        .map(|f| f.func_id)
        .ok_or_else(|| CodegenError {
            message: "no `main` function found".to_string(),
        })?;

    let mut functions = Vec::with_capacity(funcs.len() + usize::from(!globals.is_empty()));
    for f in &funcs {
        let name = pool.intern(Const::Str(f.name.clone().into_boxed_str()));
        let encoded: EncodedFunc = if f.is_builtin {
            // Placeholder body keeps the structure well-formed (BYTECODE.md §2).
            // Builtins run host-side, so their placeholder has no source span.
            EncodedFunc {
                code: vec![Opcode::Unit as u8, Opcode::Return as u8].into_boxed_slice(),
                span_map: Box::new([]),
            }
        } else {
            encode_func(f, &mut pool)?
        };
        functions.push(Func {
            name,
            params: f.params,
            locals: f.locals,
            code: encoded.code,
            span_map: encoded.span_map,
            is_builtin: f.is_builtin,
        });
    }

    // Globals table.
    let mut gmodule = Vec::with_capacity(globals.len());
    for g in &globals {
        let name = pool.intern(Const::Str(g.name.clone().into_boxed_str()));
        gmodule.push(Global {
            name,
            mutable: g.mutable,
        });
    }

    // Entry point.
    let entry = if globals.is_empty() {
        main_id
    } else {
        let init_id = FuncId(functions.len() as u32);
        let init_name = pool.intern(Const::Str("__init__".into()));
        // Glue instructions synthesized here (the `StoreGlobal` and the call
        // into `main`) have no source location, so they get the synthetic span
        // and are skipped in `span_map`; the globals' own initializer
        // instructions keep their real spans.
        let glue_span = Span::SYNTHETIC;
        let mut instrs: Vec<MirInstr> = Vec::new();
        for g in &globals {
            instrs.extend(g.init.iter().cloned());
            instrs.push(MirInstr::new(
                MirInstrKind::StoreGlobal(g.global_id.0 as u16),
                glue_span,
            ));
        }
        instrs.push(MirInstr::new(MirInstrKind::Call(main_id), glue_span));
        let init_func = crate::lower::mir::MirFunc {
            func_id: init_id,
            name: "__init__".to_string(),
            params: 0,
            locals: 0,
            entry: crate::lower::mir::BlockId(0),
            blocks: vec![crate::lower::mir::MirBlock {
                id: crate::lower::mir::BlockId(0),
                instrs,
                terminator: crate::lower::mir::Terminator::Return,
            }],
            is_builtin: false,
            ret_type: crate::typeck::typed_hir::Type::Unit,
        };
        let code = encode_func(&init_func, &mut pool)?;
        functions.push(Func {
            name: init_name,
            params: 0,
            locals: 0,
            code: code.code,
            span_map: code.span_map,
            is_builtin: false,
        });
        init_id
    };

    // Struct table (0.0.3 V02): shape-level field tags per declaration.
    let struct_defs = structs
        .iter()
        .map(|def| StructDefMeta {
            field_types: def.field_types.iter().map(FieldTypeTag::of).collect(),
        })
        .collect();

    // Enum table (0.0.6 T06): shape-level payload tags per declaration.
    // 0.0.7 T07/D8: each entry also carries the declared type-parameter
    // arity (rides through MIR; not derivable from the payloads).
    let enum_defs = enums
        .iter()
        .map(|def| EnumDefMeta {
            variants: def
                .variants
                .iter()
                .map(|v| EnumVariantDefMeta {
                    payload_types: v.payload_types.iter().map(FieldTypeTag::of).collect(),
                })
                .collect(),
            type_param_count: def.type_param_count,
        })
        .collect();

    Ok(Module {
        version: MODULE_VERSION,
        constants: pool.items,
        functions,
        globals: gmodule,
        entry,
        struct_defs,
        enum_defs,
    })
}
