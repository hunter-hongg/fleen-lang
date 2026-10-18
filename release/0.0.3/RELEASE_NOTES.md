# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and version semantics follow [SPEC.md](SPEC.md) §13 (`0.0.x` = early development, unstable API).

## 0.0.3 — Busy Beaver (2026-10-18) 🦫

The release that **gives values shape and code a home**: user-defined `struct` types and `[T]`
arrays aggregate data, `for x in iterable { … }` gives loops their natural form, the module
system (`import` / `pub`) lets programs stop being single files, the `std` library ships as
ordinary Fleen source, and `flint` — the package manager — lets projects grow beyond one
directory. Planning and design decisions: [docs/0.0.3/PLAN.md](docs/0.0.3/PLAN.md);
implementation tickets: [docs/0.0.3/TICKETS.md](docs/0.0.3/TICKETS.md) (V01–V10).

### Language at a Glance

```fleen
import std.arr;
import std.collection.iter;
import std.str;

struct Point { x: int, y: int }          // Copy iff every field is Copy

func double(n: int): int = n * 2
func is_even(n: int): bool = n % 2 == 0
func add(a: int, b: int): int = a + b

func main() {
    // struct: literal, field write (one level)
    p = Point { x: 3, y: 4 };
    p.y = 5;
    print("p = (", p.x as string, ", ", p.y as string, ")");

    // [T]: owned, index write, deep clone
    a = [1, 2, 3];
    a[0] = 10;
    b = clone a;
    b[0] = 99;
    print("a = ", join_ints(clone a), "  b = ", join_ints(clone b), " (deep clone)");

    // for: over an array, and over an end-exclusive Range
    sum = 0;
    for n in a { sum = sum + n; }
    squares: [int] = [];
    for i in range_from(1, 6) {
        squares = std.arr.push(squares, i * i);
    }
    print("sum(a) = ", sum as string, "  squares = ", join_ints(clone squares));

    // std.collection.iter: eager map / filter / fold
    doubled = std.collection.iter.map(a, double);
    evens = std.collection.iter.filter(a, is_even);
    total = std.collection.iter.fold(a, 0, add);
    print("map 2x = ", join_ints(clone doubled),
          "  filter even = ", join_ints(clone evens),
          "  fold + = ", total as string);
}
```

```console
$ fleen-vm showcase.fln
p = (3, 5)
a = [10,2,3,]  b = [99,2,3,] (deep clone)
sum(a) = 15  squares = [1,4,9,16,25,]
map 2x = [20,4,6,]  filter even = [10,2,]  fold + = 15
```

(`join_ints` is an ordinary Fleen function — a `for` loop over a `[int]` plus `std.str.concat`.)

### ✨ Language Features

**Aggregation types**

- **`struct`**: declarations (`struct Point { x: int, y: int }`, `pub` optional), literals with
  every field initialized, field read `p.f`, and one-level field write `p.f = e` on a
  local/global (or by-value parameter) binding
- A struct is a **Copy type iff all of its fields are Copy** — the `Result<T, E>` rule,
  generalized; otherwise it is an owned type and consuming positions need `move` / `clone`
- Recursive structs are rejected (`RecursiveStruct`); `move p.f` is rejected
  (`MoveOutOfStruct`, help: `clone p.f`); missing/unknown fields in a literal are rejected
- **`[T]` arrays**: literals (`[]` needs a type annotation), bounds-checked read `a[i]` and
  write `a[i] = e`; **arrays are always owned**, even `[int]` — `move` transfers and `clone`
  deep-copies; out-of-bounds is a runtime `IndexOutOfRange` carrying the source span
- `==` on structs and arrays is rejected (it arrives with traits in 0.1.0), as are
  `move p.f` / `move a[i]`

**`for` loop and `Range`**

- `for x in iterable { … }` over the closed iterable set `{ [T], Range }`; anything else
  reports `NotIterable` (user-defined iterables wait for traits)
- The loop variable is a **fresh immutable binding per iteration**; the iterable expression is
  evaluated **once**, before the loop; `for` does not consume the iterable
