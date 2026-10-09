//! Codegen stage: MIR → Bytecode.

pub mod bytecode;
pub mod encoder;
pub mod flnc;

#[cfg(test)]
mod tests;

pub use bytecode::{
    Bytecode, Const, ConstId, Func, FuncId, Global, GlobalId, Module, Opcode, SpanEntry,
    V2_OPCODE_BASE,
};
pub use encoder::{CodegenError, ConstPool, EncodedFunc, encode_func};
pub use flnc::{FlncError, from_bytes, to_bytes};

use crate::lexer::Span;
use crate::lower::mir::{Mir, MirInstr, MirInstrKind};

/// Bytecode version emitted by this compiler.
///
/// Version 1 was 0.0.1; version 2 adds the ownership / Result instructions
/// and a populated `span_map` (BYTECODE.md §10). Readers accept both.
pub const MODULE_VERSION: u16 = 2;

/// Lower MIR into a bytecode module.
///
/// Since 0.0.2 (U07) the emitted module is bytecode version 2: it carries the
/// ownership / Result instructions and a populated `span_map`.
///
/// # Errors
/// `CodegenError` on missing `main` or malformed MIR (e.g. bad block index).
pub fn codegen(mir: Mir) -> Result<Bytecode, CodegenError> {
    let Mir { funcs, globals } = mir;

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

    Ok(Module {
        version: MODULE_VERSION,
        constants: pool.items,
        functions,
        globals: gmodule,
        entry,
    })
}
