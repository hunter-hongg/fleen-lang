# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and version semantics follow [SPEC.md](SPEC.md) §13 (`0.0.x` = early development, unstable API).

## 0.0.5 — Careful Hedgehog (2026-11-02) 🦔

The release that makes raw pointers explicit — and matures the toolchain around the
language: `unsafe` / `trusted` function modifiers gate a new raw-pointer layer
(`ptr<T>` / `addr` / `deref_ptr`), a pointer never crosses the async world, and the
toolchain gets a standardized CLI, a six-target tag-triggered release pipeline, and a
version-pinned std distribution. Like a hedgehog, Fleen keeps the dangerous parts behind
spines: with no borrow checker, **every pointer use is an unsafe operation**, and the
spines (`unsafe` / `trusted`, the boundary rules) are what make approaching pointers
safe. The unsafe surface is minimized by rule, not by hope. Planning and design decisions:
[docs/0.0.5/PLAN.md](docs/0.0.5/PLAN.md) (decisions D1–D8); implementation tickets:
[docs/0.0.5/TICKETS.md](docs/0.0.5/TICKETS.md) (T01–T10).

### Language at a Glance

```fleen
// Careful Hedgehog showcase — every pointer use sits behind unsafe
g = 41;                          // a global is a place: addr may take its address too

unsafe func bump(seed: int): int {
    x = seed + 1;               // a mutable local place (params are immutable — addr rejects them)
    p = addr x;                 // p: ptr<int> — take the address (an unsafe op, D2)
    deref_ptr p = x + 1;        // write through the pointer
    p = addr g;                 // retarget at the global
    deref_ptr p = x;            // copy the value through into the global
    deref_ptr p
}

trusted func hedgehog(seed: int): int = bump(seed)   // attested; callable from safe code

func main(): int {
    r = hedgehog(41);
    print("result = ", r as string);
    print("global = ", g as string);
    0
}
```

```console
$ fleen-vm showcase.fln
result = 43
global = 43
```

(`addr` takes the address of a place — local, global, struct field, array element, or box
interior; `deref_ptr p` reads **and** writes through the pointer, the one settled spelling
(the box-only `deref` is untouched). The whole pointer dance happens inside `unsafe func`;
`trusted func` is the attested wrapper that safe code may call. A pointer never lives in a
field, element, box cell, or global — it is local, parameter, or return value.)

### ✨ Language Features

**Raw pointers — `ptr<T>` / `addr` / `deref_ptr`**

- `ptr<T>` is a type in constructor position (like `Future<T>` / `channel<T>`); a bare
  `ptr` is not a type. **Non-Copy, move-only** (`clone p` → `CloneOfPointer`) and
  **non-storable** (`PtrNotStorable` — no struct fields, array elements, box cells, or
  globals): a pointer lives in a local, a parameter, or a return value (D1)
