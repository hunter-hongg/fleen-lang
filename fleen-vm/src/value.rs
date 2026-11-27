//! Runtime value representation (BYTECODE.md §3.2).

use std::fmt;

use fleen_compiler::codegen::FuncId;

/// A map's storage (0.0.11 T05, D8): insertion-ordered keys backed by a
/// `HashMap` for O(1) lookup, plus an `order` Vec preserving declaration
/// order for `keys` / `values`.
#[derive(Debug, Clone, PartialEq)]
pub struct MapCell {
    pub order: Vec<Value>,
    pub data: std::collections::HashMap<MapKey, Value>,
}

/// Hashable key wrapper for `Value` keys (int/bool/string only, D5).
///
/// Clippy's `mutable_key_type` fires because `Value` (and the `MapCell`
/// / `ChannelInner` variants) contain `Rc` / `RefCell`. Map keys are
/// restricted to `int` / `bool` / `string` by typeck (D5), so the
/// interior-mutability-bearing variants can never be keys at runtime.
#[allow(clippy::mutable_key_type)]
#[derive(Debug, Clone)]
pub struct MapKey(pub Value);

impl PartialEq for MapKey {
    fn eq(&self, other: &Self) -> bool {
        match (&self.0, &other.0) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for MapKey {}

impl std::hash::Hash for MapKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match &self.0 {
            Value::Int(v) => {
                0u8.hash(state);
                v.hash(state);
            }
            Value::Bool(v) => {
                1u8.hash(state);
                v.hash(state);
            }
            Value::Str(v) => {
                2u8.hash(state);
                v.hash(state);
            }
            _ => {}
        }
    }
}

use crate::frame::FutureFrame;
use std::cell::RefCell;
use std::rc::Rc;

/// Runtime value of `channel<T>` (0.0.4 T07).
///
/// The buffer is shared by all handles to the same channel via `Rc<RefCell>`.
/// This is a deliberate, documented exception to SPEC §11's blanket `Rc`/
/// `RefCell` prohibition — same reasoning as `Value::Future` (T04): the
/// aliasing *is* the language semantics, not an implementation shortcut.
/// `size_of::<Value>()` is unchanged (one pointer).
#[derive(Debug, Clone, PartialEq)]
pub struct ChannelInner {
    /// Bounded buffer capacity. 0 = rendezvous (no buffer at all).
    pub cap: usize,
    /// Buffered values, FIFO (only used when `cap > 0`).
    pub buffer: std::collections::VecDeque<Value>,
    /// Tasks blocked on `send`, FIFO arrival order, each still holding the
    /// payload it was sending (the value moves into the queue with the
    /// parked chain — a parked sender owns its payload until a receiver
    /// takes it). Used for both cap=0 (rendezvous) and cap>0 (buffer full).
    /// `VecDeque` because the queue is consumed from the *front* (`pop_front`):
    /// a `Vec` + `.pop()` would wake the most recent sender first (LIFO).
    pub senders: std::collections::VecDeque<(Value, Vec<FutureFrame>)>,
    /// Tasks blocked on `recv` waiting for a value, FIFO arrival order.
    /// A parked receiver's chain resumes *after* its `recv` instruction;
    /// the waking side injects the payload into the innermost frame's
    /// operands (`Vm::inject_value_into_chain`).
    pub receivers: std::collections::VecDeque<Vec<FutureFrame>>,
    /// Payload kind of the values this channel has accepted so far
    /// (0.0.4 T08 defensive backstop). `None` until the first payload; the
    /// first accepted value fixes the kind and every later `send` must match
    /// it. Coarser than the static type on purpose — `Ok`/`Err` collapse to
    /// `Result`, every struct is `Struct` — because typeck owns the real
    /// uniformity rule; this only catches malformed bytecode that pushes
    /// heterogeneous payloads into one channel, where the failure would
    /// otherwise surface delayed and mislocated at the receiver's next typed
    /// instruction.
    pub payload_kind: Option<PayloadKind>,
}

