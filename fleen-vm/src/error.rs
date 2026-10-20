//! VM runtime errors (BYTECODE.md §3.4). Never catchable from Fleen code.

use fleen_compiler::codegen::{ConstId, FuncId, GlobalId};

/// The specific failure behind a [`RuntimeError`].
///
/// These are never catchable from Fleen code; a runtime error aborts
/// execution of the module and is reported to the host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeErrorKind {
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
    /// `UnwrapOk` hit an `Err` value or `UnwrapErr` hit an `Ok` value
    /// (defensive: typeck routes each payload through the matching
    /// arm / unwrap opcode before codegen).
    ResultMismatch,
    /// A borrow handle pointed outside the live operand stack (defensive:
    /// handles only reference lower frames that outlive them). Reserved for
    /// U09's `MakeRefLocal` and handle dereference.
    BorrowOutOfRange,
    /// `StructNew` names a struct id outside the `struct_defs` table
    /// (0.0.3 V02, defensive: verify rejects it first).
    StructOutOfRange(u16),
    /// A field offset exceeded the struct's field count (0.0.3 V02,
    /// defensive: the encoding carries no struct id on `GetField`/
    /// `SetField*`, so verify cannot check offsets statically).
    FieldOutOfRange { index: u16, len: usize },
    /// An array index was outside `0..len` (0.0.3 V03). This is *not*
    /// defensive — the `a[i]` syntax is bounds-checked at runtime by
    /// design (D3); the error carries the faulting instruction's span via
    /// the span_map (V01).
    IndexOutOfRange { index: i64, len: usize },
    /// `std.os.exit(code)` requested process termination (0.0.3 V06).
    /// This is *not* a failure — the intrinsic must unwind the interpreter
    /// loop from inside a call chain, and the runtime-error channel is the
    /// only path out; the CLI maps it to a bare `process::exit(code)`
    /// without rendering anything.
    Exit(i64),
    /// A coroutine frame was resumed after it had already run to completion or
    /// failed (0.0.4 T04, D1 — a `Future` is driven once).  Defensive: typeck
    /// gives every `Future<T>` value a single owner, so reaching this needs
    /// hand-written bytecode.
    FutureNotResumable,
}

impl std::fmt::Display for RuntimeErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RuntimeErrorKind::StackUnderflow => write!(f, "stack underflow"),
            RuntimeErrorKind::InvalidOpcode(b) => write!(f, "invalid opcode 0x{b:02x}"),
            RuntimeErrorKind::ConstOutOfRange(id) => write!(f, "const id {} out of range", id.0),
            RuntimeErrorKind::FuncOutOfRange(id) => write!(f, "func id {} out of range", id.0),
            RuntimeErrorKind::GlobalOutOfRange(id) => write!(f, "global id {} out of range", id.0),
            RuntimeErrorKind::ImmutableGlobal(id) => {
                write!(f, "cannot assign to immutable global {}", id.0)
            }
            RuntimeErrorKind::ArithTypeMismatch => write!(f, "arithmetic type mismatch"),
            RuntimeErrorKind::CastOperandNotScalar => {
                write!(f, "cast operand is not a scalar (int/float/bool)")
            }
            RuntimeErrorKind::DivisionByZero => write!(f, "division by zero"),
            RuntimeErrorKind::CallDepthExceeded => write!(f, "call depth exceeded"),
            RuntimeErrorKind::ArithmeticOverflow => write!(f, "integer overflow"),
            RuntimeErrorKind::StackShape => write!(f, "stack shape violation"),
            RuntimeErrorKind::UnknownBuiltin(name) => write!(f, "unknown builtin: {name}"),
            RuntimeErrorKind::BadJumpTarget => write!(f, "bad jump target"),
            RuntimeErrorKind::UnsupportedVersion(v) => write!(f, "unsupported module version {v}"),
            RuntimeErrorKind::ResultMismatch => {
                write!(f, "unwrap opcode applied to mismatched Result variant")
            }
            RuntimeErrorKind::BorrowOutOfRange => write!(f, "borrow handle out of range"),
            RuntimeErrorKind::StructOutOfRange(id) => {
                write!(f, "struct id {id} out of range")
            }
            RuntimeErrorKind::FieldOutOfRange { index, len } => {
                write!(
                    f,
                    "field offset {index} out of range (struct has {len} fields)"
                )
            }
            RuntimeErrorKind::IndexOutOfRange { index, len } => {
                write!(f, "index {index} out of range (array length {len})")
            }
            RuntimeErrorKind::Exit(code) => write!(f, "process exited with code {code}"),
            RuntimeErrorKind::FutureNotResumable => {
                write!(f, "future has already been driven and cannot be resumed")
            }
        }
    }
}

/// A runtime failure together with where it happened (0.0.2 U10).
///
/// The location is attached once, at the [`crate::vm::Vm::run`] exit, from
/// the top call frame — `step()` only writes `frame.ip` back on the success
/// path, so an in-flight `ip` still names the failing instruction, and
/// `call`-path errors happen before a new frame is pushed (TICKETS U07
/// implementation note 4). Fields are `None` when no frame was active
/// (e.g. [`RuntimeErrorKind::UnsupportedVersion`]) or the module carries no
/// `span_map` entries (v1 bytecode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeError {
    /// The specific failure.
    pub kind: RuntimeErrorKind,
    /// Source byte span `(start, end)` of the failing instruction, taken
    /// from the function's `span_map` (the closest entry at or before the
    /// failing `ip`, since synthetic instructions carry no entry).
    pub span: Option<(u32, u32)>,
    /// Name of the function executing when the error surfaced.
    pub func: Option<Box<str>>,
    /// Byte offset within the function's `code` of the failing instruction
    /// (the `frame.ip` at error time).
    pub pc: Option<u32>,
}

impl RuntimeError {
    /// A bare error with no location information.
    pub fn new(kind: RuntimeErrorKind) -> Self {
        RuntimeError {
            kind,
            span: None,
            func: None,
            pc: None,
        }
    }
}

impl From<RuntimeErrorKind> for RuntimeError {
    fn from(kind: RuntimeErrorKind) -> Self {
        RuntimeError::new(kind)
    }
}

impl std::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Only the message; source location rendering is the CLI's job
        // (it owns the source text in `.fln` mode and the output format).
        self.kind.fmt(f)
    }
}

impl std::error::Error for RuntimeError {}
