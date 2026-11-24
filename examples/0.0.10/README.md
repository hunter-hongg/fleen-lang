# 0.0.10 demo — `Result` declared in std + string interpolation

Two single-file programs exercising the 0.0.10 feature set (PLAN D1–D7). No
bytecode change this milestone (D7): the version stays **v8**, no new opcodes,
no table changes — both files run on the unchanged 0.0.9 VM.

- **`result_in_std.fln`** — the `Result` declaration's provenance and flows
  (TICKETS.md T08; D1–D3):
  1. **Declaration provenance (D1)** — `Result` / `Ok` / `Err` are available in
     every file with no import (the prelude trio, backed by the language-level
     `std/result.fln` declaration); `import std.result;` imports the very same
     names, so this file shows both spellings side by side.
  2. **`?` propagation (D3)** — the exact 0.0.2 lowering: an `Err` short-circuits
     the whole function, an `Ok` yields its payload (`safe_div`, `ratio`).
  3. **choose-on-Result (D3 + 0.0.9 D2)** — the general `otherwise` tail catches
     both a failed `Ok`-arm guard and an `Err` value (`describe`); the classic
     exhaustive `Ok` + `Err` pair needs no `otherwise` (`show`).
  4. **Non-shadowable (D2)** — naming a binding/func/struct/enum `Ok`, `Err`, or
     `Result` is a `ReservedBuiltinName` error; this is the one 0.0.10 behavior
     change. It has **no runtime demo here** (it cannot compile) — see the
     `tests/e2e/invalid/reserved_builtin_*` fixtures for the error shapes.
- **`interp_demo.fln`** — string interpolation (D4–D6):
  1. **Every D5-convertible type** — `int` / `float` / `bool` stringify via the
     `as string` path, `string` embeds verbatim.
  2. **`\$` escape (D4)** — a literal `$`; a bare `$` before a non-`{` is
     already literal (`"just a dollar: $"` is unchanged from 0.0.9).
  3. **Full expressions** — `${…}` embeds a call (`double(21)`), arithmetic
     (`n + 8`), and a nested interpolating string.
  4. **Copy semantics (D5)** — a string binding used in several interpolations
     and a plain `print` survives all of them (read-context deep-copies, never
     moves).

## Run

```bash
cargo fln examples/0.0.10/result_in_std.fln
cargo fln examples/0.0.10/interp_demo.fln
```

Output is fully deterministic — no timers, no concurrency, no randomness.
The `cargo test` examples sweep (`fleen-vm/tests/examples.rs`) compiles and
runs both on every test run.

## Features exercised

| Feature | Decision | Where |
|---------|----------|-------|
| `Result` declared in std; prelude trio available with no import | D1 | `result_in_std` §1 |
| `import std.result;` imports the same `Ok`/`Err`/`Result` | D1 | `result_in_std` header |
| `?` propagation unchanged | D3 | `result_in_std` §2 `safe_div` / `ratio` |
| choose-on-Result with `otherwise` (guards fall through) | D3 / 0.0.9 D2 | `result_in_std` §3 `describe` |
| Exhaustive `Ok` + `Err` pair | D3 | `result_in_std` §3 `show` |
| `${expr}` for `int`/`float`/`bool` (`as string`) and `string` (verbatim) | D5 | `interp_demo` §1 |
| `\$` escape (literal `$`); bare `$` before non-`{` is literal | D4 | `interp_demo` §2 |
| Full expressions in `${…}` (calls, arithmetic, nested strings) | D4 | `interp_demo` §3 |
| String embed is deep-copied, reusable | D5 | `interp_demo` §4 |

The static rejections have no runtime demo here: `ReservedBuiltinName`
(shadowing the trio), `UnclosedInterpolation` (EOF inside `${…}`),
`InvalidEscape`, `InterpUnsupportedType` (a non-D5 type in `${…}`), and
`ComplexGlobalInit` (a global initializer cannot interpolate yet). See the
`tests/e2e/{valid,invalid}` `interp_*` / `dollar_*` / `reserved_builtin_*` /
`result_*` fixtures for those error shapes.

## Files

```
0.0.10/
├── README.md            # this file
├── result_in_std.fln    # Result provenance, `?`, choose-otherwise (T08)
└── interp_demo.fln      # interpolation: D5 types, `\$`, exprs, copy semantics (T08)
```
