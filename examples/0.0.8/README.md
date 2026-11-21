# 0.0.8 demos — per-file feature flags + currying & partial application

Two programs that exercise the 0.0.8 feature set (PLAN D1–D16, codename
**Cheeky Hamster 🐹**). Everything shown is behind the per-file
`#feature curry` directive (D1); a file without the directive behaves
exactly as 0.0.7 (D16).

- **`curry_demo.fln`** — a single-file flag-on demo (TICKETS.md T10):
  two-part curried declarations (including a two-part `main`), curried
  arrow types (`int -> int -> int`, a grouped higher-order parameter
  `(int -> int) -> int -> int`), implicit partial application (D5),
  partials bound but never discarded (D10), Copy partials reused (D11),
  and a partial flowing through flag-off std `map` (D15).
- **`curry_tour/`** — a multi-module **project** (TICKETS.md T10):
  the flag-on `main.fln` imports the flag-off `helpers.fln` module and
  partially applies its ordinary `func` exports — partial application is
  a **call-site** rule (D5), so the importing file's flag decides, while
  the flag-off module itself could not do the same (D16). A partial also
  flows into the flag-off higher-order helper `twice(f: (int) -> int, x)`
  and is called there (D15, T08's copy-soundness property cross-module).

## Run

```bash
# the single-file demo
cargo fln examples/0.0.8/curry_demo.fln

# the cross-flag project tour (one-stop driver)
cargo fln --project examples/0.0.8/curry_tour

# or through flint
cd examples/0.0.8/curry_tour
cargo run -q -p flint-cli -- run

# the tests/ e2e case (`flint test`)
cd examples/0.0.8/curry_tour
cargo run -q -p flint-cli -- test
```

Both outputs are fully deterministic — no timers, no concurrency, no
randomness. The `cargo test` examples sweep
(`fleen-vm/tests/examples.rs`) compiles and runs both on every test run.

## Features exercised

| Feature | Decision | Where |
|---------|----------|-------|
| Per-file `#feature curry` directive, header region only | D1 | both files |
| Two-part curried declarations (`add: int -> int -> int` + `add(a, b) = a + b`), `func` forbidden in the flag-on file | D2/D3 | `curry_demo`, `curry_tour/src/main.fln` |
| Curried arrow types: chains, right-associativity, grouped param `(int -> int)`, zero-param `() -> unit` for `main` | D4 | `curry_demo`, `main.fln` |
| Implicit partial application at the call site (`add(10)` is an `int -> int` value) | D5 | both |
| Cross-flag call-site rule: a flag-on main partially applies flag-off imports; flag-off code keeps `ArityMismatch` | D5/D16 | `curry_tour` §2 |
| A partial flows into flag-off code and is called there (std `map`, `helpers.twice`) | D15 | `curry_demo` §3; `curry_tour` §3 |
| Partials are Copy — the same partial serves several calls | D11 | `curry_demo` §5 |
| No discarded partials: every partial is bound or passed as an argument | D10 | both |

## Known limitation — fixed in the release (corrected 0.0.9 T06)

A partial over an **inline-lowered builtin** (e.g. `range_from(0)`) lowered
correctly (the D8 reroute) but originally failed at runtime with
`UnknownBuiltin` — the recorded T08 gap (`docs/0.0.8/TICKETS.md`, T09
deviations). Commit `8dbbc8b`, inside the `v0.0.8` tag, closed it: captured
inline-lowered builtins run end to end (full record: the CHANGELOG
`[0.0.8]` entry). The demos still apply **user
functions** (local or imported) and call std at full arity — that is a
pedagogical choice, not a workaround.

The static rejections also have no runtime demo here: `func` in a flag-on
file (`FuncFormForbiddenInCurryFile`), a partial of a
`ref`/`ptr`/`unsafe` function (D9), a too-few-args call in a flag-off file
(unchanged `ArityMismatch`), a discarded partial, `UnknownFeature`,
`DuplicateFeature`, `FeatureDirectiveMisplaced`. See the
`tests/e2e/{valid,invalid}` `curry_*` fixtures and the
`tests/e2e/projects/curry_*` fixtures for those error shapes.

## Files

```
0.0.8/
├── curry_demo.fln           # single-file flag-on demo (T10)
├── README.md                # this file
└── curry_tour/              # cross-flag project (T10)
    ├── flint.toml           # manifest (flint)
    ├── src/
    │   ├── main.fln         # flag-on: two-part declarations, partials of imports
    │   └── helpers.fln      # flag-off: ordinary `func` exports, no directive
    └── tests/
        ├── curry_case.fln       # `flint test` case (standalone by contract)
        └── curry_case.expected  # expected stdout ("17\n7\n100")
```
