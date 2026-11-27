# 0.0.11 demo — the `map` type + `std.map` intrinsics

One single-file program exercising the 0.0.11 feature set (PLAN D1–D8,
codename **Cartographer Ant 🗺️**). Bytecode v9 by the content-driven rule
(D7): this file encodes `NewMap` (0xAF), so it is one of the modules that
reaches v9 — every map-free module (including every `std` module) stays v8.

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
