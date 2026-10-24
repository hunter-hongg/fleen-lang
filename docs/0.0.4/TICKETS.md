# 0.0.4 — `async` / `await` + channels (Tickets & Decisions)

> Companion to `docs/0.0.4/PLAN.md`.  One card = one unit of work, closed with
> tests.  Decisions D1–D6 referenced here are **settled** (see PLAN §1).
> **Status: IN PROGRESS — T01–T07 implemented (T07 channels); T08–T10 planned.**

---

## T01 — Keywords + lexer (F1/F7)

- [x] Reserve `async`, `await`, `channel` in the lexer keyword table
- [x] `TokenKind::{Async,Await,Channel}` wired through `is_keyword` / `keyword_str` / `Display`
- [x] Reserved-word enforcement: reserving the words makes misuse surface at the
      existing statement/expr entry guards (`expected … found async`)
      — same path as `as` / `ref`; no new `ReservedWord` kind needed (0.0.4)
- [x] Tests: lexer unit tests (keyword tokens, metadata, prefix-safety) +
      `tests/lexer/valid/async_keywords.fln`; a *valid* file only — reserving a
      word never fails tokenization, so there is no `invalid/async_*` lexer case

**D1**: postfix `await` (`e await`) — matches Rust; keeps `Future<T>` passable.
**Note**: because these are non-expression-starting words, the parser rejects
them via the generic statement guard (as `as`/`ref` do), *not* via
`reject_keyword_binding_name` (which only fires for prefix keywords that start
an expression, e.g. `move`/`box`). Any prettier `keyword`-named diagnostic for
new async positions belongs to T02 (parser).

---

## T02 — `async fn` AST + `Future<T>` type (F1/F3)

- [x] AST: `Func` + `is_async`, `ExprAwait`; `Future<T>` in the type system
- [x] Type rule: `e: Future<T> ⇒ e await : T`; `await` on non-Future → `AwaitNonFuture`
- [x] `not_copy(Future<T>)`; Copy classes unchanged inside async fn
- [x] Tests: `tests/parser` + `tests/typeck` async suites

**Notes (implementation)**: `Future` is **not** a reserved word (0.0.4 reserves
only `async`/`await`/`channel`); it is recognized only in constructor position
`Future<T>` in `parse_type` — a bare `Future` stays a `Named` struct type.
`async` (prefixing `func`) is added to `can_start_stmt`; `await` remains
non-statement-starting and is a postfix suffix in the `() [i] .f await ?` group
(so `e await?` = `(e await)?`). An async fn's *caller* signature is wrapped in
`Future<T>` in `collect_func_signatures`; the fn's own `ret_type`/body stay `T`.
Async **lowering** is deliberately not wired yet — `lower_expr_into` rejects
`TypedExprHir::Await` with `UnsupportedFeature` until T05.

---

## T03 — Borrow/global boundaries (F8; D2/D5)

- [x] typeck: reject `ref T` anywhere in an async fn / captured across a boundary → `RefInAsync`
- [x] typeck: reject **global writes** inside async fn → `AsyncGlobalWrite`
- [x] Tests: `tests/typeck/invalid/ref_in_async.fln`, `async_global_write.fln`

**Notes (implementation)**: the ref rule is lexical — any `ref` declared or
used while an enclosing function is `async` reports `RefInAsync`. Since the
resolver allows `ref` only as an outer function parameter, the reachable
shapes are:

- an async fn's own parameter (reported at declaration);
- a nested async fn capturing an enclosing sync fn's parameter (reported at
  the use, once per binding);
- a *sync* fn declared inside an async fn whose parameter is `ref` (reported
  at declaration — the lexical rule deliberately covers nested sync
  declarations too).

`clone s` of a captured ref is rejected on its own typeck path. An operation
with a stronger pre-existing rule keeps its own error: `move s` reports
`MoveOfBorrowed` rather than `RefInAsync`.

An async fn calling a *synchronous* ref-taking helper stays legal: the borrow
lives only for the call and cannot escape (refs are not storable).

