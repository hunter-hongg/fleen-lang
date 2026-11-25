# 0.0.10 — `Result` Moves to std + String Interpolation (Tickets)

> Companion to `docs/0.0.10/PLAN.md`.  One card = one unit of work, closed
> with tests.  Decisions D1–D7 referenced here are **settled at planning
> level** (see PLAN §1); a card may reopen one only by adding a new
> decision entry, not by editing it silently.
> **Status: T01 done; T02 done; T03 done; T04 done; T05 done; T06 done; T07 done; T08 done.**
> **Posture reminder**: no bytecode change this milestone (PLAN D7) — the
> version stays v8, no new opcodes, no table changes. Every card that
> touches a shared path keeps non-feature behavior byte-identical; the
> full e2e suite (59 valid fixtures as of T03) is the standing regression net
> and must stay green with unchanged output throughout.
>
> **Regression-test-first rule** (SPEC §7) applies to every behavior
> change: for the two source-visible changes of this milestone — `${`
> gaining meaning inside strings (D4) and `\$` becoming a valid escape —
> the pinning fixtures are written before the lexer change.

---

## T01 — std: the `Result` declaration module (F1; D1)

- [x] `std/result.fln`: declare `pub enum Result<T, E> { Ok(T), Err(E) }`
      with module docs stating the D1 contract (this declaration is the
      language-level source of truth; the compiler maps
      `std.result.Result` onto its internal builtin representation; the
      runtime keeps the 0.0.2 encoding; no `enum_defs` entry is emitted
      for this declaration).
- [x] Compiler recognition: `std.result.Result` is registered as the
      canonical builtin path — the declaration parses through the normal
      0.0.7 parameterized-enum frontend (2 type params, 2 variants:
      `Ok(T)`, `Err(E)`), but std's compilation excludes it from codegen
      (no `enum_defs` entry, no variant codegen — declaration-only, the
      0.0.3 D8 doc-only precedent in spirit).
- [x] The module is importable (`import std.result;` resolves and yields
      the `Ok` / `Err` / `Result` names).
- [x] Tests: std compile green; parser unit test that the declaration
      parses with the expected shape; recorded expectation that std's
      other modules' bytecode is unchanged and no `enum_defs` growth
      occurs anywhere (not a hash gate — 0.0.9 T05 posture).

**As-built notes (T01)**:
- The declaration is spelled `Result<T, E>`, not the card's
  `Result[T, E]`: the declaration-site grammar accepts `<` only
  (`parse_enum_type_params`); `[]` stays the use-site spelling
  (0.0.7 D9).
- The resolver rejects `enum Result` outside `std.result`
  (`ReservedBuiltinName`) — formally T02 shadow-rule territory, landed
  here because the parser widening (the enum-name position now accepts
  the keyword) makes the rejection necessary for verdict parity, and
  PLAN D1 "What this locks" requires it. Recorded here, not silently.
- The "yields the names" item is pinned at the table level
  (`std_result_module_exports_result_with_ok_err_variants`: the export
  carries the `pub` enum with the `Ok` / `Err` variant names and arity
  2); the "no `enum_defs` growth anywhere" expectation is recorded at
  the source level too (`std_declares_no_enums_outside_result`: a
  lexer-level scan — no std module but `std/result.fln` declares an
  enum), alongside the Session-level empty-`enum_defs` assertion.

**Depends on**: nothing.
**Refs**: PLAN §1 D1, §4; `resolver/mod.rs:42` (prelude note);
`codegen/bytecode.rs:186` (`FieldTypeTag::Result`).

---

## T02 — Parser/Resolver: `Ok` / `Err` / `Result` become prelude names (F1; D1/D2)

- [x] **Regression-first**: pin current rejections at the merge-base
      before touching anything — shadowing the ctor names, bare `Ok` in
      expression position, a user enum named `Result` (record the exact
      merge-base error texts; any later text change is recorded in this
      card, never silent).
