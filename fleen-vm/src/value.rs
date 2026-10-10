//! Runtime value representation (BYTECODE.md §3.2).

use std::fmt;

use fleen_compiler::codegen::FuncId;

/// A self-describing runtime value.
///
/// Ownership discipline is static: typeck guarantees that `Str`, `Boxed`,
/// `Ok` and `Err` values always have a unique owner, and that `Ref` handles
/// are read-only, never stored, and die with the call they were created in.
/// The VM's Rust move/drop semantics are the runtime implementation —
/// `Drop` is deterministic (frame `truncate`, slot overwrite, `Pop`);
/// there is no GC.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit IEEE-754 float.
    Float(f64),
    /// Boolean.
    Bool(bool),
    /// Immutable string with exclusive ownership. 0.0.2 replaced the 0.0.1
    /// temporary shared-reference string so that `clone` is a real deep copy
    /// instead of a reference-count bump (BYTECODE.md §3.2).
    Str(Box<str>),
    /// Heap box (`box<T>`): unique owner of the inner value.
    Boxed(Box<Value>),
    /// Read-only borrow handle (0.0.2, `ref` parameters only).
    ///
    /// `base` is the *absolute* index of the borrowed local inside the
    /// shared operand stack (`CallFrame::stack_base + slot` of the frame
    /// that owns it); `slot` is the local slot itself. Handles use indices,
    /// not pointers, so operand-stack reallocation cannot invalidate them.
    /// Typeck guarantees a handle only ever points at a frame below the
    /// current call chain, which therefore outlives the handle; the
    /// defensive `BorrowOutOfRange` check lives in the instruction dispatch.
    /// Handles never appear in the constant pool (values are not
    /// serialized).
    ///
    /// The derived `PartialEq` compares handles structurally and exists for
    /// tests only; the `Eq` instruction dereferences before comparing.
    Ref { base: u32, slot: u16 },
    /// `Ok(v)` payload of a `Result` value.
    Ok(Box<Value>),
    /// `Err(e)` payload of a `Result` value.
    Err(Box<Value>),
    /// The unit value (no meaningful data).
    Unit,
    /// First-class function reference.
    Func(FuncId),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Str(s) => write!(f, "{s}"),
            // A box is observed through its pointee (BYTECODE.md §3.2:
            // `Boxed` compares — and prints — the inner value).
            Value::Boxed(v) => write!(f, "{v}"),
            Value::Ref { .. } => write!(f, "<ref>"),
            Value::Ok(v) => write!(f, "Ok({v})"),
            Value::Err(v) => write!(f, "Err({v})"),
            Value::Unit => write!(f, "unit"),
            Value::Func(id) => write!(f, "<func #{}>", id.0),
        }
    }
}
