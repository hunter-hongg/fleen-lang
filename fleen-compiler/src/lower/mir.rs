//! MIR (Mid-level Intermediate Representation) for Fleen.
//!
//! MIR is the output of the Lower stage: TypedHir with control flow
//! (if/while/choose/and/or) flattened into basic blocks with terminators.
//! It is stack-machine shaped and maps ~1:1 to bytecode instructions.

use crate::typeck::typed_hir::Type;
use std::fmt;

/// Unique ID of a function in the MIR module.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FuncId(pub u32);

impl fmt::Display for FuncId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "func#{}", self.0)
    }
}

/// Unique ID of a global variable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlobalId(pub u32);

impl fmt::Display for GlobalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "global#{}", self.0)
    }
}

/// Unique ID of a basic block within a function.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub u32);

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block#{}", self.0)
    }
}

/// MIR program.
#[derive(Debug, Clone, PartialEq)]
pub struct Mir {
    pub funcs: Vec<MirFunc>,
    pub globals: Vec<MirGlobal>,
}

/// A lowered function.
#[derive(Debug, Clone, PartialEq)]
pub struct MirFunc {
    pub func_id: FuncId,
    pub name: String,
    /// Number of parameters (slots `0..params`).
    pub params: u16,
    /// Total local slots including parameters.
    pub locals: u16,
    /// Entry block.
    pub entry: BlockId,
    pub blocks: Vec<MirBlock>,
    /// Builtin functions (e.g. `print`) are executed host-side by the VM;
    /// their `blocks` carry a `Unit; Return` placeholder so the structure
    /// stays well-formed.
    pub is_builtin: bool,
    pub ret_type: Type,
}

/// A global variable with its initialization code.
///
/// `init` lowers the initializer expression; it leaves exactly one value
/// on the operand stack which becomes the global's initial value. Globals
/// are initialized in declaration order (BYTECODE.md §2).
#[derive(Debug, Clone, PartialEq)]
pub struct MirGlobal {
    pub global_id: GlobalId,
    pub name: String,
    pub mutable: bool,
    pub ty: Type,
    pub init: Vec<MirInstr>,
}

/// A basic block: straight-line instructions + a terminator.
#[derive(Debug, Clone, PartialEq)]
pub struct MirBlock {
    pub id: BlockId,
    pub instrs: Vec<MirInstr>,
    pub terminator: Terminator,
}

/// Block terminator.
///
/// Conditional jumps pop the condition bool; the **untaken** path falls
/// through to the next block in `MirFunc::blocks` (mirroring bytecode's
/// linear fallthrough, BYTECODE.md §6.3). Taken paths jump to the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminator {
    /// Return from the function; stack top is the return value.
    Return,
    /// Unconditional jump.
    Jump(BlockId),
    /// Pop a bool; jump on `false`, else fall through to the next block.
    JumpIfFalse(BlockId),
    /// Pop a bool; jump on `true`, else fall through to the next block.
    JumpIfTrue(BlockId),
    // No Switch / Call terminators in 0.0.1 (Call is a normal instruction).
}

/// A MIR instruction (栈式, maps ~1:1 to bytecode).
#[derive(Debug, Clone, PartialEq)]
pub enum MirInstr {
    // Constants
    ConstInt(i64),
    ConstFloat(f64),
    ConstStr(String),
    True,
    False,
    Unit,
    // Stack manipulation
    Pop,
    Dup,
    // Locals (slots)
    LoadLocal(u16),
    StoreLocal(u16),
    // Globals
    LoadGlobal(u16),
    StoreGlobal(u16),
    // Int arithmetic
    IAdd,
    ISub,
    IMul,
    IDiv,
    IMod,
    // Float arithmetic
    FAdd,
    FSub,
    FMul,
    FDiv,
    // Comparisons / logic
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,
    Not,
    NegI,
    NegF,
    // Calls
    Call(FuncId),
    LoadFunc(FuncId),
    CallValue(u8),
    // choose helper: copy stack top into the binding slot (v → v).
    BindMatch(u16),
    // 0.0.2 U13: type cast (scalar → string); pops a scalar, pushes a fresh
    // owned string. Stack effect: Δ0, min 1 (v → s).
    ToStr,
}
