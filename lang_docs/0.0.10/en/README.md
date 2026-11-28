# Fleen 0.0.10 · Loom Spider 🕸️ Language Reference

This is the complete reference for the Fleen programming language 0.0.10, covering all syntax and features implemented by the current toolchain. It is organized by syntax category, with examples for each construct. See the [Appendix](15-appendix.md) for the reserved-word table, operator precedence, and runtime errors.

Key additions/changes since 0.0.9: **string interpolation `"${expr}"`** — `${expr}` embeds a full expression (calls, field access, arithmetic, nested interpolating strings); `int` / `float` / `bool` stringify via exactly the `as string` path and `string` embeds verbatim (deep-copied, never moved); any other type is a compile error (`InterpUnsupportedType`); there are **no format specifiers** (ch. 01 §1.4, ch. 05 §5.5). Lowering desugars interpolation to a `std.str.concat` chain, so a literal **without** `${` compiles byte-identically to 0.0.9. **`\$` escape** — the one new escape, producing a literal `$`; without it, `$` is special only immediately before `{` (ch. 01 §1.4). **`Result` declared in std** — `std/result.fln` declares `pub enum Result<T, E> { Ok(T), Err(E) }` as the language-level source of truth; `Ok` / `Err` / `Result` are available in every file without an import and are **non-shadowable** (`ReservedBuiltinName`) (ch. 09 §9.1, ch. 13 §13.11). No bytecode change: the version stays v8, no new opcodes, no table changes (D7).

Breaking set: a **string literal containing the exact sequence `${`** — its meaning changes from "ordinary characters" to "interpolation begins"; write `\${` (escaped dollar, ordinary brace) to keep the literal text. Everything else is additive (`\$` was an invalid-escape error before; every program that compiled at 0.0.9 without a `${` string compiles identically).

Historical additions: **`break` / `continue`**, zero-payload variant patterns + joint coverage, Result-`choose` `otherwise`, the std.str batch (0.0.9, ch. 02/06/09/13), **per-file feature flags `#feature curry`**, two-part curried declarations, curried arrow types, implicit partial application (0.0.8, ch. 01/04/07/08), **parameterized enums `enum Option<T>`** and recursive patterns (0.0.7, ch. 04/06), **`enum` tagged unions** (0.0.6, ch. 04/06), **`unsafe`/`trusted` and pointers `ptr<T>`/`addr`/`deref_ptr`** (0.0.5, ch. 16), and array library `std.arr.concat`/`flatten` plus `foldl`/`foldr` (ch. 13).

## Contents

| Chapter | Topic |
|---------|-------|
| [01 Lexical Structure](01-lexical-structure.md) | Comments, identifiers, reserved words, literals, **string interpolation `"${expr}"` + the `\$` escape (0.0.10)**, the `#feature` header directive (0.0.8) |
| [02 Statements and Semicolons](02-statements-and-semicolons.md) | Statement forms (`break` / `continue`, 0.0.9), semicolon rules, ASI |
| [03 Variables and Assignment](03-variables-and-assignment.md) | Bindings, `const`, type annotations, assignment, shadowing |
| [04 Types](04-types.md) | All types: primitives, arrays, struct, enum and type parameters (0.0.7), Range, box, ref, **Result (declared in std, 0.0.10)**, Future, channel, ptr, function types (curried arrow spelling, 0.0.8) |
| [05 Operators](05-operators.md) | Arithmetic, comparison, logic, `as` (**the interpolation conversion path, 0.0.10**), unary/suffix, pointer operators |
| [06 Control Flow](06-control-flow.md) | `if`/`elif`/`else`, `while`, `for`, `break` / `continue` (0.0.9), `choose` (exhaustive `Result` and `enum`, recursive patterns, zero-payload variant patterns + joint coverage, 0.0.9) |
| [07 Functions](07-functions.md) | Declarations, two-part curried declarations (0.0.8), parameters, return values, implicit partial application (0.0.8), function values |
| [08 Ownership](08-ownership.md) | Copy vs owned, partial-application capture semantics (0.0.8), `move`, `clone`, `box`/`deref`, `ref`, async/pointer boundaries |
| [09 Error Handling](09-error-handling.md) | `Result` (**declared in std; the prelude trio `Ok`/`Err`/`Result` non-shadowable, 0.0.10**), `Ok`/`Err`, `?`, handling `Result` with `choose` (`otherwise` and general arms, 0.0.9) |
| [10 Async and Await](10-async-await.md) | `async func`, `Future<T>`, `await`, `await?`, executor, timers |
| [11 Channels](11-channels.md) | `channel<T>`, `send`, `recv`, `split` |
| [12 Modules](12-modules.md) | `import`, `pub`, visibility, resolution |
| [13 Standard Library](13-standard-library.md) | Every std module and function (**`std.result` and the prelude trio, 0.0.10**; the 0.0.9 std.str batch) |
| [14 Projects and flint](14-projects-and-flint.md) | Project layout, `flint.toml`, `flint` commands, tests |
| [15 Appendix](15-appendix.md) | Reserved words, operator precedence, errors (**0.0.10 error codes**), version notes |
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