Global writes are checked at the three HIR forms that can reach one:
`AssignField` / `AssignIndex` match on the target binding id (a local shadow
has a different id), and a module-qualified `Assign` (`m.g = v`) matches on
its mangled dotted name. A bare `g = v` in a function body is a local binding
(or loop assignment), not a global write, and stays legal. `deref g = v`
keeps the older, stronger `DerefAssignOfGlobalBox` instead of double-reporting.
Global reads are unchanged (deep copies via the ownership pass). Known gap,
out of scope: an async fn calling a sync helper that writes its own module's
global is not caught without a call graph (T06/T08). Because functions are
first-class values, call-graph analysis is necessarily incomplete; T08 must
either accept and document that limit or tighten D5.

---

## T04 — VM coroutine frame (F4; D1)

- [x] `FutureFrame { func, resume_pc, locals, operands, state }`
- [x] `Suspend` snapshot / resume from `resume_pc`
- [x] `Future` runtime `Value` variant (no disk tag — see notes)
- [x] Tests: coroutine save/restore (`fleen-vm/src/tests.rs`)

**Notes (implementation)**: the frame is **location-independent** — `resume_pc`
is an offset into the function's own `code`, `locals` / `operands` are owned
values, and unlike `CallFrame` a snapshot stores **no `stack_base`**. That is
what lets `Vm::resume_frame` restore the frame at a different operand-stack
depth (and, from T06, on a different worker thread). `suspend_top_frame` *moves*
the frame region out of the shared stack (never copies), so Rust `Drop` is the
release path if the future is dropped instead of resumed; the frame must be the
innermost one, since `Suspend` is an instruction of the async function itself.

Deviations from the card as written, all deliberate:

- **No `0x0D` value tag.** Runtime `Value` variants are not serialized and carry
  no numeric tag (PLAN §6, BYTECODE.md §3.2; see LESSON L3 — this exact draft
  number was already caught once as an invention). T04 adds
  `Value::Future(Box<FutureFrame>)` only: const-pool tags, `FieldTypeTag`, the
  `.flnc` tables and the module version stay untouched. The `Box` is load-bearing:
  inlining two `Vec`s plus the state would inflate every operand slot, array
  element and struct field (`size_of::<Value>()` is pinned by a test).
- **Tests are unit-level, not `tests/vm/async_valid` fixtures.** There is no
  opcode that can reach a suspend point until T05, and no `.fln` can drive the
  mechanics; the fixture directory lands with T05/T06.
- `live_locals` became the full local area (`Func::locals` slots). Liveness
  metadata does not exist anywhere in the pipeline, and D1 already accepts the
  whole-frame snapshot; a liveness pass would be a separate optimization.

The struct also carries a `FutureState` the original card omitted:
`Resumable` (covers the not-yet-started frame `FutureFrame::new` produces —
`resume_pc` 0, arguments padded to `Func::locals` — and a suspended one),
`Completed(Value)` and `Failed(RuntimeError)`. A `Future` is driven once
(PLAN §3.3), so resuming a completed/failed frame is the defensive
`RuntimeErrorKind::FutureNotResumable`; `Failed` stores the error captured while
the coroutine frame was live, so the T06 re-raise at the awaiting site keeps its
location. `resume_pc` is `usize`, mirroring `CallFrame::ip`, rather than the
`u32` in PLAN §4.1's sketch.

Still open for T05/T08: `clone` of a `Future` is currently reachable (typeck
treats it as an ordinary owned type, and `CloneGlobal` deep-copies on every
global read), which would snapshot a suspended coroutine. `Value` derives
`Clone`/`PartialEq` regardless, so the language rule and the defensive backstop
are T08's to settle; D2 already guarantees no `ref` handle can appear in a
snapshot, checked by a `debug_assert` here.

---

## T05 — Opcodes + lower/codegen (F5)

- [x] `AsyncStart/Suspend/Poll/Await` (0xC0–0xC3)
- [x] Lower async fn → `AsyncStart`; `await` → `Await`; version → **4**
- [x] Verify: `Δdepth` for the 4 new instructions; version gate `{1,2,3,4}`
- [x] Tests: `tests/vm/async_valid`-style VM tests + verify v4-gate/depth tests

