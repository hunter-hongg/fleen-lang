# 0.0.6 — `enum` + Array Library Completion (Tickets)

> Companion to `docs/0.0.6/PLAN.md`.  One card = one unit of work, closed with
> tests.  Decisions D1–D6 referenced here are **settled at planning level**
> (see PLAN §1); a card may reopen one only by adding a new decision entry,
> not by editing it silently.
> **Status: T01 implemented 2026-11-04; T02–T08 pending.**

---

## T01 — Keywords + lexer (F1; D6)

- [x] Reserve `enum` in the lexer keyword table
- [x] `TokenKind::Enum` wired through `is_keyword` / `keyword_str` / `Display`
- [x] Reserved-word enforcement: **as-built deviation from the card text** —
      misuse does not stop at the statement/expr entry guards. `enum` joins
      `can_start_stmt` immediately (unlike the 0.0.5 words, which waited for
      their T02) and gets an `Enum` branch in `Parser::reject_keyword_binding_name`
      (the 0.0.5 T02 pattern), so `enum = 1;` reports
      `keyword \`enum\` cannot be used as identifier` instead of the ASI
      statement-separator error (`expected \`;\`, … found enum`), which never
      mentions reservation. `enum S { A }` still surfaces at the
      primary-expression fallback (`expected expression, found enum`) —
      declaration syntax itself lands in T02.
- [x] Tests: lexer unit tests (keyword token, metadata incl. the load-bearing
      `to_string()` assertion — `Display` renders keywords via `keyword_str`, so
      a missing arm there is an `unreachable!` panic on user-error paths and the
      catch-all `other` arm makes the omission compile-invisible — and
      prefix-safety: `enums` / `EnumX` / `enum_id` / `enumx` / `Enum` stay
      identifiers) + parser tests (`reject_enum_as_binding_name`,
      `enum_decl_not_yet_parseable`) + `tests/lexer/valid/enum_keywords.fln`
      (a *valid* file only — reserving a word never fails tokenization)
- [x] ASI bookkeeping: `TokenKind::Enum` added to the `can_start_stmt` start
      set, the `can_start_stmt_table` yes-list, and `implies_stmt_end_table`.
      Also fixed a pre-existing gap while there: `TokenKind::Struct` had been
      missing from both ASI test tables (`can_start_stmt_table` yes-list,
      `implies_stmt_end_table`) since 0.0.3 V02; both now pin it.

**Depends on**: nothing.
**Refs**: PLAN §3.1, D6.
**Note**: `enum` is a declaration introducer only; it never starts an
expression, so no unary/prefix group changes.
**T02 memo**: no further `can_start_stmt` change is needed for `enum_decl`
(`enum` is already statement-starting); `can_start_stmt_table` and
`implies_stmt_end_table` already assert it. Module-path segments
(`parse_namespace_segment`) accept only `async` / `await` / `channel` among
keywords — if `enum` module members are ever needed, that whitelist is the
place (module files named `enum.fln` are NOT importable in 0.0.6).

---

## T02 — Parser: `enum_decl` + variant patterns (F1/F3; D1/D3)

- [ ] AST: `Decl::Enum(StructDecl-shaped)` — ordered variants, each with name +
      payload types; top-level only, trailing comma allowed, no trailing
      semicolon (mirror `parse_struct_decl`)
- [ ] `parse_type`: bare identifiers resolve to enums via the registration
      table at typeck (parser emits `Named`, unchanged shape)
- [ ] Pattern: `enum_pattern = ident [ "(" ident ")" ]` added to `Pattern`
      (generalizes `ResultCtor { ctor, binding }` — the Result spellings stay
      untouched and win for `Ok` / `Err`)
- [ ] Constructor expressions need **no parser change**: `Circle(1.5)` parses
      as a call, `Point` as an identifier (typeck resolves, D3)
- [ ] Tests: parser valid fixtures (zero/multi-payload decls, trailing comma,
      `when Variant(x)` pattern); invalid (enum inside a function, missing
      variant name, payload without parens)

**Depends on**: T01.
**Refs**: PLAN §3.2, D3.

---

