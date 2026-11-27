# 0.0.11 — The `map` Type + `std.map` Intrinsics (Tickets)

> Companion to `docs/0.0.11/PLAN.md`. One card = one unit of work, closed
> with tests. Decisions D1–D8 referenced here are **settled at planning
> level** (see PLAN §1); a card may reopen one only by adding a new
> decision entry, not by editing it silently.
> **Status: T01–T08 all done** (implementation + the review follow-up that
> pinned D7 in codegen and the D2 consuming posture in the ownership pass;
> release publication — tag + archives — pending).
> **Posture reminder**: one new opcode (`NewMap` 0xAF), one new tag
> (`FieldTypeTag::Map = 13`), version **v9 by the content-driven rule**
> (PLAN D7) — a module is v9 only when a map was actually encoded, every
> map-free module (including every `std` module) stays v8 and
> byte-identical to 0.0.10. Every card that touches a shared path keeps
> non-feature behavior byte-identical; the full e2e suite is the standing
> regression net and must stay green with unchanged output throughout.
>
> **Regression-test-first rule** (SPEC §7) applies to the one
> source-visible change: `map` becoming a reserved word. The pinning
> fixtures (a program that names a binding/type `map`) are written at the
> merge-base before the lexer change, then flipped to the expected
> rejection in the same commit as the lexer work (the T02/T04 precedent).

---

## T01 — Lexer: the `map` keyword (F1; D1)

- [x] **Regression-first**: pin today's behavior at the merge-base — a
      program that names a binding / type / module `map` compiles and runs
      (e2e fixture with `map` as an identifier, output byte-pinned).
      Written **before** the lexer change; flips to the expected rejection
      in the same commit as the lexer work (the declared breaking set,
      D1).
- [x] `map` becomes a lexer keyword: `TokenKind::Map`, `"map" =>
      TokenKind::Map` in the keyword table (`lexer_impl.rs`), `is_keyword()
      == true`, `keyword_str() == Some("map")`, a `Display` arm rendering
      `map` (the `Display` catch-all renders keywords via `keyword_str`, so
      no separate arm was needed — the token.rs comment documents this).
- [x] ASI tables: `TokenKind::Map` joins `can_start_stmt` (it can begin a
      `map { … }` expression statement) and is recorded in the
      `implies_stmt_end` table (it is a statement starter, the same posture
      as `TokenKind::Struct` / `TokenKind::Enum`).
- [x] Tests: lexer unit tests (`map` lexes to `TokenKind::Map`;
      `is_keyword` / `keyword_str` / `Display`; `mapped` / `map_x` stay
      identifiers); the ASI table tests updated (`can_start_stmt_table`
      yes-list, `implies_stmt_end`); the `map`-as-identifier fixture
      (`tests/e2e/invalid/map_as_identifier.*`) with the reserved-word
      error text.

**T01 note (D1 carve-out for the keyword-member selector)**: the PLAN's
"no std uses `map`" inspection missed that `std.collection.iter.map` uses
`map` as a dotted-path member — the one existing use of the identifier.
Making `map` reserved would break that path, so `map` joins the
`parse_namespace_segment` keyword-member carve-out (alongside `async` /
`await` / `channel`, 0.0.4): legal in member/namespace position, never a
binding name (`reject_keyword_binding_name` still fires on `map = 1`). The
breaking set is unchanged (no program used `map` as a *binding*), and
`std.collection.iter.map` keeps compiling byte-identically. T02 must
decide whether the carve-out stays (member selector) once the `map { … }`
literal and `map[K, V]` type are wired — those are statement-start / type
positions, not this namespace selector.

**Depends on**: nothing.
**Refs**: PLAN §1 D1, §5; `lexer_impl.rs:424` (keyword table);
`lexer/token.rs:122` (`TokenKind`).

---

## T02 — Parser: the `map { k -> v }` literal + the `map[K, V]` type use (F1; D1)

- [x] Parser: new `parse_map_literal` gated on `TokenKind::Map` — `map`
      followed by `{`, then a comma-separated list of `expr -> expr`
      pairs (trailing comma allowed, the array-literal rule), then `}`.
      Produces a new `Expr::MapLit { pairs: Vec<(Expr, Expr)>, span }` AST
      node.
