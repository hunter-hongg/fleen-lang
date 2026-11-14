# Fleen 0.0.7 · Mantis Shrimp 🦐

Parameterized types + pattern matching completion: user-defined type constructors
(`enum Option<T>` …, fully applied at every use site), argument-dependent `is_copy`,
full multi-payload destructuring, nested patterns, or-patterns (`p₁ | p₂`), and type
guards (`when v: T`). Bytecode v7 (the `enum_defs` table gains one
`type_param_count: u32` per enum; no new opcodes, no `FieldTypeTag` change).

Decisions: `docs/0.0.7/PLAN.md` (D1–D10). Tickets: `docs/0.0.7/TICKETS.md` (T01–T11).
Non-breaking additive (D10) — `Result[T, E]` stays builtin (D9); the `Result` → std
migration is deferred to 0.1.0.

## Highlights

- **Parameterized enums (D1/D2)** — an `enum` head may declare type parameters;
  every use site must be **fully applied** (`Option<int>`, `Pair[int, string]`; a bare
  `Option` in type position is `TypeConstructorNeedsArgs`, too many args is
  `ArityMismatch`). Both angle (`<>`) and bracket (`[]`) spellings are accepted at use
  sites (D9, the `Result[T, E]` style). `is_copy(Option<T>)` is **argument-dependent**
  (D2): `Option<int>` is Copy, `Option<string>` is owned (needs `move` / `clone`).
  Recursive parameterized enums stay rejected (`enum List<T> { Cons(T, List<T>) }` is
  `RecursiveEnum`, D3).
- **Recursive patterns (D4–D7)** — full multi-payload destructuring
  (`when Rect(w, h)`), nested patterns (`when Ok(Some(x))`), or-patterns
  (`when 1 | 2 | 3`; alternatives must bind the same names with the same types,
  `OrPatternBindingMismatch`), and compile-time type guards (`when v: int if v > 100`).
  Exhaustiveness treats an or-pattern as the union of its alternatives' coverage and is
  payload-aware for parameterized enums.
- **Bytecode v7 (D8)** — writers emit version 7, readers accept `{1, 2, 3, 4, 5, 6, 7}`;
  `enum_defs` gains one descriptive `type_param_count: u32` per enum; **no new opcodes**,
  no `FieldTypeTag` change — the v1–v6 layout is byte-identical otherwise.
  `fleen-verify`/VM need no further changes (T08).

## Breaking changes

None (non-breaking additive, D10).

## Artifacts

The release workflow (`.github/workflows/release.yml`, triggered by pushing the `v0.0.7`
tag) builds and publishes these assets:

| Asset | Platform |
|---|---|
| `fleen-0.0.7-x86_64-linux-gnu.tar.gz` | Linux x86_64 (glibc) |
| `fleen-0.0.7-x86_64-linux-musl.tar.gz` | Linux x86_64 (musl) |
| `fleen-0.0.7-aarch64-linux.tar.gz` | Linux aarch64 (glibc) |
| `fleen-0.0.7-x86_64-macos.tar.gz` | macOS Intel |
| `fleen-0.0.7-aarch64-macos.tar.gz` | macOS Apple Silicon |
| `fleen-0.0.7-x86_64-windows.zip` | Windows x86_64 |

Each archive ships `fleen-vm`, `fleen-compiler`, `fleen-verify`, `flint` and the
version-pinned `std/` tree. (`tar.gz`/`zip` binaries are gitignored under `release/`.)

## Docs

- `docs/0.0.7/PLAN.md` — decisions D1–D10
- `docs/0.0.7/TICKETS.md` — T01–T11
- `docs/DESIGN.md` §7 / §9.8 — patterns and parameterized types
- `docs/BYTECODE.md` §2 / §9 / §10 — the v7 `enum_defs` layout
- `docs/SYNTAX.ebnf` — `type_params` / `type_args` / the recursive `pattern` grammar
- `examples/0.0.7/tour` and `examples/0.0.7/option_demo.fln` — runnable tours
