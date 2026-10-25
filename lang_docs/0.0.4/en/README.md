# The Fleen Language Reference

This is the complete reference documentation for the Fleen programming
language as implemented by the current toolchain. It describes every syntax
form and every feature a Fleen programmer can use, each with examples. It is
organized by syntax category; the [appendix](15-appendix.md) has the
reserved-word list, the operator precedence table, and the runtime error
list.

## Contents

| Chapter | Topic |
|---------|-------|
| [01 Lexical structure](01-lexical-structure.md) | Comments, identifiers, reserved words, literals |
| [02 Statements and semicolons](02-statements-and-semicolons.md) | Statement forms, semicolon rules, automatic semicolon insertion |
| [03 Variables and assignment](03-variables-and-assignment.md) | Bindings, `const`, type annotations, assignment, shadowing |
| [04 Types](04-types.md) | Every type: primitives, arrays, structs, Range, box, ref, Result, Future, channel, function types |
| [05 Operators](05-operators.md) | Arithmetic, comparison, logical, `as`, unary and postfix operators |
| [06 Control flow](06-control-flow.md) | `if`/`elif`/`else`, `while`, `for`, `choose` |
| [07 Functions](07-functions.md) | Function declarations, parameters, return values, function values |
| [08 Ownership](08-ownership.md) | Copy and owned values, `move`, `clone`, `box`/`deref`, `ref` parameters |
| [09 Error handling](09-error-handling.md) | `Result`, `Ok`/`Err`, the `?` operator, handling `Result` with `choose` |
| [10 Async and await](10-async-await.md) | `async func`, `Future<T>`, `await`, `await?`, the executor, timers |
| [11 Channels](11-channels.md) | `channel<T>`, `send`, `recv`, `split` |
| [12 Modules](12-modules.md) | `import`, `pub`, visibility, module resolution |
| [13 Standard library](13-standard-library.md) | Every std module and function, with signatures and examples |
| [14 Projects and flint](14-projects-and-flint.md) | Project layout, `flint.toml`, the `flint` commands, tests |
| [15 Appendix](15-appendix.md) | Reserved words, operator precedence, runtime errors, version notes |

## Running a Fleen program

Source files use the extension `.fln`.

```bash
# compile and run one file
cargo fln hello.fln

# compile and run a project (a directory tree of modules)
cargo fln --project myproj

# the same through the flint package manager
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
