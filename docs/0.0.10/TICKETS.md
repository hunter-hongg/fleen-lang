# 0.0.10 — `Result` Moves to std + String Interpolation (Tickets)

> Companion to `docs/0.0.10/PLAN.md`.  One card = one unit of work, closed
> with tests.  Decisions D1–D7 referenced here are **settled at planning
> level** (see PLAN §1); a card may reopen one only by adding a new
> decision entry, not by editing it silently.
> **Status: planning (T01–T08 all open).**
> **Posture reminder**: no bytecode change this milestone (PLAN D7) — the
> version stays v8, no new opcodes, no table changes. Every card that
> touches a shared path keeps non-feature behavior byte-identical; the
> full e2e suite (108 valid fixtures) is the standing regression net and
> must stay green with unchanged output throughout.
>
> **Regression-test-first rule** (SPEC §7) applies to every behavior
> change: for the two source-visible changes of this milestone — `${`
> gaining meaning inside strings (D4) and `\$` becoming a valid escape —
> the pinning fixtures are written before the lexer change.

---

## T01 — std: the `Result` declaration module (F1; D1)

- [ ] `std/result.fln`: declare `pub enum Result[T, E] { Ok(T), Err(E) }`
      with module docs stating the D1 contract (this declaration is the
      language-level source of truth; the compiler maps
      `std.result.Result` onto its internal builtin representation; the
      runtime keeps the 0.0.2 encoding; no `enum_defs` entry is emitted
      for this declaration).
- [ ] Compiler recognition: `std.result.Result` is registered as the
      canonical builtin path — the declaration parses through the normal
      0.0.7 parameterized-enum frontend (2 type params, 2 variants:
      `Ok(T)`, `Err(E)`), but std's compilation excludes it from codegen
      (no `enum_defs` entry, no variant codegen — declaration-only, the
      0.0.3 D8 doc-only precedent in spirit).
- [ ] The module is importable (`import std.result;` resolves and yields
      the `Ok` / `Err` / `Result` names).
- [ ] Tests: std compile green; parser unit test that the declaration
      parses with the expected shape; recorded expectation that std's
      other modules' bytecode is unchanged and no `enum_defs` growth
      occurs anywhere (not a hash gate — 0.0.9 T05 posture).

**Depends on**: nothing.
**Refs**: PLAN §1 D1, §4; `resolver/mod.rs:42` (prelude note);
`codegen/bytecode.rs:186` (`FieldTypeTag::Result`).

---

## T02 — Parser/Resolver: `Ok` / `Err` / `Result` become prelude names (F1; D1/D2)

- [ ] **Regression-first**: pin current rejections at the merge-base
      before touching anything — shadowing the ctor names, bare `Ok` in
      expression position, a user enum named `Result` (record the exact
      merge-base error texts; any later text change is recorded in this
      card, never silent).
- [ ] Parser: retire the `Ok`/`Err` syntactic special case — `Ok(...)`
      and `Err(...)` parse as ordinary calls of bare identifiers; `when
      Ok(v)` / `when Err(e)` parse as ordinary constructor patterns (bare
      identifier + payload sub-patterns); the `ResultCtor` **AST** nodes
      are deleted (`parser/ast.rs`, `parser/expr.rs`).
- [ ] Parser: retire the type-position `Result<` special case —
      `Result<int, string>` (and `Result[T, E]`, 0.0.7 D9 spellings)
      parses as an ordinary parameterized type use.
- [ ] Resolver: the prelude gains `Ok` / `Err` (variant-constructor
      names) and `Result` (type name); classification runs through the
      0.0.9 D1 variant-reference machinery and maps back onto the
      existing `ResultCtor` **HIR** nodes (`resolver/hir.rs:478`) — no
      typeck/lower churn.
- [ ] Shadow rules: `Ok` / `Err` / `Result` cannot be shadowed (D2);
      bare `Ok` / `Err` in expression position (no call) keeps today's
      rejection (they are constructors, not values).
- [ ] Parity sweep: every merge-base-pinned rejection still rejects (same
      verdict; error text may only differ where recorded above).
- [ ] Tests: parser unit tests (generic AST shape for ctor calls,
      patterns, and the parameterized type use); resolver unit tests
      (prelude availability with and without `import std.result`, shadow
      rejection, bare-ctor rejection); the pinned fixtures stay red→green
      correctly.

