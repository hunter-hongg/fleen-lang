//! Encode MIR instructions into the byte stream (two-pass label resolution).

use super::bytecode::*;
use crate::lower::mir::{MirFunc, MirInstr, MirInstrKind, Terminator};

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
        let op = mir_opcode(&instr.kind)?;
        self.emit_opcode(op);
        match &instr.kind {
            MirInstrKind::ConstInt(v) => {
                let id = pool.intern(Const::Int(*v));
                self.emit_u32(id.0);
            }
            MirInstrKind::ConstFloat(v) => {
                let id = pool.intern(Const::Float(*v));
                self.emit_u32(id.0);
            }
            MirInstrKind::ConstStr(s) => {
                let id = pool.intern(Const::Str(s.clone().into_boxed_str()));
                self.emit_u32(id.0);
            }
            MirInstrKind::LoadLocal(slot) => self.emit_u16(*slot),
            MirInstrKind::StoreLocal(slot) => self.emit_u16(*slot),
            MirInstrKind::LoadGlobal(gid) => self.emit_u16(*gid),
            MirInstrKind::StoreGlobal(gid) => self.emit_u16(*gid),
            MirInstrKind::Call(fid) => self.emit_u16(fid_u16(*fid)?),
            MirInstrKind::LoadFunc(fid) => self.emit_u16(fid_u16(*fid)?),
            MirInstrKind::CallValue(argc) => self.emit_u8(*argc),
            MirInstrKind::BindMatch(slot) => self.emit_u16(*slot),
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
///
/// Returns `Err` for the 0.0.2 (U06) instructions that belong to the v2
/// bytecode; those opcodes (0x80–0xA4) are owned by U07 and are not encodable
/// in the v1 module yet.
fn mir_opcode(kind: &MirInstrKind) -> Result<Opcode, CodegenError> {
    Ok(match kind {
        MirInstrKind::ConstInt(..) | MirInstrKind::ConstFloat(..) | MirInstrKind::ConstStr(..) => {
            Opcode::Const
        }
        MirInstrKind::True => Opcode::True,
        MirInstrKind::False => Opcode::False,
        MirInstrKind::Unit => Opcode::Unit,
        MirInstrKind::Pop => Opcode::Pop,
        MirInstrKind::Dup => Opcode::Dup,
        MirInstrKind::LoadLocal(..) => Opcode::LoadLocal,
        MirInstrKind::StoreLocal(..) => Opcode::StoreLocal,
        MirInstrKind::LoadGlobal(..) => Opcode::LoadGlobal,
        MirInstrKind::StoreGlobal(..) => Opcode::StoreGlobal,
        MirInstrKind::IAdd => Opcode::IAdd,
        MirInstrKind::ISub => Opcode::ISub,
        MirInstrKind::IMul => Opcode::IMul,
        MirInstrKind::IDiv => Opcode::IDiv,
        MirInstrKind::IMod => Opcode::IMod,
        MirInstrKind::FAdd => Opcode::FAdd,
        MirInstrKind::FSub => Opcode::FSub,
        MirInstrKind::FMul => Opcode::FMul,
        MirInstrKind::FDiv => Opcode::FDiv,
        MirInstrKind::Eq => Opcode::Eq,
        MirInstrKind::Ne => Opcode::Ne,
        MirInstrKind::Lt => Opcode::Lt,
        MirInstrKind::Gt => Opcode::Gt,
        MirInstrKind::Le => Opcode::Le,
        MirInstrKind::Ge => Opcode::Ge,
        MirInstrKind::Not => Opcode::Not,
        MirInstrKind::NegI => Opcode::NegI,
        MirInstrKind::NegF => Opcode::NegF,
        MirInstrKind::Call(..) => Opcode::Call,
        MirInstrKind::LoadFunc(..) => Opcode::LoadFunc,
        MirInstrKind::CallValue(..) => Opcode::CallValue,
        MirInstrKind::BindMatch(..) => Opcode::BindMatch,
        MirInstrKind::ToStr => Opcode::ToStr,
        // 0.0.2 U06: the v2 instructions have no v1 opcode. Lower emits them,
        // but codegen must fail loudly rather than mis-assign a v1 opcode.
        // U07 assigns 0x80–0xA4 and wires these up.
        _ => {
            return Err(CodegenError {
                message: "v2 instruction (0.0.2) requires bytecode v2 (U07); not yet encodable"
                    .to_string(),
            });
        }
    })
}

/// Opcode size of a MIR instruction (for offset pre-computation).
fn instr_size(instr: &MirInstr) -> Result<usize, CodegenError> {
    Ok(mir_opcode(&instr.kind)?.instr_len())
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
        for instr in &b.instrs {
            pos += instr_size(instr)?;
        }
        pos += terminator_size(b.terminator);
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
