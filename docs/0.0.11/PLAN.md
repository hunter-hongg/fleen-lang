# 0.0.11 — The `map` Type + `std.map` Intrinsics

> **Codename**: Cartographer Ant 🗺️ (settled at the release gate; lineage
> 0.0.1 Swift Fox 🦊, 0.0.2 Hermit Crab 🐚, 0.0.3 Busy Beaver 🦫, 0.0.4
> Patient Ants 🐜, 0.0.5 Careful Hedgehog 🦔, 0.0.7 Mantis Shrimp 🦐, 0.0.8
> Cheeky Hamster 🐹, 0.0.9 Mending Weaverbird 🪡, 0.0.10 Loom Spider 🕸️).
>
> **Goal**: one language-level data structure — the key→value `map`. A new
> parameterized type `map[K, V]` with a reserved-word literal `map { k ->
> v, … }`, owned semantics (always owned, like `[T]` — D2), `m[k]` index
> access through the existing `index_suffix` (D4), and a `std.map` batch of
> eight VM intrinsics with **zero new opcodes** (D3, the 0.0.9 std.str
> precedent). One new opcode for construction (`NewMap`, 0xAF) + one
> `FieldTypeTag::Map = 13`; the version moves to **v9** via the
> content-driven rule (a module is v9 only when a map was actually encoded,
> D7) — every map-free module stays v8 and byte-identical to 0.0.10.
>
> **Theme**: fill the one structural gap left after 0.0.6's enums — the
> language has arrays (`[T]`, 0.0.3), structs (0.0.3), enums (0.0.6), and
> parameterized types (0.0.7), but no key→value structure. `map[K, V]`
> completes the family without borrowing the 0.1.0 traits machinery: its
> rules are cut from the same cloth as `[T]` (always-owned, `index_suffix`
> access, intrinsic-driven std helpers), so nothing here opens a front
> 0.1.0 has to finish.
>
> **Status**: planning. Decisions D1–D8 below are settled at planning
> level; each names the alternative it rejected and what the choice locks.
> They may be revised only by a new entry here, not silently in a ticket
> card.
>
> **What this is not**: no mutable map (D2 — `insert`/`remove` return a new
> map; in-place `m[k] = v` is 0.1.0); no traits / operator overloading /
> generics / closures (all stay 0.1.0); no `map == map` (deep equality
> needs the traits work — `map` follows the 0.0.6 enum posture, D5); no
> `float` / `Range` / `enum` / struct / `array` / `Result` keys (the key
> whitelist is the `==`-comparable + hash-stable set, D6); no `for` over a
> map (iterate `std.map.keys` / `std.map.values` instead, D3); no
> map-literal inside a global initializer beyond what the simple-expression
> rule already allows (a `map {…}` literal in a global falls into the
> `ComplexGlobalInit` catch-all, exactly as a `struct` literal does).

---

## 0. Relationship to adjacent versions

| Version | Scope |
|---------|-------|
| 0.0.10 | complete (release published — tag and archives cut) |
| **0.0.11** | **the `map[K, V]` type: literal, `index_suffix` access, `std.map` intrinsics (D1–D6) + bytecode v9 (content-driven, D7) + docs/examples/version sync** |
| 0.1.0 | function-level generics / traits / operator overloading / closures; recursive types revisit; extended string escapes; `substring`/`slice`/`chars`; channel `close`/`select`; file I/O; **mutable map (`m[k] = v` assignment, D2 carry); `map == map` deep equality (D5 carry); `for` over a map** |

No planning-time corrections this milestone: 0.0.10 closed clean, so
0.0.11 opens **features**.

**Breaking set**: `map` becomes a reserved word (the 0.0.5/0.0.9/0.0.10
precedent for reserved-word additions — programs that named a binding or
type `map` must rename). Everything else is additive: a `map`-free program
lexes to the identical token stream, and the `map { … }` literal is new
syntax that could not previously parse.

---

## 1. Settled decisions (D1–D8)

### D1 — `map` is a reserved word and the `map { … }` literal is the only map syntax