/// The runtime "kind" of a [`Value`] at the granularity of one variant per
/// *static* type — `Ok`/`Err` collapse to `Result` because they are the two
/// constructors of one type (0.0.4 T08).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadKind {
    Int,
    Float,
    Bool,
    Str,
    Boxed,
    Ref,
    Result,
    Struct,
    Arr,
    Range,
    Unit,
    Func,
    Future,
    Channel,
    Ptr,
    Cell,
    Enum,
    Map,
}

impl PayloadKind {
    /// The kind's user-facing name (used in the payload-mismatch message).
    pub fn name(self) -> &'static str {
        match self {
            PayloadKind::Int => "int",
            PayloadKind::Float => "float",
            PayloadKind::Bool => "bool",
            PayloadKind::Str => "string",
            PayloadKind::Boxed => "box",
            PayloadKind::Ref => "ref",
            PayloadKind::Result => "Result",
            PayloadKind::Struct => "struct",
            PayloadKind::Arr => "array",
            PayloadKind::Range => "Range",
            PayloadKind::Unit => "unit",
            PayloadKind::Func => "function",
            PayloadKind::Future => "Future",
            PayloadKind::Channel => "channel",
            PayloadKind::Ptr => "ptr",
            PayloadKind::Cell => "cell",
            PayloadKind::Enum => "enum",
            PayloadKind::Map => "map",
        }
    }
}

