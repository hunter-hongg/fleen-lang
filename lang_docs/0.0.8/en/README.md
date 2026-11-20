# Fleen 0.0.8 · Cheeky Hamster 🐹 Language Reference

This is the complete reference for the Fleen programming language 0.0.8, covering all syntax and features implemented by the current toolchain. It is organized by syntax category, with examples for each construct. See the [Appendix](15-appendix.md) for the reserved-word table, operator precedence, and runtime errors.

Key additions/changes since 0.0.7: **per-file feature flags `#feature curry`** — a header directive that opts a file into currying (ch. 01 §1.5); **two-part curried declarations** — a signature line plus a named-parameter body, with `func` forbidden in a flag-on file (ch. 07 §7.1); **curried arrow function types** `int -> int -> int` — right-associative, `() -> T` for zero parameters, the canonical spelling in a flag-on file (ch. 04 §4.2); **implicit partial application** — a short call produces a function value carrying the supplied arguments, partials are Copy values and their captures are deep-copied (ch. 07 §7.5, ch. 08 §8.1). Non-breaking additive (D16).

Historical additions: **parameterized enums `enum Option<T>`** and recursive patterns (0.0.7, ch. 04/06), **`enum` tagged unions** (0.0.6, ch. 04/06), **`unsafe`/`trusted` and pointers `ptr<T>`/`addr`/`deref_ptr`** (0.0.5, ch. 16), and array library `std.arr.concat`/`flatten` plus `fold`→`foldl`/`foldr` (ch. 13).

## Contents

| Chapter | Topic |
|---------|-------|
| [01 Lexical Structure](01-lexical-structure.md) | Comments, identifiers, reserved words, literals, **the `#feature` header directive (0.0.8)** |
| [02 Statements and Semicolons](02-statements-and-semicolons.md) | Statement forms, semicolon rules, ASI |
| [03 Variables and Assignment](03-variables-and-assignment.md) | Bindings, `const`, type annotations, assignment, shadowing |
| [04 Types](04-types.md) | All types: primitives, arrays, struct, **enum and type parameters (0.0.7)**, Range, box, ref, Result, Future, channel, **ptr**, function types (**curried arrow spelling, 0.0.8**) |
| [05 Operators](05-operators.md) | Arithmetic, comparison, logic, `as`, unary/suffix, pointer operators |
| [06 Control Flow](06-control-flow.md) | `if`/`elif`/`else`, `while`, `for`, `choose` (exhaustive `Result` and `enum`, **recursive patterns: multi-payload / nested / or / guards, 0.0.7**) |
| [07 Functions](07-functions.md) | Declarations, **two-part curried declarations (0.0.8)**, parameters, return values, **implicit partial application (0.0.8)**, function values |
| [08 Ownership](08-ownership.md) | Copy vs owned, **partial-application capture semantics (0.0.8)**, `move`, `clone`, `box`/`deref`, `ref`, async/pointer boundaries |
| [09 Error Handling](09-error-handling.md) | `Result`, `Ok`/`Err`, `?`, handling `Result` with `choose` |
| [10 Async and Await](10-async-await.md) | `async func`, `Future<T>`, `await`, `await?`, executor, timers |
| [11 Channels](11-channels.md) | `channel<T>`, `send`, `recv`, `split` |
| [12 Modules](12-modules.md) | `import`, `pub`, visibility, resolution |
| [13 Standard Library](13-standard-library.md) | Every std module and function (`concat`/`flatten`, `foldl`/`foldr`) |
| [14 Projects and flint](14-projects-and-flint.md) | Project layout, `flint.toml`, `flint` commands, tests |
| [15 Appendix](15-appendix.md) | Reserved words, operator precedence, errors, version notes (**0.0.8 error codes**) |
| [16 Unsafe and Pointers](16-unsafe-and-pointers.md) | `unsafe`/`trusted`, `ptr<T>`, `addr`, `deref_ptr` (0.0.5) |

## Running a Fleen program

Source files use the `.fln` extension.

```bash
# Compile and run a single file
cargo fln hello.fln

# Compile and run a project (a module tree)
cargo fln --project myproj

# Run via the flint package manager
cd myproj
flint run
```

A minimal program:

```fleen
// hello.fln
import std.io;

func main() {
    std.io.println("Hello, Fleen");
}
```

```text
$ cargo fln hello.fln
Hello, Fleen
```
