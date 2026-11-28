# 0.0.10 — `Result` Moves to std + String Interpolation

> **Codename**: Loom Spider 🕸️ (settled at the release gate; lineage
> 0.0.1 Swift Fox 🦊, 0.0.2 Hermit Crab 🐚, 0.0.3 Busy Beaver 🦫, 0.0.4
> Patient Ants 🐜, 0.0.5 Careful Hedgehog 🦔, 0.0.7 Mantis Shrimp 🦐, 0.0.8
> Cheeky Hamster 🐹, 0.0.9 Mending Weaverbird 🪡).
>
> **Goal**: two zero-bytecode features. (1) `Result` stops being a
> compiler-level builtin and becomes a **declared parameterized enum in
> `std`** — the language reads it as
> `enum Result[T, E] { Ok(T), Err(E) }` (0.0.7's type-constructor machinery
> makes this expressible), while the backend keeps the 0.0.2 encoding
> unchanged (D1/D3). (2) String interpolation `"xxx${expr}"` (D4–D6),
> desugared to the existing concat machinery.
>
> **Theme**: retire the last pre-enum-era builtin representation from the
> *language* layer — the backend keeps it as a private encoding — and add
> the string-building feature the "no string `+`" design has been missing.
> Both without touching codegen, verify, the VM, or BYTECODE.md (D7).
>
> **Status**: planning. Decisions D1–D7 below are settled at planning
> level; each names the alternative it rejected and what the choice locks.
> They may be revised only by a new entry here, not silently in a ticket
> card.
>
> **What this is not**: no generics / traits / closures / operator
> overloading (all stay 0.1.0); no format specifiers (`${x:.2}`); no
> `$name` shorthand; no extended escapes beyond `\$` (`\0`, `\b`, `\f`,
> `\xHH`, `\uXXXX` stay on the 0.1.0 string-work line); no channel
> `close` / `select`; no file I/O; **no retirement of the 0.0.2 Result
> opcodes** (D1 keeps them as the runtime encoding); no bytecode change
> (D7).

---

## 0. Relationship to adjacent versions

| Version | Scope |
|---------|-------|
| 0.0.9 | complete (release published — tag and archives cut; both `docs/0.0.7/BUGS.md` findings fixed) |
| **0.0.10** | **`Result` declared in `std` with compiler-side mapping (D1–D3) + string interpolation `"${expr}"` (D4–D6) + docs/examples/version sync** |
| 0.1.0 | function-level generics / traits / operator overloading / closures; recursive types revisit; extended string escapes; `substring`/`slice`/`chars`; `for` over `string`; channel `close`/`select`; file I/O (unchanged carries) |

No planning-time corrections this milestone: 0.0.9 closed every recorded
bug (BUG-1 by commit `a5b8bd7`, BUG-2 by T01), so this milestone opens
**features** instead of repairs.

**Breaking set**: the only source-visible breaking surface is string
literals containing the exact sequence `${` — their meaning changes from
"ordinary characters" to "interpolation begins" (D4). `\$` becoming a
valid escape is additive (it was an `invalid_escape` error before). The
prelude change (D2) is behavior-preserving: every program that compiled
before still compiles, and programs rejected for shadowing the ctor names
before are still rejected (parity is verified against the merge-base in
T02; any error-text change is recorded there, never silent). Everything
else is additive.

---

## 1. Settled decisions (D1–D7)

### D1 — `Result` migrates to std as a real, compiler-mapped declaration

`std/result.fln` declares the enum:

```fleen
pub enum Result[T, E] {
    Ok(T),
    Err(E),
}
```

The declaration is the **language-level source of truth**: it parses
through the normal 0.0.7 parameterized-enum frontend, `Result<T, E>` /
`Result[T, E]` resolve as a parameterized type use of `std.result.Result`,
and `Ok` / `Err` resolve as its variant constructors. The compiler
recognizes `std.result.Result` by canonical path and maps it onto the
existing internal `Type::Result(ok, err)`; std's compilation emits **no
`enum_defs` entry** for it and no variant codegen — the module is
declaration-only (spiritual precedent: the 0.0.3 D8 rule that std source
stays doc-only for VM intrinsics).

