# 0.0.5 — `unsafe` / `trusted` / `ptr` / `addr` + Toolchain Maturity

> **Codename**: Careful Hedgehog 🦔 (0.0.1 Swift Fox 🦊, 0.0.4 Patient Ants 🐜).
>
> **Goal**: land the pointer/unsafe layer sketched in `DESIGN.md` §10/§12 since
> 0.0.2, and — for the first time — treat the *toolchain around the language*
> as a deliverable: a standardized CLI, a multi-platform CI/CD release
> pipeline, and a std distribution story that lets a released package be used
> without the compiler's embedded copy.
>
> **Theme**: the unsafe surface is minimized by rule, not by hope.  Fleen has
> no borrow checker, so *creating* a pointer is already an unsafe operation
> (unlike Rust's safe `&`); everything ptr-shaped is gated behind `unsafe`
> functions, and pointers — like borrows before them — never cross an async /
> task / channel boundary.
>
> **Status**: PLANNED — no ticket started.  Decisions D1–D8 below are settled
> at planning level (2026-10-25); each names the alternative it rejected and
> what the choice locks.  They may be revised only by a new entry here, not
> silently in a ticket card.

---

## 0. Relationship to adjacent versions

| Version | Scope |
|---------|-------|
| 0.0.4 | complete |
| **0.0.5** | **`unsafe` / `trusted` / `ptr` / `addr` + CLI standard + CI/CD release pipeline + std distribution** |
| 0.1.0 | generics / traits / operator overloading (closures land here — see D3) |

The unsafe layer is self-contained (no trait/generics machinery).  The
toolchain features are orthogonal to it and to 0.1.0.

---

## 1. Settled decisions (D1–D8)

### D1 — `ptr<T>` is **non-Copy, move-only**

`ptr<T>` joins the 0.0.4 family of non-Copy owned values (`Future<T>`,
`channel<T>`): `move p` works, `clone p` is a typeck error
(`CloneOfPointer`).

- **Rejected**: Copy `ptr<T>`.  Copy-vs-move does not change aliasing safety
  either way — Fleen has no borrow checker, so `addr x` twice already aliases,
  and no static rule can prevent it.  Move-only is chosen for **semantic
  consistency** with 0.0.4's owned non-Copy types and to keep `move` explicit
  at every boundary; the aliasing risk is absorbed by D2 (creating any pointer
  is unsafe) rather than by the copy rule.
- **What this locks**: the typeck Copy table and the ownership sub-pass get a
  new non-Copy type; `clone p` rejection is permanent language surface.

### D2 — `addr` is an **unsafe operation** (unlike Rust's `&`)

Both creating a pointer (`addr x`) and using one (`LoadPtr` read / `StorePtr`
write) require an unsafe context (D3).  Rust makes `&` safe because its borrow
checker guarantees referent validity; **Fleen has no borrow checker**, so safe
code could move the referent out from under a pointer:

```fleen
p = addr x;     // D2: already unsafe — creating the dangling possibility
y = move x;     // safe code, no borrowck to stop it
```

Making `addr` safe would ship a language where one unsafe op plus one *safe*
op produces a dangling pointer, and no diagnostic can catch the safe half.
Gating `addr` keeps the entire "pointer exists at all" state behind `unsafe`,
which is the smallest defensible surface for a borrowck-less language.

- **Rejected**: safe `addr` / unsafe-only deref (the Rust split).  It locks in
  a validity guarantee the compiler cannot actually make.
- **Cost accepted**: even read-only pointer use needs an `unsafe` function.
  `trusted` (D3) is the relief valve for vetted wrappers.

### D3 — `unsafe` / `trusted` are **func modifiers**, not lambdas

`DESIGN.md` §12's draft spells them `unsafe raw_op = () => { … }` — a lambda
form.  Fleen has **no closures until 0.1.0**, so the draft syntax is not
implementable in 0.0.5.  The modifier form is adopted, exactly mirroring
`async func` (0.0.4):

```fleen
unsafe func raw_op(): int { … }      // may contain unsafe ops (addr, ptr deref)
trusted func safe_wrapper(): int { … }  // may also contain unsafe ops; callable from anywhere

func normal() {
    safe_wrapper()        // ok: trusted is callable from safe code
    // raw_op()           // error: UnsafeCallOutsideUnsafe
}
```

Rules:

| Construct | Body may contain unsafe ops | Callable from |
|-----------|------------------------------|----------------|
| `unsafe func` | yes | `unsafe func` / `trusted func` bodies only |
| `trusted func` | yes | anywhere (the author attests its safety) |
| plain `func` | no | anywhere |

- **Rejected**: the lambda form of the §12 draft — it requires closures, which
  are 0.1.0.  **Rejected**: an additional `unsafe { }` block form — function-
  level only keeps 0.0.5 to one context-tracking rule; blocks can be added
  later without breaking anything, whereas removing a block form would.
- **Doc debt**: `DESIGN.md` §12's lambda sketch is superseded by this decision;
  T10 re-syncs §12 (and §10's `ptr`/`addr` lines) to the final syntax.

### D4 — Pointers never cross an async / task / channel boundary; `unsafe async`

Sibling of 0.0.4's D2/D4 (the borrow-free boundary): a `ptr<T>` may not be
captured across an `await`, moved into a task, or sent through a channel —
typeck error `PtrAcrossBoundary`.  Rationale: 0.0.4's one-rule `Send` story
only stays one rule if every non-Copy handle whose validity depends on a
*frame staying put* is excluded; a pointer's referent lives in (or is promoted
from) a coroutine frame that the scheduler moves at will (0.0.4 T04 notes:
frames are location-independent by design).

`unsafe async` is allowed: an `async func` whose body contains unsafe ops must
be declared `async unsafe func`.  Marking changes nothing for the caller
mechanics — calling (and awaiting) an unsafe async fn still requires an
unsafe/trusted context, same as a sync unsafe fn.

- **Rejected**: letting ptr ride the 0.0.4 `Send` rule as an owned value.  It
  would make "ownership moved" mean "raw address moved", invalidating the
  frame-location independence 0.0.4 T04 locked.
- **Deferred to 0.1.0**: ptr through channels even inside `unsafe` (a legit
  want once cross-task heaps exist; not now).

### D5 — Bytecode: three opcodes in the reserved band, version 5, zero layout change

| Opcode | Mnemonic | Purpose |
|--------|----------|---------|
| 0xC8 | `AddrOf` | promote the referent slot to a stable cell; push `Value::Ptr` |
| 0xC9 | `LoadPtr` | read through a pointer (bounds/type-checked defensively) |
| 0xCA | `StorePtr` | write through a pointer |

The `0xC8–0xDF` band was reserved as "0.0.4 expansion space"; 0.0.5 takes its
front three slots.  Every existing encoding (v1–v4) is untouched — the
BYTECODE.md §10 compatibility promise holds.  `.flnc` `version` bumps to **5**
(readers accept `{1, 2, 3, 4, 5}`); **no table layout change** (the `Ptr` run
value is not serialized, like `Future`/`Channel` before it).  T10 rewords the
band label in BYTECODE.md §4.2.

- **Rejected**: a fresh `0xE0+` band — gratuitous fragmentation of the number
  space; the reserved band exists precisely for this.

### D6 — CLI: `clap` (derive) for all three binaries, hand-rolled ANSI color

`fleen-vm`, `flint`, and `fleen-verify` adopt a uniform CLI spec:

- flags: `--project <dir>`, `-h/--help`, `-V/--version`, `--quiet`, `--no-color`
- exit codes formalized and documented in `--help`: `0` success, `1`
  compile/resolve/verify/runtime failure, `2` usage/IO error (current
  behavior, now written down)
- diagnostics print with span highlighting: `file:line:col` header, the source
  line, a `^^^^` underline, then indented `note:`/`suggestion:` lines
  (rustc-style); color is plain ANSI sequences — no color crate — and is
  disabled by `--no-color` or a set `NO_COLOR`

- **Rejected**: continuing the hand-rolled `env::args()` parsing.  The flag
  surface is growing across three binaries; hand-rolling locks in three
  divergent parsers and help texts.  `clap` is the conventional Rust choice.
- **What this locks**: `clap` becomes a workspace-wide dependency (three
  crates); ANSI output code lives in one shared location, not per-crate.

### D7 — Release: six targets, tag-triggered artifacts

The release job triggers on `v*` tags and builds:

| Target | Artifact |
|--------|----------|
| `x86_64-unknown-linux-gnu` | `fleen-0.0.5-x86_64-linux-gnu.tar.gz` |
| `x86_64-unknown-linux-musl` | `fleen-0.0.5-x86_64-linux-musl.tar.gz` (static binary) |
| `aarch64-unknown-linux-gnu` | `fleen-0.0.5-aarch64-linux.tar.gz` |
| `x86_64-apple-darwin` | `fleen-0.0.5-x86_64-macos.tar.gz` |
| `aarch64-apple-darwin` | `fleen-0.0.5-aarch64-macos.tar.gz` (Apple Silicon) |
| `x86_64-pc-windows-msvc` | `fleen-0.0.5-x86_64-windows.zip` |

Each archive contains the three binaries (`flint`, `fleen`, `fleen-verify`),
the `std/` source tree (T08's companion copy), and a short README.  Release
publication follows the established precedent (`release/0.0.x` branch,
`v0.0.x` tag, `gh release`).  musl is included deliberately — a static,
dependency-free binary is the "single-file usable" selling point for a
language distribution.

- **Rejected**: building only on ubuntu (status quo) — it locks "Fleen = Linux
  only", which is exactly what T09 exists to remove.  **Deferred**: wasm and
  FreeBSD targets (no users yet; matrix rows are cheap to add later).
- **Known risk**: `std.io.read_line` / other terminal intrinsics must be
  exercised on Windows CI, not just compiled.

### D8 — std addressing: `FLINT_STD_PATH` → `$FLINT_HOME/std/<version>` → embedded

flint-cli and the `cargo fln` driver share one lookup order:

1. `FLINT_STD_PATH` env var — explicit user override (experiments, std dev);
2. `$FLINT_HOME/std/<compiler-version>/` — the copy shipped in release
   packages (D7 archives put `std/` there on install);
3. the embedded copy (`fleen-compiler/src/std_src.rs`) — always present, the
   default and the fallback.

An external std found at steps 1–2 is admitted only if its version matches
the compiler's; a mismatch is a **hard error naming both versions**, never a
silent downgrade to embedded.

- **Rejected**: dropping the embedded copy — it is what makes the bare
  binary work anywhere and keeps `cargo test` exercising std continuously
  (0.0.3 D8).  **Rejected**: putting `$FLINT_HOME` above embedded without a
  version check — a stale cached std would silently shadow the built-in one.

---

## 2. Feature list

| # | Feature | Category | Depends on |
|---|---------|----------|-----------|
| **F1** | `ptr<T>` type (non-Copy, move-only, D1) | language + compiler + VM | F2, F3 |
| **F2** | `addr x` prefix operator (unsafe op, D2) | language + compiler + VM | F3 |
| **F3** | `unsafe` / `trusted` func modifiers + unsafe-context gating (D3) | language + compiler | F9 |
| **F4** | Boundary rules: `PtrAcrossBoundary`; `unsafe async` (D4) | compiler (typeck) | F1, F3, 0.0.4 D2/D4 |
| **F5** | Opcodes `0xC8–0xCA` + bytecode version 5 (D5) | compiler + VM + verify | F1, F2 |
| **F6** | VM pointer representation (`Value::Ptr`, stable cells) | VM | F5 |
| **F7** | CLI standard: clap, help/version, colored span diagnostics (D6) | toolchain (3 binaries) | — |
| **F8** | CI/CD release pipeline, six targets, tag-triggered artifacts (D7) | toolchain (CI) | T08's std packaging |
| **F9** | `ptr` / `addr` / `unsafe` / `trusted` reserved words | compiler (lexer) | — |
| **F10** | std distribution + flint std addressing (D8) | toolchain (flint + compiler) | — |
| **F11** | Integration matrix + docs sync + version bump | tests + docs | all |

---

## 3. Language design

### 3.1 Keywords

```
ptr        // raw pointer type constructor ptr<T> (reserved word)
addr       // prefix operator: take the address of a place (reserved word, unsafe op)
unsafe     // func modifier: body may contain unsafe ops (reserved word)
trusted    // func modifier: author-attested safe wrapper (reserved word)
```

`ptr<T>` is recognized in type/constructor position like `Future<T>` and
`channel<T>` in 0.0.4 (no bare-`ptr` type).

### 3.2 Syntax

```fleen
x = 42;
p = addr x;              // p: ptr<int>  — legal only inside unsafe/trusted body (D2)
n = deref_ptr p;         // read through p — unsafe op; spelling settled at T02
                         // (postfix `.deref` vs builtin `deref_ptr` vs reusing `deref`)
deref_ptr p = n + 1;     // write through p — unsafe op

unsafe func raw_op(): int {
    q = addr x;
    deref_ptr q
}

trusted func safe_wrapper(): int { raw_op() }   // attested; callable anywhere
async unsafe func risky(c: channel<int>) { … }  // D4: unsafe async form
```

The write-through-ptr spelling and its precedence slot are the one syntax
question left open at planning time; T02 settles it and folds the answer back
here.  `deref` itself (0.0.2) stays box-only — overloading it for `ptr` in
0.0.5 would silently change the meaning of existing safe code if `deref`
received a pointer.

### 3.3 Type rules

| Construct | Rule |
|-----------|------|
| `addr x` where `x: T` (place expr) | `ptr<T>`; **unsafe op** (D2) |
| `deref_ptr p` where `p: ptr<T>` | read: `T` copy out; **unsafe op** |
| `deref_ptr p = v` where `p: ptr<T>`, `v: T` | write; **unsafe op** |
| `ptr<T>` | non-Copy (D1); move-only; no disk serialization |
| `clone p` on `ptr` | typeck error `CloneOfPointer` |
| `p` across await/task/channel | typeck error `PtrAcrossBoundary` (D4) |
| unsafe op outside unsafe/trusted body | typeck error `UnsafeOpOutsideUnsafe` (D3) |
| call to `unsafe func` from plain `func` | typeck error `UnsafeCallOutsideUnsafe` (D3) |

`addr` accepts any place expression with a storage slot: locals, globals,
struct fields of a local/global, array elements of a local/global, and box
interiors (`deref b`).  Referent validity after moves/aliasing is the unsafe
author's contract — that is what `unsafe` means here (D2).

---

## 4. VM impact (F6)

`Value::Ptr` is a VM-internal Rust variant — **not serialized** into `.flnc`
(same rule as `Value::Future` / `Value::Channel`, 0.0.4 PLAN §6).  Const-pool
tags and `FieldTypeTag` are untouched.

The representation question: VM frames live in Rust `Vec`s that **move**
(suspend relocates whole frame regions — 0.0.4 T04), so a raw address into
frame memory is not storable.  The working sketch, for T06 to settle:

- `AddrOf` **promotes the referent slot to a stable heap cell** (a `Box<Value>`
  owned by the VM arena); the local's slot becomes an indirection to that
  cell, and `Value::Ptr` holds the cell handle.  Reads/writes of the local
  before and after promotion are unchanged semantically.
- Cost accepted: one allocation per `addr`-taken place (amortized 0 for code
  that never takes addresses); the promote-on-addr path must keep the
  `size_of::<Value>()` test passing (cell handle, not inline).
- `LoadPtr`/`StorePtr` defensively validate the cell handle; a malformed
  handle is a `RuntimeError` backstop, not the primary defense (the primary
  defense is static: only unsafe code can make a pointer at all, D2/D3).

Alternatives T06 may still weigh: a flat global arena with indices (larger VM
change, touches every access) vs. promoting eagerly (slower common path).
Promote-on-addr is the default because it leaves all non-ptr code paths
byte-identical.

---

## 5. Toolchain: CLI (F7, D6)

Scope, per binary:

| Binary | Today | 0.0.5 |
|--------|-------|-------|
| `fleen-vm` | hand-rolled `env::args()`, only `--project` | clap: `--project`, `--help`, `--version`, `--quiet`, `--no-color`; program args after `--` |
| `flint` | one-line `USAGE` const | clap subcommands `new/build/run/test/check` with per-command help; global `--version` |
| `fleen-verify` | positional input only | clap; same flag set as `fleen-vm` |

Shared: the span-diagnostic pretty-printer (header / source line / `^^^^` /
notes) lives in one crate-level module and is reused by all three; `NO_COLOR`
and `--no-color` disable it; `--quiet` suppresses non-error output.  Exit
codes 0/1/2 are formalized (they already behave this way — T07 writes them
down and tests them).

---

## 6. Toolchain: std distribution & addressing (F10, D8)

- `flint` and `cargo fln` share one std-resolution module: `FLINT_STD_PATH` →
  `$FLINT_HOME/std/<version>` → embedded (D8), with the strict version check.
- Release archives (D7) carry `std/` and an install step copies it to
  `$FLINT_HOME/std/<version>/`.
- `flint check --std` reports which std copy is in effect and its version
  (diagnosing "which std am I using" is the failure mode the version check
  exists for).
- The embedded copy stays the default and the `cargo test` surface (0.0.3 D8
  unchanged).

---

## 7. Toolchain: CI/CD release pipeline (F8, D7)

- `ci.yml`: the existing `check` job gains an OS matrix (ubuntu / macos /
  windows) — compile + test on all three, so breakage is caught before tag
  time, not by the release job.
- New `release.yml` (or a job in `ci.yml`): on `v*` tag → build the six
  targets of D7 → package archives (binaries + `std/` + README) → upload to
  the GitHub Release per the established precedent (`release/0.0.x` branch,
  `v0.0.x` tag, `gh release`).
- The release job runs the full `cargo test` on the tagging commit for the
  native target before uploading anything.

---

## 8. Compiler-stage changes

| Stage | Changes |
|-------|---------|
| **Lex** | `TokenKind::{Ptr, Addr, Unsafe, Trusted}`; keyword tables; reserved-word diagnostics |
| **Parse** | `ptr<T>` in `parse_type`; `addr` prefix expr (`ExprAddr`) in the unary group; `unsafe` / `trusted` modifiers on `func` (`is_unsafe` / `is_trusted`); the write-through-ptr form (T02) |
| **Resolve** | ptr type wiring; unsafe/trusted function flags carried into HIR |
| **Typeck** | unsafe-context tracking (D3): ops and calls checked against the enclosing function's flags; D1 non-Copy `ptr<T>`; D2 op rules; D4 `PtrAcrossBoundary` (sibling check of `RefInAsync`); new errors: `UnsafeOpOutsideUnsafe`, `UnsafeCallOutsideUnsafe`, `CloneOfPointer`, `PtrAcrossBoundary` |
| **Lower** | `addr` → `AddrOf`; ptr read/write → `LoadPtr`/`StorePtr`; nothing async-specific (D4 is purely typeck) |
| **Codegen** | emit `0xC8–0xCA`; version **5** |
| **Verify** | `Δdepth` for the three new instructions; version gate accepts `{1..5}`; v<5 module with a 0xC8+ opcode → `BadOpcode` |
| **VM** | `Value::Ptr` + stable-cell promote-on-addr (§4); `LoadPtr`/`StorePtr` with defensive backstops |

---

## 9. Bytecode impact

See D5: `0xC8 AddrOf`, `0xC9 LoadPtr`, `0xCA StorePtr`; `.flnc` version 5,
readers accept `{1, 2, 3, 4, 5}`; **zero layout change**; `Value::Ptr` not
serialized.  v1–v4 modules stay executable on the 0.0.5 verify/VM.

---

## 10. Explicitly out of scope (0.0.5)

| Not doing | Reason |
|-----------|--------|
| `unsafe { }` block form | function-level gating only (D3); add later without breakage |
| Lambda form `unsafe f = () => { … }` | needs closures (0.1.0); DESIGN §12 draft superseded by D3 |
| Overloading `deref` for ptr | would re-define existing safe code paths; ptr read/write get their own spelling (T02) |
| ptr across channels/await, even in `unsafe` | D4; revisit with 0.1.0 cross-task machinery |
| Pointer arithmetic / `ptr<T>` to `ptr<U>` casts | no use case without FFI; FFI itself is unscheduled |
| Generics over unsafe (e.g. `UnsafeCell<T>`) | 0.1.0 with generics |
| Windows terminal-intrinsic parity work beyond CI coverage | T09 only *exercises* `read_line` etc. on Windows; fixes if broken are 0.0.5 patch material |
| wasm / FreeBSD targets | no users yet; matrix rows are additive |

---

## 11. Test matrix (planned)

```text
tests/
├── lexer/        ptr/addr/unsafe/trusted tokens        ptr_{valid,invalid}
├── parser/       ptr<T>, addr expr, unsafe/trusted modifiers   ptr_{valid,invalid}
├── typeck/       D1–D4 rules                           ptr_{valid,invalid}
│                 (CloneOfPointer, UnsafeOpOutsideUnsafe,
│                  UnsafeCallOutsideUnsafe, PtrAcrossBoundary)
├── e2e/
│   ├── valid/     addr_write.fln, trusted_wrapper.fln, unsafe_async.fln …
│   └── invalid/   clone_ptr.fln, unsafe_outside.fln, ptr_channel.fln …
└── projects/     flint std-addressing fixtures (D8 order + version-mismatch error)
```

Toolchain tests (T07–T09) are CLI/CI-level, not `.fln` fixtures: exit codes,
`--help`/`--version` output, `NO_COLOR` behavior, archive layout smoke test.

Compatibility fixtures: keep one fixed-byte v3 and one v4 `.flnc` asserting
the verify/VM still execute them (BYTECODE.md §10 promise).

---

## 12. Versioning

- `.flnc` `version` → **5** (three new opcodes; no layout change).
- Workspace version → **0.0.5** (T10), codename **Careful Hedgehog 🦔** in
  CHANGELOG/README.
- `ptr` / `addr` / `unsafe` / `trusted` become reserved words: 0.0.4 programs
  using them as identifiers need renaming (recorded as breaking in the
  CHANGELOG).
- Reader acceptance widens to `{1, 2, 3, 4, 5}`; older readers reject v5
  modules (`BadOpcode` on any 0xC8+).