impl Value {
    /// The value's [`PayloadKind`] (0.0.4 T08) — see the enum's doc for why
    /// `Ok`/`Err` collapse.
    pub fn payload_kind(&self) -> PayloadKind {
        match self {
            Value::Int(_) => PayloadKind::Int,
            Value::Float(_) => PayloadKind::Float,
            Value::Bool(_) => PayloadKind::Bool,
            Value::Str(_) => PayloadKind::Str,
            Value::Boxed(_) => PayloadKind::Boxed,
            Value::Ref { .. } => PayloadKind::Ref,
            Value::Ok(_) | Value::Err(_) => PayloadKind::Result,
            Value::Struct(_) => PayloadKind::Struct,
            Value::Arr(_) => PayloadKind::Arr,
            Value::Range { .. } => PayloadKind::Range,
            Value::Unit => PayloadKind::Unit,
            Value::Func { .. } => PayloadKind::Func,
            Value::Future(_) => PayloadKind::Future,
            Value::Channel(_) => PayloadKind::Channel,
            Value::Ptr(_) => PayloadKind::Ptr,
            Value::Cell(_) => PayloadKind::Cell,
            Value::Enum { .. } => PayloadKind::Enum,
            Value::Map(_) => PayloadKind::Map,
        }
    }
}

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
    /// User-defined enum value (0.0.6 T06): the discriminant variant index
    /// plus the variant's payload values in declaration order. Owned (the
    /// payload values are moved in). The enum id is a compile-time fact
    /// (from the `EnumNew` operand), so only the variant id rides the value
    /// — `EnumPayload` checks it to guard against forged bytecode.
    Enum { variant: u16, payload: Box<[Value]> },
    /// Array value (0.0.3 V03): elements in source order. **Always owned**
    /// (D3 — even `[int]`); `Eq` on an array is a typeck-level error
    /// (defensive error in the VM).
    Arr(Box<[Value]>),
    /// Map value (0.0.11 T05, D8): insertion-ordered map. Keys are drawn
    /// from the `{int, bool, string}` whitelist (D5). The `KeyHash` wrapper
    /// makes the key hashable while preserving declaration order via `order`.
    Map(Box<MapCell>),
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
    /// First-class function reference. A *bare* function value carries an
    /// empty capture list; a value produced by `PartialApply` (0.0.8 T08,
    /// F9) carries the already-applied arguments so that a later
    /// `CallValue` can consume `captures.len() + argc` values together.
    /// The capture list is runtime-only (D14): like `Future`/`Channel`/
    /// `Ptr` it is never serialized — there is no `.flnc` tag, no
    /// constant-pool or `FieldTypeTag` surface. `Func` is unconditionally
    /// Copy at the language level (D11), so any copy consumption
    /// (`clone`, `Dup`, array/struct/global stores) deep-copies the list
    /// via the derived `Clone` (owned kinds duplicate their heap data; a
    /// `Future`/`Channel`/`Ptr` capture would alias — the same
    /// `deep_fork_kind` backstops that reject a copy, mirroring the
    /// container rules for arrays and enum payloads).
    Func {
        id: FuncId,
        /// The already-applied arguments (D11/D13). Boxed like `Enum`'s
        /// payload list and `Arr` so the variant does not inflate `Value`.
        captures: Box<[Value]>,
    },
    /// Suspendable coroutine frame — the runtime value of a `Future<T>`
    /// (0.0.4 D1/T04).  Boxed so that the frame's two `Vec`s and its state do
    /// not inflate every `Value` the VM touches (D1 also calls a future a heap
    /// object).  Like `Ref` it is **not serialized**: there is no `.flnc` tag,
    /// no constant-pool or `FieldTypeTag` change.  Typeck makes it non-Copy
    /// (`not_copy(Future<T>)`), so compiled code moves it rather than sharing
    /// it.
    Future(Box<FutureFrame>),
    /// Channel handle (0.0.4 T07).  Shared buffer via `Rc<RefCell<ChannelInner>>`
    /// — aliasing is the language semantics (SPEC §11 exception, like Future).
    Channel(Rc<RefCell<ChannelInner>>),
    /// Pointer value (0.0.5 T06): a stable heap cell handle.  `addr x`
    /// promotes `x`'s slot to a `Rc<RefCell<Value>>` cell (promote-on-addr,
    /// PLAN §4) and this variant holds a handle to that cell.  Not serialized
    /// (no `.flnc` tag), same rule as Future/Channel.  Typeck makes `ptr<T>`
    /// non-Copy and move-only (D1), so a bare *handle load* clones the `Rc`
    /// (BYTECODE.md §5.16) while `clone p` is a typeck error.
    Ptr(Rc<RefCell<Value>>),
    /// Internal marker stored in a local/global slot that `AddrOf` promoted
    /// to a stable heap cell (0.0.5 T06).  Never appears on the operand stack
    /// from compiled code: local/global access paths dereference it
    /// transparently so reads/writes before and after promotion are
    /// semantically unchanged.  Holding the `Rc` here — not a bare stack index
    /// — is what keeps frames movable across suspend/resume.
    Cell(Rc<RefCell<Value>>),
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
            Value::Map(cell) => {
                write!(f, "{{")?;
                for (i, k) in cell.order.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    let v = cell
                        .data
                        .get(&MapKey(k.clone()))
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    write!(f, "{k} -> {v}")?;
                }
                write!(f, "}}")
            }
            // `start..end`, `end` exclusive (matches `std.collection.range`).
            Value::Range { start, end } => write!(f, "{start}..{end}"),
            Value::Unit => write!(f, "unit"),
            // The capture list is runtime state, not user-visible text (T08):
            // a captured partial renders exactly like its bare function.
            Value::Func { id, .. } => write!(f, "<func #{}>", id.0),
            // A future has no user-visible payload; the frame contents would be
            // noise (and can be large).
            Value::Future(_) => write!(f, "<future>"),
            // A channel handle — renders as <channel cap=N>.
            Value::Channel(inner) => write!(f, "<channel cap={}>", inner.borrow().cap),
            Value::Enum { variant, payload } => {
                write!(f, "Enum({variant}")?;
                if !payload.is_empty() {
                    write!(f, ", ")?;
                    for (i, v) in payload.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{v}")?;
                    }
                }
                write!(f, ")")
            }
            Value::Ptr(_) => write!(f, "<ptr>"),
            Value::Cell(_) => write!(f, "<cell>"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 0.0.6 T07: `clone` deep-copies enum payloads — the derived `Clone`
    /// copies the `Box<[Value]>` recursively, so a nested array payload is
    /// fully forked, never shared.
    #[test]
    fn enum_clone_deep_copies_payloads() {
        let e = Value::Enum {
            variant: 1,
            payload: vec![Value::Arr(
                vec![Value::Int(1), Value::Int(2)].into_boxed_slice(),
            )]
            .into_boxed_slice(),
        };
        let copy = e.clone();
        // Mutate the original's inner array via pattern match reconstruction.
        let Value::Enum { payload, .. } = &e else {
            unreachable!("just built an enum")
        };
        let Value::Arr(elems) = &payload[0] else {
            unreachable!("just built an array payload")
        };
        assert_eq!(elems.len(), 2);
        // The copy is structurally equal but independent (deep copy).
        assert_eq!(e, copy);
        match (&e, &copy) {
            (Value::Enum { payload: a, .. }, Value::Enum { payload: b, .. }) => {
                // Pointer-distinct payload buffers, and the inner array is
                // pointer-distinct too: the copy is deep, not just one level.
                assert_ne!(a.as_ptr(), b.as_ptr());
                let (Value::Arr(x), Value::Arr(y)) = (&a[0], &b[0]) else {
                    unreachable!("payload is an array")
                };
                assert_ne!(x.as_ptr(), y.as_ptr());
            }
            _ => unreachable!("both are enums"),
        }
    }

    /// 0.0.6 T07: `PayloadKind::Enum` rides the channel payload-kind
    /// backstop (0.0.4 T08) — the first enum send fixes the kind.
    #[test]
    fn enum_payload_kind_is_reported() {
        let e = Value::Enum {
            variant: 0,
            payload: Box::new([]),
        };
        assert_eq!(e.payload_kind(), PayloadKind::Enum);
        assert_eq!(PayloadKind::Enum.name(), "enum");
    }

    /// 0.0.6 T07: `Display` renders the variant index plus payloads
    /// positionally (names live only in source, like struct fields).
    #[test]
    fn enum_display_renders_variant_and_payloads() {
        let zero = Value::Enum {
            variant: 0,
            payload: Box::new([]),
        };
        assert_eq!(zero.to_string(), "Enum(0)");
        let two = Value::Enum {
            variant: 2,
            payload: vec![Value::Int(7), Value::Str("hi".into())].into_boxed_slice(),
        };
        assert_eq!(two.to_string(), "Enum(2, 7, hi)");
    }

    /// 0.0.8 T08 (D11): cloning a captured function value deep-copies the
    /// capture list — owned payloads (here a `Str`) are pointer-distinct in
    /// the copy, so two copies of a partial each own an independent
    /// snapshot. `PayloadKind::Func` is unchanged, and a captured partial
    /// renders exactly like its bare function (the capture list is not
    /// user-visible).
    #[test]
    fn func_clone_deep_copies_captures_and_display_is_bare() {
        let captured = Value::Func {
            id: FuncId(3),
            captures: vec![Value::Str("snapshot".into()), Value::Int(9)].into(),
        };
        let copy = captured.clone();
        assert_eq!(captured, copy);
        match (&captured, &copy) {
            (
                Value::Func {
                    id: ia,
                    captures: ca,
                },
                Value::Func {
                    id: ib,
                    captures: cb,
                },
            ) => {
                assert_eq!(ia, ib);
                // Pointer-distinct capture buffers: the copy is deep, not a
                // shared list.
                assert_ne!(ca.as_ptr(), cb.as_ptr());
                let (Value::Str(x), Value::Str(y)) = (&ca[0], &cb[0]) else {
                    unreachable!("first capture is a string")
                };
                // Distinct heap strings, same content.
                assert_ne!(x.as_ptr(), y.as_ptr());
                assert_eq!(x, y);
            }
            _ => unreachable!("both are funcs"),
        }
        // Bare and captured render identically; captures stay runtime-only.
        let bare = Value::Func {
            id: FuncId(3),
            captures: Vec::new().into(),
        };
        assert_eq!(captured.to_string(), bare.to_string());
        assert_eq!(captured.to_string(), "<func #3>");
        assert_eq!(captured.payload_kind(), PayloadKind::Func);
    }
}
