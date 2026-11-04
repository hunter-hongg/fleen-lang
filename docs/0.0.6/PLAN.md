# 0.0.6 — `enum` + Array Library Completion

> **Codename**: Busy Beaver 🦫 (0.0.1 Swift Fox 🦊, 0.0.4 Patient Ants 🐜, 0.0.5 Careful Hedgehog 🦔).
>
> **Goal**: give Fleen user-defined **tagged unions** — `enum` with typed payloads,
> constructed and consumed through the existing `choose` pattern machinery that
> `Result` pioneered — and round out the array library with the functions the
> 0.0.3 std deliberately left out (`flatten` / `concat` / `foldr`), plus the
> `fold` → `foldl` rename that makes fold direction explicit.
>
> **Theme**: one shape of data, two fronts. `enum` is the *language* front: it
> generalizes the hand-written two-variant `Result` into an arbitrary
> user-defined sum type, reusing struct registration, `choose` patterns, and the
> Result lowering template end to end. The *library* front is pure std work on
> the existing schematic-builtin mechanism — no new syntax, no new opcodes.
>
> **Status**: planning. Decisions D1–D6 below are settled at planning level;
> each names the alternative it rejected and what the choice locks. They may be
> revised only by a new entry here, not silently in a ticket card.

---

## 0. Relationship to adjacent versions

| Version | Scope |
|---------|-------|
| 0.0.5 | complete |
| **0.0.6** | **`enum` (typed variants, non-recursive) + `flatten` / `concat` / `foldr` + `fold` → `foldl` rename** |
| 0.1.0 | generics / traits / operator overloading / closures; recursive types revisit |

The enum layer is self-contained: no generics, no traits, no closures. The
array functions need nothing beyond the 0.0.3 schematic-builtin mechanism.
Recursive enums (self-referential variants, directly or via `box` / `[T]`) are
**excluded** by D2 and stay out until a later version settles the
representation question D2 records.

---

## 1. Settled decisions (D1–D6)

### D1 — `enum` shape: a sum of typed variants; `is_copy` mirrors struct

```fleen
enum Shape {
    Point,
    Circle(float),
    Rect(float, float, float, float),
}
```

- An `enum` declares an **ordered set of variants**; each variant has a name
  and 0+ payload types. The enum name is a type, resolved through the same
  `pub` / `import` visibility machinery as struct names (0.0.3 V05).
- **`is_copy(E)` = every payload type of every variant is Copy** — the exact
  mirror of the struct rule (DESIGN §9.4). An enum with any owned payload
  (`string`, `box`, `[T]`, owned struct, owned enum) is an owned type:
  consuming positions need `move` / `clone`; `clone` deep-copies.
- Payloads obey the **same storage rules as struct fields** of the same type:
  a `ptr<T>` payload is rejected (`PtrNotStorable` family), `Future` /
  `channel` payloads inherit their boundary rules naturally, arrays and
  nested enums are legal.
- *Rejected*: struct-with-tag encoding ("just write `struct { tag: int, … }`")
  — it re-opens every misuse `choose` + exhaustiveness already closes, and it
  is exactly the shape `Result` proved worth first-class support.
- *Locks*: the runtime value representation (D4) and the verify table (D4)
  permanently carry variant/payload shape; a later change to payload layout is
  a bytecode-version bump, not a local edit.

### D2 — No recursive enums in 0.0.6 (`RecursiveEnum`)

- A variant payload may not (transitively) contain the enum being declared —
  directly, via `box`, or via `[T]` — mirroring the struct `RecursiveStruct`
  rule (DESIGN §9.4). New typeck error: `RecursiveEnum` with the full path.
- *Rejected*: allowing `box`-mediated recursion (needed for `List = Nil |
  Cons(int, List)`). It forces a decision on heap-cycle representation and
  drop order that 0.0.6 should not own; the struct side never needed it
  because `RecursiveStruct` has always been total. Revisit alongside 0.1.0
  generics, where the same machinery would serve both aggregates.
