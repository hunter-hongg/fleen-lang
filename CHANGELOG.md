## [0.0.7] - Unreleased

### Added
- **Parameterized enums** (D1): an `enum` head may declare type parameters —
  `enum Option<T> { Some(T), None }`, `enum Pair<A, B> { P(A, B) }`. The
  parameters live in a declaration-local scope and every use site must be
  **fully applied** (`Option<int>`; a bare `Option` in type position is
  `TypeConstructorNeedsArgs`, too many args is `ArityMismatch`). Both use-site
  spellings are accepted: angle (`Option<int>`) and bracket (`Pair[int, string]`,
  the builtin `Result[T, E]` form — D9). Constructor sites infer the parameters
  from the payload arguments; a zero-payload variant fixes none, so
  `Option.None` needs a context annotation (`TypeParameterAmbiguous` otherwise).
  Type parameters ride through resolve (module export/import tables carry the
  arity) and the `RecursiveEnum` ban carries forward unchanged
  (`enum List<T> { Cons(T, List<T>) }` is rejected, D3).
- **Argument-dependent `is_copy`** (D2): `is_copy(App(T1..Tn))` substitutes the
  arguments into the payload types and re-evaluates the payload-Copy rule —
  `Option<int>` is Copy, `Option<string>` is owned, `Pair[int, string]` is
  owned. Monomorphic enums keep the 0.0.6 constant rule as the arity-0 case.
- **Pattern-matching completion** (D4–D7): the pattern grammar is recursive.
  Multi-payload destructuring — `when P(a, b)` binds every payload, strictly
  (`ArityMismatchInPattern` on a count mismatch, D4). Nested patterns —
  `Ok(Some(x))`, `Some(P(a, b))`; bindings from all levels land in the arm
  scope (D5). Or-patterns — `p₁ | p₂` matches either alternative; all
  alternatives must bind the same names with the same types
  (`OrPatternBindingMismatch`), and exhaustiveness takes the union of the
  alternatives' coverage (D6). Type guards — `pattern: T` ascribes a type to
  the match; compile-time only, no runtime test (`PatternTypeMismatch` on a
  mismatch, D7). Lowering chains nested payloads with re-parked subjects and
  lowers one test chain per or-alternative into a shared body block; typeck's
  exhaustiveness is payload-aware (a literal sub-pattern never covers its whole
  payload type).
- **Bytecode v7** (D8): `enum_defs` gains one `type_param_count: u32` per enum
  (0 for a monomorphic enum), written after `variant_count`; parameter names
  are not serialized (declaration-local, no runtime meaning). Writers emit
  version 7, readers accept `{1, 2, 3, 4, 5, 6, 7}`; no new opcodes, no
  `FieldTypeTag` change, `Value::Enum` unchanged — the VM needs no source
  changes (the run gate widened to `1..=7`).
- E2E fixtures: valid `option_demo` / `pair_demo` / `nested_pattern` /
  `or_pattern` / `type_guard`; invalid `option_needs_args` / `option_arity` /
  `recursive_param_enum` / `or_pattern_binding_mismatch` / `type_guard_mismatch`.
- `examples/0.0.7/` — the single-file `option_demo.fln` (T09) and the
  multi-module `tour/` project (parameterized enums, nested / or / guarded
  patterns) with `tests/enum_param_case.{fln,expected}` (T10).

### Non-breaking
- **This release is additive** (D10) — a first for the 0.0.x line. The only
  source-visible changes are new syntax (enum type parameters, the extended
  pattern grammar); no existing program changes meaning, and `is_copy` only
  becomes argument-dependent for enums that have parameters (none exist before
  0.0.7). No deprecation aliases, no opt-out flags.
- **`Result[T, E]` stays builtin** (D9): the `Result` → std enum migration is
  deferred to 0.1.0; 0.0.7 carries the two parameterized spellings (builtin
  bracket `Result[T, E]`, user-defined angle `Option<T>` — both accepted for
  user enums too) as recorded 0.0.x debt.



### Added
- Tagged unions: `enum` declarations with ordered variants and typed payloads
  (zero- and multi-payload), variant constructors (`Color.Red`, the dotted form
  `EnumName.Variant`), and generalized `choose` exhaustiveness (the `Result` rule
  applied to enums — one arm per variant, or an `otherwise` arm). 0.0.6 patterns
  bind the **first payload only**; a multi-payload variant can be constructed but
  not fully destructured.
