# 0.0.5 feature tour — `unsafe` / `trusted` / `ptr<T>` / `addr` / `deref_ptr`

A small multi-file Fleen **project** that touches every 0.0.5 language feature
(PLAN F1–F4: the pointer/unsafe layer) and exercises it with the 0.0.5
toolchain.

## Run (with the 0.0.5 compiler)

```bash
# one-stop driver (module system + std, no manifest needed)
fleen-vm --project examples/0.0.5/tour

# or through flint
cd examples/0.0.5/tour
flint run

# the tests/ e2e case (`flint test`)
cd examples/0.0.5/tour
flint test
```

The program's output is fully deterministic.

## Features exercised

| Feature | Ticket | Where |
|---------|--------|-------|
| `ptr<T>` type (non-Copy, move-only), `addr` / `deref_ptr` read+write | F1/F2/F5 | `raw.bump`, `raw.read_ptr`, `raw.take_ownership` |
| `addr` of a **local** place | F2 | §1 of `main.fln`, `raw.bump` |
| `addr` of a **global** place | F2 | §2 of `main.fln`, `raw.touch_global` |
| `addr` of other place forms (local/global/field/element/box interior defined by 0.0.5) | F2 | §2–§3 of `main.fln`, `raw.touch_places` |
| `ptr<T>` as parameter and return value (move-only hand-off) | F1 | `raw.bump`, `raw.make_ptr`, `raw.take_ownership` |
| `unsafe func` (body may do unsafe ops; callable only from unsafe/trusted) | F3 | `raw.bump`, `raw.raw_global` |
| `trusted func` (attested wrapper; callable from safe code) | F3 | `raw.hedgehog`, `raw.touch_global`, `raw.touch_places` |
| `async unsafe func` (composes with `async`; call+await needs unsafe/trusted ctx) | F4 | `raw.risky_step`, `raw.run_risky` (awaited from `trusted`; ptrs never cross await/task/channel — `PtrAcrossBoundary`) |
| `deref` stays box-only (pointer spelling is additive) | F2 | `deref` remains box-only while `deref_ptr` reads/writes pointer referents |

**Static rejections** (no runtime demo): `addr`/`deref_ptr` outside an
unsafe/trusted body (`UnsafeOpOutsideUnsafe`), calling an `unsafe func` from
plain code (`UnsafeCallOutsideUnsafe`), `clone p` on a pointer
(`CloneOfPointer`), a `ptr` crossing an await/task/channel boundary
(`PtrAcrossBoundary`), and storing a `ptr` in a field/element/box/global
(`PtrNotStorable`). `ptr`, `addr`, `deref_ptr`, `unsafe`, `trusted` are
reserved words (the 0.0.5 breaking change).

## Files

```
tour/
├── flint.toml
├── src/
│   ├── main.fln        # the five numbered demos
│   └── raw.fln         # `raw` module: unsafe/trusted ptr operations
└── tests/
    ├── ptr_case.fln
    └── ptr_case.expected
```
