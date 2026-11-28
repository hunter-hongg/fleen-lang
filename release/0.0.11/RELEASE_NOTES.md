# Fleen 0.0.11 · Cartographer Ant 🗺️

The `map` type: `map { k -> v }` literals, `m[k]` access, and the eight-helper
`std.map` intrinsic batch. Bytecode v9 by the **content-driven** rule (D7) —
one new opcode (`NewMap` 0xAF), one new `FieldTypeTag` (`Map = 13`), and a
module reaches v9 only when a `NewMap` was actually encoded; every map-free
module (including every `std` module) stays v8 and is byte-identical to 0.0.10.

Decisions: `docs/0.0.11/PLAN.md` (D1–D8). Tickets: `docs/0.0.11/TICKETS.md`
(T01–T08). Declared breaking set: **`map` becomes a reserved word** (D1).

## Highlights

- **`map` literals (D1/D2)** — `map { 1 -> "a", "k" -> [1, 2] }` builds an
  always-owned `map[K, V]` (the 0.0.3 array posture: assignment/argument
  passing deep-copies; `move` transfers). `map` is a reserved word — a user
  identifier named `map` (variable / function / struct / enum) is now a
  compile error. Duplicate literal keys overwrite in place, one entry.
- **`m[k]` rides the existing index suffix (D4)** — zero parser change. The
  read returns `Result[V, string]` (D3): a missing key is a *value*, not a
  crash (`m[99]` → `Err("key not found")`). Chained `nested["row"]["x"]`
  works — the second `[…]` rides on the `Ok` payload of the first.
- **Key whitelist (D5)** — keys are `{int, bool, string}` only (the
  `==`-comparable set minus `float` / `unit` / `Range`); anything else is
  `MapKeyNotComparable`. No `Future` / `channel` / `ptr` in the `V` slot (D6).
- **The `std.map` batch (D3)** — eight intrinsics, zero new opcodes (the
  0.0.9 `std.str` template): `get` / `insert` / `remove` / `contains` /
  `keys` / `values` / `len` / `clone`. `insert` / `remove` are **functional
  and consuming** (D2): they take the input map by move and return a fresh
  one — `std.map.insert(clone m, …)` keeps the original alive, a bare
  `std.map.insert(m, …)` consumes `m` (reuse is the ordinary
  `UseAfterMove`).
- **Insertion order preserved (D8)** — iteration order (`keys` / `values`)
  follows source insertion order (`HashMap` + order `Vec` + a `KeyHash`
  wrapper).
- **Bytecode v9, content-driven (D7)** — `NewMap` (0xAF) lives in the free
  slot below the 0xB0 v3 block; verify validates its stack shape
  (pop `2n`, push 1) and keeps the opcode out of v1–v8 modules. `.flnc`
  field-tag 13 (`Map`) decodes, so a struct/array holding a map round-trips
  through the project format. The `Map` runtime value is not serialized.

## E2E / examples

- `examples/0.0.11/map_demo.fln` — the feature end to end: literals + access
  (missing key → `Err`), the intrinsic batch, consuming `insert` / `remove`
  with `clone`, chained access, the key whitelist. Single-file demo picked
  up by the `cargo test` examples sweep; README in the 0.0.7–0.0.10 layout.
- Valid fixtures: `map_basics.fln`, `map_std_intrinsics.fln`.
- Invalid fixtures (12 `map_*`): `map_key_float` / `map_key_array` /
  `map_key_enum` / `map_key_range` / `map_key_unit` / `map_key_inconsistent`
  (key whitelist), `map_index_wrong_type`, `map_value_channel` (D6),
  `map_use_after_move` (D2 consuming), `map_as_string`, `map_eq`,
  `map_as_identifier` (the reserved word, D1).

## Breaking changes

`map` is a reserved word (D1): a user identifier named `map` no longer
compiles. No other breaking change — map-free modules are byte-identical to
0.0.10 and the VM/verify accept the v8 format unchanged.

## Known limitations

No generic map type parameters beyond the key whitelist; `map` values are
not `==`-comparable and not `as string`-convertible (like enums). The
`Map` runtime value is not serialized (content-driven D7).

## Artifacts

The release workflow (`.github/workflows/release.yml`, triggered by pushing
the `v0.0.11` tag) builds and publishes these assets:

| Asset | Platform |
|---|---|
| `fleen-0.0.11-x86_64-linux-gnu.tar.gz` | Linux x86_64 (glibc) |
| `fleen-0.0.11-x86_64-linux-musl.tar.gz` | Linux x86_64 (musl) |
| `fleen-0.0.11-aarch64-linux.tar.gz` | Linux aarch64 (glibc) |
| `fleen-0.0.11-x86_64-macos.tar.gz` | macOS Intel |
| `fleen-0.0.11-aarch64-macos.tar.gz` | macOS Apple Silicon |
| `fleen-0.0.11-x86_64-windows.zip` | Windows x86_64 |

Each archive ships `fleen-vm`, `fleen-compiler`, `fleen-verify`, `flint` and
the version-pinned `std/` tree. (`tar.gz`/`zip` binaries are gitignored under
`release/`.)

## Docs

- `docs/0.0.11/PLAN.md` — decisions D1–D8
- `docs/0.0.11/TICKETS.md` — T01–T08
- `docs/DESIGN.md` — the `map` subsection, the 0.0.11 matrix column and roadmap
- `docs/BYTECODE.md` — the v9 note: `NewMap` (0xAF, §5.19), `FieldTypeTag::Map = 13`,
  the content-driven version rule (D7)
- `examples/0.0.11/` — the runnable demo
- `CHANGELOG.md` `[0.0.11]` — the full entry
