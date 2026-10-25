# 0.0.4 — `async` / `await` + 消息传递并发

> **Goal**: give Fleen cooperative async/await and a concurrency model that is
> consistent with its 0.0.2 ownership foundations and its `DESIGN.md` creed
> (express the safest semantics with the most concise syntax).
>
> **Theme**: a `Future` is a suspendable coroutine frame; tasks communicate by
> **ownership-moving values through channels**, never by sharing memory.  Borrow
> handles (`ref`) are confined to the synchronous call depth — the async world
> and the borrowing world are cleanly separated.
>
> **Status**: COMPLETE.  Decisions D1–D6 below are **settled** after review
> and recorded here; `TICKETS.md` carries the per-ticket cards and encoding
> tables.  T01–T10 are implemented — T10 synced the docs
> (DESIGN.md / BYTECODE.md / SYNTAX.ebnf / CHANGELOG / README) and bumped the
> workspace version to 0.0.4 (public release — tag and archives — is separate
> and not yet performed).

---

## 0. Relationship to adjacent versions

| Version | Scope |
|---------|-------|
| 0.0.3 | complete |
| **0.0.4** | **`async` / `await` (coroutine frames) + message-passing concurrency (channels)** |
| 0.0.5 | `unsafe` / `trusted` / `ptr` / `addr` |
| 0.1.0 | generics / traits / operator overloading (where *language-level* composing async lives) |

Self-contained: needs no trait/generics machinery to land before 0.1.0.

---

## 1. Settled decisions (D1–D6)

> These are the three decisions that were open at planning time.  They are final.

### D1 — Future mechanism: **VM coroutine frame**

A `Future<T>` is a **heap coroutine frame**: a complete, suspendable function
frame plus a saved operand-stack snapshot and a resume PC.  It reuses the
existing `fleen-vm` frame machinery (`frame.rs`) — the same way 0.0.2/0.0.3
extended the VM with new instruction groups.

- **Positioning**: the coroutine frame locks the **VM execution layer** (internal
  to bytecode).  *Language-level* composing/generic async (state-machine
  lowering, `dyn Future`-style polymorphism) is **deliberately deferred to 0.1.0**
  together with traits.  Swapping the representation later is a VM/bytecode
  concern, not exposed as a language promise.
- **No trait dependency**: 0.0.4 needs no traits/generics.  This is why it can
  land before 0.1.0.
- Costs accepted: each future snapshot saves the whole operand stack, so memory
  grows with live task count; futures are not composable yet (no `select`).

### D2 — Borrow/`ref` boundary: **B1 (async world is borrow-free)**

Borrow handles (`ref T`) **never cross the async boundary**:

- a `ref` parameter can only live within synchronous call depth;
- an `async fn` **cannot hold, capture, or pass** a `ref` handle across any
  `await`, task, or channel boundary;
- an `async fn` body cannot take a `ref` at all (compile error).

Rationale: 0.0.2's "no borrow checker" argument (DESIGN §3.3) relies on the
borrowed slot being untouched for the duration of a single call — only valid
within a single executing thread.  B1 keeps that invariant by simply never
letting borrows reach concurrent code.  Cost: the async world and the borrowing
world are strictly separate; async functions work exclusively on owned and Copy
values.

### D3 — Concurrency model: **message passing (no shared memory)**

Tasks communicate by **moving values through channels**, never by sharing
memory.  This is Go's "share memory by communicating" in spirit, but stricter:

| | Go channel | Fleen channel (projected) |
|---|---|---|
| payload | references — slices/maps/pointers stay shared, can still race | **ownership move** — owned values transfer real ownership; receiver takes it out |
| truly shared? | only value-types; reference payloads still race | **never**: single ownership + move means one thread owns the data at any time |
| Copy types | copy | copy (int/float/bool/unit/function values) |

Because single ownership already guarantees "moving = ownership transfer"
(0.0.2), a channel only extends that transfer across the thread boundary:
`send(move x)` moves a value from task A's frame into B's (through the channel
buffer).  Since exactly one thread ever holds a value, `Send` static checking
collapses to **one rule** — D4.

### D4 — `Send` rule: **static, one-rule**

> A borrow handle (`ref`) must not cross any async / task / channel boundary (D2).

That's the **entire** static `Send` rule.  Copy values move by copy; owned values
move by ownership; nothing is shared; so nothing else needs checking.  Runtime
checks exist only as a defensive `RuntimeError` backstop (a future value or
channel misinterpreted), never as the primary defense.

