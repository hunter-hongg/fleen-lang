# 0.0.4 — `async` / `await` + channels (Tickets & Decisions)

> Companion to `docs/0.0.4/PLAN.md`.  One card = one unit of work, closed with
> tests.  Decisions D1–D6 referenced here are **settled** (see PLAN §1).
> **Status: PLANNING — nothing implemented.**

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

- [ ] typeck: reject `ref T` anywhere in an async fn / captured across a boundary → `RefInAsync`
- [ ] typeck: reject **global writes** inside async fn → `AsyncGlobalWrite`
- [ ] Tests: `tests/typeck/invalid/ref_in_async.fln`, `async_global_write.fln`

---

## T04 — VM coroutine frame (F4; D1)

- [ ] `FutureFrame { func, operand_stack, resume_pc, live_locals }`
- [ ] `Suspend` snapshot / resume from `resume_pc`
- [ ] `Future` value tag (`0x0D`)
- [ ] Tests: `tests/vm/async_valid` coroutine save/restore

---

## T05 — Opcodes + lower/codegen (F5)

- [ ] `AsyncStart/Suspend/Poll/Await` (0xC0–0xC3)
- [ ] Lower async fn → `AsyncStart`; `await` → `Poll`/`Await`; version → **4**
- [ ] Verify: `Δdepth` for the 4 new instructions; version gate `{1,2,3,4}`
- [ ] Tests: `tests/vm/async_valid`, `tests/verify`

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
| 0xC0 | `AsyncStart` | TBD at implementation |
| 0xC1 | `Suspend` | TBD |
| 0xC2 | `Poll` | TBD |
| 0xC3 | `Await` | TBD |
| 0xC4 | `ChannelNew` | TBD |
| 0xC5 | `ChannelSend` | TBD |
| 0xC6 | `ChannelRecv` | TBD |

---

## Relationship to 0.0.3 deferred list

- ✅ `async` (0.0.4 per 0.0.3 TICKETS §Notes) — now planned here
- Everything else in that list stays deferred (0.1.0 generaics/traits, 0.0.5 unsafe).