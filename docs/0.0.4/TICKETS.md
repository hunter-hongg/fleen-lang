# 0.0.4 — `async` / `await` + channels (Tickets & Decisions)

> Companion to `docs/0.0.4/PLAN.md`.  One card = one unit of work, closed with
> tests.  Decisions D1–D6 referenced here are **settled** (see PLAN §1).
> **Status: IN PROGRESS — T01–T05 implemented; T06–T10 planned.**

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

## T06 — Multi-thread executor (F6)

- [ ] Worker-thread pool + scheduler queue + cooperative `Pending`
- [ ] Top-level bare `await` boots a blocking drive until the root completes
- [ ] `std.io.sleep(n)` awaitable timer; `std.async.yield_now()`
- [ ] Panic re-raise as `RuntimeError` at the awaiting site
- [ ] Tests: `tests/vm/async_valid` multi-task, ordering regressions

---

## T07 — Channels (F9; D3/D6)

- [ ] `channel<T>` type + `ChannelNew/ChannelSend/ChannelRecv` (0xC4–0xC6); runtime variant `Channel`
- [ ] Bounded buffer; `send` suspends on full, `recv` suspends on empty
- [ ] Payload move: Copy by copy, owned by ownership; explicit `move x` required (D6)
- [ ] `std.concurrent.channel<T>(cap)`
- [ ] Tests: `tests/vm/channel_*`, `tests/e2e/channel_*`

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
| 0xBC–0xFF | reserved (BYTECODE.md §4.2); 0.0.4 uses 0xC0–0xC6, 0xC7–0xDF reserved for expansion |

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
| 0xC4 | `ChannelNew` | TBD (T07) |
| 0xC5 | `ChannelSend` | TBD (T07) |
| 0xC6 | `ChannelRecv` | TBD (T07) |

---

## Relationship to 0.0.3 deferred list

- ✅ `async` (0.0.4 per 0.0.3 TICKETS §Notes) — now planned here
- Everything else in that list stays deferred (0.1.0 generaics/traits, 0.0.5 unsafe).