- *Locks*: nothing structural — lifting D2 later is additive (a new error
  case stops firing, the representation gains a heap form). This decision is
  cheap to reverse, which is why taking it now is safe.

### D3 — Construction and patterns: variant names are constructors; exhaustiveness generalizes `Result`

- **Construction**: a variant name in expression position is a constructor —
  `Point` (zero payload), `Circle(1.5)` (payloads). This is exactly how
  `Ok` / `Err` work today (parser emits a call-shaped expression; typeck
  resolves the identifier against the registered variant table and otherwise
  falls through to ordinary function lookup). Dotted `Color.Red` spellings
  resolve through the same module-path rules as any imported item.
- **Patterns**: `when Circle(r) { … }` joins the `choose` pattern grammar as
  `enum_pattern`, the generalization of `result_pattern` (`Ok(x)` /
  `Err(e)`); payload bindings follow the existing move semantics.
- **Exhaustiveness**: guard-free arms covering **all variants** of the
  scrutinee enum are exhaustive, exactly like `Ok` + `Err` (DESIGN §10.5);
  guarded arms never count; otherwise `otherwise` is required
  (`ChooseNotExhaustive` names the missing variants).
- **No `==` on enums** — same as structs and arrays; matching via `choose`
  is the comparison story. Equality waits for 0.1.0 traits.
- *Rejected*: requiring a qualified `Shape.Circle(...)` for every use —
  it penalizes the single-enum case for a collision the variant
  namespace (per-enum, checked at registration) already prevents.
- *Locks*: variant names enter the same flat identifier namespace as
  functions and structs within their visibility scope; a later rename of the
  resolution strategy is a breaking change, accepted here as 0.0.x.

### D4 — Bytecode: version 6, three opcodes in the reserved band, one appended table

Numbers below are checked against `BYTECODE.md` §4.2 / §2 / §9 (0xCB–0xDF is
the first free band; `FieldTypeTag` currently ends at 11):

- Opcodes **0xCB–0xCD** (new §5.17, sibling of 0xC8–0xCA):
  - `EnumNew` — operands `u16` enum_id, `u16` variant_id;
    `p1 … pn → e` (n = payload count of the variant, from `enum_defs`)
  - `EnumIs` — operand `u16` variant_ref; `e → bool` (the `IsErr` generalization)
  - `EnumPayload` — operands `u16` variant_ref, `u16` index;
    `e → p` (move out one payload; the `UnwrapOk` generalization)
- Runtime value `Value::Enum { variant: u16, payload: Box<[Value]> }` —
  **not serialized** (constructed at runtime, like `Struct`; no const-pool tag).
  The enum_id is a compile-time fact; the variant id rides the value so
  `EnumPayload` can raise the defensive `EnumPayloadMismatch` (the
  `ResultMismatch` generalization) on forged bytecode.
- One appended table `enum_defs` (read when `version >= 6`, mirror of
  `struct_defs` §9): `enum_defs_len: u32`, then per enum:
  `variant_count: u32`, then per variant: `payload_count: u32` +
  `payload_count × u8` `FieldTypeTag`. `FieldTypeTag` gains **`Enum = 12`**
  (append-only; tags exist for verify shape checks only).
- verify cross-checks `EnumNew(id)` against `enum_defs[id]` (forged id →
  `EnumIndexOutOfRange`; payload-count mismatch → format error) and the Δdepth
  of the three opcodes.
- *Rejected*: pushing discriminants as ints and comparing (`GetField`-style)
  — it would make every forged-bytecode payload extraction silent UB instead
  of a defensive error, and it abandons the Result precedent for no gain.
- *Locks*: 0xCB–0xCD encodings, the `enum_defs` table position (appended at
  the end, §9 append-only rule), and `Enum = 12` — all frozen per BYTECODE §10.

### D5 — Array library: `flatten` / `concat` in `std.arr`; `foldl` / `foldr` in `std.collection.iter`