- [x] Parser: retire the `Ok`/`Err` syntactic special case — `Ok(...)`
      and `Err(...)` parse as ordinary calls of bare identifiers; `when
      Ok(v)` / `when Err(e)` parse as ordinary constructor patterns (bare
      identifier + payload sub-patterns); the `ResultCtor` **AST** nodes
      are deleted (`parser/ast.rs`, `parser/expr.rs`).
- [x] Parser: retire the type-position `Result<` special case —
      `Result<int, string>` (and `Result[T, E]`, 0.0.7 D9 spellings)
      parses as an ordinary parameterized type use.
- [x] Resolver: the prelude gains `Ok` / `Err` (variant-constructor
      names) and `Result` (type name); classification runs through the
      0.0.9 D1 variant-reference machinery and maps back onto the
      existing `ResultCtor` **HIR** nodes (`resolver/hir.rs:478`) — no
      typeck/lower churn.
- [x] Shadow rules: `Ok` / `Err` / `Result` cannot be shadowed (D2);
      bare `Ok` / `Err` in expression position (no call) keeps today's
      rejection (they are constructors, not values).
- [x] Parity sweep: every merge-base-pinned rejection still rejects (same
      verdict; error text may only differ where recorded above).
- [x] Tests: parser unit tests (generic AST shape for ctor calls,
      patterns, and the parameterized type use); resolver unit tests
      (prelude availability with and without `import std.result`, shadow
      rejection, bare-ctor rejection); the pinned fixtures stay red→green
      correctly.

**As-built notes (T02)**:

- **D2's factual premise was wrong, its normative rule is implemented —
  recorded here, never silent.** D2 claims non-shadowable is "the same
  effective semantics as today" and the card listed "shadowing the ctor
  names" among the merge-base *rejections* to pin. Empirical pinning at
  the merge-base (`505f11e`) proved the opposite: `Scope::declare`'s
  §4.3 Python-style builtin overwrite made `Ok` / `Err` **shadowable** —
  `func Ok(x: int)` + call, a local `Ok = 42`, `const Err = "…"`, a
  parameter named `Ok`, and even a pattern binding named `Ok` (`when Ok {
  1 }` on a non-Result scrutinee) all *compiled and ran* (pinned by the
  `check_user_shadows_ok` / `check_local_binding_shadows_ok` /
  `check_param_shadows_ok` typeck tests, which asserted the §4.3 shadow).
  D2's normative bullet ("declaring a binding … named `Ok` / `Err` /
  `Result` is rejected") and this card's shadow-rules item are
  unambiguous, so T02 implements non-shadowable via
  `check_reserved_binding_name` (`ReservedBuiltinName`, reusing the T01
  kind and text) at every declaration site: top-level funcs and curried
  signatures, params, var/const bindings, `for` loop variables, and
  pattern bindings. The flips are pinned: the typeck shadow tests are
  rewritten as rejection tests, `tests/resolver/valid/ok_err_shadow.fln`
  moved to `invalid/`, and the six new e2e invalid pins
  (`reserved_ctor_shadow_{func,binding,const,param}`,
  `bare_ctor_not_a_value`, `enum_result_reserved`) record the verdicts.
  The parity sweep's one-directional constraint holds — every program
  rejected before is still rejected. If the maintainer prefers the old
  shadowable semantics, that is a D2 revision by a new PLAN entry, not a
  code revert.
- **Merge-base error texts pinned** (unchanged after T02 unless noted):
  bare `Ok` in expression position → typeck `undefined variable \`Ok\``
  (unchanged — the resolver keeps emitting `ExprHir::Ident` for the
  prelude constructor names and typeck's name-based interception
  (`infer.rs`) still handles only calls); `enum Result` → resolve
  `` `Result` is a builtin name (declared in `std.result`) and cannot be
  shadowed — choose a different name `` (T01 text, unchanged).