`map` becomes a lexer keyword (`TokenKind::Map`) and a **reserved
construction** (`is_keyword() == true`, `keyword_str() == Some("map")`).
The literal form is `map { k -> v, k2 -> v2 }`, the same brace-block
shape the language already uses for `struct { … }`, `enum { … }`, and
`if { … }` bodies — no new delimiter is introduced.

- **The reserved-word cost is the only source-visible break**: a program
  with a binding/type/module named `map` must rename it (the 0.0.5
  `ptr`/`break`/`continue` and 0.0.10 `Ok`/`Err` precedent). No other
  behavior changes.
- **Rejected (bare `{ k -> v }` in expression position)**: a bare `{` in
  primary position is already a **block expression** (the empty body / the
  `if`-less block form). A block whose first token is a key expression is
  syntactically indistinguishable from `map { … }` until the lexer sees
  `->`, which is a token-lookahead rule the parser cannot express without
  either banning empty/`k;` blocks or adding a one-token backtracking
  hook. `map {` is unambiguous at the first token and matches the
  language's existing brace-block idiom.
- **Rejected (`{ k: v }` colon form)**: same block collision, plus a
  second meaning for `:` (it is currently only a type annotation).
- **Rejected (`Map.new(...)` constructor form)**: no literal syntax — a
  map built from N pairs would need N `insert` calls; the language's
  arrays, structs, and enums all have literals, so a map should too.
