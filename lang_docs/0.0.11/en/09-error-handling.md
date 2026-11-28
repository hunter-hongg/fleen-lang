# 09 — Error Handling

## 9.1 `Result<T, E>`

`Result<T, E>` is the type of an operation that may fail. It has two
variants:

- `Ok(value)` — success, carrying the payload of type `T`
- `Err(value)` — failure, carrying the error payload of type `E`

**Declared in std (0.0.10, D1)**: `Result` is a real declared parameterized
enum — `std/result.fln` declares `pub enum Result<T, E> { Ok(T), Err(E) }`
and is the language-level source of truth. The compiler recognizes
`std.result.Result` by canonical path and maps it onto its internal
representation; the runtime encoding (the `Result` opcodes, the VM's
`Ok`/`Err` values) is unchanged, and no `enum_defs` entry is emitted — the
module is declaration-only (chapter 13 §13.11).

**Prelude trio (0.0.10, D2)**: `Result`, `Ok`, and `Err` are available in
every file **without an import**. `import std.result;` also works (it imports
the same names) and is the documented spelling for readers who want the
provenance explicit. The trio is **non-shadowable**: naming a binding, a
`const`, a parameter, a function, a struct, or an enum `Ok`, `Err`, or
`Result` is a compile error:

```text
`Ok` is a builtin name (declared in `std.result`) and cannot be shadowed —
choose a different name
```

(`ReservedBuiltinName`. This is a behavior change from ≤0.0.9, where the
names could be overwritten Python-style.)

`Ok` and `Err` are constructor expressions. Their types come from context: a
return type annotation, a binding's type annotation, or the scrutinee of a
`choose`. Bare `Ok` / `Err` in expression position (not applied to a payload)
is still rejected.

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}
```

An `Ok`/`Err` constructor whose type cannot be determined from context is a
compile error; add a type annotation.

`Result`'s type arguments accept both equivalent spellings (0.0.7, D9):
`Result<int, string>` and `Result[int, string]`. As a parameterized enum
(chapter 04 §4.6) it shares the same type-argument rules, and nested patterns
like `when Ok(Some(x))` match into it (chapter 06 §6.4).

**Map lookups are Results (0.0.11)**: indexing a `map[K, V]` with `m[k]` —
and the explicit `std.map.get(m, k)` — returns `Result[V, string]`. A missing
key is the value `Err("key not found")`, never a crash; `m[k]?` propagates it
like any other `Result`, and `choose` over it works with the ordinary
`Ok`/`Err` arms (chapter 04 §4.13, chapter 13 §13.12).

## 9.2 Handling a Result with `choose`

```fleen
res = div(10, 2);
out = choose move res {
    when Ok(v) {
        v
    }
    when Err(e) {
        0 - 1
    }
};
// out = 5
```

- `when Ok(v)` / `when Err(e)` bind the payload to `v` / `e` for the arm's
  block.
- An **unguarded** `Ok` + `Err` pair is exhaustive: no `otherwise` is needed.
- **`otherwise` is also legal on a `Result` scrutinee (0.0.9, D2)**: it is
  the always-matching fallback arm and makes any arm set exhaustive. It may
  mix freely with Ok/Err arms; multiple `Ok` arms and multiple `Err` arms
  compose on the same match:

  ```fleen
  func classify(r: Result<int, string>): string {
      choose move r {
          when Ok(v) if v > 100 { "big" }    // guarded: does not cover Ok
          when Ok(v) { "small" }             // unguarded duplicate: covers Ok
          when Err(e) { e }
          otherwise { "unreachable" }        // legal; redundant here
      }
  }
  ```

- Guarded arms (`when Ok(v) if cond`) do **not** count toward exhaustiveness;
  pair a guard with an unguarded duplicate arm or with `otherwise`. Before
  0.0.9 this recipe did not compile (only `Ok`+`Err` arm sets were accepted);
  the guard + `otherwise` combination is now the standard shape:

  ```fleen
  h = choose move parse_result {
      when Ok(v) if v % 2 == 0 { v }    // guard: works since 0.0.7
      otherwise { 0 - 1 }               // 0.0.9: legal fallback
  };
  ```

- Nested patterns keep working through the same match (`when Ok(Some(y))`,
  or-pattern arms, payload guards).
- If the scrutinee is an owned local variable, write `move res` (or
  `clone res`); a fresh call result needs no keyword:

  ```fleen
  out = choose div(10, 2) {
      when Ok(v) { v }
      when Err(e) { 0 - 1 }
  };
  ```

- The payload binding follows move semantics: an owned payload transfers into
  the arm that receives it.
- A missing cover is a typeck error like for every other scrutinee type:
  `choose is not exhaustive for type Result<...>` (the 0.0.2-era
  `ChooseResultNeedsOkErrArms` gate is retired in 0.0.9).
- Several general-path Result `choose`s in one function body are supported
  (a 0.0.9 dispatch fix; each choose verifies independently).

## 9.3 The `?` operator

`?` unwraps a `Result` inside a function that itself returns a `Result`:

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func ratio(a: int, b: int): Result<int, string> {
    q = div(a, b)?;      // if div fails: ratio returns that Err immediately
    r = div(q, 2)?;      // if div succeeds: r is the unwrapped payload
    Ok(r)
}
```

Rules:

- `e?` requires `e: Result<T, E>`.
- The containing function's return type must be `Result<T2, E>` with **the
  same error type `E`**; there is no error-type conversion.
- On `Err`, the function returns that `Err` immediately (the one early-exit
  mechanism in the language).
- On `Ok`, the expression's value is the unwrapped payload.
- `?` is a postfix suffix; it chains with the other suffixes (chapter 05
  §5.7), including `await` (chapter 10 §10.4).

## 9.4 A Result must never be discarded

An expression statement whose value is a `Result<T, E>` is a compile error
(`UnhandledResult`). Every `Result` must be handled:

```fleen
div(1, 0);            // error: an unhandled Result
```

Handle it by binding, propagating with `?`, or matching with `choose`:

```fleen
r = div(1, 0);
choose move r {
    when Ok(v) { print(v as string) }
    when Err(e) { print(e) }
};
```

## 9.5 `main` returning a Result

`main` may return a `Result<T, E>`. When the program ends with an `Err`, the
error payload is printed to standard error and the process exits with code 1:

```fleen
func main(): Result<int, string> {
    if true { Err("startup failed") } else { Ok(0) }
}
```

```text
$ cargo fln demo.fln
startup failed
$ echo $?
1
```

## 9.6 Runtime errors

Some failures are not `Result` values but runtime errors that end the
program: array index out of range (`IndexOutOfRange`), arithmetic overflow,
and channel deadlock (chapter 11 §11.5) are examples. There is no mechanism
to catch a runtime error; `panic`-style failures are not recoverable.

Runtime errors print a message with the source location and exit with a
non-zero code.
