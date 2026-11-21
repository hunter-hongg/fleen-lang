# 0.0.9 — Small Repairs: Pattern Fix, Result-`choose` Completion, `break`/`continue`, std.str Batch (Tickets)

> Companion to `docs/0.0.9/PLAN.md`.  One card = one unit of work, closed
> with tests.  Decisions D1–D6 referenced here are **settled at planning
> level** (see PLAN §1); a card may reopen one only by adding a new decision
> entry, not by editing it silently.
> **Status: T01–T07 implemented and green (T07: example + CHANGELOG +
> version bump + the full gate suite). Release published — the `v0.0.9`
> tag and the six target archives are cut; `release/0.0.9/` mirrors the
> 0.0.8 layout.**
> **Posture reminder**: no bytecode change this milestone (PLAN D6) — the
> version stays v8, no new opcodes, no table changes. Every card that
> touches a shared path keeps flag-off behavior unchanged; the 0.0.8
> both-mode e2e gate stays green throughout.
>
> **Planning-time corrections this milestone owns** (verified against git
> history and live compiles, not memory):
> - BUG-1 (`docs/0.0.7/BUGS.md`) is already fixed — commit `a5b8bd7`, inside
>   the `v0.0.8` tag; fixture `tests/e2e/valid/choose_arm_if_else.fln` green.
> - The 0.0.8 captured-builtin "known limitation" is already fixed — commit
>   `8dbbc8b`, inside the `v0.0.8` tag; `range_from(0)` partials run e2e.
> - Result-`choose` guards already lower via 0.0.7's attempt chain;
>   `when Ok(x) if x > 3 { … }` compiles and runs today.
> T06 is the docs-sync card that records all three.

---

## T01 — Resolver: zero-payload variant patterns are variant references (F1; D1)

- [x] Resolver: over each `choose` pattern subtree, classify a bare
      identifier pattern as a **variant reference** iff it names a
      zero-payload variant of an enum reachable in scope and no binding of
      that name was registered earlier in the same pattern (lookup order
      PLAN D1: same-pattern earlier bindings → enclosing locals →
      zero-payload variant → unresolved). The parser keeps emitting bare
      identifiers; classification is resolver work.
- [x] The classified reference reuses the existing `EnumVariant` typed
      pattern path (zero bindings, discriminant-check lowering unchanged);
      `collect_pattern_bindings_into` already returns nothing for it.
- [x] Errors: `DuplicateBinding` still fires for repeated **binding** names
      (`when P(x, x)` with a local `x`); unresolved bare names unchanged;
      non-variant repeated bare names (`when P(1, 1)` literal arms) unchanged.
- [x] Exhaustiveness unchanged: a zero-payload variant arm covers exactly
      its variant; or-pattern alternatives `None | Some(0)` compose.
- [x] Tests: resolver unit tests (variant-vs-binding precedence, shadow
      order); e2e valid — the BUG-2 repro verbatim (`when P(None, None)` +
      `Some`/`None` mix, mirrors `examples/0.0.7/pair_demo.fln` case 5 with
      the `otherwise` workaround removed); e2e invalid — `DuplicateBinding`
      on repeated bindings preserved.
