# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and version semantics follow [SPEC.md](SPEC.md) §13 (`0.0.x` = early development, unstable API).

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
