# 0.0.4 — `async` / `await` + channels (Tickets & Decisions)

> Companion to `docs/0.0.4/PLAN.md`.  One card = one unit of work, closed with
> tests.  Decisions D1–D6 referenced here are **settled** (see PLAN §1).
> **Status: IN PROGRESS — T01–T09 implemented; T10 planned.**

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
Async **lowering** is wired as of T05: `TypedExprHir::Await` lowers to the
`Await` opcode and an async fn's entry block opens with `AsyncStart` (see the
T05 card).

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
either accept and document that limit or tighten D5. *(Settled by T08c:
accepted and documented — see the T08 card.)*

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

The struct also carries a `FutureState` the original card omitted. As drafted
it had three variants — `Resumable`, `Completed(Value)` and `Failed(RuntimeError)`
— but **T06 reduced it to `Resumable` only**: the cooperative scheduler drives a
task to completion in one resume (the payload travels on the operand stack to
the `Await` site, and an error propagates directly from `step()`), so
completed/failed frames are never stored. The `FutureNotResumable` guard stays
as the defensive backstop for malformed bytecode; the T06 re-raise at the
awaiting site keeps its location because `step()` writes `frame.ip` only on
success. `resume_pc` is `usize`, mirroring `CallFrame::ip`, rather than the
`u32` in PLAN §4.1's sketch.

Still open for T05/T08: `clone` of a `Future` is currently reachable (typeck
treats it as an ordinary owned type, and `CloneGlobal` deep-copies on every
global read), which would snapshot a suspended coroutine. `Value` derives
`Clone`/`PartialEq` regardless, so the language rule and the defensive backstop
are T08's to settle; D2 already guarantees no `ref` handle can appear in a
snapshot, checked by a `debug_assert` here. *(Settled by T08a/T08d: every
copy route is rejected statically and backstopped in the VM — see the T08
card.)*

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
- `Suspend` is defined, verify-tabled and dispatched, but no T05 lowering
  emits it (there is no channel/sleep/`yield_now` source yet), so a verified
  program never reaches it — in T05 the VM arm was the defensive
  `RuntimeErrorKind::UnsupportedAsyncSuspend`, which **T06 replaced with the
  real yield** (`suspend_chain` into the scheduler; outside a future context
  the defensive error is now `StackShape`). The T06 card records the change.
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

**Review fixes (post-T01–T07 full review)**:

- **The nested join is matched by task id, not counted.** The single-slot
  `waiting_join` counted *any* task completing at the boundary: a sibling
  finishing while a nested run was outstanding released the waiter early, a
  second nested `std.async.run` overwrote the waiter (dropping its parked
  chain), and `run([])` parked the caller forever (a zero count no completion
  can satisfy). Now `std.async.run` assigns each spawned task an id carried on
  the chain's root frame (`FutureFrame::task_id`, preserved through every
  snapshot/resume via `CallFrame::task_id`); `waiting_join` is a stack of
  `JoinWait { task, pending: Vec<u64> }`, a completion is matched against each
  waiter's pending set, and an empty pending set (`run([])`) re-enqueues the
  caller immediately. `Task` no longer derives `Clone` — the released
  waiter's chain is moved back into the scheduler. Regressions:
  `nested_nested_run_releases_every_level_in_order`,
  `run_with_an_empty_task_list_completes`,
  `sibling_completion_does_not_release_a_nested_join_early`,
  e2e `nested_run_sibling` / `nested_run_two_levels`.
- **Timer wake order is deterministic**: `promote_timers` promotes due timers
  in insertion order (`Vec::remove`, not `swap_remove`), so tasks whose
  deadlines fall in the same promotion pass wake in the order they went to
  sleep (regression: e2e `timer_wake_order`).

**Review fixes (2026-10-25 completion review):**