- **The backend is untouched**: the 0.0.2 Result encoding stays exactly as
  it is — opcodes `PackOk` / `PackErr` / `IsErr` / `UnwrapOk` /
  `UnwrapErr` (region 0xA0–0xA4, BYTECODE.md §5.11), `FieldTypeTag::Result
  = 6` (§4), and the VM's `Value::Ok` / `Value::Err` variants. Lowering
  sees the same `TypedHir::ResultCtor` nodes as today (the resolver maps
  its classifications back onto them — planning-time inspection:
  `resolver/hir.rs:478`, `lower/mod.rs:1492`, `value.rs:160`,
  `codegen/bytecode.rs:186`).
- **Rejected (full unification)**: representing Result as an ordinary
  enum value at runtime (`Value::Enum` + `EnumNew`/`EnumIs`/`EnumPayload`,
  retiring 0xA0–0xA4). It would force bytecode v9 and a reader decision on
  old Result-bearing modules — bytecode is frozen for compatibility once
  released — for zero user-visible gain (the five opcodes are not language
  surface). The cleanup it buys is backend-internal churn.
- **Rejected (comment-only declaration)**: documenting the enum in
  `std/result.fln` as comments only (the intrinsic style, no real
  declaration). Then the definition would still live in compiler tables
  and the migration would be cosmetic — the declaration would not be the
  source of truth.
- **What this locks**: the 0.0.2 Result opcodes are a **permanent reader
  obligation** (every future reader accepts 0xA0–0xA4); `std.result.Result`
  is a compiler-recognized builtin path (a user module cannot shadow or
  re-declare it, D2).

### D2 — Prelude exception: `Ok` / `Err` / `Result` are prelude names

