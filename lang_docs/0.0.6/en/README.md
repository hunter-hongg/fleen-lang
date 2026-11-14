# Fleen 0.0.6 Language Reference

This is the complete reference for the Fleen programming language 0.0.6, covering all syntax and features implemented by the current toolchain. It is organized by syntax category, with examples for each construct. See the [Appendix](15-appendix.md) for the reserved-word table, operator precedence, and runtime errors.

Key additions/changes since 0.0.4: **`enum` tagged unions** (0.0.6, ch. 04/06), **`unsafe`/`trusted` and pointers `ptr<T>`/`addr`/`deref_ptr`** (0.0.5, ch. 16), and array library `std.arr.concat`/`flatten` plus `fold`→`foldl`/`foldr` (ch. 13).

## Contents

| Chapter | Topic |
|---------|-------|
| [01 Lexical Structure](01-lexical-structure.md) | Comments, identifiers, reserved words, literals |
| [02 Statements and Semicolons](02-statements-and-semicolons.md) | Statement forms, semicolon rules, ASI |
| [03 Variables and Assignment](03-variables-and-assignment.md) | Bindings, `const`, type annotations, assignment, shadowing |
| [04 Types](04-types.md) | All types: primitives, arrays, struct, **enum**, Range, box, ref, Result, Future, channel, **ptr**, function types |
| [05 Operators](05-operators.md) | Arithmetic, comparison, logic, `as`, unary/suffix, pointer operators |
| [06 Control Flow](06-control-flow.md) | `if`/`elif`/`else`, `while`, `for`, `choose` (exhaustive `Result` and `enum`) |
| [07 Functions](07-functions.md) | Declarations, parameters, return values, function values |
| [08 Ownership](08-ownership.md) | Copy vs owned, `move`, `clone`, `box`/`deref`, `ref`, async/pointer boundaries |
| [09 Error Handling](09-error-handling.md) | `Result`, `Ok`/`Err`, `?`, handling `Result` with `choose` |
| [10 Async and Await](10-async-await.md) | `async func`, `Future<T>`, `await`, `await?`, executor, timers |
| [11 Channels](11-channels.md) | `channel<T>`, `send`, `recv`, `split` |
| [12 Modules](12-modules.md) | `import`, `pub`, visibility, resolution |
| [13 Standard Library](13-standard-library.md) | Every std module and function (`concat`/`flatten`, `foldl`/`foldr`) |
| [14 Projects and flint](14-projects-and-flint.md) | Project layout, `flint.toml`, `flint` commands, tests |
| [15 Appendix](15-appendix.md) | Reserved words, operator precedence, errors, version notes |
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
