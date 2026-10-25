# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and version semantics follow [SPEC.md](SPEC.md) §13 (`0.0.x` = early development, unstable API).

## 0.0.4 — Patient Ants (2026-10-25) 🐜

The release that **lets tasks talk**: `async func` + postfix `await` turn any function into a
resumable `Future<T>` coroutine, a single-threaded cooperative executor runs many tasks
round-robin with deterministic timers, and `channel<T>` moves ownership between tasks —
never shares it. The 0.0.2 ownership model carries the whole design: the async world is
borrow-free, so there is still no borrow checker and no shared memory. Like ants, tasks
coordinate by leaving messages for each other, each minding its own work until the message
arrives. Planning and design decisions: [docs/0.0.4/PLAN.md](docs/0.0.4/PLAN.md)
(decisions D1–D6); implementation tickets: [docs/0.0.4/TICKETS.md](docs/0.0.4/TICKETS.md)
(T01–T10).

### Language at a Glance

```fleen
import std.io;
import std.async;
import std.concurrent;

// async fib: recursive awaits over coroutine frames
async func fib(n: int): int {
    if n < 2 { n }
    else { fib(n - 1) await + fib(n - 2) await }
}

// a rendezvous channel shared through split: one buffer, two handles
async func pinger(ch: channel<int>) {
    for i in range(3) {
        ch.send(i * i);
        std.async.yield_now();
    }
}

async func ponger(ch: channel<int>) {
    for i in range(3) {
        n = ch.recv();
        std.io.println("got ", n as string);
    }
}

// timers: ready tasks beat parked ones, wake order is the deadline order
async func sleeper(tag: string, ms: int) {
    std.io.sleep(ms);
    std.io.println(tag, " woke");
}

func main() {
    n = fib(10) await;
    std.io.println("fib(10) = ", n as string);

    ch = channel<int>(0);
    peer = std.concurrent.split(ch);
    std.async.run([pinger(move ch), ponger(move peer)]);

    std.async.run([sleeper("slow", 30), sleeper("fast", 5)]);

    std.io.println("done");
}
```

```console
$ fleen-vm showcase.fln
fib(10) = 55
got 0
got 1
got 4
fast woke
slow woke
done
```

(`e await` is a postfix operator, so `fib(n - 1) await + fib(n - 2) await` needs no
parentheses; `move ch` hands the channel handle into the task — channels are move-only and
`split` is the one way to give a second task a handle to the same buffer.)

### ✨ Language Features

**`async` functions, `await`, and `Future<T>`**

- `async func f(...): T` returns `Future<T>` to the caller while the body still returns `T`:
  calling it packages the arguments into a heap coroutine frame (locals + saved operand
  stack + resume PC, D1) instead of running the body
- **`e await` is a postfix operator** (bound with the tightest suffixes, chain left-to-right):
  it drives the future to completion and yields the `T`; `e await?` composes with `?`
  (`(e await)?` — the 0.0.2 propagation rules, applied after the drive); a bare
  `fib(10) await;` statement drives and discards
- `Future<T>` is **non-Copy and driven once** — it moves like any owned value and can be
  passed to functions or stored; `await` on a non-future is rejected (`AwaitNonFuture`)
- A **top-level await** in `main` boots a blocking drive that runs the scheduler until the
  root completes — a normal program "just works"
- Errors surface at the awaiting site: a failed future re-raises its error when driven

**The cooperative executor**

- A **single-threaded round-robin scheduler** in the VM (one worker is all the 0.0.4
  semantics need — message passing means nothing is ever shared): a task runs until a
  suspension source yields
- `std.async.run(tasks: [Future<unit>])` spawns the roots, runs them to completion, and
  joins; it is re-entrant inside async bodies (a nested, in-place join)
- `std.async.yield_now()` is an explicit cooperative yield; `std.io.sleep(ms)` is an
  awaitable timer — inside an async body the compiler lowers the call to `Call; Suspend`,
  so no explicit await is needed
- **Deterministic by construction**: ready tasks always run before timer-parked ones, timers
  wake in deadline (then insertion) order, and channel waiter queues are FIFO — the ordering
  rules above are the only thing a test may rely on, and they never change

**Channels — message passing, not shared memory**

- `channel<T>(cap)` is a reserved-word constructor: `cap = 0` is a **rendezvous** channel
  (a send parks until a receiver takes the value directly), `cap > 0` a **bounded buffer**
  (a send parks only when full); `c.send(v)` / `c.recv()` suspend on full/empty