| Function | Module | Signature (schematic) | Mechanism |
|----------|--------|----------------------|-----------|
| `concat` | `std.arr` | `concat([T], [T]) -> [T]` | VM intrinsic (schematic, like `std.arr.push`) |
| `flatten` | `std.arr` | `flatten([[T]]) -> [T]` | VM intrinsic (schematic; nested `Array(Var(n))` unification is already supported) |
| `foldl` | `std.collection.iter` | `foldl([T], A, (A, T) -> A) -> A` | `InlineIter` (the existing `fold` lowering, renamed) |
| `foldr` | `std.collection.iter` | `foldr([T], A, (T, A) -> A) -> A` | `InlineIter` (backward index loop; no reverse-copy) |

- **`fold` → `foldl` rename, no alias** (breaking, accepted in 0.0.x): the
  current `fold` *is* a left fold; keeping a direction-ambiguous name while
  adding `foldr` is how the ambiguity becomes permanent. All call sites
  (`std/collection/iter.fln` header, `tests/std/iter.fln`,
  `tests/e2e/valid/iter_helpers.fln`, `examples/0.0.3/tour`) move in T07.
- `flatten` is **one level only**: `flatten(xs)` on `[[[T]]]` is a type error
  (schematic unification binds `T = [U]` and the intrinsic consumes one
  layer); chaining `flatten` reaches deeper layers explicitly.
- Ownership follows the 0.0.3 array semantics: arguments are read as copies,
  results are fresh owned arrays; nothing mutates in place.
- *Rejected*: a `std.list` module distinct from `std.arr` — there is one
  array type (`[T]`, DESIGN §9.5); a second home for the same operations is
  API drift, not organization.
- *Locks*: the four names and their argument orders are public API from the
  moment 0.0.6 ships.

### D6 — Breaking-change posture: one reserved word, one rename, no shims

- `enum` becomes a **reserved keyword** (source-breaking: programs using
  `enum` as an identifier fail to compile) — the same posture as `ptr` /
  `addr` / `unsafe` / `trusted` in 0.0.5, recorded here so the breaking set
  is explicit: {`enum` reserved, `fold` renamed}.
- No deprecation aliases (`fold` kept as an alias for `foldl`), no
  opt-out flag. 0.0.x is the unstable-API window (SPEC §13); shims would
  fossilize the wrong name into the first stable release.
- *Locks*: every 0.0.5-and-earlier program that binds `enum` or calls
  `std.collection.iter.fold` breaks on upgrade; the CHANGELOG entry (T08)
  must carry both as explicit breaking changes.

---

## 2. Feature list

| ID | Feature | Layer | Depends on |
|----|---------|-------|------------|
| F1 | `enum` declarations (typed variants, registration, visibility) | Language (lex → typeck) | — |
| F2 | Constructor expressions (`Variant(...)`, zero- and multi-payload) | Language (parse → typeck → lower) | F1 |
| F3 | Variant patterns in `choose` + enum exhaustiveness | Language (parse → typeck → lower) | F1 |
| F4 | Bytecode v6: `EnumNew` / `EnumIs` / `EnumPayload`, `enum_defs`, `Value::Enum`, verify | compiler + VM + verify | F1–F3 |
| F5 | std: `concat` / `flatten` (intrinsics), `foldl` rename, `foldr` | std + builtins + VM intrinsics | — |
| F6 | Docs sync (DESIGN / SPEC / BYTECODE / SYNTAX.ebnf / CHANGELOG) + version bump | docs | F1–F5 |

---

## 3. Language design

### 3.1 Keywords

| Word | Status |
|------|--------|
| `enum` | **new reserved keyword** (declaration introducer; D6). Joins the 0.0.5 reservation path: `enum` is statement-starting from T01, so misuse is caught by `reject_keyword_binding_name` (the 0.0.5 T02 pattern) instead of an ASI statement-separator error; no new `ReservedWord` kind. |

### 3.2 Syntax (EBNF additions, `SYNTAX.ebnf`)

