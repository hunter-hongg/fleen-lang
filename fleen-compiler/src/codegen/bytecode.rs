//! Bytecode module structures and instruction encoding.

use std::fmt;

pub use crate::lower::mir::{FuncId, GlobalId};

/// Unique ID into the constant pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstId(pub u32);

impl fmt::Display for ConstId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "const#{}", self.0)
    }
}

/// A constant-pool entry. `Float` hashes/compares by bit pattern so it can
/// live in a `HashMap` (`f64` is not `Eq`/`Hash` on its own). Consequence:
/// `0.0` and `-0.0` are distinct constants, and NaNs with different bit
/// patterns are distinct — this is intentional (bit-exact interning).
#[derive(Debug, Clone)]
pub enum Const {
    Int(i64),
    Float(f64),
    Str(Box<str>),
}

impl PartialEq for Const {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Const::Int(a), Const::Int(b)) => a == b,
            // Bit-exact, *not* `f64::eq`: `0.0 == -0.0` under `f64`, but the
            // `Hash` impl below hashes floats by `to_bits()`. Equality and
            // hashing must agree, otherwise `ConstPool::intern` returns the
            // wrong existing id whenever the two keys land in the same bucket
            // — which depends on the map's per-process random seed, i.e. a
            // nondeterministic dedup (observed as a flaky
            // `float_neg_zero_and_zero_are_distinct_constants` failure).
            (Const::Float(a), Const::Float(b)) => a.to_bits() == b.to_bits(),
            (Const::Str(a), Const::Str(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for Const {}

impl std::hash::Hash for Const {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Const::Int(v) => {
                0u8.hash(state);
                v.hash(state);
            }
            Const::Float(v) => {
                1u8.hash(state);
                v.to_bits().hash(state);
            }
            Const::Str(s) => {
                2u8.hash(state);
                s.hash(state);
            }
        }
    }
}

/// Span → source position map entry.
///
/// Generated per instruction by codegen since 0.0.2 (U07); the binary
/// layout was reserved in 0.0.1 (§9) and is unchanged. Instructions whose
/// MIR span is [`Span::SYNTHETIC`](crate::lexer::Span::SYNTHETIC) (synthesized
/// glue with no source location) and block terminators carry **no** entry —
/// consumers must treat a missing offset as "no known location".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpanEntry {
    /// Instruction start byte offset, relative to the function's `code`.
    pub offset: u32,
    /// Source start byte offset.
    pub start: u32,
    /// Source end byte offset (exclusive).
    pub end: u32,
}

/// A compiled function.
#[derive(Debug, Clone, PartialEq)]
pub struct Func {
    pub name: ConstId,
    pub params: u16,
    pub locals: u16,
    pub code: Box<[u8]>,
    pub span_map: Box<[SpanEntry]>,
    pub is_builtin: bool,
}

/// A global variable declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct Global {
    pub name: ConstId,
    pub mutable: bool,
}

/// The compiled module.
#[derive(Debug, Clone, PartialEq)]
pub struct Module {
    pub version: u16,
    pub constants: Vec<Const>,
    pub functions: Vec<Func>,
    pub globals: Vec<Global>,
    pub entry: FuncId,
    /// Struct declarations in id order (0.0.3 V02): the `struct_defs`
    /// table. Populated only in v3+ modules; v1/v2 modules keep it empty
    /// (the appended table is read only for version >= 3).
    pub struct_defs: Vec<StructDefMeta>,
}

/// One `struct_defs` entry (0.0.3 V02, PLAN §5.5): the field type tags in
/// declaration order. The table is shape-level — verify cross-checks
/// `StructNew`'s operand against the field count; field *offsets* of
/// `GetField`/`SetField*` cannot be validated statically (the encoding
/// carries no struct id on those instructions) and are checked at runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDefMeta {
    pub field_types: Box<[FieldTypeTag]>,
}

/// Shape-level field type tag (0.0.3 V02). Deliberately coarse: it names
/// the *kind* of a field, not the full type (struct-typed fields are all
/// `Struct` regardless of which struct), which is all verify's count/shape
/// checks need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum FieldTypeTag {
    Int = 0,
    Float = 1,
    Bool = 2,
    Str = 3,
    Unit = 4,
    Func = 5,
    Result = 6,
    Array = 7,
    Box = 8,
    Ref = 9,
    Struct = 10,
    /// `Range` (0.0.3 V04): a builtin Copy type.
    Range = 11,
}

