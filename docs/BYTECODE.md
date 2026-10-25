# Fleen Bytecode Specification v0.0.4

> **Fast, Simple, Safe, Intuitive**
> This document defines the output format of the Fleen compiler (the `codegen`
> stage) and the execution model of the Fleen VM (`fleen-vm`).
> The compiler and the VM must both adhere to this document.
>
> **v0.0.2 additions**: ownership instructions (0x80–0x93), Result instructions (0xA0–0xA4),
> the `Value` ownership representation, and `span_map` generation.
> **v0.0.3 additions**: aggregation instructions — struct (0xB0–0xB3), array (0xB4–0xB8),
> and Range/iteration (0xB9–0xBB). `Value::Struct` / `Value::Arr` / `Value::Range` join the
> value model. Design decisions are in `docs/0.0.3/PLAN.md`.
> **v0.0.4 additions**: async / channel instructions (0xC0–0xC7) — coroutine-frame futures
> (`AsyncStart` / `Suspend` / `Poll` / `Await`) and message-passing channels
> (`ChannelNew` / `ChannelSend` / `ChannelRecv` / `ChannelSplit`). `Value::Future` /
> `Value::Channel` join the value model as non-serialized runtime variants, and the
> `.flnc` version bumps to 4 (no layout change — no new tables). Design decisions are
> in `docs/0.0.4/PLAN.md`.

---

## 1. Overview

Fleen compiles to a **custom stack-based bytecode**, executed by a purpose-built VM.

- **Execution model**: stack machine; operands are implicitly on top of the stack
- **Encoding**: byte stream, little-endian
- **Instruction format**: `opcode: u8` + fixed- or variable-length operands
- **Structure**: one bytecode module (`Module`) = constant pool + function table + global variable table + `struct_defs` (v3) + entry point
- **No GC, no exception table, no meta-circular instructions**: consistent with the language design
  (ownership is carried by v2 instructions such as `AllocBox` /
  `MoveLocal` / `CloneLocal`, §5.9–5.10; release is done via deterministic destruction of values)

### Stage Handoff

```
Lower (Typed HIR → MIR)    MIR is a linear instruction sequence after control-flow flattening
Codegen (MIR → Bytecode)   MIR instructions map 1:1 to bytecode + constant pool indices
Execute (Bytecode → result)  VM interprets and executes
```

Rules:
- Codegen is a **pure function**: `pub fn codegen(mir: Mir) -> Result<Bytecode, CodegenError>`
- MIR and bytecode instructions correspond **essentially one-to-one**; no peephole optimization (later versions may add it, but the format stays unchanged)
- The VM does no type checking — type safety is guaranteed by the `typeck` stage; the VM trusts the bytecode

---

## 2. Module Format

```rust
pub struct Module {
    pub version: u16,          // bytecode version: 0.0.4 writes 4, readers accept {1, 2, 3, 4}
    pub constants: Vec<Const>, // constant pool
    pub functions: Vec<Func>,  // function table
    pub globals: Vec<Global>,  // global variable table
    pub struct_defs: Vec<StructDef>, // struct field table (0.0.3, present when version >= 3, §2)
    pub entry: FuncId,         // entry function: usually main; synthesized __init__ when globals exist
}
```

> **`struct_defs` (0.0.3 V02)**: one entry per user struct, giving verify exactly
> `StructNew`'s field count and each field's type tag (see §2 "Struct Definitions"). It is the
> single appended table of bytecode version 3 (§9), so it is present only when `version >= 3`.

Rules:
- **Constant pool deduplication**: identical constants are stored once and referenced by index `ConstId(u32)`
- **Functions are numbered consecutively from 0**, referenced by `FuncId(u32)`
- **Global variables are numbered consecutively from 0**, referenced by `GlobalId(u32)`
- All IDs are newtypes to avoid confusion (see SPEC.md §5)
- **The entry point is specified by `entry`**, not by function-name convention; when the module
  **has global variables**, `entry` points to the synthesized `__init__` (see below)

### Entry and Global Initialization

Global variables have no separate "initialization section"; their initialization code is carried by a
**synthesized function `__init__`**:

- **No globals**: `entry` points directly to `main`.
- **With globals**: `codegen` additionally synthesizes a function `__init__` (`params = 0`, `locals = 0`, `is_builtin = false`) and places it in the function table (its `FuncId` comes after all functions).
  - `__init__` executes each global's initialization code in **declaration order** (`MirGlobal::init` in MIR); the initialization expression leaves one value on the stack, immediately followed by a `StoreGlobal gid` that stores it into the corresponding global.
  - After all stores, it executes `Call main_id`, then `Return` (passing `main`'s return value through unchanged).
  - `entry` points to `__init__`.
- **`main` is still looked up by name**: if the function table has no function named `main`, `codegen` errors.
- The VM's execution contract is unchanged: **execute `entry` first**. When `entry` is `__init__`, it calls `main` after initialization completes.

> A `StoreGlobal` write to a `mutable: false` global is only allowed inside `__init__` (the entry function), as initialization; a write to a `const` global from any other function is a verify failure (see §8).

### Constant Pool

```rust
pub enum Const {
    Int(i64),
    Float(f64),
    Bool(bool),      // bool need not be pooled (encoded by instructions); reserved
    Str(Box<str>),   // UTF-8 string
}
```

Rules:
- `int` / `float` / `string` literals go into the pool; see §7 for the small-integer optimization
- `bool` is encoded directly by the `True` / `False` instructions and is not pooled
- `unit` has no value; it is encoded by the `Unit` instruction

### Function Table

```rust
pub struct Func {
    pub name: ConstId,        // index into the constant pool's strings
    pub params: u16,          // number of parameters
    pub locals: u16,          // total number of local slots (including parameters)
    pub code: Box<[u8]>,      // instruction byte stream
    pub span_map: Box<[SpanEntry]>, // instruction offset → source location (for runtime diagnostics)
    pub is_builtin: bool,     // host builtin (e.g. print), executed directly by the VM, `code` not run
}
```

> **SpanEntry (defined and enabled in 0.0.2)**:
>
> ```rust
> pub struct SpanEntry {
>     pub offset: u32,  // starting byte offset of the instruction (relative to the function's code)
>     pub start: u32,   // starting byte offset in the source
>     pub end: u32,     // ending byte offset in the source (exclusive)
> }
> ```
>
> 0.0.1 decided not to generate it for now (left empty, `Box::new([])`). From 0.0.2 on,
> codegen fills `span_map` instruction by instruction (data source: the Spans carried by MIR
> instructions), for use in runtime error diagnostics.
>
> **Entry rules (contract fixed by 0.0.2 U07)**:
>
> - Only **MIR instructions** are covered; block terminators (`Return` / `Jump` / `JumpIf*`)
>   have no span at the MIR level and **produce no entries**;
> - Glue instructions synthesized by codegen (`StoreGlobal` / `Call main` inside `__init__`)
>   use `Span::SYNTHETIC` and **produce no entries**;
> - `is_builtin` functions have placeholder bodies with no source correspondence; their `span_map` is always empty;
> - `offset` is the position of the instruction's opcode byte, synonymous with `frame.ip` when the VM
>   errors (U10 looks up by `frame.ip`); readers must accept both empty and non-empty tables;
> - **A missed offset lookup = no source location**: consumers fall back to
>   "function name + instruction offset" and must not assume the table covers every instruction.

Rules:
- **Builtin functions are marked with `is_builtin: true`**, not identified by name: the language lets users shadow builtin names (e.g. a user-defined `func print`), so the host builtin cannot be distinguished from user functions by name alone.
- When `is_builtin` is `true`, the VM executes the host implementation directly and ignores `code`; `code` still holds a placeholder body (`Unit; Return`) to keep the structure well-formed and pass verify.
- `codegen` copies MIR's `MirFunc::is_builtin` verbatim into this field.

### Global Variable Table

```rust
pub struct Global {
    pub name: ConstId,   // index into the constant pool's strings
    pub mutable: bool,   // whether mutable (const is false)
}
```

Rules:
- **Immutability of globals is enforced at the VM level**: a `StoreGlobal` targeting a `mutable: false` global is a VM error
- Global initialization order = table order; initialization code lives in the synthesized `__init__` function (see §2 "Entry and Global Initialization")
- A `StoreGlobal` that initializes a `const` global inside `__init__` is allowed; everywhere else, the target of `StoreGlobal` must be `mutable: true`

### Struct Definitions (`struct_defs`, 0.0.3)

```rust
/// Type tag for one struct field — compact enough for verify's count/shape checks.
pub enum FieldTypeTag {
    Int = 0,  Float = 1,  Bool = 2,  Str = 3,  Unit = 4,  Func = 5,
    Result = 6, Array = 7, Box = 8,  Ref = 9,  Struct = 10, Range = 11,
}

pub struct StructDef {
    pub field_types: Vec<FieldTypeTag>, // one tag per field, in declaration order
}
```

Rules:
- `StructDef.field_types.len()` is exactly the operand count `StructNew` must carry; verify
  cross-checks `StructNew(id)` against `struct_defs[id]` (a forged/mismatched count is
  `StructIndexOutOfRange` / a field-count mismatch).
- Field tags exist only so `fleen-verify` can validate shape without executing; the VM does not
  consult them. Struct values are stored as a plain `Box<[Value]>` (§3.2).
- The table is **append-only in the module** and read only when `version >= 3` (§9).

---

## 3. Runtime Model (VM)

### 3.1 Stacks

- **Operand stack**: holds temporary values; the inputs and outputs of instructions
- **Call stack**: one `CallFrame` per call level:

```rust
pub struct CallFrame {
    pub func: FuncId,
    pub ip: usize,          // instruction pointer, a byte offset into code
    pub stack_base: usize,  // starting stack slot of this frame's locals
}
```

- Locals **do not get dedicated stack slots**; they are located on the operand stack at `stack_base + slot`
- `LoadLocal i` = read `stack_base + i`; `StoreLocal i` = write `stack_base + i`

### 3.2 Value Representation

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),                 // 0.0.2: exclusive ownership (0.0.1's temporary Rc<str> has been replaced as planned)
    Boxed(Box<Value>),             // 0.0.2: box<T>, unique ownership
    Ref { base: u32, slot: u16 },  // 0.0.2: borrow handle, lives only within the callee frame's lifetime
    Ok(Box<Value>),                // 0.0.2: Result construction
    Err(Box<Value>),               // 0.0.2: Result construction
    Struct(Box<[Value]>),          // 0.0.3: struct fields in declaration order (owned/ Copy per is_copy)
    Arr(Box<[Value]>),             // 0.0.3: array elements (owned for every T)
    Range { start: i64, end: i64 },// 0.0.3: builtin int interval (Copy)
    Future(Box<FutureFrame>),      // 0.0.4: suspendable coroutine frame (Future<T>; non-Copy, not serialized)
    Channel(Rc<RefCell<ChannelInner>>), // 0.0.4: channel handle — handles share one buffer (Rc is the semantics, not a shortcut)
    Unit,           // empty value, placeholder
    Func(FuncId),   // first-class function reference
}
```

Rules:
- Values are **self-describing**: the VM uses this for fast dispatch of arithmetic instructions
- An arithmetic instruction with mismatched types (e.g. `int + string`) is a **VM error** (`RuntimeError`), but correctly compiled bytecode should never contain one — that is `typeck`'s responsibility
- **Ownership discipline is statically guaranteed by typeck**; on the VM side, Rust's move/drop semantics are the runtime implementation,
  with `Drop` performed deterministically (frame-destruction truncation, slot overwrite, `Pop`), no GC
- `Eq` semantics: `Str` compares by content; `Boxed` compares the value inside the box (consistent with Rust's `Box: PartialEq`);
  `Ref` is dereferenced before comparing
- **0.0.3**: `Struct` / `Arr` / `Range` are **not comparable** (`==` on them is a *typeck* error);
  `Eq`/`Ne`/ordering reaching one in the VM is a defensive `RuntimeError::TypeMismatch`
  (unreachable via legal bytecode). `DupDeep` recurses into `Struct` / `Arr` / `Boxed` / `Ok` /
  `Err`, so `clone` deep-copies aggregates. `Range` is **Copy** (two `i64`s, no heap); `Struct`
  and `Arr` are owned.
- `Ref` handles cannot escape: they are only produced by `MakeRefLocal` during argument preparation, are read-only, cannot be stored,
  and die when the call ends (out-of-range read → `RuntimeError::BorrowOutOfRange`, defensive)
- `Value` is never serialized: the constant pool has no `Ref` / `Boxed` / `Ok` / `Err` variants
- **0.0.4**: `Future` / `Channel` are likewise **not serialized** (no `.flnc` tag, no
  constant-pool or `FieldTypeTag` change — the disk format is untouched). `Future<T>` is
  non-Copy: the coroutine frame moves like any owned value. A `channel<T>` handle is also
  non-Copy and move-only (`clone` is a typeck error); two handles may only share a buffer
  via the explicit fork point `ChannelSplit`, which is why the `Rc` inside `Value::Channel`
  is the language semantics (single-owner channels, documented exception to SPEC §11),
  not an implementation shortcut. Defensive runtime checks: awaiting a non-future is a
  `FutureExpected` error, and a program whose tasks are all blocked on channels with
  nothing runnable is `ChannelDeadlock`.

> **0.0.2 decision**: 0.0.1's `Str(Rc<str>)` was a temporary decision — strings are immutable with no
> reference-cycle risk, and `Rc`'s deterministic destruction is not GC. After introducing `move` / `clone`,
> `Rc`'s shared semantics conflicted with the ownership model
> (`clone` is a deep copy, while `Rc::clone` is a refcount increment), so per `docs/0.0.2/PLAN.md` §5.3
> it was replaced with `Str(Box<str>)`: `clone` is a deep copy, and refcounted sharing is gone.

### 3.3 Calling Convention

- **Direct call**: push the `params` arguments in order → execute `Call FuncId`
- **Indirect call (first-class value)**: first push `f: Func(FuncId)`, then push the arguments in order → execute `CallValue argc` (`argc = params`)
- **Callee side**: the arguments occupy this frame's `slot 0..params`; the remaining slots are locals; `f` stays on the caller's stack and is consumed by `CallValue`
- **Return**: `Return` pops 1 value from the top of the stack as the return value, pops the entire call frame, and **pushes the return value back onto the caller's stack top**
- Functions returning `unit` must also `Return` explicitly (push `Unit` then return) — keeping the convention uniform
- **Builtin function calls**: when the callee has `is_builtin == true`, the VM does not build a normal frame to run `code`; instead the host executes it via a preloaded mapping (e.g. `print` → `println!`), then pushes the return value back onto the caller's stack per the normal calling convention

### 3.4 VM Errors

```rust
pub enum RuntimeError {
    StackUnderflow,          // operand stack underflow
    InvalidOpcode(u8),       // unknown instruction
    ConstOutOfRange(ConstId),
    FuncOutOfRange(FuncId),
    GlobalOutOfRange(GlobalId),
    ImmutableGlobal(GlobalId),   // assignment to a const global
    ArithTypeMismatch,       // arithmetic on mismatched types
    DivisionByZero,
    CallDepthExceeded,       // recursion depth limit
}
```

Rules:
- VM errors **cannot be caught** (matching the language design: panic is only for unrecoverable errors)
- The `CallDepthExceeded` limit defaults to `1024` frames, preventing unbounded recursion from blowing the stack

---

## 4. Instruction Encoding

### 4.1 Encoding Rules

- `opcode`: 1 byte (`u8`)
- Operands:
  - `u8`: 1 byte, little-endian
  - `u16`: 2 bytes, little-endian
  - `u32`: 4 bytes, little-endian
  - `i64` / `f64`: 8 bytes, little-endian
- Total instruction length = 1 + the sum of operand sizes, **variable-length but statically determinable** (the decoder looks up the length by opcode)

### 4.2 Number Allocation

Number space is reserved by functional group for easy extension (0.0.2+ instructions such as `box` / `move` fit into gaps within their groups):

```
0x00–0x0F  Constants and stack operations
0x10–0x1F  Locals / globals
0x20–0x2F  Arithmetic (int)
0x30–0x3F  Arithmetic (float)
0x40–0x4F  Comparison / logic
0x50–0x5F  Control flow
0x60–0x6F  Function calls
0x70–0x7F  Selection (choose helpers)
0x80–0x83  box / ref (0.0.2, §5.9)
0x84–0x8F  Reserved
0x90–0x93  move / clone (0.0.2, §5.10)
0x94–0x9F  Reserved
0xA0–0xA4  Result / error handling (0.0.2, §5.11)
0xA5       `as` conversion `ToStr` (0.0.2 U13, F8 §3.6)
0xB0–0xB3  struct (0.0.3 V02, §5.12)
0xB4–0xB8  array (0.0.3 V03, §5.13)
0xB9–0xBB  Range / iteration (0.0.3 V04, §5.14)
0xBC–0xBF  Reserved
0xC0–0xC7  async / channel (0.0.4, §5.15)
0xC8–0xDF  Reserved (0.0.4 expansion space)
0xE0–0xFF  Reserved
```

---

## 5. Instruction Set (v0.0.1 base + v0.0.2 additions + v0.0.3 additions)

### 5.1 Constants and Stack Operations

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `Const` | `u32` (ConstId) | 5 | `→ v` | Load int/float/string from the constant pool |
| `True` | — | 1 | `→ true` | Push bool true |
| `False` | — | 1 | `→ false` | Push bool false |
| `Unit` | — | 1 | `→ unit` | Push the empty value |
| `Pop` | — | 1 | `v →` | Discard the stack top (discarding a statement's value) |
| `Dup` | — | 1 | `v → v v` | Duplicate the stack top |

### 5.2 Variables

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `LoadLocal` | `u16` (slot) | 3 | `→ v` | Read local at `stack_base + slot` |
| `StoreLocal` | `u16` (slot) | 3 | `v →` | Write local at `stack_base + slot` |
| `LoadGlobal` | `u16` (GlobalId) | 3 | `→ v` | Read a global |
| `StoreGlobal` | `u16` (GlobalId) | 3 | `v →` | Write a global (VM error if const) |

### 5.3 Arithmetic (int)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `IAdd` | — | 1 | `a b → a+b` | int addition; overflow is a VM error (see below) |
| `ISub` | — | 1 | `a b → a-b` | int subtraction |
| `IMul` | — | 1 | `a b → a*b` | int multiplication |
| `IDiv` | — | 1 | `a b → a/b` | int division; division by 0 → `DivisionByZero` |
| `IMod` | — | 1 | `a b → a%b` | int modulo; sign follows the dividend (Rust `%` semantics) |

**Integer overflow**: `IAdd` / `ISub` / `IMul` use `checked_*`; overflow is a VM error. Rationale: explicit and predictable — silent wraparound violates the "Safe" philosophy. (Later versions may add `unchecked` variants inside `unsafe` blocks.)

### 5.4 Arithmetic (float)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `FAdd` | — | 1 | `a b → a+b` | IEEE 754 addition |
| `FSub` | — | 1 | `a b → a-b` | |
| `FMul` | — | 1 | `a b → a*b` | |
| `FDiv` | — | 1 | `a b → a/b` | Division by 0 yields `inf`/`NaN` (IEEE semantics, not an error) |

### 5.5 Comparison / Logic

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `Eq` | — | 1 | `a b → bool` | Equal |
| `Ne` | — | 1 | `a b → bool` | Not equal |
| `Lt` | — | 1 | `a b → bool` | Less than |
| `Gt` | — | 1 | `a b → bool` | Greater than |
| `Le` | — | 1 | `a b → bool` | Less than or equal |
| `Ge` | — | 1 | `a b → bool` | Greater than or equal |
| `Not` | — | 1 | `b → !b` | Logical negation (unary `!`) |
| `NegI` | — | 1 | `a → -a` | int negation (unary `-`) |
| `NegF` | — | 1 | `a → -a` | float negation |

Rules:
- **There are no `And` / `Or` instructions**: `and` / `or` must short-circuit, implemented by control-flow instructions (see §5.6 and §6.3)

### 5.6 Control Flow

The jump operand `u16` is the **absolute byte offset of the target instruction** (relative to the start of `code`).

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `Jump` | `u16` | 3 | `→` | Unconditional jump |
| `JumpIfFalse` | `u16` | 3 | `b →` | Jump if the stack top is false (and pop it) |
| `JumpIfTrue` | `u16` | 3 | `b →` | Jump if the stack top is true (and pop it) |

Rules:
- Jump targets must **land on instruction boundaries** (misaligned → VM error); `fleen-verify` performs the static check
- `JumpIfTrue` is used for `or` short-circuiting; `JumpIfFalse` for `if` conditions and `and` short-circuiting
- `for` desugars to indexed iteration + control-flow jumps (see §5.13–5.14 v0.0.3); no `break` / `continue`

### 5.7 Function Calls

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `Call` | `u16` (FuncId) | 3 | `args... → ret` | Call a function (see §3.3) |
| `LoadFunc` | `u16` (FuncId) | 3 | `→ Func(id)` | Push a function as a first-class value |
| `CallValue` | `u8` (argc) | 2 | `f args... → ret` | Indirectly call the function value on the stack |
| `Return` | — | 1 | `ret →` | Return (see §3.3) |

Rules:
- `Call` is for **direct calls** (target known at compile time, guaranteed by MIR after typeck resolution)
- `CallValue` is for **first-class calls** (`f(a, b)` where `f` is a variable); `argc` is the number of arguments
- **Stack order**: `f` must be pushed before the arguments, i.e. `f arg0 arg1 ...` (`f` toward the bottom of the stack, arguments toward the top)
- 0.0.1 uses `u8` for `argc` (up to 255 arguments); if the language later needs more, add `CallValueWide` without changing the existing encoding
- **Variadic builtin** (`print`): `Call` pops exactly `callee.params` arguments,
  so each observed call arity corresponds to a distinct instance in the function table (`params = argc`,
  dispatched by the host under the `print` name, fixed by U09) — sharing a single
  `params == 1` instance would leave excess arguments stranded on the stack

### 5.8 choose Helpers

`choose`'s pattern matching is flattened at the MIR level into a **comparison chain + jumps** (see §6.4); only one helper instruction is needed to bind the guard variable:

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `BindMatch` | `u16` (slot) | 3 | `v → v` | Copy the stack top into the guard slot (the `x` of `when x if ...`) |

Rules:
- `BindMatch` does not pop the stack top: the matched value still participates in later comparisons or serves as the source of the branch value

### 5.9 box / ref (v0.0.2)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `AllocBox` | — | 1 | `v → b` | Heap-allocate; ownership of the value moves into the box |
| `DerefBox` | — | 1 | `b → b v` | Read the value inside the box (copy/deep copy); the box stays on the stack |
| `StoreDerefBox` | — | 1 | `b v →` | Write the value inside the box; the old value is released |
| `MakeRefLocal` | `u16` (slot) | 3 | `→ ref` | Produce a borrow handle pointing at this frame's `stack_base + slot` (argument preparation) |

Rules:
- `box` has unique ownership: there is **no** "move out of box" instruction — reading produces a copy;
  obtaining the value is done at the language level via `clone deref b` (`DerefBox`)
- `MakeRefLocal`'s slot is validated like `LoadLocal`/`StoreLocal` (slot < `locals`);
  the `base` in the handle value `Ref { base, slot }` is bound by the VM at call time to the caller's frame
  and is not part of the encoding
- **Forwarding** for `MakeRefLocal`: when the target slot already holds a handle (a `ref` argument passed on to the next
  `ref` argument), push that same handle as-is; do not wrap it again (a handle wrapping a handle cannot be dereferenced through)
- For a local box, the compiler generates `v; AllocBox; StoreLocal b; Unit` for `deref b = v`
  (fixed by U09; the trailing `Unit` is the value of the assignment expression, discarded by `Pop` when used as a statement):
  `StoreDerefBox`'s contract is to pop the box, write into it, and discard it, while
  `LoadLocal` produces a deep copy, so the write cannot land back in the slot's box; under unique ownership,
  replacing the value inside the box and replacing the whole box are observationally equivalent. `StoreDerefBox` is kept for
  hand-written bytecode and for paths that write through handles (the 0.0.2 compiler no longer emits it)

### 5.10 move / clone (v0.0.2)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `DupDeep` | — | 1 | `v → v v` | Deep-copy the stack top (`clone`'s on-stack form) |
| `MoveLocal` | `u16` (slot) | 3 | `→ v` | **Consuming read**: take the slot's value and clear the slot to `Unit` |
| `CloneLocal` | `u16` (slot) | 3 | `→ v` | Non-consuming read: deep-copy the slot's value; the slot is untouched |
| `CloneGlobal` | `u16` (GlobalId) | 3 | `→ v` | Non-consuming read of a global (the only way to read an owned global) |

Rules:
- 0.0.1's `LoadLocal` / `LoadGlobal` (non-consuming read, shallow copy) **remain unchanged**,
  used only for Copy types; owned types use `MoveLocal` for consuming reads and
  `CloneLocal`/`CloneGlobal` for non-consuming reads, chosen by codegen based on the static type
- The contents of an "already moved" slot are invisible to the VM (typeck guarantees it is not read again); `MoveLocal` uses
  `mem::replace` to clear the slot, avoiding the old value lingering until frame destruction
- **`CloneGlobal` does not dereference handles** (U09 decision, unlike `CloneLocal`/`DupDeep`):
  typeck forbids storing a `Ref` in a global, so a global slot only ever holds owned values and the copy point needs no dereference;
  if hand-written bytecode stores a handle into a global, the fallback is at the **consumption point**
  (`Eq`/`Ne`, ordering comparisons, `print` arguments, `DupDeep`, and `DerefBox`'s `read_ref`); out of range → `BorrowOutOfRange`

### 5.11 Result / Error Handling (v0.0.2)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `PackOk` | — | 1 | `v → ok(v)` | Construct `Ok` |
| `PackErr` | — | 1 | `v → err(e)` | Construct `Err` |
| `IsErr` | — | 1 | `r → bool` | Test for `Err` |
| `UnwrapOk` | — | 1 | `r → v` | Extract the Ok payload (moving) |
| `UnwrapErr` | — | 1 | `r → e` | Extract the Err payload (moving) |

Rules:
- `UnwrapOk` on an `Err` (or vice versa) → `RuntimeError::ResultMismatch` (defensive;
  legitimately compiled output never hits this — the branches are guaranteed by `IsErr` + jumps)
- Payload extraction is a **move**: the `Result` value is consumed

### 5.12 struct (v0.0.3 V02)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `StructNew` | `u16` (struct_id) | 3 | `f₁ … fₙ → s` | Pop n field values (declaration order) into a struct of id; n comes from `struct_defs[id]` |
| `GetField` | `u16` (offset) | 3 | `s → v` | Pop the struct, push a **copy** of field `offset` (D2 read semantics) |
| `SetFieldLocal` | `u16` (slot), `u16` (offset) | 5 | `v →` | Overwrite field `offset` of the struct in local `slot`; old field released |
| `SetFieldGlobal` | `u16` (gid), `u16` (offset) | 5 | `v →` | Same, for the struct held by global `gid`; writes in place, so the `StoreGlobal` `mutable` gate does not apply |

Rules:
- `StructNew`'s field count is the **operand-derived** `struct_defs[id]` length — static, so verify
  needs no abstract interpretation (`StructNew` requires depth n; a forged id is
  `StructIndexOutOfRange`)
- `GetField` is the **pop** form (`s → v`, Δ0, min 1): the struct leaves the stack so
  `p.a.b` chains compose (the V02 erratum recorded in PLAN §5.2; the planned peek form `s → s v`
  would strand the struct under the field value)
- `SetFieldLocal` / `SetFieldGlobal` pop the **value only** — the field offset is an operand, not
  a stack item (`v →`, Δ−1, min 1)

### 5.13 array (v0.0.3 V03)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `ArrNew` | `u16` (n) | 3 | `e₁ … eₙ → a` | Pop n elements into a fresh array |
| `ArrLen` | — | 1 | `a → a int` | Length (peek — the array stays on the stack) |
| `ArrGet` | — | 1 | `a i → v` | Bounds-checked element **copy** (**pop** form); out of bounds → `IndexOutOfRange` |
| `ArrSetLocal` | `u16` (slot) | 3 | `v i →` | Bounds-checked store into local `slot`'s array (`a[i] = e`) |
| `ArrSetGlobal` | `u16` (gid) | 3 | `v i →` | Same, for the array held by global `gid`; writes in place, so the `StoreGlobal` `mutable` gate does not apply |

Rules:
- **All index operations are bounds-checked at runtime**: an out-of-range index (negative or
  `>= len`) → `RuntimeError::IndexOutOfRange { index: i64, len: usize }` with the faulting
  instruction's span (§2 `span_map`). The sign is checked **before** the `usize` cast, so `a[-1]`
  is out of range, never a huge `usize` index. This is reachable from compiler output — it is the
  intended runtime behavior of the `a[i]` syntax (D3), not just defensive.
- `ArrGet` pops the array and index and pushes the element **copy** (`a i → v`, Δ−1, min 2) — the
  same generalized pop form as `GetField` so `a[i][j]` chains and the `for` desugar work (the
  planned peek form `a i → a v` would strand the array with no remove-below-top instruction)
- `ArrLen` is peek-shaped (`a → a int`, Δ+1, min 1): the array must stay on the stack for a
  subsequent `ArrGet`
- `ArrSetLocal` / `ArrSetGlobal` pop the value **and** the index (`v i →`, Δ−2, min 2) and take the
  array from the slot/global (not the stack); owned values first dup/deep-dup as assignment
  requires
- `ArrNew(n)` requires depth n and has net effect `1 − n` (static, n is an operand)

### 5.14 Range / Iteration (v0.0.3 V04)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `MakeRange` | — | 1 | `start end → r` | Range value (Copy: two i64) — start inclusive, end exclusive |
| `RangeStart` | — | 1 | `r → r int` | Start bound (peek — the Range stays on the stack) |
| `RangeEnd` | — | 1 | `r → r int` | End bound (peek — the Range stays on the stack) |

Rules:
- A `Range` is represented as `Value::Range { start: i64, end: i64 }` (Copy)
- `RangeStart` / `RangeEnd` are peek-shaped (`r → r int`, Δ+1, min 1): the Range stays on the stack so that
  both bounds can be extracted in sequence (the parked copy is then dropped with a `Pop`)
- `MakeRange` pops both bounds and pushes the Range (`start end → r`, Δ−1, min 2)
- A `for x in r { … }` loop desugars to: `StoreLocal __it; __i = RangeStart(__it); __count = RangeEnd(__it);
  while __i < __count { x = __i; …; __i = __i + 1 }` (see PLAN.md §5.4 for the exact MIR shape)
- Source construction is via `std.collection.range` / `std.collection.range_from` (0.0.3 V06,
  D5): the two recognized names lower directly to `MakeRange` (the `Ok`/`Err` constructor
  mechanism), so the compiler emits `MakeRange` for them and the desugar above applies.

### 5.15 async / channel (v0.0.4)

| Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|------|--------|------|
| `AsyncStart` | — | 1 | `args… → f` | Head of an `async func` body: packages the parameter slots into a not-yet-started coroutine frame and delivers the `Future` value to the caller (verify treats the rest of the body as a fresh call boundary, net Δ0) |
| `Suspend` | — | 1 | `… → …` (Δ0) | Suspension point: snapshots the running coroutine frame (locals + operand stack + resume PC), yields `Pending` to the driver, and parks the task. Emitted after each suspension source (`std.io.sleep`, `std.async.yield_now`, a channel `send`/`recv` that may block) |
| `Poll` | — | 1 | `f → t` | Drive a `Future` to completion and leave its `T` on the stack. Same semantics as `Await` in the VM; 0.0.4 lowering emits only `Await` — `Poll` is the poll-shaped form kept for future driving styles |
| `Await` | — | 1 | `f → t` | Drive a `Future` to completion (`F → T`); the resumed body's `Return` hands the payload back to the awaiting frame. Postfix `e await` lowers to this; `e await?` composes with the `?` propagation rules (§6.7) |
| `ChannelNew` | — | 1 | `cap → ch` | Allocate a channel with the payload type carried in MIR only (T08 defensive backstop) — the encoded instruction has **no operand**. `cap = 0` is a rendezvous channel (direct handoff), `cap > 0` a bounded buffer |
| `ChannelSend` | — | 1 | `ch v → unit` | Send a value through the channel; **may suspend** while the buffer is full (or, at `cap = 0`, until a receiver takes the value). The payload is moved — ownership transfers to the buffer/receiver |
| `ChannelRecv` | — | 1 | `ch → v` | Receive a value; **may suspend** while the buffer is empty. The receiver takes ownership of the payload |
| `ChannelSplit` | — | 1 | `ch → ch2` | The one explicit fork point (T07d): return a second independent handle to the *same* buffer. `clone` on a channel is a typeck error — this is the only way a buffer gains a second handle |

Rules:
- All eight slots sit inside the **reserved `0xBC–0xFF` region** (§4.2); no v1/v2/v3 encoding
  changes (§10 compatibility promise)
- `AsyncStart` is operand-less: the future value it produces owns the coroutine frame
  (`Value::Future(Box<FutureFrame>)` — `func`, `resume_pc`, the frame's `locals`, the saved
  `operands`, the frame `state`, and the executor's `task_id`)
- A suspension parks a **chain** of frames (the coroutine's live call chain), not just one frame;
  the parked sender of a rendezvous/full channel keeps owning its payload inside the channel's
  waiting queue until a receiver takes it
- Channel semantics: payloads travel by ownership move (Copy payloads copy); channel
  payload kind is fixed by the first accepted value and re-checked defensively in the VM
  (T08 backstop) — static uniformity is typeck's rule
- Scheduling is the VM's **single-threaded cooperative executor**: a task runs until a
  suspension source yields `Pending`; ready tasks always run before timer-parked ones
  (wake order is deterministic); when no task is runnable and all remaining ones are
  blocked on channels, the run fails with `ChannelDeadlock` (there is no `close` in 0.0.4)

---

## 6. Source Constructs → Bytecode Mapping

> **Semicolon note**: 0.0.1 requires explicit semicolons `;`; from 0.0.2 ASI lands and semicolons become optional —
> the presence or absence of semicolons is invisible after the AST, so the mappings in this section are unaffected.
> **No semicolon after function declarations**: a `func` declaration has no trailing semicolon; see `SYNTAX.ebnf` for the grammar.
> **Return value**: the last expression of a function body has no semicolon; otherwise the return value is discarded.

### 6.1 Expression Statements

```fleen
fib(3);
```

```text
Const    3            ; argument
Call     fib
Pop                   ; the statement's value is discarded
```

### 6.2 Binding and Assignment

```fleen
x = 42;
x = 43;
```

```text
Const    42
StoreLocal 0          ; x bound to slot 0
Const    43
StoreLocal 0          ; x assigned (same slot)
```

Rules:
- **Shadowing is handled at compile time**: a new binding allocates a new slot and the old slot is no longer referenced — the VM need not know about shadowing

### 6.3 if / and / or Short-Circuiting

```fleen
if a and b { 1 } else { 2 }
```

```text
LoadLocal a
JumpIfFalse L_else    ; and short-circuits on the left operand
LoadLocal b
JumpIfFalse L_else    ; and short-circuits on the right operand
True
Jump       L_end
L_else:
False
L_end:
JumpIfFalse L2
Const 1
Jump L3
L2:
Const 2
L3:
```

Rules:
- `if` chain: the condition's `JumpIfFalse` → next branch; at the end of a branch, `Jump` to the exit
- When there is no `else` and the condition fails, the `if` expression's value is `Unit`

### 6.4 while

```fleen
while x < 10 {
    x = x + 1;
}
```

```text
L_cond:
LoadLocal x
Const    10
Lt
JumpIfFalse L_end
LoadLocal x
Const    1
IAdd
StoreLocal x
Jump     L_cond
L_end:
Unit                  ; the while expression's value is Unit
```

### 6.5 choose

```fleen
choose value {
    when 0 { "zero" }
    when x if x > 10 { "big" }
    otherwise { "other" }
};
```

**Semicolon rule**: the `when` and `otherwise` clauses themselves take no semicolons; only when the whole `choose` is used as a statement does it get a trailing semicolon.

```text
LoadLocal value
L_m0:                     ; when 0
Dup
Const    0
Eq
JumpIfFalse L_m1
Pop                       ; discard the original matched value (the bool was consumed by JumpIfFalse)
Const    "zero"
Jump      L_end