- **`ResultCtor` survival clarified**: the card's "the `ResultCtor` AST
  nodes are deleted" is satisfied by deleting the parser's
  `Pattern::ResultCtor` production (the `ast.rs` variant,
  `parse_result_pattern`, and the `expr.rs` special case). The two-variant
  `ResultCtor` tag enum survives — it is the ctor tag of the existing
  `PatternHir::ResultCtor` / `TypedPatternHir::ResultCtor` /
  `TypedExprHir::ResultCtor` nodes that typeck and lower consume; deleting
  it would have been exactly the typeck/lower churn the card forbids. Its
  doc comment now records the tag role.
- **Expression-position calls needed no parser change**: `Ok(...)` /
  `Err(...)` were *already* ordinary calls at the merge-base (the parser
  special case existed only in patterns); the value side is untouched.
- **Type-position mapping lands in the resolver (T02), not typeck
  (T03)**: the parser's `TokenKind::ResultType` arm still requires the
  two-argument shape (so a bare `Result` and `Result<T>` / `Result<T, E,
  F>` keep the exact pre-0.0.10 parse errors — zero recorded text change
  there) but now emits the ordinary `Type::Named { name: "Result", args:
  [ok, err] }`. Every type position flows through the resolver's
  `resolve_type` (annotations, params, returns, struct fields, enum
  payloads, casts), whose `Named` arm maps the prelude spelling back onto
  the internal `ast::Type::Result` — typeck/lower/codegen see the
  pre-migration shape in all cases, T03's "parameterized type use maps to
  `Type::Result`" item is satisfied by construction and pinned by tests
  (`prelude_result_type_use_maps_to_internal_result`,
  `struct_field_of_result_type_maps_to_internal_result`,
  `prelude_trio_compiles_without_import` with an empty `enum_defs`).
  `Result` stays a lexer keyword (PLAN §5: "Lexer — nothing to remove");
  the dotted `std.result.Result<…>` type spelling remains unparseable
  (parity — it was a parse error at the merge-base; the resolver's
  mapping keys on the bare prelude spelling).
- **Prelude registration**: `register_builtin_result_prelude` seeds the
  builtin declaration (mangled `std.result.Result`) into each module's
  `enum_table` + `variant_to_enum` before `collect_enums` — the resolution
  -time tables T01 kept the declaration in. First-wins seeding keeps a
  user enum's `Ok` / `Err` variants unreachable through the bare spelling,
  exactly the pre-0.0.10 parser-hijack behavior. Pattern classification
  (`Pattern::Enum{name:"Ok"/"Err"}` → `PatternHir::ResultCtor`) runs
  through those tables (`is_builtin_result_enum`), is user-binding-blind
  (defensive; D2 makes such bindings undeclarable), and requires exactly
  one payload.
- **Recorded stage shifts (verdict parity holds, stage/text move)**:
  `Ok(a, b)` (two payloads) was a parse error (`` expected `)` after
  pattern ``), now parses as the generic two-payload constructor pattern
  and is rejected at typeck (`Ok` pattern requires a matching enum
  scrutinee, found `Result[…]`); `Ok()` likewise moved from parse to
  typeck — the fixtures `ok_pattern_empty.fln` /
  `ok_pattern_two_bindings.fln` moved from `tests/parser/invalid/` to
  `tests/typeck/invalid/` with their headers updated; a bare `when Ok { …
  }` arm (a pattern *binding* named `Ok`, legal at the merge-base on a
  non-Result scrutinee) is now a resolve-time `ReservedBuiltinName`
  rejection (`declare_pattern_binding` guard).
- **Bytecode unchanged (D7, recorded expectation — no hash gate)**: the
  diff touches only parser/resolver (+tests); the HIR shapes typeck, lower
  and codegen consume are byte-shape-identical (pinned by the HIR-shape
  tests above), and the standing byte-level pin
  (`fleen-vm/tests/std_byte_identity.rs`) passes unchanged, as do all 54
  e2e valid fixtures' stdout/stderr comparisons.