- `addr x` takes the address of a place (one level deep: local, global, direct field,
  array element, box interior). It **rejects immutable bases** (`AddrOfImmutable` — a
  `const`, or a parameter: parameters are immutable bindings) and **ref-parameter
  bases** (`AddrOfRefParam` — a pointer would outlive the borrow's write protection)
- `deref_ptr p` reads and writes through the pointer — the T02-settled spelling; `deref`
  itself stays **box-only**, so no existing safe call site changes meaning
- **Every pointer use is an unsafe operation** (D2): creating a pointer and using one both
  require an `unsafe`/trusted body. Fleen has no borrow checker, so safe code could move
  the referent out from under a pointer (`y = move x`) and no diagnostic could catch the
  safe half — the whole "pointer exists at all" state sits behind `unsafe`

**`unsafe` / `trusted` function modifiers**

- Lexical gating (no call-graph analysis, the 0.0.4 `RefInAsync` approach): an `unsafe
  func` body may contain unsafe ops and is callable only from unsafe/trusted bodies
  (`UnsafeCallOutsideUnsafe`); a `trusted func` body may also contain them but is
  callable from anywhere — the author attests its safety (D3)
- They compose with `async` in either order (`async unsafe func` / `unsafe async
  func` → one AST state); calling or awaiting an unsafe async function needs the same
  context (D4)
- **Known limit (documented, T03)**: an `unsafe func` called *through a function value*
  escapes the static call check (first-class functions — the same known limit as 0.0.4's
  channel boundary); the VM's `LoadPtr` / `StorePtr` backstop is the last line of
  defense

**The pointer boundary rule**

- A `ptr<T>` may not be captured across an `await`, moved into a task, or sent through
  a channel — `PtrAcrossBoundary`, the sibling of `RefInAsync` (D4). The 0.0.4
  one-rule static Send story stays one rule: every non-Copy handle whose validity depends
  on *a frame staying put* is excluded (frames are relocated by the scheduler)

### ⚠️ Breaking

- **`ptr`, `addr`, `deref_ptr`, `unsafe`, and `trusted` become reserved words** —
  0.0.4 programs using them as identifiers must rename them. Nothing else breaks:
  `deref` remains box-only (the pointer spelling is additive), Copy/owned semantics are
  unchanged, and all 0.0.4 source programs compile as-is modulo those five renames
- **DESIGN §12's lambda sketch is superseded** by the D3 func-modifier form (closures land
  in 0.1.0) — recorded in the doc sync, not a source break
- `.flnc` `version` is now written as `5` (`AddrOf` / `LoadPtr` / `StorePtr` in the
  reserved 0xC8–0xCA band; `Value::Ptr` is a **non-serialized** runtime variant, like
  `Future` / `Channel`). **Zero layout changes** — no new table; readers accept
  `{1, 2, 3, 4, 5}`: every v1–v4 `.flnc` file still decodes, verifies and executes
  unchanged, locked by committed fixtures. A v5 opcode in a v1–v4 module is rejected with
  the dedicated `V5OpcodeInV1V2V3V4Module` gate

### 🛠 Toolchain

| Component | Changes in 0.0.5 |
|------|------------|
| `fleen-compiler` | Reserved words `Ptr` / `Addr` / `DerefPtr` / `Unsafe` / `Trusted` (T01), `ptr<T>` / `addr` / `deref_ptr` in Parse and the D1–D4 typeck rules (T02–T04), `AddrOf` / `LoadPtr` / `StorePtr` lowering + bytecode v5 emission (`MODULE_VERSION = 5`) (T05), the shared std-resolution module with the D8 three-tier order + strict version check (T08), `clap` CLI (T07) |
| `fleen-verify` | Version gate `{1..5}`, the `V5OpcodeInV1V2V3V4Module` opcode gate, Δdepth for the three new instructions (including the `StorePtr` Δ−2 two-pop correction, T06), `clap` CLI (T07) |
| `fleen-vm` | `Value::Ptr` (a stable-cell handle, not serialized; frames stay movable — the cell is an `Rc`, not a stack index), promote-on-addr: `AddrOf` promotes the referent slot to a stable heap cell and every local/global load/store + the pointer opcodes dereference it; malformed handle → defensive runtime backstop (T06), `clap` CLI (T07) |
| `flint-cli` | `clap` subcommands with per-command help and formal exit codes 0/1/2 (T07); `flint check --std` reports which std copy is in effect + its version (T08) |

- **CLI standard (T07, D6)**: `clap` (derive) across all three binaries — uniform
  `--project <dir>` / `-h/--help` / `-V/--version` / `--quiet` / `--no-color`,
  program args after `--`; rustc-style span diagnostics (`file:line:col` header,
  source line, `^^^^` underline, indented `note:`/`suggestion:`) in plain ANSI,
  disabled by `--no-color` or `NO_COLOR`
