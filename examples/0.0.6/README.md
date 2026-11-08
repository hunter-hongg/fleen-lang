# 0.0.6 feature tour — `enum` (typed variants) + the 0.0.6 std additions

A small multi-file Fleen **project** that touches every 0.0.6 feature
(PLAN F1–F5).

## Run

```bash
# one-stop driver (module system + std, no manifest needed)
cargo fln --project examples/0.0.6/tour

# or through flint
cd examples/0.0.6/tour
cargo run -q -p flint-cli -- run

# the tests/ e2e case (`flint test`)
cd examples/0.0.6/tour
cargo run -q -p flint-cli -- test
```

The program's output is fully deterministic — no timers, no concurrency, no
randomness.

## Features exercised

| Feature | Ticket | Where |
|---------|--------|-------|
| `enum` declaration: ordered variants, typed payloads, `pub` visibility | F1 | `shapes.Color`, `shapes.Shape`, `shapes.Pair`, `shapes.Label` |
| Constructor expressions: bare (`Color.Red`) and dotted (`Shape.Circle(2.0)`) | F2 | §1, §2 of `main.fln` |
| Multi-payload variant (`Pair.Two(3, 4)`) — the constructor takes both payloads | F2 | §3 |
| Variant patterns in `choose` (`when Red { … }`, `when Circle(r) { … }`) | F3 | `shapes.rank`, `shapes.area`, `shapes.text` |
| Exhaustiveness: one arm per variant is exhaustive — no `otherwise` | F3 | `shapes.rank` |
| Copy vs owned classification: a Copy payload keeps the enum Copy, a `string` payload makes it owned (`move`) | F1 (D1) | §1 vs §4 |
| `std.arr.concat` / `std.arr.flatten` (one level) | F5 | §5 |
| `std.collection.iter.foldl` / `foldr` (the `fold` → `foldl` rename) | F5 | §6 |

The **static rejections** have no runtime demo here: a recursive enum
(`RecursiveEnum`), a non-exhaustive `choose` (`ChooseNotExhaustive`), `as
string` on an enum, and `==` on an enum. See `docs/0.0.6/TICKETS.md` and the
`tests/e2e/invalid/enum_*` fixtures for the error shapes.

## Files

```
tour/
├── flint.toml          # manifest (flint)
├── src/
│   ├── main.fln        # the six numbered demos
│   └── shapes.fln      # `shapes` module: pub enums + pub funcs
└── tests/
    ├── enum_case.fln   # `flint test` case: an enum match
    └── enum_case.expected # expected stdout ("stop\n7")
```

> Note: a pattern binds the **first** payload only — 0.0.6 has no
> "read payload i" expression yet, so a two-payload variant matches but only
> its first payload is nameable inside the arm (`when Two(a)`).
