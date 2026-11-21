# Fleen 0.0.9 · Mending Weaverbird 🪡

Small repairs: the milestone that mends instead of opens a new front. One real
bug fix (BUG-2), one long-deferred semantic completion (Result-`choose`
`otherwise`), two control-flow keywords (`break` / `continue`), and one batch
of eight std.str intrinsics. No bytecode change (D6): the version stays v8, no
new opcodes, no table changes.

Decisions: `docs/0.0.9/PLAN.md` (D1–D7). Tickets: `docs/0.0.9/TICKETS.md`
(T01–T07). Breaking set: `break` / `continue` become reserved words (the
0.0.5/0.0.6 precedent for reserved-word additions).

## Highlights

- **Zero-payload variant patterns are variant references (BUG-2 fix, D1/D7)** —
  inside a pattern a bare identifier naming a zero-payload variant no longer
  occupies the binding namespace: `when P(None, None)` compiles and matches
  (0.0.7 rejected the second `None` as a duplicate *binding*). Repeated
  binding names still error (`DuplicateBinding`); lookup order is
  same-pattern earlier bindings → enclosing locals → zero-payload variant →
  unresolved. Exhaustiveness gains a strictly-additive second disjunct (D7):
  guard-free arms covering one variant jointly are accepted via constructor
  splitting — every program exhaustive under 0.0.7 stays exhaustive, and
  cross-position coverage holes (`P(Some, None)` + `P(None, Some)`) are still
  rejected.
- **Result-`choose` accepts `otherwise` / general arms (D2)** — the 0.0.2-era
  "deferred to U07" branch is deleted: `otherwise` occupies the attempt-chain
  tail slot, or-pattern arms route per alternative, and the
  `ChooseResultNeedsOkErrArms` exhaustiveness rule is retired (typeck's
  `ChooseNotExhaustive` governs, as for every other scrutinee type). Guarded
  `Ok` arms fall through to the `otherwise` tail.
- **`break` / `continue` (D4)** — reserved-word statements (unit-valued, not
  expressions — `x = break` is a parse error), binding to the innermost
  enclosing `while` / `for` (both skeletons). Lowered to deferred jumps to the
  loop exit / continue edges; `for` splits out the index-advance block only
  when the body records a `continue` (content-driven layout — `continue`-free
  bodies stay byte-identical). `BreakOutsideLoop` / `ContinueOutsideLoop`
  reject top level, loop-free bodies, `choose` arm bodies, and every
  function/`async` boundary. No labeled break, no `break <value>`.
- **std.str intrinsic batch (D3)** — eight VM intrinsics, zero opcodes:
  `parse_int` / `parse_float` (`Result`-valued parsers; overflow and
  non-numeric input are `Err` values, never crashes), `trim`, `contains`,
  `starts_with`, `ends_with`, `index_of` (not-found is `Err`; positions count
  Unicode scalar values), `replace` (an empty pattern is a no-op —
  deliberately not Rust's boundary insertion).
- **Result-`choose` dispatch spill (found and fixed at the release gate)** —
  the example sweep surfaced a latent lower bug (present since 0.0.7's
  attempt-chain generalization): two general-path Result `choose`s in ONE
  function body failed bytecode verification (`StackDepthMismatch`). The
  dispatch now spills before its conditional (a depth-preserving `Jump`; a
  no-op for the common single-choose shape). Regression fixtures:
  `tests/e2e/valid/result_choose_otherwise_twice.fln` plus the lower/verify
  `t07_*` tests.

## E2E / examples

- `examples/0.0.9/repairs_demo.fln` — the single-file tour over all four
  repair fronts (BUG-2 pattern spelling, guard + `otherwise`, `break` /
  `continue` in both loop forms, the eight std.str calls) with a README in
  the 0.0.7/0.0.8 layout; picked up automatically by the `cargo test`
  examples sweep.
- New/updated fixture families: `pair_zero_payload_variants`,
  `result_choose_otherwise`, `result_choose_otherwise_twice`,
  `loop_break_*` / `loop_continue_*` / `loop_nested_innermost`,
  `std_str_batch`, plus the invalid set (outside-loop
  break/continue, repeated binding names, non-exhaustive choose, parse
  overflow / index-of not-found `Err` paths).

## Breaking changes

`break` and `continue` are reserved words — programs using them as
identifiers must rename (the only source breaking set; every other behavior
is additive).

## Known limitations

None new. The two 0.0.7 post-release findings and the 0.0.8 captured-builtin
gap were already fixed inside the `v0.0.8` tag (`a5b8bd7` / `8dbbc8b`); the
stale docs wording was corrected by T06.

## Artifacts

The release workflow (`.github/workflows/release.yml`, triggered by pushing
the `v0.0.9` tag) builds and publishes these assets:

| Asset | Platform |
|---|---|
| `fleen-0.0.9-x86_64-linux-gnu.tar.gz` | Linux x86_64 (glibc) |
| `fleen-0.0.9-x86_64-linux-musl.tar.gz` | Linux x86_64 (musl) |
| `fleen-0.0.9-aarch64-linux.tar.gz` | Linux aarch64 (glibc) |
| `fleen-0.0.9-x86_64-macos.tar.gz` | macOS Intel |
| `fleen-0.0.9-aarch64-macos.tar.gz` | macOS Apple Silicon |
| `fleen-0.0.9-x86_64-windows.zip` | Windows x86_64 |

Each archive ships `fleen-vm`, `fleen-compiler`, `fleen-verify`, `flint` and
the version-pinned `std/` tree. (`tar.gz`/`zip` binaries are gitignored under
`release/`.)

## Docs

- `docs/0.0.9/PLAN.md` — decisions D1–D7
- `docs/0.0.9/TICKETS.md` — T01–T07
- `docs/DESIGN.md` §6 / §17 / §18 — the `break`/`continue` subsection, the
  0.0.9 matrix column and roadmap
- `docs/BYTECODE.md` — no format change (v8, content-driven rule unchanged)
- `docs/SYNTAX.ebnf` — the "New in 0.0.9" grammar-notes block
- `examples/0.0.9/repairs_demo.fln` — the runnable tour
- `CHANGELOG.md` `[0.0.9]` — the full entry