L_m1:                     ; when x if x > 10
BindMatch 1               ; bind to guard slot 1
LoadLocal 1
Const    10
Gt
JumpIfFalse L_m2
Const    "big"
Jump      L_end

L_m2:                     ; otherwise
Pop
Const    "other"
L_end:
```

Rules:
- Flattened into a **comparison chain**: one comparison + `JumpIfFalse` per `when`
- The guard `if` immediately follows the pattern comparison
- No `otherwise` and nothing matches → the expression's value is `Unit` (exhaustiveness is checked by `typeck`)

> **Label convention**: labels such as `L_m0`, `L_m1`, `L_end` in this document are merely notation.
> The bytecode binary has **no labels** — jump operands are **absolute byte offsets** within the function (§5.6).
> Codegen internally generates globally unique labels with a monotonically increasing counter (`L0`, `L1`, `L2` …);
> nested `choose` blocks simply keep incrementing the counter and can never collide by construction. The final assembly stage resolves labels into offsets.

### 6.6 Function Definitions

```fleen
func add(a: int, b: int): int = a + b
```

```text
; in the function table: Func { name: "add", params: 2, locals: 2, code: ... }
LoadLocal 0
LoadLocal 1
IAdd
Return
```

```fleen
func greet(name: string) {
    print("Hello, ", name);
}
```

```text
; in the function table: Func { name: "greet", params: 1, locals: 1, code: ... }
LoadLocal 0
Call     print
Unit                  ; unit function pushes Unit explicitly
Return
```

### 6.7 `?` Propagation (v0.0.2)

```fleen
q = div(a, b)?;
```

```text
<eval div(a, b)>      ; r: Result<int, string> (owned string payload → DupDeep)
DupDeep               ; r r
IsErr                 ; r bool
JumpIfTrue  L_err
UnwrapOk              ; r → v
StoreLocal q
Jump        L_cont
L_err:
UnwrapErr             ; r → e
PackErr               ; e → err(e)
Return                ; early return of the Err (function level)
L_cont:
```

> **Dup vs DupDeep**: the `?` duplication instruction is chosen by the operand type's ownership —
> if either `T` or `E` in `Result<T, E>` is owned (`string` / `box<T>`), the whole type is owned
> and `DupDeep` is used; purely Copy payloads (e.g. `Result<int, int>`) use `Dup`.
> In the example above the `string` payload is owned, hence `DupDeep`.

### 6.8 choose (Result scrutinee, v0.0.2)

```fleen
choose div(10, 2) {
    when Ok(v) { v }
    when Err(e) { 0 - 1 }
}
```

```text
<eval div(10, 2)>     ; r: Result<int, string> (owned string payload → DupDeep)
DupDeep
IsErr
JumpIfTrue  L_err
UnwrapOk              ; payload v (moved)
StoreLocal v          ; when Ok(v) binding
<ok arm body>         ; tail value
Jump        L_end
L_err:
UnwrapErr             ; payload e (moved)
StoreLocal e
<err arm body>
L_end:
```

Rules:
- The payloads of the `Ok`/`Err` arms are **moved** out by `UnwrapOk`/`UnwrapErr` and then bound via `StoreLocal`;
  no `BindMatch` is needed (the matched value has already been consumed and no longer participates in the comparison chain)
- With a guard (`when Ok(x) if x > 0`), lowering must preserve the payload before the guard is evaluated
  for use by later arms (the exact sequence is settled during implementation; acceptance criteria: semantic equivalence + verify passes).
  Note: guard arms do not count toward exhaustiveness (DESIGN §10.5), so there must additionally be a **guard-free duplicate**
  `Ok` / `Err` arm or an `otherwise` fallback; otherwise typeck reports `ChooseNotExhaustive`.
- Exhaustiveness is checked by `typeck`: **guard-free** `Ok` + `Err` arms alone are exhaustive

### 6.9 array `for` (v0.0.3 V04)

```fleen
for x in [10, 20, 30] {
    total = total + x;
}
```

```text
ArrLen …                      ; __count = len(__it)   (peek, then Pop the parked array copy)
StoreLocal __count
; …
L_loop:
LoadLocal __i
LoadLocal __count
Lt                            ; __i < __count
JumpIfFalse L_end
LoadLocal __it                ; __it is evaluated once, before the loop (D4)
LoadLocal __i
ArrGet                        ; element copy
StoreLocal x                  ; loop variable: a fresh immutable binding
<block body>
LoadLocal __i
Const    1
IAdd
StoreLocal __i
Jump      L_loop
L_end:
Unit                          ; for's value is unit
```

- `for` over a `Range` uses the same shape with `__i = RangeStart(__it)` and
  `__count = RangeEnd(__it)` (§5.14); the iterable is always evaluated once and parked in a
  synthetic local before the loop
- The desugar is done in **Lower** (`lower/`, `lower_for`), never the parser — Parse emits a real
  `ExprFor` / `ExprForHir` node so Resolve/Typeck see the loop structure (no phase is skipped,
  SPEC §6)

### 6.10 async / await / channel (v0.0.4)

```fleen
async func producer(c: channel<string>) {
    c.send("hi");
}
async func consumer(c: channel<string>): int {
    s = c.recv();
    std.str.len(s)
}