## T03 — Resolver: visibility + import wiring (F1; D3)

- [ ] Enum names join the module export/import machinery (`pub enum`
      importable cross-module, like `pub struct`)
- [ ] Variant names register in the per-enum namespace; a dotted constructor
      path (`Color.Red`) resolves through the existing module-path rules
- [ ] Collision rules: variant vs function/struct/enum name conflicts are
      compile errors at registration (`DuplicateDefinition` family)
- [ ] Tests: resolver valid (import/pub of enums, dotted constructor paths);
      invalid (private enum access → `PrivateAccess`, name collisions)

**Depends on**: T02.
**Refs**: PLAN §5 (Resolve row), D3.

---

## T04 — Typeck: registration, constructor typing, payload rules, `RecursiveEnum` (F1/F2; D1/D2)

- [ ] Registration table extends the struct-by-name pattern (enums get ids;
      `Type::Enum(EnumId)` joins `Type`; `is_copy` covers every new variant —
      exhaustive-match test as in 0.0.2 T-immutable)
- [ ] Constructor typing: arity + payload types checked against the
      declaration (`ArityMismatch` / `ArgTypeMismatch` reuse); result type is
      the enum; zero-payload variant as bare identifier resolves like `Ok`
- [ ] Payload storage rules = struct-field rules: `ptr<T>` payload →
      `PtrNotStorable` family; `Future` / `channel` payloads inherit boundary
      rules; owned payloads make the enum owned (D1)
- [ ] **`RecursiveEnum`** (D2): payload type graph may not reach the enum
      being declared — directly, via `box`, or via `[T]` (the
      `RecursiveStruct` cycle check generalized; error carries the full path)
- [ ] No `==` / no `as string` on enums (`UnsupportedCast`; defensive VM
      posture documented in PLAN §6)
- [ ] Tests: typeck valid (Copy enum flows free, owned enum needs move/clone,
      nested enum payloads); invalid (arity/type mismatch, recursive direct /
      via box / via array, `ptr` payload, `clone` misuse, cast)

**Depends on**: T03.
**Refs**: PLAN §3.3, D1, D2.

---

## T05 — Typeck + Lower: `choose` on enums + exhaustiveness (F3; D3)

- [ ] Exhaustiveness generalizes the `Result` case: scrutinee
      `Type::Enum(id)` enumerates the registered variants; guard-free arms
      must cover all of them (`ChooseNotExhaustive` names the missing
      variants); `otherwise` short-circuits; guards never count (§10.5 rules
      unchanged)
- [ ] Lowering: `DupDeep; EnumIs v; JumpIfTrue arm` chain per variant test,
      then `EnumPayload v i` + `StoreLocal` for the pattern binding — the
      BYTECODE §6.8 Result template generalized
- [ ] Guard arms preserve the scrutinee for later arms (same sequence rule as
      the Result-with-guard note in §6.8)
- [ ] Tests: typeck (exhaustive / missing variant / guarded-only arm rejected);
      lower (hand-checked bytecode shape for a 2-variant and a 3-variant enum,
      with-guard sequence)

**Depends on**: T04.
**Refs**: PLAN §3.4, D3.

---

## T06 — Bytecode v6 + codegen + verify (F4; D4)

- [ ] Opcodes `EnumNew` (0xCB), `EnumIs` (0xCC), `EnumPayload` (0xCD) —
      encodings and stack effects per PLAN D4 (§5.17); existing instructions
      untouched (BYTECODE §10 rule)
- [ ] `enum_defs` appended table: `enum_defs_len: u32` → per enum
      `variant_count: u32` → per variant `payload_count: u32` +
      `payload_count × u8` `FieldTypeTag`; `FieldTypeTag::Enum = 12`;
      read only when `version >= 6`; written through
      `codegen::flnc::{to_bytes, from_bytes}` only
- [ ] `MODULE_VERSION = 6`; `READ_VERSIONS = [1, 2, 3, 4, 5, 6]`
- [ ] verify: `EnumNew(id)` ↔ `enum_defs[id]` cross-check (forged id →
      `EnumIndexOutOfRange`; payload-count mismatch → format error); Δdepth
      entries for the three opcodes