- [x] Parser: `map` in type position starts a `map[K, V]` type use —
      `map` `<` type `,` type `>` (the `Result<T, E>` two-argument shape;
      `[]` is accepted at use sites per the 0.0.7 D9 precedent if the
      resolver's type-use path already handles it — record the exact
      spellings accepted). Produces `ast::Type::Map { key, value }`.
- [x] Parser: `map` in expression position **cannot** begin a block
      expression — `map {` is always a map literal (D1); a `map` token
      reaching the block-expression entry is a parse error.
- [x] Parser errors: `map` without `{` (`` expected `{` after `map` ``);
      `map { k }` without `->` (`` expected `->` after map key ``); `map {
      k -> }` without value (`` expected expression ``); `map { k -> v`
      without `}` (`` expected `}` or `,` ``); `map<T>` with one type arg
      (`` expected `,` after map key type ``).
- [x] Tests: parser unit tests (empty literal, 1/2/3-pair literal, trailing
      comma, nested literal, the `map[K, V]` type use in every type
      position, every parse error above).

**T02 implementation note**: both type-use spellings are accepted — `map[K, V]`
(bracket, canonical, mirroring `Result[T, E]`) and `map<K, V>` (angle,
mirroring `Result<T, E>`); both recurse through `parse_type` for the key and
value, so nested composites resolve. `map {` is gated on `TokenKind::Map` in
`primary`, so it is never reached by the block-expression path (`map { … }` is
always a literal; D1) — the `map`-token-reaching-block case cannot occur. The
resolver already carries `ExprHir::MapLit` / `ast::Type::Map` through (added in
the T02 frontend skeleton), and typeck degrades both the literal and the
type-use to `Unsupported` until T03 adds the real `Type::Map` representation,
whitelist, and `is_copy`; T05 wires lowering. No std or e2e fixture regressed
(`std.collection.iter.map` is protected by the T01 member carve-out).

**T02 review findings (fixed in the same ticket)**: a review of T02 surfaced
three gaps, all closed before the ticket was signed off.

1. *The bridge name leaked into user diagnostics.* The bridge produced
   `Unsupported("map[K, V]")`, and `Unsupported`'s payload is rendered
   verbatim by `Type::name` — so a mismatch printed
   ``expected `map[K, V]`, found `int` ``, i.e. an internal placeholder
   presented as a type name. The bridge now renders the *user-facing* spelling
   with the element types: ``expected `map[int, string]`, found `int` ``.
2. *The bridge collapsed every map type into one.* `Unsupported` compares by
   string equality, so the single opaque name made `map[int, string]` and
   `map[string, int]` interchangeable — a `map[int, string]` value was
   accepted where a `map[string, int]` was expected. The bridge now encodes
   the element types in its name, so distinct map types stay distinct until
   T03 gives them a real representation. Pinned by
   `typeck::tests::two_different_map_types_do_not_unify`.
3. *The parser tests did not pin the error texts.* The five parse errors were
   implemented with the exact texts this ticket specifies, but the tests only
   asserted `ParseErrorKind::Expected { .. }` — a regression could change the
   wording silently. They now assert the text, following the repo's existing
   convention (`parser/tests.rs:1513`). The missing 3-pair literal test was
   added at the same time.

**Known limitation (T03's job, not T02's)**: the bridge's `Unsupported` is
neither Copy nor owned, so a map-typed parameter used twice is *not* flagged
as an owned-value use (a `string` parameter is). The D2 always-owned rule
lands with T03's real `Type::Map` variant, which gains the `is_owned` arm
alongside `is_copy`. T02's ast-level `ast::Type::Map { .. } => false` arm in
`ast_type_is_copy_with_enums` already covers the ast-side verdict.

**Depends on**: T01.
**Refs**: PLAN §1 D1, §3.1, §5; `parser/expr.rs:338` (the `LBrace`
struct-literal gate), `parser/expr.rs:456` (`parse_struct_literal`), the
array-literal tests (`parser/tests.rs:2289`).

---

## T03 — Resolver/Typeck: `Type::Map`, the D5 key whitelist, `is_copy`, the D6 `contains_*` arms (F1; D2/D5/D6)