impl FieldTypeTag {
    /// The tag of a field type.
    pub fn of(ty: &crate::typeck::typed_hir::Type) -> Self {
        use crate::typeck::typed_hir::Type;
        match ty {
            Type::Int => FieldTypeTag::Int,
            Type::Float => FieldTypeTag::Float,
            Type::Bool => FieldTypeTag::Bool,
            Type::String => FieldTypeTag::Str,
            Type::Unit => FieldTypeTag::Unit,
            Type::Func(_, _) => FieldTypeTag::Func,
            Type::Result(_, _) => FieldTypeTag::Result,
            Type::Array(_) => FieldTypeTag::Array,
            Type::Box(_) => FieldTypeTag::Box,
            Type::Ref(_) => FieldTypeTag::Ref,
            Type::Struct(_) => FieldTypeTag::Struct,
            Type::Range => FieldTypeTag::Range,
            // `Future` can never be a *struct field* (0.0.4): the disk-format
            // tags are untouched (PLAN §6), and a suspendable coroutine frame
            // is not serializable as a field. The Unit mapping keeps the
            // match total; reaching it is a typeck/lower bug.
            Type::Future(_) => {
                debug_assert!(
                    false,
                    "Future type reached struct-field codegen: a Future cannot be a struct field"
                );
                FieldTypeTag::Unit
            }
            // `Var` is only in std builtin signatures (V06); it is always
            // resolved to a concrete type before codegen ever runs.
            Type::Var(_) => {
                debug_assert!(
                    false,
                    "Type::Var reached codegen: schematic signatures must be resolved before codegen"
                );
                FieldTypeTag::Unit
            }
            // `Unsupported` never survives typeck, so it cannot reach codegen;
            // the Unit mapping keeps the match total for release builds.
            Type::Unsupported(_) => {
                debug_assert!(
                    false,
                    "Unsupported type reached codegen: `Unsupported` cannot survive typeck"
                );
                FieldTypeTag::Unit
            }
            // `channel<T>` can never be a struct field (T07d: non-storable);
            // reaching here is a typeck bug. The Unit mapping keeps the match
            // total; the debug_assert will catch it in debug builds.
            Type::Channel(_) => {
                debug_assert!(
                    false,
                    "Channel type reached struct-field codegen: a channel cannot be a struct field (T07d)"
                );
                FieldTypeTag::Unit
            }
        }
    }
}

/// Codegen output alias (SPEC.md §6 `codegen` stage).
pub type Bytecode = Module;