The 0.0.3 V06 prelude rule ("the prelude is `print` only; everything else
is imported by name") gains **one recorded exception**: `Ok`, `Err`, and
the type name `Result` are available in every file without an import.
`import std.result;` also works (it imports the same names) and remains
the documented spelling for readers who want the provenance explicit.

- **Non-shadowable**: declaring a binding or type named `Ok` / `Err` /
  `Result` is rejected, the same effective semantics as today (they are
  builtin constructor/type names). Parity is verified against the
  merge-base in T02 — every program rejected before is still rejected;
  if any error text changes, the change is recorded in T02, never silent.
- **Rejected (explicit import required)**: making `import std.result`
  mandatory. It would break every Result-using program in existence
  (`?`, `std.arr.get`, `std.str.parse_int`, async `Result` returns — 6 of
  the 54 e2e valid fixtures alone exercise `Ok`/`Err` directly), for a
  purity gain the language does not need: `?` already makes Result
  unavoidable boilerplate-free, so the prelude exception is the honest
  description of its status.
- **What this locks**: the prelude rule is "print + the Result trio"; any
  further prelude growth needs its own decision entry.

### D3 — `?` and choose-on-Result keep their exact semantics and lowering

The 0.0.9 attempt-chain lowering (`lower_choose_result` with `otherwise`
and guards, D2 of 0.0.9) and the `?` propagation path stay as they are;
they compile to `IsErr` + `UnwrapOk` / `UnwrapErr` as today. Nothing is
re-plumbed onto the general enum `choose` path — that path remains
available to user-declared enums only.

- **Rejected**: deleting the attempt-chain lowering in favor of the
  generic enum path. The generic path targets `EnumIs`/`EnumPayload`; the
  re-route would be pure churn with byte-output risk on every
  Result-using program, and buys no semantic difference (0.0.9 D2 already
  aligned the coverage rules).
- **What this locks**: `lower/` keeps a Result-specific module — recorded
  as the private encoding of the std enum, not as a wart.

### D4 — Interpolation syntax: `"${expr}"`, `\$` escape, `$` special only before `{`

- Syntax: `${expr}` where *expr* is a **full expression** parsed with the
  ordinary expression grammar (calls, field access, arithmetic, nested
  string literals — recursive scanning makes `"a${ "x${1}" }b"` work
  naturally).
- `\$` becomes a valid escape producing a literal `$` (the one new escape,
  joining the six pre-existing ones; previously `invalid_escape`).
  `"a\${b}c"` is the literal `a${b}c`.
- `$` is special **only immediately before `{`**; in every other position
  it is an ordinary character. Existing strings containing `$` elsewhere
  (`"a$b"`, `"100$"`) keep compiling with byte-identical output.
- **Breaking set (the milestone's only one)**: a string literal containing
  the exact sequence `${` changes meaning (was literal characters, now
  interpolation opening). Same magnitude class as the 0.0.5/0.0.6 reserved
  words and 0.0.9's `break`/`continue`.
- **Rejected (`$name` shorthand)**: adds ambiguity rules (`$` followed by
  identifier chars vs. anything else) for a convenience `${name}` already
  covers; revisit only with evidence.
- **Rejected (bare `$` becomes an error)**: silently grows the breaking
  set from "strings containing `${`" to "strings containing `$`".
- **Rejected (`\(expr)` Swift style)**: less conventional for a
  brace-delimited-block language.
- **What this locks**: the lexer performs a two-character lookahead for
  `${` inside string literals; `\$` is valid forever after; a bare `$`
  never needs escaping unless followed by `{`.

### D5 — Conversion semantics inside `${}`

- `int`, `float`, `bool` implicitly stringify using **exactly the `as
  string` conversions** (same formatting, lowered through the same path —
  `ToStr`, BYTECODE.md v0.0.2 row 0xA5). No new format machinery.
- `string` embeds verbatim, **deep-copied**: interpolation is a *read
  context* and never moves a user variable — `print("x=${s}")` leaves `s`
  usable (lowering uses the existing `DupDeep`).
- Every other type (`enum`, `struct`, `[T]`, `box<T>`, `Result`,
  `Future<T>`, `channel<T>`, `ptr<T>`, function values) is a **compile
  error** (`InterpUnsupportedType`, named after the existing error style)
  with a hint naming the explicit conversion. There is no auto-derived
  formatting to reuse: 0.0.6 already rejects `as string` on enums, and
  structs have no `as string` either.
- **No format specifiers** (`${x:.2}` and friends): the `as string`
  formatting is the only formatting; if it is ever insufficient, that is a
  separate decision.
- **Rejected (explicit-only)**: requiring `${x as string}` defeats the
  feature's purpose — interpolation *is* the stringification convenience.
- **Rejected (`std.str.show(x)` required)**: noise on the most common
  operation in the language; the implicit rule is confined to `${}` and
  does not change `print`'s string-only contract or any other context.
- **What this locks**: the set of implicitly-stringifiable types inside
  `${}` is closed (the four above); widening it needs a decision entry.

### D6 — Lowering: desugar to `std.str.concat` chains; zero new opcodes

The interpolation node lowers to a chain of `std.str.concat` calls over
**merged literal chunks** (adjacent literal parts become one string).
Left-to-right evaluation; each part evaluated exactly once (the concat
chain's order locks this).

- A string literal **without `${`** compiles **byte-identically** to
  today: a single const-pool string, no calls. This preserves the
  content-driven version discipline (0.0.8 D13/D14): modules stay v7
  unless a `PartialApply` was encoded; interpolation adds no opcode.
- **Rejected (a `StrConcatN` opcode)**: YAGNI. If many-part performance
  ever matters, a `std.str.join([string]) -> string`-style intrinsic is
  the zero-opcode lever in a later milestone.
- **What this locks**: `std.str.concat`'s two-argument semantics and the
  chain's left-to-right order are observable (side-effecting embedded
  expressions evaluate in source order).

### D7 — No bytecode change, milestone-wide (0.0.9 D6 precedent)

Both features compile to existing opcodes: the Result migration reuses
`PackOk` / `IsErr` / `UnwrapOk` / `UnwrapErr` and `FieldTypeTag::Result
= 6` untouched; interpolation reuses the intrinsic-call path
(`std.str.concat`) and `ToStr`. The bytecode version stays **v8**; no new
opcodes; no table changes; readers accept `{1..=8}` unchanged;
`BYTECODE.md` gains no rows (T07 records the confirmation). Any future
opcode need is a new PLAN decision first.

---

## 2. Feature list

| ID | Feature | Tickets |
|----|---------|---------|
| F1 | `Result` declared in std, compiler-mapped, prelude-provided | T01, T02, T03 |
| F2 | String interpolation `"${expr}"` | T04, T05, T06 |
| F3 | Docs sync, examples, version bump, release gate | T07, T08 |

---

## 3. Language design

### 3.1 `Result` in std (F1)

```fleen
// std/result.fln — the declaration (the language-level source of truth).
// Module doc states the D1 contract: the compiler maps std.result.Result
// onto its internal builtin representation; the runtime keeps the 0.0.2
// encoding; no enum_defs entry is emitted for this declaration.
pub enum Result[T, E] {
    Ok(T),
    Err(E),
}
```

User code is unchanged in shape:

```fleen
func div(a: int, b: int): Result<int, string> {   // Result[int, string] equally legal (0.0.7 D9)
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

v = div(4, 2)?;                        // `?` unchanged
r = choose div(1, 0) {                 // choose unchanged (attempt chain, 0.0.9 D2)
    when Ok(x) { x }
    otherwise { 0 - 1 }
};

import std.result;                     // optional; the trio is prelude (D2)
func fallback(): Result[int, string] = Ok(7);
```

What does **not** change: `?` semantics; choose-on-Result coverage rules
(incl. `otherwise`, guards); `Ok`/`Err` as constructors only (bare `Ok`
in expression position is still rejected); `is_copy(Result<T, E>)`
argument dependence; `ref`/async-boundary checks; struct fields of
`Result` type (`FieldTypeTag::Result = 6`); every std signature speaking
`Type::Result` (`std.arr.get`, `std.str.parse_int` / `parse_float` /
`index_of`, channel `recv`, …).

### 3.2 String interpolation (F2)

```fleen
import std.io;
import std.str;

n = 42;
s = "hello";
print("n = ${n}, s = ${s}, len = ${std.str.len(s)}");   // n = 42, s = hello, len = 5

w = "${s}";                 // deep copy — s is still usable afterwards (D5)
big = "${1 + 2 * 3}";       // full expressions: "7"
nest = "a${ "x${n}" }b";    // nested string literal inside an embedded expression
lit = "a\${b}c";            // literal `a${b}c` (the \$ escape, D4)
price = "100\$";            // literal `100$` (was invalid_escape before 0.0.10)

print("${[1, 2]}");         // compile error: InterpUnsupportedType (+ hint)
```

- Only the conversion set of D5 is implicit; `print`'s string-only
  contract is untouched (`print("n = ${n}")` is one string argument, not a
  new print form).
- Escaping recap: `$` needs escaping only when immediately followed by
  `{`; `\$` is the escape; every other `$` is a plain character.

---

## 4. std library changes

- New module `std/result.fln`: the D1 declaration + doc comments. It
  contains no functions; it is declaration-only by compiler recognition
  (no enum_defs entry, no variant codegen — D1).
- No other std module changes: `std.arr` / `std.collection` / `std.io` /
  `std.os` / `std.num` / `std.time` / `std.str` / `std.async` /
  `std.concurrent` untouched. (Planning-time inspection: no std `.fln`
  uses `Ok`/`Err` in code today — only doc comments — so std itself does
  not exercise the migration; the T03 integration fixtures do.)

---

## 5. Compiler-stage changes

| Stage | Change |
|-------|--------|
| Lexer | interpolation only: parts scanning, `${` lookahead, `\$` escape (D4). `Ok`/`Err` were never lexer-level — nothing to remove |
| Parser | retire the `Ok`/`Err` syntactic ctor and the type-position `Result<` special case (ordinary constructs now); add the interpolation node |
| Resolver | prelude trio (D2) + classification through the 0.0.9 D1 variant-reference machinery, mapped back to `ResultCtor` HIR; `std.result.Result` registration |
| Typeck | parameterized type use of `std.result.Result` → `Type::Result` (no rule changes); the D5 conversion rule + `InterpUnsupportedType` |
| Lower | interpolation desugar (D6); **nothing** for Result (D3) |
| Codegen | none |
| Verify / VM | none |

---

## 6. VM impact

None (D7). Every instruction and intrinsic the features compile to
already exists: `PackOk` / `PackErr` / `IsErr` / `UnwrapOk` /
`UnwrapErr` (0xA0–0xA4), `ToStr` (0xA5), the `std.str.concat` intrinsic,
`DupDeep`. No new `Value` variants, no opcode arms, no scheduler changes.

---

## 7. Bytecode impact

None (D7). No opcode rows; no table changes; the module version follows
the content-driven rule — interpolation adds no opcode, so affected
modules stay at v7 unless a `PartialApply` was encoded. A string literal
without `${` is byte-identical to 0.0.9 output (D6).
`BYTECODE.md` untouched; T07 records the confirmation.

---

## 8. Explicitly out of scope (0.0.10)

| Item | Reason |
|------|--------|
| Generics / traits / operator overloading / closures | 0.1.0 (unchanged) |
| Extended escapes `\0` `\b` `\f` `\xHH` `\uXXXX` | the 0.1.0 string-work line; `\$` is the only new escape (D4) |
| Format specifiers (`${x:.2}`) | `as string` formatting is the only formatting (D5); separate decision if ever needed |
| `$name` shorthand | ambiguity cost without demonstrated need (D4) |
| Implicit stringification of enums/structs | no auto-derived formatting exists; 0.0.6 rejects `as string` on enums (D5) |
| Retiring the 0.0.2 Result opcodes (`Value::Enum` unification) | bytecode v9 + old-module translation for zero user-visible gain (D1) |
| Re-routing Result-`choose` onto the generic enum path | pure churn, byte-output risk, no semantic gain (D3) |
| Recursive enum revisit, `Result` special-ops removal | unchanged carries |
| Channel `close` / `select`, `async {}` blocks, file I/O, `for` over `string`, `substring`/`slice`/`chars` | 0.1.0 carries (0.0.4 §6 / 0.0.9 §8) |

---

## 9. Test matrix (planned)

**F1 (Result migration)** — the existing suite is the regression net:

- Full suite green with unchanged stdout/stderr (59 e2e valid fixtures as
  built at T03 — 54 standing + 5 new; 11 exercise `Ok`/`Err` directly).
- New valid: explicit `import std.result;`; prelude trio without import;
  `Result[T, E]` bracket spelling in type position; struct field of type
  `Result<int, string>`; `?` / choose-`otherwise` / `await?` flows
  unchanged.
- New invalid: shadowing `Ok` / `Err` / `Result` (rejected, parity-checked
  against the merge-base); bare `Ok` in expression position (rejected, as
  today); user enum named `Result` (rejected).
- std gate: std compile green; std's module set gains `std.result` with an
  empty runtime footprint; no `enum_defs` growth anywhere (recorded
  expectation, not a hash gate — 0.0.9 T05 posture).

**F2 (interpolation)**:

- Lexer valid: `\$`; `$` in other positions; nested braces in an embedded
  expression; a nested string literal inside an embedded expression.
- Lexer invalid: unclosed `${` (error with span); unterminated string
  unchanged.
- Parser/typeck invalid: enum embed, struct embed, array embed,
  `Result` embed (each `InterpUnsupportedType` + hint); `$name` shorthand
  is not syntax (a `$` before a non-`{` is literal).
- Lower/e2e valid: every D5-convertible type; embedded calls/field access;
  a string embed **reused after** interpolation (copy proof); interpolation
  inside `print` arguments and inside std calls; adjacent-literal merging.
- Byte gate: a `${}`-free program compiles byte-identically to the
  merge-base (recorded expectation on a few fixtures, not a hash gate);
  no fixture reaches v8 unless it encodes a partial.

---

## 10. Versioning

- `Cargo.toml` workspace version → `0.0.10`; `std/VERSION` → `0.0.10`
  (T08).
- Release publication (tag + six-target archives) follows the
  0.0.7/0.0.8/0.0.9 pipeline; codename settled before the release commit
  (holder's call). Per the established cadence, after the local commit +
  tag the agent **stops and reports** for the user to push (L9).
