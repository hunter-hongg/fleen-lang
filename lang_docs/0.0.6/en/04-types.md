# 04 — Types

Every value has a statically known type. Types are checked before the program runs; there are no implicit conversions between types.

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

`unit` is the type of an expression that carries no information, such as a call to a function without a return value:

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

A function with no parameters has type `() -> R`; one without a return value has type `(…) -> unit`. Functions are first-class values: they can be bound, passed as arguments, and returned. A function value is a Copy type.

## 4.3 Arrays `[T]`

`[T]` is an array with elements of type `T`:

```fleen
a = [1, 2, 3];              // a: [int]
names = ["ada", "bob"];     // names: [string]
empty: [int] = [];          // empty literals need an annotation
```

Reading and writing elements uses `[i]`; all indexing is bounds-checked at runtime:

```fleen
x = a[0];        // x = 1
a[1] = 10;       // a is now [1, 10, 3]
```

Reading an element produces a **copy** of the element. An out-of-range index (including a negative one) is a runtime error (`IndexOutOfRange`), not a wrapped access.

Arrays are always **owned** values, even when the element type is Copy: the whole array is moved or deep-copied (chapter 08):

```fleen
t = clone a;     // deep copy: a new array with new elements
u = move a;      // transfer: a is unusable afterwards
```

Element types must be uniform:

```fleen
bad = [1, "a"];        // error: ArrayTypeMismatch
```

## 4.4 Range

`Range` is a builtin type representing an integer interval with an exclusive end. A `Range` value is created with the two constructor functions from `std.collection.range` (usable bare after import, chapter 13):

```fleen
import std.collection.range;

r = range(3);          // the interval 0..3 (0, 1, 2)
r2 = range_from(2, 5); // the interval 2..5 (2, 3, 4)
```

There is no `..` operator syntax. A `Range` is used with `for` (chapter 06 §6.3) and is a Copy type:

```fleen
for i in range(3) {
    print(i as string);      // 0 1 2
};
```

The range is empty when its start equals its end.

## 4.5 Struct

A `struct` is a user-defined aggregate. All fields must be initialized in a struct literal:

```fleen
struct Point {
    x: int,
    y: int,
}

p = Point { x: 1, y: 2 };
n = p.x;        // field read (Copy field copies; owned fields follow ownership)
p.y = 3;        // field write (direct field of a local/global only)
```

Copy-ness is derived from fields: a `struct` is Copy if every field is Copy; if any field is owned, the `struct` is owned.

```fleen
struct Pair { a: int, b: int }           // Copy
struct Wrap { s: string }               // owned
```

Use `pub` for cross-module visibility (chapter 12).

## 4.6 Enum (`enum`, 0.0.6)

An `enum` is a tagged union with ordered variants and typed payloads (zero or more):

```fleen
enum Shape {
    Point,
    Circle(float),
    Rect(float, float),
}

s = Shape.Circle(2.0);    // variant constructor (dotted form EnumName.Variant)
```

- `enum` is reserved from 0.0.6 onward.
- Variant names are constructors; both the bare name and `EnumName.Variant` forms are available in the appropriate contexts.
- **Copy-ness**: `is_copy(E)` iff all payloads of all variants are Copy (same rule as `struct`).
- **Ownership**: an enum with any owned payload is owned. Consume an owned enum with `choose move s { ... }`, or use `move`/`clone` as needed (chapter 08).
- **Not comparable**: enum values do not support `==` (type error); `as string` is not supported for enums.
- **No recursive enums**: recursive enums (directly or indirectly) are rejected at typeck (`RecursiveEnum`). Use `box` indirection once available.
- **Pattern binding limit (0.0.6)**: `when` patterns bind the **first payload only** (e.g. `when Circle(r) { ... }`). Multi-payload variants can be constructed but not fully destructured in a single arm.

`choose` over an enum is exhaustively checked: every variant must have an arm, or an `otherwise` arm must be present (chapter 06 §6.5):

```fleen
func area(s: Shape): float {
    choose move s {
        when Point { 0.0 }
        when Circle(r) { 3.14159 * r * r }
        when Rect(w) { w * 0.0 }  // binds first payload only
    }
}
```

## 4.7 box\<T\>

`box<T>` is a heap-allocated cell with a single owner:

```fleen
b = box 42;             // b: box<int>
n = deref b;            // read: 42 (a copy)
deref b = n + 1;        // write: the cell now holds 43
c = clone b;            // a new box with a new inner value
```

`deref b` produces a copy of the inner value. Allowed inner types: `int`, `float`, `bool`, `string`, `box<U>`, `Result<T, E>`. Moving the value out of a box (`move deref b`) is rejected — use `clone deref b`. See chapter 08 §8.4.

## 4.8 ref T

`ref T` is a read-only borrow, allowed **only** as a function parameter type (chapter 08 §8.5):

```fleen
func shout(s: ref string): int {
    print(s);
    42
}

name = "fleen";
r = shout(name);
```

## 4.9 Result\<T, E\>

`Result<T, E>` is `Ok(value)` or `Err(error)`:

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

q = div(10, 2)?;        // propagate errors in a Result-returning function (chapter 09)
```

`Result` is owned. `Ok` and `Err` are constructors. Use `?` to propagate, `choose` to match (chapter 09).

## 4.10 Future\<T\>

`async func` returns `Future<T>`. Drive it with postfix `await` to get `T`:

```fleen
async func half(n: int): int { n / 2 }

r = half(10) await;     // r: int
```

`Future<T>` is owned, non-Copy, and cannot cross the async boundary (await/task/channel) (chapter 10).

## 4.11 channel\<T\>

`channel<T>` is a communication endpoint between concurrent tasks:

```fleen
ch = channel<int>(4);        // capacity 4
```

Created by `channel<T>(cap)`, move-only, owned, non-Copy (chapter 11).

## 4.12 ptr\<T\> (0.0.5, unsafe)

`ptr<T>` is a raw pointer used only inside `unsafe`/`trusted` function bodies. Operations: `addr x` (take address), `deref_ptr p` (dereference).

Rules (chapter 16):
- `ptr<T>` is non-Copy, move-only, and **non-storable** (no struct fields, array elements, box cells, or globals).
- `ptr<T>` never crosses `await`/task/channel boundaries.
- `addr` rejects immutable bases (`AddrOfImmutable`) and `ref`-parameter bases (`AddrOfRefParam`).
- Every pointer use is an unsafe operation and must be in `unsafe` or `trusted` context.

```fleen
unsafe func raw_op(x: int): int {
    p = addr x;            // p: ptr<int>
    n = deref_ptr p;
    deref_ptr p = n + 1;
    n
}
```

## 4.13 Copy and owned types (summary)

| Category | Types | Semantics |
|----------|-------|-----------|
| Copy | `int`, `float`, `bool`, `unit`, function values, `Range`, structs with all Copy fields, enums with all Copy payloads | implicit copy |
| Owned | `string`, `[T]` (always), `box<T>`, structs with any owned field, `Result<T,E>` with owned components, `Future<T>`, `channel<T>`, enums with any owned payload, `ptr<T>` (non-Copy, move-only) | single owner; `move` transfers, `clone` deep-copies |

`ref T` is a borrow (parameter only).
