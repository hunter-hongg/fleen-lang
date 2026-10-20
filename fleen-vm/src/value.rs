//! Runtime value representation (BYTECODE.md §3.2).

use std::fmt;

use fleen_compiler::codegen::FuncId;

use crate::frame::FutureFrame;

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
    /// User-defined struct value (0.0.3 V02): fields in declaration order.
    /// Owned unless the struct's field types are all Copy (static rule);
    /// `Eq` on a struct is a typeck-level error, so a struct only reaches
    /// comparison opcodes through malformed bytecode (defensive error).
    Struct(Box<[Value]>),
    /// Array value (0.0.3 V03): elements in source order. **Always owned**
    /// (D3 — even `[int]`); `Eq` on an array is a typeck-level error
    /// (defensive error in the VM).
    Arr(Box<[Value]>),
    /// Range value (0.0.3 V04): an int interval, `start..end` with an
    /// exclusive `end`. Copy (two `i64`s — no heap). Source-level range
    /// construction arrives with `std.collection.range` (V06); until then
    /// the variant exists to give the `for` desugar a well-typed iterable
    /// (exercised by hand-built bytecode tests, not from `.fln` source).
    /// `Eq` on a Range is a typeck-level error (defensive error in the VM).
    Range {
        /// First value the loop yields.
        start: i64,
        /// Exclusive upper bound: `start..end` never yields `end`.
        end: i64,
    },
    /// The unit value (no meaningful data).
    Unit,
    /// First-class function reference.
    Func(FuncId),
    /// Suspendable coroutine frame — the runtime value of a `Future<T>`
    /// (0.0.4 D1/T04).  Boxed so that the frame's two `Vec`s and its state do
    /// not inflate every `Value` the VM touches (D1 also calls a future a heap
    /// object).  Like `Ref` it is **not serialized**: there is no `.flnc` tag,
    /// no constant-pool or `FieldTypeTag` change.  Typeck makes it non-Copy
    /// (`not_copy(Future<T>)`), so compiled code moves it rather than sharing
    /// it.
    Future(Box<FutureFrame>),
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
            // Field names live only in source; the runtime value renders
            // its fields positionally (used in tests and error messages).
            Value::Struct(fields) => {
                write!(f, "{{ ")?;
                for (i, field) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{field}")?;
                }
                write!(f, " }}")
            }
            Value::Arr(elems) => {
                write!(f, "[")?;
                for (i, elem) in elems.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{elem}")?;
                }
                write!(f, "]")
            }
            // `start..end`, `end` exclusive (matches `std.collection.range`).
            Value::Range { start, end } => write!(f, "{start}..{end}"),
            Value::Unit => write!(f, "unit"),
            Value::Func(id) => write!(f, "<func #{}>", id.0),
            // A future has no user-visible payload; the frame contents would be
            // noise (and can be large).
            Value::Future(_) => write!(f, "<future>"),
        }
    }
}
