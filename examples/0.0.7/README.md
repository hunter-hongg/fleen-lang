# 0.0.7 demo — `option_demo` (parameterized enums)

A single-file Fleen program that exercises 0.0.7's parameterized enums and the
pattern-matching additions end to end (TICKETS.md T09).

## Run

```bash
cargo fln examples/0.0.7/option_demo.fln
```

## What it shows

| Feature | Decision | Where |
|---------|----------|-------|
| `enum Option<T>` — type constructor, fully-applied use sites (`Option<int>`) | D1 | §1, `unwrap_or` |
| Argument-dependent `is_copy`: `Option<int>` Copy vs `Option<string>` owned | D2 | §1 vs §2 |
| Nested pattern `Ok(Some(x))` inside the builtin `Result` | D5 | §3 |
| Multi-payload destructuring `when Some(P(m, n))` | D4 | §4 |
| Or-pattern `Some(v) \| Other(v)` with a consistent binding type | D6 | §5 |
| Type guard `when v: int` (compile-time ascription) | D7 | §6 |

The output is fully deterministic. The recursive-enum ban (D3) and the bare-name
error (`Option` without arguments is `TypeConstructorNeedsArgs`) are pinned by
the e2e fixtures under `tests/e2e/invalid/` instead.