- `Range` is a builtin **Copy** type, end-exclusive, built by `std.collection.range`:
  `range(n)` is `0..n`, `range_from(a, b)` is `a..b` (an empty or reversed range simply
  iterates zero times)
- No runtime iterator objects: `for` desugars to an index loop in Lower (MIR)

**Modules and visibility**

- **One file = one module**; a directory's entry is `mod.fln`. `import net.tcp;` resolves to
  `src/net/tcp.fln` (or `src/net/tcp/mod.fln`); paths are absolute from the crate root
- **Private by default**: only `pub` items (`pub func` / `pub const` / `pub struct` / `pub`
  globals) cross a module boundary; violations report `PrivateAccess`
- Import cycles (`CircularImport`), missing files (`ModuleNotFound`), duplicate paths
  (`DuplicateModule`) and cross-crate back-edges (`CrossCrateImport`) are hard errors, all
  rendered with `--> file:line:column` (`FileId` / `SourceMap`)
- `fleen-vm --project <dir>` compiles and runs a whole source tree; single-file
  `cargo fln file.fln` is unchanged
- Compilation stays **whole-program**: module structure is compile-time only and never
  reaches the bytecode

**`std` library**

- Modules `io` / `str` / `num` / `arr` / `collection` / `os` / `time`: `print` / `println` /
  `read_line`; `len` / `concat` / `split` / `from_int` / `from_float` / `from_bool`;
  `abs` / `min` / `max` / `sqrt` / `floor`; `len` / `get` (→ `Result<T, string>`) / `set` /
  `push`; `range` / `range_from`; `args` / `exit`; `now_millis`
- std is **ordinary Fleen source** embedded into the compiler and compiled on demand — only
  imported modules enter the program; the only VM-level primitives are the intrinsics, and
  `infer.rs` stays free of any std special case
- The **prelude is exactly `print`** (one implicit `import std.io;`), so every 0.0.1/0.0.2
  program compiles unchanged — subject only to the new reserved words below
- **Iterator helpers** (`std.collection.iter`): eager `map` / `filter` / `fold` / `take`,
  arrays in and arrays out, typed by the compiler's schematic `[T]` signatures and inlined at
  the call site (`zip` is deferred to 0.1.0, with tuples/generics)

### ⚠️ Breaking

- **`struct`, `for`, `in`, and `pub` become reserved words** — 0.0.1/0.0.2 programs using
  them as identifiers must rename them. Nothing else breaks: the prelude (`print`) is
  unchanged, explicit semicolons stay legal, and all 0.0.2 source programs compile as-is
  modulo those four
- v3 opcode numbering is finalized within the previously reserved `0xB0+` space only
  (`ArrSetGlobal` = 0xB8, `MakeRange` = 0xB9). **No existing instruction changed its number
  or encoding** (BYTECODE.md §10): v1 and v2 `.flnc` files still execute unchanged, locked by
  committed fixtures

### 🛠 Toolchain

```
showcase.fln ─► Lex ─► ASI ─► Parse ─► Resolve(with module table) ─► Typeck ─► Lower ─► Codegen ─► showcase.flnc
                                                                                                 │
                                    fleen-verify static checks (v1+v2+v3 rules) ◄────────────────┘
                                                                                                 │
                                                                                            fleen-vm executes
```

| Component | Changes in 0.0.3 |
|------|------------|
| `fleen-compiler` | `FileId`/`SourceMap` (multi-file spans), the module-table sub-pass inside Resolve, struct/array/range types in typeck, the `for` desugar in Lower, bytecode v3 + the appended `struct_defs` table |
| `fleen-verify` | Operand-derived stack effects for `StructNew`/`ArrNew`, `struct_defs` cross-checks, and an explicit gate rejecting a v3 opcode in a v1/v2 module |
| `fleen-vm` | `Value::Struct` / `Value::Arr` / `Value::Range`, spanned `IndexOutOfRange`, the new intrinsics (`println`, `read_line`, `sqrt`, `floor`, `args`, `exit`, `now_millis`); v1/v2 bytecode still executes |
| `flint-cli` (**new**) | The `flint` package manager: `flint.toml` + `Flint.lock`, git-only dependencies under `$FLINT_HOME`, and `new` / `build` / `run` / `test` / `check` |

