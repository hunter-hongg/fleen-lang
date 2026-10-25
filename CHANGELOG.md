# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and version semantics follow [SPEC.md](SPEC.md) §13 (`0.0.x` = early development, unstable API).

## 0.0.4 — Patient Ants (2026-10-25) 🐜

The release that lets tasks talk: cooperative `async` / `await` over `Future<T>` coroutine
frames, a single-threaded cooperative executor, and message-passing `channel<T>` — with an
ownership boundary (async is borrow-free) that keeps the 0.0.2 model intact without a borrow
checker. Planning and design decisions: [docs/0.0.4/PLAN.md](docs/0.0.4/PLAN.md)
(D1–D6); implementation tickets: [docs/0.0.4/TICKETS.md](docs/0.0.4/TICKETS.md) (T01–T10 all landed).

### Language Features

- **`async` functions + `await`** (from T02/T05): `async func f(...): T` returns `Future<T>`
  to the caller while the body still returns `T`; the future is a heap coroutine frame
  (D1 — locals + saved operand stack + resume PC) driven by the postfix operator
  `e await` (`Future<T>` → `T`), composable with `?` as `e await?`. `Future<T>` is non-Copy,
  movable, and driven once; `await` on a non-future is `AwaitNonFuture`
- **Cooperative executor** (from T06): a single-threaded round-robin scheduler in the VM.
  `std.async.run(tasks: [Future<unit>])` spawns and joins several roots (re-entrant inside
  async bodies); `std.async.yield_now()` is an explicit cooperative yield; `std.io.sleep(n)`
  is an awaitable timer (ready tasks always beat due timers, and timer wake order is
  deterministic). A top-level await in `main` boots a blocking drive, so a normal program
  "just works"
- **Channels** (from T07): `channel<T>(cap)` is a reserved-word constructor — `cap = 0` is a
  rendezvous channel (direct handoff), `cap > 0` a bounded buffer. `c.send(v)` / `c.recv()`
  suspend on full/empty; payloads travel by **ownership move** (Copy payloads copy). Channels
  are non-Copy and move-only (`clone ch` is rejected by `ChannelNotClonable`);
  `std.concurrent.split(ch)` is the **one explicit fork point** giving a second handle to the
  same buffer. There is no `close` / `select` / `for`-over-channel in 0.0.4 (deferred to
  0.1.0); a run where every remaining task is blocked on channels is a `ChannelDeadlock`
  runtime error
- **Static concurrency boundary** (from T03/T07, D2/D4/D5): an `async func` cannot take a
  `ref` parameter or move a borrow across any await/task/channel boundary (`RefInAsync`,
  `ChannelNotBorrowable`) — the entire `Send` rule is one line; async bodies may read globals
  (auto deep copy) but never write them (`AsyncGlobalWrite`); an owned variable handed to
  `send` needs an explicit `move`/`clone` (D6, `OwnedArgRequiresMove` with a channel branch)

### Toolchain

- **Bytecode v4** (T05/T07): async/channel instructions in the reserved 0xC0+ region —
  `AsyncStart` / `Suspend` / `Poll` / `Await` (0xC0–0xC3) and `ChannelNew` / `ChannelSend` /
  `ChannelRecv` / `ChannelSplit` (0xC4–0xC7); `Value::Future` / `Value::Channel` join the
  value model as **non-serialized** runtime variants; `.flnc` version writes `4`, readers
  accept `{1, 2, 3, 4}` with **zero layout changes** (no new table) — v1/v2/v3 `.flnc`
  fixtures still decode, verify and execute unchanged, and a v4 opcode in a v1–v3 module is
  rejected (`BadOpcode`). Specification: [docs/BYTECODE.md](docs/BYTECODE.md) §5.15
- **std additions**: `std.async` (`run` / `yield_now`) and `std.concurrent` (`split`) —
  compiler/VM primitives documented in `std/async.fln` / `std/concurrent.fln`;
  `std.io.sleep` joins `std.io`
- **Verify** gains the Δdepth table for the eight new instructions and the v4 opcode gate
- **Project demo**: `examples/0.0.4/tour` — a flint project exercising async fn, await,
  the executor, and both channel shapes end to end

### ⚠️ Breaking

- **`async`, `await`, and `channel` become reserved words**: 0.0.3 programs using them as
  identifiers must rename them. Nothing else breaks — `Future` stays a plain identifier
  (only the `Future<T>` type form is new), Copy/owned semantics are unchanged, and every
  0.0.3 program compiles as-is modulo these three renames
- `.flnc` `version` is now written as `4` (0.0.3 files with `version = 3` remain readable —
  the format has no layout change)

## 0.0.3 — Busy Beaver (2026-10-18) 🦫

