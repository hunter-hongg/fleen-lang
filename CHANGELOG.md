# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and version semantics follow [SPEC.md](SPEC.md) §13 (`0.0.x` = early development, unstable API).

## 0.0.2 — Hermit Crab (2026-10-11) 🐚

The beginning of ownership: heap values get owners, errors start to flow, and semicolons start to retire.
Feature planning and design decisions: [docs/0.0.2/PLAN.md](docs/0.0.2/PLAN.md).

### ✨ Language Features

- **Ownership system**: `string` / `box<T>` have a single owner; `move` transfers explicitly and
  `clone` deep-copies; Copy types (`int` / `float` / `bool` / `unit` / function values)
  are unaffected; use of a moved value is rejected at compile time (`UseAfterMove`,
  `MaybeMovedAfterBranch`, etc., including loop back-edges)
- **`box<T>`**: `box expr` heap-allocates, `deref b` reads/writes the value inside the box,
  with deterministic deallocation (no GC); the set of allowed pointee types:
  `int` / `float` / `bool` / `string` / `box<U>` /
  `Result<T, E>`; values cannot be moved out of a box (`move deref b` is a compile error —
  use `clone deref b` if you need the value); in `deref b = v`, `b` must be a local box
  (writing through a global box reports `DerefAssignOfGlobalBox`)
- **`ref T`**: read-only borrowing, in function parameter position only — handles cannot escape,
  so no borrow checker is needed; the argument must be a variable; global arguments are
  copied via a temporary slot (`CloneGlobal` + temporary slot +
  `MakeRefLocal`); global initializers do not support ref arguments (`RefArgInGlobalInit`)
- **Error handling**: `Ok(v)` / `Err(e)` constructors, `Ok`/`Err` patterns in `choose`
  (both arms together are exhaustive; guarded arms do not count toward exhaustiveness),
  and the `?` propagation operator; the entry function
  (`main`) may return a `Result` — a trailing `Err` prints its payload to stderr and exits
  with code 1 (the exit code contract is unchanged)
- **ASI**: semicolons are optional; explicit semicolons remain legal; line-break sensitivity
  (joining statements on one line is still illegal); for cross-line continuation rules see
  [docs/0.0.2/ASI.md](docs/0.0.2/ASI.md)
- **`as` type casts** (temporary in 0.0.2, to be revisited after generics): the whitelist covers
  only `int` / `float` / `bool` → `string`; anything else (including identity casts) reports
  `UnsupportedCast { from, to }`; the result is a fresh owned string, sharing the same
  formatting helper as print (`fleen-vm/src/fmt.rs`)

### 🛠 Toolchain

- Bytecode v2: ownership instructions (`AllocBox` / `DerefBox` / `StoreDerefBox` /
  `MakeRefLocal` 0x80–0x83; `DupDeep` / `MoveLocal` / `CloneLocal` /
  `CloneGlobal` 0x90–0x93), Result instructions (0xA0–0xA4), and `ToStr` (0xA5);
  v2 modules can execute v1 instruction streams, while v2 instructions are forbidden in version=1 modules
  (specification: [docs/BYTECODE.md](docs/BYTECODE.md))
- `Value::Str(Rc<str>)` → `Str(Box<str>)` (exclusive ownership, paying off a temporary decision from 0.0.1)
- `span_map` promoted to a first-class component + runtime errors carry source line/column
  (`.fln` mode); errors propagated through a call chain point to the call site
- `print` gets its real built-in signature (variadic string; non-string arguments report
  `ArgTypeMismatch` with a hint to use `as string`); the typeck special case is removed
  (a known Hack from 0.0.1 — the known-Hacks list in `DESIGN.md` §18 is now empty)
- Implementation-level behavior details: `deref b = v` on a local box lowers to
  `v; AllocBox; StoreLocal b` (pointee replacement and whole-box replacement are observationally
  equivalent, so the compiler no longer emits `StoreDerefBox`); choose-Result lowers to an
  `IsErr` branch chain (a Result-choose with guarded arms or `otherwise` is rejected by typeck);
  `CloneGlobal` does not dereference handles
  (typeck forbids handles from entering globals)
- Security fixes: the `.flnc` decoder no longer preallocates unboundedly for malicious table
  lengths (since 0.0.1 an oversized length would abort the process; it is now downgraded to a
  decode error); verify rejects modules with empty function bodies
  (since 0.0.1 this would panic; it now reports `EmptyFunctionBody`)

