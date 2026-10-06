//! Control-flow graph construction helpers.
//!
//! `FnBuilder` accumulates instructions block by block. Blocks are created
//! ahead of their predecessors when a terminator needs to reference a
//! not-yet-created target; every block is eventually terminated exactly once.
//!
//! Conditional jumps fall through (untaken path) to the *next* block in
//! `blocks`, so emission order must place each true-branch block
//! immediately after its condition block (BYTECODE.md §6.3).

use super::mir::*;

/// Placeholder target patched in once the real block exists.
const UNKNOWN: BlockId = BlockId(u32::MAX);

/// In-progress function body under construction.
pub struct FnBuilder {
    pub blocks: Vec<MirBlock>,
    /// Index of the block currently being filled.
    cur: BlockId,
    /// Block indices whose terminator still references `UNKNOWN`.
    pending: Vec<BlockId>,
}

impl Default for FnBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl FnBuilder {
    pub fn new() -> Self {
        FnBuilder {
            blocks: vec![MirBlock {
                id: BlockId(0),
                instrs: Vec::new(),
                terminator: Terminator::Return, // placeholder, overwritten
            }],
            cur: BlockId(0),
            pending: Vec::new(),
        }
    }

    /// The BlockId of the block currently being filled.
    pub fn current(&self) -> BlockId {
        self.cur
    }

    /// Emit an instruction into the current block.
    pub fn emit(&mut self, instr: MirInstr) {
        self.block_mut(self.cur).instrs.push(instr);
    }

    fn block_mut(&mut self, id: BlockId) -> &mut MirBlock {
        &mut self.blocks[id.0 as usize]
    }

    /// Allocate a fresh empty block and return its id.
    pub fn new_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(MirBlock {
            id,
            instrs: Vec::new(),
            terminator: Terminator::Return, // placeholder
        });
        id
    }

    /// Switch emission to `id`.
    pub fn start(&mut self, id: BlockId) {
        self.cur = id;
    }

    /// Terminate the current block.
    pub fn end(&mut self, terminator: Terminator) {
        let cur = self.cur;
        self.block_mut(cur).terminator = terminator;
    }

    /// Terminate the current block with a jump to `target`.
    pub fn jump(&mut self, target: BlockId) {
        self.end(Terminator::Jump(target));
    }

    /// Terminate the current block with a conditional jump whose taken
    /// target is not yet known. Call [`Self::resolve`] once the target
    /// block exists.
    pub fn jump_if_false_later(&mut self) -> BlockId {
        let cur = self.cur;
        self.end(Terminator::JumpIfFalse(UNKNOWN));
        self.pending.push(cur);
        cur
    }

    /// Same as [`Self::jump_if_false_later`] but for `JumpIfTrue`.
    pub fn jump_if_true_later(&mut self) -> BlockId {
        let cur = self.cur;
        self.end(Terminator::JumpIfTrue(UNKNOWN));
        self.pending.push(cur);
        cur
    }

    /// Terminate the current block with an unconditional jump whose target
    /// is not yet known.
    pub fn jump_later(&mut self) -> BlockId {
        let cur = self.cur;
        self.end(Terminator::Jump(UNKNOWN));
        self.pending.push(cur);
        cur
    }

    /// Patch a previously deferred jump target.
    pub fn resolve(&mut self, cond_block: BlockId, target: BlockId) {
        let b = self.block_mut(cond_block);
        match b.terminator {
            Terminator::Jump(t) if t == UNKNOWN => b.terminator = Terminator::Jump(target),
            Terminator::JumpIfFalse(t) if t == UNKNOWN => {
                b.terminator = Terminator::JumpIfFalse(target)
            }
            Terminator::JumpIfTrue(t) if t == UNKNOWN => {
                b.terminator = Terminator::JumpIfTrue(target)
            }
            other => unreachable!("resolve on non-pending terminator: {other:?}"),
        }
        self.pending.retain(|&p| p != cond_block);
    }

    /// Finalize into the block list. Entry is always `BlockId(0)`.
    ///
    /// # Panics
    /// Panics if a deferred jump target was never resolved (compiler bug).
    pub fn finish(self) -> (BlockId, Vec<MirBlock>) {
        assert!(
            self.pending.is_empty(),
            "unresolved jump targets: {:?}",
            self.pending
        );
        (BlockId(0), self.blocks)
    }
}
