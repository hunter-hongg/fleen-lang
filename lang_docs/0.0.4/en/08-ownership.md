# 08 — Ownership: move, clone, box, ref

Every value is either **Copy** or **owned** (chapter 04 §4.11). This chapter
describes what follows from that distinction in everyday code.

## 8.1 Copy values

`int`, `float`, `bool`, `unit`, function values, `Range`, and structs whose
fields are all Copy are **Copy** values. Binding, assignment, argument
passing, and returns all copy them implicitly. No keywords are ever needed:

```fleen
a = 42
b = a              // copied; both usable
c = b              // copied again
```

Applying `move` or `clone` to a Copy value is a compile error (the fix is to
remove the keyword).

## 8.2 Owned values and `move`

`string`, arrays, `box<T>`, structs with an owned field, `Result` with an
owned component, `Future<T>`, and `channel<T>` are **owned** values: each has
exactly one owner at any time.

When an owned **local variable** lands in a *consumption position*, the
transfer must be explicit. Consumption positions are:

- the top level of a binding or assignment right-hand side:
  `t = <here>`
- a call argument: `f(<here>)`
- a `choose` scrutinee: `choose <here> { … }`

Write `move x` to transfer ownership, or `clone x` to pass a deep copy and
keep `x`:

```fleen
s = "hello";
t = move s;        // ownership moves to t; s is unusable afterwards
```

```fleen
s = "hello";
t = clone s;       // deep copy; s remains usable
```

Writing a bare owned variable in a consumption position is a compile error;
the error message names the missing keyword:

```fleen
s = "hello";
t = s;             // error: owned value `s` must be transferred explicitly
                   // help: write `move s` or `clone s`
```

After a `move`, the source variable can no longer be used:

```fleen
s = "hello";
t = move s;
print(s);          // error: use of moved value `s`
```

## 8.3 Values that transfer implicitly

- **Fresh values need no keyword.** A literal, a call result, an `as string`
  result, a struct/array literal, a constructor such as `Ok(...)` — the value
  did not exist before, so it transfers naturally:

  ```fleen
  s = "hello";          // fresh: no move needed
  t = clone s;
  u = concat(t, "!");   // fresh call result: no move needed
  v = Point { x: 1, y: 2 };
  ```

- **Production positions transfer implicitly**: the last expression of a
  function, block, or branch flows out without a keyword:

  ```fleen
  func make(): string {
      s = "fresh";
      s             // no move needed: it flows out of the function
  }
  ```

  A transfer at the end of a *branch* is conditional; using the variable
  after the branch reports that it was possibly moved.

- **Read-only uses never transfer**: comparison operators and `print`/
  `println` arguments copy automatically:

  ```fleen
  s = "hello";
  if s == "hello" {     // read-only: s remains usable
      print(s);         // read-only: s remains usable
  };
  ```

## 8.4 What cannot be moved out of

A `move` cannot reach *inside* a container or a global — the read forms
produce copies instead:

| Expression | Result |
|------------|--------|
| `move deref b` | compile error — use `clone deref b` |
| `move a[i]` | compile error — reads copy; `a[i] = v` writes in place |
| `move p.f` | compile error — reads copy; `p.f = v` writes in place |
| `move g` (a global) | compile error — reads deep-copy; writes replace |

```fleen
b = box "inner";
s = clone deref b;    // owned copy of the inner value; b remains usable
```

```fleen
a = ["x", "y"];
e = a[0];             // a copy; a remains usable
a[1] = "z";           // in-place write
```

```fleen
g = "global text";
h = g;                // reading an owned global deep-copies; no keyword needed
```

Additional rules:

- `deref b = v` (writing through a box) requires `b` to be a **local** box
  variable. Writing through a global box is rejected; clone the box into a
  local first.
- `move`/`clone` written on a `ref` parameter is an error; `clone s` on a ref
  parameter produces an owned copy of the borrowed value.

## 8.5 `ref` — read-only borrow parameters

`ref T` may appear **only** as a function parameter type. It lets a function
read a caller's value without taking ownership:

```fleen
func shout(s: ref string): int {
    print(s);          // read-only use
    42
}

func main(): int {
    name = "fleen";
    shout(name);       // borrow: no move/clone, name remains usable
    shout(name);       // borrow again: legal
    0
}
```

Rules:

- Ref parameters are **read-only**: assigning to one or moving it is a
  compile error. `clone s` inside the function yields an owned copy.
- A ref handle can be forwarded to another function's ref parameter; it
  cannot be stored, returned, or assigned to a variable.
- The argument to a `ref T` parameter must be a **variable** — a local, a
  global, or another ref parameter. Literals, call results, and other
  temporaries are rejected: there is nothing for the borrow to point at after
  the call.
- `ref string` and `string` are different types: assigning a borrow to an
  owned binding is a type error (use `clone s`); passing `move x` where a
  `ref` is expected is also an error.
- Borrow arguments are **not transferred** and Copy/owned locals stay usable.

```fleen
func shout(s: ref string) {
    print(s);          // ok: read-only
    s = "x";           // error: cannot assign to a borrowed parameter
}
```

## 8.6 Box: allocate, read, write, copy

`box expr` heap-allocates; `deref b` reads; `deref b = v` writes; `clone b`
copies the whole box.

```fleen
b = box 42;            // b: box<int>
n = deref b;           // 42
deref b = n + 1;       // the cell now holds 43
c = clone b;           // new box, new inner value
deref c = 100;         // b still holds 43; the boxes are independent
```

```fleen
s = box "heap";
t = clone deref s;     // an owned copy of the inner string; the box untouched
```

Allowed inner types: `int`, `float`, `bool`, `string`, `box<U>`,
`Result<T, E>`. `box` and `deref` are reserved words; the type is spelled
`box<T>` in type position and used as a prefix expression in code.

## 8.7 Summary table

| You write | Meaning |
|-----------|---------|
| `x = expr` (fresh RHS) | bind; no keyword needed for any type |
| `x = y` (owned local `y`) | error — write `move y` or `clone y` |
| `x = y` (Copy `y`) | implicit copy |
| `x = move y` | transfer ownership; `y` unusable afterwards |
| `x = clone y` | deep copy; `y` remains usable |
| `f(y)` (owned local `y`) | error — write `f(move y)` or `f(clone y)` |
| `f(y)` where `p: ref T` | borrow; no keyword, `y` remains usable |
| `deref b` | copy of the box's inner value |
| `deref b = v` | replace the inner value (local box `b` only) |
| `move deref b` / `move a[i]` / `move p.f` / `move g` | compile errors — clone instead |
