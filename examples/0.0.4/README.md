# 0.0.4 feature tour — an `async`/`await` + channels project example

A small multi-file Fleen **project** that touches every 0.0.4 feature
(PLAN F1–F10).

## Run

```bash
# one-stop driver (module system + std, no manifest needed)
cargo fln --project examples/0.0.4/tour

# or through flint
cd examples/0.0.4/tour
cargo run -q -p flint-cli -- run

# the tests/ e2e case (`flint test` — F10, the concurrency matrix)
cd examples/0.0.4/tour
cargo run -q -p flint-cli -- test
```

The program's output is fully deterministic (the cooperative executor's
ordering rules: ready tasks beat due timers, timers promote in insertion
order, channel queues are FIFO, nested joins match task ids).

## Features exercised

| Feature | Ticket | Where |
|---------|--------|-------|
| `async fn` (caller sees `Future<T>`, body stays `T`) | F1 | `tasks.add`, `pipeline.half` |
| `await` postfix, incl. `e await?` composition | F2 | `tasks.add(…) await`, `pipeline.half(n) await?` |
| `Future<T>` value, non-Copy, movable | F3 | `f: Future<int> = tasks.add(20, 22)` |
| Cooperative executor: `std.async.run` / `yield_now` / `sleep` timers, top-level bare await | F6 | §3 and §6 of `main.fln` |
| Sync-helper `run` joins in place | F6 | `tasks.spawn_batch` (called from `tasks.orchestrator`) |
| Reserved words `async`/`await`/`channel` legal as module/member names | F7 | `import std.async`, `import std.concurrent` |
| `channel<T>(cap)` constructor, rendezvous (cap 0) | F9 (T07e) | §4: `channel<int>(0)` |
| Bounded buffer (send parks on full), FIFO wake | F9 (T07d) | §5: `channel<string>(2)` |
| `std.concurrent.split` — the one explicit fork point | F9 (T07d) | §4/§5: `split(ch)` |
| Owned payload move at send (D6) | F9 | §5: `out.send(move a)` |
| `async` funcs across modules with `pub` | F1 / V05 | `import tasks; import pipeline;` |

The **static boundary rules** (F8) are *rejections*, so they have no runtime
demo here: no `ref` in async bodies (`RefInAsync`), no global writes in async
(`AsyncGlobalWrite`), `clone f` / `clone ch` rejected, Future/channel
non-storable, and a borrowed channel payload rejected
(`ChannelPayloadBorrowed`). See `docs/0.0.4/TICKETS.md` and the
`tests/e2e/invalid/async_*` / `channel_*` fixtures for the error shapes.

## Files

```
tour/
├── flint.toml          # manifest (flint, 0.0.3 V08)
├── src/
│   ├── main.fln        # the six numbered demos
│   ├── tasks.fln       # async task helpers (beat / sleeper / ping / pong / …)
│   └── pipeline.fln    # Result-returning async fn for the `await?` demo
└── tests/
    ├── ping.fln        # `flint test` case: channel rendezvous
    └── ping.expected   # expected stdout ("got 1")
```
