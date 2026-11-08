# 0.0.7 — Parameterized Types + Pattern Matching Completion (Tickets)

> Companion to `docs/0.0.7/PLAN.md`.  One card = one unit of work, closed with
> tests.  Decisions D1–D10 referenced here are **settled at planning level**
> (see PLAN §1); a card may reopen one only by adding a new decision entry,
> not by editing it silently.
> **Status: planning. No ticket started.**

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

- [ ] `EnumDeclHir` gains `type_param_count: usize` (replacing nothing —
      monomorphic enums keep 0).
- [ ] Type parameters register in a **declaration-local scope** during
      resolution; they are not visible outside the enum body and are not
      values.
- [ ] Enum names in the module export table now carry arity;
      `ExportDef::enum_variants` unchanged (variant names do not change with
      parameterization); `register_imported_items` skips enums as before.
- [ ] Cross-module: importing a parameterized enum carries the arity;
      use-site type applications at the import site are checked against the
      declared arity at typeck (T04).
- [ ] Collision rules unchanged: variant names still enter the per-enum
      namespace; type-parameter names do **not** enter the binding scope.
- [ ] Tests: resolver valid (import / pub of parameterized enums, dotted
      constructor paths); invalid (private parameterized enum cross-module,
      name collisions).

**Depends on**: T01.
**Refs**: PLAN §5 (Resolve row), D1.

---

## T04 — Typeck: parameterized type resolution + fully-applied-use + argument-dependent `is_copy` (F1/F2; D1/D2)

- [ ] `Type` gains `App(Box<EnumInfo>, Vec<Type>)` — a fully-applied
      parameterized enum type. The `EnumInfo` inside carries the
      declaration; the `Vec<Type>` carries the substituted arguments.
      Monomorphic enums keep `Type::Enum(EnumInfo)` (arity 0, no `App`
      wrapper).
- [ ] `type_param_count` on `EnumInfo` is checked at every use-site type
      application: too few → `TypeConstructorNeedsArgs { name, expected,
      found, span }`; too many → `ArityMismatch` (reuse).
- [ ] Parameter substitution: inside a declaration body, parameters are
      resolved to `Type::Param(usize)` (a new variant for declaration-local
      type parameters — distinct from `Type::Var(u32)` which is for
      schematic builtin signatures). At use sites, `Param(i)` in payloads is
      replaced by the corresponding argument.
- [ ] **Argument-dependent `is_copy` (D2)**: `is_copy(Type::App(info, args))`
      substitutes args into the declaration's payload types and re-runs the
      struct-mirror rule. `is_copy(Type::Enum(info))` (monomorphic) is the
      arity-0 case and behaves exactly as 0.0.6.