Consequence: **globals** — an async task may not write a global mutable (data
race), consistent with D3's no-shared-memory rule; reads follow 0.0.2 D5
(automatic deep copy).  `std.concurrent.shared`-style containers are **not**
introduced.

### D5 — Global writes from concurrent code: **forbidden**

An `async fn` body may **read** globals (auto-copy, as in 0.0.2 D5) but may
**not write** them — writing would be a cross-thread shared mutation, which
D3/D4 exclude.  Compile error with a hint to route the value through a channel.

### D6 — Channel `send` ownership spelling

Follows 0.0.2 D1's "explicit, no magic": an owned value handed to `send` at an
argument position requires an explicit `move x` (or `clone x`); a fresh value
produced on the spot (`c.send(expr)`) transfers naturally, same as any
function argument in 0.0.2.  A bare owned variable (`c.send(x)`) is a compile
error with a `move`/`clone` hint.

---

## 2. Feature list

| # | Feature | Category | Depends on |
|---|---------|----------|-----------|
| **F1** | `async` keyword + `async fn` (externally returns `Future<T>`, body still `T`) | Language + compiler + VM | F3, F7 |
| **F2** | `await` postfix operator (`e await`; composes with `?` as `e await?`) | Language + compiler + VM | F3, 0.0.2 `?` |
| **F3** | `Future<T>` type + `Future` value tag (non-Copy; owns suspend state) | compiler + VM | type system; D1 |
| **F4** | VM coroutine frame (operand-stack snapshot + resume PC) | VM | F3, D1 |
| **F5** | New opcodes in reserved `0xC0+` (no v1–v3 encodings touched) | compiler + VM + verify | F3, F4 |
| **F6** | **Cooperative executor**: single-threaded scheduler + timers + top-level blocking entry | language-semantics + std + VM | F2, F4, F8 |
| **F7** | `async` / `await` become reserved words + diagnostics | compiler | throughout |
| **F8** | `Send` static rule (D4): borrow handles never cross async/task/channel; `async fn` cannot write globals (D5) | compiler (typeck) + VM | D2, D4, D5 |
| **F9** | **Channels**: first-class `channel<T>` + `std.concurrent.channel` | std + VM | D3, D6, F6 |
| **F10** | Concurrency test matrix: scheduling / ordering / boundary regressions | tests | F6–F9 |

std additions fold into F6/F9: `std.async.run`, `std.async.yield_now`,
`std.io.sleep`, and `channel<T>` under `std.concurrent`.

---

## 3. Language design

### 3.1 Keywords

```
async      // marks a func as producing a Future (reserved word)
await      // postfix operator that drives a Future to completion (reserved word)
channel    // first-class channel type constructor (reserved word)
```

### 3.2 Syntax

```fleen
// async function: returns Future<T> externally, body still returns T
async func producer(c: channel<string>) {
    c.send("hi");                    // fresh value transfers naturally (D6)
}

async func consumer(c: channel<string>) {
    s = c.recv();                    // s owns the string; receiver takes ownership
    std.io.println(s);
}

// a synchronous caller drives a future with await
n = fib(10) await;                   // postfix: Future<int> → int
fib(10) await;                       // statement form: drive and discard
// `e await?` composes with ? inside a Result-returning function (§3.3)
```

- `await` is **postfix** (like Rust), so `Future<T>` stays a first-class,
  passable value.
- Precedence: `() [i] .f await ?` all bind tightest, chain left-to-right.

### 3.3 Types

| Construct | Rule |
|-----------|------|
| `async fn f(...): T` | caller sees `Future<T>`; body returns `T` |
| `e await` with `e: Future<T>` | `T` (drives to completion) |
| `Future<T>` | non-Copy; owns suspended state; driven once |
| `channel<T>` | first-class; non-Copy; owns its buffer |
| Copy types | `int`/`float`/`bool`/`unit`/function values remain Copy inside async fn |
| `?` on `Future<Result<T,E>>` | `e await?` ≡ match on `e await` (0.0.2 rules) |

### 3.4 The async world is borrow-free (D2)

- `async fn` may not declare a `ref T` parameter, may not capture a `ref`, and
  may not pass a `ref` across any await/task/channel boundary — all compile
  errors (`RefInAsync`).
- Watchword: async works only on owned + Copy values.

