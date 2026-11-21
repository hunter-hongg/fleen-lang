# Fleen 0.0.9 · Mending Weaverbird 🪡 Language Reference

This is the complete reference for the Fleen programming language 0.0.9, covering all syntax and features implemented by the current toolchain. It is organized by syntax category, with examples for each construct. See the [Appendix](15-appendix.md) for the reserved-word table, operator precedence, and runtime errors.

Key additions/changes since 0.0.8: **`break` / `continue`** — the two new reserved words, legal only inside a `while` / `for` body of the current function; they are statements, not expressions (ch. 06 §6.5). **Zero-payload variant patterns are variant references** — `when P(None, None)` compiles and matches (BUG-2 fix, ch. 06 §6.4); guard-free arms covering one variant *jointly* are accepted via constructor splitting, while cross-position coverage holes stay rejected (D7). **Result-`choose` completion** — `otherwise` and general arms are legal on a `Result` scrutinee; the 0.0.2-era rejection is gone (ch. 09 §9.2). **std.str batch** — eight new intrinsics: `parse_int` / `parse_float` / `trim` / `contains` / `starts_with` / `ends_with` / `index_of` / `replace` (ch. 13 §13.2). No bytecode change: the version stays v8, no new opcodes, no table changes (D6).

Breaking set: `break` and `continue` are reserved words — programs using them as identifiers must rename (the 0.0.5/0.0.6 precedent).

Historical additions: **per-file feature flags `#feature curry`**, two-part curried declarations, curried arrow types, implicit partial application (0.0.8, ch. 01/04/07/08), **parameterized enums `enum Option<T>`** and recursive patterns (0.0.7, ch. 04/06), **`enum` tagged unions** (0.0.6, ch. 04/06), **`unsafe`/`trusted` and pointers `ptr<T>`/`addr`/`deref_ptr`** (0.0.5, ch. 16), and array library `std.arr.concat`/`flatten` plus `foldl`/`foldr` (ch. 13).

## Contents

| Chapter | Topic |
|---------|-------|
| [01 Lexical Structure](01-lexical-structure.md) | Comments, identifiers, reserved words (**`break` / `continue`, 0.0.9**), literals, the `#feature` header directive (0.0.8) |
| [02 Statements and Semicolons](02-statements-and-semicolons.md) | Statement forms (**`break` / `continue`, 0.0.9**), semicolon rules, ASI |
| [03 Variables and Assignment](03-variables-and-assignment.md) | Bindings, `const`, type annotations, assignment, shadowing |
| [04 Types](04-types.md) | All types: primitives, arrays, struct, enum and type parameters (0.0.7), Range, box, ref, Result, Future, channel, ptr, function types (curried arrow spelling, 0.0.8) |
| [05 Operators](05-operators.md) | Arithmetic, comparison, logic, `as`, unary/suffix, pointer operators |
| [06 Control Flow](06-control-flow.md) | `if`/`elif`/`else`, `while`, `for`, **`break` / `continue` (0.0.9)**, `choose` (exhaustive `Result` and `enum`, recursive patterns, **zero-payload variant patterns + joint coverage, 0.0.9**) |
| [07 Functions](07-functions.md) | Declarations, two-part curried declarations (0.0.8), parameters, return values, implicit partial application (0.0.8), function values |
| [08 Ownership](08-ownership.md) | Copy vs owned, partial-application capture semantics (0.0.8), `move`, `clone`, `box`/`deref`, `ref`, async/pointer boundaries |
| [09 Error Handling](09-error-handling.md) | `Result`, `Ok`/`Err`, `?`, handling `Result` with `choose` (**`otherwise` and general arms, 0.0.9**) |
| [10 Async and Await](10-async-await.md) | `async func`, `Future<T>`, `await`, `await?`, executor, timers |
| [11 Channels](11-channels.md) | `channel<T>`, `send`, `recv`, `split` |
| [12 Modules](12-modules.md) | `import`, `pub`, visibility, resolution |
| [13 Standard Library](13-standard-library.md) | Every std module and function (**the 0.0.9 std.str batch**) |
| [14 Projects and flint](14-projects-and-flint.md) | Project layout, `flint.toml`, `flint` commands, tests |
| [15 Appendix](15-appendix.md) | Reserved words, operator precedence, errors (**0.0.9 error codes**), version notes |
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