### ⚠️ Breaking

- Assigning/passing owned values (`string` / `box<T>`) requires an explicit `move` or `clone`
  (in 0.0.1, `t = s` was a shallow shared copy; it is now a compile error)
- `move` / `clone` / `deref` / `as` become reserved words (in 0.0.1 they could be used as identifiers)
- `print(function value)`, allowed in 0.0.1, is now a compile error

## 0.0.1 — Swift Fox (2026-10-06) 🦊

The first milestone release: a **complete toolchain** from source to bytecode to execution.
An expression-oriented small language with a five-stage compiler, a static bytecode
verifier, and a stack-based virtual machine.

### Language at a Glance

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0;
    const limit = 10;

    while x < limit {
        print(fib(x));
        x = x + 1;
    }

    0
}
```

```console
$ fleen-vm fib.fln
0
1
1
2
3
5
8
13
21
34
```

### ✨ Language Features

**Variables and Immutability**

- `x = 42` for mutable bindings and assignment; `const y = 42` is immutable —
  reassignment is rejected at compile time
- Optional type annotations: `x: int = 42`

**Type System**

- Basic types: `int` (64-bit) / `float` / `bool` / `string` / `unit`
- The `Result<T, U>` type
- **Functions are first-class citizens**, and function types can be annotated directly:
  `func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)`

**Control Flow Is Expression-Oriented** — every branch produces a value

- `if / elif / else` expressions
- `while` loops
- `choose` pattern matching: literal patterns, guarded patterns like `when x if x > 10`,
  and an `otherwise` fallback; non-exhaustive matching is a compile-time error

**Diagnostics Experience**

- All compile-time errors carry a source location (Span), and messages are actionable
- Runtime errors are clearly categorized: division by zero, integer overflow
  (checked arithmetic — no silent wraparound), and exceeding the recursion depth limit (1024)

**Deliberate Trade-offs in 0.0.1**: semicolons `;` must be written explicitly
(automatic semicolon insertion comes in a later version);
`?`, `struct`, `for`, generics, FFI, etc. are not yet included — see `SPEC.md` §15 for the full scope.

### 🛠 Toolchain

```
fib.fln ─► Lex ─► Parse ─► Resolve ─► Typeck ─► Lower ─► Codegen ─► fib.flnc
                                                                     │
                                       fleen-verify static checks ◄─┘
                                                                     │
                                                          fleen-vm executes
```

| Component | Responsibility |
|------|------|
| `fleen-compiler` | Five-stage compiler front end, emitting versioned `.flnc` bytecode modules |
| `fleen-verify` | A static safety net before execution: all-path stack depth analysis, jump targets must land on full instruction boundaries, range checks on constant/function/global indices, `const` globals writable only by the module initializer, no fall-off-end |
| `fleen-vm` | A stack-based virtual machine: call frames and local slots, first-class function calls, host built-ins; bytecode is **never executed without verification** |

A single-command entry point is provided in the repository:

```bash
cargo fln fib.fln     # compile → verify → execute, all in one step
cargo fln fib.flnc    # you can also run bytecode directly
```

Exit code contract: `0` success / `1` compile·verify·runtime error / `2` usage·IO error — scripts can rely on it.

### 🧪 Quality Assurance

- **321 tests**: unit tests + integration tests driven by per-stage valid/invalid fixture files +
  end-to-end tests (driving the real binaries and comparing stdout, exit codes, and stderr)
- E2E coverage of error paths: lexer/parser/name-resolution/type errors, `const` reassignment,
  division by zero, integer overflow, infinite recursion
- A four-piece quality gate: `cargo fmt --check` · `cargo clippy -D warnings` ·
  `cargo test` · `cargo doc` (configured in GitHub Actions)

### 📦 Installation

Build from source (requires Rust 1.85+, 2024 edition):

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang
cargo build --release
./target/release/fleen-vm examples/0.0.1/hello.fln
```

Prebuilt binaries: the release assets include a linux x86_64 archive
(containing the `fleen-compiler` / `fleen-verify` / `fleen-vm` executables) —
just unpack and run: `./fleen-vm fib.fln`.

### 🔜 What's Next

Following the design in `docs/DESIGN.md`: automatic semicolon insertion (ASI), `struct`, `for`,
generics, the `?` operator, and more will land in subsequent releases; see
`docs/BYTECODE.md` for the bytecode format specification and `docs/0.0.1/TICKETS.md` for the roadmap.