- [ ] Tests: v6 write/read round-trip; v1–v5 fixture decode unchanged; forged
      `EnumNew` / truncated `enum_defs` rejected (verify + flnc errors)

**Depends on**: T04 (needs enum ids and payload counts to emit).
**Refs**: PLAN §7, D4.

---

## T07 — VM: `Value::Enum` + opcode dispatch + std intrinsics (F4/F5; D4/D5)

- [ ] `Value::Enum { variant: u16, payload: Box<[Value]> }` — not serialized;
      `clone` deep-copies payloads; `Eq` on an enum is a defensive VM error
      (same posture as struct/array); `fmt.rs` debug rendering
- [ ] Dispatch: `EnumNew` / `EnumIs` / `EnumPayload`; defensive errors
      `EnumIndexOutOfRange`, `EnumPayloadMismatch` (variant ≠ operand; payload
      index out of range joins the family)
- [ ] std intrinsics (schematic, `std.str.concat` precedent):
      `std.arr.concat([T], [T]) -> [T]`, `std.arr.flatten([[T]]) -> [T]`
      registered in `builtins.rs` (`BuiltinLowering::Intrinsic`), implemented
      in the VM builtin dispatcher
- [ ] `InlineIter`: `fold` → `foldl` rename (inline lowering unchanged);
      `foldr` added as a backward index loop (no reverse copy)
- [ ] `std/arr.fln` + `std/collection/iter.fln` headers updated (the module
      header is the API contract); all `fold` call sites renamed:
      `tests/std/iter.fln`, `tests/e2e/valid/iter_helpers.fln`,
      `examples/0.0.3/tour`
- [ ] Tests: VM unit (enum value clone/move, defensive errors); std tests for
      concat/flatten/foldl/foldr incl. a foldl-vs-foldr associativity
      differential (`(a-b)-c` vs `a-(b-c)` on `[1,2,3]`)

**Depends on**: T06; T05 for the lowering the fold rename touches.
**Refs**: PLAN §4, §6, D4, D5.

---

## T08 — Integration matrix + docs sync + version bump (F6; D6)

- [ ] E2E fixtures: `tests/e2e/valid/enum_demo.fln` (construct + choose +
      payload use across Copy and owned enums); invalid: non-exhaustive
      choose, recursive enum, `as string` on enum, `==` on enum
- [ ] Compatibility: full existing suite green (v1–v5 fixtures, projects,
      std tests) — the rename must leave zero stale `fold` references
- [ ] Docs sync:
      - `docs/SYNTAX.ebnf`: `enum_decl` / `enum_pattern`
      - `docs/BYTECODE.md`: §5.17, §9 `enum_defs`, §10 v6 row
      - `docs/DESIGN.md`: §9 enum section, §17 feature table `enum` row,
        §18 roadmap 0.0.6 complete
      - `SPEC.md` (= AGENTS.md/CLAUDE.md symlink target): §13 0.0.6 goal
        paragraph, §14 quick-reference section, §15 table
      - `CHANGELOG.md`: 0.0.6 entry with the explicit breaking set
        {`enum` reserved; `fold` → `foldl`} per D6
- [ ] Version bump: workspace `Cargo.toml` → `0.0.6`; `std/VERSION` → `0.0.6`
- [ ] Examples: `examples/0.0.6/` (at least one enum demo)
- [ ] Release publication (tag/archives) stays the separate step, as in 0.0.3–0.0.5

**Depends on**: T01–T07 all green.
**Refs**: PLAN §10, D6.

---

## Dependency graph

```
T01 → T02 → T03 → T04 → T05 → T07
                    T04 → T06 → T07
T01–T07 → T08
```

T05 and T06 are parallel after T04. T07 is the single merge point (VM consumes
both the opcode set and the lowering shapes). T08 is the gate.

---

## Out of scope reminders (from PLAN §8)

No recursive enums (`RecursiveEnum` stays a compile error), no `==`, no
generic enums, no multi-binding patterns, no serialization of enum values,
no monadic combinators. Any of these needs a new decision entry in PLAN §1
first.