ch = channel<string>(4);
peer = std.concurrent.split(ch);
std.async.run([producer(move ch), consumer(move peer)]);
```

```text
; channel construction / fork (channel<T>(cap) and split are real HIR nodes)
Const    4            ; capacity
ChannelNew            ; cap → ch
; std.concurrent.split(ch) lowers to <eval ch>; ChannelSplit — ch → ch2

; producer's body (the function body's first instruction is AsyncStart):
L_body:
AsyncStart            ; coroutine frame → f (delivered to the caller)
<eval c>              ; LoadLocal 0
<eval "hi">           ; Const <s>
ChannelSend           ; ch v → unit (followed by Suspend — send may block)
Unit
Return

; driving: `f await` lowers to <eval f>; Await (f → t); `e await?` reuses
; the §6.7 ? machinery on the awaited value; a bare `await;` statement
; discards the payload with Pop. `std.async.run(...)` is a VM builtin whose
; call hands the futures to the cooperative scheduler and joins.
```

Rules:
- `async func f(...)` compiles to a **normal function whose body starts with `AsyncStart`**:
  the call itself does not run the body — it packages the arguments into a not-yet-started
  coroutine frame and returns the `Future`; the caller sees `Future<T>` while the body still
  returns `T`
- `c.send(v)` lowers to `<eval c>; <eval v>; ChannelSend`; `c.recv()` to `<eval c>; ChannelRecv`;
  `std.concurrent.split(ch)` to `<eval ch>; ChannelSplit` — all recognized in typeck (T07a)
  and lowered as first-class MIR instructions, never as ordinary calls
- Suspension sources (`std.io.sleep`, `std.async.yield_now`, a blocking `ChannelSend` /
  `ChannelRecv`) lower to the call followed by `Suspend` — the compiler emits the suspension
  point, the executor decides whether the task actually parks
- The channel payload move is a typeck rule (D6): an owned variable handed to `send` requires
  an explicit `move`/`clone` (the `OwnedArgRequiresMove` hint, with a channel branch naming
  `move` / `std.concurrent.split`), and a `ref` payload is rejected outright
  (`ChannelNotBorrowable`) — no bytecode-level check needed

---

## 7. Complete Example

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}
```

