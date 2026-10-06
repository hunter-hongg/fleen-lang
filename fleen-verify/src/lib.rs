//! fleen-verify: static bytecode verification (BYTECODE.md §8).
//!
//! Verifies a [`Module`] without executing it. Used by the compiler
//! pipeline acceptance (codegen output must pass) and by the VM
//! before running.

pub mod stack_analysis;
mod verify;

pub use verify::{VerifyError, verify};

#[cfg(test)]
mod tests;