- [x] Regression-test-first per SPEC §7 (BUGS.md's recorded fix rule).

**As-built note (exhaustiveness)**: the resolver classification alone still
rejects the card's own acceptance fixture — the repro's four complementary
arms cover `P` only *jointly*, and 0.0.7's per-arm rule
(`pattern_covers_variant` per arm) requires each arm to be individually
total, so `when P(None, None)` + the `Some`/`None` mix was
`ChooseNotExhaustive` as soon as `DuplicateBinding` stopped firing (the
path was unreachable before, which is why 0.0.7 never saw it). Recorded as
decision **D7** (the composition reopens D1's "no change to the coverage
algebra" sentence, per the reopen-by-new-entry rule): the 0.0.7 per-arm
rule stays first (`pattern_covers_variant` unchanged), and
`check_exhaustiveness` gains a second, additive disjunct —
`variant_arms_jointly_total` composes the guard-free arms matching one
variant over a coverage **matrix**: each `Enum` arm of the variant
contributes its payload sub-patterns as a row (`collect_variant_rows`,
through or-patterns/type-guards; zero-payload `EnumVariant` rows are
empty), and `pattern_rows_cover` splits the rows
constructor-by-constructor across the positions, recursing on the rows
that survive each specialization. Strictly additive: every program
exhaustive under the 0.0.7 rule stays exhaustive; zero-payload variants
keep their outright `EnumVariant` coverage.

**Soundness note (corrected from the first draft)**: the draft composed
arms per payload *position* and asked `patterns_are_total` per pooled
group, claiming "position-wise totality means no payload combination falls
through". That claim is false — position-wise totality ignores the
correlation between positions: `P(Some, None)` + `P(None, Some)` pools
total per position while `P(Some, Some)` matches neither arm and silently
falls through (compiles at the draft, rejected at the merge-base); the
same pooling hid a `Result`-payload hole (`P(Ok(a), x)` + `P(Err(e), 5)`)
and a scalar-head hole (`P(x, 1)` + `P(2, y)`). The splitting engine
replaced it: each specialization keeps only rows matching *all* values of
the split constructor, so the surviving rows stay correlated with the
tuples they matched, and per-position totality is never used as a verdict.

Tests: `typeck_integration.rs`
`complementary_zero_payload_arms_are_exhaustive` (positive) +
`joint_coverage_with_a_gap_is_rejected` (single-position negative) +
`cross_position_pattern_gap_is_rejected` /
`result_payload_position_gap_is_rejected` /
`binding_head_does_not_cover_other_arms_tuples` (soundness negatives) +
`or_pattern_alternatives_compose_in_joint_coverage` (D6 inside the
matrix); resolver-level T01 tests in `resolver_integration.rs` (incl.
`resolve_literal_payload_arms_bind_no_names` for the literal-arms
checklist item); e2e `tests/e2e/valid/pair_zero_payload_variants.fln`
(repro verbatim, written first and red) +
`tests/e2e/invalid/choose_payload_cross_position_gap.fln` (soundness pin)
+ `tests/e2e/invalid/pattern_repeated_binding.fln` (`DuplicateBinding`
preserved).

**Depends on**: nothing.
**Refs**: PLAN §1 D1, §3.1; `docs/0.0.7/BUGS.md` BUG-2.

---

## T02 — Lower: Result-`choose` accepts `otherwise` / general arms (F2; D2)

- [x] Typeck: an `otherwise` arm on a Result scrutinee makes the choose
      exhaustive (same coverage rule as scalar scrutinees); a guarded arm
      still does not count toward coverage (§10.5 rule unchanged).
- [x] Lower: delete the `UnsupportedFeature "non-Ok/Err arm … deferred to
      U07"` branch in `lower_choose_result`; route non-ResultCtor arms into
      the attempt-chain structure — `otherwise` occupies the defensive-tail
      slot (its body replaces the pop+unit tail; the tail's Pop of the
      parked payload still happens before the body runs).
- [x] `ChooseResultNeedsOkErrArms` retired: remove the variant and its
      diagnostics; exhaustive-choose errors now come from typeck
      (`ChooseNotExhaustive`) for Result scrutinees as for every other type.
- [x] Guard arms: no lowering change (already work); add the tests that
      document the behavior (`when Ok(x) if g { … }` + unguarded duplicate
      arm; + `otherwise`).
- [x] Tests: e2e valid — guard + `otherwise`; multiple Ok arms + Err arm +
      `otherwise`; nested payload `when Ok(Some(y))` + `otherwise`; e2e
      invalid — Result choose missing both coverage paths (typeck error);
      lower unit tests for the tail-slot arm shape (payload popped once).

**As-built note (fallback shape)**: the two attempt chains share **one**
fallback body block — each chain's tail emits its `Pop` (the parked
payload) and jumps to it, so the body is emitted exactly once and always
runs at the function's base stack depth (both tails pop before jumping;
the block is entered only via explicit edges, so the BUG-1 fallthrough
spill rule adds nothing new). The simple 0.0.6/U06 fast path is preserved
for exactly its original shape, now gated on `fallback.is_none()` and
both chains non-empty — an empty chain takes the general path, where the
chain's single attempt block *is* the tail (`Err`-arm + `otherwise` and
bare-`otherwise` chooses lower and run). **First-wins on duplicate
always-matching arms**: when several guard-free wildcard arms (`when _`)
are present, the partition keeps the **first** — later duplicates are dead
arms whose body is never emitted — matching the scalar path's sequential
first-match semantics (the first partition draft was last-wins, which
silently overwrote the first arm's body and disagreed with the scalar
scrutinee path; pinned by `t02_result_choose_first_wildcard_arm_wins`).

**As-built note (or-arms)**: deleting the rejection branch wholesale would
have *silently dropped* top-level or-pattern arms (`when Ok(1) | Ok(2) { … }`
passes typeck — the payload patterns join the side's coverage group), so
they are routed per D2's "route non-ResultCtor arms": the arm expands into
one attempt per alternative sharing the arm's guard and body (a mixed-ctor
or-arm contributes to both chains; only one chain can run for a given
value). Typeck already unified the alternatives' binding ids onto the
first alternative's (D6 first-wins), so every attempt stores the same
slot. Any other non-ResultCtor, non-or, non-wildcard arm shape keeps a
defensive `UnsupportedFeature`. Literals and enum patterns are
typeck-rejected upstream, but bare bindings and guarded wildcard/binding
forms (`when w { … }`, `when _ if g { … }`) pass typeck when an
`otherwise` makes the choose exhaustive and reach lower, which rejects
them there (as in HEAD; routing a binding arm would need a decision on
how the bound name receives the scrutinee value at the popped-payload
fallback — not made by this ticket). The generic (non-Result) choose's
ResultCtor pattern site becomes the same defensive error now that the
dedicated variant is gone.