Function table:

| FuncId | name | params | locals |
|--------|------|--------|--------|
| 0 | `fib` | 1 | 1 |

```text
L0:
LoadLocal 0
Const    <1>          ; 2
Lt
JumpIfFalse L1
LoadLocal 0            ; n
Return

L1:
LoadLocal 0
Const    <1>          ; 2
Eq
JumpIfFalse L2
Const    <0>          ; 1
Return

L2:
LoadLocal 0
Const    <0>          ; 1
ISub                   ; n-1
Call     fib
LoadLocal 0
Const    <1>          ; 2
ISub                   ; n-2
Call     fib
IAdd                   ; fib(n-1) + fib(n-2)
Return
```

Constant pool:

| ConstId | Value |
|---------|-----|
| 0 | `1` |
| 1 | `2` |

---

## 8. Responsibilities of fleen-verify

`fleen-verify` statically validates a bytecode module without **executing** it:

- [ ] All indices of `Const` / `LoadFunc` / `Call` are in range
- [ ] All jump targets land on **instruction boundaries**
- [ ] Every path through every function ends with `Return` (no execution path falls off the bottom of a function)
- [ ] **Operand stack depth never underflows**: for each function, perform a **full control-flow abstract stack-depth analysis** —
  - Each instruction's stack effect (`Δdepth`) is statically known (see the instruction tables in §5)
  - Starting from the function entry (depth = 0), propagate the abstract stack depth along the control-flow graph
  - When merging branches: if all predecessors agree on the depth, use that depth; **a mismatch is a verify failure** (no conservative "take max/min" approximation)
  - The depth before each instruction executes is ≥ that instruction's minimum required depth
