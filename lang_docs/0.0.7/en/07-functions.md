# 07 — Functions

## 7.1 Declaration

A function is declared with `func`. A declaration takes **no** trailing
semicolon.

Single-line form — the body is one expression after `=`:

```fleen
func add(a: int, b: int): int = a + b
```

Multi-line form — the body is a block; the **last expression without a
semicolon** is the return value:

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}
```

A function without a return type returns `unit`:

```fleen
func greet(name: string) {
    print("Hello, ", name);
}
```

## 7.2 Parameters

Parameters are written `name: type`:

```fleen
func point_sum(p: Point, q: Point): int = p.x + p.y
```

A parameter type may be:

- any Copy or owned type — `int`, `string`, `[T]`, a struct, `box<T>`,
  `Result<T, E>`, `Future<T>`, `channel<T>`, a function type;
- a borrow `ref T` — a read-only parameter (chapter 08 §8.5).

Owned arguments follow the ownership rules of chapter 08: a fresh value or a
Copy value is passed as-is; an owned variable must be written `move x` or
`clone x` at the call:

```fleen
func length(s: string): int = std.str.len(s)

name = "ada";
n = length(move name);      // name is moved; use clone name to keep it
```

## 7.3 Return values

- The return type is written after `: ` following the parameter list; omit it
  for a `unit` return.
- The value of a single-line body is the expression after `=`.
- The value of a multi-line body is its last expression **without** a
  semicolon:

  ```fleen
  func f(): int {
      x = 1;
      x + 1          // returns 2
  }
  ```

  Writing `x + 1;` discards the value.

- There is no `return` statement. The one early-exit mechanism is the `?`
  operator on a `Result` (chapter 09 §9.3).
- If the declared return type is `Result<T, E>`, every exit path must produce
  a `Result`.

## 7.4 Calling

A call is `name(args)`:

```fleen
r = add(1, 2);
greet("bob");
```

- The argument types must match the parameter types exactly; there are no
  implicit conversions and no default arguments.
- Function overloading does not exist: one name, one function.
- A function may call itself (recursion):

  ```fleen
  func fact(n: int): int {
      if n <= 1 { 1 } else { n * fact(n - 1) }
  }
  ```

- Calling an `async func` produces a `Future` and does not run the body;
  see chapter 10.

## 7.5 Functions as values

Functions are first-class values with function types (chapter 04 §4.2): they
can be bound, passed, and returned.

```fleen
func add(a: int, b: int): int = a + b
func sub(a: int, b: int): int = a - b

func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)

op = add;
r = apply(op, 10, 3);       // 13
r2 = apply(sub, 10, 3);     // 7
```

A function value is a Copy type: binding or passing it copies it.

## 7.6 Anonymous functions

There is no anonymous-function syntax. Define a named function and pass its
name:

```fleen
func twice(n: int): int = n * 2

xs = [1, 2, 3];
ys = std.collection.iter.map(xs, twice);    // [2, 4, 6]
```

## 7.7 Where functions can be declared

- At file top level (module functions; mark them `pub` to export them,
  chapter 12).
- A function cannot be declared inside another function or inside a block.
- A function name can shadow a builtin (for example, defining your own
  `print`); the user definition wins within its scope.
