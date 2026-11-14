# 0.0.7 — Parameterized Types + Pattern Matching Completion (Tickets)

> Companion to `docs/0.0.7/PLAN.md`.  One card = one unit of work, closed with
> tests.  Decisions D1–D10 referenced here are **settled at planning level**
> (see PLAN §1); a card may reopen one only by adding a new decision entry,
> not by editing it silently.
> **Status: T01–T11 implemented and green (T07 emits bytecode v7 with the
> `enum_defs` arity field; T08 confirmed the VM needs no further changes;
> T09 landed the e2e fixtures/examples and fixed a T06 nested-payload
> failure-edge over-pop; T10 landed the tour example + CHANGELOG; T11 synced
> the docs — the version bump / release publication is separate and pending).**

---

## T01 — Parser: type parameters on `enum` declarations + parameterized type use sites (F1; D1)

- [ ] `enum_decl` grammar gains an optional `type_params` list:
      `enum Name<T> { … }` / `enum Name<A, B> { … }`. Parameters are
      identifiers only — no defaults, no bounds.
- [ ] `named_type` grammar gains an optional `type_args` list. Both
      spellings accepted: `Option<int>` (angle) and `Option[int]` (bracket,
      matching the existing `Result[T, E]` form — D9).
- [ ] `AST::EnumDecl` carries `type_params: Vec<String>`; the parser
      enforces at least one parameter (an empty `<>` is a parse error).
- [ ] Use-site type applications produce a new AST node or an annotated
      `Named { name, args, span }` — parser emits it, typeck resolves.
- [ ] Tests: parser valid (1-param, 2-param, trailing comma, both bracket
      and angle spellings at use sites); invalid (bare `<>`, orphan `>` at
      use site, `enum X<> { }` empty params, parameter shadowing a real type
      name in the body).

**Depends on**: nothing.
**Refs**: PLAN §3.2, D1, D9.

---

## T02 — Parser: recursive pattern grammar — multi-payload, nested, or-patterns, type guards (F3–F6; D4–D7)

- [ ] Replace the flat `Pattern` enum with a **recursive** shape. The
      0.0.6 variants (`Literal`, `Ident`, `ResultCtor`, `Enum`) become
      constructors of a recursive `Pattern`:
      - `Literal(Expr)` — unchanged
      - `Ident(String, Span)` — unchanged (binding or `_` wildcard)
      - `ResultCtor { ctor, payload: Box<Pattern>, span }` — payload is now
        a full `Pattern`, not just a binding name (enables `Ok(Some(x))`)
      - `Enum { name, payloads: Vec<Pattern>, span }` — payloads is now a
        `Vec<Pattern>` (multi-payload, D4; each element is a full pattern,
        D5)
      - `Or(Vec<Pattern>, Span)` — new (D6)
      - `TypeGuard { pattern: Box<Pattern>, ty: Type, span }` — new (D7)
- [ ] Grammar (`SYNTAX.ebnf` §3.2 of PLAN):
      ```
      pattern         = or_pattern ;
      or_pattern      = typed_pattern , { "|" , typed_pattern } ;
      typed_pattern   = constructor_pattern , [ ":" , type ] ;
      constructor_pattern =
                        literal_pattern
                      | ident_pattern
                      | ident , "(" , [ pattern , { "," , pattern } ] , ")" ;
      ```
- [ ] Precedence: `:` binds tighter than `|` (`a: T | b: U` parses as
      `(a: T) | (b: U)`). `|` binds looser than a constructor application.
- [ ] `Ok(x)` / `Err(e)` shortcuts stay at the parser level (they win over
      `Enum` for those two names, as 0.0.6 T02); they produce
      `ResultCtor` with the payload as a full `Pattern`.
- [ ] Tests: parser valid (zero/multi-payload `Variant(a, b)`,
      `Some(Some(x))`, `Some(_)`, `Ok(Pair(a, b))`, `1 | 2`, `Some(v) |
      None`, `v: int`, `Some(v): Option<int>`); invalid (orphan `|`, orphan
      `:`, mismatched paren depth in nested patterns, trailing content
      after a complete pattern in an arm).

**Depends on**: nothing (parallel with T01).
**Refs**: PLAN §3.2, D4, D5, D6, D7.

---

## T03 — Resolver: type-parameter scope + arity-carrying enum registration (F1; D1)

- [x] `EnumDeclHir` gains `type_param_count: usize` (replacing nothing —
      monomorphic enums keep 0).
- [x] Type parameters register in a **declaration-local scope** during
      resolution; they are not visible outside the enum body and are not
      values.
- [x] Enum names in the module export table now carry arity;
      `ExportDef::enum_variants` unchanged (variant names do not change with
      parameterization); `register_imported_items` skips enums as before.
- [x] Cross-module: importing a parameterized enum carries the arity;
      use-site type applications at the import site are checked against the
      declared arity at typeck (T04).
