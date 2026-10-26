# 0.0.5 — `unsafe` / `trusted` / `ptr` / `addr` + Toolchain Maturity (Tickets)

> Companion to `docs/0.0.5/PLAN.md`.  One card = one unit of work, closed with
> tests.  Decisions D1–D8 referenced here are **settled at planning level**
> (see PLAN §1); a card may reopen one only by adding a new decision entry,
> not by editing it silently.
> **Status: T01, T02 implemented and green (2026-10-25, uncommitted at
> implementation time); T03+ PLANNED.**

---

## T01 — Keywords + lexer (F9)

- [ ] Reserve `ptr`, `addr`, `unsafe`, `trusted` in the lexer keyword table
- [ ] `TokenKind::{Ptr, Addr, Unsafe, Trusted}` wired through `is_keyword` /
      `keyword_str` / `Display`
- [ ] Reserved-word enforcement follows the 0.0.4 T01 path: misuse surfaces at
      the existing statement/expr entry guards — no new `ReservedWord` kind
- [ ] Tests: lexer unit tests (keyword tokens, metadata, prefix-safety) +
      `tests/lexer/valid/ptr_keywords.fln` (a *valid* file only — reserving a
      word never fails tokenization)

**Depends on**: nothing.
**Refs**: PLAN §3.1.
**Note**: `ptr` and `addr` are non-expression-starting / prefix keywords
respectively — `addr` joins the unary prefix group (`box deref move clone addr
- !` in the precedence table) at parse time (T02), not in the lexer.

---

## T02 — Parser + type system for `ptr<T>` / `addr` (F1/F2; D1/D2)

- [x] AST: `ptr<T>` in `parse_type` (constructor position, like `Future<T>` /
      `channel<T>`); `ExprAddr` prefix expr in the unary group
- [x] Settle the **write-through-ptr spelling** (the one open syntax question —
      PLAN §3.2): settled on the builtin `deref_ptr` prefix keyword (read and
      write share the spelling, mirroring `deref b = v`); folded back into
      PLAN §3.2 together with the addr-of-immutable / addr-of-ref-param
      rejections and the non-storable (`PtrNotStorable`) family entry
- [x] Type rules: `addr place : ptr<T>`; read/write through ptr typed `T`
- [x] `ptr<T>` non-Copy (D1); `clone p` → `CloneOfPointer`
- [x] `Func` gains `is_unsafe` / `is_trusted` (parse-level, consumed by T03);
      `unsafe`/`trusted` compose with `async` in both orders (one AST state)
- [x] ASI: adding `addr` to the unary prefix group requires adding
      `TokenKind::Addr` to `can_start_stmt` (parser/asi.rs, alongside
      `box`/`deref`/`move`/`clone`) and updating the
      `can_start_stmt_table` test — done, together with `DerefPtr` and the
      now declaration-starting `Unsafe`/`Trusted`
- [x] Tests: `tests/parser` + `tests/typeck` ptr suites (plus resolver
      `addr`-place fixtures and parser/typeck unit tests); lowering stubs
      reject ptr programs with `UnsupportedFeature` at the correct phase
      (opcodes land in T05)
- [ ] Update DESIGN §10 (`ptr` / `addr` lines) **deferred to T10** — record
      here, don't edit early

**Depends on**: T01.
**Refs**: PLAN §3.2, §3.3, D1, D2.
**Note (L2)**: `deref` itself stays box-only — overloading it in 0.0.5 would
silently re-type existing safe call sites that might one day receive a ptr.
The new spelling is additive, so nothing existing changes.

---

## T03 — Unsafe-context gating (F3; D3)