The release that gives values shape: aggregation types (`struct`, `[T]`), the `for` loop,
the module system, the `std` library, and the `flint` package manager. Planning and design
decisions: [docs/0.0.3/PLAN.md](docs/0.0.3/PLAN.md); implementation tickets:
[docs/0.0.3/TICKETS.md](docs/0.0.3/TICKETS.md) (V01–V10 all landed).

### Language Features

- **`struct`** (from V02): struct declarations (`pub` optional), literals (all fields must be
  initialized), field access, and field assignment (one level, on a local/global struct);
  `is_copy(S)` = all fields Copy, otherwise the struct is an owned type (`move` / `clone`);
  recursive structs are rejected (`RecursiveStruct`), and `move p.f` is rejected (`MoveOutOfStruct`)
- **`[T]` arrays** (from V03): array literals, index read/write (`a[i]`), and `len` built-in;
  arrays are owned values with `move`/`clone` semantics for every element type; indexing is
  bounds-checked at runtime (out of bounds to `IndexOutOfRange`); empty literals need a type
  annotation (`CannotInferArrayType`)
- **`for` loop + `Range`** (from V04): `for x in iterable { … }` over arrays `[T]` and ranges
  (`start..end`, end-exclusive); the loop variable is a fresh immutable binding per iteration;
  the iterable is evaluated once; `Range` is a Copy value built by `std.collection.range` /
  `range_from`
- **Modules** (from V05): `import` resolves module paths to `.fln` files, top-level `pub`
  controls cross-module visibility, qualified paths (`net.tcp.make`) reach functions, globals,
  consts and structs, `--project <dir>` compiles whole source trees while single-file
  `cargo fln file.fln` remains unchanged; `FileId`/`SourceMap` give every diagnostic a
  `file:line:column`; import cycles (`CircularImport`), missing files (`ModuleNotFound`),
  name collisions (`DuplicateModule`) and cross-crate back-edges (`CrossCrateImport`) are
  hard errors
- **`std` library** (from V06): modules `io` / `str` / `num` / `arr` / `collection` / `os` / `time`.
  `print` / `println` / `read_line` (io), `len` / `concat` / `split` and the `from_*` cast helpers (str),
  `abs` / `min` / `max` / `sqrt` / `floor` (num), `len` / `get` (→ `Result`) / `set` / `push` over
  schematic `[T]` (arr), `args` / `exit` (os), and `now_millis` (time).
  The std root ships in `std/*.fln` and is embedded into the compiler; only VM-irreducible primitives
  are intrinsics, everything else is ordinary Fleen; the prelude stays `print` only (D7)
- **std iterator helpers** (from V07): eager `map` / `filter` / `fold` / `take` in
  `std.collection.iter` (arrays in, arrays out; lazy iterators wait for generics in 0.1.0)

### Toolchain

- **`flint` package manager** (from V08): new `flint-cli` workspace crate. `flint.toml`
  declares git-only dependencies pinned by a committed `Flint.lock`; `flint new / build /
  run / check` scaffold, compile (whole-program, dependency crates + std) and execute a
  project; `flint test` runs `tests/*.fln` against `.expected`. Git access shells out to the
  `git` CLI; cache lives under `$FLINT_HOME` (default `~/.flint`); dependency sources are
  read-only and their `pub` items are importable as `<crate>.<module>`.
- **Bytecode v3** (V02–V04): struct instructions (`StructNew` / `GetField` / `SetFieldLocal` /
  `SetFieldGlobal`, 0xB0–0xB3), array instructions (`ArrNew` / `ArrLen` / `ArrGet` /
  `ArrSetLocal` / `ArrSetGlobal`, 0xB4–0xB8), Range instructions (`MakeRange` / `RangeStart` /
  `RangeEnd`, 0xB9–0xBB), the appended `struct_defs` table, and the `Value::Struct` / `Value::Arr`
  / `Value::Range` value variants (specification: [docs/BYTECODE.md](docs/BYTECODE.md))
- **Compatibility gate** (from V09): committed v1 / v2 / v3 `.flnc` fixtures are decoded,
  verified and executed on the 0.0.3 verify/VM on every `cargo test`, every example under
  `examples/` is compiled·verified·run, and the whole §8 test matrix is green
- **Fixed**: constant-pool interning is now truly bit-exact — `Const`'s `PartialEq` compared
  floats with `f64::eq` (`0.0 == -0.0`) while its `Hash` used `to_bits()`, a contract
  violation that could collapse `0.0` and `-0.0` onto one pool entry depending on the map's
  random seed (a seed-dependent test flake made it observable)
- `IndexOutOfRange` runtime error (with source span)
- Verify gains the `StructNew`/`ArrNew` operand-derived arity checks and the v3-opcode gate
  (a v3 opcode in a v1/v2 module is rejected)

