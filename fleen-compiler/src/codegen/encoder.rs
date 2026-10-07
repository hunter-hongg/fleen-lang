//! Encode MIR instructions into the byte stream (two-pass label resolution).

use super::bytecode::*;
use crate::lower::mir::{MirFunc, MirInstr, Terminator};

/// Bytecode assembly for one function.
pub struct Encoder {
    code: Vec<u8>,
}

/// Lowering error surfaced by codegen (invariant violations from Lower).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodegenError {
    pub message: String,
}

impl std::fmt::Display for CodegenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "codegen error: {}", self.message)
    }
}

impl std::error::Error for CodegenError {}

impl Default for Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Encoder {
    pub fn new() -> Self {
        Encoder { code: Vec::new() }
    }

    pub fn finish(self) -> Box<[u8]> {
        self.code.into_boxed_slice()
    }

    fn emit_opcode(&mut self, op: Opcode) {
        self.code.push(op as u8);
    }

    fn emit_u8(&mut self, v: u8) {
        self.code.push(v);
    }

    fn emit_u16(&mut self, v: u16) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    fn emit_u32(&mut self, v: u32) {
        self.code.extend_from_slice(&v.to_le_bytes());
    }

    fn emit_instr(&mut self, instr: &MirInstr, pool: &mut ConstPool) -> Result<(), CodegenError> {
        let op = mir_opcode(instr);
        self.emit_opcode(op);
        match instr {
            MirInstr::ConstInt(v) => {
                let id = pool.intern(Const::Int(*v));
                self.emit_u32(id.0);
            }
            MirInstr::ConstFloat(v) => {
                let id = pool.intern(Const::Float(*v));
                self.emit_u32(id.0);
            }
            MirInstr::ConstStr(s) => {
                let id = pool.intern(Const::Str(s.clone().into_boxed_str()));
                self.emit_u32(id.0);
            }
            MirInstr::LoadLocal(slot) => self.emit_u16(*slot),
            MirInstr::StoreLocal(slot) => self.emit_u16(*slot),
            MirInstr::LoadGlobal(gid) => self.emit_u16(*gid),
            MirInstr::StoreGlobal(gid) => self.emit_u16(*gid),
            MirInstr::Call(fid) => self.emit_u16(fid_u16(*fid)?),
            MirInstr::LoadFunc(fid) => self.emit_u16(fid_u16(*fid)?),
            MirInstr::CallValue(argc) => self.emit_u8(*argc),
            MirInstr::BindMatch(slot) => self.emit_u16(*slot),
            _ => {}
        }
        Ok(())
    }
}

/// Fit a function id into the u16 operand slot.
fn fid_u16(fid: crate::lower::mir::FuncId) -> Result<u16, CodegenError> {
    u16::try_from(fid.0).map_err(|_| CodegenError {
        message: format!("function id {} exceeds u16", fid.0),
    })
}

/// Constant pool with dedup (intern returns existing id for equal constants).
#[derive(Default)]
pub struct ConstPool {
    map: std::collections::HashMap<Const, ConstId>,
    pub items: Vec<Const>,
}

impl ConstPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, c: Const) -> ConstId {
        if let Some(&id) = self.map.get(&c) {
            return id;
        }
        let id = ConstId(self.items.len() as u32);
        self.map.insert(c.clone(), id);
        self.items.push(c);
        id
    }
}