- `enum` is a reserved word (the 0.0.6 breaking change, D6).
- Type rules (D1/D2): `is_copy(E)` = all payloads of all variants are Copy (the
  same rule as a struct); owned enums are consumed by `choose` (`choose move s`).
  Recursive enums (directly or through an array/payload) are rejected at typeck
  (`RecursiveEnum`). Enums are not comparable (`==` is a type error) and the
  0.0.2 `as string` cast whitelist does not include them.
- Bytecode v6 (writers emit version 6, readers accept `{1, 2, 3, 4, 5, 6}`): the
  enum instructions `EnumNew` (0xCB, u16 enum id + u16 variant id, `p1 … pn → e`),
  `EnumIs` (0xCC, u16 variant ref, `e → bool`), `EnumPayload` (0xCD, u16 variant
  ref + u16 payload index, `e → p`); the `Value::Enum { variant, payload }`
  runtime value (not serialized); the appended `enum_defs` table (after
  `struct_defs`) and `FieldTypeTag::Enum = 12` (D4). `enum_defs` is written only
  for `version >= 6`, so the v1–v5 layout is byte-identical.
- Verify cross-checks `EnumNew` ids/payload arity (`EnumIndexOutOfRange`), gates
  the v6 opcodes (0xCB–0xCD) out of v1–v5 modules, and tracks their stack effects;
  the VM re-checks ids at runtime (`EnumIndexOutOfRange` / `EnumVariantOutOfRange`
  / `EnumPayloadMismatch`).
- Array library completion (D5): `std.arr.concat` and `std.arr.flatten` intrinsics;
  `std.collection.iter.fold` renamed to `foldl` and `foldr` added (the same eager
  builtin-backed helpers as `map`/`filter`/`foldl`).
- E2E valid fixture `tests/e2e/valid/enum_demo.fln` (+ `.expected`); invalid
  fixtures `enum_choose_not_exhaustive`, `enum_recursive`, `enum_cast`, `enum_eq`
  (each `.fln` + `.stderr` + `.exit`).
- `examples/0.0.6/tour` — a multi-module project (enums, pattern matching, and
  `concat`/`flatten`/`foldl`/`foldr`) with `tests/enum_case.{fln,expected}` and
  `examples/0.0.6/README.md`.
- `release/0.0.6/RELEASE_NOTES.md`.

### Changed
- Workspace and std `VERSION` bumped to `0.0.6`; bytecode version is 6 (readers
  accept `{1, 2, 3, 4, 5, 6}`).
- Synced `docs/DESIGN.md` (composite-types enum entry + §9.8, §17 matrix and 0.0.6
  goal, §18 roadmap), `docs/BYTECODE.md` (v0.0.6 header, §2/§3.2 enum types, §4.2
  0xCB–0xCD, §5.17, §6.11, §8 verify + stack depths, §9 `enum_defs`/version 6, §10
  version table), `docs/SYNTAX.ebnf` (v0.0.6, `enum_decl`/`enum_variant`/
  `enum_pattern`), `SPEC.md` / `AGENTS.md` / `CLAUDE.md` (0.0.6 goal, §14 enum
  block, §15 matrix + 0.0.6 status), and `docs/0.0.6/TICKETS.md` (T08 complete).

### Breaking
- `enum` is a reserved word (D6) — an identifier named `enum` is now a parse error.
- `std.collection.iter.fold` is renamed to `foldl` and `foldr` is added (D5).

# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com/),
and version semantics follow [SPEC.md](SPEC.md) §13 (`0.0.x` = early development, unstable API).

## 0.0.5 — Careful Hedgehog (2026-11-02) 🦔

The release that makes raw pointers explicit and matures the toolchain: `unsafe` / `trusted`
function modifiers gate a new raw-pointer layer (`ptr<T>` / `addr` / `deref_ptr`), pointers
never cross the async world, and the toolchain gets a standardized CLI, a six-target
release pipeline, and a version-pinned std distribution. Planning and design decisions:
[docs/0.0.5/PLAN.md](docs/0.0.5/PLAN.md) (D1–D8); implementation tickets:
[docs/0.0.5/TICKETS.md](docs/0.0.5/TICKETS.md) (T01–T10 all landed).