/// Virtual-machine opcodes (BYTECODE.md §4.2 number ranges).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    // 0x00–0x0F constants & stack
    Const = 0x00,
    True = 0x01,
    False = 0x02,
    Unit = 0x03,
    Pop = 0x04,
    Dup = 0x05,
    // 0x10–0x1F locals / globals
    LoadLocal = 0x10,
    StoreLocal = 0x11,
    LoadGlobal = 0x12,
    StoreGlobal = 0x13,
    // 0x20–0x2F int arithmetic
    IAdd = 0x20,
    ISub = 0x21,
    IMul = 0x22,
    IDiv = 0x23,
    IMod = 0x24,
    // 0x30–0x3F float arithmetic
    FAdd = 0x30,
    FSub = 0x31,
    FMul = 0x32,
    FDiv = 0x33,
    // 0x40–0x4F compare / logic
    Eq = 0x40,
    Ne = 0x41,
    Lt = 0x42,
    Gt = 0x43,
    Le = 0x44,
    Ge = 0x45,
    Not = 0x46,
    NegI = 0x47,
    NegF = 0x48,
    // 0x50–0x5F control flow
    Jump = 0x50,
    JumpIfFalse = 0x51,
    JumpIfTrue = 0x52,
    // 0x60–0x6F calls
    Call = 0x60,
    LoadFunc = 0x61,
    CallValue = 0x62,
    Return = 0x63,
    // 0x70–0x7F choose helpers
    BindMatch = 0x70,
    // 0x80–0x83 box / ref (0.0.2 U07, §5.9)
    AllocBox = 0x80,
    DerefBox = 0x81,
    StoreDerefBox = 0x82,
    MakeRefLocal = 0x83,
    // 0x90–0x93 move / clone (0.0.2 U07, §5.10)
    DupDeep = 0x90,
    MoveLocal = 0x91,
    CloneLocal = 0x92,
    CloneGlobal = 0x93,
    // 0xA0–0xA4 Result / error handling (0.0.2 U07, §5.11)
    PackOk = 0xA0,
    PackErr = 0xA1,
    IsErr = 0xA2,
    UnwrapOk = 0xA3,
    UnwrapErr = 0xA4,
    // 0xA5+ further 0.0.2 extensions
    /// Convert a scalar (int/float/bool) on the stack to a fresh owned string.
    /// 0.0.2 U13 (F8): `e as string`.
    ToStr = 0xA5,
    // 0xB0–0xB3 structs (0.0.3 V02, bytecode v3)
    /// Pop n field values (n = struct table entry) into struct `struct_id`.
    StructNew = 0xB0,
    /// Pop the struct, push a copy of field `offset` (deep for owned fields).
    GetField = 0xB1,
    /// Overwrite field `offset` of the struct in local `slot` (v →).
    SetFieldLocal = 0xB2,
    /// Overwrite field `offset` of the struct held by global `gid` (v →).
    SetFieldGlobal = 0xB3,
    // 0xB4–0xB8 arrays (0.0.3 V03, bytecode v3)
    /// Pop n element values (n = operand) into a fresh array (e₁…eₙ → a).
    ArrNew = 0xB4,
    /// Push a copy of the array's length; the array stays on the stack
    /// (a → a int).
    ArrLen = 0xB5,
    /// Pop the index, then the array; push a bounds-checked **copy** of the
    /// element (a i → a v). Out of bounds → `IndexOutOfRange` at runtime.
    ArrGet = 0xB6,
    /// Overwrite element `index` (popped second) of the array in local
    /// `slot` (v i →), bounds-checked.
    ArrSetLocal = 0xB7,
    /// Same, for the array held by global `gid` (v i →), bounds-checked.
    ArrSetGlobal = 0xB8,
    // 0xB9–0xBB range (0.0.3 V04, bytecode v3)
    /// Pop the end bound, then the start bound; build a `Range` value
    /// (start end → r). Not reachable from source until V06
    /// (`std.collection.range`).
    MakeRange = 0xB9,
    /// Push a copy of a Range's start bound; the range stays on the stack
    /// (r → r int). Used once, before the loop, by the `for` desugar.
    RangeStart = 0xBA,
    /// Push a copy of a Range's end bound; the range stays on the stack
    /// (r → r int). Used once, before the loop, by the `for` desugar.
    RangeEnd = 0xBB,
    // 0xC0–0xC6 async / channel (0.0.4, bytecode v4; 0xC0–0xC3 land in T05,
    // 0xC4–0xC6 in T07). All inside the reserved 0xBC–0xFF region, so v1–v3
    // encodings are untouched (BYTECODE.md §10 compatibility promise).
    /// Async call boundary (0.0.4 T05): at the head of an `async func`'s
    /// body it packages the frame's parameter slots into a not-yet-started
    /// coroutine frame and delivers the `Future` value back to the caller.
    /// Operand-less; body entry is always the next instruction (pc + 1).
    AsyncStart = 0xC0,
    /// Suspension point (0.0.4 T05): snapshots the current coroutine frame
    /// and yields `Pending` to the driver. Unreachable from 0.0.4 T05
    /// lowering (no suspension source yet); the real yield lands with the
    /// T06 executor and T07 channels.
    Suspend = 0xC1,
    /// Drive a `Future` to its next state, pushing a Ready/Pending tag
    /// (0.0.4 T05; the T06 executor completes the Pending protocol).
    Poll = 0xC2,
    /// `Poll` + expect Ready + unwrap the payload: drive a `Future` to
    /// completion and leave its `T` on the operand stack (0.0.4 T05).
    /// Stack effect: `F → T` (Δ0, min 1).
    Await = 0xC3,
    // 0xC4–0xC7 channel (0.0.4 T07). All inside the reserved 0xBC–0xFF
    // region.
    /// Create a new channel with the given payload type. The payload type is
    /// carried in the MIR instruction only (for T08's defensive run-time
    /// backstop) — the encoded instruction has **no operand**.
    /// Stack effect: cap → ch (Δ0, min 1).
    ChannelNew = 0xC4,
    /// Send a value through a channel; may suspend if full.
    /// Stack effect: ch v → unit (Δ-1, min 2).
    ChannelSend = 0xC5,
    /// Receive a value from a channel; may suspend if empty.
    /// Stack effect: ch → v (Δ0, min 1).
    ChannelRecv = 0xC6,
    /// Split a channel handle: ch → ch2 (Δ0, min 1).
    ChannelSplit = 0xC7,
}

/// Lowest opcode number reserved for the 0.0.2 (v2) instruction set.
///
/// Every v1 opcode is below this and every v2 opcode at or above it, which
/// lets `fleen-verify` reject v2 instructions in a v1 module with one
/// comparison (BYTECODE.md §4.2 / §10).
pub const V2_OPCODE_BASE: u8 = 0x80;

/// Lowest opcode number reserved for the 0.0.3 (v3) instruction set
/// (structs 0xB0–0xB3, arrays 0xB4–0xB8; range follows in V04, 0xB9–0xBB).
pub const V3_OPCODE_BASE: u8 = 0xB0;

/// Lowest opcode number reserved for the 0.0.4 (v4) instruction set
/// (async / channel 0xC0–0xC6). v4 opcodes are at or above this.
pub const V4_OPCODE_BASE: u8 = 0xC0;