- **Bytecode v5 (T05/T06)**: `AddrOf` (0xC8, one opcode for local + global places via
  bit 15 of its u16 operand) / `LoadPtr` (0xC9) / `StorePtr` (0xCA) in the previously
  reserved 0xC8–0xDF band (specification: [docs/BYTECODE.md](docs/BYTECODE.md) §5.16)
- **Release pipeline (T09, D7)**: `.github/workflows/release.yml` triggers on `v*` tags
  and builds six targets — x86_64/aarch64 Linux (gnu + musl static), x86_64/aarch64
  macOS, x86_64 Windows — into flat archives (four binaries + `std/` + README;
  `.tar.gz` for Linux/macOS, `.zip` for Windows, no install script — README documents
  manual install); the Windows job runs an explicit `std.io.read_line` smoke test. wasm
  and FreeBSD are intentionally absent (PLAN §10)
- **std distribution (T08, D8)**: one shared resolution order — `FLINT_STD_PATH` →
  `$FLINT_HOME/std/<compiler-version>/` → embedded. An external std is admitted only if
  its `VERSION` file matches the compiler's (a mismatch is a hard error naming both
  versions, never a silent fallback); release archives carry `std/` at a documented path

### 🧪 Quality Assurance

- **1048 tests** (up from 953 in 0.0.4): unit tests, per-stage valid/invalid fixture
  tests (the ptr/addr parser + typeck suites, the unsafe-gating and boundary fixtures),
  end-to-end tests driving the real binaries (e.g. `addr_write.fln`), the flint
  std-addressing fixtures, the CLI-level tests (exit codes, help/version text, NO_COLOR),
  and the v1–v4 bytecode compatibility fixtures
- **Every 0.0.1–0.0.4 end-to-end test passes byte-for-byte** — the five reserved words
  above are the only source-level break; v1/v2/v3/v4 `.flnc` fixtures still decode,
  verify and execute
- A four-piece quality gate: `cargo fmt --check` · `cargo clippy -D warnings` ·
  `cargo test` · `cargo doc` (configured in GitHub Actions on the ubuntu/macos/windows
  OS matrix; the flint e2e fixtures need a writable `$FLINT_HOME`)

### 📦 Installation

Build from source (requires Rust 1.85+, 2024 edition):

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang
cargo build --release
./target/release/fleen-vm showcase.fln
```

Prebuilt binaries: the six release archives (x86_64-linux-gnu / x86_64-linux-musl /
aarch64-linux / x86_64-macos / aarch64-macos / x86_64-windows) each contain
`fleen-vm` / `fleen-compiler` / `fleen-verify` / `flint-cli`, the `std/` tree, and a
README. To use a released package's std with `flint` / `cargo fln`, copy the archive's
`std/` to `$FLINT_HOME/std/0.0.5/` (the archive's README documents the manual
install); `flint check --std` reports which copy is in effect.

Single command and project entry points:

```bash
cargo fln file.fln                # one file: compile → verify → execute
cargo fln --project <dir>         # a whole source tree
flint new hello && flint run      # package-manager entry
```

Exit code contract (now uniform across all three binaries, T07): `0` success /
`1` compile·verify·runtime error / `2` usage·IO error (`exit(n)` passes `n` through).

### 🔜 What's Next

0.1.0: generics, traits, and operator overloading — with them closures (the rejected
lambda form of `unsafe` / `trusted`), language-level composing async
(`select` / `join!` fan-out, channel `close` and `for`-over-channel), user-defined
iterables, and struct/array `==`; a multi-threaded executor with a cross-task heap (the
0.0.5 boundary rule, D4, only holds while frames stay task-local). Extended string escapes
(`\0`, `\b`, `\f`, `\xHH`, `\uXXXX`). See `docs/DESIGN.md` §18 for the full
roadmap, and [docs/0.0.5/PLAN.md](docs/0.0.5/PLAN.md) /
[docs/0.0.5/TICKETS.md](docs/0.0.5/TICKETS.md) for 0.0.5 planning and implementation
decisions (including every recorded deviation).
