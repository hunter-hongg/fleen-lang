# 0.0.8 — Per-File Feature Flags + Function Currying & Partial Application (Tickets)

> Companion to `docs/0.0.8/PLAN.md`.  One card = one unit of work, closed
> with tests.  Decisions D1–D16 referenced here are **settled at planning
> level** (see PLAN §1); a card may reopen one only by adding a new decision
> entry, not by editing it silently.
> **Status: T01–T04 implemented and green (parser-level for T01–T03; T02's
> entry-wiring verification done in T04; T03's typeck rendering wiring carried
> into T05); T05–T11 open.**
> **Posture reminder**: this release must stay non-breaking additive (PLAN
> D16).  Every card that touches a shared path (parser, typeck, lower,
> codegen) must keep flag-off behavior byte-identical; the e2e suite runs
> both modes (T09) and is the gate that proves it.

---

## T01 — Lexer + parser: `#feature` magic statement + per-file FeatureSet (F1; D1)

- [ ] Lexer: new `Hash` token, unconditional. Comments (`//`, `/* */`)
      unchanged; `#` never starts a comment. Lex errors unchanged.
- [ ] Parser: `feature_directive = "#" , "feature" , ident` accepted only
      in the file header region — before the first item (imports count as
      items; comments/blank lines do not end the region). Directive lines
      take no semicolon, are newline-terminated.
- [ ] Errors: `FeatureDirectiveMisplaced` (any `Hash` after the first
      item, in item position or inside a block body — the block-statement
      parser rejects `#` like it rejects `pub`), `UnknownFeature` (hard
      error), `DuplicateFeature` (same feature twice in one file).
- [ ] Session: parse results record the file's feature set; the compiler
      config grows a `FileId → FeatureSet` table (`Span.file` exists —
      0.0.3 V01). The set is consulted by parser/typeck/lower in later
      cards; `curry` is the only registered feature name in 0.0.8.
- [ ] The directive is unconditional grammar — recognized in every file,
      flag-on or flag-off (it *is* the flag carrier).
- [ ] Tests: parser valid (directive first, after comments/blank lines,
      two distinct directives — blocked in 0.0.8: only `curry` registers, so
      a second *distinct* name is `UnknownFeature`; the case is covered once
      a second feature registers); invalid (after `import`, mid-file, inside
      a function body, unknown name, duplicate name, directive joined on a
      line with another statement, explicit `;` on a directive line).

**Depends on**: nothing.
**Refs**: PLAN §3.2, D1.

---

## T02 — Parser: two-part curried declarations, strict per file (F2; D2/D3)

- [x] New AST nodes: `Decl::CurriedSignature { name, modifiers
      (async/unsafe/trusted), ty (the arrow type), is_pub, span }` and
      `Decl::CurriedBody { name, params: Vec<String>, body, span }`.
      (Implemented shape: body params are `Vec<CurriedParam>` — name + span,
      so `DuplicateParam` can point at the second occurrence; the three
      modifiers are `is_async`/`is_unsafe`/`is_trusted` fields.)