### ⚠️ Breaking

- **`struct`, `for`, `in`, and `pub` become reserved words** (0.0.1/0.0.2 programs using them
  as identifiers must rename them; nothing else breaks — the prelude (`print`) is unchanged,
  explicit semicolons stay legal, and all 0.0.2 source programs compile as-is modulo these four)
- v3 opcode renumbering within the (previously reserved) 0xB0+ space only: `ArrSetGlobal` =
  0xB8 and `MakeRange` moved from 0xB8 to 0xB9. **No existing instruction changed its number or
  encoding** (BYTECODE.md §10's rule): every 0.0.1/0.0.2 instruction keeps its opcode, and v1
  / v2 `.flnc` files still execute unchanged

## 0.0.2 — Hermit Crab (2026-10-11) 🐚

The beginning of ownership: heap values get owners, errors start to flow, and semicolons start to retire.
Feature planning and design decisions: [docs/0.0.2/PLAN.md](docs/0.0.2/PLAN.md).

### ✨ Language Features

- **Ownership system**: `string` / `box<T>` have a single owner; `move` transfers explicitly and
  `clone` deep-copies; Copy types (`int` / `float` / `bool` / `unit` / function values)
  are unaffected; use of a moved value is rejected at compile time (`UseAfterMove`,
  `MaybeMovedAfterBranch`, etc., including loop back-edges)
- **`box<T>`**: `box expr` heap-allocates, `deref b` reads/writes the value inside the box,
  with deterministic deallocation (no GC); the set of allowed pointee types:
  `int` / `float` / `bool` / `string` / `box<U>` /
  `Result<T, E>`; values cannot be moved out of a box (`move deref b` is a compile error —
  use `clone deref b` if you need the value); in `deref b = v`, `b` must be a local box
  (writing through a global box reports `DerefAssignOfGlobalBox`)
- **`ref T`**: read-only borrowing, in function parameter position only — handles cannot escape,
  so no borrow checker is needed; the argument must be a variable; global arguments are
  copied via a temporary slot (`CloneGlobal` + temporary slot +
  `MakeRefLocal`); global initializers do not support ref arguments (`RefArgInGlobalInit`)
- **Error handling**: `Ok(v)` / `Err(e)` constructors, `Ok`/`Err` patterns in `choose`
  (both arms together are exhaustive; guarded arms do not count toward exhaustiveness),
  and the `?` propagation operator; the entry function
  (`main`) may return a `Result` — a trailing `Err` prints its payload to stderr and exits
  with code 1 (the exit code contract is unchanged)
- **ASI**: semicolons are optional; explicit semicolons remain legal; line-break sensitivity
  (joining statements on one line is still illegal); for cross-line continuation rules see
  [docs/0.0.2/ASI.md](docs/0.0.2/ASI.md)
- **`as` type casts** (temporary in 0.0.2, to be revisited after generics): the whitelist covers
  only `int` / `float` / `bool` → `string`; anything else (including identity casts) reports
  `UnsupportedCast { from, to }`; the result is a fresh owned string, sharing the same
  formatting helper as print (`fleen-vm/src/fmt.rs`)

### 🛠 Toolchain

- Bytecode v2: ownership instructions (`AllocBox` / `DerefBox` / `StoreDerefBox` /
  `MakeRefLocal` 0x80–0x83; `DupDeep` / `MoveLocal` / `CloneLocal` /
  `CloneGlobal` 0x90–0x93), Result instructions (0xA0–0xA4), and `ToStr` (0xA5);
  v2 modules can execute v1 instruction streams, while v2 instructions are forbidden in version=1 modules
  (specification: [docs/BYTECODE.md](docs/BYTECODE.md))
- `Value::Str(Rc<str>)` → `Str(Box<str>)` (exclusive ownership, paying off a temporary decision from 0.0.1)
- `span_map` promoted to a first-class component + runtime errors carry source line/column
  (`.fln` mode); errors propagated through a call chain point to the call site
- `print` gets its real built-in signature (variadic string; non-string arguments report
  `ArgTypeMismatch` with a hint to use `as string`); the typeck special case is removed
  (a known Hack from 0.0.1 — the known-Hacks list in `DESIGN.md` §18 is now empty)
- Implementation-level behavior details: `deref b = v` on a local box lowers to
  `v; AllocBox; StoreLocal b` (pointee replacement and whole-box replacement are observationally
  equivalent, so the compiler no longer emits `StoreDerefBox`); choose-Result lowers to an
  `IsErr` branch chain (a Result-choose with guarded arms or `otherwise` is rejected by typeck);
  `CloneGlobal` does not dereference handles
  (typeck forbids handles from entering globals)
- Security fixes: the `.flnc` decoder no longer preallocates unboundedly for malicious table
  lengths (since 0.0.1 an oversized length would abort the process; it is now downgraded to a
  decode error); verify rejects modules with empty function bodies
  (since 0.0.1 this would panic; it now reports `EmptyFunctionBody`)

### ⚠️ Breaking

- Assigning/passing owned values (`string` / `box<T>`) requires an explicit `move` or `clone`
  (in 0.0.1, `t = s` was a shallow shared copy; it is now a compile error)
- `move` / `clone` / `deref` / `as` become reserved words (in 0.0.1 they could be used as identifiers)
- `print(function value)`, allowed in 0.0.1, is now a compile error

## 0.0.1 — Swift Fox (2026-10-06) 🦊

The first milestone release: a **complete toolchain** from source to bytecode to execution.
An expression-oriented small language with a five-stage compiler, a static bytecode
verifier, and a stack-based virtual machine.

### Language at a Glance

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0;
    const limit = 10;

    while x < limit {
        print(fib(x));
        x = x + 1;
    }

    0
}
```

```console
$ fleen-vm fib.fln
0
1
1
2
3
5
8
13
21
34
```

### ✨ Language Features

**Variables and Immutability**

- `x = 42` for mutable bindings and assignment; `const y = 42` is immutable —
  reassignment is rejected at compile time
- Optional type annotations: `x: int = 42`

**Type System**

- Basic types: `int` (64-bit) / `float` / `bool` / `string` / `unit`
- The `Result<T, U>` type
- **Functions are first-class citizens**, and function types can be annotated directly:
  `func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)`

**Control Flow Is Expression-Oriented** — every branch produces a value

- `if / elif / else` expressions
- `while` loops
- `choose` pattern matching: literal patterns, guarded patterns like `when x if x > 10`,
  and an `otherwise` fallback; non-exhaustive matching is a compile-time error

**Diagnostics Experience**

- All compile-time errors carry a source location (Span), and messages are actionable
- Runtime errors are clearly categorized: division by zero, integer overflow
  (checked arithmetic — no silent wraparound), and exceeding the recursion depth limit (1024)

**Deliberate Trade-offs in 0.0.1**: semicolons `;` must be written explicitly
(automatic semicolon insertion comes in a later version);
`?`, `struct`, `for`, generics, FFI, etc. are not yet included — see `SPEC.md` §15 for the full scope.

### 🛠 Toolchain

```
fib.fln ─► Lex ─► Parse ─► Resolve ─► Typeck ─► Lower ─► Codegen ─► fib.flnc
                                                                     │
                                       fleen-verify static checks ◄─┘
                                                                     │
                                                          fleen-vm executes
