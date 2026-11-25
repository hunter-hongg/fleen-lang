# Fleen 0.0.10 · Loom Spider 🕸️

Two zero-bytecode features: `Result` becomes a declared std enum (the prelude
trio, non-shadowable), and string literals gain interpolation. No bytecode
change (D7): the version stays v8, no new opcodes, no table changes —
`docs/BYTECODE.md` is untouched.

Decisions: `docs/0.0.10/PLAN.md` (D1–D7). Tickets: `docs/0.0.10/TICKETS.md`
(T01–T08). Breaking set: a **string literal containing the exact sequence
`${`** (previously a literal `$` followed by `{`) — now interpolation.

## Highlights

- **`Result` is declared in std, not a compiler builtin (D1)** —
  `std/result.fln` is the language-level source of truth
  (`pub enum Result<T, E> { Ok(T), Err(E) }`, the ordinary 0.0.7
  parameterized-enum frontend), and the compiler maps `std.result.Result` onto
  its internal representation by canonical path. **Language-level only**: the
  backend (opcodes 0xA0–0xA4, `FieldTypeTag::Result = 6`, the VM
  `Value::Ok` / `Value::Err`) is untouched and no `enum_defs` entry is emitted
  (the module is declaration-only).
- **The trio is non-shadowable (D2)** — `Ok` / `Err` / `Result` are available
  in every file without an import **and** naming a
  binding/parameter/func/struct/enum `Ok`, `Err`, or `Result` is a
  `ReservedBuiltinName` error. This is the milestone's one behavior change: a
  program that declared its own `Result` (allowed ≤0.0.9) now fails to compile.
- **String interpolation `"${expr}"` (D4–D6)** — `${expr}` embeds a full
  expression (recursive; nested interpolating strings work); `int` / `float` /
  `bool` stringify via exactly the `as string` path, `string` embeds verbatim
  (deep-copied, never moved), any other type is `InterpUnsupportedType`, and
  there are **no format specifiers**. Lowering desugars to a `std.str.concat`
  chain, so a literal **without** `${` compiles **byte-identically** to 0.0.9.
- **The `\$` escape (D4)** — the one new escape, producing a literal `$`.
  Without it `$` is special only immediately before `{`, so `"cost: $5"` is
  unchanged. A `$` immediately before `{` now interpolates; write `\$${` to
  keep a literal `$` followed by `{`.
- **`import std.result;` (D1)** — the `Result` module is now a normal declared
  module; the dotted import hands over the same prelude trio (the bare
  `Result` type spelling is unchanged — the dotted `std.result.Result<...>`
  type spelling is not syntax).
- **`?` propagation and choose-on-Result unchanged (D3)** — both keep their
  exact 0.0.2 / 0.0.9 lowering, including the 0.0.9 `otherwise` tail.

## E2E / examples

- `examples/0.0.10/result_in_std.fln` — the `Result` declaration's provenance
  (prelude trio + the explicit import), `?` propagation, and choose-on-Result
  with the general `otherwise` tail; the non-shadowable rule has no runtime
  demo (it cannot compile) — see the `reserved_builtin_*` fixtures.
- `examples/0.0.10/interp_demo.fln` — every D5-convertible type, the `\$`
  escape, full expressions in `${…}` (calls, arithmetic, nested strings), and
  string copy semantics.
- Both are single-file demos picked up automatically by the `cargo test`
  examples sweep (`fleen-vm/tests/examples.rs`); README in the
  0.0.7/0.0.8/0.0.9 layout.
- New/updated fixture families: `interp_*` (`scalar_types`, `embed_call`,
  `string_reuse`, `adjacent_merge`, `brace_interp`, `in_print_args`,
  `in_std_call`), `dollar_elsewhere` / `dollar_escape`, `result_*`
  (`prelude_no_import`, `import_std`, `bracket_type`, `struct_field`,
  `choose_otherwise*`), plus the invalid set (`reserved_builtin_*`,
  `unclosed_interp_*`, `invalid_escape`, `interp_unsupported_*`).

## Breaking changes

A string literal containing the exact sequence `${` now begins an
interpolation (escape the dollar as `\$${` to keep a literal `$` + `{`). The
`Ok` / `Err` / `Result` names are non-shadowable — a program that declares its
own `Result` (or binds `Ok`/`Err`) is now a `ReservedBuiltinName` error. No
other behavior change; `?`, choose-on-Result, and the bytecode are untouched.

## Known limitations

A global initializer cannot interpolate yet (`ComplexGlobalInit`) — interpolate
in a function body. `docs/BYTECODE.md` is untouched (D7: version stays v8).

## Artifacts

The release workflow (`.github/workflows/release.yml`, triggered by pushing
the `v0.0.10` tag) builds and publishes these assets:

| Asset | Platform |
|---|---|
| `fleen-0.0.10-x86_64-linux-gnu.tar.gz` | Linux x86_64 (glibc) |
| `fleen-0.0.10-x86_64-linux-musl.tar.gz` | Linux x86_64 (musl) |
| `fleen-0.0.10-aarch64-linux.tar.gz` | Linux aarch64 (glibc) |
| `fleen-0.0.10-x86_64-macos.tar.gz` | macOS Intel |
| `fleen-0.0.10-aarch64-macos.tar.gz` | macOS Apple Silicon |
| `fleen-0.0.10-x86_64-windows.zip` | Windows x86_64 |

Each archive ships `fleen-vm`, `fleen-compiler`, `fleen-verify`, `flint` and
the version-pinned `std/` tree. (`tar.gz`/`zip` binaries are gitignored under
`release/`.)

## Docs

- `docs/0.0.10/PLAN.md` — decisions D1–D7
- `docs/0.0.10/TICKETS.md` — T01–T08
- `docs/DESIGN.md` §2 / §8 / §9.8 / §9.9 / §11 / §17 / §18 — the interpolation
  subsection, the Result-in-std updates, the 0.0.10 matrix column and roadmap
- `docs/BYTECODE.md` — no format change (v8, content-driven rule unchanged)
- `docs/SYNTAX.ebnf` — the `string_lit` / `string_char` grammar update
- `examples/0.0.10/` — the two runnable demos
- `CHANGELOG.md` `[0.0.10]` — the full entry