```ebnf
enum_decl       = "enum" , ident , "{" , variant , { "," , variant } , [ "," ] , "}" ;
variant         = ident , [ "(" , type , { "," , type } , ")" ] ;

(* expression position: a variant name with payload args parses as a call;
   zero-payload as a bare identifier — resolution is typeck's (D3) *)
(* pattern position: *)
enum_pattern    = ident , [ "(" , ident , ")" ] ;   (* binds the single payload;
                                                       multi-payload variants destructure
                                                       via payload reads inside the arm *)
```

- Declarations are **top-level only** (mirror `struct`); no trailing semicolon;
  trailing comma allowed (mirror struct fields).
- Variant patterns bind **one** payload (`Circle(r)`); variants with 2+
  payloads match positionally with guard-free arms and destructure inside the
  arm via payload reads (`EnumPayload` with index) — the same one-binding
  restriction `Ok(x)` has today, kept so the pattern grammar and the Result
  precedent stay identical.
- `enum` declarations may not appear inside functions (mirror `struct`).

### 3.3 Type rules

- The enum name is a type (`Type::Enum(EnumId)` in typeck; `Named` stays
  reserved for structs — bare identifiers in type position resolve through
  the struct-style registration table, extended to enums).
- `is_copy(E)` = all payloads Copy (D1); `clone e` deep-copies payload arrays.
- Constructor typing: `Circle(e)` requires payload arity and types to match
  the declaration (`ArgTypeMismatch` / `ArityMismatch` reuse); the result
  type is the enum.
- Payload read typing: in a matched arm, `EnumPayload` yields the declared
  payload type by move (owned payloads move out of the consumed enum value —
  the `UnwrapOk` rule).
- No `==`, no `as string` on enums (`UnsupportedCast`); printing requires
  `choose` to project a string first.
- Payloads obey struct-field storage rules (D1): `ptr` payloads rejected,
  `Future` / `channel` payloads inherit boundary rules.

### 3.4 Pattern matching & exhaustiveness

- Scrutinee type `Type::Enum(id)`: enumerate the registered variants;
  guard-free arms must cover every variant (`ChooseNotExhaustive` names the
  missing ones); `otherwise` short-circuits as today.
- Lowering per arm: `DupDeep; EnumIs v; JumpIfTrue arm` chain, then
  `EnumPayload v i` + `StoreLocal` for the binding — the §6.8 Result template
  with the variant test generalized. Guard semantics unchanged (guards never
  count toward exhaustiveness; a guard needs a guard-free duplicate or
  `otherwise`).

---

## 4. std library changes (F5, D5)

- `std/arr.fln` header gains `concat` / `flatten` docs; both are schematic
  intrinsics registered in `builtins.rs` (`BuiltinLowering::Intrinsic`),
  implemented in the VM's builtin dispatcher — no new opcodes (the
  `std.str.concat` precedent).
- `std/collection/iter.fln`: `fold` entry renamed `foldl`; `foldr` added
  (`InlineIter`, backward index loop); header comment updated — the module
  header is the API contract.
- Builtins-table signatures (schematic `Var(n)`):
  `std.arr.concat: ([T], [T]) -> [T]`,
  `std.arr.flatten: ([[T]]) -> [T]`,
  `std.collection.iter.foldl: ([T], A, (A, T) -> A) -> A`,
  `std.collection.iter.foldr: ([T], A, (T, A) -> A) -> A`.

---

## 5. Compiler-stage changes

| Stage | Change |
|-------|--------|
| Lex | `enum` → `TokenKind::Enum` (keyword table, `is_keyword` / `keyword_str` / `Display`) |
| Parse | `enum_decl` top-level item; `enum_pattern` in `Pattern`; constructor call-shape expressions pass through unchanged (typeck resolves) |
| Resolve | visibility/import wiring for enum names and variants (module table already carries item kinds) |
| Typeck | enum registration table; constructor typing; payload storage checks (`PtrNotStorable` family, `RecursiveEnum`); scrutinee exhaustiveness for `Type::Enum` |
| Lower | `EnumNew` for constructors; the §6.8-style arm chain for enum `choose` |
| Codegen | emit `enum_defs` (v6 appended table); `MODULE_VERSION = 6`; `READ_VERSIONS = {1..6}` |
| verify | `EnumNew` ↔ `enum_defs` cross-check; Δdepth for the three opcodes |