- [ ] typeck tracks the enclosing function's `unsafe` / `trusted` flags down
      the body walk (nested `func` declarations carry their own flags, not the
      parent's)
- [ ] Unsafe ops (`addr`, ptr read, ptr write) outside an unsafe/trusted body
      → `UnsafeOpOutsideUnsafe`
- [ ] Call to an `unsafe func` from a plain `func` → `UnsafeCallOutsideUnsafe`;
      calls from `unsafe`/`trusted` bodies pass
- [ ] `trusted` bodies may contain unsafe ops (that is their entire purpose —
      the author attests); no runtime distinction between unsafe and trusted
- [ ] Tests: `tests/typeck/invalid/unsafe_op_outside.fln`,
      `unsafe_call_outside.fln`; valid: `trusted_wrapper.fln`

**Depends on**: T02.
**Refs**: PLAN §3.2 table, D3.
**Note**: the rule is lexical, mirroring 0.0.4 T03's `RefInAsync` approach —
no call-graph analysis.  An `unsafe func` called through a function *value*
(first-class functions!) escapes the static call check by the same
known-and-documented limit as 0.0.4 T08c; the VM `LoadPtr`/`StorePtr` backstop
(T06) is the last line.  This limit must be named in the T10 doc sync, not
discovered by a user.

---

## T04 — Boundary rules + `unsafe async` (F4; D4)

- [ ] typeck: a `ptr<T>` value captured across an `await`, moved into a task,
      or sent through a channel → `PtrAcrossBoundary` (sibling of
      `RefInAsync`; same lexical rule shape)
- [ ] `async unsafe func` / `unsafe async func` parse accepted (modifier
      order: settle at implementation, both spellings → one AST state)
- [ ] Calling (and awaiting) an unsafe async fn requires an unsafe/trusted
      context, same as sync unsafe fns
- [ ] Tests: `tests/typeck/invalid/ptr_across_await.fln`,
      `ptr_channel.fln`, `ptr_task.fln`; valid: `unsafe_async.fln`

**Depends on**: T03 (context tracking), 0.0.4 T03 (the `RefInAsync` walk it
extends).
**Refs**: PLAN D4.
**Note (L2)**: this is what keeps 0.0.4's "one static Send rule" true — frames
are relocated by the scheduler (0.0.4 T04: location-independent), so a ptr
whose referent was promoted from frame storage must not outlive the frame's
locality.  The alternative (allow ptr in unsafe code across boundaries) would
require the promote-on-addr cells to be globally reachable — a VM-layer
rewrite deferred to 0.1.0's cross-task heap, not a smooth swap.

---

## T05 — Lowering + opcodes + verify (F5; D5)

- [ ] Lower `addr place` → `AddrOf` (0xC8); ptr read → `LoadPtr` (0xC9);
      ptr write → `StorePtr` (0xCA)
- [ ] Codegen: `.flnc` version **5**; verify version gate `{1, 2, 3, 4, 5}`;
      `Δdepth` for the three new instructions; v<5 module with a 0xC8+ opcode
      → `BadOpcode`
- [ ] BYTECODE.md edits (§4.2 band label "0.0.4 expansion space" → note the
      0xC8–0xCA occupancy; §5.x instruction specs; §10 version history) —
      **in this ticket**, not T10, since BYTECODE.md is the authoritative
      encoding doc (LESSON L3)
- [ ] Tests: verify unit tests (depth, version gate); fixed-byte v3/v4
      fixtures still execute (compat promise)

**Depends on**: T02 (AST), T03/T04 (gating must exist so only unsafe code can
reach the opcodes).
**Refs**: PLAN D5, §9.

---

## T06 — VM: pointer representation (F6)

- [ ] `Value::Ptr` variant holding a stable-cell handle — **not serialized**
      (same rule as `Future`/`Channel`; keep the `size_of::<Value>()` test
      passing)
- [ ] `AddrOf` promotes the referent slot to a stable heap cell
      (promote-on-addr sketch, PLAN §4); local reads/writes before and after
      promotion are semantically unchanged
- [ ] `LoadPtr` / `StorePtr` execute through the cell; malformed handle →
      defensive `RuntimeError` backstop
- [ ] Settle the representation among: promote-on-addr (default) vs. global
      arena vs. eager boxing — record the choice + rejected alternatives here
- [ ] Tests: `fleen-vm/src/tests.rs` — addr/load/store round-trip, promotion
      transparency, backstop on a corrupted handle; e2e `addr_write.fln`

**Depends on**: T05.
**Refs**: PLAN §4, D2, D5.
**Note (L2)**: the promote-on-addr cost is one allocation per address-taking
place, paid only by code that takes addresses; the global-arena alternative
would touch every load/store path — the reason it is the rejected default.
`Value::Ptr` must keep frames movable (suspend relocates frame regions) — a
test asserting suspend/resume across an addr-promoted local is mandatory.

---

## T07 — CLI overhaul (F7; D6)

- [ ] `clap` (derive) adopted for `fleen-vm`, `flint`, `fleen-verify`
- [ ] Uniform flags: `--project <dir>`, `-h/--help`, `-V/--version`,
      `--quiet`, `--no-color`; program args after `--` for `fleen-vm`/`flint run`
- [ ] `flint` subcommands get per-command help text; exit codes 0/1/2
      documented in help and covered by tests
- [ ] Shared span-diagnostic pretty-printer module: `file:line:col` header,
      source line, `^^^^` underline, indented `note:`/`suggestion:` lines
- [ ] Color via plain ANSI; disabled by `--no-color` or set `NO_COLOR`;
      `--quiet` suppresses non-error output
- [ ] Tests: CLI-level (exit codes, help/version text, NO_COLOR) — not `.fln`
      fixtures

**Depends on**: nothing (parallel-safe with T01–T06).
**Refs**: PLAN §5, D6.
**Note (L2)**: clap becomes a workspace-wide dependency (three crates) —
accepted because three hand-rolled parsers were the alternative growth path,
not because hand-rolling is impossible.  If the dependency is unwanted, the
alternative to reject is *this ticket*, not to silently hand-roll.

---

## T08 — std distribution + flint addressing (F10; D8)

- [ ] One shared std-resolution module: `FLINT_STD_PATH` →
      `$FLINT_HOME/std/<compiler-version>/` → embedded copy (order per D8)
- [ ] Strict version check on an external std: mismatch = hard error naming
      both versions (never silent fallback)
- [ ] `flint check --std`: reports which copy is in effect + its version
- [ ] Release-package layout fixed: archives carry `std/` at a documented
      path; install step copies it to `$FLINT_HOME/std/<version>/` (the copy
      step itself lands with T09's packaging)
- [ ] Embedded copy stays the default and the `cargo test` surface (0.0.3 D8
      unchanged)
- [ ] Tests: `tests/projects/` fixtures — resolution order, env override,
      version-mismatch error, embedded fallback

**Depends on**: nothing for the resolution module; archive layout contract
shared with T09.
**Refs**: PLAN §6, D8.

---

## T09 — CI/CD release pipeline (F8; D7)

- [ ] `ci.yml` `check` job gains an OS matrix (ubuntu / macos / windows) —
      compile + test on all three
- [ ] Release job on `v*` tag: build the six targets (D7 table), run full
      `cargo test` on the native target first, then package
- [ ] Archive layout per T08's contract: binaries + `std/` + README;
      `.tar.gz` everywhere, `.zip` for Windows; musl target builds static
- [ ] Upload to the GitHub Release (precedent: `release/0.0.x` branch,
      `v0.0.x` tag, `gh release`)
- [ ] Windows job actually exercises `std.io.read_line` / terminal intrinsics
      (e2e run, not just compile) — known-risk item (PLAN D7)
- [ ] Deferred rows (wasm, FreeBSD) documented as intentionally absent (PLAN §10)

**Depends on**: T08 (std archive layout).
**Refs**: PLAN §7, D7.

---

## T10 — Integration matrix + docs sync + version bump (F11)

- [ ] Full §11 matrix green; `cargo fmt` / `clippy -D warnings` / `test` /
      `doc` clean
- [ ] Docs sync: DESIGN §10 (ptr/addr lines) and §12 (lambda draft → D3
      modifier form); §13 roadmap + §18 roadmap rows; SPEC §14 quick reference
      + §15 feature table; SYNTAX.ebnf; README version line
      (**0.0.5 · Careful Hedgehog 🦔**); CHANGELOG entry
- [ ] CHANGELOG records the breaking change: `ptr`/`addr`/`unsafe`/`trusted`
      now reserved words; DESIGN §12's superseded lambda sketch explicitly
      marked as superseded by D3
- [ ] Document the known limit from T03 (unsafe fn through a function value
      escapes the static call check; VM backstop is the last line)
- [ ] Workspace version → 0.0.5; `.flnc` writer version 5, readers `{1..5}`
- [ ] Release publication (tag/archives) is **separate** — T09 provides the
      pipeline; cutting the actual release follows the 0.0.4 precedent and is
      not part of this ticket

**Depends on**: T01–T09.
**Refs**: PLAN §11, §12.

---

## Dependency graph

```text
T01 ── T02 ── T03 ── T04 ── T05 ── T06
                                    │
T07 (parallel-safe)                 │
T08 ────────── T09                  │
              └──────── T10 ────────┘
```

Critical path: T01 → T06 (the language layer).  T07 is fully parallel; T08 can
start immediately with only its resolution-module half; T09 needs T08's
archive contract; T10 closes the milestone.