**Notes (implementation)**: AsyncStart/Suspend/Poll/Await are all operand-less
(`operand_len = 0`). The async fn's `code` is `[AsyncStart, <body>]`; a normal
`Call` runs `AsyncStart` at pc 0, which packages the frame's parameter slots
into a **not-yet-started** `FutureFrame` (T04 `D1`) with `resume_pc` set to the
body entry (pc + 1) and delivers the `Future` to the caller. The body itself is
only driven by `Await`/`Poll` via `resume_frame`.

- `Await`/`Poll` pop a `Future`, then advance the awaiting frame's `ip` past the
  instruction *before* calling `resume_frame` — mirroring `Call`'s ip-advance
  (the U09 ip-rewind lesson). The resumed body's own `Return` delivers the
  payload `T`, so in T05's synchronous subset (no suspension source) both
  opcodes land `T` with no Ready/Pending tag or re-drive loop; the T06 executor
  rewires `Poll` to honor a real `Pending`.
- `Suspend` is defined, verify-tabled and dispatched, but no 0.0.4 T05 lowering
  emits it (there is no channel/sleep/`yield_now` source yet), so a verified
  program never reaches it — the VM arm is the defensive
  `RuntimeErrorKind::UnsupportedAsyncSuspend`. The real yield lands with T06/T07.
- Opcode stack effects (verify): `AsyncStart`/`Suspend` Δ0/min0 (AsyncStart is
  a call boundary — net 0 in the body's linear analysis), `Poll`/`Await`
  Δ0/min1 (`F → T`).
- Version bump to **4**, applied where the version constant lives:
  `codegen::MODULE_VERSION`, `flnc::READ_VERSIONS`, the VM version gate, and the
  v3→v4 fixture/version-test assertions all updated. The committed
  `tests/fixtures/v3/aggregate.flnc` stays **byte-v3** (BYTECODE.md §10 compat
  promise); `v3_fixture_bytes()` pins `module.version = 3` so the v3 drift test
  stays meaningful now that the compiler emits v4.
- Tests: `fleen-vm/src/tests.rs` (source end-to-end async/await + raw-opcode
  dispatch + defensive `FutureExpected`/`UnsupportedAsyncSuspend`),
  `fleen-verify/src/tests.rs` (v4 async verifies, v4-in-v3 gate, Await underflow,
  opcode numbering). `Poll`'s VM dispatch and both `Poll`/`Suspend` verify
  stack-effect branches are covered by hand-written bytecode tests
  (`raw_poll_*`, `v4_*_stack_effect_verifies`); a lower test asserts an async
  fn's entry block opens with `AsyncStart` (body entry = fixed pc + 1).

---

## T06 — Cooperative executor (F6)

Implemented as a **single-threaded cooperative scheduler first** (the accepted
planning recommendation, and semantically complete for 0.0.4: D3's no-shared-
memory means one worker is all the features need). A multi-thread **worker-thread
pool** is a follow-up optimization, not a semantic gap — the scheduler and
`FutureFrame` chain already encode the boundary (a task's whole suspended frame
chain detaches when it yields), so pooling can layer on later.

- [x] Cooperative `Pending`: `Suspend` (0xC1) is now a real yield — it detaches
      the running task's *whole* `is_future` frame chain into the scheduler
      (`suspend_chain`/`resume_chain`), not T05's defensive `UnsupportedAsyncSuspend`
- [x] Scheduler queue + timers (`fleen-vm/src/executor.rs`); ready tasks always
      win (a task that hasn't reached its own sleep is never starved by an earlier
      timeout)
- [x] Top-level bare `await` boots a blocking drive until the root completes
      (`run_inner` drives the scheduler to empty at a sync boundary)
- [x] `std.io.sleep(n)` awaitable timer; `std.async.yield_now()`; `std.async.run([Future<unit>])`
      (the compiler lowers `sleep`/`yield_now` in an async body to `Call; Suspend`)
- [x] Panic re-raise as `RuntimeError` at the awaiting/join site
      (errors propagate directly from `step()`; `FutureState` is `Resumable` only)
- [x] Tests: `async` VM unit tests + e2e fixtures `async_multi_task` / `async_sleep_order`
      (deterministic round-robin + timer ordering regressions)
- [x] _Enabler_: `import std.async` needed the reserved word `async` to parse as a
      module/member name — added `parse_namespace_segment` (parser) so `async`/
      `await`/`channel` are legal only as dotted-path/member names, never bindings.
- [ ] **Deferred**: multi-thread worker pool (threads) — no 0.0.4 feature needs it.

**Notes (implementation)**: each `Vm` step that yields parks exactly one task in
`self.pending`; `run_inner` re-enqueues it (with any `sleep` deadline) and, at a
synchronous boundary, calls `drive_join` to drain the scheduler before stepping
the sync caller again. A task's suspended state is a `Vec<FutureFrame>` chain
(outermost-first) moved *out* of the shared operand stack — never copied — so a
`Suspend` under a nested `await` parks every future-marked frame above the sync
boundary. Out of scope here, as in the PLAN: language-level `select`/`join`
composition (0.1.0).

**Review fixes (post-T07 review)**:

- **Nested `std.async.run` from an *awaited* async body** used to fail with a
  `StackShape`: the builtin signaled `need_join` unconditionally, one
  instruction *before* the compiler-emitted `Suspend` parks the caller, so
  `drive_join` ran the spawned tasks while the caller was still a live frame —
  the tasks completed "into" the future-marked caller (never counted by the
  `JoinWait`), the caller's own `Suspend` then created a `JoinWait` that never
  fills, and the awaiting frame below lost its value. The builtin now signals
  the join only for *synchronous* callers; an async caller's `Suspend` parks it
  and `run_inner`'s pending path builds the `JoinWait` (regressions:
  `await_of_async_body_that_calls_run_completes`,
  `await_of_async_body_that_recvs_after_run_completes`, e2e `await_nested_run`).
