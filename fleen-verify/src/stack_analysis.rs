//! Exact operand-stack depth analysis via CFG worklist propagation
//! (BYTECODE.md §8 rule 4).
//!
//! Each instruction's stack effect is fixed at compile time, so the
//! analysis is exact rather than conservative. At control-flow joins,
//! all predecessors must agree on the depth — disagreement is a
//! verification failure, never a conservative approximation.

use fleen_compiler::codegen::{Func, Module, Opcode};

use crate::verify::{Decoded, VerifyError, fallthrough, jump_targets, stack_effect};

/// Analyze one function. `at` is the structural decode from
/// [`crate::verify::verify`] (every byte covered by an instruction).
///
/// Returns the depth at each visited instruction's entry, indexed by
/// byte offset (`None` for never-reached offsets).
pub fn analyze(
    func: &Func,
    module: &Module,
    at: &[Decoded],
) -> Result<Vec<Option<i64>>, VerifyError> {
    let code = &func.code;
    let mut depth_at: Vec<Option<i64>> = vec![None; code.len() + 1];
    depth_at[0] = Some(0);
    let mut worklist = vec![0usize];
    // pc -> 解码指令索引，O(1) 定位（at 由 0 起结构解码，故 pc 单调唯一）。
    let mut by_pc: Vec<Option<&Decoded>> = vec![None; code.len()];
    for d in at {
        by_pc[d.pc] = Some(d);
    }

    while let Some(pc) = worklist.pop() {
        let current = depth_at[pc].ok_or(VerifyError::TruncatedInstruction { pc })?;
        // 不变量：worklist 中的 pc 必来自结构解码结果 `at`。用 `.get()` 取而非索引，
        // 使 `at` 与 code 不一致时（或 code 为空、`at` 为空而 pc 0 已入栈）返回
        // 错误而不是 panic。
        let d = *by_pc
            .get(pc)
            .copied()
            .flatten()
            .ok_or(VerifyError::TruncatedInstruction { pc })?;
        let (delta, min_depth) = stack_effect(code, &d, &module.functions)?;

        if current < min_depth {
            return Err(VerifyError::StackUnderflow {
                pc,
                needed: min_depth,
                actual: current,
            });
        }

        if d.opcode == Opcode::Return {
            // `Return` 要求执行前栈深恰好为 1（只剩返回值）。
            if current != 1 {
                return Err(VerifyError::ReturnDepthWrong { pc, depth: current });
            }
            continue;
        }

        let next_depth = current + delta;
        if next_depth < 0 {
            return Err(VerifyError::StackUnderflow {
                pc,
                needed: -delta,
                actual: current,
            });
        }

        // Fall-through successor.
        if let Some(next) = fallthrough(&d, code.len()) {
            propagate(&mut depth_at, &mut worklist, next, next_depth)?;
        }

        // Jump targets carry the same depth (jumps don't touch the stack,
        // but the condition-pop of JumpIf* is included in `delta`).
        for target in jump_targets(code, &d)? {
            propagate(&mut depth_at, &mut worklist, target, next_depth)?;
        }
    }
    Ok(depth_at)
}

fn propagate(
    depth_at: &mut [Option<i64>],
    worklist: &mut Vec<usize>,
    target: usize,
    depth: i64,
) -> Result<(), VerifyError> {
    match depth_at[target] {
        None => {
            depth_at[target] = Some(depth);
            worklist.push(target);
            Ok(())
        }
        Some(existing) if existing == depth => Ok(()),
        Some(existing) => Err(VerifyError::StackDepthMismatch {
            pc: target,
            expected: existing,
            actual: depth,
        }),
    }
}
