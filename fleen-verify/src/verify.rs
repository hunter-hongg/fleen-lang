//! Static verification of bytecode modules (BYTECODE.md §8).

use fleen_compiler::codegen::{Const, Func, FuncId, Module, Opcode};

/// A verification failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// Unknown opcode byte at this pc.
    BadOpcode {
        pc: usize,
        byte: u8,
    },
    /// Instruction runs off the end of `code`.
    TruncatedInstruction {
        pc: usize,
    },
    /// Byte stream not fully covered by instruction boundaries.
    BadInstructionStream {
        pc: usize,
    },
    /// Jump target is outside the function body.
    JumpTargetOutOfRange {
        pc: usize,
        target: usize,
    },
    /// Jump target does not land on an instruction boundary.
    JumpTargetNotBoundary {
        pc: usize,
        target: usize,
    },
    /// A path reaches the end of `code` without `Return`.
    FallOffEnd {
        pc: usize,
    },
    /// An index (constant / function / global) is out of range.
    ConstIndexOutOfRange {
        pc: usize,
        index: u32,
    },
    FuncIndexOutOfRange {
        pc: usize,
        index: u16,
    },
    GlobalIndexOutOfRange {
        pc: usize,
        index: u16,
    },
    /// `Return` does not have exactly the return value on the stack.
    ReturnDepthWrong {
        pc: usize,
        depth: i64,
    },
    /// Operand stack underflows before an instruction.
    StackUnderflow {
        pc: usize,
        needed: i64,
        actual: i64,
    },
    /// Two control-flow predecessors disagree on stack depth.
    StackDepthMismatch {
        pc: usize,
        expected: i64,
        actual: i64,
    },
    /// `StoreGlobal` writes a `mutable: false` global outside `__init__`.
    StoreToConstGlobal {
        pc: usize,
        func: usize,
        global: usize,
    },
    /// Constant pool contains duplicates.
    DuplicateConst {
        first: usize,
        second: usize,
    },
    /// `locals < params`.
    LocalsLessThanParams {
        func: usize,
    },
    /// Local slot operand out of range.
    LocalSlotOutOfRange {
        pc: usize,
        slot: u16,
        locals: u16,
    },
    /// Entry function id out of range.
    BadEntry {
        entry: u32,
    },
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadOpcode { pc, byte } => {
                write!(f, "unknown opcode 0x{byte:02x} at pc {pc}")
            }
            Self::TruncatedInstruction { pc } => {
                write!(f, "instruction at pc {pc} runs past end of code")
            }
            Self::BadInstructionStream { pc } => {
                write!(
                    f,
                    "code bytes not covered by instruction boundaries at pc {pc}"
                )
            }
            Self::JumpTargetOutOfRange { pc, target } => {
                write!(f, "jump at pc {pc} targets {target}, outside function body")
            }
            Self::JumpTargetNotBoundary { pc, target } => {
                write!(
                    f,
                    "jump at pc {pc} targets {target}, not an instruction boundary"
                )
            }
            Self::FallOffEnd { pc } => {
                write!(
                    f,
                    "instruction at pc {pc} falls off the end of the function"
                )
            }
            Self::ConstIndexOutOfRange { pc, index } => {
                write!(
                    f,
                    "Const at pc {pc} references constant index {index}, out of range"
                )
            }
            Self::FuncIndexOutOfRange { pc, index } => {
                write!(
                    f,
                    "instruction at pc {pc} references function index {index}, out of range"
                )
            }
            Self::GlobalIndexOutOfRange { pc, index } => {
                write!(
                    f,
                    "global access at pc {pc} references index {index}, out of range"
                )
            }
            Self::ReturnDepthWrong { pc, depth } => {
                write!(f, "Return at pc {pc} expects stack depth 1, found {depth}")
            }
            Self::StackUnderflow { pc, needed, actual } => {
                write!(
                    f,
                    "stack underflow at pc {pc}: needs {needed}, has {actual}"
                )
            }
            Self::StackDepthMismatch {
                pc,
                expected,
                actual,
            } => {
                write!(
                    f,
                    "stack depth mismatch at pc {pc}: predecessor expects {expected}, got {actual}"
                )
            }
            Self::StoreToConstGlobal { pc, func, global } => {
                write!(
                    f,
                    "StoreGlobal at pc {pc} in function {func} writes const global {global}"
                )
            }
            Self::DuplicateConst { first, second } => {
                write!(
                    f,
                    "constant pool entries {first} and {second} are duplicates"
                )
            }
            Self::LocalsLessThanParams { func } => {
                write!(f, "function {func}: locals < params")
            }
            Self::LocalSlotOutOfRange { pc, slot, locals } => {
                write!(
                    f,
                    "local slot {slot} at pc {pc} out of range (locals = {locals})"
                )
            }
            Self::BadEntry { entry } => write!(f, "entry function id {entry} out of range"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// One decoded instruction.
#[derive(Debug, Clone, Copy)]
pub struct Decoded {
    pub pc: usize,
    pub opcode: Opcode,
    pub len: usize,
}

fn u16_at(code: &[u8], pc: usize) -> Result<u16, VerifyError> {
    if pc + 2 > code.len() {
        return Err(VerifyError::TruncatedInstruction { pc });
    }
    Ok(u16::from_le_bytes([code[pc], code[pc + 1]]))
}

fn u32_at(code: &[u8], pc: usize) -> Result<u32, VerifyError> {
    if pc + 4 > code.len() {
        return Err(VerifyError::TruncatedInstruction { pc });
    }
    Ok(u32::from_le_bytes([
        code[pc],
        code[pc + 1],
        code[pc + 2],
        code[pc + 3],
    ]))
}

/// Decode the instruction starting at `pc`.
pub fn decode(code: &[u8], pc: usize) -> Result<Decoded, VerifyError> {
    let byte = *code
        .get(pc)
        .ok_or(VerifyError::TruncatedInstruction { pc })?;
    let opcode = Opcode::from_byte(byte).ok_or(VerifyError::BadOpcode { pc, byte })?;
    let len = opcode.instr_len();
    if pc + len > code.len() {
        return Err(VerifyError::TruncatedInstruction { pc });
    }
    Ok(Decoded { pc, opcode, len })
}

/// Operand accessors.
pub fn const_index(code: &[u8], d: &Decoded) -> Result<u32, VerifyError> {
    u32_at(code, d.pc + 1)
}
pub fn func_index(code: &[u8], d: &Decoded) -> Result<u16, VerifyError> {
    u16_at(code, d.pc + 1)
}
pub fn slot_index(code: &[u8], d: &Decoded) -> Result<u16, VerifyError> {
    u16_at(code, d.pc + 1)
}
pub fn global_index(code: &[u8], d: &Decoded) -> Result<u16, VerifyError> {
    u16_at(code, d.pc + 1)
}
pub fn jump_target(code: &[u8], d: &Decoded) -> Result<usize, VerifyError> {
    Ok(u16_at(code, d.pc + 1)? as usize)
}
pub fn call_value_argc(code: &[u8], d: &Decoded) -> Result<u8, VerifyError> {
    code.get(d.pc + 1)
        .copied()
        .ok_or(VerifyError::TruncatedInstruction { pc: d.pc })
}

/// Jump targets of an instruction (absolute byte offsets).
pub fn jump_targets(code: &[u8], d: &Decoded) -> Result<Vec<usize>, VerifyError> {
    match d.opcode {
        Opcode::Jump | Opcode::JumpIfFalse | Opcode::JumpIfTrue => Ok(vec![jump_target(code, d)?]),
        _ => Ok(vec![]),
    }
}

/// The sequential fall-through successor, if any.
pub fn fallthrough(d: &Decoded, code_len: usize) -> Option<usize> {
    match d.opcode {
        Opcode::Jump | Opcode::Return => None,
        _ => {
            let next = d.pc + d.len;
            (next < code_len).then_some(next)
        }
    }
}

/// Static stack effect of an instruction: `(delta, min_depth_before)`.
///
/// `Call` needs the callee's `params` to compute its effect, hence the
/// `functions` table. Corresponds to the table in BYTECODE.md §5.
pub fn stack_effect(
    code: &[u8],
    d: &Decoded,
    functions: &[Func],
) -> Result<(i64, i64), VerifyError> {
    Ok(match d.opcode {
        Opcode::Const
        | Opcode::True
        | Opcode::False
        | Opcode::Unit
        | Opcode::LoadLocal
        | Opcode::LoadGlobal
        | Opcode::LoadFunc => (1, 0),
        Opcode::Pop => (-1, 1),
        Opcode::Dup => (1, 1),
        Opcode::StoreLocal | Opcode::StoreGlobal => (-1, 1),
        Opcode::IAdd
        | Opcode::ISub
        | Opcode::IMul
        | Opcode::IDiv
        | Opcode::IMod
        | Opcode::FAdd
        | Opcode::FSub
        | Opcode::FMul
        | Opcode::FDiv
        | Opcode::Eq
        | Opcode::Ne
        | Opcode::Lt
        | Opcode::Gt
        | Opcode::Le
        | Opcode::Ge => (-1, 2),
        Opcode::Not | Opcode::NegI | Opcode::NegF => (0, 1),
        // 0.0.2 U13: ToStr pops a scalar and pushes a fresh string (Δ0, min 1).
        Opcode::ToStr => (0, 1),
        Opcode::Jump => (0, 0),
        Opcode::JumpIfFalse | Opcode::JumpIfTrue => (-1, 1),
        Opcode::Call => {
            let t = func_index(code, d)? as usize;
            let callee = functions.get(t).ok_or(VerifyError::FuncIndexOutOfRange {
                pc: d.pc,
                index: t as u16,
            })?;
            let p = callee.params as i64;
            (1 - p, p)
        }
        Opcode::CallValue => {
            let argc = call_value_argc(code, d)? as i64;
            (-argc, argc + 1)
        }
        Opcode::BindMatch => (0, 1),
        Opcode::Return => (0, 1), // 视为路径终止；深度相等性在分析中单独校验
    })
}

/// Walk every instruction in `code` (structural decode from 0).
///
/// Returns the list of decoded instructions. Fails on bad opcodes,
/// truncation, trailing garbage, or a jump target that is out of
/// range / not on an instruction boundary. Also validates operand
/// index ranges and local slot bounds.
fn walk_function(
    func: &Func,
    func_idx: usize,
    module: &Module,
    allow_const_global_store: bool,
) -> Result<Vec<Decoded>, VerifyError> {
    if func.locals < func.params {
        return Err(VerifyError::LocalsLessThanParams { func: func_idx });
    }
    let code = &func.code;
    let mut at = Vec::new();
    let mut boundaries = Vec::new();
    let mut pc = 0;
    while pc < code.len() {
        let d = decode(code, pc)?;
        boundaries.push(pc);
        at.push(d);
        pc += d.len;
    }
    if pc != code.len() {
        return Err(VerifyError::BadInstructionStream { pc });
    }
    let is_boundary = |t: usize| boundaries.binary_search(&t).is_ok();

    for d in &at {
        match d.opcode {
            Opcode::Const => {
                let idx = const_index(code, d)?;
                if idx as usize >= module.constants.len() {
                    return Err(VerifyError::ConstIndexOutOfRange {
                        pc: d.pc,
                        index: idx,
                    });
                }
            }
            Opcode::Call | Opcode::LoadFunc => {
                let idx = func_index(code, d)?;
                if idx as usize >= module.functions.len() {
                    return Err(VerifyError::FuncIndexOutOfRange {
                        pc: d.pc,
                        index: idx,
                    });
                }
            }
            Opcode::LoadGlobal | Opcode::StoreGlobal => {
                let idx = global_index(code, d)?;
                let g =
                    module
                        .globals
                        .get(idx as usize)
                        .ok_or(VerifyError::GlobalIndexOutOfRange {
                            pc: d.pc,
                            index: idx,
                        })?;
                if d.opcode == Opcode::StoreGlobal && !g.mutable && !allow_const_global_store {
                    return Err(VerifyError::StoreToConstGlobal {
                        pc: d.pc,
                        func: func_idx,
                        global: idx as usize,
                    });
                }
            }
            Opcode::LoadLocal | Opcode::StoreLocal | Opcode::BindMatch => {
                let slot = slot_index(code, d)?;
                if slot >= func.locals {
                    return Err(VerifyError::LocalSlotOutOfRange {
                        pc: d.pc,
                        slot,
                        locals: func.locals,
                    });
                }
            }
            Opcode::Jump | Opcode::JumpIfFalse | Opcode::JumpIfTrue => {
                let target = jump_target(code, d)?;
                if target >= code.len() {
                    return Err(VerifyError::JumpTargetOutOfRange { pc: d.pc, target });
                }
                if !is_boundary(target) {
                    return Err(VerifyError::JumpTargetNotBoundary { pc: d.pc, target });
                }
            }
            _ => {}
        }
        // Rule 3 (structural part): only `Return` and `Jump` may end the
        // stream — anything else would fall off the end of the function.
        if d.pc + d.len == code.len() {
            match d.opcode {
                Opcode::Return | Opcode::Jump => {}
                _ => return Err(VerifyError::FallOffEnd { pc: d.pc }),
            }
        }
    }
    Ok(at)
}

/// Verify a whole module. Runs all rules in BYTECODE.md §8.
pub fn verify(module: &Module) -> Result<(), VerifyError> {
    // Rule 6: constant pool has no duplicates.
    for i in 0..module.constants.len() {
        for j in (i + 1)..module.constants.len() {
            if module.constants[i] == module.constants[j] {
                return Err(VerifyError::DuplicateConst {
                    first: i,
                    second: j,
                });
            }
        }
    }

    if module.entry.0 as usize >= module.functions.len() {
        return Err(VerifyError::BadEntry {
            entry: module.entry.0,
        });
    }

    // `__init__` is identified by its well-known name.
    let entry_name = function_name(module, module.entry);
    let entry_is_init = entry_name == Some("__init__");

    for (i, func) in module.functions.iter().enumerate() {
        let allow = i == module.entry.0 as usize && entry_is_init;
        let at = walk_function(func, i, module, allow)?;
        crate::stack_analysis::analyze(func, module, &at)?;
    }
    Ok(())
}

/// Resolve a function's display name through the constant pool.
fn function_name(module: &Module, id: FuncId) -> Option<&str> {
    let func = module.functions.get(id.0 as usize)?;
    match module.constants.get(func.name.0 as usize) {
        Some(Const::Str(s)) => Some(s),
        _ => None,
    }
}
