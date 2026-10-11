# Fleen 0.0.3 Implementation Ticket Breakdown

> **Goal**: Aggregation and organization — land `struct` / `[T]` / `for` + `Range` / the module
> system end-to-end, ship the `std` library, and deliver the `flint` package manager
> (`flint.toml` + git dependencies + `new/build/run/test/check`).
> **Basis**: `docs/0.0.3/PLAN.md` (draft 2026-10-11, decision points D1–D10 pending review),
> `docs/BYTECODE.md` v0.0.2 (opcode space, §10 versioning rules), `docs/DESIGN.md` §9/§10/§18,
> `docs/0.0.2/TICKETS.md` (current implementation state: 0.0.2 complete).
> **Status** (2026-10-11): V01 implemented (completion notes at the end of its section);
> V02 implemented (completion notes at the end of its section); V03+ not started.
> Implementation begins after the PLAN review freezes D1–D10.
> **Split method**: unlike 0.0.2 (tickets cut **by compiler stage**, U01 Lex → U12 release),
> 0.0.3 is cut **by feature** — every ticket is a vertical slice through Lex → Parse → … → VM
> for one feature (the 0.0.2 U13 precedent, generalized). PLAN §9's stage-based table is
> superseded by this breakdown; the milestone mapping is kept (§ Milestones below).

---

## Overview

| Ticket | Feature | Phases touched | Crates | Deliverable | Depends on |
|------|------|------|-------|------|------|
| V01 | Foundation: `FileId` / `SourceMap` | all (mechanical) | fleen-compiler, fleen-vm | Spans name their file; multi-file diagnostics render `--> file:line:col` | — |
| V02 | F1 `struct` | Lex → VM | fleen-compiler, fleen-verify, fleen-vm | struct decl/literal/field access, `Value::Struct`, opcodes 0xB0–0xB3, `struct_defs` table, `struct_demo.fln` | V01 |
| V03 | F2 `[T]` arrays | Lex-free → VM | fleen-compiler, fleen-verify, fleen-vm | array literal, index read/write, `Value::Arr`, opcodes 0xB4–0xB8, `array_demo.fln` | V02 (shares the aggregate-type pattern; V01) |
| V04 | F3 `for` + `Range` | Lex → VM | fleen-compiler, fleen-verify, fleen-vm | `for x in expr {}`, `Range`, desugar lowering, opcodes 0xB9–0xBB, `for_range.fln` | V03 (arrays are the primary iterable) |
| V05 | F5 module system | Lex → driver | fleen-compiler, fleen-vm | `import` resolves files, `pub` visibility, module table, cycle detection, prelude, project-mode e2e harness | V01 (**hard**), V02 (`pub struct`) |
| V06 | F6 `std` library | builtins + VM | fleen-compiler, fleen-vm | `std/` Fleen sources, intrinsics table, schematic `[T]` signatures, prelude live, `tests/std/` | V05 (**hard**: std is modules), V02–V04 (std uses them) |
| V07 | F4 iterator helpers | std only | fleen-compiler (compile-only) | `std.collection.iter`: `map`/`filter`/`fold`/`take`/`zip` (eager), `iter_helpers.fln` | V06 |
| V08 | F7 `flint` | tooling | flint-cli (new) | `flint.toml`/`Flint.lock`, git resolution + cache, `new/build/run/test/check`, `depdemo` e2e | V06 (**hard**: projects need std) |
| V09 | Testing & compatibility | integration | all | full §8 matrix, v1/v2 `.flnc` fixtures, byte-for-byte regression | V08 |
| V10 | Docs & release | wrap-up | docs, Cargo.toml | BYTECODE/DESIGN/EBNF/SPEC v0.0.3, CHANGELOG, version bump | V09 |

> **Phase boundaries unchanged** (SPEC §6): the module table is a sub-pass within Resolve
> (`resolver/modules.rs`), the `for` desugar happens within Lower — no new independent phase.

---

## V01: Foundation — `FileId` / `SourceMap` (Multi-File Diagnostics)

> **Why first**: every later ticket (modules especially) produces diagnostics that must name
> their file. Doing the mechanical refactor before any feature work keeps feature tickets free
> of refactor noise. PLAN §3.4.5.

### Files involved
```
fleen-compiler/src/
├── lexer/token.rs        # Span gains file identity
├── source_map.rs         # new: SourceFile / SourceMap / FileId
├── lib.rs                # compiler entry gains add_file(name, src) -> FileId
└── (all stages)          # Span construction/consumption sites — mechanical

fleen-vm/src/
└── main.rs / diag path   # CLI resolves FileId → file name for --> rendering
```

### Design
```rust
/// Newtype per SPEC §5 ID rules (IDs start at 0, contiguous).
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct FileId(u32);

pub struct SourceFile {
    pub name: String,     // path as given on the command line / module path
    pub source: String,
}

/// Held by the compiler session; index = FileId.
pub struct SourceMap { files: Vec<SourceFile> }
```

- **`Span` carries the `FileId`** (PLAN §3.4.5 recommendation): `Span { file: FileId, start, end }`.
  Every span consumer gets file identity for free; the alternative (file on `Diagnostic` only)
  would leave `MIR`/bytecode spans ambiguous. Settled finally here.
- Single-file API unchanged: `cargo fln file.fln` registers exactly one file (`FileId(0)`);
  all existing diagnostics render identically.
- Diagnostic rendering gains the file name: `--> src/net/tcp.fln:12:5`; when running a `.flnc`
  directly, degrade exactly as 0.0.2 does (function name + instruction offset).
- The refactor is **one PR, no assertion-semantics changes** (the 0.0.2 U06 span-sinking
  precedent): constructors gain a file argument, tests pass `FileId(0)`.

### Tests
- Unit: two files, an error in each, rendered `-->` names both correctly; line/column mapping
  per file (tabs/newlines at file boundaries)
- All existing compiler/VM tests pass unchanged (single-file, `FileId(0)`)
- 0.0.1/0.0.2 e2e regression: output byte-for-byte

### Acceptance
```
cargo test
cargo fmt -- --check && cargo clippy --workspace --all-targets -- -D warnings
```