/// Single source of truth: MIR instruction → its bytecode opcode.
fn mir_opcode(instr: &MirInstr) -> Opcode {
    match instr {
        MirInstr::ConstInt(..) | MirInstr::ConstFloat(..) | MirInstr::ConstStr(..) => Opcode::Const,
        MirInstr::True => Opcode::True,
        MirInstr::False => Opcode::False,
        MirInstr::Unit => Opcode::Unit,
        MirInstr::Pop => Opcode::Pop,
        MirInstr::Dup => Opcode::Dup,
        MirInstr::LoadLocal(..) => Opcode::LoadLocal,
        MirInstr::StoreLocal(..) => Opcode::StoreLocal,
        MirInstr::LoadGlobal(..) => Opcode::LoadGlobal,
        MirInstr::StoreGlobal(..) => Opcode::StoreGlobal,
        MirInstr::IAdd => Opcode::IAdd,
        MirInstr::ISub => Opcode::ISub,
        MirInstr::IMul => Opcode::IMul,
        MirInstr::IDiv => Opcode::IDiv,
        MirInstr::IMod => Opcode::IMod,
        MirInstr::FAdd => Opcode::FAdd,
        MirInstr::FSub => Opcode::FSub,
        MirInstr::FMul => Opcode::FMul,
        MirInstr::FDiv => Opcode::FDiv,
        MirInstr::Eq => Opcode::Eq,
        MirInstr::Ne => Opcode::Ne,
        MirInstr::Lt => Opcode::Lt,
        MirInstr::Gt => Opcode::Gt,
        MirInstr::Le => Opcode::Le,
        MirInstr::Ge => Opcode::Ge,
        MirInstr::Not => Opcode::Not,
        MirInstr::NegI => Opcode::NegI,
        MirInstr::NegF => Opcode::NegF,
        MirInstr::Call(..) => Opcode::Call,
        MirInstr::LoadFunc(..) => Opcode::LoadFunc,
        MirInstr::CallValue(..) => Opcode::CallValue,
        MirInstr::BindMatch(..) => Opcode::BindMatch,
        MirInstr::ToStr => Opcode::ToStr,
    }
}

/// Opcode size of a MIR instruction (for offset pre-computation).
fn instr_size(instr: &MirInstr) -> usize {
    mir_opcode(instr).instr_len()
}

fn terminator_size(t: Terminator) -> usize {
    match t {
        Terminator::Return => Opcode::Return.instr_len(),
        Terminator::Jump(..) => Opcode::Jump.instr_len(),
        Terminator::JumpIfFalse(..) => Opcode::JumpIfFalse.instr_len(),
        Terminator::JumpIfTrue(..) => Opcode::JumpIfTrue.instr_len(),
    }
}

/// Encode a function body. Blocks are emitted in `func.blocks` order so the
/// untaken edge of a conditional jump falls through to the next block
/// (BYTECODE.md §6.3). Jump targets become absolute byte offsets.
pub fn encode_func(func: &MirFunc, pool: &mut ConstPool) -> Result<Box<[u8]>, CodegenError> {
    // Pass 1: byte offset at which each block starts.
    let mut offsets = Vec::with_capacity(func.blocks.len());
    let mut pos = 0usize;
    for b in &func.blocks {
        offsets.push(pos);
        pos += b.instrs.iter().map(instr_size).sum::<usize>() + terminator_size(b.terminator);
    }
    let block_offset = |id: crate::lower::mir::BlockId| -> Result<u16, CodegenError> {
        offsets
            .get(id.0 as usize)
            .and_then(|&o| u16::try_from(o).ok())
            .ok_or_else(|| CodegenError {
                message: format!("bad jump target {id}"),
            })
    };

    // Pass 2: emit.
    let mut enc = Encoder::new();
    for b in &func.blocks {
        for instr in &b.instrs {
            enc.emit_instr(instr, pool)?;
        }
        match b.terminator {
            Terminator::Return => enc.emit_opcode(Opcode::Return),
            Terminator::Jump(target) => {
                enc.emit_opcode(Opcode::Jump);
                enc.emit_u16(block_offset(target)?);
            }
            Terminator::JumpIfFalse(target) => {
                enc.emit_opcode(Opcode::JumpIfFalse);
                enc.emit_u16(block_offset(target)?);
            }
            Terminator::JumpIfTrue(target) => {
                enc.emit_opcode(Opcode::JumpIfTrue);
                enc.emit_u16(block_offset(target)?);
            }
        }
    }
    Ok(enc.finish())
}