- [ ] Constructor typing: `Option.Some(1)` — the argument's type fixes
      `T = int`; result is `App(Option, [int])`. When the parameter does not
      appear in the chosen variant's payloads (rare — `enum Weird<T> {
      Empty(T), Nothing }` and the arm is `Weird.Nothing`), the surrounding
      annotation must fix it; otherwise `TypeParameterAmbiguous { name,
      span }`.
- [ ] Storage rules: `ptr<T>` payloads rejected (unchanged);
      `Future` / `channel` payloads inherit boundary rules (unchanged).
      Parameterized payloads inherit the same rules via substitution.
- [ ] **`RecursiveEnum` carries forward (D3)**: the cycle check now walks
      through parameterized payloads too — `enum List<T> { Cons(T, List<T>)
      }` is rejected with the full path.
- [ ] Tests: typeck valid (`Option<int>` Copy flows free; `Option<string>`
      needs move / clone; `Pair[string, int]` owned; nested `Option<Pair[A,
      B]>`); invalid (bare `Option` as type, `Option<int, string>` too many
      args, `List<T>` recursive, `ptr` payload, ambiguous empty-payload
      parameter).

**Depends on**: T01, T03.
**Refs**: PLAN §3.3, D1, D2, D3.

---

## T05 — Typeck: recursive pattern typecheck + exhaustiveness (F3–F7; D4–D7)

- [ ] Pattern typecheck walks the scrutinee type alongside the pattern
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
- [ ] Bindings from all nesting levels and all or-alternatives (after
      consistency check) are declared in the arm scope with their
      substituted types. Duplicate binding name across levels →
      `DuplicateBinding`.
- [ ] **Exhaustiveness (F7)**: generalize the 0.0.6 T05 algorithm to handle
      or-patterns — an or-pattern contributes the **union** of its
      alternatives' variant coverage. Multi-binding arms contribute the
      variant they name (same as 0.0.6). Guard-free arms covering all
      variants are exhaustive; `ChooseNotExhaustive` names the missing
      variants (uncovered variants listed by name).
- [ ] Tests: typeck valid (nested `Ok(Some(x))`, multi-binding `Pair(a,
      b)`, or-pattern with same binding `Some(v) | Other(v)`, type guard on
      ident binding); invalid (nested constructor mismatch, arity mismatch
      in pattern, or-pattern with mismatched bindings, or-pattern with
      mismatched types for the same binding, type guard on wrong type,
      exhaustiveness missing a variant, or-pattern not covering the full
      set).

**Depends on**: T02, T04.
**Refs**: PLAN §3.4, D4–D7.

---

## T06 — Lower: multi-binding arms, nested-pattern chains, or-pattern alternative tests (F3–F6; D4–D6)

- [ ] **Multi-binding arms (D4)**: a `Variant(p₁, …, pₙ)` arm lowers as
      the 0.0.6 T05 chain (park scrutinee, `Dup; EnumIs v; JumpIfTrue
      body`), and the body block opens with `EnumPayload v i; StoreLocal
      xᵢ` for each binding `xᵢ` extracted from `pᵢ` (only for sub-patterns
      that are `Ident` bindings or that contain bindings — a literal or
      wildcard sub-pattern skips the store).
- [ ] **Nested-pattern chains (D5)**: for `Ok(Some(x))` (or any nesting),
      the outer test succeeds and the outer payload is extracted and
      **re-parked**; the inner pattern becomes a new `EnumIs` / `EnumPayload`
      chain on the re-parked value. The chain terminates when the innermost
      pattern is a binding or a literal. Guard arms stage payloads as in
      0.0.6 T05 (move out of a `Dup` copy before the guard; jump to the
      next test on failure). Every edge is resolved immediately (no
      deferred jumps).
- [ ] **Or-pattern alternative tests (D6)**: a `p₁ | p₂` arm lowers as
      **multiple** test chains — one per alternative — all jumping to the
      same body block on success. Each alternative independently extracts
      its bindings before the jump (they are the same names with the same
      types; only one alternative's extraction runs at runtime). The body
      block is the shared successor of all alternative test blocks.
- [ ] **Type guards lower as their inner pattern (D7)** — no runtime work;
      the ascription is compile-time only. `when v: int { … }` lowers as
      `when v { … }` (a binding).
- [ ] Stack-depth validation: the `assert_stack_depths_consistent`
      validator (introduced in 0.0.6 T05 review) must accept the new arm
      shapes — or-pattern chains, nested chains, multi-binding bodies.
- [ ] Tests: lower unit tests on hand-checked fixtures (multi-binding arm
      shape; nested chain for a 2-level pattern; or-pattern with two
      alternatives; guard inside a nested pattern).

**Depends on**: T05.
**Refs**: PLAN §3.4, D4, D5, D6, D7.

---

## T07 — Bytecode v7: `enum_defs` arity field + codegen + verify (F8; D8)

- [ ] `enum_defs` v7 layout gains `type_param_count: u32` per enum, written
      immediately after `variant_count`. The parameter **names** are not
      serialized (declaration-local; no runtime meaning).
- [ ] `MODULE_VERSION = 7`; `READ_VERSIONS = [1, 2, 3, 4, 5, 6, 7]`.
      v1–v6 fixtures are byte-identical (append-only rule respected: one
      new u32 per enum in `enum_defs`, no new opcodes, no `FieldTypeTag`
      change).
- [ ] verify: `enum_defs` v7 reader consumes the new field; no new
      cross-checks beyond that (the arity field is descriptive, not a
      runtime contract).
- [ ] Regenerate the v6-compat fixture that carries `enum_defs` (the 0.0.6
      T07 regeneration procedure: `cargo test -p fleen-vm --test compat
      regenerate_fixtures -- --ignored`); golden-encoding tests that build
      their own modules need no change.
- [ ] Tests: v7 write / read round-trip (parameterized and monomorphic
      enums); v1–v6 fixture decode unchanged; truncated `enum_defs`
      rejected.

**Depends on**: T04 (needs arity to emit).
**Refs**: PLAN §7, D8.

---

## T08 — VM: no changes (confirmation card) (F8; D8)

- [ ] Confirm no VM source changes are needed: `Value::Enum` shape
      unchanged; `EnumNew` / `EnumIs` / `EnumPayload` encodings and stack
      effects unchanged; defensive errors unchanged.
- [ ] Confirm the v7 `.flnc` is decoded by the existing v6 VM reader with
      the one extra u32 per enum skipped (or validated) in the `enum_defs`
      reader.
- [ ] Test: existing VM enum tests green unchanged; a v7 module with a
      parameterized enum runs correctly through the full toolchain.

**Depends on**: T07.
**Refs**: PLAN §6, D8.

---

## T09 — Integration: e2e fixtures + std untouched confirmation (F9; D9)

- [ ] E2E valid fixtures:
      - `tests/e2e/valid/option_demo.fln` (+ `.expected`) — `Option<int>`
        Copy flow, `Option<string>` owned flow, construct + choose +
        payload use.
      - `tests/e2e/valid/pair_demo.fln` (+ `.expected`) — multi-payload
        `Pair[A, B]` construct + full destructure.
      - `tests/e2e/valid/nested_pattern.fln` (+ `.expected`) —
        `Ok(Some(x))` / `Some(Pair(a, b))`.
      - `tests/e2e/valid/or_pattern.fln` (+ `.expected`) — `1 | 2`,
        `Some(v) | Other(v)`.
      - `tests/e2e/valid/type_guard.fln` (+ `.expected`) — `v: int` typed
        binding.
- [ ] E2E invalid fixtures (each `.fln` + `.stderr` + `.exit`):
      - `option_needs_args` — bare `Option` in type position.
      - `option_arity` — `Option<int, string>` too many args.
      - `recursive_param_enum` — `enum List<T> { Cons(T, List<T>) }`.
      - `or_pattern_binding_mismatch` — `Some(v) | None`.
      - `type_guard_mismatch` — `v: string` on an `int` scrutinee.
- [ ] Confirm std is untouched: no std source changes; existing std tests
      green unchanged.
- [ ] Examples: `examples/0.0.7/` — at least one parameterized-enum demo
      (`option_demo` at minimum) with `README.md`.

**Depends on**: T05, T06.
**Refs**: PLAN §9.

---

## T10 — Examples project + CHANGELOG (F9; D10)

- [ ] `examples/0.0.7/tour` — a small multi-module project exercising
      parameterized enums (`Option<T>`, `Pair[A, B]`), nested patterns,
      or-patterns, and type guards; with `tests/enum_param_case.{fln,
      expected}` and `examples/0.0.7/README.md`.
- [ ] `CHANGELOG.md` gains the `0.0.7` entry under "Unreleased" (the
      version bump to `0.0.7` is a T11 decision when the release is cut,
      not now). The entry records: parameterized enums (D1/D2/D3),
      pattern enhancements (D4–D7), bytecode v7 (D8), `Result` retention
      (D9), **non-breaking additive** posture (D10).

**Depends on**: T09.
**Refs**: PLAN §10, D9, D10.

---

## T11 — Docs sync (F9)

- [ ] `docs/SYNTAX.ebnf`: `type_params`, `type_args`, recursive `pattern`
      grammar (replaces the 0.0.6 `enum_pattern` / `result_pattern`
      productions; keeps `ResultCtor` as a shortcut documented alongside).
- [ ] `docs/BYTECODE.md`: §2 `enum_defs` v7 layout (`type_param_count`
      field); §9 updated layout; §10 v7 row. No §5.x changes (no new
      opcodes).
- [ ] `docs/DESIGN.md`: §7 pattern matching section (recursive grammar,
      or-patterns, type guards, multi-payload); §9 enum section gains a
      "Parameterized enums" subsection (D1/D2/D3); §17 feature table gains
      "Parameterized enums" and "Nested / or / type-guard patterns" rows;
      §18 roadmap inserts the 0.0.7 row.
- [ ] `SPEC.md` (symlink target for `AGENTS.md` / `CLAUDE.md`): §13 gains
      the 0.0.7 goal paragraph; §14 quick-reference gains a "Parameterized
      types" section; §15 table gains columns for 0.0.7.
- [ ] Cross-check: BYTECODE numbers in this release are `MODULE_VERSION =
      7` / `READ_VERSIONS = {1..7}` / `enum_defs` gains one u32 per enum /
      no new opcodes / no `FieldTypeTag` change (D8). Confirm against
      `fleen-compiler/src/codegen/mod.rs` after T07 lands.

**Depends on**: T01–T09 all green.
**Refs**: PLAN §10.

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