Tests: `typeck_integration.rs` `result_choose_otherwise_is_exhaustive` +
`result_choose_guarded_arm_without_otherwise_is_not_exhaustive` +
`result_choose_err_arm_with_otherwise_is_exhaustive`; lower unit tests
`t02_result_choose_otherwise_occupies_tail_slot` (shared block, one body,
both tails Pop-then-jump) + `t02_result_choose_empty_ok_chain_lowers_with_
otherwise` + `t02_result_choose_otherwise_only_lowers` +
`t02_result_choose_or_arm_routes_into_attempt_chain` +
`t02_result_choose_guard_with_unguarded_duplicate_lowers` +
`t02_result_choose_otherwise_after_ok_err_is_not_dropped` (the U06
"rejected not dropped" pin flipped: the body is emitted once, never
dropped) + `t02_result_choose_first_wildcard_arm_wins` (duplicate `when _`
arms: the first wins, matching scalar) + the stale `u06_choose_result_*`
comments updated; e2e valid
`tests/e2e/valid/result_choose_otherwise.fln` (guard-fail → otherwise,
multi-Ok + Err + otherwise, nested payload, empty ok/err chains, all
runtime-checked) and e2e invalid `result_choose_missing_ok_and_err.fln`
(`ChooseNotExhaustive` missing both sides); the U06 invalid fixture
`choose_result_otherwise.*` (which pinned the old rejection) deleted —
its shape is now a valid-program case covered by the e2e valid fixture.

**Depends on**: nothing.
**Refs**: PLAN §1 D2, §3.2; DESIGN.md §10.5 (T06 corrects its deferral text).

---

## T03 — Frontend: `break` / `continue` reserved words + loop-context checks (F3; D4)

- [x] Lexer: `break` / `continue` keywords (two new token kinds); reserved
      word lists updated (source breaking change of 0.0.9, D4).
- [x] Parser: `break_stmt` / `continue_stmt` keyword statements (ASI
      handles the trailing semicolon; they take no value).
- [x] Typeck: loop-context tracking — legal inside `while` bodies and `for`
      bodies (both array and Range skeletons, including inline-iter expanded
      loops); `BreakOutsideLoop` / `ContinueOutsideLoop` at top level, in
      plain function bodies without a lexically enclosing loop, inside
      `choose` arm bodies, and across any function/`async` boundary (D4's
      lexical containment rule); `x = break` / break-as-expression rejected
      (statements, not expressions).
- [x] Errors: the two new variants with actionable spans + help text (the
      lexical-containment rule — which boundaries do not count).
- [x] Tests: typeck unit tests per position above; parser tests (statement
      shape, ASI interaction); e2e invalid set per PLAN §9.

**As-built note (ASI membership)**: the keywords joined **both**
`can_start_stmt` *and* `can_end_stmt`. The end-set membership is
load-bearing and easy to miss: the parser has no newline information (ASI
consumed it), so without `break` in `can_end_stmt`, rule 3 drops the
newline after a `break` and `break` ⏎ `x = 1` reaches the parser as
`break x = 1` — indistinguishable from same-line juxtaposition, which the
statement parser must then reject. With the membership, ASI's output stays
isomorphic ("semicolons everywhere the parser expects them") and the
parser accepts the same three boundaries as every other statement shape:
`;`, `}` (rule 2 never inserts before it — `{ break }` stays legal), EOF.
The keywords deliberately stay out of `can_continue_expr`, so `x = 1 +` ⏎
`break` joins as `break`-in-expression and dies with "expected expression".