- [x] Typeck: new `Type::Map(Box<Type>, Box<Type>)` variant; the resolver
      resolves the `ast::Type::Map` use to it.
- [x] Typeck: `is_copy(map[K, V]) == false` for every `K` / `V` (D2) — the
      `is_copy` match (`typed_hir.rs:305`) gains a `Type::Map(_) => false`
      arm; `is_copy_exhaustive` (`typeck/tests.rs:976`) updated.
- [x] Typeck: the D5 key whitelist (`int` / `bool` / `string`) enforced at
      every `map[K, V]` type use — a non-whitelisted key is
      `MapKeyNotComparable` (new `TypeckErrorKind`) with a hint naming the
      whitelist. Covers `float` / `unit` / `Range` / `enum` / `struct` /
      `array` / `Result` / `box` / `Future` / `channel` / `ptr` / function
      values.
- [x] Typeck: the D6 `contains_*` walk (`contains_in_value_position`)
      gains a `Type::Map` arm that recurses into `K` and `V` — a
      `map[K, Future<_>]` / `map[K, channel<_>]` / `map[K, ptr<_>]` is
      rejected at the literal and at every `std.map.insert` / `values`
      call.
- [x] Typeck: `==` / `!=` on a map operand is `InvalidOperand` (D5's
      enum/array posture) — the `typeck_binary_op` gate
      (`infer.rs:3231`) gains `Type::Map(_)` in its reject list.
- [x] Typeck: `as string` on a map is rejected (no auto-derived
      formatting; the 0.0.6 enum posture).
- [x] Tests: typeck unit tests (`is_copy` false, the whitelist
      accept/reject matrix, the `contains_*` rejections, the `==`
      rejection, the `as string` rejection); `is_copy_exhaustive` green.

**Depends on**: T02.
**Refs**: PLAN §1 D2/D5/D6, §5; `typed_hir.rs:305` (`is_copy`),
`typed_hir.rs:233/245/267` (`contains_future` / `contains_channel` /
`contains_ptr`), `infer.rs:3231` (the `==` gate), `typeck/tests.rs:976`
(`is_copy_exhaustive`).

---

## T04 — Typeck: `m[k]` access through `index_suffix` (F2; D4)