- **`std.async.run` from a *sync helper* inside a task now joins in place.**
  The builtin armed the async-body join expectation on the runtime inherited
  `is_future` flag, but the compiler emits the pairing `Suspend` only in
  source-async bodies — a sync helper inherits the flag and emits none, so
  the ids were stranded until some unrelated task's `Suspend` consumed them
  (the join contract broke: the caller continued past `run(...)` before its
  tasks ran, and a timer-ordered variant lost a continuation silently). The
  VM now distinguishes the two by code shape — an async body's `code` always
  opens with `AsyncStart` (`frame_is_async_body`; pinned by the lower-pass
  test) — and a sync-helper caller drives its spawned ids to completion
  inside the builtin (`join_spawn_ids`, frame-depth completion detection, by
  task id). The same helper makes `std.io.sleep`'s deadline guard precise
  (no more dead deadline entries for sync helpers). Regressions:
  `run_from_a_sync_helper_joins_in_place` (a spawned task satisfiable only
  by the helper's continuation is now `ChannelDeadlock` — it used to
  complete),
  `run_from_a_sync_helper_completes_spawned_tasks_before_returning`,
  `nested_sync_helper_run_inside_a_driven_task_completes`, e2e
  `run_from_sync_helper` (worker output before "after join").
- **`drive_join` reports a stuck join instead of dropping it**: the empty-
  scheduler exit used to return `Ok` with a non-empty `waiting_join`
  (silent loss); it now raises the new `JoinDeadlock` (defensive — the
  static rules keep every spawned task reachable). Spawned-task completions
  pop their stray `unit` payload (root id ≠ 0) instead of leaving one slot
  per task on the join caller's region until the final unwind.
- **Error aborts drain the channel waiter queues** (`release_channel_waiters`
  in `Vm::run`): a parked chain holds its channel's handle, so the
  chain ↔ `ChannelInner` edges formed an `Rc` cycle that kept the aborted
  tasks' stacks alive until process exit.
- **`Await`/`Poll` roll back the awaiting frame's ip when `resume_frame`
  fails**, restoring the `locate_error` invariant (`step()` writes `ip` only
  on success) — a failed resume now reports the `Await`, not the next
  instruction.
- **One clock epoch**: the scheduler's and the VM's `OnceLock` start
  instants are unified (`vm::mono_ms`), so sleep deadlines are computed and
  compared on a single timebase.
- Stale `park_on_channel` doc comment corrected: a sync helper called from
  an async body *does* snapshot soundly (inherited `is_future` passes the
  guard; the whole live chain detaches).

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

- `Type::Channel` exists in both `ast::Type` and `typed_hir::Type`, and
  `OwnedChannelArgRequiresMove` (PLAN §7) is implemented as a new direction of
  check — at the call site, before `prepare_args` — because 0.0.2's ownership
  pass only validates `move`/`clone` *nodes* (there was no existing "this
  argument must be a move" check to reuse). A `ref channel<T>` parameter is
  rejected outright (`ChannelNotBorrowable`): send/recv park and resume across
  the scheduler, which would carry the borrowed handle across a suspension.
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
  *(Settled by T08b/T08d: the global alias is withdrawn — channel globals
  are now rejected — and the backstop records the runtime payload kind; see
  the T08 card.)*

---

## T08 — Send static rule integration (F8; D4)

- [x] Wire D4 as the single cross-thread rule: borrow handles never cross
      async/task/channel (with T03); everything else unsafely-free (no sharing)
      — the audit found D4's *ref* half already complete (T03 lexical rule +
      `ChannelNotBorrowable`), so the work closed the **copy-fork** half: every
      Value-level copy route that would fork a suspended `Future` frame or
      alias a channel buffer is now rejected statically (decisions T08a/T08b)
- [x] Decide the D5 enforcement boundary for sync helpers (decision T08c:
      accepted and documented — no call-graph analysis)
- [x] Defensive runtime `RuntimeError` backstop on misused `Future`/`Channel`
      (`FutureNotClonable`, `ChannelNotClonable`, `ChannelExpected`,
      `ChannelPayloadMismatch` — decision T08d)
- [x] Tests: typeck boundary suite (15 invalid fixtures) + defensive VM paths
      (7 raw-opcode unit tests)

**Decisions (settled during implementation — four forks the card left open):**

