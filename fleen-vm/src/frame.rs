//! Call frame layout (BYTECODE.md §3.1).

use fleen_compiler::codegen::FuncId;

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
