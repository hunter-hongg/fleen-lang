# 04 — Types

Every value has a statically known type. Types are checked before the program
runs; there are no implicit conversions between types.

## 4.1 Primitive types

| Type | Values | Example |
|------|--------|---------|
| `int` | 64-bit signed integers | `42`, `0 - 7` |
| `float` | 64-bit floats | `3.14` |
| `bool` | `true`, `false` | `true` |
| `string` | UTF-8 text | `"hello"` |
| `unit` | the single empty value | the result of `print(...)` |

```fleen
i: int = 42
f: float = 3.14
b: bool = true
s: string = "hello"
```

`unit` is the type of an expression that carries no information, such as a
call to a function without a return value:

```fleen
func greet(name: string) {
    print("Hello, ", name);       // greet returns unit
}
u: unit = greet("ada");
```

## 4.2 Function types

A function type is written `(T1, T2) -> R`:

```fleen
func add(a: int, b: int): int = a + b

func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)

r = apply(add, 1, 2);      // r = 3
```

A function with no parameters has type `() -> R`; one without a return value
has type `(…) -> unit`. Functions are first-class values: they can be bound,
passed as arguments, and returned. A function value is a Copy type.

## 4.3 Arrays `[T]`

`[T]` is an array with elements of type `T`:

```fleen
a = [1, 2, 3];              // a: [int]
names = ["ada", "bob"];     // names: [string]
empty: [int] = [];          // empty literals need an annotation
```

Reading and writing elements uses `[i]`; all indexing is bounds-checked at
runtime:

```fleen
x = a[0];        // x = 1
a[1] = 10;       // a is now [1, 10, 3]
```

Reading an element produces a **copy** of the element. An out-of-range index
(including a negative one) is a runtime error (`IndexOutOfRange`), not a
wrapped access.

Arrays are always **owned** values, even when the element type is Copy:
the whole array is moved or deep-copied (chapter 08):

```fleen
t = clone a;     // deep copy: a new array with new elements
u = move a;      // transfer: a is unusable afterwards
```

Element types must be uniform:

```fleen
bad = [1, "a"];        // error: ArrayTypeMismatch
```

## 4.4 Range

`Range` is a builtin type representing an integer interval with an exclusive
end. A `Range` value is created with the two constructor functions from
`std.collection.range` (usable bare after import, chapter 13):

```fleen
r = range(3);          // the interval 0..3 (0, 1, 2)
r2 = range_from(2, 5); // the interval 2..5 (2, 3, 4)
```

There is no `..` operator syntax. A `Range` is used with `for` (chapter 06
§6.3) and is a Copy type:

```fleen
for i in range(3) {
    print(i as string);      // 0 1 2
};
```

The range is empty when its start equals its end.

## 4.5 box\<T\>

`box<T>` is a heap-allocated cell with a single owner:

```fleen
b = box 42;             // b: box<int>
n = deref b;            // read: 42 (a copy)
deref b = n + 1;        // write: the cell now holds 43
c = clone b;            // a new box with a new inner value
```

Reading `deref b` produces a copy of the inner value. Allowed inner types:
`int`, `float`, `bool`, `string`, `box<U>`, `Result<T, E>`. Moving the value
out of a box (`move deref b`) is rejected — use `clone deref b` to obtain an
owned copy. See chapter 08 §8.4 for the full rules.

## 4.6 ref T

`ref T` is a read-only borrow, allowed **only** as a function parameter type
(chapter 08 §8.5):

```fleen
func shout(s: ref string): int {
    print(s);           // read-only use of the borrowed string
    42
}

name = "fleen";
shout(name);            // borrows name; name remains usable
```

## 4.7 struct

A `struct` declaration defines an aggregate type; the struct name is a type:

```fleen
struct Point {
    x: int,
    y: int,
}

p = Point { x: 1, y: 2 };    // struct literal: every field must be initialized
q = p.x;                     // field read: a copy
p.y = 3;                     // field write
```

Rules:

- Field access uses `.name`; it chains for nested structs.
- A field read is a copy of the field (a deep copy for owned fields).
- A field write `p.f = v` is allowed when `p` is a local or global struct
  variable and `f` is a direct field; if `v` is an owned value it needs
  `move`/`clone` as usual.
- A struct whose fields are all Copy types is itself a Copy type; a struct
  with any owned field (`string`, `box`, array, struct with owned fields) is
  an owned type: passing it to a consuming position needs `move S` or
  `clone S` (chapter 08).
- A struct cannot contain itself, directly or through `box`.
- Visibility is per-struct: `pub` on the declaration makes the whole struct
  importable (chapter 12); there are no per-field access modifiers.
- Structs support neither `==` nor `choose` patterns.

```fleen
struct Wrapper {
    inner: Point,
}

w = Wrapper { inner: Point { x: 1, y: 2 } };
n = w.inner.x;         // 1 — field chains
```

## 4.8 Result\<T, E\>

`Result<T, E>` represents an operation that may fail: it is either
`Ok(value)` with the success payload of type `T`, or `Err(value)` with the
error payload of type `E`:

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}
```

A `Result` may not be silently discarded; it must be handled with `?` or a
`choose`. See chapter 09.

## 4.9 Future\<T\>

`Future<T>` is the type of a value produced by calling an `async func`: a
suspended computation that, when driven, produces a `T`:

```fleen
async func half(n: int): Result<int, string> {
    if n == 0 { Err("cannot halve 0") } else { Ok(n / 2) }
}

f = half(10);           // f: Future<Result<int, string>>
v = f await;            // v: Result<int, string>
```

`Future<T>` is owned, cannot be cloned, and can be driven once. See chapter
10.

## 4.10 channel\<T\>

`channel<T>` is a communication endpoint between concurrent tasks:

```fleen
ch = channel<int>(4);        // a channel carrying int values, capacity 4
```

Channels are owned, cannot be cloned, and are created only with the reserved
constructor `channel<T>(cap)`. See chapter 11.

## 4.11 Copy and owned types (summary)

Every type is either **Copy** or **owned**; this determines the assignment
and argument-passing behavior (chapter 08):

| Category | Types | Binding / assignment / argument semantics |
|----------|-------|-------------------------------------------|
| Copy | `int`, `float`, `bool`, `unit`, function values, `Range`, structs with only Copy fields | implicit copy everywhere |
| Owned | `string`, `[T]` (always), `box<T>`, structs with any owned field, `Result<T, E>` with an owned component, `Future<T>`, `channel<T>` | single owner; moving needs `move`, copying needs `clone` |

`ref T` is neither: it is a borrow, and only exists as a parameter type.

```fleen
i = 42
j = i             // Copy: implicit copy, i remains usable

s = "hello"
t = move s;       // owned: explicit move, s is unusable afterwards
u = clone t;      // owned: deep copy, t remains usable
```