impl Opcode {
    /// Number of operand bytes follow the opcode (BYTECODE.md §4.1).
    ///
    /// `MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal` carry a
    /// `u16` operand (3-byte instruction); every other v2 instruction is
    /// operand-less. This table and [`Encoder::emit_instr`](crate::codegen::encoder::Encoder)
    /// must agree — `emit_instr` asserts it.
    pub fn operand_len(self) -> usize {
        match self {
            Opcode::Const => 4,
            Opcode::LoadLocal
            | Opcode::StoreLocal
            | Opcode::LoadGlobal
            | Opcode::StoreGlobal
            | Opcode::Jump
            | Opcode::JumpIfFalse
            | Opcode::JumpIfTrue
            | Opcode::Call
            | Opcode::LoadFunc
            | Opcode::BindMatch
            | Opcode::MakeRefLocal
            | Opcode::MoveLocal
            | Opcode::CloneLocal
            | Opcode::CloneGlobal
            | Opcode::StructNew
            | Opcode::GetField
            | Opcode::ArrNew
            | Opcode::ArrSetLocal
            | Opcode::ArrSetGlobal => 2,
            // `SetFieldLocal` / `SetFieldGlobal`: slot (or gid) + offset.
            Opcode::SetFieldLocal | Opcode::SetFieldGlobal => 4,
            Opcode::CallValue => 1,
            _ => 0,
        }
    }

    /// Total encoded instruction length in bytes.
    pub fn instr_len(self) -> usize {
        1 + self.operand_len()
    }

    /// Decode an opcode byte.
    pub fn from_byte(b: u8) -> Option<Opcode> {
        Some(match b {
            0x00 => Opcode::Const,
            0x01 => Opcode::True,
            0x02 => Opcode::False,
            0x03 => Opcode::Unit,
            0x04 => Opcode::Pop,
            0x05 => Opcode::Dup,
            0x10 => Opcode::LoadLocal,
            0x11 => Opcode::StoreLocal,
            0x12 => Opcode::LoadGlobal,
            0x13 => Opcode::StoreGlobal,
            0x20 => Opcode::IAdd,
            0x21 => Opcode::ISub,
            0x22 => Opcode::IMul,
            0x23 => Opcode::IDiv,
            0x24 => Opcode::IMod,
            0x30 => Opcode::FAdd,
            0x31 => Opcode::FSub,
            0x32 => Opcode::FMul,
            0x33 => Opcode::FDiv,
            0x40 => Opcode::Eq,
            0x41 => Opcode::Ne,
            0x42 => Opcode::Lt,
            0x43 => Opcode::Gt,
            0x44 => Opcode::Le,
            0x45 => Opcode::Ge,
            0x46 => Opcode::Not,
            0x47 => Opcode::NegI,
            0x48 => Opcode::NegF,
            0x50 => Opcode::Jump,
            0x51 => Opcode::JumpIfFalse,
            0x52 => Opcode::JumpIfTrue,
            0x60 => Opcode::Call,
            0x61 => Opcode::LoadFunc,
            0x62 => Opcode::CallValue,
            0x63 => Opcode::Return,
            0x70 => Opcode::BindMatch,
            0x80 => Opcode::AllocBox,
            0x81 => Opcode::DerefBox,
            0x82 => Opcode::StoreDerefBox,
            0x83 => Opcode::MakeRefLocal,
            0x90 => Opcode::DupDeep,
            0x91 => Opcode::MoveLocal,
            0x92 => Opcode::CloneLocal,
            0x93 => Opcode::CloneGlobal,
            0xA0 => Opcode::PackOk,
            0xA1 => Opcode::PackErr,
            0xA2 => Opcode::IsErr,
            0xA3 => Opcode::UnwrapOk,
            0xA4 => Opcode::UnwrapErr,
            0xA5 => Opcode::ToStr,
            0xB0 => Opcode::StructNew,
            0xB1 => Opcode::GetField,
            0xB2 => Opcode::SetFieldLocal,
            0xB3 => Opcode::SetFieldGlobal,
            0xB4 => Opcode::ArrNew,
            0xB5 => Opcode::ArrLen,
            0xB6 => Opcode::ArrGet,
            0xB7 => Opcode::ArrSetLocal,
            0xB8 => Opcode::ArrSetGlobal,
            0xB9 => Opcode::MakeRange,
            0xBA => Opcode::RangeStart,
            0xBB => Opcode::RangeEnd,
            // 0.0.4 (v4): async opcodes (T05).
            0xC0 => Opcode::AsyncStart,
            0xC1 => Opcode::Suspend,
            0xC2 => Opcode::Poll,
            0xC3 => Opcode::Await,
            // 0.0.4 (v4): channel opcodes (T07).
            0xC4 => Opcode::ChannelNew,
            0xC5 => Opcode::ChannelSend,
            0xC6 => Opcode::ChannelRecv,
            0xC7 => Opcode::ChannelSplit,
            _ => return None,
        })
    }
}