- **Post-review sweep (recorded)**: the two-axis review of `509eaee`
  surfaced findings, all addressed in the follow-up commit —
  (1) the card's "parser unit tests for ctor calls" item is now fully
  pinned: `parse_ctor_call_is_ordinary_call_expr` asserts `Ok(3)` /
  `Err(msg)` in *expression* position parse as ordinary calls of bare
  identifiers (patterns and the type use already had shape tests);
  (2) the bare-ctor rejection is pinned in `typeck/tests.rs`, not the
  resolver's unit tests — the resolver keeps emitting `ExprHir::Ident` for
  the prelude constructor names, so the verdict fires in typeck's
  name-based interception; the card's "resolver unit tests" wording is
  satisfied by the shadow-rejection tests, placement recorded here;
  (3) process note: the regression-first pins and the change land in one
  squashed commit, so "pin at the merge-base *before touching anything*" is
  order-recorded in these notes rather than archaeologically verifiable
  from history — the pins themselves are verifiable
  (`check_bare_ok_is_not_a_value`, the e2e `bare_ctor_not_a_value` /
  `enum_result_reserved` fixtures); (4) review hygiene: the five per-site
  Session shadow tests collapsed into one data-driven test
  (`reserved_ctor_name_declarations_are_rejected_at_every_site`), the
  shadow-flip narrative lives canonically at `check_reserved_binding_name`
  (code) and in this card (docs) with the call sites and
  `resolver/error.rs` pointing at it, and `Scope::declare` carries a note
  on why the D2 check is per-site (the scope layer has no span for the
  error, SPEC §4, and the resolver's own seeding declares through it
  discarding errors); (5) `ExprFor` gained `var_span`, so the
  `for Ok in …` diagnostic points at the loop variable (pinned by
  `reserved_for_var_error_points_at_the_loop_variable`), and the
  prelude-ctor pattern classification no longer clones the payload —
  output shapes are unaffected (no fixture pins those spans; the
  classification behavior is unchanged).

**Depends on**: T01.
**Refs**: PLAN §1 D1/D2, §3.1; 0.0.9 PLAN D1 (variant-reference
machinery).

---

## T03 — Typeck: type-position mapping + F1 integration matrix (F1; D1/D3)

- [x] Typeck: a parameterized type use resolving to `std.result.Result`
      maps to the internal `Type::Result(ok, err)`; `unify`, `is_copy`
      (argument-dependent), `type_contains_ref`, and the async/channel
      boundary checks see the same `Type::Result` as today — **no rule
      changes**.
- [x] Builtins untouched: `std.arr.get`, `std.str.parse_int` /
      `parse_float` / `index_of`, channel `recv`, and every other
      signature speaking `Type::Result` keep their entries as-is
      (`typeck/builtins.rs`).
- [x] `?` and choose-on-Result: no lowering change (D3) — attempt chains,
      `otherwise`, and guards keep their 0.0.9 semantics; confirm by
      running the 0.0.9 Result-choose fixtures unchanged.
- [x] Error parity sweep: `typeck/error.rs` texts unchanged; all invalid
      fixtures' stderr assertions pass as-is.
- [x] Integration matrix (new fixtures): explicit `import std.result;`;
      prelude trio without import; `Result[T, E]` bracket spelling; a
      struct field of type `Result<int, string>` (parity for
      `FieldTypeTag::Result = 6`); `?` propagation; choose with
      `otherwise`; `await?` flow.
- [x] Full suite green with unchanged stdout/stderr (59 valid fixtures —
      54 standing + 5 new);
      recorded expectation that Result-using programs' bytecode is
      unchanged (no hash gate).

**As-built notes (T03)**:

- **Zero production-code change.** The card's first item was satisfied by
  construction: the type-position mapping landed in the resolver in T02
  (`resolve_type`'s `Named` arm, `resolver/mod.rs`), so typeck / lower /
  codegen see the pre-migration internal `Type::Result` in every type
  position. T03's deliverable is the proof, not a change: two pipeline
  -level typeck tests pin that the prelude spelling feeds the *unchanged*
  rules — `result_prelude_is_copy_argument_dependent` (0.0.7 D2's
  argument-dependent `is_copy`: `Result<int, bool>` re-reads without
  `move`, `Result<int, string>` after a move is the pre-migration
  `UseAfterMove`) and `result_prelude_unify_reports_mapped_type_in_text`
  (`ArgTypeMismatch` names the mapped type — typeck's `Type::name` renders
  the bracket spelling `Result[int, string]`). The async/channel boundary
  side is pinned at e2e level by the `await?` fixture (async func returning
  `Result`, driven through `await?`); the `ref`-rejection side keeps its
  0.0.4 tests. Review follow-up: the mapping into the `type_contains_ref`
  consumer positions is pinned directly at the resolver boundary — T02's
  `prelude_result_type_use_maps_to_internal_result` /
  `struct_field_of_result_type_maps_to_internal_result` cover the
  plain-return and struct-field positions, and the follow-up added
  `async_result_return_maps_to_internal_result` for the async return, so
  all three positions hold the internal `Type::Result`, not a `Named`
  alias.
- **`type_contains_ref` placement recorded**: the card's Refs list it among
  the typeck checks, but the helper lives in the **resolver**
  (`resolver/mod.rs`, the struct-field / return-type guards) — its
  `Type::Result` arm predates the migration, and since the resolver maps
  the prelude spelling onto the internal shape before any consumer, the
  checker sees the same tree as pre-0.0.10. No change; recorded here
  because the placement differs from the card's wording.
- **Integration matrix**: five new e2e valid fixtures —
  `result_prelude_no_import` (trio + `print`, zero imports),
  `result_import_std` (the documented provenance spelling; the type use
  stays the bare `Result` — the dotted `std.result.Result<…>` type spelling
  remains unparseable, T02 parity), `result_bracket_type` (`Result[T, E]`
  in return/param/annotation positions, flowing through angle-spelled
  signatures), `result_struct_field` (the only fixture that reaches the
  `FieldTypeTag::Result = 6` storage path: init from a call, field read
  into a choose, field reassignment), and `await_question` (the first e2e
  fixture for `await?` — previously only exercised by the 0.0.4 tour
  example). The `?` and choose-with-`otherwise` matrix items are covered by
  the standing fixtures (`question.fln`, `result_choose_otherwise*.fln`)
  and were not duplicated.
- **Fixture-authoring parity fact (recorded, not a behavior change)**:
  passing an owned local as a call argument needs the explicit `move` at
  the call site (`unwrap(move a)`, `choose move r`) — the pre-migration
  rule, already visible in the pre-0.0.10 fixtures (`enum_demo.fln`'s
  `describe(move p)`). Field reads deep-copy, so `choose s.r` needs no
  `move` (`result_struct_field`).
- **Recorded bytecode expectation (no hash gate)**: the new Session-level
  pin `result_usage_adds_no_enum_defs_entry` — a Result-using program that
  *also* declares a user enum carries exactly that enum in `enum_defs`
  (complements T01/T02's empty-table pins with the mixed case); the
  version assertion rides on `std_result_import_compiles_with_no_enum_defs`
  and the standing `fleen-vm/tests/std_byte_identity.rs` pin (green,
  unchanged).
- **Gates**: `cargo fmt -- --check`, `cargo clippy --all-targets -- -D
  warnings`, `cargo test` (all targets), `cargo doc --no-deps` — all
  green; all 59 e2e valid fixtures (54 standing + 5 new) pass with their
  expected stdout byte-for-byte, and every invalid fixture's stderr
  assertion passes as-is (`typeck/error.rs` has zero diff since the
  0.0.10 merge-base `0f601be`, verified by `git diff`).

**Depends on**: T02.
**Refs**: PLAN §1 D1/D3, §3.1, §9; `lower/mod.rs:1492` (`PackOk`
emission), `lower/mod.rs:682` (`result_payload_ty`).

---

## T04 — Lexer: string parts + the `\$` escape (F2; D4)

- [x] **Regression-first**: pin today's behavior — a string containing
      `${` is literal characters (e2e fixture with `${` in a string,
      output byte-pinned); `\$` is `invalid_escape` (invalid fixture).
      Both fixtures are written **before** the lexer change; the first
      changes its expectation in the same commit as the lexer work (the
      declared breaking set, D4), the second flips from invalid to valid.
- [x] `\$` becomes the eighth valid escape, producing a literal `$`
      (`lexer_impl.rs` escape match, alongside `\n` / `\t` / `\r` / `\\`
      / `\"` / `\'`).
- [x] `$` is special only immediately before `{`: on `${` inside a string
      literal the lexer switches to expression mode with brace-depth
      tracking until the matching `}`; tokens carry absolute file spans;
      the string literal becomes a parts token (literal chunks +
      embedded-expression token runs). A nested string literal inside an
      embedded expression works (recursive scanning).
- [x] `$` in any other position is an ordinary character (`"a$b"` compiles
      byte-identically to the merge-base pin).
- [x] Errors: unclosed `${` (EOF inside expression mode) is a lexer error
      with a span; unterminated-string handling unchanged.
- [x] Tests: lexer valid (`\$`, `$`-elsewhere, nested braces, nested
      string literal, `$` at end of string via `\$`); lexer invalid
      (unclosed `${` with span; `\` at EOF unchanged).

**As-built notes (T04)**:

- **Token representation**: the parts token is
  `TokenKind::StringInterp(Vec<StringPart>)` with
  `StringPart::{Lit(String), Expr { tokens: Vec<Token>, span: Span }}` —
  the run excludes the `${`/`}` delimiters (they are consumed), each run
  token carries its absolute file span, and `span` covers `${` through the
  matching `}` inclusive so a diagnostic can point at the whole
  interpolation even for an empty run (`"${}"`; the lexer captures the
  empty run, the parser rejects it — T05). **A literal without `${` stays
  a plain `StringLit`** — that is the D6 byte-identity posture carried to
  the token level: the plain path (single const-pool string) is
  structurally untouched, so nothing downstream changes for `${}`-free
  strings. Empty literal chunks are never pushed, so adjacent `Lit` parts
  cannot occur.
- **Regression-first fixtures (order recorded here, one squashed commit —
  the T02 precedent)**: written and verified green at the merge-base
  before the lexer edit — `valid/interp_brace_literal.fln` ran and printed
  the literal `a${b}c`; `invalid/dollar_escape_invalid.fln` failed with
  ``invalid escape sequence '\$' at 329..331``; `valid/dollar_elsewhere.fln`
  printed the literal `$` outputs. In the same commit as the lexer change:
  the `\$` pin **flipped to valid** (`valid/dollar_escape.fln`, expecting
  `100$`); the `${`-literal pin **changed its expectation** — moved to
  `invalid/interp_brace_begins_interp.fln`, pinning the T04-state
  rejection `expected expression, found interpolated string` (the parts
  token reaches the parser's expression entry; the parser wiring is T05).
  T05/T06 flip that program back to valid/ with the interpolated
  expectation and update the needle accordingly. The `$`-elsewhere pin
  stayed put; its **final content** was additionally re-verified
  byte-identical (od -c) at the true merge-base by temporarily reverting
  the four touched source files (patch saved/restored), then re-verified
  identical post-change.
- **Brace-depth tracking is token-level** (`scan_interp_expr` counts
  `LBrace`/`RBrace` tokens), which is what makes the D4 recursion claims
  fall out for free: a nested string literal (itself interpolated) is one
  token in the run — its braces never leak (`interp_nested_string_literal`
  pins `"a${ "x${1}" }b"` end to end). Expression-mode whitespace
  (spaces/tabs/CR) is skipped by the scanner itself — `next_token` skips
  none, that being the `tokenize` loop's job; comments are not recognized
  inside `${}` (out of scope), and a `$` *inside* the embedded expression
  is still `invalid_char` — `$`-specialness is a string-literal rule only.
- **Newline/EOF errors**: `\n` inside the string — literal chunk or
  embedded expression alike — is `UnterminatedString` (a string literal is
  a single-line construct; "unterminated-string handling unchanged");
  EOF inside the expression is the new
  `LexErrorKind::UnclosedInterpolation` with a span covering `${` through
  EOF (`error_unclosed_interpolation_at_eof` pins offsets 1..6 on
  `"${abc`). Invalid-escape spans (backslash..escape-char) and the `\`-at
  -EOF ('\0') path are byte-for-byte the pre-T04 behavior
  (`error_backslash_at_eof_unchanged`); the `tokenize` recovery path is
  untouched.
- **Metadata consistency** (the load-bearing rule the `tokenize_enum_keyword`
  test documents): `TokenKind::StringInterp` has its own `Display` arm
  (renders `interpolated string` — the T04-state parse error reads
  ``expected expression, found interpolated string``) and a `Hash` arm;
  it is **not** a keyword (`is_keyword`/`keyword_str` untouched). ASI: it
  joined `can_start_stmt` and `can_end_stmt` (plus both table tests) so a
  statement beginning/ending with an interpolated string behaves exactly
  like a plain one.
- **Bytecode unchanged (D7, structural argument + standing pins)**: the
  diff touches only the lexer, token metadata and ASI tables; every
  `${}`-free program lexes to the identical token stream it produced at
  the merge-base, so codegen output is unchanged by construction; the
  standing `fleen-vm/tests/std_byte_identity.rs` pin and all e2e
  stdout/stderr comparisons pass unchanged. Gates: `cargo fmt -- --check`,
  `cargo clippy --all-targets -- -D warnings`, `cargo test` (all targets;
  the e2e valid set is now 61 fixtures — 59 standing + the two new pins,
  `dollar_escape` and `dollar_elsewhere`; `interp_brace_literal` existed
  only as an uncommitted merge-base pin and now lives in invalid/),
  `cargo doc --no-deps` — all green.

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

**As-built note (0.0.10 T05/T06 review)**: D6's concat-chain lowering lands
on the function-body path (`lower_expr_into`); the flat global-initializer
path (`lower_global_init`) has **no** `Interp` arm, so a global
`const s = "x${i}";` falls into the catch-all and is rejected as
`ComplexGlobalInit` ("global initializer must be a simple expression"). This
is a recorded decision, not an oversight: global initializers are the
"simple expression only" posture (the same catch-all rejects if/while/
choose blocks), and D6/D7 never direct global-init support. The lowering
error for a missing `std.str.concat` on the body path is a dedicated
`InterpConcatMissing` invariant-break kind (mirrors the `std.arr.push`
idiom — reported, not panicked), not the misleading `ComplexGlobalInit` the
first draft used. Extending global init to an Interp arm is a new PLAN
decision (it would have to mirror the merge + concat-chain sequence on a
frameless path); out of scope here.

---

## T07 — Docs sync (F3)

- [x] `docs/DESIGN.md`: §2 (string literal syntax — parts, `\$`, the
      `${` rule); §8 (Error Handling — `Result` is declared in std, D1
      contract summarized); §9 (string type + interpolation; §9.8
      cross-reference); §11 (the prelude rule gains the recorded
      `Ok`/`Err`/`Result` exception, D2); §17 feature-matrix rows;
      §18 roadmap row.
- [x] `docs/SYNTAX.ebnf`: the string-literal rule gains the interpolation
      production and the `\$` escape.
- [x] `SPEC.md`: the 0.0.10 goal paragraph + feature-matrix row(s) +
      quick-reference section for interpolation.
- [x] `docs/BYTECODE.md`: untouched (D7) — record the confirmation here.
- [x] `CHANGELOG.md`: `[0.0.10]` entries — Added (interpolation,
      `import std.result`, `\$`), Changed (`${` in strings now begins
      interpolation — the milestone's breaking set; `Result` declared in
      std, language-level only, backend identical).

**Depends on**: T01–T06.
**Refs**: PLAN §1 D1–D7, §3.

### As-built notes

**`docs/BYTECODE.md` untouched (D7) — confirmation**: the file was last
modified by 0.0.9 T06 (commit `902bb4f`) and was **not touched** by any
0.0.10 commit (verified via `git log -- docs/BYTECODE.md`). The
`Result`→std migration is language-level only: the backend opcodes
(`PackOk`/`PackErr`/`IsErr`/`UnwrapOk`/`UnwrapErr`, 0xA0–0xA4),
`FieldTypeTag::Result = 6`, and the VM `Value::Ok`/`Value::Err` all stay;
no `enum_defs` entry is emitted (`std/result.fln` is declaration-only);
string interpolation lowers to `std.str.concat` with zero new opcodes
(D6). The version stays **v8**.

**Doc drift corrected** (three "now-wrong" claims found in DESIGN.md,
all corrected by T07):

1. **§8 Error Handling**: "users may shadow `Ok`/`Err`" → corrected to
   *non-shadowable* (`ReservedBuiltinName`, D2/T02 behavior change).
2. **§9.8 enum**: "`Result[T, E]` stays builtin; migration deferred to
   0.1.0" → corrected: migration happened in 0.0.10; backend untouched.
3. **§11 prelude**: "the prelude is `print` only" → amended to include the
   D2 trio exception (`Ok`/`Err`/`Result`, non-shadowable).

**New content written**:

- `docs/DESIGN.md`: §2 string interpolation callout (D4/D5/D6 summary);
  §8 "0.0.2 semantics" amended to note D1/D2; §9 primitive-types note
  referencing §9.9; §9.8 corrected + 0.0.10 migration paragraph; **§9.9**
  new subsection (string interpolation D4–D6 — syntax, typing, lowering,
  global-init limit, breaking set); §11 prelude amended (D2); §17 feature
  matrix — 0.0.10 column added, interpolation row, Result cell descriptive;
  §18 roadmap row.
- `docs/SYNTAX.ebnf`: header comment block gained a 0.0.10 note; `string_lit`
  split into `string_char` + `escape` with an interpolation production
  (`"$" "{" expr "}"`); `escape` now includes `"$"` (the `\$` escape).
- `SPEC.md`: §13 goal paragraph for 0.0.10; §14 "String Interpolation (0.0.10)"
  subsection + status note on Result section; §15 feature matrix — 0.0.10
  column, interpolation row, Result/Bytecode-v8 cells made descriptive.
- `CHANGELOG.md`: `[0.0.10]` entry with Added / Changed / Fixed sections
  (Fixed: none — no regressions found).

---

## T08 — Examples, version bump, release gate (F3)

- [x] `examples/0.0.10/`: a Result-in-std demo (the declaration's
      provenance, `?` / choose-`otherwise` flows, `import std.result`)
      + an interpolation demo (every D5-convertible type, `\$`,
      copy semantics, nested expressions).
- [x] `CHANGELOG.md` finalized; `Cargo.toml` workspace version →
      `0.0.10`; `std/VERSION` → `0.0.10`.
- [x] Full gate suite: `cargo fmt -- --check`, `cargo clippy -- -D
      warnings`, `cargo test`, `cargo doc --no-deps`.
- [ ] Release follows the 0.0.7–0.0.9 pipeline; codename settled here
      (holder's call). Per the established cadence (L9): after the local
      commit + tag, **stop and report** for the user to push — the push
      is what starts the Release workflow.

**As-built note (codename settled at the gate)**: the holder chose **Loom
Spider 🕸️** — recorded in the PLAN header, the CHANGELOG `[0.0.10]` header,
`examples/0.0.10/README.md`, and the README version line; release notes at
`release/0.0.10/RELEASE_NOTES.md` (mirrors 0.0.7–0.0.9). The release box stays
unchecked until the `v0.0.10` tag is pushed (L9: the tag push is what starts
the Release workflow). The T07 review fixes (the FFI/Generics/Operator
matrix cells, the DESIGN `${`→`$` typo, the §17 T-status) were folded in
pre-release as editorial corrections within this same milestone.

**Depends on**: T07.
**Refs**: PLAN §10.