- **The T07 waiter registry is pruned**: `drive_join` drops `channel_waiters`
  roots whose channel has no parked task left (top of each iteration and once
  on exit), so channels no longer pin one `Rc` root per channel ever parked on
  for the program's lifetime (regression:
  `channel_waiters_registry_is_pruned_after_tasks_unpark`).

---

## T07 — Channels (F9; D3/D6)

- [x] `channel<T>` type + `ChannelNew/ChannelSend/ChannelRecv/ChannelSplit`
      (0xC4–0xC7); runtime variant `Channel`
- [x] Bounded buffer; `send` suspends on full, `recv` suspends on empty
- [x] Payload move: Copy by copy, owned by ownership; explicit `move x` required (D6)
- [x] Constructor `channel<T>(cap)` (the settled T07e reserved-word syntax; the
      card's `std.concurrent.channel<T>(cap)` spelling is not implementable —
      see the notes)
- [x] Tests: VM unit tests (`fleen-vm/src/tests.rs`, `fleen-verify/src/tests.rs`)
      + e2e valid `channel_basic` / `channel_rendezvous` / `channel_senders_fifo`
      (two parked senders wake FIFO) / `channel_string_move` (owned payload via
      `move`); e2e invalid `channel_member_not_called` / `channel_member_not_found` /
      `channel_not_clonable` / `channel_not_storable` / `channel_send_requires_move` /
      `channel_payload_mismatch` / `split_non_channel` / `channel_eq_rejected`
      (typeck) and `channel_deadlock` / `channel_negative_cap` (runtime)

**Decisions (settled before implementation — three forks the card left open):**

- **T07a — `send`/`recv` syntax: recognized in typeck, not in the grammar.**
  `c.send(v)` / `c.recv()` stay parsed as the existing
  `Call(Field(recv, "send"), args)` shape (the parser and `SYNTAX.ebnf` gain
  nothing); when the receiver's type is `channel<T>` and the member name is
  `send`/`recv`, **typeck** rewrites the node to `TypedExprHir::ChannelSend` /
  `ChannelRecv`. Rationale: no method-call mechanism exists anywhere in the
  pipeline (the dot has exactly one meaning today — a struct field read), and
  0.0.4 would be adding a whole grammar production for two built-in members.
  Recognition sits in the semantic phase, which is where "is this a valid
  member of this type" is already decided; no phase is skipped and the
  receiver is still fully typed first. A `c.send` *not* applied to arguments,
  or a `send`/`recv` on a non-channel receiver, is a typeck error
  (`ChannelMemberNotFound` / `ChannelMemberNotCalled`).