- **Bytecode v3**: struct `0xB0–0xB3`, array `0xB4–0xB8`, range `0xB9–0xBB`, plus the
  appended `struct_defs` table — all append-only (specification: [docs/BYTECODE.md](docs/BYTECODE.md))
- **`flint`**: `flint new hello` scaffolds a manifest, a `src/main.fln` and a git repo;
  dependencies are git URLs resolved to a commit pinned by the committed `Flint.lock`
  (`version` is advisory — the lockfile is the pin), fetched into `$FLINT_HOME`
  (default `~/.flint`), with dependency-graph cycle detection
- **Compatibility gate**: committed v1 / v2 / v3 `.flnc` fixtures are decoded, verified and
  executed on every `cargo test`, and every example under `examples/` is
  compiled · verified · run
- **Fixed in this release**: constant-pool interning is now truly bit-exact — `Const`'s
  `PartialEq` used `f64::eq` (`0.0 == -0.0`) while its `Hash` used the bit pattern, so
  `0.0` and `-0.0` could collapse onto one pool entry depending on the map's random seed

### 📦 Projects and flint

```
tour/
├── flint.toml              # [package] name = "tour"
├── Flint.lock              # committed once dependencies exist
└── src/
    ├── main.fln            # crate root: imports modules + std
    └── shapes/
        ├── mod.fln         # `shapes`: pub struct Point, pub funcs
        └── tools.fln       # `shapes.tools`: qualified path
```

```console
$ fleen-vm --project examples/0.0.3/tour    # or: cd tour && flint run
area(p) = 42
gcd(48, 18) = 6
p.y = 8
a = [10,2,3,]  b = [99,2,3,] (deep clone)
len(a) = 3
sum(a) = 15
squares (1..=5) = [1,4,9,16,25,]
map(a,2x) = [20,4,6,]
filter(a,even) = [10,2,]
take(squares,3) = [1,4,9,]
fold(a,0,+) = 15
argc = 0
```

### 🧪 Quality Assurance

- **845 tests** (up from 608 in 0.0.2): unit tests, per-stage valid/invalid fixture tests,
  end-to-end tests driving the real binaries, `tests/std/` (std is always compiled in CI),
  multi-file project-mode e2e, `flint` fixture tests built from local `file://` git repos
  (no network in CI), and the v1/v2/v3 bytecode compatibility fixtures
- **Every 0.0.1/0.0.2 end-to-end test passes byte-for-byte** — the reserved words above are
  the only source-level break
- A four-piece quality gate: `cargo fmt --check` · `cargo clippy -D warnings` ·
  `cargo test` · `cargo doc` (configured in GitHub Actions, run on branches and tags)

### 📦 Installation

Build from source (requires Rust 1.85+, 2024 edition):

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang
cargo build --release
./target/release/fleen-vm examples/0.0.3/tour
```

Prebuilt binaries: the release assets include a linux x86_64 archive containing
`fleen-compiler` / `fleen-verify` / `fleen-vm` / `flint-cli` — unpack and run:
`./fleen-vm showcase.fln`.

Single command and project entry points:

```bash
cargo fln file.fln                # one file: compile → verify → execute
cargo fln --project <dir>         # a whole source tree
flint new hello && flint run      # package-manager entry (as flint-cli)
```

Exit code contract unchanged: `0` success / `1` compile·verify·runtime error /
`2` usage·IO error (`exit(n)` passes `n` through).

### 🔜 What's Next

0.0.4: `async` / `await`; 0.0.5: `unsafe` / `trusted` / `ptr` / `addr`; 0.1.0: generics,
traits (and with them user-defined iterables, closures and struct/array `==`), operator
overloading. See `docs/DESIGN.md` §18 for the full roadmap, and
[docs/0.0.3/PLAN.md](docs/0.0.3/PLAN.md) /
[docs/0.0.3/TICKETS.md](docs/0.0.3/TICKETS.md) for 0.0.3 planning and implementation
decisions (including every recorded deviation).