---

## 4. Execution model

### 4.1 Coroutine frame (D1)

A `Future<T>` = heap object holding:

```rust
struct FutureFrame {
    func: FuncId,
    operand_stack: Vec<Value>,   // saved at the suspend point
    resume_pc: u32,              // instruction pointer to continue at
    // + the enclosing frame's locals live across await
}

struct ChannelBuffer {  // not yet fixed: shape of the buffer, capacity handling
    capacity: usize,
    items: VecDeque<Value>,      // owned payloads hold ownership while buffered
}
```

`Suspend` (0xC1) snapshots the current frame's operand stack + locals live
across the await, yields `Pending`, and returns control to the executor.
`Poll` (0xC2) / `Await` (0xC3) resumes it on the task's thread, continuing from
`resume_pc`.

### 4.2 Executor (F6)

- **Single-threaded cooperative scheduler**: one worker thread runs tasks in
  round-robin order.  A task yields only when a suspension source (sleep, yield,
  channel send/recv) executes inside an async body.  The multi-thread worker
  pool is a later optimization — not a semantic gap (D3's no-shared-memory
  means one worker is all 0.0.4 features need).
- **Top-level entry**: the first bare `e await` (or an explicit `main` awaiting)
  boots a **blocking** drive that runs the scheduler until the root future
  completes — a normal `main` program "just works", preserving 0.0.1
  single-command simplicity.
- `std.async.run(tasks: [Future<unit>])`: spawn several futures, run to
  completion, join.  Also usable inside an async body (re-entrant join: the
  outer task suspends and waits for the inner tasks to complete).
- Scheduling is cooperative: a future yields when an operation is `Pending`
  (channel empty/full, sleep, `yield_now`).  Ready tasks always win over
  timer-parked tasks.

### 4.3 Error propagation

A panicked/poisoned future surfaces as a `RuntimeError` like any other.  `await`
on a future that terminated abnormally re-raises it at the awaiting site.

---

## 5. Channels (F9, D3/D6)

```fleen
ch = channel<int>(16);                       // bounded buffer of 16
ch = channel<string>(4);                     // owned payload: ownership moves
peer = std.concurrent.split(ch);             // the one explicit fork point (T07d):
                                             // a second handle, same buffer

async func producer(c: channel<string>) {
    c.send("hi");                       // fresh value transfers naturally
}
async func consumer(c: channel<string>) {
    s = c.recv();                       // transfers ownership to s
    std.io.println(s);
}

// T07d: a channel has exactly one owner — move it into each task, never
// clone it (`clone ch` is a typeck error; `split` is the only sharing).
std.async.run([producer(move ch), consumer(move peer)]);
```

| Operation | Kind | Blocks when |
|-----------|------|-------------|
| `channel<T>(capacity)` | constructor (reserved word) | — |
| `c.send(v)` | awaitable | buffer full |
| `c.recv()` | awaitable | buffer empty |
| `std.concurrent.split(ch)` | plain call | — (returns a second handle) |

- Copy values move by copy; owned values move **ownership** (`move x` explicit,
  fresh values natural — D6).
- **No `select`**, **no close/range-over-channel** in 0.0.4 (both deferred to
  0.1.0, where traits make composition tractable).
- No shared-memory containers (`Mutex`/`Atomic`) in 0.0.4 — channels are the
  only cross-task communication (D3).

---

## 6. Bytecode impact

| New opcode | Mnemonic | Purpose |
|------------|----------|---------|
| 0xC0 | `AsyncStart` | mark a frame as async-suspendable; create coroutine handle |
| 0xC1 | `Suspend` | yield `Pending`, save operand stack + resume PC |
| 0xC2 | `Poll` | resume a future; push `Ready`/`Pending` tag |
| 0xC3 | `Await` | `Poll` + expect `Ready`, unwrap payload |
| 0xC4 | `ChannelNew` | allocate a channel of given capacity |
| 0xC5 | `ChannelSend` | push a value into the buffer (may suspend) |
| 0xC6 | `ChannelRecv` | pop a value out of the buffer (may suspend) |
| 0xC7 | `ChannelSplit` | produce a second handle to the same buffer (T07d) |
| 0xC8–0xDF | — | reserved for 0.0.4 expansion |

All slots are inside the **reserved `0xBC–0xFF` region** (BYTECODE.md §4.2), and
**none of the v1/v2/v3 encodings are touched** (BYTECODE.md §10 compatibility
promise holds).

