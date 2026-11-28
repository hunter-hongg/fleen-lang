# The 0.0.9 → 0.0.11 feature tour

One multi-module **project** that exercises everything the 0.0.9 → 0.0.11
milestones added, woven into a single deterministic warehouse-intake
program (codenames: **Mending Weaverbird 🪡 0.0.9**, **Loom Spider 🕸️
0.0.10**, **Cartographer Ant 🗺️ 0.0.11**). The modules split by theme, so
each file stays readable:

- **`src/audit.fln`** — quality control:
  - 0.0.9 D1/D7 (BUG-2 fix) — zero-payload variant patterns are variant
    *references*: every arm of `classify` names **two** of them in one
    pattern (`when Row(Green, Red)`); 0.0.7 rejected the second name as a
    `DuplicateBinding`.
  - 0.0.9 D2 — a guard on an `Ok` arm falls through to a general
    `otherwise` tail; an `Err` lands there too (`parse_level`).
  - 0.0.10 D1/D3 — `Result` / `Ok` / `Err` used with **no import at all**
    (the prelude trio, `std/result.fln` is the declaration) and the
    unchanged `?` propagation (`level_or`).
- **`src/intake.fln`** — belt scanning:
  - 0.0.9 D4 — `break` / `continue` in both loop forms (`scan` is the
    `for` form, `countdown` the `while` form), innermost binding.
  - 0.0.9 D3 — the eight std.str intrinsics: `parse_int` / `parse_float` /
    `trim` / `contains` / `starts_with` / `ends_with` / `index_of`
    (Unicode scalar-value positions) / `replace`; failure is an `Err`
    value extracted with `choose`, never a crash.
  - 0.0.10 D4/D5 — the report string is assembled with `${…}`
    interpolation over full expressions.
- **`src/ledger.fln`** — the price book:
  - 0.0.11 D1/D5 — `map { k -> v }` literals (`map` is a reserved word;
    keys are int / bool / string).
  - 0.0.11 D3/D4 — `m[k]` rides the existing index suffix and returns
    `Result[V, string]`: the missing key is a value, not a crash
    (`price_of`, `bill` aborts on the first miss through `?`).
  - 0.0.11 D2 — maps are always-owned; `insert` / `remove` are functional —
    they consume their input and return a fresh map, so "keep the original"
    is spelled `clone` at the call site (`restocked` / `without`; callers
    choose `clone` vs `move` deliberately).
  - 0.0.11 D8 — the read-only batch (`len` / `contains` / `keys` /
    `values`) accepts a bare map — the read copies — and `keys` preserves
    insertion order (`inventory`).
  - 0.0.11 D4 — chained access `grid[row][col]`: the second `[…]` rides the
    `Result[map[string, int], string]` the first produced (`bin_of`).
- **`src/main.fln`** — the nine-section walk-through, in tour order:

| § | Milestone | Feature | Decision |
|---|-----------|---------|----------|
| 1 | 0.0.9 | zero-payload variant patterns (BUG-2 fix) | D1/D7 |
| 2 | 0.0.9 | Result-`choose`: guarded `Ok` arm + general `otherwise` | D2 |
| 3 | 0.0.10 | the prelude trio (no import) + unchanged `?` | D1/D2/D3 |
| 4 | 0.0.9 | `break` / `continue`, both loop forms | D4 |
| 5 | 0.0.9 | the std.str batch (8 intrinsics) | D3 |
| 6 | 0.0.10 | interpolation details: `\$`, full expressions, nested strings, bare `$` | D4/D5 |
| 7 | 0.0.11 | map literals + `m[k]` → `Result[V, string]` | D1/D3/D4 |
| 8 | 0.0.11 | functional insert/remove + read-only batch, insertion-order keys | D2/D8 |
| 9 | 0.0.11 | chained access `m[k][k]` | D4 |

Interpolation (0.0.10) is also woven into every section's output; §3 in
`main.fln` shows the optional explicit `import std.result;` spelling, which
imports the very same prelude names.

## Run

```bash
# project mode, one command (compile → verify → execute)
cargo fln --project examples/0.0.11/tour

# or through flint
cd examples/0.0.11/tour
cargo run -q -p flint-cli -- run

# the tests/ e2e case
cd examples/0.0.11/tour
cargo run -q -p flint-cli -- test
```

Output is fully deterministic — no timers, no concurrency, no randomness.
The `cargo test` examples sweep (`fleen-vm/tests/examples.rs`) compiles and
runs the project on every test run; `tests/tour_case.fln` + `.expected`
pin the compressed single-file version of the same features.

## Ownership posture (worth reading the source for)

Maps and strings are owned values, so every cross-function hand-off is
explicit — the tour deliberately shows both spellings at call sites:
`clone` where the original stays alive (`ledger.bill(clone book, …)`),
`move` where the value is spent (`ledger.without(move step1, …)`). The
read-only std helpers (`std.map.len` / `keys` / `values` / `contains`,
`std.str.*`) accept a bare argument — the read copies — which is why
`inventory` can index `keys` and then still call `contains` on the same
map (see `std/map.fln`).

## Not shown here (compile errors, pinned by fixtures)

`ReservedBuiltinName` (shadowing `Ok` / `Err` / `Result`),
`UnclosedInterpolation`, `InvalidEscape`, `InterpUnsupportedType`,
`ComplexGlobalInit` (a global cannot interpolate), `break` / `continue`
outside a loop (`BreakOutsideLoop` / `ContinueOutsideLoop`), a repeated
*binding* name in one pattern (`DuplicateBinding`), and the map key/value
whitelist rejections. See the `tests/e2e/{valid,invalid}` fixtures
(`loop_*`, `std_str_*`, `interp_*` / `dollar_*`, `reserved_*`, `map_*`)
for those shapes.

## Files

```
tour/
├── README.md             # this file
├── flint.toml            # manifest (flint)
├── src/
│   ├── main.fln          # the nine-section walk-through
│   ├── audit.fln         # 0.0.9 §1/§2 patterns + 0.0.10 trio / ?
│   ├── intake.fln        # 0.0.9 break/continue + std.str batch
│   └── ledger.fln        # 0.0.11 map, end to end
└── tests/
    ├── tour_case.fln     # `flint test` case (standalone by contract)
    └── tour_case.expected # expected stdout
```