- **T07b — deadlock is a runtime error, detected immediately.** 0.0.4 has no
  `close` and no `for`-over-channel, so on a single cooperative worker "every
  task blocked on channels" is a hard deadlock, not a pause. The scheduler
  raises `RuntimeErrorKind::ChannelDeadlock` at the moment the ready queue is
  empty, no timer is due, and at least one task is blocked — never by parking
  forever. Same path covers a *synchronous* `main` that receives from an empty
  channel (nothing else can ever fill it). Chosen over silent hang so the
  failure is a located `RuntimeError` (the `drive_join` `?` chain preserves the
  awaiting site) instead of a silent hang with no exit code.
- **T07c — the buffer is shared by `Rc<RefCell<Inner>>`, with a written reason.**
  A channel handle is by definition live in more than one task at a time (via
  the T07d split), so the payload buffer cannot live inline in the `Value`:
  `Value: Clone` would give every holder a *fresh* buffer and a parked task
  would wake to find an empty one — a silent hang. This is a deliberate,
  documented departure from SPEC §11's blanket `Rc`/`RefCell` prohibition (same
  reasoning as T04 boxing `Value::Future`): the aliasing *is* the language
  semantics here, not an implementation shortcut, and it stays inside the VM
  crate. `size_of::<Value>()` is unchanged (one pointer).
- **T07d — `channel<T>` stays non-Copy and move-only; `std.concurrent.split`
  is the one explicit fork point; channels are non-storable.**  PLAN §3.3
  ("non-Copy; owns its buffer") and §5 (`run([producer(ch), consumer(ch)])`,
  one move-only value used twice) contradict each other: the rule wins and the
  example changes.  A channel has exactly one owner — `clone ch` is rejected
  (`ChannelNotClonable`: a deep copy of a buffer is meaningless, and a shallow
  one would silently alias while typeck believed it copied), and 0.0.2's
  read-only-auto-clone path is rejected for channel operands at the same place.
  Two tasks share a buffer only through `std.concurrent.split(ch)`, which
  returns a *second independent handle* (same `Inner`, fresh owner) — explicit
  sharing, no magic, and the only place duplication happens.  §5's example
  becomes `peer = split(ch); run([producer(move ch), consumer(move peer)])`.
  Channels are also **non-storable**: `Type::Channel` is rejected as a
  struct-field or array-element type (`ChannelNotStorable`), because 0.0.3
  field/index reads deep-copy owned values, which would fork buffers silently.
- **T07e — `channel<T>(cap)` is constructor syntax; rendezvous (cap = 0) is
  direct handoff.**  `channel` is a reserved word, `int` is a keyword, and a
  type is not an expression, so no expression grammar route can express a
  value expression `channel < int > (16)`; `channel<` at expression position
  instead parses a type and then the capacity argument (a `>` that does not
  follow a parseable type stays the ordinary comparison).  `cap = 0` blocks a
  `send` until a *parked* receiver takes the value directly: the blocked sender
  carries its pending payload in its own suspended frame and the handoff stages
  it through a one-slot window, so the value never sits in a buffer it does not
  have.  A negative capacity is `InvalidChannelCapacity`.

**Consequences the card must carry:**

- `Type::Channel` does not exist yet in either `ast::Type` or
  `typed_hir::Type`, and `OwnedChannelArgRequiresMove` (PLAN §7) was never
  implemented: 0.0.2's ownership pass only validates `move`/`clone` *nodes*,
  so there is no existing "this argument must be a move" check. D6 adds a new
  direction of check, at the call site, before `prepare_args`.