- [ ] All `StoreGlobal` targets are `mutable: true`, with **one exception**: initialization writes to globals inside the entry function (the `__init__` for global initialization), used to give `const` globals their initial value
- [ ] The constant pool has no duplicate entries
- [ ] Every function has `locals >= params`, and all `LoadLocal` / `StoreLocal` / `BindMatch` /
      `MakeRefLocal` / `MoveLocal` / `CloneLocal` slot operands are < `locals`;
      `CloneGlobal`'s gid is within the global table's range (0.0.2 U07)
- [ ] **v2/v3/v4 instructions must not appear in a lower-version module**: all v1 opcodes are < 0x80,
      all v2 opcodes are in 0x80–0xAF, all v3 opcodes are ≥ 0xB0, and all v4 opcodes are ≥ 0xC0 (§4.2),
      so range comparisons suffice (verify's `V2OpcodeInV1Module` and the v3/v4 gates); a v3 opcode
      in a v2 module — or a v4 opcode in any v1–v3 module — → `BadOpcode`
- [ ] **`StructNew(id)` indexes `struct_defs`** and carries exactly `field_types.len()` values;
      `StructIndexOutOfRange` otherwise (0.0.3 V02)

### Per-Instruction Stack Depth Effect (Δdepth)

The stack-depth analysis is based on a static table (`n` is the operand count):

| Instruction | Δdepth | Minimum depth before execution |
|------|--------|----------------|
| `Const` / `True` / `False` / `Unit` / `LoadLocal` / `LoadGlobal` / `LoadFunc` | +1 | 0 |
| `Pop` | -1 | 1 |
| `Dup` | +1 | 1 |
| `StoreLocal` / `StoreGlobal` | -1 | 1 |
| `IAdd`…`IMod` / `FAdd`…`FDiv` / `Eq`…`Ge` | -1 | 2 |
| `Not` / `NegI` / `NegF` | 0 | 1 |
| `Jump` | 0 | 0 |
| `JumpIfFalse` / `JumpIfTrue` | -1 | 1 |
| `Call` | 1 - p (p is the callee's `params` count) | p |
| `CallValue` | -(argc + 1) + 1 = -argc | argc + 1 |
| `BindMatch` | 0 | 1 |
| `AllocBox` | **0** (`v → b`: the value is consumed and replaced by the box) | 1 |
| `DerefBox` / `DupDeep` | +1 | 1 |
| `MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal` | +1 | 0 |
| `StoreDerefBox` | -2 | 2 |
| `PackOk` / `PackErr` / `IsErr` / `UnwrapOk` / `UnwrapErr` | 0 | 1 |
| `ToStr` | 0 (`v → s`) | 1 |
| `StructNew(id)` | `1 − n` (n = `struct_defs[id].field_types.len()`) | n |
| `GetField` | 0 (`s → v`) | 1 |
| `SetFieldLocal` / `SetFieldGlobal` | −1 (the value; the offset is an operand) | 1 |
| `ArrNew(n)` | `1 − n` | n |
| `ArrLen` / `RangeStart` / `RangeEnd` | +1 | 1 |
| `ArrGet` | −1 (`a i → v`) | 2 |
| `ArrSetLocal` / `ArrSetGlobal` | −2 (−1 value, −1 index) | 2 |
| `MakeRange` | −1 | 2 |
| `AsyncStart` | 0 (call boundary — the body is verified as a fresh call) | 0 |
| `Suspend` | 0 | 0 |
| `Poll` / `Await` | 0 (`f → t`) | 1 |
| `ChannelNew` | +1 (`cap → ch`) | 1 |
| `ChannelSend` | −2 (`ch v → unit`) | 2 |
| `ChannelRecv` | +1 (`ch → v`) | 1 |
| `ChannelSplit` | 0 (`ch → ch2`) | 1 |
| `Return` | treated as a path terminator | 1 (the return value is on the stack top) |

> **U07 erratum**: `AllocBox`'s net effect is **0** (`v → b`: the value is replaced by the box),
> consistent with the stack-effect column in §5.9 and Lower's depth table; during planning it was
> mistakenly written as `+1` on the same row as `DupDeep`, which would make verify compute every
> depth after `box e` off by 1.
> The inconsistency was found and fixed by U07's end-to-end test "compiler output must pass verify".

Rules:
- `Call`'s minimum depth is the callee's `params` count; the callee's return value is always pushed, so the net effect is `1 - params`.
- `CallValue` needs `f` plus `argc` arguments on the stack; after popping them it pushes the return value, so the net effect is `-argc`.
- `Return` requires the depth before execution to be exactly 1 (only the return value remains); any residue is a verify failure.

Rules:
- The analysis is **exact, not conservative**: bytecode instructions have fixed stack effects, so the analysis can prove no underflow
- In the VM, `StackUnderflow` is purely a **defensive fallback** (a verify bug, or hand-tampered bytecode)
- `codegen` output must pass `fleen-verify` — this is codegen's acceptance criterion

---

## 9. Binary Serialization (`.flnc`)

0.0.1 defines the on-disk binary format of the in-memory `Module`, with the extension `.flnc`. All integers are **little-endian**; multi-byte fields use the widths below.
0.0.2 makes **zero layout changes**: `version` is bumped to `2`, readers accept `{1, 2}`; `span_map` is populated from 0.0.2 on (§2).
0.0.3 (V02) writes `version = 3`, readers accept `{1, 2, 3}`; it appends **one** new table
(`struct_defs`) at the end — the field order and widths of every existing table are unchanged.
0.0.4 writes `version = 4`, readers accept `{1, 2, 3, 4}`; it makes **zero layout changes** —
the new async/channel instructions live inside function bodies and `Value::Future` /
`Value::Channel` are not serialized, so no new table and no field change is needed.

### File Header

| Field | Width | Value |
|------|------|-----|
| magic | 4 B | `"FLNC"` (0x46 0x4C 0x4E 0x43) |
| version | u16 | bytecode version: the 0.0.4 compiler writes `4`, readers accept `{1, 2, 3, 4}` |

### Constant Pool

`constants_len: u32`, then one entry at a time:

| tag | Meaning | Following fields |
|-----|------|----------|
| 0 | `Int` | `v: i64` |
| 1 | `Float` | `bits: u64` (the f64 bit pattern) |
| 2 | `Str` | `len: u32` + UTF-8 bytes |

### Function Table

`functions_len: u32`, then one entry at a time:

| Field | Width |
|------|------|
| `name` (ConstId) | u32 |
| `params` | u16 |
| `locals` | u16 |
| `is_builtin` | u8 (0/1) |
| `code_len` | u32 |
| `code` | `code_len` bytes |
| `span_map_len` | u32 |
| `span_map` | each entry `offset: u32, start: u32, end: u32` |

### Global Variable Table

`globals_len: u32`, then one entry at a time:

| Field | Width |
|------|------|
| `name` (ConstId) | u32 |
| `mutable` | u8 (0/1) |

### Entry

`entry: u32` (FuncId).

### Struct Definitions (`struct_defs`, 0.0.3 V02, version ≥ 3 only)

`struct_defs_len: u32`, then one entry at a time:

| Field | Width |
|------|------|
| `field_count` | u32 |
| `field_types` | `field_count` × u8 (a `FieldTypeTag`, §2) |

The `FieldTypeTag` enum maps to the byte as `Int=0, Float=1, Bool=2, Str=3, Unit=4, Func=5,
Result=6, Array=7, Box=8, Ref=9, Struct=10, Range=11`; any other byte is a format error.
The table is present only when `version >= 3`.

Rules:
- Reading and writing must go through the same implementation (`codegen::flnc::{to_bytes, from_bytes}`); hand-written parsers are not allowed.
- Version mismatch, bad magic, out-of-range indices, and truncated lengths are all deserialization errors.
- `is_builtin` / `mutable` only accept `0` or `1`; any other byte is a format error.
- The constant pool `tag` only accepts `0/1/2`; anything else is a format error.
- Later versions (2+) only **append new tables at the end**; the field order and widths of existing tables remain stable.
- `version` accepts `{1, 2, 3, 4}` (from 0.0.4 on); v3 appends `struct_defs`; v4 changes no table.

---

## 10. Versioning

| Bytecode version | Language version | Contents |
|-----------|-------------|------|
| 1 | 0.0.1 | The instruction set defined in this document |
| 2 | 0.0.2 | + ownership instructions `AllocBox` / `DerefBox` / `StoreDerefBox` / `MakeRefLocal` (0x80–0x83), `DupDeep` / `MoveLocal` / `CloneLocal` / `CloneGlobal` (0x90–0x93), Result instructions (0xA0–0xA4), `ToStr` (0xA5, `as` conversion); `span_map` starts being populated (§5.9–5.11, §2) |
| 3 | 0.0.3 | + aggregation instructions: `StructNew`/`GetField`/`SetFieldLocal`/`SetFieldGlobal` (0xB0–0xB3), `ArrNew`/`ArrLen`/`ArrGet`/`ArrSetLocal`/`ArrSetGlobal` (0xB4–0xB8), `MakeRange`/`RangeStart`/`RangeEnd` (0xB9–0xBB); `Struct`/`Arr`/`Range` values; the appended `struct_defs` table (§2, §9) |
| 4 | 0.0.4 | + async / channel instructions: `AsyncStart`/`Suspend`/`Poll`/`Await` (0xC0–0xC3), `ChannelNew`/`ChannelSend`/`ChannelRecv`/`ChannelSplit` (0xC4–0xC7); `Future`/`Channel` runtime values (not serialized); **zero layout changes** — no new table (§5.15) |

Rules:
- The bytecode version increases **monotonically** with the language version
- New instructions only occupy reserved number groups; **the numbers and encodings of existing instructions never change**
- The VM refuses to execute modules whose version exceeds what it supports