- [x] Collision rules unchanged: variant names still enter the per-enum
      namespace; type-parameter names do **not** enter the binding scope.
- [x] Tests: resolver valid (import / pub of parameterized enums, dotted
      constructor paths); invalid (private parameterized enum cross-module,
      name collisions).

**Depends on**: T01.
**Refs**: PLAN §5 (Resolve row), D1.

**As-built (post-review hardening, 0.0.7)**:
- Duplicate-parameter diagnostic is precise: `ast::EnumDecl.type_params` is
  `Vec<TypeParam>` (name + identifier span — **as-built deviation from T01's
  `Vec<String>` card text**, needed so `DuplicateBinding.first_span` points
  at the first occurrence instead of the enum head; SPEC §4). The error's own
  span is the duplicate parameter's identifier.
- The declaration-local scope is installed only by
  `Resolver::resolve_enum_variants` (the single setter, restored before it
  returns) instead of inline set/clear around payload resolution — no early
  path can leak it; a `debug_assert` guards emptiness outside the window.
- Duplicate-name registration uses one helper (`Resolver::push_duplicate`)
  for all four sites (module items, scope bindings, type parameters, enum
  variants).
- `exports_of_ast` builds `ExportDef` through `ExportDef::simple` /
  `ExportDef::enum_export` constructors instead of a six-element positional
  tuple.
- New invalid fixture `tests/resolver/invalid/param_type_param_not_a_value.fln`
  + tests `param_type_param_not_in_binding_scope` (UndeclaredVariable) and
  `param_enum_duplicate_type_param_points_at_first_occurrence` (first_span
  is the identifier, distinct from the error span).
- Note: PLAN §5's Resolve row says use-site applications "check arity"; per
  the ticket that check lands at typeck (T04) — resolve only carries the
  arity through the export table. PLAN wording stays as written; T04
  implements the check.

---

## T04 — Typeck: parameterized type resolution + fully-applied-use + argument-dependent `is_copy` (F1/F2; D1/D2)

- [x] `Type` gains `App(Box<EnumInfo>, Vec<Type>)` — a fully-applied
      parameterized enum type. The `EnumInfo` inside carries the
      declaration; the `Vec<Type>` carries the substituted arguments.
      Monomorphic enums keep `Type::Enum(EnumInfo)` (arity 0, no `App`
      wrapper).
- [x] `type_param_count` on `EnumInfo` is checked at every use-site type
      application: too few → `TypeConstructorNeedsArgs { name, expected,
      found, span }`; too many → `ArityMismatch` (reuse).
- [x] Parameter substitution: inside a declaration body, parameters are
      resolved to `Type::Param(usize)` (a new variant for declaration-local
      type parameters — distinct from `Type::Var(u32)` which is for
      schematic builtin signatures). At use sites, `Param(i)` in payloads is
      replaced by the corresponding argument.
- [x] **Argument-dependent `is_copy` (D2)**: `is_copy(Type::App(info, args))`
      substitutes args into the declaration's payload types and re-runs the
      struct-mirror rule. `is_copy(Type::Enum(info))` (monomorphic) is the
      arity-0 case and behaves exactly as 0.0.6.
