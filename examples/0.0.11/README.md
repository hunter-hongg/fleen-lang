# 0.0.11 demo — the `map` type + `std.map` intrinsics

One single-file program exercising the 0.0.11 feature set (PLAN D1–D8,
codename **Cartographer Ant 🗺️**). Bytecode v9 by the content-driven rule
(D7): this file encodes `NewMap` (0xAF), so it is one of the modules that
reaches v9 — every map-free module (including every `std` module) stays v8.

- **`tour/`** — a multi-module **project** walking the whole 0.0.9 → 0.0.11
  span in nine sections: zero-payload variant patterns and Result-`choose`
  `otherwise` (0.0.9 D1/D2), `break` / `continue` and the std.str batch
  (0.0.9 D3/D4), the Result prelude trio + `?` and interpolation details
  (0.0.10 D1–D5), and maps end to end (0.0.11 D1–D8). See `tour/README.md`
  for the section-by-section map.

- **`map_demo.fln`** — the `map` feature end to end (TICKETS.md T08):
  1. **Literals + access (D1/D3/D4)** — `map { k -> v, … }`, `m[k]` riding
     the existing `index_suffix` and returning `Result[V, string]` (the
     missing key is a value, not a crash: `m[99]` → `Err("key not found")`).
  2. **The intrinsic batch (D3)** — `std.map.len` / `contains` / `keys` —
     the 0.0.9 `std.str` template: eight helpers, zero new opcodes.
  3. **Ownership (D2)** — `insert` / `remove` are FUNCTIONAL: they consume
     their input map and return a fresh one (`std.map.insert(clone m, …)`
     keeps the original alive; a bare `std.map.insert(m, …)` consumes `m`).
  4. **Chained access (D4)** — `nested["row"]["x"]`: the second `[…]` rides
     on the `Result[map[…], string]` the first produced; `?` propagates the
     miss (`chained()` helper returns `Result<int, string>`).
  5. **Key whitelist (D5)** — keys are `int` / `bool` / `string` only.

Run it:

```sh
cargo fln examples/0.0.11/map_demo.fln
```

Expected output is pinned in `map_demo.expected` (checked by the e2e
examples test). The e2e fixtures `tests/e2e/valid/map_basics.fln` and
`tests/e2e/valid/map_std_intrinsics.fln` cover the same rules at the
fixture level, plus the `tests/e2e/invalid/map_*.fln` error set.

## Files

```
0.0.11/
├── README.md          # this file
├── map_demo.fln       # the map feature end to end (T08)
├── map_demo.expected  # pinned stdout
└── tour/              # the 0.0.9 → 0.0.11 feature tour (project mode)
    ├── README.md
    ├── flint.toml
    ├── src/           # main + audit / intake / ledger
    └── tests/         # tour_case.fln + .expected (flint test)
```