Runtime `Value` additions: `Future(FutureFrame)`, `Channel(ChannelBuffer)`.  These are VM-internal
Rust enum variants — like `Value::Ref` (0.0.2 PLAN §5.3), they are **not serialized** into `.flnc`
(no numeric tag).  The disk-format tags (`const-pool 0/1/2`, `FieldTypeTag 0–11`, BYTECODE.md §2/§9)
are untouched.

---

## 7. Compiler-stage changes

| Stage | Changes |
|-------|---------|
| **Lex** | `TokenKind::Async` / `Await` / `Channel`; sync `is_keyword` / `keyword_str` / `Display`; reserved-word diagnostics |
| **Parse** | `async` modifier on `func` decl; postfix `await` chain (`ExprAwait`); `channel<T>` type syntax; keep `e await?` as `?.` composition |
| **Resolve** | scope/visibility of async fn + channel types; `Future<T>` wired through let-bound signatures |
| **Typeck** | `Future<T>` rule (`e: Future<T> ⇒ e await: T`); **F8**: reject `ref` in async (D2), forbid global writes in async (D5), enforce channel payload move (D6); `await` on non-Future → `AwaitNonFuture` |
| **Lower** | async fn → `AsyncStart`; body lowering with `Suspend` at each suspend point; `await` → `Poll`/`Await`; channel constructor/op ops |
| **Codegen** | emit new opcodes into the `0xC0+` region; `Future`/`Channel` value tags; version bump to **4** |
| **Verify** | `Δdepth` for the 7 new instructions; version gate accepts `{1,2,3,4}`; v<4 module with a v4 opcode → `BadOpcode` |
| **VM** | coroutine frame save/restore; thread-pool scheduler; channel buffer (bounded, suspend-aware); RuntimeError re-raise on future poll |

New error kinds: `RefInAsync` (D2), `AsyncGlobalWrite` (D5),
`OwnedChannelArgRequiresMove` (D6), `AwaitNonFuture`.

---

## 8. New std modules

| Module | Item | Note |
|--------|------|------|
| `std.concurrent` | `channel<T>(cap)` | first-class channel constructor |
| `std.async` | `run(tasks: [Future<unit>])` | multi-task run + join |
| `std.async` | `yield_now()` | explicit cooperative yield |
| `std.io` | `sleep(n: int)` | awaitable timer |

---

## 9. Explicitly out of scope (0.0.4)

| Not doing | Reason |
|-----------|--------|
| `async {}` blocks | need closures/trait machinery (0.1.0) |
| Language-level composing/generic async, `dyn Future` | 0.1.0 with traits; coroutine frame stays a VM-layer mechanism (D1) |
| `select!` / `join!` fan-out | needs control-flow multi-suspend; deferred to 0.1.0 |
| Channel close / `for` over a channel | deferred |
| Shared-memory containers (`Mutex`/`Atomic`), globals written from async | against D3/D5 (no-shared-memory) |
| Multi-thread executor exposing `Send` trains | 0.1.0 + generics |
| `unsafe async` | 0.0.5 with `unsafe` |

---

## 10. Test matrix (planned)

```text
tests/
├── lexer/        async/await/channel tokens          async_{valid,invalid}
├── parser/       async fn + await chain + channel<T>  async_{valid,invalid}
├── typeck/       Future<T> rules, F8 boundaries        async_{valid,invalid}
├── vm/           coroutine save/restore, scheduler     async_{valid,invalid}
└── e2e/
    ├── e2e/valid/     fib_async.fln+expected, channel_pingpong.fln+expected,
    │                  channel_buffer.fln+expected, multi_task.fln+expected
    └── e2e/invalid/   await_non_future.fln+exit+stderr, ref_in_async.fln+...,
                       async_global_write.fln+..., owned_channel_arg.fln+...
```

Compatibility fixtures: keep one fixed-byte v1 and one v2/v3 `.flnc` asserting
the verify/VM still execute them (BYTECODE.md §10 promise).

---

## 11. Versioning

- `.flnc` `version` → **4** (new opcodes + `Future`/`Channel` value tags).
- v1/v2/v3 modules stay executable on the 0.0.4 verify/VM.
- `async` / `await` / `channel` become reserved words: 0.0.3 programs using them
  as identifiers need renaming (recorded as breaking in the CHANGELOG).