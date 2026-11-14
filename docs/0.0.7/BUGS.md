# 0.0.7 — Post-release bug findings (legacy, unassigned)

> Found on 2026-11-14 while writing `examples/0.0.7/pair_demo.fln` and the
> `lang_docs/0.0.7/` reference, after the `v0.0.7` release was cut. Both are
> recorded here verbatim (repros, observed, expected, suspected area) so the
> next milestone can pick them up. **Fix rule (SPEC §7): the regression test
> is written first.** Note that any compiler fix makes the published
> `v0.0.7` archives stale — the fix should land in the next milestone's
> version, not by re-cutting `v0.0.7`.

---

## BUG-1 — `choose` over an enum scrutinee rejects an arm ending in `if`/`else` (verify failure)

**Severity**: compile-time hard failure on legal source; blocks the natural
"dispatch inside a match arm" pattern.

**Repro** (minimal, non-parameterized — the parameterized enums are not
involved):

```fleen
enum E { A, B }

func f(e: E): int {
    choose e {
        when A { if true { 1 } else { 2 } }
        when B { 3 }
    }
}

func main(): int {
    print(f(E.A) as string);
    0
}
```

**Observed**: bytecode verification fails before execution:

```text
verify: FAIL: stack underflow at pc 30: needs 1, has 0
```

(the same construct inside the larger `pair_demo` draft produced
`verify: FAIL: stack depth mismatch at pc 45: predecessor expects 0, got 1`
— the exact message varies with the surrounding code.)

**Expected**: `1` — an `if`/`else` is an expression (SPEC §14) and may be the
final expression of a `choose` arm's block like anywhere else.

**Characterization**:

- Enum scrutinee + an arm whose block's final expression is `if`/`else` → fails.
- Scalar (int) scrutinee with `otherwise` + the same `if`/`else`-ending arm →
  compiles and runs correctly (probed separately).
- `if`/`else` inside a *non-final* statement position of the arm, or in a
  plain function body, is unaffected.

**Suspected area**: lowering (`lower/`) — the enum-scrutinee `choose` path
parks the scrutinee across payload checks (0.0.7 T06, commit `0ac696b` "keep
the choose scrutinee parked across a payload mismatch"); the arm-local
`if`/`else` join appears to desynchronize the stack discipline the verifier
expects on that path (`codegen` block-exit expectations). Start by diffing
the MIR/bytecode of the failing arm against the scalar-scrutinee variant.

**Fix plan**: regression fixture first (`tests/e2e/valid/choose_arm_if_else.fln`
+ `.expected`, and a typeck/codegen unit test), then fix lowering, then run
the full suite.

---

## BUG-2 — a bare name may appear only once within one pattern (zero-payload variants occupy the binding namespace)

**Severity**: expressiveness wart, not a crash. Currently worked around with
`otherwise`; documented as a rule in `lang_docs/0.0.7/{zh,en}` ch. 06/15.

**Repro**:

```fleen
enum Option<T> { Some(T), None }
enum Pair<A, B> { P(A, B) }

func main(): int {
    p: Pair<Option<int>, Option<int>> = Pair.P(Option.None, Option.None);
    choose p {
        when P(None, None) { print("empty"); }   // <- rejected
        when P(Some(z), Some(w)) { print("both"); }
        when P(Some(z2), None) { print(z2 as string); }
        when P(None, Some(w2)) { print(w2 as string); }
    }
    0
}
```

**Observed**:

```text
resolve: duplicate declaration of `None` in same scope
```

**Expected** (open decision): either

- (a) zero-payload variant patterns are recognized as variant references and
  do not enter the pattern's binding namespace — `when P(None, None)` compiles
  and matches; **or**
- (b) the rule stays ("one bare name per pattern") and is promoted from a
  bug to a documented, tested language rule (the docs currently describe the
  observed behavior; a decision entry would be needed first per
  `docs/0.0.7/PLAN.md` §1 discipline).

**Characterization**:

- The same bare name repeated **across arms** is fine
  (`when P(Some(v), None) { … } when P(None, Some(v)) { … }` compiles).
- Only a repetition **within one pattern** trips the check — the resolver's
  duplicate-binding rule fires (`resolver/mod.rs`, `DuplicateBinding`), which
  implies zero-payload variant patterns currently register the variant name
  as a pattern binding.

**Suspected area**: resolver pattern-binding registration
(`resolver/mod.rs`) — a variant pattern with no payload sub-patterns should
resolve as a variant reference (possibly shadow-sensitive) instead of
declaring a binding; the parser's classification of bare identifiers in
patterns (`parser/`) is the first place to look.

**Fix plan**: decide (a) vs (b) in PLAN §1 first; if (a), regression fixture
(`tests/e2e/valid/` zero-payload variant repeated in one pattern) before the
resolver change.
