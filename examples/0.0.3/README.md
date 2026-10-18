# 0.0.3 feature tour — a `flint`/project-mode example

A small multi-file Fleen **project** that touches every 0.0.3 feature.

## Run

```bash
# one-stop driver (module system + std, no manifest needed)
cargo fln --project examples/0.0.3/tour

# or through flint (V08)
cd examples/0.0.3/tour
cargo run -q -p flint-cli -- run

# pass program arguments through to std.os.args
cargo fln --project examples/0.0.3/tour alpha beta
```

## Features exercised

| Feature | Ticket | Where |
|---------|--------|-------|
| `struct` declaration / literal / field read+write, `Copy` struct | V02 | `shapes.make_point`, `p.y = 8` |
| `[T]` arrays: literal, index read/write, deep `clone` | V03 | `a = [1, 2, 3]; a[0] = 10; b = clone a` |
| `for` over an array | V04 | `for n in a { … }` |
| `for` over a `Range` (`range_from`) | V04 / V06 | `for i in range_from(1, 6) { … }` |
| Modules + `pub` visibility, nested module path | V05 | `shapes.…`, `shapes.tools.gcd` |
| `std` library: `io`, `str`, `arr`, `os`, `collection.range` | V06 | `std.arr.push`, `std.str.concat`, `std.os.args` |
| `std.collection.iter` (`map`/`filter`/`fold`/`take`) | V07 | `std.collection.iter.…` |
| `flint` package manager (`flint.toml`) | V08 | `flint.toml` (manifest) |

## Layout

```
tour/
├── flint.toml            # package manifest (name is the crate's module root)
└── src/
    ├── main.fln          # crate root: imports the modules + std, drives the tour
    └── shapes/
        ├── mod.fln       # `shapes` module: pub struct Point + pub funcs
        └── tools.fln     # `shapes.tools`: accessed by qualified path
```

> Note: a struct **literal** needs an unqualified identifier (`Point { … }`),
> so cross-module construction goes through a `pub` constructor
> (`shapes.make_point(x, y)`) rather than a qualified `shapes.Point { … }`.