---

## 6. VM impact (F4)

- `Value::Enum { variant: u16, payload: Box<[Value]> }` (not serialized);
  `clone` deep-copies; `Eq` on an enum is a typeck-level error (defensive VM
  error, same posture as struct/array).
- New opcodes implemented in dispatch; defensive errors:
  `EnumIndexOutOfRange` (forged enum_id), `EnumPayloadMismatch` (variant
  ≠ operand; payload index out of range joins the family).
- `fmt.rs`: `Value::Enum` debug rendering for runtime-error messages.

---

## 7. Bytecode impact

- Version 5 → **6**; readers accept `{1..6}`; v1–v5 fixtures unchanged
  (append-only rules respected: new opcodes in the reserved band, one new
  table appended at the end, `FieldTypeTag` extended by one value).
- New section `§5.17 enum (v0.0.6)`; `§9` gains the `enum_defs` layout;
  `§10` versioning table gains the v6 row.

---

## 8. Explicitly out of scope (0.0.6)

| Item | Reason |
|------|--------|
| Recursive enums (even via `box` / `[T]`) | D2 — representation/drop-order question deliberately not owned here |
| `==` on enums (or structs/arrays) | needs 0.1.0 traits (0.0.3 exclusion unchanged) |
| Generic enums (`enum Option<T>`) | user-facing generics are 0.1.0; schematic type variables stay builtin-table-only |
| Multi-payload patterns binding 2+ names in one arm | kept at the `Ok(x)` one-binding level; positional payload reads cover the rest |
| Enum in constant pool / serialization | enums are runtime-constructed; no const-pool tag (D4) |
| `map` / `filter` over `Result` / enums (monadic combinators) | needs closures or generics — 0.1.0 |
| Guard inside the pattern (`when Circle(r if r > 1)`) | guards stay clause-level (`when Circle(r) if r > 1`), as today |

---

## 9. Test matrix (planned)

| Area | Coverage |
|------|----------|
| Lex | `enum` keyword token; prefix-safety (`enums`, `EnumX` stay identifiers); valid-only lexer fixture |
| Parser | zero/multi-payload decls; trailing comma; nested-brace rejection; `when Variant(x)` pattern |
| Resolver | import/pub for enums; shadowing rules; dotted constructor paths |
| Typeck | constructor arity/type mismatch; `RecursiveEnum` (direct, via box, via array); `ptr` payload rejected; Copy/owned classification; exhaustive / missing-variant / guarded-arm cases |
| Lower | constructor + arm-chain bytecode shape on hand-checked fixtures |
| E2E valid | `enum_demo.fln` (construct + choose + payload use); std tests for concat/flatten/foldl/foldr (incl. `foldr` associativity differential vs `foldl`) |
| E2E invalid | non-exhaustive choose; recursive enum; `as string` on enum; `==` on enum |
| Bytecode | v6 write/read round-trip; v1–v5 fixture decode; forged `EnumNew` / `EnumPayload` rejected (verify + VM) |

---

## 10. Versioning

- Workspace `Cargo.toml` → `0.0.6`; `std/VERSION` → `0.0.6` (strict pinning
  keeps matching).
- Bytecode `MODULE_VERSION = 6`; `READ_VERSIONS` gains 6.
- CHANGELOG gains the `0.0.6` entry with the explicit breaking set from D6.
- SPEC §13 gains the 0.0.6 goal paragraph; DESIGN §17 feature table gains the
  `enum` row; §18 roadmap marks 0.0.6 complete at T08.