- **T08a — a `Future<T>` is move-only everywhere a copy could fork it; the
  array element type is the one storable exception.** Every position that is
  read by deep copy would fork the suspended coroutine frame, so typeck now
  rejects: `clone f` (`FutureNotClonable`), a `Future<T>` struct field, box
  cell, or global (`FutureNotStorable`), `ts[i]` / `for`-over-`[Future]`
  (`FutureIndexRead`), and `f == g` (`InvalidOperand`, same rule as struct/
  array/channel). Arrays stay legal as element *storage* only because
  `std.async.run` consumes them by moving the frames out wholesale — element
  reads are rejected, so no source shape can copy a frame. This retires the
  T04 open item (`clone` of a `Future` used to typeck as an ordinary owned
  type) and the codegen-side `debug_assert!` panic for `Future` struct fields
  (a user error must be a Diagnostic, never a compiler panic — SPEC §4). The
  ownership pass additionally rejects a bare *read* of a Future local
  (`OwnedArgRequiresMove`) because `LoadLocal` clones the `Value` too —
  defense in depth; every source shape already trips an earlier rule.
  What this locks: no language route may observe or duplicate a suspended
  frame — swapping the coroutine representation later (D1) stays invisible.
- **T08b — the T07 accepted-for-0.0.4 channel-global alias is withdrawn.**
  T07 had accepted "a bare read of a channel-global lowers to a plain handle
  load — an alias without explicit `split`". Wiring D4 ("borrow handles never
  cross; everything else unsafely-free, no sharing") exposed the contradiction:
  the alias *is* sharing without the one fork point. A channel (like a Future)
  is now rejected as a global type (`ChannelNotStorable`), and `box channel`
  is rejected for the same reason. Channels move through bindings and
  `std.concurrent.split`; the split-only fork discipline now has no
  exception. What this locks: handle aliasing is observable only through an
  explicit `split` in source.
- **T08c — the D5 sync-helper gap is accepted and documented, not closed.**
  T03's known gap stands: an async fn calling a *synchronous* helper that
  writes its own module's global is not caught, because catching it needs a
  call graph and functions are first-class values, which makes call-graph
  analysis necessarily incomplete. Accepted on two grounds: (1) the current
  executor is single-threaded cooperative (T06; the worker pool is a deferred
  optimization), so such a write cannot actually race in 0.0.4 — D5
  future-proofs the semantics, it does not defend a present race; (2) D4's
  value is being one static rule — an interprocedural approximation would
  add false positives without proving absence. Recorded as a documented
  language limit (this card + DESIGN wording in T10); tightening would be a
  0.1.0+ decision alongside traits/closures.
- **T08d — the payload backstop records the runtime *kind*, not an encoded
  static type.** T07 deferred the defensive payload check because the MIR's
  `payload_ty` is dropped by codegen and the channel opcodes are operand-less.
  Extending the 0xC4–0xC7 encoding (or the const pool) was considered and
  rejected while version 4 is unreleased-but-frozen-in-docs: it would touch
  encoder/decoder/verify for a check that (per D4) is only ever a backstop —
  typeck owns static uniformity and verify already gates hand-written
  bytecode. Instead `ChannelInner` records the `PayloadKind` of the first
  accepted payload (one `Value` variant per *static* type; `Ok`/`Err` collapse
  to `Result` because they are two constructors of one type) and a later
  `send` of a different kind raises `ChannelPayloadMismatch` at the send,
  rather than letting the corruption surface delayed and mislocated at the
  receiver's next typed instruction. The recorded `payload_ty` in MIR stays
  unused by codegen (a future encoding change can pick it up; nothing to
  remove). No opcode, encoding, or version change — BYTECODE.md §10 and the
  locked opcode map are untouched.

**Notes (implementation):**

