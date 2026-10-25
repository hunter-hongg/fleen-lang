# 13 — Standard Library

The standard library ships with the compiler and is imported by leaf module:
`import std.io;` brings in `std.io`. The only prelude name (usable without
any import) is `print`. There is no re-export mechanism: import each module
you use.

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

```fleen
import std.str;

s = std.str.from_int(42);          // "42"
n = std.str.len("héllo");          // 5
t = std.str.concat("foo", "bar");  // "foobar"
parts = std.str.split("a,,b", ",");// ["a", "", "b"]
```

`from_int` / `from_float` / `from_bool` are the typed entry points for
`as string`; prefer `as string` in everyday code.

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
| `fold` | `([T], A, (A, T) -> A) -> A` | folds left with the accumulator |
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
total   = std.collection.iter.fold(xs, 0, add);     // 10
head    = std.collection.iter.take(xs, 2);          // [1, 2]
```

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
