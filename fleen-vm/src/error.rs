//! VM runtime errors (BYTECODE.md §3.4). Never catchable from Fleen code.

use fleen_compiler::codegen::{ConstId, FuncId, GlobalId};

/// Errors raised while executing bytecode.
///
/// These are never catchable from Fleen code; a runtime error aborts
/// execution of the module and is reported to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    /// Operand stack underflowed.
    StackUnderflow,
    /// Encountered a byte that is not a known opcode.
    InvalidOpcode(u8),
    /// Constant-pool index out of range.
    ConstOutOfRange(ConstId),
    /// Function index out of range.
    FuncOutOfRange(FuncId),
    /// Global index out of range.
    GlobalOutOfRange(GlobalId),
    /// Assignment to a `const` global outside `__init__`.
    ImmutableGlobal(GlobalId),
    /// Operand types did not match the instruction (e.g. `int + float`).
    ArithTypeMismatch,
    /// `ToStr` operand was not a scalar int/float/bool (defensive: typeck
    /// rejects non-scalar casts before codegen).
    CastOperandNotScalar,
    /// Integer division or modulo by zero.
    DivisionByZero,
    /// Recursion deeper than `MAX_CALL_DEPTH`.
    CallDepthExceeded,
    /// Integer addition/subtraction/multiplication overflow (BYTECODE.md §5.3).
    ArithmeticOverflow,
    /// Operand stack shape does not match the instruction's contract
    /// (defensive: `fleen-verify` should have rejected the bytecode).
    StackShape,
    /// Call to a builtin name that the VM does not implement.
    UnknownBuiltin(String),
    /// Jump target outside the function body or mid-instruction.
    BadJumpTarget,
    /// Module header version this VM cannot execute.
    UnsupportedVersion(u16),
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeError::StackUnderflow => write!(f, "stack underflow"),
            RuntimeError::InvalidOpcode(b) => write!(f, "invalid opcode 0x{b:02x}"),
            RuntimeError::ConstOutOfRange(id) => write!(f, "const id {} out of range", id.0),
            RuntimeError::FuncOutOfRange(id) => write!(f, "func id {} out of range", id.0),
            RuntimeError::GlobalOutOfRange(id) => write!(f, "global id {} out of range", id.0),
            RuntimeError::ImmutableGlobal(id) => {
                write!(f, "cannot assign to immutable global {}", id.0)
            }
            RuntimeError::ArithTypeMismatch => write!(f, "arithmetic type mismatch"),
            RuntimeError::CastOperandNotScalar => {
                write!(f, "cast operand is not a scalar (int/float/bool)")
            }
            RuntimeError::DivisionByZero => write!(f, "division by zero"),
            RuntimeError::CallDepthExceeded => write!(f, "call depth exceeded"),
            RuntimeError::ArithmeticOverflow => write!(f, "integer overflow"),
            RuntimeError::StackShape => write!(f, "stack shape violation"),
            RuntimeError::UnknownBuiltin(name) => write!(f, "unknown builtin: {name}"),
            RuntimeError::BadJumpTarget => write!(f, "bad jump target"),
            RuntimeError::UnsupportedVersion(v) => write!(f, "unsupported module version {v}"),
        }
    }
}

impl std::error::Error for RuntimeError {}
