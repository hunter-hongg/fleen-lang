# Fleen 0.0.11 · Cartographer Ant 🗺️ Language Reference

This is the complete reference for the Fleen programming language 0.0.11, covering all syntax and features implemented by the current toolchain. It is organized by syntax category, with examples for each construct. See the [Appendix](15-appendix.md) for the reserved-word table, operator precedence, and runtime errors.

Key additions/changes since 0.0.10: **the `map` type** — `map[K, V]` with `map { k -> v }` literals (always owned, the array posture), `m[k]` access riding the existing index suffix and returning `Result[V, string]` (a missing key is `Err("key not found")`, never a crash), and the eight-helper `std.map` intrinsic batch `get` / `insert` / `remove` / `contains` / `keys` / `values` / `len` / `clone` with zero new opcodes; `insert` / `remove` are functional and consuming; keys are whitelisted to `{int, bool, string}` (`MapKeyNotComparable` otherwise); no `Future` / `channel` / `ptr` in the value slot; insertion order is preserved (ch. 01 §1.4, ch. 04 §4.13, ch. 05 §5.7, ch. 13 §13.12). **Bytecode v9, content-driven (D7)** — one new opcode (`NewMap`, 0xAF) and one new `FieldTypeTag` (`Map = 13`); a module is v9 only when a `NewMap` was actually encoded, so every map-free module (including every `std` module) stays v8 and byte-identical to 0.0.10.

Breaking set: **`map` becomes a reserved word** — a program with a binding, function, struct, enum, or module-local item named `map` must rename it. Everything else is additive: a map-free program lexes to the identical token stream, and the `map { … }` literal is new syntax that could not previously parse.

Historical additions: **string interpolation `"${expr}"` + the `\$` escape**, `Result` declared in std with the non-shadowable prelude trio (0.0.10, ch. 01/05/09/13), **`break` / `continue`**, zero-payload variant patterns + joint coverage, Result-`choose` `otherwise`, the std.str batch (0.0.9, ch. 02/06/09/13), **per-file feature flags `#feature curry`**, two-part curried declarations, curried arrow types, implicit partial application (0.0.8, ch. 01/04/07/08), **parameterized enums `enum Option<T>`** and recursive patterns (0.0.7, ch. 04/06), **`enum` tagged unions** (0.0.6, ch. 04/06), **`unsafe`/`trusted` and pointers `ptr<T>`/`addr`/`deref_ptr`** (0.0.5, ch. 16), and array library `std.arr.concat`/`flatten` plus `foldl`/`foldr` (ch. 13).

## Contents

| Chapter | Topic |
|---------|-------|
| [01 Lexical Structure](01-lexical-structure.md) | Comments, identifiers, reserved words (**`map`, 0.0.11**), literals, string interpolation `"${expr}"` + the `\$` escape (0.0.10), the `#feature` header directive (0.0.8) |
| [02 Statements and Semicolons](02-statements-and-semicolons.md) | Statement forms (`break` / `continue`, 0.0.9), semicolon rules, ASI |
| [03 Variables and Assignment](03-variables-and-assignment.md) | Bindings, `const`, type annotations, assignment, shadowing |
| [04 Types](04-types.md) | All types: primitives, arrays, structs, enums and type parameters (0.0.7), **`map[K, V]` (0.0.11)**, Range, box, ref, Result (declared in std, 0.0.10), Future, channel, ptr, function types (curried arrow spelling, 0.0.8) |
| [05 Operators](05-operators.md) | Arithmetic, comparison, logic, `as`, unary/suffix operators (**map lookup `m[k]` via the index suffix, 0.0.11**), pointer operators |
| [06 Control Flow](06-control-flow.md) | `if`/`elif`/`else`, `while`, `for` (**no `for` over a map** — iterate `std.map.keys` / `values`), `break` / `continue` (0.0.9), `choose` (exhaustive `Result` and `enum`, recursive patterns, zero-payload variant patterns + joint coverage, 0.0.9) |
| [07 Functions](07-functions.md) | Declarations, two-part curried declarations (0.0.8), parameters, return values, implicit partial application (0.0.8), function values |
| [08 Ownership](08-ownership.md) | Copy vs owned (**maps are always owned, 0.0.11**), partial-application capture semantics (0.0.8), `move`, `clone`, `box`/`deref`, `ref`, async/pointer boundaries |
| [09 Error Handling](09-error-handling.md) | `Result` (declared in std; the prelude trio `Ok`/`Err`/`Result` non-shadowable, 0.0.10), `Ok`/`Err`, `?`, **map lookups are `Result[V, string]` (0.0.11)**, handling `Result` with `choose` (`otherwise` and general arms, 0.0.9) |
| [10 Async and Await](10-async-await.md) | `async func`, `Future<T>`, `await`, `await?`, executor, timers |
| [11 Channels](11-channels.md) | `channel<T>`, `send`, `recv`, `split` |
| [12 Modules](12-modules.md) | `import`, `pub`, visibility, resolution |
| [13 Standard Library](13-standard-library.md) | Every std module and function (**`std.map`, the eight-helper batch, 0.0.11**; `std.result` and the prelude trio, 0.0.10; the 0.0.9 std.str batch) |
| [14 Projects and flint](14-projects-and-flint.md) | Project layout, `flint.toml`, `flint` commands, tests |
| [15 Appendix](15-appendix.md) | Reserved words, operator precedence, errors (**`MapKeyNotComparable`, 0.0.11**), version notes |
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
