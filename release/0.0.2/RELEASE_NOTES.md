# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and version semantics follow [SPEC.md](SPEC.md) §13 (`0.0.x` = early development, unstable API).

## 0.0.2 — Hermit Crab (2026-10-11) 🐚

The beginning of ownership: **heap values get owners, errors start to flow, and semicolons
start to retire.** Values now have a single owner with explicit `move` / `clone`; errors gain
`?` propagation and `Ok` / `Err` pattern matching; semicolons are handed to automatic
insertion (ASI), while explicit semicolons remain perfectly legal.

### Language at a Glance

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func safe_half(a: int): Result<int, string> {
    q = div(a, 2)?          // on Err, the whole function returns that Err immediately
    Ok(q)
}

func main(): Result<int, string> {
    s = "hello"
    t = move s              // ownership moves; s is no longer usable

    n = choose safe_half(10) {
        when Ok(v) { v }
        when Err(e) { 0 - 1 }
    }

    b = box n               // heap allocation; b: box<int>
    deref b = deref b + 1   // write the value inside the box

    print(t, " world: ", (n + deref b) as string)
    Ok(0)
}
```

```console
$ fleen-vm showcase.fln
hello world: 11
```

Semicolons are no longer required (ASI; see `docs/0.0.2/ASI.md` for cross-line continuation
and statement-boundary rules), and the explicit-semicolon style of 0.0.1 remains fully legal.

### ✨ Language Features

**Ownership System**

- `string` / `box<T>` have a single owner; `move x` transfers explicitly, `clone x` deep-copies
- Copy types (`int` / `float` / `bool` / `unit` / function values) are unaffected and keep copying by value
- Using a moved binding on any path is rejected at compile time (`UseAfterMove`,
  `MaybeMovedAfterBranch`, etc.); moving inside a loop and using the value again after the
  back-edge is rejected too
- Assigning/passing owned values requires an explicit `move` or `clone`
  (`OwnedArgRequiresMove`, with help text offering both options)

**`box<T>` and `deref`**

- `box expr` heap-allocates (`b: box<int>`); `deref b` reads the value inside the box
  (a copy, a deep owned clone); `deref b = v` writes it; allowed pointee types:
  `int` / `float` / `bool` / `string` / `box<U>` / `Result<T, E>`
- **Values cannot be moved out of a box**: `move deref b` is a compile error —
  use `clone deref b` if you need the value
- In `deref b = v`, `b` must be a **local box** (writing through a global box reports
  `DerefAssignOfGlobalBox`); deallocation is deterministic, with no GC

**`ref T` Read-Only Borrowing (parameter position only)**

- `func shout(s: ref string): int { print(s); 42 }` — read-only, zero-copy
- Handles cannot escape: they cannot be assigned, stored, or returned → no borrow checker
  needed, and no possibility of dangling
- Arguments must be variables (local / global / another ref parameter); global arguments are
  copied via a temporary slot, and global initializers do not support ref arguments
  (`RefArgInGlobalInit`)

**Error Handling**

- `Ok(v)` / `Err(e)` constructors; `Ok` / `Err` patterns in `choose` — both arms together
  are exhaustive (guarded arms do not count toward exhaustiveness — the same rule as a
  Bool scrutinee)
- The `?` propagation operator: `q = div(a, 2)?` — on Err, the whole function returns
  that Err immediately
- The entry function (`main`) may return a `Result`: a trailing `Err` prints its payload
  to stderr and exits with code 1 (the exit code contract is unchanged:
  `0` success / `1` error / `2` usage·IO)

**ASI: Semicolons Are Optional**

- Line-break sensitive: joining statements on one line is still illegal; cross-line
  continuation (binary operators, `?` chains, `as`) and statement boundaries are decided by
  a dedicated ASI pass (`docs/0.0.2/ASI.md`)
- Explicit semicolons are legal everywhere — no 0.0.1 program needs to change

**`as` Type Casts (temporary in 0.0.2, to be revisited after generics)**

- The whitelist has exactly three entries: `int as string` (decimal, with a minus sign),
  `float as string` (matching print output), and `bool as string` (`"true"` / `"false"`);
  everything else (including identity casts) reports `UnsupportedCast { from, to }`
- The result is a fresh owned string — no `move` / `clone` needed at the point of consumption
- print and `as` share the same formatting helper (`fleen-vm/src/fmt.rs`), so the two
  paths cannot drift apart

### ⚠️ Breaking

- Assigning/passing owned values (`string` / `box<T>`) requires an explicit `move` or `clone`
  (in 0.0.1, `t = s` was a shallow shared copy; it is now a compile error)
- `move` / `clone` / `deref` / `as` become reserved words (in 0.0.1 they could be used as identifiers)
- `print(function value)`, allowed in 0.0.1, is now a compile error

### 🛠 Toolchain

```
showcase.fln ─► Lex ─► ASI ─► Parse ─► Resolve ─► Typeck(with ownership) ─► Lower ─► Codegen ─► showcase.flnc
                                                                                          │
                                  fleen-verify static checks (v1+v2 rules) ◄─────────────┘
                                                                                          │
                                                                               fleen-vm executes