- The VM backstops are unreachable from compiled source by construction —
  each one mirrors a typeck rule and fires only for malformed bytecode:
  `LoadLocal`/`CloneLocal`/`CloneGlobal`/`ArrGet` on a `Future` (a copy would
  fork the frame → `FutureNotClonable`), the same on a `Channel` (an alias
  without `split` → `ChannelNotClonable`), non-channel operands of the
  channel opcodes (`ChannelExpected`, mirroring `FutureExpected`). Deliberate
  non-backstops: `LoadLocal` on a channel stays legal (the Rc-bump clone *is*
  the handle-load semantics) and `DupDeep` is untouched (it is the compiled
  form of every owned-RHS assignment, including `f = gen(1)` — the clone of a
  not-yet-started frame there is the assignment's move, observed once).
- The T04 test `nested_future_survives_an_outer_snapshot` read its slot back
  with `LoadLocal` — exactly the fork path T08 now rejects; the fixture's
  host func uses `MoveLocal` instead (the semantics compiled source is
  forced into). Its intent (a nested future survives an outer snapshot
  unchanged) is unchanged.
- `tests/typeck/valid/async_fn.fln` moved its `f: Future<int>` annotation
  binding from top level into a function — T08 forbids Future globals, and
  the fixture's intent (annotation-position support) is preserved.

**Notes (review follow-ups, 2026-10-24):** a review found the array-storage
exception of T08a leaking: the storable/copy rules matched a bare `Future`
type directly, so a Future *inside an array* slipped through every position
that copies the array wholesale — `clone ts`, a global/field/box `[Future]`,
and nested `tss[0]` / `for`-over-`[[Future]]` (channel arrays were already
sealed at the array literal by T07d, so only the Future side leaked). All
copy routes now see through `[T]` nesting via `Type::contains_future`:
`clone` → `FutureNotClonable`, global/field/box → `FutureNotStorable`,
element reads → `FutureIndexRead`; the ownership bare-read rule uses the
same predicate as its backstop. One route sat in the std builtins: a bare
`[Future]` ident argument to `std.async.run` cloned the array while leaving
the binding alive, so `run(ts); run(ts)` drove the task twice — builtin
calls (fixed and schematic) now run `reject_bare_owned_arg` on any
future-typed argument, so the spelling is `std.async.run(move ts)` (the T06
`run_with_an_empty_task_list_completes` fixture updated accordingly; the
`run([...])` literal spelling is unchanged). Two diagnostic fixes rode
along: the
`OwnedArgRequiresMove` help text no longer suggests `clone` for a
Future-typed value (it would be rejected by `FutureNotClonable` — it now
names `move` / `await` / `std.async.run`), and the runtime
`ChannelPayloadMismatch` carries the recorded and offending `PayloadKind`s
so the message names both (this also gives the previously dead
`PayloadKind::name` its documented caller). `tests/typeck/valid/async_fn.fln`
gained its missing trailing newline.

**Review fixes (2026-10-25 completion review):**

- **The fork rules now see through `Result`** — the 10-24 `[T]`-nesting fix
  had a sibling hole: `Type::contains_future` walked only `Type::Array`,
  while `Result<T, E>` carries both payloads *by value* (`Value::Ok`/`Err`
  box them; the derived `Clone` deep-copies), and every channel check was a
  top-level `matches!` with no mirror predicate at all. Eight source shapes
  forked a suspended frame or aliased a buffer through a `Result` wrapper:
  `clone r` (Future *and* Channel), a `Result<Future, _>` struct field, box
  cell, `ts[i]`, `for`-over-`[...]`, `std.arr.push`/`get` on the moved
  array, and `r == c`. The fixes: `contains_future` walks `Result` too; a
  new mirrored `Type::contains_channel` joins the field/global/box/clone/`==`
  rules and the array-literal element seal (channels, unlike futures, have
  no wholesale array consumer); the ownership bare-read backstop covers
  channel-containing locals; and the `OwnedArgRequiresMove` help gained
  Result-shaped branches (the old text called every non-top-level Future an
  "array"). `?` on a `Result<Future, _>` local is now rejected
  (`OwnedArgRequiresMove`) — extracting through `?` clones the payload while
  the binding stays live, the same fork; the legal spelling is `choose move
  r { when Ok(v) { v await } ... }`. Regressions: typeck invalid
  `future_result_{clone,field,index,for,box,eq,nested_array_clone}`,
  `channel_result_{clone,array}`.
- **A `ref` payload cannot cross a channel (D4 at the send).** `ch.send(s)`
  with `s: ref string` typecked — `unify_arg_ref_aware` strips `Ref` when
  checking the payload, and the send move-check only fired for owned types —
  and the VM had no `Value::Ref` backstop in `do_channel_send`: the
  rendezvous path tripped `suspend_top_frame`'s debug_assert (a
  user-reachable panic) and the buffered path silently delivered a stale
  operand-stack slot as the "value" (exit 0). The send path now rejects a
  borrowed payload outright (`ChannelPayloadBorrowed`: send `clone s` for a
  copy), and the VM backstops the same condition for malformed bytecode.
  Regression: typeck invalid `channel_ref_payload`; VM raw
  `raw_channel_send_with_a_ref_payload_is_rejected`.