### Language Features

- **Raw pointers** (T01/T02/T05/T06, D1/D2): `ptr<T>` is a type in constructor position,
  **non-Copy and move-only** (`clone p` → `CloneOfPointer`) and **non-storable**
  (`PtrNotStorable` — no struct fields, array elements, box cells, or globals); `addr x`
  takes the address of a place (local, global, struct field, array element, box interior;
  immutable bases and `ref`-parameter bases are rejected); `deref_ptr p` reads/writes
  through the pointer (the settled T02 spelling — `deref` itself stays box-only). Every
  pointer use is an **unsafe operation** (D2): creating one and using one both require an
  `unsafe`/`trusted` body, because Fleen has no borrow checker and safe code could move the
  referent out from under a pointer.
- **`unsafe` / `trusted` function modifiers** (T03, D3): `unsafe func` bodies may contain
  unsafe ops and are callable only from unsafe/trusted bodies (`UnsafeCallOutsideUnsafe`);
  `trusted func` bodies may also contain them but are callable from anywhere (the author
  attests). They compose with `async` in either order (D4). The rule is lexical, mirroring
  0.0.4's `RefInAsync` — no call-graph analysis.
- **Pointer boundary rule** (T04, D4): a `ptr<T>` may not be captured across an `await`,
  moved into a task, or sent through a channel — `PtrAcrossBoundary`, the sibling of
  `RefInAsync`; `async unsafe func` / `unsafe async func` parse and gate the same way.

### Toolchain

- **Bytecode v5** (T05/T06): `AddrOf` / `LoadPtr` / `StorePtr` (0xC8–0xCA) in the reserved
  expansion band; `Value::Ptr` holds a stable-cell handle (promote-on-addr) and is **not
  serialized** — v1–v4 `.flnc` fixtures still decode, verify and execute unchanged, and a
  v5 opcode in a v1–v4 module is rejected (`V5OpcodeInV1V2V3V4Module`). `.flnc` version
  writes `5`, readers accept `{1, 2, 3, 4, 5}`. Specification: [docs/BYTECODE.md](docs/BYTECODE.md)
  §5.16
- **CLI standard** (T07, D6): `clap` (derive) for `fleen-vm`, `flint`, and `fleen-verify`;
  uniform `--project` / `-h/--help` / `-V/--version` / `--quiet` / `--no-color`; formal
  exit codes (0 success, 1 compile/resolve/verify/runtime failure, 2 usage/IO);
  rustc-style colored span diagnostics (plain ANSI, `NO_COLOR` honored)
- **Release pipeline** (T09, D7): `.github/workflows/release.yml` triggers on `v*` tags and
  builds six targets — x86_64/aarch64 Linux (musl static included), x86_64/aarch64 macOS,
  x86_64 Windows — into flat archives (four binaries + `std/` + README; `.tar.gz` for
  Linux/macOS, `.zip` for Windows); the Windows job runs an explicit `std.io.read_line`
  smoke test. No install script — README documents manual install
- **std distribution** (T08, D8): one shared std-resolution order — `FLINT_STD_PATH` →
  `$FLINT_HOME/std/<compiler-version>/` → embedded — with a strict version check (a mismatch
  is a hard error naming both versions, never a silent fallback); `flint check --std`
  reports the effective source
- **Docs/status**: T10 synced the design spec (DESIGN §10.7/§12/§17/§18), the EBNF grammar,
  the SPEC quick reference and feature table, and the README; workspace and `std/VERSION`
  bump to 0.0.5. Release publication (tag/archives) is the separate step.

### ⚠️ Breaking

- **`ptr`, `addr`, `deref_ptr`, `unsafe`, and `trusted` become reserved words**: 0.0.4
  programs using them as identifiers must rename them. Nothing else breaks — `deref`
  remains box-only (the pointer spelling is additive, so existing safe call sites are
  untouched), and every prior `.flnc` fixture still verifies and executes.
- **Known limit (documented, T03)**: an `unsafe func` called *through a function value*
  escapes the static call check (first-class functions); the VM's `LoadPtr` / `StorePtr`
  backstop is the last line of defense.

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
