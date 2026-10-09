//! MIR (Mid-level Intermediate Representation) for Fleen.
//!
//! MIR is the output of the Lower stage: TypedHir with control flow
//! (if/while/choose/and/or) flattened into basic blocks with terminators.
//! It is stack-machine shaped and maps ~1:1 to bytecode instructions.
//!
//! As of 0.0.2 (U06), each instruction carries a source span for span_map
//! generation (U07). The MirInstr kind and span are stored in a struct wrapper.

use crate::lexer::Span;
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

/// A MIR instruction (stack machine, maps ~1:1 to bytecode).
///
/// As of 0.0.2 (U06), this is a struct wrapper containing the instruction kind
/// and its source span. The span is used by the codegen stage (U07) to build
/// the span_map for runtime diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct MirInstr {
    /// The instruction kind (opcode and operands).
    pub kind: MirInstrKind,
    /// Source span of the expression/statement that generated this instruction.
    pub span: Span,
}

impl MirInstr {
    /// Construct an instruction with a source span.
    pub fn new(kind: MirInstrKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Instruction kind (opcode and operands) for a MIR instruction.
#[derive(Debug, Clone, PartialEq)]
pub enum MirInstrKind {
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
    DupDeep, // 0.0.2 U06: deep copy (clone's stack form)
    // Locals (slots)
    LoadLocal(u16),
    StoreLocal(u16),
    MoveLocal(u16),  // 0.0.2 U06: consume slot, slot cleared to Unit
    CloneLocal(u16), // 0.0.2 U06: deep copy from slot
    // Globals
    LoadGlobal(u16),
    StoreGlobal(u16),
    CloneGlobal(u16), // 0.0.2 U06: deep copy from global
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
    // 0.0.2 U06: box / ref instructions
    AllocBox,          // v → b (heap allocation)
    DerefBox,          // b → b v (read pointee copy)
    StoreDerefBox,     // b v → (write pointee, old value released)
    MakeRefLocal(u16), // → ref (borrow handle to local slot)
    // 0.0.2 U06: Result / error handling instructions
    PackOk,    // v → ok(v)
    PackErr,   // v → err(v)
    IsErr,     // result → bool
    UnwrapOk,  // result → v (payload T)
    UnwrapErr, // result → e (payload E)
}