- `ChannelSend`/`ChannelRecv` are **not idempotent**, unlike `std.io.sleep`.
  The T06 `Call intrinsic; Suspend` pattern re-executes the call on resume,
  which is harmless for a timer and a duplicate delivery for a send. The
  suspend therefore completes the op *before* parking — the frame's ip is
  already past the instruction and the channel operands are popped, so a
  parked task resumes at the instruction *after* the send/recv (see Notes),
  never re-executing it.
- A channel whose type is not statically uniform is rejected by typeck
  (`TypeMismatch` on the send payload); the payload type recorded on the MIR
  instruction backs T08's defensive run-time check (its codegen use is
  deferred — see the T08 card; verify's stack analysis is type-blind).
- No `close`, so a channel is never drained to completion by construction: the
  run's exit condition is "no task is runnable", not "the channels are empty".

**Notes (implementation)**:

- **Constructor syntax, and why there is no `std.concurrent.channel` builtin.**
  The constructor is the reserved-word expression `channel<T>(cap)` (T07e, the
  parser's `channel<` route). The card's qualified spelling is not implementable:
  a builtin `channel(cap)` call has no way to learn its payload type `T` (the
  schematic table's `Var` would leak into TypedHIR, which 0.0.3 explicitly
  forbids), so the entry was removed from `typeck/builtins.rs` and only
  `std.concurrent.split` remains. `std/concurrent.fln` is therefore doc-only
  (same shape as `std/async.fln` from T06 — `channel` is a reserved word and
  cannot be declared as a Fleen function).
- **The suspended op resumes *after* itself, not at itself.** Unlike T06's
  `Call intrinsic; Suspend` (which re-executes the call on resume — harmless
  for a timer), `ChannelSend`/`ChannelRecv` suspend with the frame's `ip`
  already past the instruction and all channel operands popped. A parked
  receiver resumes with its payload injected into the innermost frame's
  operands (`inject_value_into_chain`); a parked sender's payload travels in
  the channel's sender queue, so a woken sender continues as if the send had
  just completed. This kills the double-delivery risk for non-idempotent ops
  by construction.
- **`ChannelSend` produces `unit`.** The send expression's `Unit` is left on
  the stack (verify Δ−1, min 2) so the statement wrapper's `Pop` stays
  balanced — on the park path the `Unit` is pushed *before* the chain
  detaches, so the resumed frame pops it exactly once.
- **The buffer is FIFO** (`VecDeque`, `push_back`/`pop_front`) — a channel is
  a queue, not a stack (the first draft used `Vec::pop`, LIFO). The *sender and
  receiver queues* are FIFO too (`VecDeque`; the first draft's `Vec` + `.pop()`
  woke the most recently parked sender first — caught by review, fixed with the
  `two_parked_senders_wake_in_fifo_arrival_order` unit test and the e2e
  `channel_senders_fifo` fixture).
- **Channels have no `==`/`!=`.** A handle's derived `PartialEq` would compare
  *buffer contents*, so two distinct channels holding equal values would
  compare equal — silently aliasing what the split-only fork discipline keeps
  distinct. Typeck rejects the operands (`InvalidOperand`, same rule as
  struct/array); the VM's `ArithTypeMismatch` stays the defensive backstop.
- **A parked task is registered, not just parked.** A task that parks on a
  channel is invisible to the scheduler (its chain moved into
  `ChannelInner`), so: (a) `drive_join` keeps going while any channel has
  waiters and flags the park with `suspended_on_channel` (a nested join must
  not count a channel park as a task completion); (b) the VM keeps one
  handle per waiting channel (`channel_waiters`) — a parked task's own frame
  no longer holds the handle, so without this root the `Rc<RefCell<Inner>>`
  could drop and silently destroy the parked task. Deadlock (T07b) is
  detected when the scheduler is empty (no ready, no timers) and a channel
  still has waiters; a *synchronous* blocking `send`/`recv` (no future frame
  live) raises `ChannelDeadlock` directly at the instruction, which also
  covers sync `main` receiving from an empty channel.
- **Non-storable and non-clonable are enforced in typeck.** `ChannelNotStorable`
  fires on struct-field declarations and array literals (both element-type
  routes); `ChannelNotClonable` on `clone ch`; `OwnedChannelArgRequiresMove` on
  a bare owned local in `ch.send(x)` (same predicate as
  `reject_bare_owned_arg` — Copy payloads and owned *globals* pass);
  `ChannelMemberNotFound` on `send`/`recv` against any non-channel,
  non-struct receiver; `ChannelMemberNotCalled` on a bare `c.send` field read.
  A struct *with* a field literally named `send` still resolves through the
  ordinary field path.
- **Deferred to T08**: the defensive runtime payload-type backstop (the
  recorded `payload_ty` in `MirInstrKind::ChannelNew`/`ChannelSend` is
  currently unused by codegen — typeck guarantees uniformity statically).
  Also accepted for 0.0.4: a bare read of a channel-*global* lowers to a
  plain handle load (globals read as deep copies per D5, but a channel's
  "deep copy" is a handle copy) — an alias without explicit `split`;
  `ChannelNotClonable` covers the explicit `clone` form only.

---

## T08 — Send static rule integration (F8; D4)

- [ ] Wire D4 as the single cross-thread rule: borrow handles never cross
      async/task/channel (with T03); everything else unsafely-free (no sharing)
- [ ] Decide the D5 enforcement boundary for sync helpers (call-graph analysis
      is incomplete with first-class function values)
- [ ] Defensive runtime `RuntimeError` backstop on misused `Future`/`Channel`
- [ ] Tests: typeck boundary suite + defensive VM paths

---

## T09 — Integration fixtures & std e2e

- [ ] `tests/e2e/valid/fib_async`, `channel_pingpong`, `channel_buffer`, `multi_task` (.fln + .expected)
- [ ] `tests/e2e/invalid/await_non_future`, `ref_in_async`, `async_global_write`, `owned_channel_arg` (.fln + .exit + .stderr)
- [ ] Full §10 matrix through `cargo fln` and `flint`
- [ ] Fixed-byte v1 + v2/v3 `.flnc` fixtures still execute (BYTECODE.md §10)

---

## T10 — Docs & version sync

- [ ] DESIGN.md async/await + channels sections; §17 matrix; §18 roadmap 0.0.4 ✅
- [ ] BYTECODE.md async opcodes + `Future`/`Channel` value tags + version 4 tables
- [ ] SYNTAX.ebnf bump; CHANGELOG 0.0.4 + reserved-word breaking note
- [ ] version `0.0.3` → `0.0.4`

---

## Opcode/value map (locked)

| Region | Owner |
|--------|-------|
| 0x00–0xBB | v1/v2/v3 — untouched (compat promise holds) |
| 0xBC–0xFF | reserved (BYTECODE.md §4.2); 0.0.4 uses 0xC0–0xC7, 0xC8–0xDF reserved for expansion |

| tag | new runtime `Value` variant (not serialized) |
|-----|-----------|
| — | `Future(FutureFrame)` |
| — | `Channel(ChannelBuffer)` |
| (const-pool and FieldTypeTag numbers: untouched) | |

| new opcode | mnemonic | Δdepth |
|------------|----------|--------|
| 0xC0 | `AsyncStart` | (0, 0) — call boundary, net 0 in the body's linear analysis |
| 0xC1 | `Suspend` | (0, 0) — yield boundary (unreachable from T05 lowering) |
| 0xC2 | `Poll` | (0, 1) — `F → T` (T05: drives to completion) |
| 0xC3 | `Await` | (0, 1) — `F → T` |
| 0xC4 | `ChannelNew` | (0, 1) — `cap → ch` |
| 0xC5 | `ChannelSend` | (-1, 2) — `ch v → unit` |
| 0xC6 | `ChannelRecv` | (0, 1) — `ch → v` |
| 0xC7 | `ChannelSplit` | (0, 1) — `ch → ch2` (T07d: the one explicit fork point; assigned during T07, extending the 0.0.4 region by one slot) |

---

## Relationship to 0.0.3 deferred list

- ✅ `async` (0.0.4 per 0.0.3 TICKETS §Notes) — now planned here
- Everything else in that list stays deferred (0.1.0 generaics/traits, 0.0.5 unsafe).