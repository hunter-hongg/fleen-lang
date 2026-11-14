# 09 — Error Handling

## 9.1 `Result<T, E>`

`Result<T, E>` is the type of an operation that may fail. It has two
variants:

- `Ok(value)` — success, carrying the payload of type `T`
- `Err(value)` — failure, carrying the error payload of type `E`

`Ok` and `Err` are builtin constructor expressions. Their types come from
context: a return type annotation, a binding's type annotation, or the
scrutinee of a `choose`.

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}
```

An `Ok`/`Err` constructor whose type cannot be determined from context is a
compile error; add a type annotation.

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
- Guarded arms (`when Ok(v) if cond`) do **not** count toward exhaustiveness;
  pair a guard with an unguarded duplicate arm or with `otherwise`.
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