**Depends on**: T01.
**Refs**: PLAN §1 D1/D2, §3.1; 0.0.9 PLAN D1 (variant-reference
machinery).

---

## T03 — Typeck: type-position mapping + F1 integration matrix (F1; D1/D3)

- [ ] Typeck: a parameterized type use resolving to `std.result.Result`
      maps to the internal `Type::Result(ok, err)`; `unify`, `is_copy`
      (argument-dependent), `type_contains_ref`, and the async/channel
      boundary checks see the same `Type::Result` as today — **no rule
      changes**.
- [ ] Builtins untouched: `std.arr.get`, `std.str.parse_int` /
      `parse_float` / `index_of`, channel `recv`, and every other
      signature speaking `Type::Result` keep their entries as-is
      (`typeck/builtins.rs`).
- [ ] `?` and choose-on-Result: no lowering change (D3) — attempt chains,
      `otherwise`, and guards keep their 0.0.9 semantics; confirm by
      running the 0.0.9 Result-choose fixtures unchanged.
- [ ] Error parity sweep: `typeck/error.rs` texts unchanged; all invalid
      fixtures' stderr assertions pass as-is.
- [ ] Integration matrix (new fixtures): explicit `import std.result;`;
      prelude trio without import; `Result[T, E]` bracket spelling; a
      struct field of type `Result<int, string>` (parity for
      `FieldTypeTag::Result = 6`); `?` propagation; choose with
      `otherwise`; `await?` flow.
- [ ] Full suite green with unchanged stdout/stderr (108 valid fixtures);
      recorded expectation that Result-using programs' bytecode is
      unchanged (no hash gate).

**Depends on**: T02.
**Refs**: PLAN §1 D1/D3, §3.1, §9; `lower/mod.rs:1492` (`PackOk`
emission), `lower/mod.rs:682` (`result_payload_ty`).

---

## T04 — Lexer: string parts + the `\$` escape (F2; D4)

- [ ] **Regression-first**: pin today's behavior — a string containing
      `${` is literal characters (e2e fixture with `${` in a string,
      output byte-pinned); `\$` is `invalid_escape` (invalid fixture).
      Both fixtures are written **before** the lexer change; the first
      changes its expectation in the same commit as the lexer work (the
      declared breaking set, D4), the second flips from invalid to valid.
- [ ] `\$` becomes the eighth valid escape, producing a literal `$`
      (`lexer_impl.rs` escape match, alongside `\n` / `\t` / `\r` / `\\`
      / `\"` / `\'`).
- [ ] `$` is special only immediately before `{`: on `${` inside a string
      literal the lexer switches to expression mode with brace-depth
      tracking until the matching `}`; tokens carry absolute file spans;
      the string literal becomes a parts token (literal chunks +
      embedded-expression token runs). A nested string literal inside an
      embedded expression works (recursive scanning).
- [ ] `$` in any other position is an ordinary character (`"a$b"` compiles
      byte-identically to the merge-base pin).
- [ ] Errors: unclosed `${` (EOF inside expression mode) is a lexer error
      with a span; unterminated-string handling unchanged.