- [x] Constructor typing: `Option.Some(1)` — the argument's type fixes
      `T = int`; result is `App(Option, [int])`. When the parameter does not
      appear in the chosen variant's payloads (rare — `enum Weird<T> {
      Empty(T), Nothing }` and the arm is `Weird.Nothing`), the surrounding
      annotation must fix it; otherwise `TypeParameterAmbiguous { name,
      span }`.
- [x] Storage rules: `ptr<T>` payloads rejected (unchanged);
      `Future` / `channel` payloads inherit boundary rules (unchanged).
      Parameterized payloads inherit the same rules via substitution.
- [x] **`RecursiveEnum` carries forward (D3)**: the cycle check walks
      through parameterized payloads (the `collect_named_type_refs`
      recursion into `Named` args landed with T03 at 30ed027; this card
      adds the `enum List<T> { Cons(T, List<T>) }` test — rejected with
      the full path).
- [x] Tests: typeck valid (`Option<int>` Copy flows free; `Option<string>`
      needs move / clone; `Pair[string, int]` owned; nested `Option<Pair[A,
      B]>`); invalid (bare `Option` as type, `Option<int, string>` too many
      args, `List<T>` recursive, `ptr` payload, ambiguous empty-payload
      parameter).

**Depends on**: T01, T03.
**Refs**: PLAN §3.3, D1, D2, D3.

---

## T05 — Typeck: recursive pattern typecheck + exhaustiveness (F3–F7; D4–D7)

- [x] Pattern typecheck walks the scrutinee type alongside the pattern
      recursively:
      - `Enum { name, payloads }` on scrutinee `App(E, args)`: substitute
        args into variant payload types; each `payloads[i]` typechecks
        against `variant.payload_types[i]` (recursively for nested patterns).
        Wrong count → `ArityMismatchInPattern { variant, expected, found,
        span }` (new diagnostic; distinct from the constructor
        `ArityMismatch`).
      - `ResultCtor` on `Result[T, E]`: unchanged from 0.0.2/0.0.6, except
        the payload pattern is now recursive.
      - `Or(alternatives)` (D6): typecheck each alternative against the
        scrutinee; require identical binding-name sets with identical types
        across alternatives; mismatch → `OrPatternBindingMismatch { name,
        first_type, second_type, span }` (new diagnostic).
      - `TypeGuard { pattern, ty }` (D7): `ty` must be the scrutinee type
        (or a supertype — there are no supertypes in 0.0.7, so equality);
        mismatch → `PatternTypeMismatch`. Bindings inside `pattern` carry
        the types they had; the guard ascribes `ty` to the overall match,
        not to individual bindings.
- [x] Bindings from all nesting levels and all or-alternatives (after
      consistency check) are declared in the arm scope with their
      substituted types. Duplicate binding name across levels →
      `DuplicateBinding`.
- [x] **Exhaustiveness (F7)**: generalize the 0.0.6 T05 algorithm to handle
      or-patterns — an or-pattern contributes the **union** of its
      alternatives' variant coverage. Multi-binding arms contribute the
      variant they name (same as 0.0.6). Guard-free arms covering all
      variants are exhaustive; `ChooseNotExhaustive` names the missing
      variants (uncovered variants listed by name).
- [x] Tests: typeck valid (nested `Ok(Some(x))`, multi-binding `Pair(a,
      b)`, or-pattern with same binding `Some(v) | Other(v)`, type guard on
      ident binding); invalid (nested constructor mismatch, arity mismatch
      in pattern, or-pattern with mismatched bindings, or-pattern with
      mismatched types for the same binding, type guard on wrong type,
      exhaustiveness missing a variant, or-pattern not covering the full
      set).

**Depends on**: T02, T04.
**Refs**: PLAN §3.4, D4–D7.

**As-built (0.0.7)**:
- **Strict arity supersedes the 0.0.6 leniency (D4)**: a count mismatch is
  now always `ArityMismatchInPattern` — including the 0.0.6 forms
  `when Rect(a)` on a 2-payload variant and `when Point(x)` on a zero-payload
  variant (both previously `EnumPatternMismatch`). The 0.0.6-era tests were
  updated to the D4 forms, and the T04 e2e fixture `param_enum_demo.fln`
  changed `when P(first)` → `when P(first, second)`.
- **Resolver edit** (card is typeck-only, but T02's staging comment reserved
  this for T05): `Pattern::Or` resolution now resolves each alternative in a
  child block scope and merges its bindings into the arm scope — the first
  alternative to bind a name wins the `BindingId`, later same-name
  declarations are skipped (`ScopeStack::current_bindings` accessor added).
  Duplicates *within* one alternative still report `DuplicateBinding`; the
  cross-alternative same-names/same-types rule is typeck's.
- `OrPatternBindingMismatch` boxes its two `Type` fields
  (`result_large_err`); a binding missing from one alternative is reported
  with `unit` as that side's type (documented in the diagnostic text).
- Type-guard mismatch reuses `PatternTypeMismatch` with the label
  `"type-guard"`; its Display text was generalized from "requires a
  `Result<T, E>` scrutinee" to "requires a matching scrutinee" (no fixture
  asserted the old wording; e2e stderr matching is substring-based).
- **Lower staging gate (T06)**: typeck now accepts or-pattern / type-guard /
  multi-payload / nested patterns, so lower explicitly rejects what it
  cannot yet lower rather than miscompile (the T02 review hazard):
  `lower_choose_enum` rejects a top-level `Or` / `TypeGuard` arm (previously
  it silently became the `otherwise` fallback with unstored bindings);
  nested/multi-payload enum payloads keep the `first_payload_binding`
  `0.0.7 T06` error; nested Result payloads and non-Ok/Err arms keep their
  gates. Full-pipeline support lands in T06.
- Type guards contribute their inner pattern's coverage to exhaustiveness
  (the ascription is compile-time only, D7); a bare ident guard never
  covers an enum variant (conservative, matching 0.0.6 ident behavior).
- Review pass: `ArityMismatchInPattern` carries no kind-level `span` (the
  diagnostic-level `TypeckError::span` suffices, like the sibling pattern
  diagnostics); lower's `choose_arm_span` delegates to
  `TypedPatternHir::span` instead of duplicating the match; new lower tests
  pin the or-pattern / type-guard staging gates, and a typeck test pins the
  reverse-order or-pattern mismatch (binding missing from the first
  alternative).

---

## T06 — Lower: multi-binding arms, nested-pattern chains, or-pattern alternative tests (F3–F6; D4–D6)

- [x] **Multi-binding arms (D4)**: a `Variant(p₁, …, pₙ)` arm lowers as
      the 0.0.6 T05 chain (park scrutinee, `Dup; EnumIs v; JumpIfTrue
      body`), and the body block opens with `EnumPayload v i; StoreLocal
      xᵢ` for each binding `xᵢ` extracted from `pᵢ` (only for sub-patterns
      that are `Ident` bindings or that contain bindings — a literal or
      wildcard sub-pattern skips the store).
- [x] **Nested-pattern chains (D5)**: for `Ok(Some(x))` (or any nesting),
      the outer test succeeds and the outer payload is extracted and
      **re-parked**; the inner pattern becomes a new `EnumIs` / `EnumPayload`
      chain on the re-parked value. The chain terminates when the innermost
      pattern is a binding or a literal. Guard arms stage payloads as in
      0.0.6 T05 (move out of a `Dup` copy before the guard; jump to the
      next test on failure). Every edge is resolved immediately (no
      deferred jumps).
- [x] **Or-pattern alternative tests (D6)**: a `p₁ | p₂` arm lowers as
      **multiple** test chains — one per alternative — all jumping to the
      same body block on success. Each alternative independently extracts
      its bindings before the jump (they are the same names with the same
      types; only one alternative's extraction runs at runtime). The body
      block is the shared successor of all alternative test blocks.
- [x] **Type guards lower as their inner pattern (D7)** — no runtime work;
      the ascription is compile-time only. `when v: int { … }` lowers as
      `when v { … }` (a binding).
- [x] Stack-depth validation: the `assert_stack_depths_consistent`
      validator (introduced in 0.0.6 T05 review) must accept the new arm
      shapes — or-pattern chains, nested chains, multi-binding bodies.
- [x] Tests: lower unit tests on hand-checked fixtures (multi-binding arm
      shape; nested chain for a 2-level pattern; or-pattern with two
      alternatives; guard inside a nested pattern).

**As-built (T06)**:
- Uniform sub-pattern contract (`lower_subpattern_match`, `FailEdge`): on
  success the subject is consumed (bindings stored); on failure the subject,
  its temps, and `extra_below` re-parked values are popped and control jumps
  to `on_fail` — failures propagate directly, no cleanup edges. Nested
  extraction passes `extra_below + 1` per parked level; `UnwrapOk/UnwrapErr`
  consumes the copy, so the count survives a Result unwrap. The leaf test
  shape (subject copy + conditional + matched/fail blocks) is shared by the
  literal and zero-payload variant arms (`emit_leaf_test`), and every
  failure-cleanup block is built by one `fail_cleanup_block` helper.
- Fallthrough discipline: every conditional is preceded by
  `FnBuilder::spill_for_fallthrough`, so the untaken edge always lands on the
  continuation block created right after it, regardless of pre-created
  blocks. Conditionals pop the bool on both edges (BYTECODE.md §5.6/§6.8/VM).
- Result choose general path: one `UnwrapOk`/`UnwrapErr` per side, then one
  attempt block per arm + defensive tail (typeck-guaranteed unreachable —
  see the exhaustiveness note below); each attempt matches a `Dup`/`DupDeep`
  copy so failures retry the next arm with the payload parked; guard failure
  resolves directly to the next attempt.
- Enum choose general path: one test block per flattened alternative, shared
  body block per arm, guard evaluated with the scrutinee parked and popped
  only when the arm is taken; payload bindings of one or-arm are unified onto
  the first alternative's `BindingId` in typeck (`unify_or_alt_bindings`) so
  all alternatives store the same slot.
- **D4 deviation**: in the general path, payload extraction moved out of the
  body block into the test chain (each alternative extracts its own bindings
  before jumping to the shared body — required for D6 stack-depth
  normalization), so the body block no longer opens with
  `EnumPayload; StoreLocal`. Literal sub-patterns still skip the store; a
  wildcard `_` payload is a resolver binding and does store (harmless).
- **Nested or-patterns in payloads are lowered, not rejected**: the parser
  accepts e.g. `Pair(1 | 2, x)` (payloads parse as patterns), and the
  recursive sub-pattern machinery lowers a nested `Or`/`TypeGuard` on the
  enum and Result paths. The only remaining defensive error is a nested
  or/type-guard directly inside a *generic* choose alternative (the
  literal/ident chain is not recursive).
- **Exhaustiveness is payload-aware (tightens F7/D3)**: a variant arm covers
  its variant only when every payload sub-pattern is total over its payload
  type (`patterns_are_total`), and a Result side is covered only when its
  guard-free payload patterns jointly cover the payload type. A literal
  never covers a whole type, so `when P(1, x)` alone does not cover `P` and
  `Ok(1) | Ok(2)` alone does not cover `Ok` — both are now
  `ChooseNotExhaustive` (previously they compiled and fell into lower's
  defensive tail at runtime). `Ok(Some(x))` + `Ok(None2)` still cover `Ok`
  jointly; ident/wildcard payloads keep covering.
- The three T05 staging gates are removed: `first_payload_binding` deleted;
  enum and generic choose accept or-pattern / type-guard arms.
- Verified beyond stack-depth: runtime smoke of all four shapes plus generic
  `1 | 2` / guarded / wildcard arms executes with correct results; the four
  hand-checked lower fixtures pin the instruction shapes (nested-chain
  unwrap→re-park→inner-test ordering, per-alternative extraction into the
  shared first-alternative slot, guard-after-staging, payload store pairs).

**Depends on**: T05.
**Refs**: PLAN §3.4, D4, D5, D6, D7.

---

## T07 — Bytecode v7: `enum_defs` arity field + codegen + verify (F8; D8)

- [x] `enum_defs` v7 layout gains `type_param_count: u32` per enum, written
      immediately after `variant_count`. The parameter **names** are not
      serialized (declaration-local; no runtime meaning).
- [x] `MODULE_VERSION = 7`; `READ_VERSIONS = [1, 2, 3, 4, 5, 6, 7]`.
      v1–v6 fixtures are byte-identical (append-only rule respected: one
      new u32 per enum in `enum_defs`, no new opcodes, no `FieldTypeTag`
      change).
- [x] verify: `enum_defs` v7 reader consumes the new field; no new
      cross-checks beyond that (the arity field is descriptive, not a
      runtime contract).
- [x] Verify the fixture-regeneration instruction is stale (no committed
      fixture carries `enum_defs`: only v1/v2/v3 exist, the v2 fixture is
      hand-built and the v3 builder pins `module.version = 3`), so nothing
      to regenerate; golden-encoding tests that build their own modules
      need no change. (Supersedes the original "regenerate the v6-compat
      fixture" wording — see the as-built note below.)
- [x] Tests: v7 write / read round-trip (parameterized and monomorphic
      enums); v1–v6 fixture decode unchanged; truncated `enum_defs`
      rejected.

**Depends on**: T04 (needs arity to emit).
**Refs**: PLAN §7, D8.

**As-built (0.0.7)**:
- The arity rides through MIR: `MirEnumDef` gains
  `type_param_count: u32`, set by lower from `TypedEnumDef.type_params.len()`.
  It cannot be derived in codegen from the payload types — a parameter need
  not appear in any variant's payloads, and the payloads are declaration-site
  skeletons (`Param(i)`).
- Writer/reader symmetry: the new u32 is written after `variant_count` only
  when `m.version >= 7`, and read only when the *file* `version >= 7`; the
  table itself stays gated at `>= 6` on both sides, so a v6 stream decodes
  unshifted and the v1–v5 prefix is untouched (pinned by
  `enum_defs_append_only_keeps_v5_bytes_identical`, updated to compare the
  v7 stream against a v5-stamped clone). The reader's `capacity_hint` for
  `enum_defs` becomes version-aware (min entry 8 bytes on v7, 4 before).
- **No fixture regeneration was needed — the card's instruction is stale
  relative to the repo**: there is no committed v6 fixture; only
  v1/v2/v3 exist (`tests/fixtures/{v1,v2,v3}`), the v2 fixture is hand-built
  and the v3 builder pins `module.version = 3`, so all byte-identity /
  decode tests stayed green across the bump without touching any fixture.
- **Version-gate ripple handled with the compiled-output/legacy distinction**:
  every `assert_eq!(module.version, 6)` on *compiled* output was bumped to 7
  (compiler lib tests + doc examples, codegen tests, verify ptr/enum
  end-to-end tests, vm `run_src`/await tests, codegen_integration); the
  hand-built legacy modules (`fleen-verify` `v6_enum_module`,
  `fleen-vm` `enum_vm_module`) stay at `version: 6` — with
  `type_param_count: 0` added to their literals — and the vm gate tests that
  probed "unsupported" with version 7 now probe with 8.
- **VM run gate extended here, narrowing T08**: `Vm::run_inner` accepted
  `1..=6`, so every compiled-output module was `UnsupportedVersion(7)` at
  execution. The gate now accepts `1..=7` (comment: v7 changes only the
  descriptive `enum_defs` field, opcode set and `Value::Enum` shape are
  identical to v6). T08's remaining work is the confirmation card: run the
  full toolchain on a parameterized enum and confirm the legacy reader path.
- verify needs no source change: its per-version opcode floors (`version <
  N && opcode >= BASE`) have no upper gate, and the arity field is
  descriptive (D8) — no new cross-checks.
- New tests: `enum_defs_v7_round_trips_type_param_count` (1-param / 2-param /
  monomorphic through the byte round trip) and
  `v6_stream_encodes_without_arity_and_decodes_as_zero` (v7 encoding is
  exactly one u32 per enum longer; a v6-stamped stream decodes all-zero
  arity). The v6-era `truncated_enum_defs_is_rejected` now exercises cuts
  inside the v7 table (including the interleaved arity bytes).

---

## T08 — VM: no changes (confirmation card) (F8; D8)

> Note (T07 as-built): the VM *run gate* `1..=6` → `1..=7` already landed
> with T07 (`Vm::run_inner`) — it was required for any compiled-output
> module to execute. This card now only runs the confirmations below.

- [x] Confirm no VM source changes are needed: `Value::Enum` shape
      unchanged; `EnumNew` / `EnumIs` / `EnumPayload` encodings and stack
      effects unchanged; defensive errors unchanged.
- [x] Confirm the v7 `.flnc` is decoded by the existing v6 VM reader with
      the one extra u32 per enum skipped (or validated) in the `enum_defs`
      reader.
- [x] Test: existing VM enum tests green unchanged; a v7 module with a
      parameterized enum runs correctly through the full toolchain.

**Depends on**: T07.
**Refs**: PLAN §6, D8.

**As-built (0.0.7)**:
- Zero VM source changes beyond the T07 run gate: `Value::Enum { variant,
  payload: Box<[Value]> }` is unchanged (`value.rs`), the
  `EnumNew`/`EnumIs`/`EnumPayload` dispatch and their defensive errors
  (`EnumIndexOutOfRange` / `EnumVariantOutOfRange` / `EnumPayloadMismatch`)
  are untouched, and the existing hand-built-bytecode VM enum tests pass
  unmodified (full `fleen-vm` suite green).
- **Card wording correction**: the `.flnc` reader is *not* in the VM —
  decoding lives in the shared `fleen_compiler::codegen::flnc`
  (`from_bytes`, `READ_VERSIONS = {1..7}`); the VM only consumes the decoded
  `Module`. The confirmation is therefore that the v7 stream decodes with
  the per-enum arity u32 consumed and executes.
- Three new VM tests pin the confirmations (all reusing the T05-era
  `run_src` full-pipeline helper, which asserts `module.version ==
  MODULE_VERSION`): `v7_parameterized_enum_runs_through_the_full_toolchain`
  (`Option<int>` Copy flow, construct + choose + payload),
  `v7_parameterized_enum_entry_value_keeps_the_enum_shape` (the entry
  returns the enum itself and the test asserts the `Value::Enum { variant,
  payload }` shape structurally — D8's "the parameter is compile-time only"
  as a pinned test, not a claim), and
  `v7_flnc_round_trip_runs_an_owned_parameterized_enum`
  (compile → `to_bytes` → `from_bytes` → `Vm::run`, the
  owned `Option<string>` flow).

---

## T09 — Integration: e2e fixtures + std untouched confirmation (F9; D9)

- [x] E2E valid fixtures:
      - `tests/e2e/valid/option_demo.fln` (+ `.expected`) — `Option<int>`
        Copy flow, `Option<string>` owned flow, construct + choose +
        payload use.
      - `tests/e2e/valid/pair_demo.fln` (+ `.expected`) — multi-payload
        `Pair[A, B]` construct + full destructure.
      - `tests/e2e/valid/nested_pattern.fln` (+ `.expected`) —
        `Ok(Some(x))` / `Some(P(a, b))`.
      - `tests/e2e/valid/or_pattern.fln` (+ `.expected`) — `1 | 2`,
        `Some(v) | Other(v)`.
      - `tests/e2e/valid/type_guard.fln` (+ `.expected`) — `v: int` typed
        binding.
- [x] E2E invalid fixtures (each `.fln` + `.stderr` + `.exit`):
      - `option_needs_args` — bare `Option` in type position.
      - `option_arity` — `Option<int, string>` too many args.
      - `recursive_param_enum` — `enum List<T> { Cons(T, List<T>) }`.
      - `or_pattern_binding_mismatch` — `Some(v) | None`.
      - `type_guard_mismatch` — `v: string` on an `int` scrutinee.
- [x] Confirm std is untouched: no std source changes; existing std tests
      green unchanged.
- [x] Examples: `examples/0.0.7/` — at least one parameterized-enum demo
      (`option_demo` at minimum) with `README.md`.

**Depends on**: T05, T06.
**Refs**: PLAN §9.

**As-built (0.0.7)**:
- **Found a T06 lowering bug (the card's job to surface it); the fix and the
  red-first regression tests landed in the preceding commit `0ac696b`
  (`fix(0.0.7): keep the choose scrutinee parked across a payload mismatch`),
  not in the T09 fixture commit**: a
  payload sub-pattern with a failure edge (nested enum variant, literal,
  nested zero-payload variant) over-popped the `choose`-level scrutinee on
  mismatch, so the next alternative's test block ran `Dup` on an empty stack
  and the verifier rejected the module ("stack depth mismatch: predecessor
  expects 1, got 0"). The root cause: `lower_enum_payloads` unconditionally
  added `+1` to `fail.extra_below` — correct when the parked subject is owned
  by the enclosing match (nested sub-pattern call site: the subject is popped
  on success), wrong at the alternative level where the scrutinee is *shared*
  and must survive. The ownership decision moved to the call sites:
  `lower_enum_alt_match` passes its `fail` unwrapped (`extra_below: 0`),
  `lower_subpattern_match`'s `Enum` case wraps `extra_below + 1`. Regression
  tests (`t09_*_keeps_scrutinee_parked`, written red-first) pin the cleanup
  counts through `assert_stack_depths_consistent`.
- **Fixture-shape notes** (deviations from the card's shorthand):
  - The card's `Some(Pair(a, b))` notation names the *enum*; the actual
    pattern names the *variant* — `Some(P(a, b))` for `enum Pair<A, B> {
    P(A, B) }`.
  - The card's `Some(v) | None` mismatch fixture is
    `or_pattern_binding_mismatch` on `Option<int>`; the card's
    `Some(v) | Other(v)` valid case needs both variants in one enum (an
    or-pattern's alternatives must typecheck against the same scrutinee), so
    `or_pattern.fln` declares `enum Pipe<T> { Some(T), Other(T), Empty }`.
  - On a scalar scrutinee a type-guard arm does not cover (a bare ident
    arm's 0.0.6 behavior), so `type_guard.fln` keeps an `otherwise` arm;
    `o2 = Option.None` needs an annotation (`o2: Option<int>`) — the
    zero-payload variant has nothing to infer `T` from.
  - The Result choose general path and `Result` constructors in argument
    position need annotations (`r: Result<Option<int>, string> = Ok(...)`).
- `pair_demo.fln` exercises the D9 bracket spelling (`Pair[int, string]`)
  at every use site and the Copy split for `Pair[int, int]` vs
  `Pair[int, string]`.
- std untouched confirmed: no commit since the 0.0.6 release bump touches
  `std/` (`git log --oneline -- std/` tops out at the 0.0.6 bump `ff15a2c`,
  and `git diff ff15a2c --stat -- std/` is empty); the `tests/std/` suite
  (run inside `cargo test`) is green unchanged.
- `examples/0.0.7/option_demo.fln` + `README.md`: single-file demo covering
  D1/D2/D4/D5/D6/D7, runnable via `cargo fln examples/0.0.7/option_demo.fln`.

---

## T10 — Examples project + CHANGELOG (F9; D10)

- [x] `examples/0.0.7/tour` — a small multi-module project exercising
      parameterized enums (`Option<T>`, `Pair[A, B]`), nested patterns,
      or-patterns, and type guards; with `tests/enum_param_case.{fln,
      expected}` and `examples/0.0.7/README.md`.
- [x] `CHANGELOG.md` gains the `0.0.7` entry under "Unreleased" (the
      version bump to `0.0.7` is a T11 decision when the release is cut,
      not now). The entry records: parameterized enums (D1/D2/D3),
      pattern enhancements (D4–D7), bytecode v7 (D8), `Result` retention
      (D9), **non-breaking additive** posture (D10).

**Depends on**: T09.
**Refs**: PLAN §10, D9, D10.

**As-built (0.0.7)**:
- The tour declares the parameterized enums in `src/containers.fln`
  (`Option<T>`, `Pair<A, B>`, plus a `Pipe<T>` shaped for or-patterns) and
  drives seven numbered demos from `src/main.fln`. Cross-module notes the
  example records in `examples/0.0.7/README.md`: a type argument still needs
  the module-qualified name (`Result<containers.Option<int>, string>`, a bare
  `Option` there is "unknown type"), and `[int]` arrays are owned, so
  `containers.find(move xs, 15)` transfers.
- `tests/enum_param_case.fln` is standalone by contract — `flint test`
  compiles each `tests/*.fln` as a single file (`fleen_compiler::compile`,
  no project imports), so the fixture re-declares its own `Option<T>`
  (same shape as the 0.0.6 tour's `enum_case`).
- Verified through all three paths: `cargo fln --project
  examples/0.0.7/tour`, `flint run`, and `flint test` (1 passed, 0 failed);
  the full `cargo test` suite stays green (no source changes).
- The CHANGELOG `0.0.7` entry sits at the top of the file above the
  `# Changelog` title (the 0.0.6 entry's position), marked **Unreleased**;
  no `Cargo.toml` / `std/VERSION` changes (the version bump is T11's, when
  the release is cut).

---

## T11 — Docs sync (F9)

- [x] `docs/SYNTAX.ebnf`: `type_params`, `type_args`, recursive `pattern`
      grammar (replaces the 0.0.6 `enum_pattern` / `result_pattern`
      productions; keeps `ResultCtor` as a shortcut documented alongside).
- [x] `docs/BYTECODE.md`: §2 `enum_defs` v7 layout (`type_param_count`
      field); §9 updated layout; §10 v7 row. No §5.x changes (no new
      opcodes).
- [x] `docs/DESIGN.md`: §7 pattern matching section (recursive grammar,
      or-patterns, type guards, multi-payload); §9 enum section gains a
      "Parameterized enums" subsection (D1/D2/D3); §17 feature table gains
      "Parameterized enums" and "Nested / or / type-guard patterns" rows;
      §18 roadmap inserts the 0.0.7 row.
- [x] `SPEC.md` (symlink target for `AGENTS.md` / `CLAUDE.md`): §13 gains
      the 0.0.7 goal paragraph; §14 quick-reference gains a "Parameterized
      types" section; §15 table gains columns for 0.0.7.
- [x] Cross-check: BYTECODE numbers in this release are `MODULE_VERSION =
      7` / `READ_VERSIONS = {1..7}` / `enum_defs` gains one u32 per enum /
      no new opcodes / no `FieldTypeTag` change (D8). Confirm against
      `fleen-compiler/src/codegen/mod.rs` after T07 lands.

**Depends on**: T01–T09 all green.
**Refs**: PLAN §10.

**As-built (T11)**:
- The recursive pattern grammar was **already in `SYNTAX.ebnf`** — T02 had
  updated `pattern` / `or_pattern` / `typed_pattern` / `result_pattern` /
  `enum_pattern` in place when it landed, so this card's remaining EBNF work
  was `type_params` on `enum_decl` and `type_args` on `named_type` (both
  spellings, trailing comma allowed, at least one param/arg — matching
  `parse_enum_type_params` / `parse_optional_type_args` exactly) plus the
  0.0.7 header note.
- `BYTECODE.md` bumped to v0.0.7: the header note, §2 (Module `version`
  comment, `enum_defs` callout, the `EnumDef` struct, the rules), §9 (the
  version paragraph, the header row, the `enum_defs` layout table with the
  version-gated `type_param_count` row, the rules line), and the §10 v7 row.
  The docs state the v7 subtlety explicitly: v7 is the first version to add
  a field **inside** an existing table rather than append a new one — the
  field is written/read only when `version >= 7`, so the v1–v6 byte streams
  are unchanged.
- Cross-check confirmed against the source: `MODULE_VERSION = 7`
  (`codegen/mod.rs:32`), `READ_VERSIONS = [1..7]` (`codegen/flnc.rs:20`),
  the arity u32 written immediately after `variant_count` and gated on
  `version >= 7` on both the writer and reader side (`flnc.rs`), no new
  opcodes, `FieldTypeTag` untouched.
- `DESIGN.md`: §7 rewritten from the 0.0.6 state to the 0.0.7 state (full
  pattern grammar in EBNF form, D4–D7 semantics including the strict
  `ArityMismatchInPattern` rule and payload-aware exhaustiveness); §9.8's
  planning blockquote became a real "Parameterized enums (0.0.7, D1–D3)"
  subsection; the 0.0.6-era "first payload only" wording in the §9.8 example
  and rules was corrected to the 0.0.7 behavior (`when Rect(w, h)`); §17
  gained the 0.0.7 column plus the two new rows; the §17 goal paragraphs and
  the §18 roadmap row moved from "planning" to "implemented".
- `SPEC.md`: §13's 0.0.7 goal paragraph now records T01–T10 implemented +
  T11 docs synced; §14 gained the "Parameterized Types (0.0.7)" quick-ref
  section (with the D9 dual spelling, D2 Copy split, and a scalar
  or-pattern/type-guard example that keeps its `otherwise` arm — a bare
  type-guard arm does not cover on a scalar scrutinee) and the Aggregation
  Types / choose "planning" notes were flipped to implemented; §15 gained
  the 0.0.7 column, the three new feature rows, and a "Status of 0.0.7"
  paragraph.
- No version bump performed (per the T10 as-built note): `Cargo.toml` /
  `std/VERSION` and the release publication remain a separate release-step
  decision.

---

## Dependency graph

```
T01 ──→ T03 ──→ T04 ──→ T05 ──→ T06 ──→ T09 ──→ T10
  │               ↑                │       │      │
  └──→ T02 ───────┘                │       │      │
                                   ↓       ↓      ↓
                                  T07 ──→ T08    T11
                                   └──────┘      ↑
                                    T09 ─────────┘
```

- T01 and T02 are independent and can start in parallel.
- T03 depends on T01; T04 on T01 + T03; T05 on T02 + T04; T06 on T05.
- T07 depends on T04; T08 depends on T07; T09 on T05 + T06.
- T10 depends on T09; T11 depends on T01–T09 all green.
- T11 is the release gate.

---

## Out of scope reminders (from PLAN §8)

No function-level generics, no traits / operator overloading, no closures,
no `map` / `filter` over parameterized enums (monads), no recursive
parameterized enums (`RecursiveEnum` stays), no `Result` → std migration,
no higher-kinded types, no default type parameters, no type-parameter
bounds, no struct parameterization, no struct / array destructuring in
patterns, no guards inside sub-patterns. Any of these needs a new decision
entry in PLAN §1 first.