- Payloads travel by **ownership move** (D3/D6): owned payloads transfer real ownership to
  the receiver, Copy payloads copy; an owned *variable* handed to `send` needs an explicit
  `move x` / `clone x` (the `OwnedArgRequiresMove` hint names `move` and
  `std.concurrent.split`), while a fresh value (`c.send("hi")`) transfers naturally
- Channels are **non-Copy and move-only**: `clone ch` is rejected (`ChannelNotClonable`);
  `std.concurrent.split(ch)` is the **one explicit fork point** returning a second handle to
  the same buffer (T07d) — two tasks share a buffer only through handles the program
  explicitly forked
- There is no `close` / `select` / `for`-over-channel in 0.0.4 (deferred to 0.1.0 with
  traits); a run where every remaining task is blocked on channels is a `ChannelDeadlock`
  runtime error

**The ownership boundary (no shared memory, no borrow checker)**

- An `async func` cannot take a `ref` parameter (`RefInAsync`), and a borrow handle cannot
  cross any await, task, or channel boundary — the **entire static `Send` rule of 0.0.4 is
  that one line** (D2/D4); `ref` payloads are rejected outright (`ChannelNotBorrowable`)
- An async task may **read** globals (auto deep copy, the 0.0.2 rules) but never **write**
  them (`AsyncGlobalWrite` — route the value through a channel instead, D5)
- Watchword: async works exclusively on **owned + Copy** values. The async world and the
  borrowing world are cleanly separated, and no `Mutex`/`Atomic` containers exist

### ⚠️ Breaking

- **`async`, `await`, and `channel` become reserved words** — 0.0.3 programs using them as
  identifiers must rename them. Nothing else breaks: `Future` stays a plain identifier (only
  the `Future<T>` type form is new), Copy/owned semantics are unchanged, and all 0.0.3
  source programs compile as-is modulo those three renames
- `.flnc` `version` is now written as `4` (async/channel opcodes + the `Value::Future` /
  `Value::Channel` runtime variants). **The format has zero layout changes** — no new table,
  no field change — and readers accept `{1, 2, 3, 4}`: every v1/v2/v3 `.flnc` file still
  decodes, verifies and executes unchanged, locked by committed fixtures. A v4 opcode in a
  v1–v3 module is rejected (`BadOpcode`)

### 🛠 Toolchain

```
tour.fln ─► Lex ─► ASI ─► Parse ─► Resolve(with module table) ─► Typeck ─► Lower ─► Codegen ─► tour.flnc
                                                                   │                             │
                     F8 boundary rules (no ref in async,           │                             │
                     no global writes, channel move) ──────────────┘                             │
                                                                                                 │
                     fleen-verify static checks (v1+v2+v3+v4 rules) ◄────────────────────────────┘
                                                                                                 │
                                                       fleen-vm (coroutine frames + cooperative executor)
```

| Component | Changes in 0.0.4 |
|------|------------|
| `fleen-compiler` | `TokenKind::Async` / `Await` / `Channel` (reserved), `async func` + postfix `await` + `channel<T>(cap)` in Parse, `Future<T>` / `channel<T>` types in Typeck with the F8 boundary rules and D6 payload-move rule, `Suspend` emission at every suspension source in Lower, bytecode v4 emission (`MODULE_VERSION = 4`) |
| `fleen-verify` | The Δdepth table for the eight new instructions (`AsyncStart` as a fresh-call boundary) and the v4 opcode gate (`opcode >= 0xC0` in a v1–v3 module → `BadOpcode`) |
| `fleen-vm` | `Value::Future(Box<FutureFrame>)` / `Value::Channel(Rc<RefCell<ChannelInner>>)` (non-serialized; the `Rc` is the semantics — handles to one buffer), coroutine-frame save/restore/resume, the cooperative scheduler with deterministic timer wake order, channel buffers with FIFO sender/receiver waiter queues, `ChannelDeadlock`, and the T08 defensive backstops (`FutureExpected`, payload-kind uniformity) |
| `flint-cli` | Unchanged interface; async/channel projects compile, run and test (`async` / `await` / `channel` are legal as module/member paths) — see `examples/0.0.4/tour` |