> **Implemented** (2026-10-11). Everything in this section landed as specified: `Span` carries
> `FileId` (design above + PLAN §3.4.5), `Session::add_file`/`compile` in `lib.rs`, single-file
> `compile(&str)` unchanged on `FileId(0)`, fmt/clippy/tests/e2e all green, e2e byte-for-byte.
> Deviations, recorded:
> - **fleen-vm wiring deferred to V05.** The "Files involved" entry for fleen-vm becomes
>   relevant only when a compilation spans files; until then the single-file `FileId(0)`
>   convention keeps every diagnostic byte-identical, so the VM keeps its own `diag` path.
>   The deliberate `locate`/`render_location` duplication between
>   `fleen_compiler::SourceMap` and `fleen-vm`'s `diag` is pinned by a cross-consistency test
>   (`fleen-vm/src/diag.rs::compiler_source_map_and_vm_diag_agree`) until V05 unifies them.
> - **One semantic fix beyond the mechanical refactor**: builtin stub bodies now carry
>   `Span::SYNTHETIC` instead of `Span::new(0, 0)` (`lower/mod.rs`). The old zero span was
>   recorded in builtin span_maps as a junk entry pointing at byte 0 of the user file; the
>   encoder already skips synthetic spans, so builtins now degrade to func-name + pc, which
>   is the intended behavior. `FileId::SYNTHETIC` (`u32::MAX`) was added as the sentinel's
>   file half, and `SourceMap::render_location` renders a `--> <synthetic>` placeholder
>   instead of panicking on such spans; both renderers now also clamp out-of-range /
>   mid-char span ends like `locate` does (found by the cross-consistency test, which
>   otherwise panicked on a bare `source[start..end]` slice).

---

## V02: F1 — `struct` Full Chain

> Vertical slice: declaration syntax → `Value::Struct` → e2e. Establishes the "add an owned
> aggregate" pattern (Value variant + opcode sub-group + is_copy rule) that V03 reuses.

