//! Call frame layout (BYTECODE.md §3.1) and the 0.0.4 coroutine frame (D1).

use fleen_compiler::codegen::FuncId;

use crate::error::RuntimeError;
use crate::value::Value;

/// One activation of a function.
#[derive(Debug, Clone, Copy)]
pub struct CallFrame {
    /// The function being executed.
    pub func: FuncId,
    /// Instruction pointer: byte offset into `code`.
    pub ip: usize,
    /// Start slot of this frame's locals inside the shared operand stack.
    pub stack_base: usize,
}

/// Whether a [`FutureFrame`] can still make progress (0.0.4, D1).
#[derive(Debug, Clone, PartialEq)]
pub enum FutureState {
    /// Resumable: execution continues at `FutureFrame::resume_pc` with the
    /// saved `locals` / `operands`.  Covers both the not-yet-started frame an
    /// async call creates (pc 0, arguments in slots `0..params`) and a frame
    /// suspended mid-body.
    Resumable,
    /// The body ran to completion and produced its `T`.  A driven future never
    /// runs again (PLAN §3.3 "driven once").
    Completed(Value),
    /// The body failed.  `await` on such a future re-raises the error at the
    /// awaiting site (PLAN §4.3); the error is captured while the coroutine
    /// frame was live, so it keeps its source location.
    Failed(RuntimeError),
}

/// A suspendable coroutine frame — the runtime value of a `Future<T>`
/// (0.0.4 D1).
///
/// A frame is **location-independent**: `resume_pc` is an offset into the
/// function's own `code`, and `locals` / `operands` are owned values, so
/// nothing in a frame refers to a live operand stack.  That is what lets
/// [`crate::vm::Vm::resume_frame`] restore it at a different stack depth — and,
/// once the T06 executor lands, on a different worker thread.  Unlike
/// [`CallFrame`], a stored frame carries no `stack_base`.
///
/// A frame must never capture a borrow handle: `Value::Ref` stores an absolute
/// operand-stack index that cannot survive a snapshot, and typeck rejects `ref`
/// inside an async fn (D2, `RefInAsync`).
#[derive(Debug, Clone, PartialEq)]
pub struct FutureFrame {
    /// The async function being executed.
    pub func: FuncId,
    /// Instruction pointer to continue at — the instruction after the
    /// suspension point.  Sized like [`CallFrame::ip`], the value it becomes on
    /// resume.
    pub resume_pc: usize,
    /// The frame's local slots, exactly `Func::locals` of them; the parameters
    /// occupy slots `0..params`.
    pub locals: Vec<Value>,
    /// The operand-stack entries above the local slots at the suspension point.
    pub operands: Vec<Value>,
    /// Whether the frame can still run.
    pub state: FutureState,
}

impl FutureFrame {
    /// A frame that has not started yet: `args` fill the parameter slots, the
    /// remaining `locals` slots are [`Value::Unit`] placeholders, and execution
    /// begins at pc 0.  This is the shape an async call produces before its
    /// first poll.
    pub fn new(func: FuncId, args: Vec<Value>, locals: usize) -> Self {
        let mut slots = args;
        slots.resize(locals, Value::Unit);
        FutureFrame {
            func,
            resume_pc: 0,
            locals: slots,
            operands: Vec::new(),
            state: FutureState::Resumable,
        }
    }
}
