# 08 — Ownership: move, clone, box, ref

Every value is either **Copy** or **owned** (chapter 04 §4.14). This chapter
describes what follows from that distinction in everyday code.

## 8.1 Copy values

`int`, `float`, `bool`, `unit`, function values, `Range`, structs whose
fields are all Copy, and enums whose payloads are all Copy are **Copy** values.
A parameterized enum is classified per type argument (0.0.7, D2):
`Option<int>` is Copy, `Option<string>` is owned. Binding, assignment, argument
passing, and returns all copy them implicitly. No keywords are ever needed:

```fleen
a = 42
b = a              // copied; both usable
c = b              // copied again
```

Applying `move` or `clone` to a Copy value is a compile error (the fix is to
remove the keyword).

> **Partial-application capture semantics (0.0.8, D11)**: function values are Copy, and so are **partials** (chapter 07 §7.4.1). Captured arguments live inside the function value at runtime; binding a partial, or copying it with `clone` / assignment, deep-copies those captures. A partial therefore copies like any Copy value. If a captured argument is an owned value, the "capture" happens at the call site after it has already been moved in under the ordinary ownership rules — copying the partial copies the captured values and never re-visits the original variable.

## 8.2 Owned values and `move`

`string`, arrays, `map[K, V]` (0.0.11 — always owned, the array posture), `box<T>`, structs with an owned field, `Result` with an
owned component, `Future<T>`, and `channel<T>` are **owned** values: each has
exactly one owner at any time.

Reading a value **out** of an owned container never consumes the container:
`m[k]` on a map copies the value out and the map stays usable (chapter 04
§4.13). Passing the map itself to a function moves it; the `std.map.insert`
/ `std.map.remove` mutators consume their input and return a fresh map
(chapter 13 §13.12).

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


`ptr<T>` does not support `clone` (0.0.5); `ptr<T>` may not cross `await`/task/channel boundaries (`PtrAcrossBoundary`). See chapter 16.
