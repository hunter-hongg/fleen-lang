# 11 — Channels

## 11.1 Creating a channel

`channel<T>(cap)` creates a channel carrying values of type `T` with a
buffer capacity of `cap`:

```fleen
ch = channel<int>(16);       // buffered: up to 16 values fit before send parks
ch = channel<string>(4);     // channel carrying strings
ch = channel<int>(0);        // rendezvous: no buffer; send hands the value over directly
```

- `channel` is a reserved word with two forms: the type `channel<T>` (in type
  position) and the constructor `channel<T>(cap)` (in expression position).
- `cap = 0` is a **rendezvous** channel: nothing is buffered, and a `send`
  parks until a receiver takes the value.
- `cap < 0` is a runtime error.

## 11.2 Sending and receiving

`c.send(v)` puts a value into the channel; `c.recv()` takes a value out.

```fleen
import std.async;
import std.concurrent;
import std.io;

async func producer(c: channel<int>) {
    c.send(1);
    c.send(2);
}

async func consumer(c: channel<int>) {
    a = c.recv();            // a = 1
    b = c.recv();            // b = 2
    std.io.println(a as string, b as string);
}
```

Blocking behavior:

| Operation | Parks when |
|-----------|------------|
| `c.send(v)` | the buffer is full — or, at capacity 0, until a receiver takes the value |
| `c.recv()` | the buffer is empty |

Both operations are written without `await` inside an async body; the task
parks automatically and is woken when the operation can proceed. Values are
received in the order they were sent (FIFO).

## 11.3 Payload ownership

- A **Copy** payload (`int`, `float`, `bool`, `unit`, function values) is
  copied through the channel.
- An **owned** payload (for example a `string`) transfers ownership: after
  `s = c.recv()`, the receiving task owns the value.
- An owned **variable** must be moved or cloned explicitly at the send; a
  **fresh** value transfers naturally:

  ```fleen
  async func send_things(c: channel<string>) {
      a = "hello";
      c.send(move a);        // owned variable: explicit move required
      c.send("world");       // fresh value: no keyword
  }
  ```

  Writing `c.send(a)` for an owned variable `a` is a compile error; the
  message suggests `move a` or `clone a`.
- A `ref` payload is rejected at compile time: borrows never cross a channel.

## 11.4 `split` — a second handle

A channel value has a single owner. `std.concurrent.split(ch)` returns a
**second handle to the same buffer**; the two handles are independent values
addressing one channel:

```fleen
ch = channel<int>(0);
peer = std.concurrent.split(ch);

std.async.run([sender(move ch), receiver(move peer)]);
```

- `split` is the only way one channel gains two handles.
- `clone ch` is a compile error — a channel cannot be cloned; use `split`.
- A channel cannot be stored in a struct field or array element; keep handles
  in locals and pass them with `move`.

## 11.5 Deadlock

There is no channel `close` operation, and `for` cannot iterate a channel.
The consequence: when a `run`'s task set can make no further progress — every
remaining task is parked on a channel send or receive that no other task will
ever satisfy — the runtime raises a `ChannelDeadlock` error immediately
instead of waiting:

```fleen
async func lonely(c: channel<int>) {
    v = c.recv();            // nobody will ever send
}

func main() {
    ch = channel<int>(0);
    std.async.run([lonely(move ch)]);    // runtime error: ChannelDeadlock
}
```

Because there is no `close`, a sender signals "done" with a protocol value of
its own choosing — for example a `Result` or a sentinel payload — which the
receiver matches with `choose`.

## 11.6 Complete example

```fleen
import std.async;
import std.concurrent;
import std.io;

async func ping(ch: channel<int>) {
    n = 0;
    while n < 3 {
        ch.send(n);
        n = n + 1;
    }
}

async func pong(peer: channel<int>) {
    got = 0;
    while got < 3 {
        v = peer.recv();
        std.io.println("got ", v as string);
        got = got + 1;
    }
}

func main() {
    ch = channel<int>(0);              // rendezvous channel
    peer = std.concurrent.split(ch);
    std.async.run([ping(move ch), pong(move peer)]);
}
```

```text
$ cargo fln pingpong.fln
got 0
got 1
got 2
```