```

| Component | Changes in 0.0.2 |
|------|------------|
| `fleen-compiler` | ASI as a dedicated pass; typeck includes an ownership-checking sub-pass (`typeck/ownership.rs`, still part of the same phase); MIR instructions carry Spans that are lowered into bytecode |
| `fleen-verify` | Stack depth analysis for v2 instructions (`AllocBox` has a net effect of 0, etc.); v2 instructions are forbidden in version=1 modules (checked in a single comparison); empty function bodies are rejected (0.0.1 panicked; it now reports `EmptyFunctionBody`) |
| `fleen-vm` | Bytecode v2 execution (ownership 0x80–0x93, Result 0xA0–0xA4, `ToStr` 0xA5); `Value::Str` changed from `Rc<str>` to `Box<str>` (exclusive ownership); v1 bytecode remains executable |

- **Runtime diagnostics**: `span_map` is promoted to a first-class component — in `.fln` mode,
  runtime errors carry source line/column; errors propagated through a call chain point to
  the call site (0.0.2 U10)
- **`print` gets a real signature**: `(string...) -> unit`; non-string arguments report
  `ArgTypeMismatch` with a hint to use `as string`; the 0.0.1 typeck special case
  (a known Hack) is removed, and the known-Hacks list in `DESIGN.md` §18 is now empty
- **Bytecode v2 is compatible with v1**: v2 modules can execute v1 instruction streams;
  opcodes are only added, never changed (specification: `docs/BYTECODE.md` §10)
- **Security fixes**: the `.flnc` decoder no longer preallocates unboundedly for malicious
  table lengths (since 0.0.1 an oversized length would abort the process; it is now
  downgraded to a decode error)

### 🧪 Quality Assurance

- **608 tests** (up from 321 in 0.0.1): unit tests + integration tests driven by per-stage
  valid/invalid fixture files + end-to-end tests (driving the real binaries and comparing
  stdout, exit codes, and stderr)
- Dedicated invalid fixtures cover the error paths for ownership/borrowing/Result/`?`/casts;
  every 0.0.1 E2E test (including the explicit-semicolon `fib.fln`) passes under 0.0.2
  byte-for-byte
- A four-piece quality gate: `cargo fmt --check` · `cargo clippy -D warnings` ·
  `cargo test` · `cargo doc` (configured in GitHub Actions)

### 📦 Installation

Build from source (requires Rust 1.85+, 2024 edition):

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang
cargo build --release
./target/release/fleen-vm showcase.fln
```

Prebuilt binaries: the release assets include a linux x86_64 archive
(containing the `fleen-compiler` / `fleen-verify` / `fleen-vm` executables) —
just unpack and run: `./fleen-vm showcase.fln`.

A single-command entry point is provided in the repository:

```bash
cargo fln showcase.fln     # compile → verify → execute, all in one step
cargo fln showcase.flnc    # you can also run bytecode directly
```

### 🔜 What's Next

0.0.3: `struct` / `for` / iterators (nested owned types and the full cast matrix will be
revisited then); see `docs/DESIGN.md` §18 for the full roadmap, and `docs/0.0.2/PLAN.md`
and `docs/0.0.2/TICKETS.md` for 0.0.2 planning and implementation decisions.
