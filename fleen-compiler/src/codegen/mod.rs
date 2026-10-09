//! Codegen stage: MIR → Bytecode.

pub mod bytecode;
pub mod encoder;
pub mod flnc;

#[cfg(test)]
mod tests;

pub use bytecode::{
    Bytecode, Const, ConstId, Func, FuncId, Global, GlobalId, Module, Opcode, SpanEntry,
};
pub use encoder::{CodegenError, ConstPool, encode_func};
pub use flnc::{FlncError, from_bytes, to_bytes};

use crate::lexer::Span;
use crate::lower::mir::{Mir, MirInstr, MirInstrKind};

/// Lower MIR into a bytecode module.
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
        let code: Box<[u8]> = if f.is_builtin {
            // Placeholder body keeps the structure well-formed (BYTECODE.md §2).
            vec![Opcode::Unit as u8, Opcode::Return as u8].into_boxed_slice()
        } else {
            encode_func(f, &mut pool)?
        };
        functions.push(Func {
            name,
            params: f.params,
            locals: f.locals,
            code,
            span_map: Box::new([]),
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
        let dummy_span = Span::new(0, 0);
        let mut instrs: Vec<MirInstr> = Vec::new();
        for g in &globals {
            instrs.extend(g.init.iter().cloned());
            instrs.push(MirInstr::new(
                MirInstrKind::StoreGlobal(g.global_id.0 as u16),
                dummy_span,
            ));
        }
        instrs.push(MirInstr::new(MirInstrKind::Call(main_id), dummy_span));
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
            code,
            span_map: Box::new([]),
            is_builtin: false,
        });
        init_id
    };

    Ok(Module {
        version: 1,
        constants: pool.items,
        functions,
        globals: gmodule,
        entry,
    })
}