- **What this locks**: `map` is a lexer keyword forever after; the `->`
  separator is fixed (D8's arrow choice is separate from the brace shape);
  a `map {` cannot begin a block expression.

### D2 — `map[K, V]` is **always owned** (the `[T]` posture); `insert` / `remove` return a new map

`map[K, V]` follows the 0.0.3 V03 D3 rule for arrays: **the type is always
owned**, regardless of `K` / `V`. `is_copy(map[K, V]) == false` for every
`K` / `V` — the type never appears on the Copy list, never on the
argument-dependent list, never carries an `info.copy` bit. Reading a map
(`m[k]`, a field read, a `for`-over-`keys` binding) copies the *value*
out; passing a map to a function moves it and the source is unusable
afterwards (the ordinary 0.0.2 ownership rules).

Consequently the mutators are **functional**, not in-place:

```fleen
import std.map;

m = map { "a" -> 1, "b" -> 2 };
m2 = std.map.insert(m, "c", 3);   // a NEW map; `m` is still `{"a"->1,"b"->2}` …
                                   // … except the insert CONSUMED `m` (moved into the call).
                                   // Reuse requires `std.map.insert(clone m, …)`.
```

- **Rejected (mutable `m[k] = v` in-place assignment)**: it needs a new
  LHS form for `index_suffix` (today's assignment is name-only), an
  ownership/borrow story for nested `m.a.b[k] = v` (which the struct
  write-path rules do not yet cover), and a `MapIndexAssign` opcode. All
  three are real work that 0.1.0's "revisit after generics" line already
  owns (the same line that revisits `as string` and adds the recursive
  `==`). 0.0.11 ships the immutable core; `std.arr.set`'s in-place
  behavior on an *owned* array stays the only in-place data mutator.
- **Rejected (`is_copy` propagates through `map`, like `Result`)**: it
  would make `map[int, int]` Copy, which means every read hands out a
  shared handle and every `m2 = m` aliases — the opposite of the array
  contract a map's users will reach for. Arrays chose always-owned for
  exactly this reason (V03 D3); `map` follows.
- **What this locks**: `is_copy(map[K, V]) == false` for every `K` / `V`;
  `contains_future` / `contains_channel` / `contains_ptr` walk into a map's
  `V` (a `map[int, channel<int>]` is rejected at the literal — D5/D6's
  storable discipline); the `clone` / `move` rules are byte-for-byte the
  array rules.

### D3 — `std.map` is a batch of VM intrinsics with **zero new opcodes** for access and the eight helpers

The 0.0.9 std.str batch (D3) is the template: each `std.map.*` helper is a
`BuiltinSig` with `BuiltinLowering::Intrinsic` and a VM arm — no opcode is
added for any of them. The eight helpers:

| Intrinsic | Signature (planning spelling) | Notes |
|-----------|-------------------------------|-------|
| `std.map.get` | `(map[K, V], K) -> Result[V, string]` | missing key → `Err("key not found")` (the `std.str.index_of` precedent) |
| `std.map.insert` | `(map[K, V], K, V) -> map[K, V]` | functional: consumes the input, returns a new map (D2) |
| `std.map.remove` | `(map[K, V], K) -> map[K, V]` | functional; removing an absent key returns an equal map |
| `std.map.contains` | `(map[K, V], K) -> bool` | |
| `std.map.keys` | `(map[K, V]) -> [K]` | declaration order (insertion order, see D8) |
| `std.map.values` | `(map[K, V]) -> [V]` | declaration order; each value deep-copied |
| `std.map.len` | `(map[K, V]) -> int` | |
| `std.map.clone` | `(map[K, V]) -> map[K, V]` | explicit deep copy (the `clone` keyword also works, D2) |

- **`std.map.get` returns `Result[V, string]`** — the missing-key question
  the user explicitly deferred: `Err("key not found")` follows the
  `std.str.index_of` "not-found is Err" precedent (0.0.9 D3) and is
  composable with `?`. `m[k]` (`index_suffix`, D4) keeps the same
  `Result[V, string]` return type for consistency.
- **Rejected (panic on missing key)**: a `MapKeyNotFound` runtime error
  would make every lookup a potential crash. The language's post-0.0.9
  posture is that *recoverable* failures are `Result` values.
- **Rejected (return a default `V`)**: requires a default value the type
  system cannot synthesize for every `V` without traits (0.1.0).
- **Rejected (`for (k, v) in m` iteration)**: `for` iterates `[T]` or
  `Range` (0.0.3 V04). Adding tuple destructuring or a second iteration
  protocol is 0.1.0 work; `std.map.keys` / `std.map.values` cover the
  common loops today.
- **What this locks**: the eight signatures are the std contract; a new
  intrinsic needs a new PLAN decision (and a `std/map.fln` doc-comment
  update).

### D4 — `m[k]` access rides the existing `index_suffix`; **no parser change**

`m[k]` parses as an ordinary `index_suffix` (`docs/SYNTAX.ebnf:429`) — the
same production as `a[i]` on an array. The only change is in **typeck's
`typeck_index`** (the single `ExprHir::Index` evaluation branch,
`infer.rs:2410` / `:4584`): if the base is `map[K, V]` and the index is
`K`, the result is `Result[V, string]` (D3).

- **Zero parser change**: `index_suffix` is already `expr [ expr ]`; a map
  base is just a new base type.
- **Zero new opcode for access**: the VM's index dispatch already exists
  (`ArrayIndex`); a map-index arm is added to the same opcode (the base
  `Value` discriminates array vs map), or `std.map.get` is called and the
  parser desugars `m[k]` to that intrinsic (planning-level: the dispatch
  arm, since it is one opcode arm and mirrors the array path — T03/T05
  decide the exact wiring, recorded there).
- **Rejected (a `.get(k)` method sugar)**: methods are 0.1.0 (traits);
  `m[k]` and `std.map.get(m, k)` are the same operation today, and the
  array precedent is `[T]` + `std.arr.get(a, i)`.
- **What this locks**: `m[k]` is a postfix expression, composable with `?`
  (`v = m[k]?;`), chainable (`m[k][k2]` on a `map[K, map[K2, V2]]`), and
  typed `Result[V, string]`.

### D5 — The key whitelist: `K ∈ {int, bool, string}` (the `==`-comparable **and** hash-stable set)

A `map[K, V]` type use is rejected at typeck unless `K` is one of:

```
int, bool, string
```

The rule is stated as **the intersection of `==`-comparable and
hash-stable**. `==`-comparable is the narrower of the two today — the
`typeck_binary_op` `==` gate (`infer.rs:3231`) rejects `enum` / `struct` /
`array` / `channel` / `future` and accepts `int` / `float` / `bool` /
`unit` / `string` / `Range` — but the whitelist also excludes:

- **`float`** — `NaN != NaN`, `0.0 == -0.0`, and the hash of an equal
  float must equal the hash of its equal — all three are classic
  map-key hazards. `float` is the one `==`-comparable type the whitelist
  drops.
- **`unit`** — a single-value type makes a single-key map, which
  `map[int, V]` with one entry already expresses.
- **`Range`** — legal in principle, but no use case today and it keeps the
  whitelist to three names.

A `map[struct{…}, V]` / `map[SomeEnum, V]` / `map[[int], V]` / `map[float,
V]` is a `MapKeyNotComparable` typeck error with a hint naming the
whitelist.

- **The user's rule honored**: "键的类型必须有 `==`" (the key type must
  have `==`) — the whitelist is exactly the `==`-comparable set minus
  `float` / `unit` / `Range`, and the error names the rule.
- **Rejected (allow `enum` / `struct` keys, gated on deep `==`)**: deep
  `==` on enums and structs is the 0.1.0 traits work (the same line that
  unifies `Result` with `Value::Enum`). Allowing them here would mean
  either a hand-rolled structural-equality walk (a second implementation
  0.1.0 replaces) or silently rejecting them again at the hash step —
  both worse than a clean typeck rejection today.
- **Rejected (allow `float` with a NaN caveat)**: `NaN != NaN` is the
  default, so `map[float, V]` would silently store a `NaN` key that no
  lookup can ever find. Dropping `float` from the whitelist is the honest
  line.
- **What this locks**: the whitelist is closed; widening it (to `enum`
  keys once deep `==` exists, to `float` once NaN is settled) needs a new
  PLAN decision. The `MapKeyNotComparable` error names the whitelist in
  its hint.

### D6 — Value discipline: `V` cannot hold a `Future` / `channel` / `ptr`; the map is not storable in those slots either

A map's `V` is a full value position: `contains_future(V)` /
`contains_channel(V)` / `contains_ptr(V)` are rejected at the literal and
at every `std.map.insert` / `std.map.values` call, exactly as they are
rejected in an array element or a struct field (the 0.0.4 T07d / 0.0.5 T02
storable discipline). Symmetrically, a `map[K, V]` is rejected as a struct
field, array element, box cell, and global **only if `V` carries a
`Future` / `channel` / `ptr`** — a `map[int, int]` field is a normal
owned value.

- **Why**: `Value::Map` holds its values by ownership; a value-level copy
  of a `Future` would fork a suspended coroutine, of a `channel` would
  alias the shared buffer without `std.concurrent.split`, of a `ptr` would
  hand out aliasing access in safe code (the `contains_*` walks exist for
  exactly this).
- **Rejected (allow `channel` values with a runtime backstop)**: the
  compiler-level rejection is the 0.0.4/0.0.5 posture — the VM backstop is
  the last line, not the first.
- **What this locks**: the `contains_in_value_position` walk gains a
  `Type::Map` arm that recurses into `V` (and into `K`, for the same
  reason — a `map[channel<int>, int]` key is rejected by D5 anyway, but
  the walk keeps the invariant total); `m[k]` reading a `V` deep-copies
  (the `DupDeep` path, D2's read-copies rule).

### D7 — Bytecode: one new opcode (`NewMap` 0xAF), `FieldTypeTag::Map = 13`, version **v9** by the content-driven rule

The only bytecode change is the **construction** opcode — access and the
eight `std.map.*` helpers ride existing machinery (the array-index arm for
`m[k]`, the intrinsic-call path for the helpers, D3/D4):

- **`NewMap` (0xAF)** — pops `2 * n` values (`k1 v1 k2 v2 …` in source
  order) and pushes one `Value::Map`. The `n` count is a `u16` immediate.
  Region: the 0xB0 array block's last unused slot (0xB0–0xB3 struct,
  0xB4–0xB8 array, 0xB9–0xBB range, 0xBC–0xBE reserved, **0xAF is the
  first free slot below the block** — T05 confirms the exact byte and
  renumbers if 0xAF is taken).
- **`FieldTypeTag::Map = 13`** — a map stored in a struct field or an
  array element carries the `Map` tag (the array/struct shape checks read
  the tag; a `map` value reaches a struct field the same way an `Array`
  value does).
- **Version v9, content-driven** (the 0.0.8 D13/D14 rule, applied to the
  new opcode): a module is **v9 only if a `NewMap` was actually encoded**.
  A map-free module (including every `std` module — none of them use
  `map`) stays **v8** and byte-identical to 0.0.10 output. Readers accept
  `{1..=9}`; a v9 module decodes on the 0.0.11 reader; a v8 module that
  somehow contains `NewMap` is a `BadOpcode` (the same gate 0.0.4 added
  for 0xC0–0xC7).
- **`BYTECODE.md` gains one opcode row + one tag row** (the first format
  change since 0.0.8's `PartialApply`).
- **Rejected (skip the opcode, lower map literals to `std.map.insert`
  chains)**: an N-pair literal becomes N clone-N-deep-copies of the
  intermediate map — O(N²) work and O(N²) allocation for a literal. One
  opcode is the array-literal precedent (`BuildArray`, 0xB4).
- **Rejected (new opcode for map access / the eight helpers)**: zero user
  value — the array-index arm and the intrinsic-call path already cover
  them, and every extra opcode is a permanent reader obligation (D1 of
  0.0.10).
- **What this locks**: the version moves to v9 only when a map exists;
  every map-free module's bytecode is byte-identical to 0.0.10 (the
  content-driven discipline); `BYTECODE.md` carries the two new rows
  recorded by T07.

### D8 — Runtime representation: insertion-ordered, `HashMap`-backed, with a `KeyHash` wrapper

`Value::Map` wraps a Rust `HashMap<KeyHash, Value>` plus a
`Vec<KeyHash>` insertion-order index (the order `std.map.keys` /
`std.map.values` return). `KeyHash` is the implementation-layer wrapper
that hashes the D5 key subset — `Int(i64)` / `Bool(bool)` /
`Str(Box<str>)` — with a derived `Hash` + `Eq`. The insertion-order index
is what makes `std.map.keys(m)` deterministic (declaration order), which
is the only ordering a test may rely on (the 0.0.4 executor's "deterministic
by construction" posture).

- **Why `HashMap` + index and not `BTreeMap`**: `HashMap` gives O(1)
  average lookup; the insertion-order `Vec` is O(1) amortized to append
  and O(N) to build — the same trade `std.arr` makes. A `BTreeMap` would
  give sorted keys for free but requires `Ord` on `KeyHash`, which drags
  in `float`-style NaN questions for no current use case.
- **Why a `KeyHash` wrapper and not `impl Hash for Value`**: `Value` has
  15 variants, most of which are either unhashable (`Arr` / `Struct` /
  `Map` — the same types D5's whitelist excludes) or fork-prone (`Future`
  / `channel` / `ptr` / `Ref`). Deriving `Hash` on the full `Value` would
  either panic on the unhashable variants or need a hand-rolled `Hash`
  that silently mis-hashes them; the wrapper names the hashable subset
  and is total by construction.
- **Rejected (`BTreeMap`)**: see above — no current use case for sorted
  keys, and it forces the `Ord` question.
- **Rejected (FNV / SipHash by hand)**: the std `Hash` derive on a
  three-variant enum is the same mechanism `Value` itself uses; a
  hand-rolled hash is a second implementation to keep correct.
- **What this locks**: `std.map.keys` / `std.map.values` return
  declaration order (the only observable ordering); `Value::Map` is
  non-Copy and deep-copied by `clone` / assignment; the runtime cost of a
  lookup is the `HashMap` cost.

---

## 2. Feature list

| ID | Feature | Tickets |
|----|---------|---------|
| F1 | The `map` keyword, `map[K, V]` type, and the `map { k -> v }` literal | T01, T02, T03 |
| F2 | `m[k]` access through `index_suffix` + the `std.map` eight-intrinsic batch | T04, T05 |
| F3 | Bytecode v9 (`NewMap` + `FieldTypeTag::Map`), content-driven | T06 |
| F4 | Docs sync, examples, version bump, release gate | T07, T08 |

---

## 3. Language design

### 3.1 The literal and the type (F1)

```fleen
import std.map;
import std.io;

// a map literal: `map { key -> value, … }` (D1); keys are int/bool/string (D5)
ages = map { "alice" -> 30, "bob" -> 25 };
counts = map { 1 -> "one", 2 -> "two" };
flags = map { true -> 1, false -> 0 };

// empty map: needs a type annotation (the array-empty-literal rule)
empty: map[string, int] = map {};

// the type is always owned (D2); `m2 = m` moves, `m` is unusable afterwards
m = map { "a" -> 1 };
m2 = m;                          // `m` moved
// std.io.println(ages as string);   // `as string` on a map is rejected (D5)
```

What does **not** change: every existing type, every existing literal,
every existing `for` (no map iteration — D3), every existing `==` gate
(a `map` operand to `==` is `InvalidOperand`, D5's enum/array posture).

### 3.2 Access and the std helpers (F2)

```fleen
import std.map;
import std.io;

m = map { "a" -> 1, "b" -> 2 };

// index access rides `index_suffix` (D4); result is `Result[V, string]` (D3)
r = m["a"];                       // r: Result[int, string] = Ok(1)
miss = m["z"];                    // miss: Result[int, string] = Err("key not found")

// `?` composes (the 0.0.2 propagation rules)
func lookup(m: map[string, int], k: string): Result<int, string> {
    v = m[k]?;
    Ok(v)
}

// the eight helpers (D3); `insert` / `remove` are functional (D2)
m2 = std.map.insert(m, "c", 3);   // NEW map {"a"->1,"b"->2,"c"->3}
m3 = std.map.remove(m2, "b");     // NEW map {"a"->1,"c"->3}
has_a = std.map.contains(m3, "a");  // true
n = std.map.len(m3);              // 2
ks = std.map.keys(m3);            // ["a", "c"] (insertion order, D8)
vs = std.map.values(m3);          // [1, 3]

// iterate via the value arrays (D3: no `for` over a map)
for k in std.map.keys(m3) {
    std.io.println(k);
};
```

- `m[k]` and `std.map.get(m, k)` are the same operation; `m[k]` is the
  postfix spelling, `std.map.get` the explicit-call spelling.
- `insert` / `remove` consume their input map (D2's ownership rules); a
  reuse-after-insert is the ordinary `UseAfterMove`.

---

## 4. std library changes

- New module `std/map.fln` (the 0.0.9 std.str batch shape): doc comments
  for the eight intrinsics — their signatures, the D3 missing-key rule,
  the D5 key whitelist, the D2 ownership contract (`insert` / `remove`
  are functional; the input is consumed), and the D8 declaration-order
  guarantee. No `pub` re-exports (0.0.3 V06 has no `pub use`).
- No other std module changes: `std.arr` / `std.collection` / `std.io` /
  `std.os` / `std.num` / `std.time` / `std.str` / `std.async` /
  `std.concurrent` / `std.result` untouched. (Planning-time inspection:
  no std `.fln` uses `map` today, so std itself does not exercise the
  feature and **stays v8** — D7's content-driven rule.)

---

## 5. Compiler-stage changes

| Stage | Change |
|-------|--------|
| Lexer | `map` keyword (`TokenKind::Map`, `is_keyword`, `keyword_str`) |
| Parser | `map { k -> v, … }` literal (`parse_map_literal`, gated on `TokenKind::Map`); `map[K, V]` type use in type position |
| Resolver | `std.map.*` registration (the eight `BuiltinSig`s are typeck-level, but the resolver seeds `std.map` as a known module path, mirroring `std.str`); `map[K, V]` type resolution to `Type::Map` |
| Typeck | `Type::Map(K, V)` variant; `is_copy(map[K, V]) == false`; the D5 key whitelist (`MapKeyNotComparable`); the D6 `contains_*` map arm; the D4 `typeck_index` map branch (`Result[V, string]`); `==` rejection on a map operand (`InvalidOperand`) |
| Lower | the map-literal node → `NewMap`; the eight intrinsics ride the intrinsic-call path; `m[k]` rides the array-index arm (or desugars to `std.map.get` — T05 records the wiring) |
| Codegen | `NewMap` (0xAF) encoder + `FieldTypeTag::Map = 13`; version v9 content-driven (a `NewMap` in the module bumps the writer to v9) |
| Verify | the `NewMap` depth table entry (`-2n + 1`); the v9 opcode gate (`NewMap` in a v1–v8 module → `BadOpcode`); the `Map` tag recognized in struct-field / array-element shape checks |
| VM | `Value::Map(Box<MapCell>)` variant (the `KeyHash` wrapper, D8); the `NewMap` dispatch; the map arm of the array-index opcode (or the `std.map.get` intrinsic); the eight `std.map.*` intrinsic arms; the deep-copy path in `clone` / assignment / `DupDeep` |

---

## 6. VM impact

`Value::Map(Box<MapCell>)` is the one new runtime variant (the 0.0.6
`Value::Enum` precedent for a mechanical addition):

- **`MapCell`**: `HashMap<KeyHash, Value>` + `Vec<KeyHash>` (insertion
  order, D8). Owned, `Clone` derives a deep copy.
- **`NewMap`**: pops `2n` values, builds a `MapCell`, pushes `Value::Map`.
- **Index dispatch**: the array-index opcode gains a map arm (base is
  `Value::Map`) — returns `Ok(value.clone())` on a hit, `Err("key not
  found")` on a miss (D3).
- **Intrinsic arms**: the eight `std.map.*` arms in `vm.rs`, mirroring the
  0.0.9 std.str batch.
- **Deep copy**: the `clone` path, the assignment path, and the `DupDeep`
  path all deep-copy a `Value::Map` (the 0.0.3 `Value::Arr` deep-copy
  precedent).
- **`Value::PartialEq`**: the derived `PartialEq` compares maps
  structurally (for tests only, the same posture as `Value::Arr`); the
  `Eq` instruction rejects a map operand (D5 — typeck-level error, the VM
  backstop is `InvalidOperand`).

---

## 7. Bytecode impact

One new opcode (`NewMap` 0xAF, D7), one new tag (`FieldTypeTag::Map = 13`,
D7), version **v9** by the content-driven rule (a module is v9 only when a
`NewMap` was encoded). Readers accept `{1..=9}`; every map-free module —
including every `std` module — stays **v8** and byte-identical to 0.0.10
output. `BYTECODE.md` gains the `NewMap` opcode row (§5.x), the
`FieldTypeTag::Map` tag row (§4), and the v9 reader note; T07 records the
sync. No table schema change (`enum_defs` / `struct_defs` untouched).

---

## 8. Explicitly out of scope (0.0.11)

| Item | Reason |
|------|--------|
| Mutable map (`m[k] = v` in-place assignment) | new LHS form + ownership story + `MapIndexAssign` opcode; 0.1.0's "revisit after generics" line (D2) |
| `map == map` deep equality | needs the 0.1.0 traits work (the enum/array posture, D5) |
| `enum` / `struct` / `array` / `Result` keys | need deep `==` (0.1.0); the D5 whitelist is the `==`-comparable set |
| `float` keys | `NaN != NaN` / `0.0 == -0.0` hazards; dropped from the D5 whitelist |
| `for` over a map | `for` iterates `[T]` or `Range`; tuple destructuring is 0.1.0 (D3) |
| Map-literal in a global initializer | `ComplexGlobalInit` catch-all, the same posture as a `struct` literal |
| `map` in a constant pool | values are not serialized (the `Value::Arr` posture) |
| `std.map.map` / `std.map.filter` / `std.map.fold` | higher-order helpers need closures / function-value arguments over a map; 0.1.0 (the `std.collection.iter` family is array-only today) |
| Sorted-keys map | `BTreeMap` has no current use case (D8) |
| Generics / traits / operator overloading / closures | 0.1.0 (unchanged) |
| Extended escapes, `substring`/`slice`/`chars`, channel `close`/`select`, file I/O, `for` over `string` | 0.1.0 carries (0.0.9 §8 / 0.0.10 §8) |

---

## 9. Test matrix (planned)

**F1 (the type + literal)**:

- Lexer valid: `map` as a keyword (`is_keyword` / `keyword_str`); the
  `map { … }` token stream.
- Lexer invalid: `map` used as an identifier (`expected expression, found
  keyword \`map\``, the reserved-word error text).
- Parser valid: empty literal with annotation; N-pair literal (1, 2, 3
  pairs); trailing comma; nested literal (`map { "k" -> map { … } }`);
  `map[K, V]` in return/param/annotation/struct-field positions.
- Parser invalid: `map` without `{`; `{ … }` without `map` in expression
  position (still a block); `map { k }` without `->`; `map { k -> }`
  without value; a duplicate key at parse time is *not* caught (it is a
  runtime overwrite, D8 — recorded, not an error).
- Typeck valid: every D5-whitelisted key type (`int` / `bool` / `string`)
  × every value type (`int` / `float` / `bool` / `string` / `unit` /
  `Range` / `map[K, V]` / struct / enum / `Result`).
- Typeck invalid: `map[float, V]` / `map[SomeStruct, V]` /
  `map[SomeEnum, V]` / `map[[int], V]` / `map[Result[int, string], V]` /
  `map[unit, V]` / `map[Range, V]` (each `MapKeyNotComparable` + hint);
  `map[string, Future<int>]` / `map[string, channel<int>]` /
  `map[string, ptr<int>]` (each the `contains_*` rejection, D6); `map ==
  map` (`InvalidOperand`); `as string` on a map; `is_copy(map[int, int])`
  is false (a `move` is required to pass it to a function).

**F2 (access + std helpers)**:

- Lower/e2e valid: `m[k]` hit (`Ok(v)`); `m[k]` miss
  (`Err("key not found")`); `m[k]?` propagation; `std.map.get` explicit
  form; `std.map.insert` / `remove` functional (the input is consumed —
  `UseAfterMove` if reused); `std.map.contains` hit and miss;
  `std.map.keys` / `std.map.values` declaration order; `std.map.len`;
  `std.map.clone`; `for` over `std.map.keys`; nested map access
  (`m[k][k2]`); a map in a struct field and in an array element
  (`FieldTypeTag::Map = 13`).
- Lower/e2e invalid: `m[k]` where `k` is the wrong type
  (`ArgTypeMismatch`); `std.map.insert` with a non-whitelisted key type
  (`MapKeyNotComparable`).
- Byte gate: recorded byte-identity expectation on a few map-free
  fixtures against the 0.0.10 merge-base (not a hash gate); every std
  module stays v8; a map-using fixture reaches v9.

**F3 (bytecode v9)**:

- Verify valid: a v9 module with `NewMap` decodes and verifies.
- Verify invalid: `NewMap` in a v1–v8 module → `BadOpcode`; a `Map` tag in
  a v1–v8 struct-field shape → the shape-check error.
- Reader compat: the standing v1–v8 `.flnc` fixtures decode, verify and
  execute unchanged (the reader accepts `{1..=9}`).

---

## 10. Versioning

- `Cargo.toml` workspace version → `0.0.11`; `std/VERSION` → `0.0.11`
  (T08).
- Release publication (tag + six-target archives) follows the
  0.0.7–0.0.10 pipeline; codename settled before the release commit
  (holder's call). Per the established cadence (L9), after the local
  commit + tag the agent **stops and reports** for the user to push.