- **The VM copy backstops walk containers** (`deep_fork_kind` over
  `Ok`/`Err`/arrays, mirroring the typeck predicates): `LoadLocal`/
  `LoadGlobal` (a top-level channel load stays the legal handle load; a
  buried one is an alias), `CloneLocal`/`CloneGlobal`/`ArrGet`, and the
  `std.arr.get`/`push`/`set` builtins — whose `move`'d-array argument passes
  the bare-read guards and then clones its elements host-side. Regressions:
  VM raw `raw_load_global_on_a_future_is_rejected`,
  `raw_arr_get_builtin_on_an_ok_future_element_is_rejected`.
- Accepted, documented language limit: `?` cannot take a `move` operand
  (`move r?` does not parse), so a `Result<Future, _>` local has no `?`
  propagation spelling — `choose move r` extraction is the route. Tightening
  the postfix grammar is a 0.1.0 decision alongside the other ergonomics.

---

## T09 — Integration fixtures & std e2e

- [x] `tests/e2e/valid/fib_async`, `channel_pingpong`, `channel_buffer`, `multi_task` (.fln + .expected)
- [x] `tests/e2e/invalid/await_non_future`, `ref_in_async`, `async_global_write`, `owned_channel_arg` (.fln + .exit + .stderr)
- [x] Full §10 matrix through `cargo fln` and `flint`
- [x] Fixed-byte v1 + v2/v3 `.flnc` fixtures still execute (BYTECODE.md §10)

**Notes (implementation):**

- **`fib_async` exercises the sync-drive path, not the suspend path.** fib has
  no suspend source, so every `await` drives its future to completion
  synchronously (T05); the fixture pins async recursion through the full
  pipeline — each recursive call yields a `Future<int>` awaited inline
  (`fib(n - 1) await + fib(n - 2) await`; `await` binds tighter than `+`).
  Suspended-frame resume stays covered by the T06/T07 fixtures
  (`async_multi_task` / `channel_*`).
- **`channel_pingpong` shares one rendezvous channel through `split`** (T07d's
  one fork point) and terminates by construction: both sides do exactly 3
  sends and 3 recvs, so no task is left parked when `run` joins — with no
  `close`, any unbalanced send/recv would end in `ChannelDeadlock` (T07b).
- **`channel_buffer` pins the bounded-buffer boundary without timers**: the
  producer prints after each send, so "sent 0/1" (buffered immediately) vs
  the blocked send 2 marks the capacity-2 edge; the interleaving relies only
  on the locked T07 rules (complete-before-park, FIFO buffer + sender queue).
- **`multi_task` composes every executor feature** — sleep timer, yields, a
  rendezvous channel shared via `split`, sequential awaits in a driver, and a
  nested `std.async.run` from an awaited body (the T06 review-fix path).
  Ordering relies only on the locked rules: ready tasks beat due timers,
  timers promote in insertion order, channel queues are FIFO, nested joins
  match task ids. `sleep` in an async body needs no explicit `await` (the
  compiler lowers `std.io.sleep(n)` to `Call; Suspend`).
- **`owned_channel_arg` is the call-site direction** of the move rule (a bare
  channel as a task-fn argument → `OwnedArgRequiresMove`), distinct from the
  existing e2e `channel_send_requires_move` (the send-payload direction).
- The invalid fixtures' `.stderr` needles quote the typeck messages verbatim
  (the harness matches `contains`); exit code is 1 (compile failure).
- **Diagnostic fix (found while writing the fixtures)**: the
  `OwnedArgRequiresMove` help suggested `clone ch` for a channel-typed value,
  which `ChannelNotClonable` then rejects. The message now has a channel
  branch (it names `move` / `std.concurrent.split`, the one legal fork
  point); the e2e `owned_channel_arg` needle pins the corrected text.
- The §10-through-`flint` item is `flint-cli/tests/e2e.rs::
  flint_async_and_channel_projects_run_and_test`: project-mode compile of
  async/await/channel (reserved words legal as module/member paths),
  `flint run` of an awaited async fn, and `flint test` of a channel
  rendezvous task. The v1/v2/v3 fixed-byte `.flnc` fixtures keep their
  dedicated suites (`v1_compat.rs`, `compat.rs`) — re-run green, no changes.

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