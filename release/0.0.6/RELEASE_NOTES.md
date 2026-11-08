# Fleen 0.0.6

Tagged unions: `enum` declarations with ordered variants and typed payloads,
generalized `choose` exhaustiveness (the `Result` rule applied to enums, D3),
bytecode v6 (`EnumNew`/`EnumIs`/`EnumPayload` + the `enum_defs` table), and an
array library completion (`concat`/`flatten`, `fold` → `foldl` plus `foldr`).

Decisions: `docs/0.0.6/PLAN.md` (D1–D6). Tickets: `docs/0.0.6/TICKETS.md` (T01–T08).
Breaking set (D6): `enum` is a reserved word; `std.collection.iter.fold` is
renamed to `foldl` and `foldr` is added.

## Highlights

- **`enum` (D1/D3)** — ordered variants with typed payloads (zero- and multi-payload),
  variant constructors (`Color.Red`, dotted `EnumName.Variant`; a bare payload variant
  without `()` is not a constructor). `choose` over an enum is exhaustive when it has
  one arm per variant (or an `otherwise` arm); a missing variant is
  `ChooseNotExhaustive` (`choose is not exhaustive for type \`Color\`: missing Blue`).
  `is_copy(E)` = all payloads of all variants are Copy (the struct rule); an owned enum
  is consumed by `choose` (`choose move s { … }`). 0.0.6 patterns bind the **first
  payload only** (a multi-payload variant can be constructed but not fully destructured).
- **Safety/type rules (D1/D2)** — recursive enums are rejected at typeck
  (`RecursiveEnum`: "an enum's size must be statically known; break the cycle with `box`
  indirection once it exists"). Enums are not comparable (`==` is a type error) and are
  outside the 0.0.2 `as string` cast whitelist.
- **Bytecode v6 (D4)** — `EnumNew` (0xCB, u16 enum id + u16 variant id),
  `EnumIs` (0xCC), `EnumPayload` (0xCD); the `Value::Enum { variant, payload }` runtime
  value (not serialized); the appended `enum_defs` table and `FieldTypeTag::Enum = 12`.
  Writers emit version 6, readers accept `{1, 2, 3, 4, 5, 6}`. `enum_defs` is appended
  after `struct_defs` and written only for `version >= 6`, so the v1–v5 layout is
  byte-identical. `fleen-verify` cross-checks ids/arity and gates 0xCB–0xCD out of v1–v5
  modules.
- **Array library completion (D5)** — `std.arr.concat` and `std.arr.flatten` intrinsics;
  `std.collection.iter.fold` → `foldl`, plus `foldr` (eager, builtin-backed).

## Breaking changes (D6)

- `enum` is a **reserved word** — an identifier named `enum` is now a parse error.
- `std.collection.iter.fold` is **renamed to `foldl`**; `foldr` is added.

## Artifacts

The release workflow (`.github/workflows/release.yml`, triggered by pushing the `v0.0.6`
tag) builds and publishes these assets:

| Asset | Platform |
|---|---|
| `fleen-0.0.6-x86_64-linux.tar.gz` | Linux x86_64 (glibc) |
| `fleen-0.0.6-aarch64-linux.tar.gz` | Linux aarch64 (glibc) |
| `fleen-0.0.6-x86_64-macos.tar.gz` | macOS Intel |
| `fleen-0.0.6-aarch64-macos.tar.gz` | macOS Apple Silicon |
| `fleen-0.0.6-x86_64-windows.zip` | Windows x86_64 |
| `fleen-0.0.6-aarch64-windows.zip` | Windows aarch64 |

Each archive ships `fleen-vm`, `fleen-compiler`, `fleen-verify`, `flint` and the
version-pinned `std/` tree. (`tar.gz`/`zip` binaries are gitignored under `release/`.)

## Docs

- `docs/0.0.6/PLAN.md` — decisions D1–D6
- `docs/0.0.6/TICKETS.md` — T01–T08
- `docs/DESIGN.md` §9.8 — enum
- `docs/BYTECODE.md` §5.17 / §6.11 / §9 — enum instructions, lowering, `enum_defs`
- `docs/SYNTAX.ebnf` — `enum_decl` / `enum_variant` / `enum_pattern`
- `examples/0.0.6/tour` — runnable tour