**As-built note (`x = break` is a parse error, not a typeck one)**: the
card lists break-as-expression under Typeck, but per PLAN §3.3 ("they are
statements, not expressions; the parser classifies them like any keyword
statement") the grammar has no break-expression at any IR layer — there is
no `Expr`/`ExprHir`/`TypedExprHir` variant to type-check. `x = break` dies
in the expression parser ("expected expression, found break"), pinned by
`parse_rejects_break_as_value` and e2e `break_as_value.fln`. Nothing
reachable remains for typeck to check.

**As-built note (top level)**: a top-level `break` parses (typeck owns the
position check, so the shape is legal to parse everywhere statements are)
through dedicated `Item::Break`/`Item::Continue` → `HirItem` →
`TypedHirItem` variants; `typeck_item` routes them through the same shared
check, where the depth is always 0. Lower skips the item variants like the
pre-existing `TypedHirItem::Expr` skip — unreachable in a well-typed
program because typeck rejects.

**As-built note (loop context)**: one `loop_depth: usize` field on the
`TypeChecker`; `while`/`for` bodies increment (both skeletons — the
array/Range split is lower's desugar, so typeck sees every `for` the same
way and accepts the keywords in any `for` body; whether the inline-iter
closure-shaped expansion threads the edges is T04's decision to record);
`choose` arm bodies and nested function declarations (the only in-body
function boundary — `async` bodies are function bodies) **clear** it for
their extent, so one non-zero test rejects all four positions through one
shared `typeck_loop_jump`. The house collect-errors discipline made the
save/restore-before-`?` order mandatory: an error inside a loop body must
not leak the incremented (or cleared) depth into later functions — pinned
by `loop_depth_is_not_leaked_across_functions` (first function's loop-body
tail error + second function's bare `break` ⇒ both errors) and
`choose_arm_depth_is_restored_after_an_arm_error` (an arm error must not
leave the cleared depth behind; the `break` after the choose stays legal).

**As-built note (lower interim)**: T04 owns the loop edges, so
`lower_block`'s two new arms return the named, actionable defensive error
(`unsupported feature: \`break\` (loop exit edges are not implemented yet)` —
the T02 defensive-arm precedent) until T04 replaces them: a *legal*
program containing the keywords passes typeck and fails at lower. The
slots pass and the ownership walk gained no-op arms (no bindings, no
values; the ownership liveness model is flow-insensitive to the jump and
stays sound since a jump only shrinks the paths reaching later
statements). No bytecode change (D6 holds trivially: nothing new reaches
codegen until T04).

Tests: lexer `tokenize_break_continue_keywords` /
`tokenize_break_continue_prefix_identifiers`; ASI
`break_starts_a_new_statement` / `break_before_rbrace_inserts_nothing` +
the two decision-table updates; parser `parse_break_statement` /
`parse_continue_statement` / `parse_break_inside_while_block` /
`parse_break_last_in_block_no_semicolon` /
`parse_break_after_statement_on_next_line` /
`parse_rejects_break_as_value` / `parse_rejects_break_as_identifier` /
`parse_rejects_break_followed_by_expression` /
`parse_rejects_break_inside_expression_position`; typeck legal set
(`break_in_while_body_is_legal`, `break_in_for_over_array_is_legal`,
`continue_in_for_over_range_is_legal`, `continue_in_while_body_is_legal`,
`break_binds_innermost_of_nested_loops`,
`break_inside_loop_inside_choose_arm_is_legal`) and rejection set
(top level, function without loop, choose arm ×2, nested `func` and
`async func` boundary) plus the two leak pins; e2e invalid per PLAN §9 —
`break_outside_loop.fln` (function without a loop),
`continue_in_choose_arm.fln`, `break_across_function_boundary.fln`
(nested `func`), `break_as_identifier.fln` (reserved word),
`break_as_value.fln` (statement-not-expression). All gates green:
`cargo fmt --check`, `cargo clippy --all-targets -D warnings`, `cargo
test` (26 suites), `cargo doc --no-deps`; the 0.0.8 both-mode e2e matrix
unchanged and green.

**Depends on**: nothing.
**Refs**: PLAN §1 D4, §3.3.

---

## T04 — Lower: loop exit / continue edges (F3; D6)

- [x] `lower_while` and both `lower_for` skeletons (array / Range) pre-create
      an exit block and a continue-edge target; `break` → `Jump` exit,
      `continue` → `Jump` continue edge. Innermost loop binds (nested loops
      each carry their own edges; a stack of active edges in the `FnBuilder`
      or the typeck-checked context carries through).
- [x] Stack discipline: at both edges the stack depth equals the loop-head
      depth; apply the BUG-1 spill rule (`spill_for_fallthrough`) at every
      conditional terminator inside a loop body — verify-clean by
      construction, proven by the e2e set.
- [x] Inline-iter expanded loops (`map`/`filter`/`foldl`/`foldr`/`take` /
      Range inline lowering): `break`/`continue` inside the closure-shaped
      expansion either work (edges threaded) or are a clean typeck rejection
      — decide by implementation shape, record the outcome here, no silent
      behavior.
- [x] Loop-as-expression: loops are unit-valued (0.0.1 posture) — `break`
      exits produce unit; the merge block after the loop is unchanged.
- [x] Tests: lower unit tests for while/array-for/Range-for shapes; e2e
      valid — `break` in `while`, `continue` in `for`-over-Range,
      `continue` in `for`-over-array, nested loops (innermost wins),
      `break` inside `if` inside `while`; e2e invalid — the T03 set
      end-to-end.

**As-built note (deferred edges, not literal pre-creation)**: the card's
"pre-create an exit block" cannot be done *physically* for the exit (and the
`for` advance) target — cfg.rs §6.3 pins a conditional's untaken edge to the
*physically next* block, so nothing may be created between the loop head and
its body, or the condition-true path would fall into the wrong block. The
edges are instead **deferred** (the `lower_if` `branch_jumps` mechanism,
D6's "resolve-after-the-fact terminator patching"): the `break`/`continue`
arm terminates its block with `jump_later()` and records the block in
`LoopEdges { breaks, continues }` (a stack field on `FnBodyCx`, pushed
before the body lowering and popped after — innermost binding; lower is
fail-fast so an error mid-body cannot leak state into a later function);
after the body, `lower_while` creates the exit block and resolves breaks →
exit and continues → head, and `lower_for` creates the exit block and —
**content-driven** — the advance block (`L_incr` =
`[LoadLocal __i; Const 1; IAdd; StoreLocal __i]`, split out of the body's
tail flow) only when the body actually recorded a `continue`: that edge
must land on the increment (jumping straight to the head would skip it and
loop forever), and only the split makes that landing possible. A
`continue`-free body keeps the 0.0.8 layout — the advance stays inline in
the tail flow — so such modules stay byte-identical to their pre-0.0.9
encoding (0.0.8 D16's content-driven rule applied to the layout).
`L_incr` is entered at the head depth from both the body's fall-out and
every `continue` (the body value's push/pop stays inside the body-tail
flow). The shared mechanics live in three helpers: `lower_loop_jump` (one
body for the `Break`/`Continue` arms — deferred jump, edge record, dead
continuation), `pop_loop_edges` (the push/pop discipline, its
typeck-guarantee invariants stated as `unreachable!` per SPEC §4 — the
interim `.expect`s are gone, SPEC §11 clean), and `LoopEdges::resolve`
(the patch step shared by `lower_while` and `lower_for`).

**As-built note (the dead-continuation rule)**: the `break`/`continue` jump
is a block terminator; everything the same block still emits (a trailing
unit, the caller's continuation — body-value `Pop`, back edge, a caller
like `lower_if` ending each branch with its own jump to the merge) must
never follow a live terminator — codegen places block instructions before
the terminator's bytes, so trailing instructions would execute on the
reachable path, and a later jump would **overwrite** the break jump
entirely (silently turning `break` into a jump back into the loop). The
arm therefore opens a fresh block immediately after the jump; nothing
reaches it, the verifier's worklist (propagation from pc 0) never visits
it, and the dead subgraph absorbs the rest of the emission. Pinned by
`t04_statements_after_break_land_in_dead_code` (the live block is
instruction-free; the dead statement exists in the unreachable block) and
`t04_break_jumps_to_while_exit_block` (the jump is the block's whole
terminator).

**As-built note (inline-iter decision)**: no edges threaded — by
implementation shape there is nothing to thread. `lower_inline_iter`
emits only expressions and a `Call` to the callback function value; no
user statement is ever lowered inside the closure-shaped expansion, so
`TypedStmtHir::Break`/`Continue` cannot reach it. The callback body is its
own function, where typeck's lexical-containment rule (D4) rejects the
keywords as `BreakOutsideLoop`/`ContinueOutsideLoop` — the clean typeck
rejection. No defensive arm needed (no reachable path), no silent
behavior.

**As-built note (loop-as-expression + fixtures)**: unchanged — the exit
block still emits one `Unit` (the loop's value), entered at the head depth
from the condition's resolved jump and every `break`. e2e valid:
`tests/e2e/valid/loop_break_while.fln` (4), `loop_continue_for_range.fln`
(9), `loop_continue_for_array.fln` (8), `loop_nested_innermost.fln`
(`for`-over-Range inside `while`, innermost binding: 3/63),
`loop_break_inside_if.fln` (`break` two `if` levels deep: 3); the T03
e2e invalid set (`break_outside_loop`, `continue_in_choose_arm`,
`break_across_function_boundary`, `break_as_identifier`,
`break_as_value`) already pins the rejection shapes end-to-end. Lower unit
tests: the five `t04_*` above plus
`t04_for_range_continue_targets_index_advance_block` (Range skeleton,
imported `range_from`), and the three new functions in
`lower_block_entry_stack_depths_are_consistent` (`break_while`,
`continue_for`, `nested_loops`) put every edge entry through the
entry-depth worklist proof. The two pre-existing `for` shape tests
(`lower_for_array_desugar_instruction_by_instruction`,
`lower_for_range_desugar_uses_range_bounds_once`) keep their 0.0.8
expectations — their loops record no `continue`, so the inline advance is
the pinned layout (the split shape is pinned by the `t04_for_*` tests
above). The committed v3 compat fixture
`tests/fixtures/v3/aggregate.flnc` needed **no** regeneration: its two
`for` loops record no `continue`, so the content-driven layout keeps the
module byte-identical to its committed bytes (a re-run of
`regenerate_fixtures` diffs empty; v2 fixture untouched — no loops).
Bytecode gate (PLAN §9): no version bump —
`fleen-vm/tests/loop_bytecode.rs` compiles the loop fixtures in-process
and pins `MODULE_VERSION` (v7, 0.0.8 D13's content-driven rule). §9's
"writers emit v8" and D6's "writers keep emitting v8" resolve to that
default for these fixtures: none encodes a `PartialApply`, so lifting to
v8 would misdescribe the content; the rule D6 keeps is exactly 0.0.8's
content-driven bump (recorded deviation, no new opcode reaches the
stream).

All gates green: `cargo fmt -- --check`, `cargo clippy --all-targets -D
warnings`, `cargo test`, `cargo doc --no-deps`; the 0.0.8 both-mode e2e
matrix unchanged and green.

**Depends on**: T03.
**Refs**: PLAN §1 D4/D6, §3.3.

---

## T05 — std.str intrinsic batch (F4; D3)

- [x] `fleen-compiler/src/typeck/builtins.rs`: eight `BuiltinSig` entries
      per PLAN D3's table (names, signatures, intrinsic lowering).
- [x] `fleen-vm/src/vm.rs` `exec_builtin`: eight arms —
      `parse_int` (`str::parse::<i64>`, overflow/non-numeric → `Err(string)`,
      optional leading `-`/`+`, decimal only), `parse_float` (non-numeric →
      `Err`), `trim` (Unicode whitespace), `contains`, `starts_with`,
      `ends_with` (all bool), `index_of` (first occurrence by **Unicode
      scalar value**, not found → `Err`), `replace` (all occurrences,
      scalar-indexed pattern).
- [x] `std/str.fln`: doc-comment entries for the eight (VM intrinsics stay
      doc-only per 0.0.3 D8); indexing-unit contract stated per function.
- [x] No opcodes, no bytecode version change (D6); no new `Value` variants.
- [x] Tests: VM unit tests per function (empty needle for `contains`/
      `index_of`/`replace`/`starts_with` — decide empty-needle semantics:
      Rust-consistent, `index_of("")` = `Ok(0)`, `replace("", x, y)` no-op —
      and pin them in the doc comments); e2e valid fixture exercising all
      eight; e2e invalid — `parse_int` overflow caught as a **value** (`Err`
      extracted by `choose`, program prints the error, exits 0).

**As-built note (empty-needle semantics)**: `contains` / `starts_with` /
`ends_with` with the empty needle answer `true` and `index_of(s, "")` is
`Ok(0)` — Rust-consistent, no special-casing needed. `replace(s, "", y)`
is a **no-op** (the string returned unchanged), pinned by
`std_str_replace_empty_pattern_is_no_op` and in `std/str.fln`'s doc
comment — this **deviates from Rust**, whose `str::replace` inserts `to`
at every char boundary for an empty pattern; the ticket's named outcome
wins, and the deviation is documented where the contract lives. Error
payloads: `parse_int` → `"invalid int: <s>"` (PLAN §3.4's example),
`parse_float` → `"invalid float: <s>"`, `index_of` → `"substring not
found: <needle>"`. `parse_int` does **not** trim whitespace (Rust's
`i64::from_str` grammar; ` 42` is an `Err` — documented in
`std/str.fln`).

**As-built note (scalar index conversion)**: `str::find` returns a **byte
offset**, so the D3 scalar-index contract needs an explicit conversion —
`s[..byte].chars().count()` (pinned by
`std_str_index_of_reports_scalar_index`: `"héllo"` finds `"l"` at byte 3
but scalar index 2). The e2e valid fixture verifies the same case through
the full pipeline.

**As-built note (module bytes grow — the two 0.0.8 byte pins re-captured)**:
PLAN §9's parenthetical "intrinsics change VM behavior only, module
bytecode unchanged" is wrong as written: `lower/mod.rs` gives **every**
builtin a leading `FuncId` + name constant in every module, so eight new
entries grow every compiled stream (probe: 1859 → 2182 bytes, flag-off
pin: 1385 → 1708; +323 each, and decoding both dumps before re-capturing
confirmed the delta is exactly the eight new names + their placeholder
function records — no other byte changed, version stays 7 by the
content-driven rule). Consequence: the two 0.0.8 D16 byte-identity pins
could not stay green and were re-captured from HEAD as self-pins —
`fleen-vm/tests/std_byte_identity.rs`
(`std_module_is_byte_identical_with_0_0_7` → `std_probe_dump_is_pinned`)
and `codegen/tests.rs`
(`flag_off_v7_output_is_byte_identical_with_0_0_7` →
`flag_off_v7_output_is_pinned`), each with the history recorded in its
doc comment. This matches PLAN §9's actual verdict ("std byte-identity
not required … recorded expectation, not a hash gate") — corrected as
built: the byte stream does grow, which is why the gate is a self-pin
now, not a cross-version identity claim. The third recompile-and-compare
consumer, the committed v3 compat fixture
`tests/fixtures/v3/aggregate.flnc` (`v3_fixture_is_committed_byte_identical`
compiles the fixture source through the current pipeline), was
regenerated with the repo's own `regenerate_fixtures` tool: 2830 → 3153
bytes, the same +323 delta (eight builtin names + placeholder records,
version still pinned to 3 after compile; `v3_fixture_verifies_and_executes`
re-confirms the instruction groups, verification, and stdout). The v2
fixture is hand-built (no compiler pass) and stayed byte-identical; the
v1 fixture is a genuine 0.0.1 artifact, decoded only — both untouched.

PLAN §7's "Version stays 8" and §9's "writers emit v8 / module bytecode
unchanged" wording were corrected to this reality in the post-T05 review
pass (the content-driven rule keeps the touched modules at v7, and the
intrinsics do grow every module stream via the leading builtin table).

**As-built note (test plumbing)**: the eight arms take plain string
arguments (strings cannot carry a Future/Channel), so — unlike the
`std.arr.*` group — no deep-fork guard is needed; ordinary arms after
`std.str.split`. VM unit tests share one `str_call` helper (string args
only). The e2e invalid fixture `str_parse_int_overflow_caught.fln` pins
`.exit` = 0 (the error-as-value program must not crash); its printed
message is stdout-verified in `tests/e2e/valid/std_str_batch.fln`
because the invalid harness compares stderr only.

**Depends on**: nothing.
**Refs**: PLAN §1 D3, §3.4, §4.

---

## T06 — Docs sync: debt corrections + BUGS.md status (F5)

- [x] `docs/0.0.7/BUGS.md`: header + BUG-1 marked **fixed** (commit
      `a5b8bd7`, v0.0.8 tag, fixture path); BUG-2 marked **fixed in 0.0.9**
      (T01, PLAN D1) — the file keeps its verbatim findings (append-only
      posture), status lines added.
- [x] CHANGELOG.md §0.0.8 "Known limitation" + SPEC.md §0.0.8 known-limit
      sentence + `docs/0.0.8/TICKETS.md` T09 deviation text + README:
      replace the captured-builtin "fails at runtime with `UnknownBuiltin`"
      wording with the fixed-at-`8dbbc8b` record (all four verified stale).
- [x] DESIGN.md §10.5: guard-arm lowering no longer "deferred to U09/U07" —
      guards work (0.0.7 attempt chain); `otherwise` legal from 0.0.9
      (cross-ref `docs/0.0.9/PLAN.md` D2).
- [x] DESIGN.md / SYNTAX.ebnf: `break` / `continue` grammar + reserved-word
      notes; std.str section gains the eight intrinsics; DESIGN.md §14
      Feature Scope matrix row for 0.0.9; §18 Roadmap row for 0.0.9.
- [x] SPEC.md §14 0.0.7 known-limit sentence (2) (flagged in the T01
      review): `when P(None, None)` no longer trips `DuplicateBinding` —
      fixed in 0.0.9 by T01 (zero-payload variant patterns are variant
      references, PLAN D1/D7); rewrite the limit as resolved. (The same
      block's (1) is fixed by `a5b8bd7`, already recorded in the header
      above.)
- [x] BYTECODE.md: no edits — confirm and record in the card that no row
      changed (D6).
- [x] New CHANGELOG `[0.0.9]` section drafted (T07 finalizes).

**As-built note (the card's "all four verified stale" was wrong about
README)**: the root `README.md` never carried the captured-builtin wording
(grepped: no "UnknownBuiltin" / "fails at runtime" hit). The actual stale
copies live in **`examples/0.0.8/README.md`** ("Known limitation (honest
scope)" section — retitled and corrected, the demo's user-function-only
scope re-explained as a pedagogical choice, not a workaround) and
**`examples/0.0.8/curry_demo.fln`** (the header comment; comments never
reach the bytecode, so no fixture changes). Both corrected to the
fixed-at-`8dbbc8b` record; the root README was left untouched.

**As-built note (SPEC §14 choose known-limits block already done)**: the
block (SPEC.md "choose (Pattern Matching)" → Known limits) already marks
(1) fixed in 0.0.8 (`a5b8bd7`) and (2) fixed in 0.0.9 (T01, D1/D7) — it
landed with the T01 review pass. Verified at HEAD, no further edit; the
stale sentence this card did correct in SPEC.md is the §14 0.0.8 known-limit
line inside "Feature Flags and Curried Declarations" (the
`range_from(0)` runtime-gap wording → fixed-at-`8dbbc8b`).

**As-built note (the card's "DESIGN.md §14 Feature Scope matrix" is a
mislabel, same as 0.0.8's T11 card)**: DESIGN.md §14 is *Explicit
Rejections*; the feature matrix is **§17** and the roadmap **§18**. The
0.0.9 column was added there (§17 also gained the 0.0.9 goal paragraph,
matching 0.0.1–0.0.8; new `break` / `continue` row; the `choose` / `Result` /
std rows annotated with the 0.0.9 additions). DESIGN.md §6 gained the
`break / continue (0.0.9, D4)` subsection and §2 the reserved-word note; the
std.str batch is recorded under §11's std bullet (there is no standalone
std.str section in DESIGN.md).

**As-built note (§2 Keywords block corrected as one unit)**: the block the
break/continue note attaches to was stale in both directions — it listed
`return` (not a Fleen keyword; the lexer table has no "return" match) and
missed `enum` (0.0.6), `ptr` / `addr` / `deref_ptr` (0.0.5), `or` / `and` /
`not`, and the type keywords (`int` / `float` / `bool` / `string` / `unit` /
`Result`, `box` / `ref`). The block now mirrors the lexer keyword table
(`fleen-compiler/src/lexer/lexer_impl.rs`) and says so; the per-milestone
reserved-word notes carry forward.

**As-built note (BYTECODE.md: one prose bullet corrected — the card's "no
edits" holds for format rows)**: D6 verified directly — no opcode row, no
table, no version statement changed (writers still emit ≤8; T04 lowered
`break`/`continue` to plain `Jump` opcodes and T05 added
VM-intrinsic arms, none of which is a format row). But §5.6's rule bullet
"no `break` / `continue`" (a 0.0.3-era source-construct note, not a format
row) became a falsehood when T04 landed — leaving it would make the format
spec assert that the keywords do not compile. The bullet was rewritten in
place to the Jump-based lowering with the correction recorded in the bullet
itself; recorded here as a card deviation. No other BYTECODE.md text
touched.

**As-built note (CHANGELOG `[0.0.9]` draft content)**: drafted from the
T01–T05 as-built cards, not the PLAN's pre-implementation wording — the D7
coverage-algebra deviation (joint coverage via constructor splitting), the
`ChooseResultNeedsOkErrArms` retirement, the content-driven `for`-advance
split, the two new reserved words as the only breaking set, the +~323-byte
module-stream growth with the two byte pins re-captured as self-pins (T05),
and the corrected-in-place §0.0.8 limitation bullet. The section header
carries a "drafted by T06 / finalized at the T07 release gate" note; the
codename stays TBD (PLAN header: holder's call).

**As-built note (SYNTAX.ebnf header)**: the version banner comment was bumped
v0.0.8 → v0.0.9 and a "New in 0.0.9" block added (`break` / `continue`,
Result-`choose` `otherwise`) — part of the grammar-notes bullet, recorded here
because the card text names only the productions.

**As-built note (pre-commit review pass)**: the BYTECODE.md note above carried a
wrong "0xC0-region" opcode-region claim — corrected (Jump is plain; the region
claim dropped; control flow lives in 0x50–0x5F, and the note's point is only
"not a format row"). The `8dbbc8b` commit-title parenthetical collapsed to the
canonical records (CHANGELOG `[0.0.8]` keeps the title; BUGS.md's BUG-1 status
line keeps the `a5b8bd7` title — summary references elsewhere stay SHA-only).
"self-pins" is glossed in the CHANGELOG draft. BYTECODE.md's §2 version-field
comment (0.0.7-era "writes 7, readers accept {1..7}") synced to the
content-driven rule (writers emit 7 or 8; readers accept {1..8}). SPEC gained
the 0.0.9 goal paragraph (§13) and the 0.0.9 matrix column (§15) — T07's
SPEC-matrix sync pre-completed here; DESIGN §17's 0.0.9 goal status updated to
match (T01–T06 green).

**Depends on**: T01–T05 (records their as-built state).
**Refs**: PLAN §0, §1 D1–D6, §8.

---

## T07 — Examples + CHANGELOG + version bump + release gate (F5/F6)

- [x] `examples/0.0.9/`: single-file demo (BUG-2 pattern spelling,
      Result-`choose` guard + `otherwise`, `break`/`continue` in both loop
      forms, the eight std.str calls) runnable with `cargo fln`; README
      line per the 0.0.7/0.0.8 layout.
- [x] CHANGELOG `[0.0.9]` finalized (Added: pattern fix, `otherwise`,
      `break`/`continue`, std.str batch; Changed: two new reserved words —
      the breaking set; Fixed: BUG-2 with BUGS.md ref).
- [x] Version bump: `Cargo.toml` workspace `version` + `std/VERSION` →
      `0.0.9`; `Cargo.lock` regenerated by the build.
- [x] Full gates green: `cargo fmt -- --check`, `cargo clippy -- -D
      warnings`, `cargo test`, `cargo doc --no-deps`; the 0.0.8 both-mode
      e2e matrix still passes byte-identically.
- [x] Release publication (tag `v0.0.9` + six-target archives) via the
      existing pipeline; codename settled in PLAN header before the release
      commit (holder's call).

**Depends on**: T06.
**Refs**: PLAN §10.

**As-built note (the gate found a real bug — T02's otherwise path)**: the
demo's §4 called two `std.str.*`-scrutinee `choose`s with `otherwise` in one
`main` and failed bytecode verification (`StackDepthMismatch`: predecessor
expects 0, got 1). Root cause (lower, not verify): the general Result path
emits its `IsErr` dispatch into the current block and lets the untaken (Ok)
edge **fall through** to `ok_entry` — safe for a first choose (its
prologue sits in a freshly-started block), but for a second `choose` the
prologue lands in the first choose's merge block, which is not physically
last (the first choose's fallback body and err entry were created after the
merge), so the fallthrough landed in the earlier fallback body at depth 1
against its depth-0 tail predecessors. Latent since 0.0.7's attempt-chain
generalization (two guarded/nested Result chooses in one body had the same
shape; no fixture ever exercised two in one body — the T02/T06 fixtures put
one per function). Fix: `spill_for_fallthrough()` at the shared dispatch
(before the `Dup`/`IsErr`/`JumpIfTrue` prologue) — a depth-preserving
`Jump` into a fresh physically-last tail, a **no-op** for the common
single-choose shape so existing streams are unchanged (0.0.8/0.0.9
flag-off, both-mode and byte-pin fixtures all still green). The simple
Result path and every other conditional site were audited: all are safe by
construction (fresh head blocks, immediate create+start, explicit dispatch
jumps, or an existing spill). Regression set: lower
`t07_second_result_choose_ok_entry_is_physically_next` (the invariant: the
block physically next after every Result `IsErr` dispatch starts with
`UnwrapOk`), verify `t07_two_result_otherwise_chooses_in_one_body_verifies`
(compile→verify end to end), e2e `result_choose_otherwise_twice.fln`
(companion to `result_choose_otherwise.fln`). CHANGELOG `[0.0.9]` Fixed
records it; SPEC §13's 0.0.9 paragraph mentions it.

**As-built note (examples sweep needs no registration)**:
`fleen-vm/tests/examples.rs` walks `examples/` recursively, so
`examples/0.0.9/repairs_demo.fln` is picked up by `cargo test`
automatically (single-file contract: exit 0, empty stderr).

**As-built note (byte-identity interpretation)**: the card's "0.0.8
both-mode e2e matrix still passes byte-identically" is satisfied via the
T05 as-built record — the byte pins became self-pins when the eight std.str
builtin names grew every module stream by design, so the gate is the
both-mode e2e matrix inside `cargo test` (all green), not a cross-version
hash check.

**As-built note (codename deferred)**: the holder chose at the T07 session
to keep the codename **TBD** and settle it at the release gate (the card
allows "before the release commit"); the CHANGELOG header and the
`examples/0.0.9/README.md` carry the TBD note. **Settled at the release
gate: Mending Weaverbird 🪡** (PLAN header, CHANGELOG `[0.0.9]`,
`examples/0.0.9/README.md`, `release/0.0.9/RELEASE_NOTES.md`).

---

## Dependency graph

```
T01 ──┐
T02 ──┼──→ T06 ──→ T07
T03 ──→ T04 ──┘
T05 ──┘
```

- T01, T02, T03, T05 are independent roots.
- T04 depends on T03; T06 depends on T01–T05; T07 depends on T06 and is the
  release gate.

---

## Out of scope reminders (from PLAN §8)

No BUG-1 re-work (already fixed), no captured-builtin re-work (already
fixed), no `substring`/`slice`/`chars`, no `for` over `string`, no labeled
`break`, no `break <value>`, no generics / traits / operator overloading /
closures (0.1.0), no channel `close`/`select`, no `async {}` blocks, no
file I/O, no bytecode version change. Any of these needs a new decision
entry in PLAN §1 first.