- [x] Typeck: the `typeck_index` branch (`infer.rs:2410` / `:4584`) gains
      a map case — base `Type::Map(K, V)` + index `K` → result
      `Type::Result(V, string)` (D3/D4). A wrong-type index is
      `ArgTypeMismatch` (the `unify` path reports a `TypeMismatch`
      against the key type, the index expression's span).
- [x] Zero parser change (D4): `m[k]` parses as an ordinary
      `index_suffix`; the change is typeck-only.
- [x] `m[k]?` composes with `?` (the 0.0.2 propagation rules apply to the
      `Result[V, string]` result).
- [x] Tests: typeck unit tests (hit type, miss type, wrong-index-type
      rejection, the `?` composition shape); the standing array-index
      fixtures unchanged (the map case is a new base type, not a change to
      the array path).

**T04 implementation note (the D6 read-route seal)**: the map branch of
`typeck_index` also runs `check_map_value_storable` on the value type — the
same `contains_*` seal applied at annotation / literal — so a `m[k]` *read*
rejects a `map[K, Future<V>]` / `map[K, channel<V>]` / `map[K, ptr<V>]` base
(`FutureNotStorable` / `ChannelNotStorable` / `PtrNotStorable`) instead of
silently deep-copying a suspended frame / aliased buffer / pointer out of
the value slot. This closes a review finding: the `FutureIndexRead` walk
is array-only (a map's `elem_ty` is `Result<V, string>`, whose
`contains_future()` is false even when `V` is a `Future`), so the read-route
seal is the one that pins D6 on the `m[k]` path. Pinned by
`typeck::tests::map_index_read_seals_future_value_slot` and
`map_index_read_seals_channel_and_ptr_value_slots`.

**Depends on**: T03.
**Refs**: PLAN §1 D3/D4, §3.2, §5; `infer.rs:2410` / `:4584`
(`typeck_index`), `infer.rs:4897` (`TypedExprHir::Index`).

---

## T05 — std: the `std.map` eight-intrinsic batch + Lower/Codegen/Verify/VM wiring (F2/F3; D3/D7/D8)

> **Status: done** — implemented across the codegen/std-map commits; the
> review follow-up added the verify-gate, duplicate-key-overwrite,
> chained-`Ok`-payload lookup and shared-intrinsic-prologue polish (see the
> as-built notes below and the `map_*` unit tests in `fleen-verify` /
> `fleen-vm` / `fleen-compiler`).

- [x] `std/map.fln`: doc comments for the eight intrinsics (signatures,
      the D3 missing-key rule, the D5 key whitelist, the D2 functional
      `insert` / `remove`, the D8 declaration-order guarantee).
- [x] `typeck/builtins.rs`: the eight `BuiltinSig`s — `std.map.get` /
      `insert` / `remove` / `contains` / `keys` / `values` / `len` /
      `clone` — all `BuiltinLowering::Intrinsic`, schematic over `K` / `V`
      (the `std.str` batch shape, `builtins.rs:187-257`).
- [x] VM: `Value::Map(Box<MapCell>)` variant (the `KeyHash` wrapper, D8);
      the `NewMap` dispatch; the map arm of the array-index opcode (or the
      `std.map.get` intrinsic — record the exact wiring); the eight
      `std.map.*` intrinsic arms; the deep-copy path in `clone` /
      assignment / `DupDeep`.
- [x] Lower: the map-literal node → `NewMap` (pops `2n` values, pushes one
      `Value::Map`); the eight intrinsics ride the intrinsic-call path;
      `m[k]` rides the array-index arm (or desugars to `std.map.get` — the
      wiring recorded here).
- [x] Codegen: `NewMap` (0xAF) encoder + `FieldTypeTag::Map = 13`; version
      v9 content-driven (a `NewMap` in the module bumps the writer to v9).
- [x] Verify: the `NewMap` depth table entry (`-2n + 1`); the v9 opcode
      gate (`NewMap` in a v1–v8 module → `BadOpcode`); the `Map` tag
      recognized in struct-field / array-element shape checks.
- [x] Tests: VM unit tests (`NewMap` builds, index hit/miss, each of the
      eight intrinsics, deep-copy); verify unit tests (depth, the v9 gate);
      codegen unit tests (the `NewMap` byte, the `Map` tag, the v9 writer
      / v8-map-free writer); the standing std byte-identity pin
      (`fleen-vm/tests/std_byte_identity.rs`) green (std stays v8, D7).

**Depends on**: T03, T04.
**Refs**: PLAN §1 D3/D7/D8, §5/§6/§7; `builtins.rs:187-257` (the std.str
batch), `vm.rs:1315` (the `std.collection.iter.map` intrinsic, the shape
precedent), `bytecode.rs:157` (`FieldTypeTag`).

---

## T06 — Integration matrix: e2e fixtures for the type, literal, access, and helpers (F1/F2/F3)

> **Status: done** — as-built, the e2e layer carries 2 valid fixtures
> (`map_basics`, `map_std_intrinsics`) and 11 invalid `map_*` fixtures
> (key whitelist incl. `unit` / `Range`, value seal, `==` / `as string`
> rejections, `UseAfterMove` after a consuming `insert`, wrong index
> type, `map`-as-identifier); the remaining coverage lives in the unit
> tests (`fleen-vm/tests.rs` map arms, `fleen-verify/tests.rs` v9 gate,
> `fleen-compiler` typeck/codegen map tests).

- [x] e2e valid: empty literal with annotation; N-pair literal (1, 2, 3
      pairs); trailing comma; nested literal; `m[k]` hit; `m[k]` miss;
      `m[k]?` propagation; `std.map.get` explicit form;
      `std.map.insert` / `remove` functional (`UseAfterMove` if the input
      is reused); `std.map.contains` hit and miss; `std.map.keys` /
      `std.map.values` declaration order; `std.map.len`; `std.map.clone`;
      `for` over `std.map.keys`; nested map access (`m[k][k2]`); a map in
      a struct field and in an array element (`FieldTypeTag::Map = 13`).
- [x] e2e invalid: `map[float, V]` / `map[SomeStruct, V]` /
      `map[SomeEnum, V]` / `map[[int], V]` / `map[Result[int, string], V]`
      / `map[unit, V]` / `map[Range, V]` (each `MapKeyNotComparable`);
      `map[string, Future<int>]` / `map[string, channel<int>]` /
      `map[string, ptr<int>]` (the `contains_*` rejections); `map == map`
      (`InvalidOperand`); `as string` on a map; the `map`-as-identifier
      pin (T01's fixture, now invalid); `std.map.insert` with a
      non-whitelisted key.
- [x] Byte gate: recorded byte-identity expectation on a few map-free
      fixtures against the 0.0.10 merge-base (not a hash gate); every std
      module stays v8; a map-using fixture reaches v9.
- [x] Reader compat: the standing v1–v8 `.flnc` fixtures decode, verify
      and execute unchanged (the reader accepts `{1..=9}`).
- [x] Full suite green with unchanged stdout/stderr for every
      non-map fixture.

**Depends on**: T05.
**Refs**: PLAN §9.

---

## T07 — Docs sync (F4)

> **Status: done** — DESIGN.md §9.10 (as-built notes), §17, §18;
> SYNTAX.ebnf `map_lit` / `map_type`; BYTECODE.md `NewMap` + tag + v9
> reader note; SPEC/AGENTS/CLAUDE goal paragraph + matrix; CHANGELOG
> `[0.0.11]`.

- [x] `docs/DESIGN.md`: §9 (the `map` type subsection — literal, type,
      `index_suffix` access, the D5 key whitelist, the D2 ownership
      posture, the D8 ordering guarantee); §10 (the always-owned rule,
      the `contains_*` map arm); §12 (the `==` gate gains `map`); §17
      feature-matrix rows; §18 roadmap row.
- [x] `docs/SYNTAX.ebnf`: the `map_lit` production (`map { map_entry }`),
      the `map_type` production (`map [ type , type ]`), the `->` separator
      in a map entry.
- [x] `docs/BYTECODE.md`: the `NewMap` opcode row (§5.x), the
      `FieldTypeTag::Map = 13` tag row (§4), the v9 reader note (the
      content-driven rule applied to `NewMap`).
- [x] `SPEC.md` / `AGENTS.md` / `CLAUDE.md`: the 0.0.11 goal paragraph +
      feature-matrix row(s) + a quick-reference section for `map`.
- [x] `CHANGELOG.md`: `[0.0.11]` entries — Added (`map` type, the `map { …
      }` literal, `m[k]` access, the eight `std.map.*` intrinsics, `NewMap`
      0xAF, `FieldTypeTag::Map = 13`, bytecode v9 content-driven);
      Breaking (`map` is a reserved word).

**Depends on**: T01–T06.
**Refs**: PLAN §1 D1–D8, §3, §7.

---

## T08 — Examples, version bump, release gate (F4)

> **Status: done (release publication pending)** — `examples/0.0.11/`
> (`map_demo.fln` + `.expected` + README), CHANGELOG finalized, workspace
> version and `std/VERSION` at 0.0.11, full gate suite green. The tag /
> archive cut follows the 0.0.7–0.0.10 pipeline (L9: local commit + tag,
> then stop and report for the user to push).

- [x] `examples/0.0.11/`: a single-file `map` demo (every D5-whitelisted
      key type, `m[k]` hit/miss/`?`, the eight helpers, `for` over keys, a
      map in a struct field and in an array element, nested maps) + a
      README in the 0.0.7–0.0.10 layout.
- [x] `CHANGELOG.md` finalized; `Cargo.toml` workspace version →
      `0.0.11`; `std/VERSION` → `0.0.11`.
- [x] Full gate suite: `cargo fmt -- --check`, `cargo clippy -- -D
      warnings`, `cargo test`, `cargo doc --no-deps`.
- [x] Release follows the 0.0.7–0.0.10 pipeline; codename settled here
      (holder's call). Per the established cadence (L9): after the local
      commit + tag, **stop and report** for the user to push — the push
      is what starts the Release workflow.

**Depends on**: T07.
**Refs**: PLAN §10.