- [ ] Tests: lexer valid (`\$`, `$`-elsewhere, nested braces, nested
      string literal, `$` at end of string via `\$`); lexer invalid
      (unclosed `${` with span; `\` at EOF unchanged).

**Depends on**: nothing (parallel to T01–T03).
**Refs**: PLAN §1 D4, §3.2; `lexer_impl.rs:190` (escape table).

---

## T05 — Parser/Typeck: interpolation node + the conversion rule (F2; D5)

- [ ] Parser: new interpolation expression node — parts are literal
      chunks and embedded expressions; embedded expressions parse with the
      ordinary expression grammar (full expressions, D4).
- [ ] Resolver: embedded expressions resolve in the enclosing scope
      (ordinary name resolution — no new rules).
- [ ] Typeck: each embedded expression must be stringly — `int` /
      `float` / `bool` implicitly stringify via exactly the `as string`
      conversions; `string` embeds verbatim; every other type (`enum`,
      `struct`, `[T]`, `box<T>`, `Result`, `Future<T>`, `channel<T>`,
      `ptr<T>`, function values) is `InterpUnsupportedType` (error-naming
      follows the existing style) with a hint naming the explicit
      conversion (D5).
- [ ] No format specifiers (D5): `${x:.2}` is a parse/type error, not a
      feature.
- [ ] Tests: parser unit tests (parts shape, nested interpolation,
      expression-embedding spans); typeck unit + integration tests (the
      full per-type accept/reject matrix, hint text, error span covers
      the embedded expression).

**Depends on**: T04.
**Refs**: PLAN §1 D5, §3.2; `typeck/infer.rs:5319` (the `as string` cast
rule it reuses).

---

## T06 — Lower: concat-chain desugar + F2 e2e matrix (F2; D6)

- [ ] Lower: desugar interpolation to `std.str.concat` chains over
      **merged literal chunks** (adjacent literal parts become one
      string); scalar embeds lower through the existing `as string` path
      (`ToStr`, BYTECODE.md v0.0.2 row 0xA5); string embeds are
      deep-copied (`DupDeep`) — interpolation never moves a user variable
      (D5 read-context rule).
- [ ] Evaluation order: left-to-right, each part evaluated exactly once
      (the concat chain's order locks this — observable via side-effecting
      embedded calls, D6).
- [ ] Zero new opcodes; a literal without `${` compiles **byte-identically**
      to the merge-base (single const-pool string); modules stay v7 unless
      a `PartialApply` was encoded (content-driven rule, 0.0.8 D13/D14).
- [ ] e2e valid: every D5-convertible type; embedded calls / field access;
      a string embed **reused after** interpolation (copy proof);
      interpolation inside `print` arguments and inside std calls;
      adjacent-literal merging; `\$` and `$`-elsewhere output pins.
- [ ] e2e invalid: enum embed, struct embed, array embed, `Result` embed
      (each `InterpUnsupportedType` + hint); format-specifier rejection.
- [ ] Byte gate: recorded byte-identity expectation on a few
      `${}`-free fixtures against the merge-base (not a hash gate); no
      new fixture reaches v8 unless it encodes a partial.

**Depends on**: T05.
**Refs**: PLAN §1 D5/D6, §3.2, §9; BYTECODE.md §5.11 / v0.0.2 row
(`ToStr` 0xA5).

---

## T07 — Docs sync (F3)

- [ ] `docs/DESIGN.md`: §2 (string literal syntax — parts, `\$`, the
      `${` rule); §8 (Error Handling — `Result` is declared in std, D1
      contract summarized); §9 (string type + interpolation; §9.8
      cross-reference); §11 (the prelude rule gains the recorded
      `Ok`/`Err`/`Result` exception, D2); §17 feature-matrix rows;
      §18 roadmap row.
- [ ] `docs/SYNTAX.ebnf`: the string-literal rule gains the interpolation
      production and the `\$` escape.
- [ ] `SPEC.md`: the 0.0.10 goal paragraph + feature-matrix row(s) +
      quick-reference section for interpolation.
- [ ] `docs/BYTECODE.md`: untouched (D7) — record the confirmation here.
- [ ] `CHANGELOG.md`: `[0.0.10]` entries — Added (interpolation,
      `import std.result`, `\$`), Changed (`${` in strings now begins
      interpolation — the milestone's breaking set; `Result` declared in
      std, language-level only, backend identical).

**Depends on**: T01–T06.
**Refs**: PLAN §1 D1–D7, §3.

---

## T08 — Examples, version bump, release gate (F3)

- [ ] `examples/0.0.10/`: a Result-in-std demo (the declaration's
      provenance, `?` / choose-`otherwise` flows, `import std.result`)
      + an interpolation demo (every D5-convertible type, `\$`,
      copy semantics, nested expressions).
- [ ] `CHANGELOG.md` finalized; `Cargo.toml` workspace version →
      `0.0.10`; `std/VERSION` → `0.0.10`.
- [ ] Full gate suite: `cargo fmt -- --check`, `cargo clippy -- -D
      warnings`, `cargo test`, `cargo doc --no-deps`.
- [ ] Release follows the 0.0.7–0.0.9 pipeline; codename settled here
      (holder's call). Per the established cadence (L9): after the local
      commit + tag, **stop and report** for the user to push — the push
      is what starts the Release workflow.

**Depends on**: T07.
**Refs**: PLAN §10.
