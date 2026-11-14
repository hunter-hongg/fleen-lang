# 0.0.7 demos — parameterized enums + pattern-matching completion

Two programs that exercise the 0.0.7 feature set (PLAN D1–D10):

- **`option_demo.fln`** — a single-file demo covering D1/D2/D4/D5/D6/D7 end
  to end (TICKETS.md T09).
- **`tour/`** — a small multi-module **project** (TICKETS.md T10): the
  `containers` module declares the parameterized enums, `main.fln` runs seven
  numbered demos, and `tests/` carries a `flint test` case.

## Run

```bash
# the single-file demo (module system + std, no manifest needed)
cargo fln examples/0.0.7/option_demo.fln

# the project tour (one-stop driver)
cargo fln --project examples/0.0.7/tour

# or through flint
cd examples/0.0.7/tour
cargo run -q -p flint-cli -- run

# the tests/ e2e case (`flint test`)
cd examples/0.0.7/tour
cargo run -q -p flint-cli -- test
```

Both outputs are fully deterministic — no timers, no concurrency, no
randomness.

## Features exercised

| Feature | Decision | Where |
|---------|----------|-------|
| `enum Option<T>` — type constructor, fully-applied use sites (`Option<int>`) | D1 | `option_demo` §1; `tour/src/containers.fln` |
| Argument-dependent `is_copy`: `Option<int>` Copy vs `Option<string>` owned; `Pair[int, int]` Copy vs `Pair[int, string]` owned | D2 | `option_demo` §1/§2; `tour` §1/§2/§4 |
| Zero-payload variant needs its parameter fixed by context (`Option.None` alone cannot infer `T`) | D1 | `containers.none_int`; `tour` §3/§6 |
| Multi-payload destructuring `when P(a, b)` — every payload is bindable | D4 | `option_demo` §4; `tour` §4 |
| Nested patterns `Ok(Some(x))` / `Some(P(a, b))` | D5 | `option_demo` §3; `tour` §5 (`describe`) |
| Or-patterns `1 \| 2` and `Some(v) \| Other(v)` (same binding name + type) | D6 | `option_demo` §5; `tour` §6 |
| Type guards `when v: int` (compile-time ascription; a scalar scrutinee keeps `otherwise`) | D7 | `option_demo` §6; `tour` §7 |
| Both use-site spellings: `Option<int>` (angle) and `Pair[int, string]` (bracket) | D9 | `pair_demo.fln`; `containers.fln` signatures |

The **static rejections** have no runtime demo here: the recursive-parameter
ban (`enum List<T> { Cons(T, List<T>) }` → `RecursiveEnum`, D3), a bare
`Option` in type position (`TypeConstructorNeedsArgs`), pattern arity
mismatches, or-pattern binding mismatches, and type-guard mismatches. See
the `tests/e2e/invalid/{option_needs_args,option_arity,recursive_param_enum,or_pattern_binding_mismatch,type_guard_mismatch}`
fixtures for the error shapes.

## Files

```
0.0.7/
├── option_demo.fln              # single-file demo (T09)
├── README.md                    # this file
└── tour/                        # multi-module project (T10)
    ├── flint.toml               # manifest (flint)
    ├── src/
    │   ├── main.fln             # the seven numbered demos
    │   └── containers.fln       # `containers` module: pub parameterized enums + pub funcs
    └── tests/
        ├── enum_param_case.fln      # `flint test` case (standalone by contract)
        └── enum_param_case.expected # expected stdout ("7\n-1")
```

> Note (T10 as-built): inside a type argument the enum name is still
> module-qualified — `Result<containers.Option<int>, string>`, not
> `Result<Option<int>, string>`; and `[int]` arrays are owned, so passing
> one to a function consumes it (`containers.find(move xs, 15)`).
