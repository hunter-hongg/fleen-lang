# Fleen

**Fleen** is an expression-oriented small programming language with a complete toolchain from
source to bytecode to execution: a five-stage compiler (lexing → parsing → name resolution →
type checking → lowering and bytecode generation), a static bytecode verifier, and a
stack-based virtual machine.

> Current version **0.0.3 (in progress) · aggregation and organization** — built on
> **0.0.2 · Hermit Crab 🐚** (the "heap values get owners" milestone).
> Remaining 0.0.3 scope: `std` iterator helpers and `flint` (planned).

## Language at a Glance

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

Feature overview: mutable bindings with `=` and immutable `const`; `int` / `float` / `bool` / `string` / `unit`;
`Result<T, U>` (`Ok` / `Err` constructors, `choose` patterns, `?` propagation); first-class functions
and function types; `if` / `elif` / `else` expressions; `while`; `choose` pattern matching
(with guards); optional semicolons (ASI; explicit semicolons remain legal).

**0.0.2 · Ownership begins**: `string` / `box<T>` have a single owner — `move` transfers explicitly,
`clone` deep-copies, `box expr` heap-allocates and `deref b` reads/writes the value inside the box
(deterministic deallocation, no GC); `ref T` provides read-only borrowing (parameter position only);
`as string` converts scalars (a temporary feature); runtime errors carry source line/column.
See [docs/0.0.2/PLAN.md](docs/0.0.2/PLAN.md) for planning and implementation decisions.

**0.0.3 · Aggregation**: user `struct` declarations/literals/field access; `[T]` arrays with
written elements (both bounds-checked at runtime); `for x in iterable { … }` over arrays and
`Range` (the loop variable is a fresh immutable binding; the iterable is evaluated once).
`struct` / `for` / `in` are now reserved words (a breaking change). Modules / `pub` (V05) and
the `std` library (V06) are in; the `flint` package manager comes later in 0.0.3.
See [docs/0.0.3/PLAN.md](docs/0.0.3/PLAN.md) and [docs/0.0.3/TICKETS.md](docs/0.0.3/TICKETS.md).

## Quick Start

Requires Rust 1.85+ (2024 edition).

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang

# Build
cargo build --release

# Single command: compile + verify + execute (a cargo alias available in the repo)
cargo fln tests/e2e/valid/fib.fln

# Or run the binary directly
./target/release/fleen-vm tests/e2e/valid/fib.fln
```

Three executables:

| Command | Usage |
|------|------|
| `fleen-vm` | `fleen-vm <file.fln \| file.flnc>`: one-stop compile→verify→execute, or run bytecode directly |
| `fleen-compiler` | `fleen-compiler <file.fln> [-o <out.flnc>]`: compile to bytecode only |
| `fleen-verify` | `fleen-verify <file.flnc>`: statically verify bytecode |

Exit code contract: `0` success / `1` compile·verify·runtime error / `2` usage·IO error.

## Testing

```bash
cargo test          # unit + integration + end-to-end (including real-binary E2E)
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --no-deps
```

## Documentation

- [SPEC.md](SPEC.md) — project and code standards, language quick reference
- [docs/DESIGN.md](docs/DESIGN.md) — language design draft
- [docs/SYNTAX.ebnf](docs/SYNTAX.ebnf) — EBNF grammar
- [docs/BYTECODE.md](docs/BYTECODE.md) — bytecode format specification
- [docs/0.0.2/PLAN.md](docs/0.0.2/PLAN.md) — 0.0.2 feature plan (the beginning of ownership)
- [docs/0.0.3/PLAN.md](docs/0.0.3/PLAN.md) — 0.0.3 feature plan · [docs/0.0.3/TICKETS.md](docs/0.0.3/TICKETS.md) — implementation tickets
- [CHANGELOG.md](CHANGELOG.md) — version history

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)
