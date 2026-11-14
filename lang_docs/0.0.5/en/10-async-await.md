# 10 — Async and Await

## 10.1 `async func`

`async` marks a function as asynchronous:

```fleen
async func half(n: int): Result<int, string> {
    if n == 0 { Err("cannot halve 0") } else { Ok(n / 2) }
}
```

The signature rule: **the body returns `T`, and the caller receives
`Future<T>`**. Calling the function does not run the body — it returns a
future value that can be driven later:

```fleen
f = half(10);                 // f: Future<Result<int, string>>; the body has not run
```

An async function's body is ordinary Fleen code: bindings, `choose`, `?`,
`for`, calls to other functions. The declaration combines with `pub` for
cross-module use (chapter 12):

```fleen
pub async func add(a: int, b: int): int {
    a + b
}
```

There is no `async {}` block form; only `async func` declarations.

## 10.2 `Future<T>`

`Future<T>` is the type of the value returned by calling an `async func`
whose body returns `T`.

| Property | Behavior | Example |
|----------|----------|---------|
| Owned | moves with `move`, deep-copies with `clone` — but `clone f` is rejected | `g = move f;` |
| Drive-once | one `await` consumes it; it cannot be driven twice | `v = f await;` |
| Storable in a local | can be bound and passed around before being driven | `f: Future<int> = add(20, 22);` |
| Not cloneable | `clone f` is a compile error | |
| Not a struct field / box cell / global | rejected at compile time | |
| Not indexable/iterable in containers | `ts[i]` on `[Future<T>]` and `for` over it are rejected | |

```fleen
f: Future<int> = add(20, 22);
v = f await;                  // v = 42
```

`Future` is not a reserved word: only the parameterized form `Future<T>` is
recognized in type position.

## 10.3 `await`

`e await` drives the future `e` to completion and yields its payload. It is
a **postfix** operator that binds in the tightest suffix tier, chaining
left-to-right with `()`, `[i]`, `.f`, and `?`:

```fleen
n = add(20, 22) await;              // n = 42
add(20, 22) await;                  // statement form: drive, discard the payload
r = fib(n - 1) await + fib(n - 2) await;   // no parentheses needed
```

Rules:

- `e await` requires `e: Future<T>` and yields `T`; applying `await` to any
  other type is a compile error.
- `await` consumes the future; using the future afterwards is a use-after-
  move error.
- If the future's execution failed, the error is raised at the `await`.
- A top-level `e await` (for example in `main`) runs the executor until that
  future completes — the program blocks there until it finishes.

```fleen
import std.io;

async func add(a: int, b: int): int { a + b }

func main() {
    v = add(20, 22) await;
    std.io.println((v as string));
}
```

## 10.4 `await?`

`?` composes with `await`. Inside a function returning `Result<T, E>`:

```fleen
func halve(n: int): Result<int, string> {
    v = half(n) await?;      // parsed as (half(n) await)?
    Ok(v)
}
```

`half(n) await?` drives the future (yielding `Result<int, string>`), then
applies `?`: an `Err` propagates out of `halve` immediately, an `Ok` payload
continues.

## 10.5 The executor: `std.async.run`

`std.async.run(tasks)` takes an array of futures and runs all of them to
completion:

```fleen
import std.async;

async func task_a() { /* … */ }
async func task_b() { /* … */ }

func main() {
    std.async.run([task_a(), task_b()]);
}
```

- The argument is `[Future<unit>]`: async functions whose bodies return
  `unit`. Calling `task_a()` does not run it; `run` does.
- `run` returns only after **every** task in the list has finished (the join).
- Tasks run one at a time on a single executor, in **round-robin** order. A
  task switches out only at a suspension point (§10.6); between suspension
  points a task runs uninterrupted.
- `run` can also be called inside an async body; the outer task then waits
  for the inner batch to finish before continuing:

  ```fleen
  async func orchestrator() {
      std.async.run([inner_a(), inner_b()]);   // waits for both
      more_work();
  }
  ```

- If the tasks can never make progress (all blocked on channels), `run`
  raises a `ChannelDeadlock` runtime error (chapter 11 §11.5).

## 10.6 Suspension points

A task yields to the executor only at these operations inside an async body:

| Operation | Behavior |
|-----------|----------|
| `std.io.sleep(ms)` | parks the task until `ms` milliseconds elapse |
| `std.async.yield_now()` | parks the task for one scheduling turn |
| `c.send(v)` on a full channel | parks until space frees (chapter 11) |
| `c.recv()` on an empty channel | parks until a value arrives (chapter 11) |

Inside an async body these are written **without** `await` — the compiler
inserts the suspension:

```fleen
import std.async;
import std.io;

async func worker(label: string) {
    std.io.sleep(5);            // the task parks here
    std.io.println(label, " woke");
}

async func ticker() {
    for i in range(3) {
        std.io.println(i as string);
        std.async.yield_now();  // let another task run
    }
}
```

In a **synchronous** (non-async) context, `std.io.sleep` and
`std.async.yield_now` do nothing.

Scheduling order is deterministic:

- a task that is ready to run always runs before a timer whose deadline has
  passed;
- timers that expire together wake in the order they were created;
- tasks parked on a channel wake in the order they parked.

The same program therefore produces the same output on every run.

## 10.7 Rules inside async bodies

- Parameters and values must be owned or Copy: an async function cannot
  declare a `ref` parameter, and a `ref` handle cannot be held across an
  `await`, sent through a channel, or given to a task.
- Globals can be **read** (each read deep-copies) but not **written** from an
  async body; a field write, index write, or module-qualified write that
  reaches a global is a compile error. Send the value through a channel
  instead (chapter 11).
- A synchronous function called from async code may still take `ref`
  parameters: the borrow lives only for that call.

```fleen
struct Counter { n: int }
count = Counter { n: 0 };      // global

async func bump() {
    count.n = 1;               // compile error: writing a global from async code
}

async func report(out: channel<int>) {
    out.send(count.n);         // fine: read, then communicate
}
```