### Files involved
```
fleen-compiler/src/
├── lexer/token.rs, lexer_impl.rs    # TokenKind::Struct ("struct" reserved, Breaking)
├── parser/ast.rs, stmt.rs, expr.rs  # StructDecl, struct literal (activate reserved rule), field access, p.f = e target
├── parser/asi.rs                    # struct literal `{` continuation (verify trap cases)
├── resolver/mod.rs                  # struct names enter the module's symbol table
├── typeck/infer.rs, error.rs        # Type::Struct(StructId), is_copy rule, field checks
├── lower/mir.rs, mod.rs             # StructNew / GetField / SetFieldLocal / SetFieldGlobal
└── codegen/bytecode.rs, encoder.rs, flnc.rs   # 0xB0–0xB3, version 3, struct_defs table

fleen-verify/src/verify.rs, stack_analysis.rs   # new instruction rules + struct-table cross-check
fleen-vm/src/value.rs, vm.rs                    # Value::Struct(Box<[Value]>), execution
```

### Lex
```rust
/// `struct` keyword (0.0.3: user-defined aggregate types)
Struct,
```
- `is_keyword` / `keyword_str` / `Display` in sync; **Breaking**: `struct` becomes a reserved
  word (`struct = 1;` is reported by the parser, case in `tests/parser/invalid/`)

### Parse
```rust
// declaration (no trailing semicolon, like func)
struct_decl  = "struct" , ident , "{" , [ field_decl , { "," , field_decl } ] , "}" ;
field_decl   = ident , ":" , type ;
// literal — the SYNTAX.ebnf rule reserved since 0.0.1, now activated:
//   struct_literal = ident , "{" , [ field_init , { "," , field_init } ] , "}" ;
// field access — activates the reserved `field` postfix; chains compose with call/index
```
- Assignment target set grows: `{ Ident, ExprDeref (0.0.2), FieldAccess (one level) }` —
  `p.f = e` where `p` is a place; deeper paths (`p.a.b = e`) are a **parse-level**
  `InvalidAssignmentTarget` (the one-level rule of PLAN D2 is enforced at the cheapest stage;
  typeck re-asserts for defense)
- No field-init shorthand (`Point { x, y }`) — `MissingField`/`UnknownField` are typeck; the
  parser rejects a bare ident in field position (`ExpectedFieldColon`)
- ASI traps (go into the trap table + tests): `p = Point` ⏎ `{ x: 1 }` joins across lines
  (`{` is a continuation token, existing rule 5) — this is **correct** (it is the literal);
  `struct Point` ⏎ `{` (Allman-style decl) is legal by the same rule

### Typeck
```rust
// struct table (id → definition), filled during resolution of declarations
pub struct StructDef { pub name: Ident, pub fields: Vec<(Ident, Type)>, pub span: Span }

impl Type {
    // D2: a struct is Copy iff every field is Copy (the Result<T, E> rule, generalized)
    // Type::Struct(id) => self.structs[id].fields.iter().all(|(_, t)| t.is_copy())
    // Type::Array(_) => false   (V03)
    // Type::Range => true       (V04)
}
```
- Field access `p.f`: field exists, struct visible (visibility arrives with V05 — single-file
  0.0.3 treats everything as visible), read = **copy of the field** (deep for owned fields);
  `move p.f` → `MoveOutOfStruct` (help: `clone p.f`)
- Literal: field set exactly matches the declaration — missing → `MissingField { name }`,
  unknown → `UnknownField { name }`; field types unify exactly (0.0.1 equality rule)
- `RecursiveStruct`: self-reference direct or indirect (including via `box`) — cycle path in
  the error (D1)
- Struct types extend the `box` pointee whitelist and `Result<T, E>` components (PLAN D3)

### New error kinds (typeck)
```rust
RecursiveStruct { path },
MissingField { struct_name, field },
UnknownField { struct_name, field },
MoveOutOfStruct,
FieldNotInPlace,        // p.f = e where p is not a local/global binding
```

### Lower
| Construct | MIR sequence |
|--------|----------|
| `Point { x: e1, y: e2 }` | `<e1>; <e2>; StructNew(id)` (n fields in declaration order) |
| `p.f` (read) | `<p>`; `GetField(offset)` — pushes a **copy** of the field (D2) |
| `p.f = e` (local p) | `<e>; SetFieldLocal(slot, offset)` — old field released |
| `p.f = e` (global p) | `<e>; SetFieldGlobal(gid, offset)` |

### Codegen / Verify
```rust
// 0xB0–0xB3 struct (byte-for-byte as BYTECODE.md §5 will record)
StructNew       = 0xB0,   // u16 struct_id, 3 bytes, f₁…fₙ → s
GetField        = 0xB1,   // u16 offset,     3 bytes, s → s v
SetFieldLocal   = 0xB2,   // u16 slot, u16 offset, 5 bytes, v →
SetFieldGlobal  = 0xB3,   // u16 gid,  u16 offset, 5 bytes, v →
```
- `Module.version = 3`; `flnc::from_bytes` accepts `{1, 2, 3}`; `to_bytes` appends the new
  **`struct_defs`** table (count + per-struct field_count + field type tags) per the
  append-only rule (PLAN §5.5)
- Verify: `StructNew` field count must equal the `struct_defs` entry (Δ = 1−n, min n — static,
  the count is the table entry); `GetField`/`SetField*` offsets < field count;
  **explicit** `V3OpcodeInV1V2Module` gate (the 0.0.2 U07 lesson: the gate is a comparison, not
  "naturally fine")
- Lockstep outside the ticket (or the fleen-vm build breaks): the VM `Opcode` match gains a
  wildcard arm + version gate widened to `{1, 2, 3}` (0.0.2 U07 finding)

### VM
```rust
Struct(Box<[Value]>),   // fields in declaration order; owned unless all-Copy (static)
```
- `StructNew` pops n values; `GetField` clones the field (deep for owned); `SetField*`
  overwrites (old value dropped)
- `Eq` reaching a `Struct` → defensive `RuntimeError::TypeMismatch` (typeck rejects struct
  `==` first; unreachable via legal bytecode)
- `DupDeep` generalizes to recurse into `Struct` (V03/V04 reuse)

### Tests
- lexer: the `struct` token; parser valid: decl (trailing comma, single-line), literal, field
  chains `w.payload` + `deref w.payload`, `p.f = e`, nested struct values; parser invalid:
  `struct = 1` (reserved), missing `{`, `p.a.b = e` (deep target), bare-ident field init
- typeck valid: Copy-only struct is Copy (assignment without `move`), owned-field struct needs
  `move`/`clone`, `clone p` deep-independence; invalid: one case per error kind above,
  `move p.f`, `==` on a struct, recursive struct (direct + via box)
- lower: each mapping row above, instruction-by-instruction; spans cover the source range
- codegen: opcode numbering/width assertions, u16 little-endian round-trips, struct_defs
  serialization round-trip; verify: valid/malformed for each instruction, v1/v2 regression,
  v3-gate; **full-chain smoke: compiler output passes verify** (the U07 practice)
- vm: field copy independence, deep clone of owned fields, defensive Eq

### E2E
- `tests/e2e/valid/struct_demo.fln` + `.expected`: declaration, literal, field read/write,
  Copy vs owned struct, move/clone semantics — full chain via `cargo fln`

### Acceptance
```
cargo test -p fleen-compiler -p fleen-verify -p fleen-vm
cargo fln tests/e2e/valid/struct_demo.fln
```

> **Implemented** (2026-10-11). Full chain landed: `TokenKind::Struct` (reserved word), struct
> decl/literal parsing with the block-head ambiguity resolved via a parser-wide
> `allow_struct_lit` flag (`parse_cond` disables the form in `if`/`while`/`choose` heads and arm
> guards; block bodies re-enable it), resolver `AssignTarget::Field` (one-level rule),
> typeck struct table (three-phase: names → cycle check + copy-ness in dependency order →
> field-type conversion), lower `StructNew`/`GetField`/`SetFieldLocal`/`SetFieldGlobal`
> (0xB0–0xB3, version 3, appended `struct_defs` table), verify cross-checks + explicit
> v3 gate, VM `Value::Struct` with lockstep version gate `{1, 2, 3}`. fmt/clippy/tests/doc
> green; all 0.0.1/0.0.2 e2e byte-for-byte; full-chain smoke (compiler output passes verify)
> added as a standing test. Deviations, recorded:
> - **`GetField` pops the struct (`s → v`, Δ0, min 1)** — not PLAN §5.1's peek form
>   (`s → s v`, Δ+1). The peek form leaves the struct under the field value with no
>   remove-below-top instruction; `p.f.g` chains require the pop. Erratum folds back into
>   PLAN/BYTECODE at V10 (the U07 precedent). Same for `SetField*`: Δ−1, min 1 — the offset
>   is an *operand*, not a stack item (PLAN §5.2's −2 row conflates them with `ArrSet*`,
>   which does take a stack index).
> - **`Type::Struct(Box<StructInfo>)`** instead of PLAN's bare `Type::Struct(StructId)`:
>   the shared info box carries the declaration name (error rendering has no table access)
>   and the cached D2 copy verdict, keeping the pure `Type::name()`/`is_copy()`/`is_owned()`
>   total. `StructId` is still what lowering emits.
> - **Verify cannot check `GetField`/`SetField*` field offsets** (the encoding carries no
>   struct id on those instructions — PLAN §5.1's "offsets < field count" verify claim is
>   not statically checkable): offsets are checked at runtime (`RuntimeErrorKind::
>   FieldOutOfRange`), and the offset range is pinned by a VM test.
> - **`deref` accepts any box-typed expression**, not just a bare `Ident` (0.0.2 behavior):
>   the ticket's own test list requires `deref w.payload`, whose operand is a field chain.
>   Read-through is a copy of a fresh value, so value semantics are unchanged. The write
>   form `deref w.payload = v` remains an `InvalidAssignmentTarget` (0.0.2 behavior; a box
>   interior inside a struct is V03+ territory).
> - **The one-level assignment rule is enforced in the resolver**, not the parser: the
>   existing `InvalidAssignmentTarget` for `f() = 1` lives there (`resolve_assign`), so
>   `p.a.b = e` reports from the same stage with the same error kind.
> - **Struct literals in global initializers** lower only when written in declaration order
>   (global init has no frame, hence no temp slots for the reorder); anything else reports
>   `ComplexGlobalInit`. Function-level literals reorder freely (source-order evaluation
>   into temps, declaration-order `StructNew`, covered by a dedicated test).
> - **`Eq` on a struct** is a typeck `InvalidOperand` error (PLAN §1.4); the VM's
>   `ArithTypeMismatch` backstop is pinned by a test.
> - The struct table lives in typeck (`collect_structs`), flows to lowering via
>   `TypedHir.structs`/`Mir.structs`, and to codegen as the `struct_defs` tags; the
>   resolver only collects raw declarations + `ref`-in-field-type legality.
> - **Struct declarations are top-level only**: a `struct` inside a block is a parse error
>   (scoped struct tables would buy nothing in 0.0.3). The ticket did not specify; the
>   parser-level rejection keeps the struct table flat.
> - **`FieldNotInPlace` (ticket's typeck error list) does not exist as a named kind** — its
>   four cases are covered by earlier, cheaper errors: deep paths and non-place receivers
>   (`f().x = e`) → resolver `InvalidAssignmentTarget`; undeclared object →
>   `UndeclaredVariable`; `ref`-param object → resolver `AssignToRefParam`; a non-struct
>   object that slips through → typeck `InvalidOperand { op: "field assignment" }`.
>   Recorded as an erratum so V10 does not "restore" a dead kind.
> - **By-value parameters are legal field-write targets** (`p.f = e` where `p: Point` is a
>   parameter): the callee owns its copy, so the write is local and safe. PLAN D2's
>   "local or global struct binding" is read as "any owned place binding"; `ref`
>   parameters stay read-only.
> - **Name-collision handling**: a duplicate struct name reports resolver
>   `DuplicateBinding`; a duplicate field inside one literal is rejected by the *parser*
>   (typeck re-asserts the field set with `MissingField`/`UnknownField`). Neither collision
>   case was specified in the ticket.
> - **`clone p.f` lowers to the plain field read** (typeck folds it into the `Field` node):
>   a read is already a copy (deep for owned fields), so the clone form adds nothing.
>   Documented in `typeck_clone`; only `move p.f` is an error.
> - **`p.f = e` as an expression yields the assigned value** (lower emits `Dup`/`DupDeep`
>   before `SetField*`), mirroring the existing `x = e` convention — the ticket's lower
>   table showed only `<e>; SetFieldLocal`, which would have desynced the statement-level
>   `Pop`. Assignment still types `Unit` and stays banned as a block tail.
> - **`SetFieldGlobal` skips the `mutable` gate** that `StoreGlobal` enforces: writing a
>   field does not rebind the global, so `const` struct globals are field-writable — the
>   same rule as `deref b = v` on a const box binding. (There is no copy-through problem
>   here, unlike the 0.0.2 global-box deref-assign restriction: `SetFieldGlobal` mutates
>   `globals[gid]` in place.)

---

## V03: F2 — `[T]` Arrays Full Chain

> Reuses V02's pattern. Arrays differ in one important way: **never Copy** (D3), and the
> variable-arity `ArrNew` exercises verify's operand-derived stack effect.

### Files involved
```
fleen-compiler/src/
├── parser/ast.rs, expr.rs           # array literal (primary position), a[i] = e target
├── parser/asi.rs                    # [ literal vs index trap cases (PLAN §3.2.2)
├── typeck/infer.rs, error.rs        # Type::Array, uniform-element rules, index checks
├── lower/mir.rs, mod.rs             # ArrNew / ArrLen / ArrGet / ArrSetLocal / ArrSetGlobal
└── codegen/bytecode.rs, encoder.rs, flnc.rs   # 0xB4–0xB8

fleen-verify/src/verify.rs, stack_analysis.rs
fleen-vm/src/value.rs, vm.rs        # Value::Arr(Box<[Value]>), IndexOutOfRange
```

### Parse
```rust
array_literal = "[" , [ expr , { "," , expr } ] , "]" ;   // primary position
```
- Assignment target set grows again: `a[i] = e` (index write; bounds are a **runtime**
  concern, the parser only checks the shape)
- ASI traps (documented + tested): `f()` ⏎ `[0]` = index chain (unchanged 0.0.2 rule);
  `x =` ⏎ `[1, 2]` = literal continuation (same predicate, different parse position —
  postfix-position `[` is index, primary/expression-position `[` is literal; the distinction
  the parser already makes, now also at statement start: `[1, 2];` is a legal discarded
  expression statement)

### Typeck
- `Type::Array(Box<Type>)`; element type infers from the first element, later elements must
  unify exactly → `ArrayTypeMismatch { expected, found }`; empty literal `[]` without
  annotation → `CannotInferArrayType` (help: annotate `x: [int] = [];`)
- `is_copy([T]) = false` **always** (D3) — assert exhaustively in the is_copy test
- `a[i]`: `i: int` else `IndexNotInt { found }`; read = bounds-checked **copy** of the element
  (deep for owned elements); `move a[i]` → `MoveOutOfArray`; write `a[i] = e` via place rules
  (local/global binding; owned `e` needs `move`/`clone`)
- Ragged rows legal: `[[1, 2], [3]]: [[int]]` (each row its own array)
- `box<[T]>` and arrays as struct fields / `Result` components / `ref [T]` parameters: legal
  (extends V02's whitelist work; `ref` parameters only, as 0.0.2)

### New error kinds (typeck)
```rust
CannotInferArrayType { span },
ArrayTypeMismatch { expected, found },
IndexNotInt { found },
MoveOutOfArray,
```

### Lower
| Construct | MIR sequence |
|--------|----------|
| `[e1, …, en]` | `<e1>; …; <en>; ArrNew(n)` |
| `len-of` (lowering-internal, feeds for/std) | `<a>; ArrLen` (array stays on stack) |
| `a[i]` (read) | `<a>; <i>; ArrGet` — bounds-checked copy |
| `a[i] = e` (local a) | `<e>; <i>; ArrSetLocal(slot)` |
| `a[i] = e` (global a) | `<e>; <i>; ArrSetGlobal(gid)` |

### Codegen / Verify
```rust
// 0xB4–0xB8 arrays (numbering finalized here; PLAN §5.1's "TBD" row is resolved —
// V10 folds this back into PLAN/BYTECODE, the 0.0.2 U07 erratum precedent)
ArrNew        = 0xB4,   // u16 n, 3 bytes, e₁…eₙ → a
ArrLen        = 0xB5,   // 1 byte, a → a int
ArrGet        = 0xB6,   // 1 byte, a i → a v (bounds-checked)
ArrSetLocal   = 0xB7,   // u16 slot, 3 bytes, v i →
ArrSetGlobal  = 0xB8,   // u16 gid,  3 bytes, v i →
```
- Verify stack effects: `ArrNew` Δ = 1−n, min n (n is the **operand** — static, no abstract
  interpretation); `ArrLen`/`ArrGet` Δ+1 min 1; `ArrSet*` Δ−2 min 2; slot/gid bounds checks
  as usual
- VM `IndexOutOfRange` carries a span (the faulting `ArrGet`/`ArrSet*` instruction's span via
  the 0.0.2 span_map mechanism — line/column rendering comes free from V01)

### VM
```rust
Arr(Box<[Value]>),   // always owned
```
- `ArrGet` bounds check → `RuntimeError::IndexOutOfRange { index, len }`; `DupDeep` recurses
  into `Arr` (deep element copy)

### Tests
- parser valid: nested literals, empty annotated, `a[i] = e`, literal statement `[1, 2];`;
  invalid: `a[]`, `a[1` , `[]` unannotated binding flows to typeck (parser accepts)
- typeck: uniformity (mixed `[1, "x"]` → `ArrayTypeMismatch`), empty literal inference,
  `is_copy` exhaustive, `move a[i]`, index write place rules, ragged rows
- lower/codegen/verify: each row + `ArrNew` operand-derived depth (n = 0/1/3), malformed
  modules (slot/gid OOB, v3-gate), full-chain smoke incl. all array paths
- vm: bounds OOB error with span rendering (V01), deep-clone independence, empty array

### E2E
- `tests/e2e/valid/array_demo.fln` + `.expected`: literal, index read/write, clone
  independence, move semantics, nested arrays

### Acceptance
```
cargo test -p fleen-compiler -p fleen-verify -p fleen-vm
cargo fln tests/e2e/valid/array_demo.fln
```

---

## V04: F3 — `for` + `Range` Full Chain

> Closes the aggregation trilogy: `for` is the first consumer of arrays and ranges.
> No runtime iterator objects — the desugar lives in Lower (PLAN D4/§5.4).

### Files involved
```
fleen-compiler/src/
├── lexer/token.rs, lexer_impl.rs    # TokenKind::For, TokenKind::In (both reserved, Breaking)
├── parser/ast.rs, stmt.rs           # StmtFor { var, iterable, body, span }
├── parser/asi.rs                    # For → start set + block-like-header climb; In → continuation set
├── typeck/infer.rs, error.rs        # Type::Range, iterable closed-set check, loop-var binding
├── lower/mod.rs                     # for-desugar (PLAN §5.4)
└── codegen/bytecode.rs, encoder.rs  # 0xB9–0xBB

fleen-verify/src/verify.rs, stack_analysis.rs
fleen-vm/src/value.rs, vm.rs        # Value::Range { start, end }
```

### Lex + Parse
```rust
For,   // `for` keyword
In,    // `in` keyword (iterable position separator)

// for_stmt = "for" , ident , "in" , expr , block ;
```
- ASI predicate updates (pure functions, exhaustively unit-tested): `For` joins
  `can_start_stmt` and the block-like-header continuation climb (the body may hug `}`, and the
  header may span lines); `In` joins `can_continue_expr` — `for x` ⏎ `in names { }` is legal
- `for` takes a semicolon as an expression statement, omittable per 0.0.2 rules (same as
  `while`)
- **Breaking**: `for` / `in` become reserved words

### Typeck
- `Type::Range` (Copy: two i64; constructed only via std functions — V06; until V06 lands the
  type exists and `MakeRange` is unreachable from source, which keeps this ticket's tests
  self-contained)
- Iterable check: scrutinee type ∈ `{ [T], Range }` → else `NotIterable { got }`, help names
  both options (D4's closed set; user iterables wait for traits — the help says so)
- Loop variable: fresh **immutable** binding per iteration, scoped to the body; body
  assignments follow the §4.5 loop-scope rules (same machinery as `while`)
- Iterable expression evaluated once (falls out of the desugar; note it in DESIGN.md)

### Lower (desugar — MIR stays loop-flattened)
```text
for x in expr { body }
→  <expr>; StoreLocal __it
   Const 0; StoreLocal __i            // Range: __i = RangeStart, bound = RangeEnd (once)
   L_loop: <len-or-bound check>; JumpIfFalse L_end
   <element copy>; StoreLocal x
   <body>
   __i += 1; Jump L_loop
   L_end:
```
- Array: `LoadLocal __it; ArrLen` per-iteration or hoisted to a `__n` local (either is
  correct; hoist — it is one instruction and cannot change)
- Range: `MakeRange`-produced values destructure via `RangeStart`/`RangeEnd` **once, before
  the loop**; the loop body is a plain int loop

### Codegen / Verify
```rust
// 0xB9–0xBB range
MakeRange    = 0xB9,   // 1 byte, start end → r
RangeStart   = 0xBA,   // 1 byte, r → r int
RangeEnd     = 0xBB,   // 1 byte, r → r int
```
- Verify: `MakeRange` Δ−1 min 2; `RangeStart`/`RangeEnd` Δ+1 min 1; v3-gate shared with V02

### VM
```rust
Range { start: i64, end: i64 },   // Copy
```

### Tests
- lexer: `for`/`in` tokens; parser valid: `for` over array/range literals, header spanning
  lines, body hugging `}`; invalid: `for x` (no `in`), `for = 1` / `in = 1` (reserved),
  `for x in` (no iterable)
- ASI trap table additions: `for i` ⏎ `in range(0, 3)` ⏎ `{ … }`; `for x in a { }` ⏎ `print(1)`
  (statement boundary after the block)
- typeck: `for x in 42` → `NotIterable`; loop-var immutability; §4.5 assignment semantics in
  the body; empty-iterable body never runs
- lower: desugar shapes for array and range, instruction-by-instruction (PLAN §5.4)
- verify/vm: the three range instructions; **full-chain smoke** with `for` over both iterables
  (until V06, `MakeRange` is exercised via hand-built MIR/codegen tests, not source)

### E2E
- `tests/e2e/valid/for_range.fln` + `.expected`: `for` over an array and (after V06) a range —
  this file is **extended by V06** when `std.collection.range` lands (noted in V06; the V04
  version covers arrays only)

### Acceptance
```
cargo test -p fleen-compiler -p fleen-verify -p fleen-vm
cargo fln tests/e2e/valid/for_range.fln
```

---

## V05: F5 — Module System Full Chain

> The first multi-file release feature. `pub` belongs to this ticket (not the lexer ticket —
> there is no lexer ticket in this split; each feature brings its own tokens).

### Files involved
```
fleen-compiler/src/
├── lexer/token.rs, lexer_impl.rs    # TokenKind::Pub ("pub" reserved, Breaking)
├── parser/stmt.rs                   # pub modifier on top-level declarations; PubNotAllowedHere
├── resolver/modules.rs              # NEW: module table sub-pass (the core of this ticket)
├── resolver/mod.rs                  # qualified-path resolution through the module table; PrivateAccess
├── error.rs                         # ResolveErrorKind additions
└── lib.rs                           # project-mode entry: compile_files(root_file, sourcemap)

fleen-vm/src/main.rs                # project-mode flag for the one-stop driver (e2e harness support)
tests/e2e/projects/                 # NEW: multi-file e2e harness (see Tests)
```

### Lex + Parse
```rust
Pub,   // `pub` keyword (0.0.3: visibility modifier)

// grammar: declaration = [ "pub" ] , ( const_decl | func_decl | struct_decl | var_binding ) ;
```
- `pub` is legal **only** on top-level items; anywhere else (locals, inside blocks, on
  `import`) → parse error `PubNotAllowedHere` (PLAN D6)
- **Breaking**: `pub` becomes a reserved word

### resolver/modules.rs (new file — module table sub-pass)
```rust
/// Build the module table before per-module resolution (PLAN §4.3; not a new phase —
/// the same sub-pass precedent as 0.0.2's ownership-in-typeck).
pub(crate) fn build_module_table(
    root: FileId,
    sources: &SourceMap,
    std_root: &StdRoot,           // embedded std (V06 fills it; until then empty)
) -> Result<ModuleTable, ResolveError>;

pub struct ModuleTable {
    pub modules: IndexVec<ModuleId, ModuleDef>,   // id → path, FileId, imports, exports
}
```
- Resolution algorithm (PLAN §3.4.1): BFS over `import` declarations; module path → file path
  (`a.b` → `src/a/b.fln` or `src/a/b/mod.fln`); `std.*` → the embedded std root; missing →
  `ModuleNotFound { path, tried: [both candidates] }`; import-graph cycle → `CircularImport`
  with the full path (`a → b → a`)
- **Prelude injection** (D7): one implicit `import std.io;` recorded at the crate root —
  until V06 fills the std root, the prelude resolves to the existing builtin `print` only
  (behavior-identical to 0.0.2, the compatibility requirement)
- Import semantics: no selected imports / renaming / globs (§1.4); duplicate import of the
  same path is idempotent; a local binding shadowing a module path resolves with the existing
  shadowing machinery (module paths are just names)

### Cross-module resolution (resolver/mod.rs)
- `a.b.f` resolves left-to-right through the module table; each non-final segment must be a
  module; the final segment must be a **`pub`** item → `PrivateAccess { item, module }` with
  help pointing at the declaration
- Ambiguity is impossible in 0.0.3: cross-module uses are always qualified (no unqualified
  imports)

### New ResolveErrorKind entries
```rust
ModuleNotFound { path, tried },
CircularImport { cycle: Vec<String> },
PrivateAccess { item, module, decl_span },
```

### Project-mode driver
- `fleen-vm` CLI gains `cargo fln --project <dir>` (reads `flint.toml` if present, else treats
  the dir as `src/` root): compile → verify → execute, all diagnostics multi-file
  (**used by the e2e harness now and by flint in V08** — flint is a thin wrapper over this)
- `cargo fln file.fln` (single file) is byte-for-byte unchanged

### Tests
- resolver unit: table build over a fixture tree (flat file vs `mod.fln`), cycle detection
  (2- and 3-cycles), `std.` reserved-root dispatch, prelude injection recorded once
- resolver invalid: `ModuleNotFound` (both candidates listed), `CircularImport`,
  `PrivateAccess` × {func, const, struct, global}
- parser: `pub` on top-level items; invalid: `pub` on a local binding / inside a block
- e2e **project mode** (new harness, `tests/e2e/projects/<name>/` with `src/` + `.expected` +
  `.exit` + `.stderr`):
  - `netdemo/`: `src/main.fln` + `src/net/mod.fln` + `src/net/tcp.fln` — cross-module calls,
    pub/private mix, a struct (V02) and an array (V03) crossing module boundaries
  - `circular/` (invalid): two-file import cycle, `.stderr` contains the cycle path
- Regression: all single-file e2e unchanged

### Acceptance
```
cargo test
cargo fln --project tests/e2e/projects/netdemo
```

---

## V06: F6 — `std` Library

> std rides the exact user pipeline (D8): Fleen source in `std/`, embedded into the compiler
> binary, compiled on demand. Only VM-unwritable primitives are intrinsics.

### Files involved
```
std/                                 # NEW: Fleen sources (embedded via include_str! at build time)
├── mod.fln                          # root: prelude re-exports
├── io.fln                           # print (alias), println, read_line
├── str.fln                          # len, concat, split, from_int, from_float, from_bool
├── num.fln                          # abs, min, max (Fleen) · sqrt, floor (intrinsics)
├── arr.fln                          # len, push, get (Result), set
├── collection/
│   ├── mod.fln
│   └── range.fln                    # range(n), range_from(a, b) → Range
├── os.fln                           # args, exit
└── time.fln                         # now_millis

fleen-compiler/src/
├── typeck/builtins.rs               # intrinsic signatures + schematic [T] mechanism
├── resolver/modules.rs              # StdRoot filled with the embedded sources
└── build.rs (or lib.rs)             # embed std/ at compile time

fleen-vm/src/
├── builtins.rs (new, or extends dispatch in vm.rs)   # intrinsic implementations
└── vm.rs

tests/std/                           # NEW: std exercised directly in cargo test
```

### Intrinsics (concrete signatures; the 0.0.2 builtins-table mechanism, extended)
```rust
// builtins.rs additions — every entry is a real signature; infer.rs stays clean (the
// 0.0.2 print-fix discipline; DoD checks this with a grep)
println:    Variadic(&[Type::String]) -> unit,
read_line:  Fixed(&[])                -> string,
sqrt:       Fixed(&[Type::Float])     -> float,
floor:      Fixed(&[Type::Float])     -> int,
args:       Fixed(&[])                -> Array(Box::new(Type::String)),
exit:       Fixed(&[Type::Int])       -> unit,      // terminates the VM (code = the int)
now_millis: Fixed(&[])                -> int,
```

### Schematic (type-variable) signatures — the bounded polymorphism of 0.0.3
```rust
// Type variables appear ONLY in this table, never in user syntax (PLAN §10 risk rule).
// Unification: one call site, T unifies with the argument — the Ok/Err context-inference
// mechanism of 0.0.2, extended to the table.
len:  ∀T. ([T]) -> int,
get:  ∀T. ([T], int) -> Result<T, string>,
set:  ∀T. ([T], int, T) -> [T],
push: ∀T. ([T], T) -> [T],
```
- Hard rule (review gate): a push beyond one-call unification of table variables is rejected
  as 0.1.0 scope (generics)
- `std.arr.get/set` are **pure Fleen** over `ArrLen`/`ArrGet` with an explicit bounds check
  returning `Result` (errors are values, per DESIGN.md §8 — the VM `IndexOutOfRange` path
  stays for the raw `a[i]` syntax only)

### std Fleen sources (highlights)
- `std.collection.range.fln`:
  ```fleen
  pub func range(n: int): Range { /* MakeRange via the compiler — see note */ }
  pub func range_from(a: int, b: int): Range;
  ```
  > **Construction note**: `Range` has no user-facing constructor syntax (D5). The two
  > functions are intrinsics-with-Fleen-signatures: the table maps `range`/`range_from` to
  > `MakeRange` lowering directly (the `Ok`/`Err` constructor mechanism — recognized names,
  > shadowable, context-free here since the types are concrete).
- `std.str.from_int` = the existing `ToStr` path (`as string` shares it — no drift, the
  0.0.2 fmt-helper discipline)
- `std.io.print` is the prelude `print` (same builtin; `mod.fln` re-exports it)
- Every std module carries a header comment documenting its 0.0.3 limits (eager iter —
  see V07; no file I/O; no HashMap)

### Prelude
- The V05 implicit import now resolves into the embedded std: `print` (and only `print`) is
  visible without any import; **every 0.0.1/0.0.2 program compiles unchanged** (the only
  breaks remain the reserved words of V02/V04/V05)

### VM
- Intrinsic dispatch: the builtins call path gains the seven implementations above
  (I/O via `std::io`, clock via `std::time`, `exit` terminates with the given code —
  exit-code contract unchanged: 0/1/2 reserved for compile/verify/runtime errors, `exit(n)`
  passes n through)

### Tests
- `tests/std/<module>.fln + .expected` — std compiled and run as part of `cargo test`
  (it cannot rot; PLAN DoD item)
- typeck: schematic unification (`std.arr.len` on `[int]` and on `[string]`; mismatched
  `push(["a"], 1)` → `ArrayTypeMismatch`), `range`/`range_from` construction, intrinsic
  arity/type errors
- vm: `println` output, `read_line` (fed stdin), `args`, `exit(3)` → process exit code 3,
  `now_millis` monotonic sanity, `sqrt`/`floor` values
- e2e: `tests/e2e/valid/std_io.fln`, `std_str.fln`; **extend** `for_range.fln` with range
  iteration (the V04 note)

### Acceptance
```
cargo test
cargo fln tests/e2e/valid/std_io.fln
grep -n "special\|HACK" fleen-compiler/src/typeck/infer.rs   # empty: no std special cases
```

---

## V07: F4 — Iterator Helpers (`std.collection.iter`)

> The smallest ticket — pure Fleen, zero compiler changes (if any compiler change is felt to be
> needed, it is a smell: re-read PLAN F4's "not a language feature" note).

### Files involved
```
std/collection/
├── mod.fln                 # re-export iter
└── iter.fln                # NEW

tests/std/iter.fln + .expected
tests/e2e/valid/iter_helpers.fln + .expected
```

### API (eager — arrays in, arrays out; lazy waits for 0.1.0 generics, D8/§6.5)
```fleen
pub func map[T, U](xs: [T], f: (T) -> U): [U];
pub func filter[T](xs: [T], pred: (T) -> bool): [T];
pub func fold[T, A](xs: [T], init: A, f: (A, T) -> A): A;
pub func take[T](xs: [T], n: int): [T];
pub func zip[T, U](xs: [T], ys: [U]): [[T]];   // ⚠ see design note
```
- **`zip` design note (settle in review)**: pairs need a two-element representation; 0.0.3 has
  no tuples. Options: (a) drop `zip` from 0.0.3, (b) return `[.choice]` via `Result<T, U>`-like
  tagging — both are awkward. **Recommendation: drop `zip`**, add with tuples/generics.
  Documented here so the review decides, not the implementer.
- All functions are implemented with `for` + `std.arr.push` (function values are Copy —
  0.0.2's classification pays off here); `map`/`filter` preallocate
  (`Vec::with_capacity` is the compiler's business, not std's — std just builds arrays)
- These are the schematic-signature table entries consumed from source — the first real users
  of V06's mechanism

### Tests
- `tests/std/iter.fln`: each function over `[int]` and `[string]` (Copy and owned elements —
  owned-element moves through the closure position are the interesting case: `f` receives a
  copy, the source array is untouched — D3/D4 read semantics)
- e2e `iter_helpers.fln`: a `range(0, 5)` → `map` → `filter` → `fold` chain, output anchored

### Acceptance
```
cargo test
cargo fln tests/e2e/valid/iter_helpers.fln
```

---

## V08: F7 — `flint` Package Manager

> A new workspace crate using `fleen-compiler` / `fleen-vm` as libraries (D9). Git-only
> dependencies; the lockfile is the pin.

### Files involved
```
flint-cli/                       # NEW crate (workspace member)
├── Cargo.toml
└── src/
    ├── main.rs                 # CLI: new/build/run/test/check
    ├── manifest.rs             # flint.toml parse + validation
    ├── lock.rs                 # Flint.lock read/write
    ├── resolve.rs              # dependency graph, fetch, cache
    └── build.rs                # project-mode compile invocation (V05's entry)

tests/e2e/projects/depdemo/      # NEW: git-fixture dependency e2e
```

### Manifest and lockfile
```toml
# flint.toml
[package]
name = "myproj"
version = "0.1.0"

[dependencies]
jsonlite = { git = "https://github.com/user/jsonlite", version = "0.2" }
```
- Validation: package name is a valid module root (ident); dependency names unique; `version`
  is advisory metadata in 0.0.3 (D9) — the **lockfile is the pin**
- `Flint.lock`: `name → { git, commit, version }`; committed; a manifest change dirties the
  lock → re-resolve on next build
- External crate policy: a small TOML parser crate is acceptable (pure Rust, no transitive
  heaviness); **git access shells out to the `git` CLI** (no `git2` C dependency; git is
  ubiquitous — record this decision here, revisit if a pure-Rust need appears)

### Resolution algorithm
```text
resolve(manifest) →
  for each dep: lock hit with matching git URL? → reuse commit
                else: ls-remote / clone into ~/.flint/cache/<name>-<commit>/
  read the dep's flint.toml recursively (deps of deps)
  cycle in the dependency graph → error with the cycle path
  write Flint.lock
```
- Dependency sources are **read-only** to the build: their modules import as
  `<crate-name>.<module>` (crate name is the import root, after `std`); their private items
  are invisible; a dependency importing the depending project → error
- Cache layout: `~/.flint/cache/<name>-<commit>/` (content-addressed by commit; version is
  display-only)

### Commands
| Command | Behavior |
|------|------|
| `flint new <name>` | Scaffold `flint.toml` + `src/main.fln` (hello world) + git init |
| `flint build` | resolve → compile whole program (user + deps + std) → `.flnc` |
| `flint run` | build + execute |
| `flint test` | run the project's `tests/` e2e cases (`.fln` + `.expected` contract) |
| `flint check` | build + verify, no execution |

- `flint build/run` call the V05 project-mode entry as a library; `cargo fln file.fln` is
  untouched
- `flint test` reuses the e2e harness conventions (`.expected` line-by-line comparison,
  `.exit`/`.stderr` for invalid)

### Tests
- Unit: manifest validation (bad name, duplicate deps, missing git), lockfile round-trip,
  dirty-lock re-resolution, cache path layout, dependency-graph cycle detection
- Integration with **local `file://` git fixtures only** (no network in CI; the fixture repos
  are created by the test setup script): fetch → cache → build → run
- e2e `depdemo/`: a project depending on a fixture crate; `main.fln` imports
  `depdemo-lib.util.helper`, output anchored; **private-access-across-crates** invalid case
- `flint new` smoke: scaffold → `flint run` → hello output

### Acceptance
```
cargo test -p flint-cli
cargo run -p flint-cli -- new /tmp/flint-smoke && cd /tmp/flint-smoke && flint run
```

---

## V09: Testing and Compatibility (Integration Pass)

> Per-feature tickets already carry their tests; this is the whole-repo integration and
> compatibility gate (the U11 role of 0.0.2).

### Deliverables
- **Full §8 matrix check** (PLAN §8.1): walk the table stage by stage; any gap found → add the
  case here (or in the owning ticket if it is still open)
- **Compatibility fixtures**:
  - v1 **and** v2 `.flnc` fixed-byte fixtures execute on the 0.0.3 verify/VM
    (0.0.2 kept one v1 fixture; now both)
  - a v3 fixture exercising every new instruction group (struct/array/range) end-to-end
  - `from_bytes` rejects: v3 opcode in a v1/v2 module, `struct_defs` count mismatch,
    truncated appended table
- **Byte-for-byte regression**: every 0.0.1/0.0.2 e2e file runs with identical output
  (the only permitted diffs are the reserved-word breaks, and none of the existing test
  programs use `struct`/`for`/`in`/`pub` as identifiers — assert this with a compile sweep)
- **Reserved-word sweep**: grep the whole repo's `.fln` examples/tests for the four new
  reserved words used as identifiers
- `cargo test` runs std (V06) + project-mode e2e (V05) + flint fixture tests (V08) — one
  command, everything
- Coverage cross-check against PLAN §12 DoD items; file the gaps

### Acceptance
```
cargo test && cargo doc --no-deps
cargo fmt -- --check && cargo clippy --workspace --all-targets -- -D warnings
```

---

## V10: Wrap-up — Docs, Version, Release

### Doc sync (PLAN §11 checklist, verified against the as-built code)
- [ ] `docs/BYTECODE.md` → v0.0.3: §3.2 Value variants, §4.2 opcode space (0xB0–0xBB group),
      §5 new instruction tables (with the **numbering finalized in V03** — `ArrSetGlobal` =
      0xB8, range = 0xB9–0xBB; PLAN §5.1's TBD row resolved), §8 stack effects, §10 version
      table, `struct_defs` table official
- [ ] `docs/DESIGN.md`: §9 composite types (`[T]`, `Range`), new "Structs" / "Modules" /
      "std and the prelude" chapters, §6 `for`, §17 feature table, §18 roadmap (0.0.3
      complete), known-Hacks still empty, **D1–D10 recorded as settled**
- [ ] `docs/SYNTAX.ebnf` v0.0.3: struct_decl, pub, for_stmt, array_literal, field access,
      new assignment targets
- [ ] `SPEC.md`: repo structure (+ `std/`, `flint-cli/`, `tests/std/`, `tests/e2e/projects/`),
      crate table (+ flint-cli), §14 quick reference (struct/for/import examples), §15
      feature table 0.0.3 column
- [ ] `docs/0.0.3/PLAN.md`: fold in as-built corrections (numbering finalization, any
      D-decision notes from review)
- [ ] New error-message samples compared against actual output (`MoveOutOfStruct`,
      `CircularImport`, `PrivateAccess`, `NotIterable`)

### Version and release
- [ ] Bump all four crates' `Cargo.toml` → `0.0.3`
- [ ] `CHANGELOG.md`: 0.0.3 entry with a **Breaking section** (reserved words
      `struct` / `for` / `in` / `pub`)
- [ ] `README.md`: version line, feature overview, **flint quick start** (`flint new` →
      `flint run`)
- [ ] `cargo doc --no-deps` no warnings; public APIs of `flint-cli` documented
- [ ] CI green → tag → release archives (0.0.1/0.0.2 process)

### Acceptance
```
cargo fmt -- --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test && cargo doc --no-deps
```

---

## Dependencies and Parallelization

```
V01 (FileId/SourceMap)
  ↓ (hard for V05; V02/V03 only soft — single-file work can start immediately)
V02 (struct) → V03 (arrays) → V04 (for+Range)      # one pattern, three passes; serial is
  ↘                    ↘                ↘           # cheapest for a solo dev (shared Value/
V05 (modules, hard dep on V01)          (e2e refs)  # opcode pattern is fresh in mind)
  ↓ (hard: std IS modules)
V06 (std) ←── also needs V02–V04 (std uses structs/arrays/for)
  ↓
V07 (iter helpers)        V08 (flint)   # V07 and V08 are parallelizable after V06
       ↘                    ↙
V09 (integration) ←── both
  ↓
V10 (release)
```

- **Suggested serial order for a solo developer**:
  V01 → V02 → V03 → V04 → V05 → V06 → V07 → V08 → V09 → V10
  (the aggregation trilogy back-to-back reuses one mental pattern; modules before std because
  std is written *as* modules; flint last because it consumes everything)
- **Two developers in parallel**: A takes V01 → V02 → V03 → V04 (aggregation), B starts V01
  review then V05 preparation (module-table design against fixture trees); they meet at V05;
  B then takes V08 while A takes V06 → V07

---

## Quality Gates (every ticket must pass)

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

- The prohibited list still applies (SPEC §11): no `unwrap()`/`expect()` in non-test code,
  no `Rc`/`RefCell`, errors carry Spans, public APIs documented
- **Run the full 0.0.1/0.0.2 regression before merging every ticket** — the existing e2e set
  staying byte-for-byte identical is a hard gate (reserved-word breaks may only appear where
  the PLAN Breaking list says so)
- **Compiler output must pass verify** — every feature ticket's full-chain smoke includes the
  verify pass (the U07 practice, now standing policy)
- std is never special-cased: `grep` for std names in `infer.rs` must stay empty (V06/V07
  acceptance)

---

## Milestones

| Milestone | Tickets | Marker |
|--------|------|------|
| M0 Foundations | V01 | Multi-file diagnostics render; 0.0.1/0.0.2 regression byte-for-byte |
| M1 Aggregation types | V02–V04 | `struct_demo.fln` / `array_demo.fln` / `for_range.fln` run end to end; v3 opcodes + struct_defs table land |
| M2 Modules | V05 | `netdemo` project e2e green; privacy + cycle errors carry file:line:col |
| M3 std | V06–V07 | `tests/std/` green in CI; prelude compat; `iter_helpers.fln` green |
| M4 flint | V08 | `depdemo` builds and runs with a `file://` git fixture dependency |
| M5 Integration & release | V09–V10 | Full matrix + fixtures green; docs synced; 0.0.3 released |

> Mapping to PLAN §9's milestones: M0–M1 ≈ PLAN M1, M2 ≈ PLAN M2, M3 ≈ PLAN M3+M4 (backend
> work is distributed inside the feature tickets instead of gathered in one place), M4 ≈ PLAN
> M5, M5 ≈ PLAN M6.

---

## Notes

- **Breaking change list** (the CHANGELOG ⚠️ counterpart): `struct` / `for` / `in` / `pub`
  become reserved words (0.0.2 programs using them as identifiers break; rename). Nothing else
  breaks: the prelude (`print`) is unchanged, explicit semicolons stay legal, and all 0.0.2
  source programs compile as-is modulo the reserved words.
- **v1/v2 compatibility promise**: `.flnc` v1/v2 modules remain executable on the 0.0.3
  verify/VM (locked by V09 fixtures); existing instruction numbers and encodings are unchanged,
  to the letter (BYTECODE.md §10 rule).
- **Opcode numbering finalized in this document** (supersedes PLAN §5.1's `ArrSetGlobal`-TBD
  row and its 0xB8–0xBA range numbers): struct 0xB0–0xB3, arrays 0xB4–0xB8, range 0xB9–0xBB;
  0xBC–0xBF and 0xA6–0xAF stay reserved. V10 folds this into PLAN/BYTECODE (the 0.0.2 U07
  erratum precedent: implementation documents win, docs are synced after).
- **PLAN §9 superseded**: this feature-based breakdown replaces PLAN §9's stage-based V01–V12
  table. Ticket numbering is re-assigned here; PLAN §9's dependency graph remains a valid
  coarse view.
- **Deferred decisions flagged for review** (each is called out in its ticket, not left to the
  implementer): `zip` in or out of std.collection.iter (V07), `Range` constructor mechanism
  (V06 — Ok/Err-style recognized names), the one-level field-assignment rule's parser-level
  enforcement (V02).
- **0.0.3 still does not implement**: closures, traits / generics / operator overloading /
  tuples (0.1.0), `break` / `continue` / user-defined iterables, method-call syntax, struct /
  array `==`, per-field `pub`, glob/selected imports, relative imports, separate compilation,
  a flint registry, file I/O, HashMap, `async` (0.0.4), `unsafe` / `ptr` / `addr` (0.0.5).
