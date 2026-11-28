# 13 — Standard Library

The standard library ships with the compiler and is imported by leaf module:
`import std.io;` brings in `std.io`. The prelude names (usable without any
import) are `print` and, since 0.0.10, the **Result trio** `Result` / `Ok` /
`Err` (§13.11). There is no re-export mechanism: import each module you use.

## 13.1 std.io — console and input

```fleen
import std.io;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `print` | `(string, ...) -> unit` | prints every argument, no trailing newline |
| `std.io.println` | `(string, ...) -> unit` | prints every argument, then a newline |
| `std.io.read_line` | `() -> string` | reads one line from standard input, without the trailing newline; returns `""` at end of input |
| `std.io.sleep` | `(int) -> unit` | timer, in milliseconds — see chapter 10 §10.6 |

- Arguments to `print`/`println` must be strings; convert scalars with
  `as string` (chapter 05 §5.5).
- `print` is the prelude name: `print(...)` and `std.io.print(...)` are the
  same function, but `print` needs no import.

```fleen
import std.io;

print("a", "b");            // ab (no newline)
std.io.println("c");        // c\n

name = std.io.read_line();
std.io.println("hello ", name);
```

```fleen
import std.io;

n = 42;
std.io.println("n = ", n as string);
```

## 13.2 std.str — text

```fleen
import std.str;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `from_int` | `(int) -> string` | decimal string of an integer |
| `from_float` | `(float) -> string` | string form of a float |
| `from_bool` | `(bool) -> string` | `"true"` / `"false"` |
| `len` | `(string) -> int` | number of characters (Unicode scalar values, not bytes) |
| `concat` | `(string, string) -> string` | the two strings joined |
| `split` | `(string, string) -> [string]` | substrings between every occurrence of the separator, empty parts kept |
| `parse_int` | `(string) -> Result<int, string>` | parses a decimal integer (0.0.9) |
| `parse_float` | `(string) -> Result<float, string>` | parses a decimal float (0.0.9) |
| `trim` | `(string) -> string` | removes leading and trailing Unicode whitespace (0.0.9) |
| `contains` | `(string, string) -> bool` | whether the needle occurs in the string (0.0.9) |
| `starts_with` | `(string, string) -> bool` | prefix membership (0.0.9) |
| `ends_with` | `(string, string) -> bool` | suffix membership (0.0.9) |
| `index_of` | `(string, string) -> Result<int, string>` | first occurrence of the needle (0.0.9) |
| `replace` | `(string, string, string) -> string` | every occurrence of `from` replaced by `to` (0.0.9) |

```fleen
import std.str;

s = std.str.from_int(42);          // "42"
n = std.str.len("héllo");          // 5
t = std.str.concat("foo", "bar");  // "foobar"
parts = std.str.split("a,,b", ",");// ["a", "", "b"]
```

`from_int` / `from_float` / `from_bool` are the typed entry points for
`as string`; prefer `as string` in everyday code.

### The 0.0.9 batch: parsing, searching, replacing

All position indices count **Unicode scalar values** — the same unit `len`
counts, never a byte offset. Where failure is possible the failure is a
value (`Err`), never a crash: extract it with `choose` or `?`.

```fleen
import std.str;
import std.io;

// parse_int / parse_float: decimal only, optional leading `-` or `+` for
// ints. The empty string, surrounding whitespace, non-numeric input, and
// i64 overflow are Err values — trim first if whitespace is possible.
r = std.str.parse_int("42");            // Ok(42)
bad = std.str.parse_int("4x2");         // Err("invalid int: 4x2")
big = std.str.parse_int("9223372036854775808");  // Err: overflow
f = std.str.parse_float("-2e3");        // Ok(-2000.0)

s = std.str.trim("  hi  ");             // "hi"

has = std.str.contains("hello", "ell"); // true
pfx = std.str.starts_with("hello", "he"); // true
sfx = std.str.ends_with("hello", "lo");   // true

ix = std.str.index_of("héllo", "l");    // Ok(2) — scalar index
nf = std.str.index_of("hello", "z");    // Err("substring not found: z")
e0 = std.str.index_of("abc", "");       // Ok(0) — the empty needle is found at 0

rep = std.str.replace("aaa", "a", "b"); // "bbb"
noop = std.str.replace("abc", "", "X"); // "abc" — an empty `from` is a no-op
```

Design notes (D3):

- `index_of` returns a `Result` rather than a `-1` sentinel: the failure is a
  value, the same posture as `std.arr.get`. `contains` / `starts_with` /
  `ends_with` return `bool` because a question has a total answer.
- The empty needle is contained in every string, matches as a prefix/suffix
  of every string, and is found at `Ok(0)`.
- `replace` never splits a scalar value and an empty `from` is a no-op —
  deliberately not Rust's insert-at-every-boundary behavior.
- `substring` / `slice` / `chars` are **not** provided in 0.0.9 (deferred to
  the 0.1.0 string work); neither is iterating a string with `for`.

## 13.3 std.num — numbers

```fleen
import std.num;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `abs` | `(int) -> int` | absolute value |
| `min` | `(int, int) -> int` | the smaller argument |
| `max` | `(int, int) -> int` | the larger argument |
| `sqrt` | `(float) -> float` | non-negative square root |
| `floor` | `(float) -> int` | rounds down to the nearest integer |

```fleen
import std.num;