```

| Component | Responsibility |
|------|------|
| `fleen-compiler` | Five-stage compiler front end, emitting versioned `.flnc` bytecode modules |
| `fleen-verify` | A static safety net before execution: all-path stack depth analysis, jump targets must land on full instruction boundaries, range checks on constant/function/global indices, `const` globals writable only by the module initializer, no fall-off-end |
| `fleen-vm` | A stack-based virtual machine: call frames and local slots, first-class function calls, host built-ins; bytecode is **never executed without verification** |

A single-command entry point is provided in the repository:

```bash
cargo fln fib.fln     # compile → verify → execute, all in one step
cargo fln fib.flnc    # you can also run bytecode directly
```

Exit code contract: `0` success / `1` compile·verify·runtime error / `2` usage·IO error — scripts can rely on it.

### 🧪 Quality Assurance

- **321 tests**: unit tests + integration tests driven by per-stage valid/invalid fixture files +
  end-to-end tests (driving the real binaries and comparing stdout, exit codes, and stderr)
- E2E coverage of error paths: lexer/parser/name-resolution/type errors, `const` reassignment,
  division by zero, integer overflow, infinite recursion
- A four-piece quality gate: `cargo fmt --check` · `cargo clippy -D warnings` ·
  `cargo test` · `cargo doc` (configured in GitHub Actions)

### 📦 Installation

Build from source (requires Rust 1.85+, 2024 edition):

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang
cargo build --release
./target/release/fleen-vm examples/0.0.1/hello.fln
```

Prebuilt binaries: the release assets include a linux x86_64 archive
(containing the `fleen-compiler` / `fleen-verify` / `fleen-vm` executables) —
just unpack and run: `./fleen-vm fib.fln`.

### 🔜 What's Next

Following the design in `docs/DESIGN.md`: automatic semicolon insertion (ASI), `struct`, `for`,
generics, the `?` operator, and more will land in subsequent releases; see
`docs/BYTECODE.md` for the bytecode format specification and `docs/0.0.1/TICKETS.md` for the roadmap.