- [x] `curried_sig` production: `[pub] [modifier] ident : arrow_type` —
      recognized only when the file's FeatureSet contains `curry`
      (T01's table). `curried_body`: `ident ( params ) ( block | = expr )`
      — recognized at top level by the `Ident`-followed-by-`(` lookahead
      (top level has no call statements — unambiguous).
- [x] Strictness: in a `curry` file, the legacy `func`-keyword form is a
      parse error `FuncFormForbiddenInCurryFile`. In flag-off files the
      two-part productions do not exist (bare `name : type` with no `=`
      remains today's syntax error; `func` remains the only form).
- [x] Signature line is single-line only (multi-line arrow chain hits the
      normal ASI trigger → syntax error, no carve-out). Body may be a
      block or `= expr`.
- [x] Reject at parse or resolve (card's choice, report which):
      `pub` on the body line; a modifier on the body line; duplicate body
      parameter names. `SignatureNeedsFunctionType` when a bare top-level
      `name : type` in a flag-on file has a non-function type.
      **Reported choice: all three are rejected at parse** —
      `PubOnCurriedBody`, `ModifierOnCurriedBody`, `DuplicateParam`.
- [ ] Entry point: `main` in a flag-on file uses the two-part form; verify
      the project-mode entry wiring still finds it.
      **Status**: done (the *wiring* half closed by T04 — see that card): the
      two-part-`main` parsing half landed here (test
      `curried_main_uses_two_part_form`, fixture `curry_two_part.fln`); the
      project-mode entry discovery needed no change once T04 resolved curried
      declarations into ordinary functions, and the e2e project fixture
      `curry_main` pins it end-to-end.
- [x] Tests: parser valid (0/1/2-param signatures, `= expr` body, block
      body, `async`/`unsafe`/`trusted` on the signature, `pub` on the
      signature, `main` two-part); invalid (`func` in flag-on file, two-part
      form in flag-off file, `pub`/modifier on body, multi-line signature,
      keyword as body parameter name).

**Depends on**: T01.
**Refs**: PLAN §3.2, D2, D3.

---

## T03 — Parser: curried arrow function types in type positions (F3; D4)

- [x] `arrow_type = func_atom , "->" , arrow_type` — right-associative,
      ≥ 1 arrow, accepted in type positions (signature lines, parameter
      types, return types, binding annotations) only in flag-on files.
      `int -> int -> int` parses as `int -> (int -> int)`; both
      canonicalize to `Type::Func([int, int], int)`.
      **Implementation**: `parse_type` ends in `finish_arrow_chain` (the
      flag-gated chain tail; `parse_type` is the shared type-position
      parser, so every type position — signature lines, param types,
      return types, binding annotations, type guards, type args — accepts
      the chain; `->` has no expression-level meaning, so this is
      ambiguity-free). Canonicalization is `merge_arrow_return`: the
      return-side `Func` merges into the param chain only when it carries
      ≥ 1 parameters — a zero-parameter return function is a genuine
      function value and stays nested (`int -> () -> int` is unary over
      zero-arg functions), and an empty head never absorbs the return side
      (`() -> unit -> int` stays a zero-arg function returning a unary
      function). Param-side functions never merge (`(int -> int) -> int`
      stays higher-order). The `Future<T>` constructor branch runs the
      chain tail too (`Future<int> -> int`). The legacy `(`-branch applies
      the same merge **only in flag-on files** — flag-off keeps the
      pre-0.0.8 nesting byte-identical (D16).
- [x] Zero-parameter form: `() -> int` — the `()` atom immediately
      followed by `->` is the empty parameter list; `unit -> int` is a
      unary function over `unit`. Grouping parens: `(int -> int) -> int`.
      (Ret-side grouping parens stay reserved — a `(` group still demands
      its `->`, per the no-carve-out posture; the chain spelling is the way
      to write `int -> () -> int`.)
- [x] Legacy tuple spelling `(int, int) -> int` stays legal in flag-on
      files (sugar, same canonical type — D4); in flag-off files the arrow
      chain is not a type production (a stray `->` remains today's parse
      error). In a flag-on file the sugar's function-typed *return* side
      merges too — `(int, int) -> (int) -> int` ≡ `(int, int) -> int -> int`
      ≡ `Type::Func([int, int, int], int)` (D4: two spellings, one
      canonical form); in flag-off files the legacy nesting is unchanged
      (`(int) -> (int) -> int` stays a 1-arg function returning a 1-arg
      function).
- [x] Diagnostics: type rendering picks the spelling per the *file's* flag
      — flag-on renders `int -> int -> int`, flag-off keeps the tuple
      spelling (the pre-0.0.8 `Display` text — note `Display` emits a
      trailing comma: `(int, int,) -> int`; `render(false)` is
      byte-identical to it, which is what the posture requires).
      Error-text fixtures are per-mode (T09).
      **Implementation**: `ast::Type::render(curry)` and
      `typeck::Type::name_with(curry)` (flag-off byte-identical to
      `Display` / `name()`); the parser's `SignatureNeedsFunctionType`
      already renders in the file's spelling. The typeck call-site wiring
      (consulting the per-file feature table when building diagnostics)
      lands with T05, whose card references this renderer.
- [x] Tests: parser valid (1/2/3-deep chains, right-assoc, `() -> int`,
      `unit -> int`, grouped arrows, tuple sugar alongside); invalid
      (orphan `->`, `->` in a flag-off file's type position, empty chain,
      `()` not followed by `->` used as unit where a parameter list is
      required).
      Unit tests: chain flatten/right-assoc, sugar ≡ chain, zero-param,
      `unit ->`, grouped param nested, zero-param return nested,
      zero-param head not absorbing the return chain, `Future<int> ->
      int`, arrow in a binding annotation, flag-off stray `->`, orphan
      `->`, empty chain tail (`ExpectedSemiOrNewStmt` — no ASI carve-out,
      D3), bare `()`, render spellings (parser + typeck), flag-on legacy
      sugar merges its function-typed return side, flag-off legacy nesting
      unchanged (D16 regression). Fixtures:
      `curry_arrow_types.fln` (valid), `curry_arrow_{flagoff,orphan,
      empty_chain,unit_parens}.fln` + `.err` (invalid).

**Depends on**: T01.
**Refs**: PLAN §3.2, D4.

---

## T04 — Resolver: signature/body matching + registration (F2; D3)

- [x] Carried from T02: verify the project-mode entry wiring finds a
      two-part `main` in a flag-on file end-to-end (T02 could only pin the
      parsing half — resolve still rejects curried declarations there).
      **Implementation**: no wiring change needed — a matched pair resolves
      into an ordinary `FuncDeclHir` (name `main` included), so codegen's
      `funcs.find(name == "main")` finds it unchanged; the e2e project
      fixture `curry_main` runs a two-part `main` end-to-end
      (`project_curry_two_part_main_runs`).
- [x] Signatures register in the function table exactly as `func`
      declarations do — name, canonical type, modifiers, visibility —
      before bodies resolve (recursion and forward reference unchanged).
      **Implementation**: `predeclare_global_functions` also walks
      `CurriedSignature` items (binding + mangled global name +
      `global_funcs` entry), keeping the signature (with its item index) in
      `pending_sigs`. A matched pair is synthesized into a `FuncDecl`
      (types/ret/modifiers/`pub` from the signature; names/body from the
      body) and resolved through the unchanged `resolve_func_decl` path —
      typeck/lower/codegen see an ordinary function (the D13 v7-module
      property falls out of this).
- [x] Match bodies to signatures by name within the module. New errors:
      `BodyWithoutSignature` (body line with no preceding signature —
      order is enforced, D3), `SignatureWithoutBody`, `SignatureArityMismatch`
      (arrow count ≠ body parameter count). **Implementation**: matching is
      order-sensitive via the signature's item index (`sig_index <
      body_index`), so registration stays global while the source order D3
      demands still holds. Two disambiguations: a *second* body for an
      already-consumed signature is `DuplicateBinding` (flat-namespace rule),
      and an arity-mismatched pair consumes its signature (no orphan-halves
      double report). `CurriedDeclNotYetResolved` (the pre-T04 guard) is
      removed; the `resolve_decl` arms became `unreachable!` (the parser
      accepts the two-part form at top level only, same shape as
      struct/enum).
- [x] Cross-module: export/import tables carry the signature (type +
      modifiers + visibility) exactly as `func` exports do. The importing
      file's flag never affects what it sees (D6). **Implementation**:
      `exports_of_ast` maps `CurriedSignature` → `ExportKind::Func` (the
      body half contributes no entry); the import path is the existing
      `Func` machinery, untouched — the resolver stays flag-blind (D6).
- [x] Tests: resolver valid (recursion in flag-on files, forward body
      reference, cross-module import of a curried declaration by a
      flag-off file); invalid (orphan signature, orphan body, arity
      mismatch, duplicate names, body-before-signature). Inline:
      two-part synthesis shape (param names/types/ret travel), `= expr`
      body + signature modifiers/`pub`, zero-param/`unit ->`, recursion,
      forward reference, cross-module import both halves (mangled export
      name, flag-off importer), the five error kinds, body-vs-global-var
      name clash. Fixtures:
      `curry_{recursion,zero_param,forward_reference}.fln` (valid),
      `curry_{orphan_signature,orphan_body,arity_mismatch,
      duplicate_signature,duplicate_body,body_before_signature}.fln`
      (invalid).
      **Coverage note (post-review)**: the inline cross-module test
      hand-builds the export table, so it only exercises the *import* half;
      the export half (`exports_of_ast` on a flag-on module) is pinned by
      the e2e project fixture `curry_import` — a flag-off `main` imports a
      `pub` curried declaration from a flag-on `lib` and runs it through the
      real `--project` pipeline (`project_curry_import_cross_module_runs`).

**Depends on**: T02, T03.
**Refs**: PLAN §3.4, D2, D3, D6.

---

## T05 — Typeck: implicit partial application + participation + anti-typo guard (F4–F7; D5–D11)

- [ ] Call-site rule (D5): a call to a participating callee with
      `k < n` arguments produces `Type::Func(remaining, ret)`; the typed
      call node records `partial: true` and the captured argument types.
      `k = 0` is the degenerate partial; `k > n` stays `ArityMismatch`.
- [ ] Participation check (D5/D7/D9): callee must be fixed-arity, non-
      variadic (`BuiltinParam::Variadic` excluded), without `ref`/`ptr`
      parameters, without `unsafe`/`trusted` modifiers; otherwise `k < n`
      keeps today's `ArityMismatch`. The check is call-site local — the
      callee's file flag is irrelevant.
- [ ] Argument ownership is the full-call rule set (unchanged
      `reject_bare_owned_arg` path; owned locals need `move`/`clone`).
- [ ] Anti-typo guard (D10): an expression statement whose top-level typed
      expression carries `partial: true` → `PartialApplicationDiscarded`.
      A genuine unary function value as a statement stays legal (marker
      absent). Full calls as statements unchanged.
- [ ] Copy semantics (D11): `is_copy(Type::Func)` stays `true`; no
      type-level change. No coercion: a partial does not satisfy a
      full-arity `Func` parameter and vice versa (D6).
- [ ] `async` (D12): async callees participate; a partial has type
      `(rem) -> Future<T>`; the future exists only at the final call. No
      new boundary rule.
- [ ] Diagnostic rendering: per-file spelling (T03's table consulted).
- [ ] Tests: typeck valid (partial of a 2-param user function, partial of
      a std function, degenerate `k = 0`, async partial, partial flowing
      into a flag-off higher-order helper and called there); invalid
      (too-few-args in flag-off file unchanged, `k > n`, partial of
      `print`, partial of a `ref`-param/`ptr`-param/`unsafe` function,
      discarded partial, owned arg without `move`).

**Depends on**: T04.
**Refs**: PLAN §3.3, D5, D6, D7, D9, D10, D11, D12.

---

## T06 — Lower: partial call reroute + inline-lowering exemption (F5; D8/D13)

- [ ] A partial call (direct callee or function value) lowers to
      `LoadFunc` (direct callee) / the callee expression (indirect) + the
      `k` arguments + `PartialApply k`. Full-arity calls lower exactly as
      today (`Call` / `CallValue`) — byte-identical for flag-off paths.
- [ ] Inline-lowering reroute (D8): `range` / `range_from` (`MakeRange`)
      and the `std.collection.iter` helpers (`InlineIter`) take their
      inline path only on full-arity calls; a partial call emits the
      generic sequence above. `BuiltinLowering::Intrinsic` variadics never
      reach a partial call (typeck rejects, T05).
- [ ] MIR: new instruction kind `PartialApply(u8)` with the documented
      stack effect; `MirFunc` tables unchanged (no per-arity synthesis —
      the runtime captures, unlike variadic builtins which instantiate per
      argc).
- [ ] Tests: lower shapes for direct/indirect partial calls; `range_from(0)`
      does not fold to `MakeRange`; full-arity `range_from(a, b)` still
      folds; snapshot tests that flag-off modules emit zero new
      instructions.

**Depends on**: T05.
**Refs**: PLAN §5, D8, D13.

---

## T07 — Bytecode v8: `PartialApply` (0xCE) + codegen + verify (F8; D13/D14)

- [ ] `Opcode::PartialApply = 0xCE` — first slot of the reserved
      `0xCE–0xDF` group (BYTECODE.md §4.2; enum took the group head
      `0xCB–0xCD` the same way). Operand `u8 k`; encoder/decoder updated.
- [ ] Content-driven version (D13): writers emit `MODULE_VERSION = 8`
      **iff** at least one `PartialApply` was emitted, else 7 — a
      `#feature curry` file without partial calls produces a v7 module.
      `READ_VERSIONS = {1..8}`.
- [ ] verify: accepts v8; validates the `PartialApply` stack shape (pops
      `k+1`, pushes 1); v1–v7 validation unchanged. Old verify rejecting
      v8 is the expected, documented behavior.
- [ ] BYTECODE.md: §4.2 slot note, §5.18 entry, §10 versioning row (docs
      text lands here; the T11 card only cross-checks).
- [ ] Tests: v8 write/read round-trip; flag-on-without-partials → v7
      (byte-for-byte vs 0.0.7 output); v1–v7 fixtures decode unchanged;
      hand-written v8 module with `PartialApply` runs in the VM (T08's
      gate in reverse).

**Depends on**: T06.
**Refs**: PLAN §7, D13, D14.

---

## T08 — VM: captured function values (F9; D11/D15)

- [ ] `Value::Func` grows the capture list: `Func { id: FuncId, captures:
      Vec<Value> }`; bare function = empty list. Update every
      `Value::Func` match site: `PayloadKind` mapping, `Display`
      (`<func #N>` unchanged for bare; captured renders the same — the
      capture list is not user-visible), promote/move helpers.
- [ ] `PartialApply k`: pops `f` and `k` arguments, pushes the captured
      function value.
- [ ] `CallValue`: consumes `captures.len() + argc`; total must equal the
      callee's `params` (today vm.rs:730 requires `argc == params`). Bare
      functions behave exactly as today. `Call` (direct) never sees
      captures.
- [ ] Copy paths deep-copy a non-empty capture list (D11): `Dup`, array
      element store, struct field write, global store, any other
      Copy-consumption site for `Value::Func`. Soundness property to test:
      two copies of a partial, called separately, consume their own
      snapshots.
- [ ] The VM is flag-blind — no feature plumbing, unconditional support
      (D15). No serialization of captures (D14): no constant-pool /
      `FieldTypeTag` surface.
- [ ] Tests: VM unit tests for `PartialApply`/`CallValue` with captures;
      deep-copy soundness (copy → call both); bare-function regression
      (every existing function-value test passes untouched); boundary
      (partial crossing an await boundary travels as a value).

**Depends on**: T07.
**Refs**: PLAN §6, D11, D14, D15.

---

## T09 — Integration: e2e matrix both modes + cross-flag import (F10)

- [ ] E2E valid: `curry_demo.fln` (flag-on file: two-part declarations,
      partial application, std imports used at full arity) +
      `curry_demo.expected`; `partial_flow.fln` (a partial produced in a
      flag-on file flows through flag-off code and is called there) +
      expected.
- [ ] E2E invalid: flag-off file — too-few-args `ArityMismatch`
      (unchanged), `#` mid-file (`FeatureDirectiveMisplaced`); flag-on
      file — `FuncFormForbiddenInCurryFile`, `PartialApplicationDiscarded`,
      partial of an `unsafe` function, `UnknownFeature`. Each with
      `.exit` + `.stderr` per mode.
- [ ] The harness runs the matrix in **both modes**: every pre-0.0.8
      fixture passes byte-identically (the non-breaking gate, D16);
      flag-on fixtures are new files carrying `#feature curry`.
- [ ] std untouched confirmation: std compiles flag-off in every mode; its
      bytecode contribution is byte-identical to 0.0.7 (assert via module
      hash or table comparison in a test).
- [ ] Cross-flag import both directions exercised end-to-end: flag-off
      imports flag-on (full-arity call; too-few-args error preserved),
      flag-on imports flag-off std and partially applies `range_from(0)`
      (reroute exercised end-to-end).
- [ ] Bytecode version assertions: a flag-on project without partial
      calls ships v7; one partial call lifts the merged module to v8
      (per-module reality, D13).

**Depends on**: T05, T06, T08.
**Refs**: PLAN §9, D16.

---

## T10 — Examples + CHANGELOG (F10)

- [ ] `examples/0.0.8/` — a single-file `curry_demo.fln` (two-part
      declarations, partial application, `int -> int` flowing through
      higher-order use) runnable with `cargo fln`, plus a multi-file
      project demonstrating cross-flag import (flag-on main importing
      flag-off helpers) if the 0.0.7 tour layout is followed.
- [ ] CHANGELOG gains the `0.0.8` entry: per-file `#feature` directive,
      curried declarations, curried arrow types, implicit partial
      application, participation rules, `PartialApply`/v8 (content-driven),
      non-breaking additive posture (D16).
- [ ] All examples pass `cargo test` and `cargo fln` runs.

**Depends on**: T09.
**Refs**: PLAN §10, D16.

---

## T11 — Docs sync (F11)

- [ ] SPEC.md: §13 gains the 0.0.8 goal paragraph (per-file feature flags +
      currying, status line) and the feature-matrix rows; §14 gains the
      curried-declaration and partial-application examples (flag-gated
      snippets).
- [ ] DESIGN.md: §14 (language reference) gains the two-part declaration
      and curried type notation under the flag; the ownership section
      gains D11's capture semantics; §18 roadmap inserts the 0.0.8 row.
- [ ] SYNTAX.ebnf: `feature_directive`, `curried_sig`, `curried_body`,
      `arrow_type` productions with the header-region note.
- [ ] BYTECODE.md: confirm §4.2 slot note, §5.18 `PartialApply`, §10 v8
      row landed in T07 (this card cross-checks, does not duplicate).
- [ ] Version bump if not already done by T10's CHANGELOG pass:
      `Cargo.toml` → `0.0.8`, `std/VERSION` → `0.0.8`.
- [ ] All four build checks green: `cargo fmt --check`,
      `cargo clippy -- -D warnings`, `cargo test`, `cargo doc --no-deps`.

**Depends on**: T01–T10 all green (release gate).
**Refs**: PLAN §10.

---

## Dependency graph

```
T01 ──→ T02 ──→ T04 ──→ T05 ──→ T06 ──→ T07 ──→ T08 ──→ T09 ──→ T10
  │                ↑              │                        │       │
  └──→ T03 ────────┘              └────────────────────────┘       ↓
                                                                  T11
```

- T01 is the root: the FeatureSet table everything else consults.
- T02 and T03 are independent of each other, both depend on T01.
- T04 depends on T02 + T03; T05 on T04; T06 on T05; T07 on T06; T08
  on T07; T09 on T05 + T06 + T08; T10 on T09; T11 on T01–T10 all green.
- T11 is the release gate.

---

## Out of scope reminders (from PLAN §8)

No closures, no explicit partial-application syntax (`partial(f, 1)` /
`f(1, _)`), no project-level flags in `flint.toml`, no currying of
variadic builtins, no partial application of `ref`/`ptr`/`unsafe`
functions (permanent exemption, D9), no representational currying (the
calling convention is untouched, D4), no second feature beyond `curry`,
no comma-list directives, no per-file `.flnc` outputs, no function-level
generics / traits / operator overloading (0.1.0), no recursive enums, no
`Result` → std migration. Any of these needs a new decision entry in PLAN
§1 first.
