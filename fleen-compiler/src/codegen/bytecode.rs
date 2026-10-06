//! Bytecode module structures and instruction encoding.

use std::fmt;

pub use crate::lower::mir::{FuncId, GlobalId};

/// Unique ID into the constant pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstId(pub u32);

impl fmt::Display for ConstId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "const#{}", self.0)
    }
}

/// A constant-pool entry. `Float` hashes/compares by bit pattern so it can
/// live in a `HashMap` (`f64` is not `Eq`/`Hash` on its own). Consequence:
/// `0.0` and `-0.0` are distinct constants, and NaNs with different bit
/// patterns are distinct — this is intentional (bit-exact interning).
#[derive(Debug, Clone, PartialEq)]
pub enum Const {
    Int(i64),
    Float(f64),
    Str(Box<str>),
}

impl Eq for Const {}

impl std::hash::Hash for Const {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Const::Int(v) => {
                0u8.hash(state);
                v.hash(state);
            }
            Const::Float(v) => {
                1u8.hash(state);
                v.to_bits().hash(state);
            }
            Const::Str(s) => {
                2u8.hash(state);
                s.hash(state);
            }
        }
    }
}

/// Span → source position map entry (0.0.1: never generated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanEntry {
    pub offset: u32,
    pub start: u32,
    pub end: u32,
}

/// A compiled function.
#[derive(Debug, Clone, PartialEq)]
pub struct Func {
    pub name: ConstId,
    pub params: u16,
    pub locals: u16,
    pub code: Box<[u8]>,
    pub span_map: Box<[SpanEntry]>,
    pub is_builtin: bool,
}

/// A global variable declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub name: ConstId,
    pub mutable: bool,
}

/// The compiled module.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub version: u16,
    pub constants: Vec<Const>,
    pub functions: Vec<Func>,
    pub globals: Vec<Global>,
    pub entry: FuncId,
}

/// Codegen output alias (SPEC.md §6 `codegen` stage).
pub type Bytecode = Module;

/// Virtual-machine opcodes (BYTECODE.md §4.2 number ranges).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    // 0x00–0x0F constants & stack
    Const = 0x00,
    True = 0x01,
    False = 0x02,
    Unit = 0x03,
    Pop = 0x04,
    Dup = 0x05,
    // 0x10–0x1F locals / globals
    LoadLocal = 0x10,
    StoreLocal = 0x11,
    LoadGlobal = 0x12,
    StoreGlobal = 0x13,
    // 0x20–0x2F int arithmetic
    IAdd = 0x20,
    ISub = 0x21,
    IMul = 0x22,
    IDiv = 0x23,
    IMod = 0x24,
    // 0x30–0x3F float arithmetic
    FAdd = 0x30,
    FSub = 0x31,
    FMul = 0x32,
    FDiv = 0x33,
    // 0x40–0x4F compare / logic
    Eq = 0x40,
    Ne = 0x41,
    Lt = 0x42,
    Gt = 0x43,
    Le = 0x44,
    Ge = 0x45,
    Not = 0x46,
    NegI = 0x47,
    NegF = 0x48,
    // 0x50–0x5F control flow
    Jump = 0x50,
    JumpIfFalse = 0x51,
    JumpIfTrue = 0x52,
    // 0x60–0x6F calls
    Call = 0x60,
    LoadFunc = 0x61,
    CallValue = 0x62,
    Return = 0x63,
    // 0x70–0x7F choose helpers
    BindMatch = 0x70,
}

impl Opcode {
    /// Number of operand bytes follow the opcode (BYTECODE.md §4.1).
    pub fn operand_len(self) -> usize {
        match self {
            Opcode::Const => 4,
            Opcode::LoadLocal
            | Opcode::StoreLocal
            | Opcode::LoadGlobal
            | Opcode::StoreGlobal
            | Opcode::Jump
            | Opcode::JumpIfFalse
            | Opcode::JumpIfTrue
            | Opcode::Call
            | Opcode::LoadFunc
            | Opcode::BindMatch => 2,
            Opcode::CallValue => 1,
            _ => 0,
        }
    }

    /// Total encoded instruction length in bytes.
    pub fn instr_len(self) -> usize {
        1 + self.operand_len()
    }

    /// Decode an opcode byte.
    pub fn from_byte(b: u8) -> Option<Opcode> {
        Some(match b {
            0x00 => Opcode::Const,
            0x01 => Opcode::True,
            0x02 => Opcode::False,
            0x03 => Opcode::Unit,
            0x04 => Opcode::Pop,
            0x05 => Opcode::Dup,
            0x10 => Opcode::LoadLocal,
            0x11 => Opcode::StoreLocal,
            0x12 => Opcode::LoadGlobal,
            0x13 => Opcode::StoreGlobal,
            0x20 => Opcode::IAdd,
            0x21 => Opcode::ISub,
            0x22 => Opcode::IMul,
            0x23 => Opcode::IDiv,
            0x24 => Opcode::IMod,
            0x30 => Opcode::FAdd,
            0x31 => Opcode::FSub,
            0x32 => Opcode::FMul,
            0x33 => Opcode::FDiv,
            0x40 => Opcode::Eq,
            0x41 => Opcode::Ne,
            0x42 => Opcode::Lt,
            0x43 => Opcode::Gt,
            0x44 => Opcode::Le,
            0x45 => Opcode::Ge,
            0x46 => Opcode::Not,
            0x47 => Opcode::NegI,
            0x48 => Opcode::NegF,
            0x50 => Opcode::Jump,
            0x51 => Opcode::JumpIfFalse,
            0x52 => Opcode::JumpIfTrue,
            0x60 => Opcode::Call,
            0x61 => Opcode::LoadFunc,
            0x62 => Opcode::CallValue,
            0x63 => Opcode::Return,
            0x70 => Opcode::BindMatch,
            _ => return None,
        })
    }
}