- **Bytecode v4**: `AsyncStart` / `Suspend` / `Poll` / `Await` (0xC0–0xC3) and
  `ChannelNew` / `ChannelSend` / `ChannelRecv` / `ChannelSplit` (0xC4–0xC7) — all inside the
  previously reserved `0xBC–0xFF` space, `0xC8–0xDF` left for expansion
  (specification: [docs/BYTECODE.md](docs/BYTECODE.md) §5.15)
- **Compatibility gate**: committed v1 / v2 / v3 `.flnc` fixtures are decoded, verified and
  executed on every `cargo test`, and every example under `examples/` is
  compiled · verified · run
- **std additions**: `std.async` (`run` / `yield_now`) and `std.concurrent` (`split`) are
  compiler/VM primitives documented in `std/async.fln` / `std/concurrent.fln`; `std.io.sleep`
  joins `std.io`

### 📦 Projects and flint

```
tour/
├── flint.toml              # [package] name = "tour" (no external deps — no Flint.lock)
├── src/
│   ├── main.fln            # crate root: imports modules + std, drives the tour
│   ├── tasks.fln           # async tasks: beats, sleepers, ping/pong, producer/consumer
│   └── pipeline.fln        # an awaited async body + `await?` over Result
└── tests/
    ├── ping.fln            # flint test: a channel rendezvous task
    └── ping.expected       # its expected output
```

```console
$ fleen-vm --project examples/0.0.4/tour    # or: cd tour && flint run
== 1. async fn + await ==
add(20, 22) = 42
== 2. await? over a Result ==
half(10) = 5
caught: cannot halve zero
== 3. round-robin + timers ==
A tick 0
B tick 0
A tick 1
B tick 1
fast woke after 5ms
slow woke after 30ms
== 4. rendezvous channel ==
pong got 0
pong got 1
pong got 2
== 5. buffered channel + move ==
producer: hello buffered
producer: world buffered
consumer got hello
consumer got world
consumer got third
producer: third sent (a slot freed)
== 6. nested run + sync helper ==
quick worker done
orchestrator: batch joined
tour done
```

### 🧪 Quality Assurance

- **953 tests** (up from 845 in 0.0.3): unit tests, per-stage valid/invalid fixture tests,
  end-to-end tests driving the real binaries, `tests/std/` (std is always compiled in CI),
  multi-file project-mode e2e, `flint` fixture tests built from local `file://` git repos
  (no network in CI), the v1/v2/v3 bytecode compatibility fixtures, and the new 0.0.4
  concurrency matrix (scheduling/ordering/boundary regressions, async/channel integration
  fixtures, `flint` async project e2e)
- **Every 0.0.1/0.0.2/0.0.3 end-to-end test passes byte-for-byte** — the reserved words
  above are the only source-level break
- A four-piece quality gate: `cargo fmt --check` · `cargo clippy -D warnings` ·
  `cargo test` · `cargo doc` (configured in GitHub Actions, run on branches and tags)
- The concurrency tests rely **only on the locked ordering rules** (ready beats parked,
  FIFO queues, deadline-order timer wake) — no sleeps-based timing assumptions

### 📦 Installation

Build from source (requires Rust 1.85+, 2024 edition):

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang
cargo build --release
./target/release/fleen-vm --project examples/0.0.4/tour
```

Prebuilt binaries: the release assets include a linux x86_64 archive containing
`fleen-compiler` / `fleen-verify` / `fleen-vm` / `flint-cli` — unpack and run:
`./fleen-vm showcase.fln`.

Single command and project entry points:

```bash
cargo fln file.fln                # one file: compile → verify → execute
cargo fln --project <dir>         # a whole source tree
flint new hello && flint run      # package-manager entry (as flint-cli)
```

Exit code contract unchanged: `0` success / `1` compile·verify·runtime error /
`2` usage·IO error (`exit(n)` passes `n` through).

### 🔜 What's Next

0.0.5: `unsafe` / `trusted` / `ptr` / `addr`; 0.1.0: generics and traits (and with them
language-level composing async — `select` / `join!` fan-out, channel `close` and
`for`-over-channel, user-defined iterables, struct/array `==`), operator overloading, and a
multi-threaded executor. See `docs/DESIGN.md` §18 for the full roadmap, and
[docs/0.0.4/PLAN.md](docs/0.0.4/PLAN.md) / [docs/0.0.4/TICKETS.md](docs/0.0.4/TICKETS.md)
for 0.0.4 planning and implementation decisions (including every recorded deviation).