a = std.num.abs(0 - 5);        // 5
b = std.num.min(3, 7);         // 3
c = std.num.max(3, 7);         // 7
d = std.num.sqrt(2.0);         // 1.4142…
e = std.num.floor(3.9);        // 3
```

`std.num.abs` of the minimum `int` overflows and is a runtime arithmetic
error.

## 13.4 std.arr — arrays

All four functions work on `[T]` for any element type; the element type is
inferred per call. Arrays are owned, so `set` and `push` return a **new**
array and do not modify their argument.

```fleen
import std.arr;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `len` | `([T]) -> int` | number of elements |
| `get` | `([T], int) -> Result<T, string>` | element at the index; `Ok` in bounds, `Err` otherwise |
| `set` | `([T], int, T) -> [T]` | a new array with element `i` replaced |
| `push` | `([T], T) -> [T]` | a new array with the value appended |

```fleen
import std.arr;

a = [1, 2, 3];
n = std.arr.len(a);                     // 3
r = std.arr.get(a, 1);                  // Ok(2)
b = std.arr.set(a, 0, 9);               // [9, 2, 3]; a is still [1, 2, 3]
c = std.arr.push(b, 4);                 // [9, 2, 3, 4]
bad = std.arr.get(a, 99);               // Err(...)
```

`std.arr.get` returns a `Result`, so a possible out-of-range read can be
handled with `choose` or `?` instead of crashing.

## 13.5 std.collection.range — Range constructors

```fleen
import std.collection.range;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `range` | `(int) -> Range` | the interval `0..n` |
| `range_from` | `(int, int) -> Range` | the interval `a..b` |

Both ends are exclusive; the range is empty when start equals end. After the
import, the names are also reachable without the `std.collection.range.`
prefix.

```fleen
import std.collection.range;
import std.io;

for i in range(3) {
    std.io.println(i as string);        // 0 1 2
};
for i in range_from(2, 5) {
    std.io.println(i as string);        // 2 3 4
};
```

## 13.6 std.collection.iter — eager iteration helpers

```fleen
import std.collection.iter;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `map` | `([T], (T) -> U) -> [U]` | applies `f` to every element |
| `filter` | `([T], (T) -> bool) -> [T]` | keeps the elements where `f` is true |
| `foldl` | `([T], A, (A, T) -> A) -> A` | folds left with the accumulator: `f(acc, elem)` left to right (renamed from `fold` in 0.0.6) |
| `foldr` | `([T], A, (T, A) -> A) -> A` | folds right: `f(elem, acc)` right to left (0.0.6) |
| `take` | `([T], int) -> [T]` | the first `n` elements |

All helpers are **eager** (arrays in, arrays out) and read elements as
copies: the source array is never consumed, and the function you pass never
receives the original elements.

```fleen
import std.collection.iter;

func twice(n: int): int = n * 2
func is_even(n: int): bool { n % 2 == 0 }
func add(a: int, b: int): int = a + b

xs = [1, 2, 3, 4];
doubled = std.collection.iter.map(xs, twice);       // [2, 4, 6, 8]
evens   = std.collection.iter.filter(xs, is_even);  // [2, 4]
total   = std.collection.iter.foldl(xs, 0, add);    // 10
head    = std.collection.iter.take(xs, 2);          // [1, 2]
```

Fold directions (0.0.6, D5): `foldl([1,2,3], 0, sub)` is `(0-1)-2-3`;
`foldr([1,2,3], 0, sub)` is `1-(2-(3-0))` (the array is walked backward by
index, no reverse copy).

## 13.7 std.async — the executor

```fleen
import std.async;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `run` | `([Future<unit>]) -> unit` | runs every future to completion (chapter 10 §10.5) |
| `yield_now` | `() -> unit` | parks the current task for one scheduling turn; no-op in sync code |

```fleen
import std.async;

async func work() { /* … */ }

func main() {
    std.async.run([work()]);
}
```

## 13.8 std.concurrent — channels

```fleen
import std.concurrent;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `split` | `(channel<T>) -> channel<T>` | a second handle to the same buffer (chapter 11 §11.4) |

The channel **constructor** is the reserved word `channel<T>(cap)`, not a
function of this module.

```fleen
import std.concurrent;

ch = channel<int>(2);
peer = std.concurrent.split(ch);
```

## 13.9 std.os — process

```fleen
import std.os;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `args` | `() -> [string]` | the program arguments, without the program name |
| `exit` | `(int) -> unit` | terminates the process with the given exit code; never returns |

```fleen
import std.os;

args = std.os.args();
if std.arr.len(args) == 0 {
    std.os.exit(1);        // stop immediately with exit code 1
};
```

## 13.10 std.time — clock

```fleen
import std.time;
```

| Function | Signature | Description |
|----------|-----------|-------------|
| `now_millis` | `() -> int` | wall-clock time in milliseconds |

```fleen
import std.time;

start = std.time.now_millis();
/* … work … */
elapsed = std.time.now_millis() - start;
```

## 13.11 std.result — the `Result` declaration (0.0.10)

```fleen
import std.result;    // optional — the trio is prelude
```

`std/result.fln` holds the declaration of `Result` (0.0.10, D1) — the
language-level source of truth:

```fleen
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

- The module contains **no functions**: the compiler recognizes
  `std.result.Result` by canonical path, maps it onto its internal
  representation, and emits no `enum_defs` entry and no variant codegen — the
  module is declaration-only.
- `import std.result;` imports the `Result` name; the `Ok` / `Err` variants
  travel with the export. It imports the very same names that are already
  prelude, so the explicit import is a provenance nicety, not a requirement.
- The declaration cannot be shadowed or re-declared: an `enum Result` or any
  binding/parameter/function named `Ok`, `Err`, or `Result` is
  `ReservedBuiltinName` (chapter 09 §9.1).
- Error handling itself is chapter 09: `?` propagation and choose-on-Result
  keep their exact 0.0.2/0.0.9 semantics and lowering (0.0.10, D3).
