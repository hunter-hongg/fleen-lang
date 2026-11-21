# 0.0.9 — Small Repairs: Pattern Fix, Result-`choose` Completion, `break`/`continue`, std.str Batch

> **Codename**: **Mending Weaverbird 🪡** (settled at the release gate,
> holder's call; lineage 0.0.1 Swift Fox 🦊, 0.0.2 Hermit Crab 🐚, 0.0.3 Busy
> Beaver 🦫, 0.0.4 Patient Ants 🐜, 0.0.5 Careful Hedgehog 🦔, 0.0.7 Mantis
> Shrimp 🦐, 0.0.8 Cheeky Hamster 🐹).
>
> **Goal**: a repair-and-fill milestone. One real bug fix (BUG-2), one
> long-deferred semantic completion (Result-`choose` `otherwise`), two
> control-flow keywords (`break` / `continue`), and one batch of std.str
> intrinsics. No bytecode change: the version stays v8, no new opcodes, no
> table changes, no serialization work.
>
> **Theme**: pay down the recorded debts instead of opening a new front. The
> 0.0.7 post-release findings list, the 0.0.2-era "deferred to U09" marker,
> and the missing loop-exit keywords are all closed here; std grows the same
> way it grew in 0.0.6 T07 (intrinsics + builtin-table entries, zero opcode
> cost).
>
> **Status**: planning. Decisions D1–D6 below are settled at planning level;
> each names the alternative it rejected and what the choice locks. They may
> be revised only by a new entry here, not silently in a ticket card.
> D7 was added as-built while implementing T01 (it reopens a sentence of
> D1's scope), per exactly that rule.
>
> **What this is not**: no generics, no traits, no closures, no operator
> overloading, no channel `close`/`select`, no `async {}` blocks, no file
> I/O — all stay 0.1.0 (§8).

---

## 0. Relationship to adjacent versions

| Version | Scope |
|---------|-------|
| 0.0.8 | complete (release published — tag and archives cut) |
| **0.0.9** | **BUG-2 fix (zero-payload variant patterns) + Result-`choose` `otherwise` + `break`/`continue` + std.str intrinsic batch + docs sync** |
| 0.1.0 | function-level generics / traits / operator overloading / closures; recursive types revisit; extended string escapes (unchanged) |

Corrections recorded at planning time (this milestone owns the doc debt):

- **BUG-1 is already fixed**: commit `a5b8bd7` ("spill before every
  conditional terminator in a choose arm") landed after the v0.0.8 release
  prep and inside the `v0.0.8` tag; regression fixture
  `tests/e2e/valid/choose_arm_if_else.fln` exists and passes.
  `docs/0.0.7/BUGS.md` still describes it as open — T06 updates it.
- **The 0.0.8 captured-builtin gap is already fixed**: commit `8dbbc8b`
  ("VM runtime for captured range / inline-iter builtins") is inside the
  `v0.0.8` tag; `range_from(0)` partials run end to end (probed).
  CHANGELOG.md §0.0.8, SPEC.md §0.0.8, `docs/0.0.8/TICKETS.md` T09, and
  README still describe it as a known limitation — T06 corrects all four.
- **Result-`choose` guards already work**: 0.0.7's attempt-chain lowering
  (`lower_result_single_arm` general path) accepts `when Ok(x) if x > 3`
  today; only `otherwise` (and other non-ResultCtor arms) is rejected.
  DESIGN.md §10.5 still says guard lowering is "deferred to U09" — T06
  corrects it. What 0.0.9 actually adds is `otherwise` only (T02).

The only source breaking set of 0.0.9 is **two new reserved words**
(`break`, `continue`, D4) — the same magnitude of breaking change 0.0.5
(ptr words) and 0.0.6 (`enum`) already established.

---

## 1. Settled decisions (D1–D7)

### D1 — BUG-2: a zero-payload variant pattern is a variant reference, not a binding

Adopts **option (a)** from `docs/0.0.7/BUGS.md` BUG-2. Inside a pattern, a
bare identifier that names a **zero-payload variant** of an enum in scope is
resolved as a variant reference and does **not** enter the pattern's binding
namespace. `when P(None, None)` compiles and matches; `DuplicateBinding`
no longer fires for it.

- **Shadowing rule (the BUGS.md open question, settled here)**: a pattern
  binding in the same position wins over a variant reference. `when P(x, x)`
  with a local `x` in scope binds twice (and still trips
  `DuplicateBinding` — one bare name per pattern remains the rule for
  *bindings*); `when P(None, None)` refers to the variant twice. A user can
  always bind by choosing a fresh name; a variant name can always be written
  as a binding by the existing behavior of non-variant identifiers. The
  lookup order is: local bindings already registered earlier in the same
  pattern → enclosing scopes' locals → zero-payload variant → unresolved.
- **Rejected (b)**: promoting the wart to a documented rule. It makes the
  most natural spelling of "both empty" (`when P(None, None)`) a permanent
  error while `when P(Some(z), None)` works — an asymmetry with no
  semantic payoff. Option (a) removes the special case instead of
  enshrining it.
- **What this locks**: the parser's classification of bare identifiers in
  patterns becomes scope-sensitive (the resolver already owns scope
  lookup, so classification moves to / gains a resolver pass over pattern
  subtrees; the parser keeps emitting bare identifiers unchanged).
  Bytecode unchanged: a zero-payload variant reference lowers to the same
  discriminant check as today.
- **Carried unchanged**: one bare *binding* name per pattern still holds
  (`DuplicateBinding` for repeated binding names); zero-payload variant
  references may repeat freely.

### D2 — Result-`choose` gains `otherwise` (and general arms); guards stay as they are

`lower_choose_result` stops rejecting non-ResultCtor arms. `otherwise` is
accepted as the always-matching fallback arm of a Result scrutinee:
`when Ok(x) { … } otherwise { … }` is legal and exhaustive (typeck already
treats `otherwise` as full coverage on scalar scrutinees — the same rule
extends here). General nested patterns (`when Ok(Some(y))`) keep working
through the existing attempt-chain path; the rejection branch
(`UnsupportedFeature "non-Ok/Err arm … deferred to U07"`) is deleted and
the defensive tail (pop payload + unit) becomes reachable only for
non-exhaustive input typeck can no longer produce.

- **Guards**: already functional (see §0 correction); no lowering work. The
  typeck rule from DESIGN.md §10.5 stands unchanged — a guarded arm does
  not count toward exhaustiveness, so a guard needs an unguarded duplicate
  arm or an `otherwise`; with `otherwise` legal, that recipe finally
  compiles.
- **Rejected**: deferring `otherwise` to 0.1.0. It is the completion of a
  recorded 0.0.2 deferral whose guard half already landed by accident;
  the remaining work is one lower branch + typeck coverage rule + tests.

### D3 — std.str batch: eight intrinsics, all `Result`-valued where failure is possible

New VM intrinsics registered in the builtin table and documented in
`std/str.fln` (the 0.0.6 T07 mechanism; zero opcodes):

| function | signature | failure |
|----------|-----------|---------|
| `std.str.parse_int` | `(string) -> Result<int, string>` | non-numeric input, i64 overflow → `Err` |
| `std.str.parse_float` | `(string) -> Result<float, string>` | non-numeric input → `Err` |
| `std.str.trim` | `(string) -> string` | — (Unicode whitespace) |
| `std.str.contains` | `(string, string) -> bool` | — |
| `std.str.starts_with` | `(string, string) -> bool` | — |
| `std.str.ends_with` | `(string, string) -> bool` | — |
| `std.str.index_of` | `(string, string) -> Result<int, string>` | not found → `Err` |
| `std.str.replace` | `(string, string, string) -> string` | — |

- **Indexing unit**: Unicode scalar values, matching `std.str.len`'s
  existing contract (`substring`/`slice` deliberately deferred — D6).
- **Parsing semantics**: `parse_int` accepts optional leading `-` (and `+`),
  decimal only; overflow is an `Err` value, never a panic or wrap —
  consistent with `IAdd`'s checked-overflow posture.
- **`index_of` returns a `Result`** rather than `-1`: the out-of-band
  sentinel pattern is what `std.arr.get` rejected in 0.0.3 (the failure is
  a value, not a magic number). `starts_with`/`ends_with`/`contains` return
  `bool` because a question has a total answer.
- **No `substring`/`slice`/`chars`** in 0.0.9: substring by scalar index
  invites an OBO-bug API whose failure mode (out-of-range) would need
  another decision; it joins the 0.1.0 string work (extended escapes are
  already there). Recorded as out of scope (§8).

### D4 — `break` / `continue` become reserved words, legal only inside loop bodies

- `break` exits the innermost enclosing loop (`while` or `for`); `continue`
  jumps to the innermost loop's next-iteration edge. Both are compile errors
  outside a loop body (`BreakOutsideLoop` / `ContinueOutsideLoop`) and
  inside `choose` arm bodies, `async` task boundaries, and function-boundary
  crossings of either kind (a `break` may not escape the function that
  lexically contains the loop — the same lexical containment the `?` rules
  use).
- **Breaking set**: `break` and `continue` become reserved (source breaking,
  D-for-record: programs using them as identifiers stop compiling — the
  0.0.5/0.0.6 precedent). Everything else is additive.
- **Not an expression**: `break`/`continue` are unit-valued statements, not
  values (a loop has no break-with-value form; `x = break` is a type error).
  Rust's `break value` needs expression-typed loops — out of scope forever
  at this granularity.
- **Rejected**: labeled break (`break outer`). Labels add a namespace and
  an ASI interaction for a construct structured code rarely needs; nested
  loops use a flag or a helper function as today. Revisit only with
  evidence.

### D5 — `for` over a string iterates Unicode scalar values — not in 0.0.9

Recorded so it is not re-litigated: `for c in "abc"` stays a typeck error
in 0.0.9 (the iterable set remains `[T]` and `Range`). With `break`/
`continue` landing in the same milestone it is tempting to bundle, but it
needs a `Char`-adjacent type decision (scalar value vs `string` of length 1)
that belongs with the 0.1.0 string work. Out of scope (§8).

### D6 — No bytecode change: version stays v8

All four work items compile to existing opcodes: BUG-2 reuses the
discriminant-check lowering of today's zero-payload variant patterns;
`otherwise` reuses the attempt-chain tail; `break`/`continue` lower to
`Jump` to pre-created exit/continue edges (the CFG block API's
`resolve`-after-the-fact terminator patching, the BUG-1 spill rule applies
at each loop-body conditional terminator); std.str is pure VM intrinsics.
`BYTECODE.md` gains no rows; writers keep emitting v8; readers accept
`{1..=8}` unchanged. Any future opcode need is a new PLAN decision first.

### D7 — Exhaustiveness composition changes the coverage algebra (as-built, T01)

D1's scope sentence "no change to the coverage algebra" (§3.1) turned out
to be unimplementable as written: the resolver classification alone still
rejects T01's own acceptance fixture, because the BUG-2 repro's four
complementary arms cover `P` only *jointly* and 0.0.7's per-arm rule
(`pattern_covers_variant`) requires each arm to be individually total.
Accepting jointly-covering arms **is** a change to the coverage algebra,
recorded here rather than by silently re-reading D1.

- **The rule (strictly additive over 0.0.7)**: the per-arm rule stays
  first; `check_exhaustiveness` gains a second disjunct — the guard-free
  arms matching one variant jointly cover it when constructor splitting
  over their coverage matrix accepts (`variant_arms_jointly_total` +
  `pattern_rows_cover`): each arm contributes its payload sub-patterns as
  a row, rows are specialized constructor-by-constructor across the
  positions, and a recursion level survives only when every constructor
  choice keeps some row. Every program exhaustive under 0.0.7 stays
  exhaustive; zero-payload variants keep their outright `EnumVariant`
  coverage.
- **Soundness (corrected from the first draft)**: the draft composed arms
  per payload *position* and asked `patterns_are_total` per pooled group.
  Position-wise totality does not imply joint coverage — `P(Some, None)` +
  `P(None, Some)` pools total per position while `P(Some, Some)` matches
  neither arm and silently falls through to lower's defensive tail; the
  same pooling also hid a `Result`-payload hole (`P(Ok(a), x)` +
  `P(Err(e), 5)`) and a scalar-head hole (`P(x, 1)` + `P(2, y)`), all
  accepted by the draft but rejected at the merge-base. Splitting keeps
  the surviving rows correlated with the tuples they matched, so the
  per-position argument is never made. Pinned by
  `cross_position_pattern_gap_is_rejected`,
  `result_payload_position_gap_is_rejected`,
  `binding_head_does_not_cover_other_arms_tuples`, and e2e invalid
  `choose_payload_cross_position_gap.fln`.
- **What this locks**: D6 holds unchanged — exhaustiveness is a
  compile-time verdict only; no bytecode, no opcode, no table change. The
  0.0.7 per-arm rule and its verdicts are untouched.

---

## 2. Feature list

| ID | Feature | Tickets |
|----|---------|---------|
| F1 | Zero-payload variant patterns resolve as variant references (BUG-2) | T01 |
| F2 | Result-`choose` accepts `otherwise` / general arms | T02 |
| F3 | `break` / `continue` reserved words with loop-bound lowering | T03, T04 |
| F4 | std.str intrinsic batch (8 functions) | T05 |
| F5 | Docs sync: BUGS.md status, CHANGELOG/SPEC/README corrections, version bump | T06, T07 |
| F6 | Examples + integration gate | T07 |

## 3. Language design

### 3.1 Zero-payload variant patterns (F1)

```fleen
enum Option<T> { Some(T), None }
enum Pair<A, B> { P(A, B) }

choose move p {
    when P(None, None) { print("empty"); }   // 0.0.9: legal (was DuplicateBinding)
    when P(Some(x), None) { print(x as string); }
    when P(None, Some(y)) { print(y as string); }
    when P(Some(z), Some(w)) { print("both"); }
}
```

- Classification happens in the resolver over pattern subtrees: a bare
  identifier pattern is a variant reference iff it names a zero-payload
  variant of an enum reachable in scope **and** is not already a registered
  binding earlier in the same pattern (D1 order). Everything else keeps the
  existing Ident path.
- Exhaustiveness: a zero-payload variant arm covers its variant exactly as
  today's enum-variant arms do; jointly-covering complementary arms compose
  through the splitting engine (as-built deviation from this bullet's
  original "no change to the coverage algebra" — recorded as D7).
- Or-patterns compose: `when None | Some(0)` binds nothing and covers both
  alternatives (binding-consistency already trivial for empty sets).

### 3.2 Result-`choose` completion (F2)

```fleen
func halve(n: int): Result<int, string> {
    v = choose move half(n) {
        when Ok(x) if x % 2 == 0 { x }   // guard: works since 0.0.7, now documented
        otherwise { 0 }                   // 0.0.9: legal fallback
    };
    v
}
```

- `otherwise` may mix with Ok/Err arms freely; it is always last in coverage
  terms (typeck: an `otherwise` arm makes any arm set exhaustive).
- Multiple Ok arms + multiple Err arms + `otherwise` all compose on the
  existing attempt chains; `ChooseResultNeedsOkErrArms` is retired (the
  exhaustiveness gate is typeck's, as for every other scrutinee type).

### 3.3 `break` / `continue` (F3)

```fleen
x = 0;
while true {
    x = x + 1;
    if x > 3 { break; }
};
for i in range_from(0, 10) {
    if i % 2 == 0 { continue; }
    print(i as string);
};
```

- Statement grammar: `break_stmt = "break" , [ ";" ] ;` (ASI handles the
  semicolon); same for `continue`. They are statements, not expressions
  (D4); the parser classifies them like any keyword statement.
- Lowering: `lower_while`/`lower_for` pre-create an exit block and a
  continue-edge target; a `break` emits `Jump` to the loop's exit, a
  `continue` emits `Jump` to the continue edge (for `for`-over-array that
  is the index-advance head, for `while` the condition head). The loop's
  stack depth at both edges equals the loop-head depth (BUG-1's
  spill-for-fallthrough discipline applies at each conditional terminator
  in the body). Nested loops bind to the innermost; crossing a function,
  `async`, or `choose`-arm boundary is a typeck error (D4).

### 3.4 std.str batch (F4)

```fleen
import std.str;
import std.io;

n = std.str.parse_int("42")?;          // Err propagates; value 42
bad = std.str.parse_int("4x2");        // Err("invalid int: 4x2")
s = std.str.trim("  hi  ");            // "hi"
has = std.str.contains("hello", "ell"); // true
i = std.str.index_of("hello", "l");    // Ok(2)
t = std.str.replace("aaa", "a", "b");  // "bbb"
```

## 4. std library changes

- `std/str.fln` gains doc-comment entries for the eight new intrinsics
  (Fleen source stays doc-only for VM intrinsics per the 0.0.3 D8 rule).
- No other std module changes; `std.arr`/`std.collection`/`std.io`/
  `std.os`/`std.num`/`std.time` untouched.

## 5. Compiler-stage changes

| Stage | Change |
|-------|--------|
| Lexer | `break` / `continue` keywords (D4); `#`-adjacent lexing untouched |
| Parser | `break`/`continue` statements; pattern bare-identifier handling unchanged (classification is resolver work, D1) |
| Resolver | pattern-subtree variant-reference classification + scope lookup (D1); the two keywords reserved |
| Typeck | `BreakOutsideLoop` / `ContinueOutsideLoop` + boundary-crossing checks; `otherwise`-on-Result coverage rule; loop-context tracking |
| Lower | delete the Result `UnsupportedFeature` branch (D2); `otherwise` = attempt-tail arm; loop exit/continue edges + `Jump` lowering (D6); zero-payload variant reference reuses the existing variant-arm lowering |
| Codegen | none (all MIR→bytecode paths exist) |
| Verify / VM | none for control flow; VM `exec_builtin` gains eight std.str arms |

## 6. VM impact

- `exec_builtin` grows the eight `std.str.*` arms (T05). Rust-side
  implementation: `parse_int` → `str::parse::<i64>` mapped to `Err`, Rust
  `%`-free; `trim`/`contains`/`starts_with`/`ends_with`/`index_of`/
  `replace` over `chars()`/scalar indices per D3.
- No new `Value` variants, no opcode arms, no scheduler/executor changes.

## 7. Bytecode impact

None (D6). No opcode rows; no table changes; the module version follows
0.0.8's content-driven rule — the std.str arms add no opcode, so affected
modules stay at v7 unless a `PartialApply` was encoded (as-built: T05's
pinned dumps read `version=7`). `BYTECODE.md` untouched except
(optionally) nothing — T06 confirms and records the confirmation.

## 8. Explicitly out of scope (0.0.9)

| Item | Reason |
|------|--------|
| BUG-1 re-fix | Already fixed (commit `a5b8bd7`, in the v0.0.8 tag); only the BUGS.md status line remains — T06 |
| Captured-builtin gap re-fix | Already fixed (commit `8dbbc8b`, in the v0.0.8 tag); CHANGELOG/SPEC/TICKETS/README wording remains — T06 |
| Guard-arm lowering on Result | Already works via 0.0.7's attempt chain; documentation only — T06 |
| `substring` / `slice` / `chars` | OBO-prone API + failure-mode decision; joins 0.1.0 string work (D3) |
| `for` over `string` | Needs a scalar-vs-`string`-of-length-1 type decision; 0.1.0 (D5) |
| Labeled `break` | Namespace + ASI cost without demonstrated need (D4) |
| `break <value>` | Needs expression-typed loops; not offered (D4) |
| Generics / traits / operator overloading / closures | 0.1.0 (unchanged) |
| Recursive enums, `Result` → std migration | Unchanged carries (0.0.7 D3 / D9) |
| Channel `close` / `select`, `async {}` blocks, file I/O | 0.1.0 carries (0.0.4 §10.6 / §6) |

## 9. Test matrix (planned)

- Resolver: `when P(None, None)` valid fixture (BUG-2 repro verbatim);
  shadowing fixture (`when P(x, x)` with local `x` → still
  `DuplicateBinding`); non-variant repeated bare name still rejected.
- Lower/typeck: Result-`choose` `otherwise` fixtures (with guard + without;
  `Err` arm + `otherwise`; multiple Ok arms + `otherwise`); retired
  `ChooseResultNeedsOkErrArms` invalid fixtures replaced by exhaustive-choose
  errors.
- `break`/`continue`: while + both `for` skeletons; nested loops (innermost
  binding); `continue` in `for`-over-Range; invalid set — outside loop, in
  `choose` arm, across function/async boundary, as identifier (reserved
  word), `break`-as-value.
- std.str: one e2e valid fixture exercising all eight; invalid fixtures for
  `parse_int` overflow → `Err` value path; `index_of` not-found `Err`.
- Bytecode gate: no version bump asserted — no new fixture is v8 (none
  encodes a partial; the content-driven rule keeps them at v7); std
  byte-identity not required: every builtin occupies a leading `FuncId` +
  name constant, so the eight intrinsics grow every module stream and the
  0.0.8 byte pins became self-pins (T05 as-built) — recorded expectation,
  not a hash gate.

## 10. Versioning

- `Cargo.toml` workspace version → `0.0.9`; `std/VERSION` → `0.0.9` (T07).
- Release publication (tag + six-target archives) follows the 0.0.7/0.0.8
  pipeline; codename settled before the release commit (holder's call).
