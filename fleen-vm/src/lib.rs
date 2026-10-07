//! Fleen VM library: runtime values, call frames, runtime errors and the
//! interpreter itself (BYTECODE.md §3).
//!
//! [`vm::Vm`] executes modules produced by `fleen_compiler::codegen`,
//! typically after `fleen_verify::verify` has accepted them. The `fleen-vm`
//! binary drives that flow and can additionally compile `.fln` source
//! in-process, making it a one-command compile → verify → execute driver.

pub mod error;
pub mod fmt;
pub mod frame;
pub mod value;
pub mod vm;

#[cfg(test)]
mod tests;
