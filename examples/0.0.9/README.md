# 0.0.9 demo — the small-repairs milestone

One single-file program exercising the 0.0.9 feature set (PLAN D1–D7;
codename **Mending Weaverbird 🪡**). No bytecode change this milestone (D6):
everything here runs on the 0.0.8 v8 VM rules, and a file that encodes no
partial call stays bytecode-v7.

- **`repairs_demo.fln`** — the four repair fronts in one `main`:
  1. **BUG-2 fix (D1/D7)** — a zero-payload variant pattern is a variant
     reference: `when P(None, None)` compiles (0.0.7 rejected the second
     `None` as a duplicate *binding*). The demo classifies all four shapes of
     `Pair(Opt, Opt)` exhaustively, no `otherwise` needed.
  2. **Result-`choose` completion (D2)** — a guarded `Ok` arm plus a general
     `otherwise` tail: `parse_age` parses with `std.str.parse_int`, keeps
     non-negative successes, and lands negative parses *and* `Err` values in
     `otherwise` (the retired `ChooseResultNeedsOkErrArms` rule no longer
     forces an `Err` arm).
  3. **`break` / `continue` (D4)** — both loop forms in `count_skips`: the
     `while` and the `for` each skip `2` via `continue` and exit at `4` via
     `break`, binding to the innermost loop.
  4. **std.str batch (D3)** — the remaining seven intrinsics: `parse_float`,
     `trim`, `contains`, `starts_with`, `ends_with`, `index_of` (Unicode
     scalar-value positions: `"héllo"` finds `"l"` at `2`), `replace`;
     `parse_int` ran in §2. Failure is an `Err` value extracted with
     `choose`, never a crash.

## Run

```bash
cargo fln examples/0.0.9/repairs_demo.fln
```

Output is fully deterministic — no timers, no concurrency, no randomness.
The `cargo test` examples sweep (`fleen-vm/tests/examples.rs`) compiles and
runs it on every test run.

## Features exercised

| Feature | Decision | Where |
|---------|----------|-------|
| Zero-payload variant patterns are variant references (BUG-2) | D1/D7 | §1 `classify` |
| Result-`choose` accepts `otherwise` / general arms; `ChooseResultNeedsOkErrArms` retired | D2 | §2 `parse_age` |
| A guarded `Ok` arm falls through to the `otherwise` tail | D2 | §2 |
| `break` / `continue` reserved words, innermost-loop binding, both loop forms | D4 | §3 `count_skips` |
| std.str intrinsics: `parse_int` / `parse_float` / `trim` / `contains` / `starts_with` / `ends_with` / `index_of` / `replace` | D3 | §2 + §4 |

The static rejections have no runtime demo here: `break`/`continue` outside
a loop or across a function/`async` boundary (`BreakOutsideLoop` /
`ContinueOutsideLoop`), a repeated *binding* name in one pattern
(`DuplicateBinding`), cross-position coverage holes (non-exhaustive
`choose`). See the `tests/e2e/valid/{loop_*,pair_zero_payload_variants,
result_choose_otherwise*,std_str_batch}` fixtures and the invalid set for
those error shapes.
