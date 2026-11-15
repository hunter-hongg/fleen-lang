# 0.0.8 — Per-File Feature Flags + Function Currying & Partial Application

> **Codename**: Cheeky Hamster 🐹 (0.0.1 Swift Fox 🦊, 0.0.2 Hermit Crab 🐚,
> 0.0.3 Busy Beaver 🦫, 0.0.4 Patient Ants 🐜, 0.0.5 Careful Hedgehog 🦔,
> 0.0.7 Mantis Shrimp 🦐). A hamster carries partially-collected loads in its
> cheek pouches — stored now, consumed later. So does a partial application.
>
> **Goal**: introduce **function currying and implicit partial application**
> behind a **per-file feature flag**, without breaking anything that does not
> opt in. Under the `curry` feature a file's function signatures take the
> curried two-part form —
>
> ```fleen
> add: int -> int -> int
> add(a, b) { a + b }
> ```
>
> — and calling a multi-parameter function with fewer arguments than it
> declares yields a **partial application**: a function value over the
> remaining parameters.
>
> **Theme**: one opt-in switch, three layers of new capability. The
> **syntax** layer: the `#feature` magic statement and the two-part curried
> declaration. The **type** layer: curried arrow function types and the
> implicit partial-application typing rule. The **runtime** layer: function
> values that can carry captured arguments (`PartialApply`, bytecode v8).
>
> **Status**: planning. Decisions D1–D16 below are settled at planning level;
> each names the alternative it rejected and what the choice locks. They may
> be revised only by a new entry here, not silently in a ticket card.
>
> **This is not closures.** A partial application captures only the
> arguments already supplied at its own call site — there is no way to refer
> to free variables of an enclosing scope, and no lambda syntax. Closures
> remain 0.1.0 (§8). The captured-argument mechanism is the minimal runtime
> substrate the feature needs; it does not generalize.

---

## 0. Relationship to adjacent versions

| Version | Scope |
|---------|-------|
| 0.0.7 | complete (release published — tag and archives cut) |
| **0.0.8** | **per-file feature flags (`#feature`) + curried declarations + curried arrow type notation + implicit partial application + `PartialApply` (bytecode v8, content-driven)** |
| 0.1.0 | function-level generics / traits / operator overloading / closures; recursive types revisit; extended string escapes |

0.0.8 is **non-breaking additive** like 0.0.7 — with one structural
difference: the additive surface is *opt-in per file*. A file without
`#feature curry` compiles byte-identically to 0.0.7 (D16). The flag is
**per file, not per project** (D1): a flag-off file imports a flag-on file's
functions and calls them at full arity, unaware of the feature; the flag-on
file imports flag-off code (including std) and may partially apply it.

Recursive enums stay rejected; `Result[T, E]` stays builtin; closures stay
0.1.0 — none of that moves.

---

## 1. Settled decisions (D1–D16)

### D1 — The feature flag is a per-file `#feature` magic statement

```fleen
#feature curry        ← optional, first statement(s) of the file
import std.io;        ← everything else follows
...
```

- Syntax: `#feature <name>`. `#` is a **new token** (`Hash`); the comment
  syntax is untouched (`//`, `/* */` — `#` is unused in the lexer).
- The directive is **unconditional grammar** from 0.0.8 on — it is the
  mechanism that enables flags and cannot itself be flag-gated. Only the
  *effects* of a feature are gated per file.
- A directive may appear only in the **file header region**: before the
  first item (including before `import` — imports are items). Comments and
  blank lines do not end the header region; any item does.
- **Any `Hash` token after the header region is an error**
  (`FeatureDirectiveMisplaced`), in file-item position and inside function
  bodies alike (the block-statement parser rejects `#`, the same way it
  rejects `pub` — parser/mod.rs:327 precedent). This is the "magic
  statement" rule: a `#` line seen mid-file is a hard error, never a
  comment, never a no-op.
- Unknown feature name → `UnknownFeature` (hard error — a typoed feature
  name must fail loudly, never be ignored). Duplicate declaration of the
  same feature in one file → `DuplicateFeature`.
- Multiple distinct `#feature` lines are allowed in the header region (one
  name per line; the comma-list form is deliberately not provided in
  0.0.8 — §8).
- ASI: directive lines are newline-terminated and take no semicolon.
  (Clarified during T01) one ASI rule *does* interact, contrary to the
  first draft of this bullet: `#` is statement-starting in
  `can_start_stmt` / `implies_stmt_end`, so the newline between two
  directives inserts the terminating `Semi` instead of tripping rule 6,
  and a stray `#` after the first item routes to the dedicated
  `FeatureDirectiveMisplaced` error (same route as `pub`). "Take no
  semicolon" is a *prohibition*, not an omission: an explicit `;` in the
  source is rejected — the ASI-inserted zero-width `Semi` is the only
  accepted terminator. Joining a directive with anything on one line falls
  out of the existing "joining statements on one line is illegal" rule.
- *Rejected*: a pragma inside a comment (`// fleen: features curry`) —
  invisible to the grammar, unreportable with a span, undiscoverable by
  tooling. *Rejected*: project-level flags in `flint.toml` `[features]` —
  per-file granularity is the requirement (std must stay flag-off; see D2);
  the manifest parser explicitly rejects unknown tables today
  (flint-cli/src/manifest.rs:240) and gains nothing from growing one.
- *Locks*: the Session carries a per-`FileId` feature set
  (`Span.file: FileId` exists since 0.0.3 V01); parse/typeck/lower consult
  the *current file's* set. The feature name `curry` is fixed; adding a
  second feature later is additive (new name, new gates), never a syntax
  change to the directive.

### D2 — `curry` switches the declaration form per file: the two-part curried declaration is the only function form in a flag-on file

```fleen
#feature curry
add: int -> int -> int      ← signature line (the interface)
add(a, b) { a + b }         ← body line (parameter names live here)
add2(a, b) = a * 2 + b      ← single-expression body form, same shape
```

- In a file with `#feature curry`, the legacy `func`-keyword form is
  **rejected** (`FuncFormForbiddenInCurryFile`). The two-part form is the
  *only* function declaration form there. This is strictness by design: the
  flag switches the file into the curried language.
- **std and all pre-0.0.8 code are untouched** because the flag is per file
  (D1): std sources carry no directive, keep the `func` form, and are
  compiled flag-off in every project. The strictness costs nothing outside
  the files that opt in.
- The signature line carries the **curried type**; the body line carries
  the **parameter names**. The signature has no names and the body has no
  types — each half carries exactly what it needs.
- A zero-parameter function is `ping: () -> int` / `ping() { … }` (D4).
- The entry function `main` in a flag-on file uses the same two-part form.
- *Rejected*: keeping the `func` form legal alongside the two-part form in
  flag-on files ("lenient mode") — it makes "the signature becomes curried"
  a half-truth and leaves two ways to say the same thing in one file. The
  per-file flag makes strictness free; revisit only by a new decision
  entry.
- *Locks*: every pre-0.0.8 `.fln` file (std, examples, fixtures, tour)
  compiles unchanged; the old and new forms never mix within one file.

### D3 — Two-part declaration rules: matching, ordering, modifiers, visibility

- **Signature first**: the signature line must precede its body line. A
  body whose signature has not been seen is `BodyWithoutSignature`; a
  signature never matched by a body is `SignatureWithoutBody`. Matching is
  by name within the module.
- **Arity consistency**: the number of arrows in the signature's parameter
  chain must equal the body's parameter count
  (`SignatureArityMismatch`).
- **Modifiers live on the signature line**: `async` / `unsafe` / `trusted`
  prefix the signature (`async half: int -> Result[int, string]`). They are
  interface properties — the caller-facing facts (a `Future` return type,
  the unsafe-call restriction) — so they belong on the half the caller
  sees. A modifier on the body line is an error.
- **`pub` lives on the signature line**. The signature is the interface;
  visibility is an interface property. `pub` on a body line is an error.
  Without `pub`, the function is module-private (unchanged rule).
- The signature line is **single-line only** (D1c in the discussion): ASI
  is newline-sensitive, and a multi-line arrow chain would break at the
  first trigger point. No ASI carve-out is introduced.
- Body parameter names must be distinct; the body is a normal function
  body in every other respect (ASI, last-expression return, `?`, `await`).
- *Locks*: the module export/import tables carry the signature's type and
  modifiers exactly as `func` declarations do today; cross-module behavior
  is identical.

### D4 — Curried arrow function types are the canonical notation in type positions

- In type positions (signature lines, parameter types, return types,
  binding annotations), a function type is written as a **right-associative
  arrow chain**: `int -> int -> int` parses as `int -> (int -> int)`.
- **Zero-parameter functions** spell an explicit empty parameter list:
  `() -> int`. A unary function over `unit` is written `unit -> int` — the
  `()`-atom immediately followed by `->` is the empty parameter list, never
  the unit type. This is the one spelling carve-out (D12 in the
  discussion); it keeps the zero-param signature unambiguous against plain
  `unit`.
- The legacy tuple spelling `(int, int) -> int` remains **legal as sugar**
  in type positions and canonicalizes to the same internal type — both
  spellings denote `Type::Func([int, int], int)`. This is required for
  cross-flag ergonomics: a flag-on file annotating a std (flag-off)
  function's shape may use either spelling, and flag-off files never see
  the arrow chain at all. The 0.0.7 D9 dual-spelling precedent
  (`Option<int>` / `Option[int]`) is the model: two spellings, one
  canonical form, one internal representation.
- **Diagnostics render the curried spelling**: a flag-on file's error
  messages display `int -> int -> int`; flag-off files keep the current
  `(int, int) -> int` rendering. Error-text fixtures are per-mode (§9).
- *Rejected*: representing a curried function *actually* as nested unary
  functions (a chain of one-parameter frames) — it rewrites the calling
  convention, makes every n-ary call n frame pushes, and changes the
  execution shape of flag-on code that calls at full arity. The arrow chain
  is notation + a typing rule (D5), not a representation change.
- *Locks*: `Type::Func(params, ret)` is unchanged; the arrow chain is
  parser sugar over it; bytecode is untouched by this decision (D13).

### D5 — Implicit partial application: calling with fewer arguments yields a function over the rest

- Rule: a call `f(a₁, …, aₖ)` to a **participating** function of arity
  `n > k` has type `Func([tₖ₊₁, …, tₙ], ret)` — the remaining-parameter
  function. The result is a **partial application** value carrying the
  callee and the `k` consumed arguments.
- `k = n` is a full call (unchanged); `k > n` is `ArityMismatch`
  (unchanged); `k = 0` is the degenerate partial — it yields the function
  itself (uniformity beats a special case).
- **Participation** (who may be partially applied):
  - any fixed-arity user function (declared in either a flag-on or a
    flag-off file — the rule lives at the **call site**, so the callee's
    file flag is irrelevant);
  - **not** variadic builtins (`print`, `println` — arity-unrestricted,
    D7);
  - **not** functions whose parameter types mention `ref` or `ptr<T>`, and
    **not** `unsafe`/`trusted`-modified functions (D9);
  - `async` functions participate; see D12.
- **Argument ownership is unchanged**: a partial application consumes its
  arguments exactly as a full call does (`move`/`clone` required for owned
  locals, `Copy` arguments bind freely — the same rules, the same
  predicate). The only new rule is what the *result* is (D11).
- The canonical example, and the reason the feature pairs with the curried
  signature: `add: int -> int -> int` means `add(1)` has type `int -> int`
  — literally the rest of the signature.
- *Rejected*: requiring explicit syntax (`partial(f, 1)`, `f(1, _)`) —
  the design goal is implicit currying under the flag; explicit forms can
  be added later without changing D5 (they would lower to the same
  `PartialApply`).
- *Locks*: the arity relaxation replaces today's too-few-arguments
  `ArityMismatch` **in flag-on files only**; flag-off files keep the exact
  error. Full-arity calls compile to the identical bytecode as today.

### D6 — Partial values are ordinary function values; argument passing stays exact-match

- A partial value has the remaining function type — `add(1) : int -> int`
  — and flows like any function value of that type.
- **No subtyping**: a partial of `(int, int) -> int` does *not* satisfy a
  `(int, int) -> int` parameter, and a genuine unary function does not
  gain partial-ness. Exact type match for arguments, as today.
- A flag-off file can call a flag-on file's curried function at full arity;
  it cannot partially apply it (`ArityMismatch` — the rule is call-site
  local). Conversely, partial values produced in a flag-on file flow
  through flag-off code freely: the VM is flag-blind (D15), and a
  `Func`-typed parameter called at full remaining arity works regardless of
  where the value was created.
- *Locks*: no coercion, no widening, no "any prefix arity satisfies any
  suffix function type" rule. The type system stays exact-match everywhere
  except the one relaxation in D5.

### D7 — Variadic builtins never participate

- `print` / `println` are `BuiltinParam::Variadic` — their arity is
  unrestricted by design, so "fewer arguments" is always a complete call
  for them. Implicit partial application does not apply; flag-on files call
  them exactly as today.
- *Rejected*: currying variadics (a variadic partially applied is a
  variadic — the type system has no `...` remainder type, and inventing one
  dominates the release).

### D8 — Inline-lowering builtins reroute at partial call sites

- Builtins whose calls are inlined at the call site — `range` /
  `range_from` (fold to `MakeRange`), the `std.collection.iter` helpers
  (`map` / `filter` / `foldl` / `foldr` / `take` expand inline) — take
  their inline path only on a **full-arity call**. A partial call to a
  participating inline-lowered builtin skips the inline expansion and
  emits the generic partial-application sequence (`LoadFunc` + args +
  `PartialApply`).
- This is the rule most easily missed in implementation: the inline
  lowerings are keyed at the call site (lower/mod.rs:1568), so the *call
  site's* flag decides, and the callee being flag-off std is irrelevant.
- *Locks*: every inline lowering is either full-arity-only or explicitly
  partial-capable; no inline path may receive a partial call.

### D9 — `ref` / `ptr` / `unsafe` functions are exempt from partial application

A call with fewer arguments than parameters to a function in any of these
classes keeps today's `ArityMismatch`:

- **`ref` parameters** — a partially-applied borrow would have to stay
  alive between the partial site and the final call, across arbitrary
  control flow; the borrow-free worldview (0.0.4 D1/D4) refuses this.
  (`RefInAsync` is the same refusal at the async boundary.)
- **`ptr<T>` parameters** — a pointer must never be captured into a
  stored location; a partial's capture list is a new storage location, and
  `PtrNotStorable` already enumerates the ban list (struct fields, array
  elements, box cells, globals). The capture list joins that list.
  (`PtrAcrossBoundary` is the boundary sibling.)
- **`unsafe` / `trusted` functions** — 0.0.5 has a documented known limit:
  an `unsafe func` called *through a function value* escapes the static
  `UnsafeCallOutsideUnsafe` check (the VM backstop on `LoadPtr`/`StorePtr`
  is the last line). Implicit partial application would turn that manual
  escape hatch into a *routine path* — every short-arity call to an unsafe
  function would become a function value. The exemption keeps the static
  check total for unsafe functions under the flag.
- The exemption is **at the call site**: a flag-on file cannot partially
  apply these functions regardless of where they were declared; flag-off
  files were never able to anyway.
- *Rejected*: propagating "callee is unsafe" through partial values
  type-theoretically — it adds a whole tracked property to function types
  for one exemption that already has a clean conservative answer.

### D10 — Anti-typo guard: a discarded partial application is an error

- In a flag-on file, an expression statement whose value is a
  **partial-application result** is a compile error
  (`PartialApplicationDiscarded`).
- Rationale: the classic currying trap — `add(a)` where `add(a, b)` was
  meant — silently produces a function value. Bare expression statements
  are legal today for side-effectful calls, but a partial application is
  pure: discarding it is always meaningless, so the rule has no false
  positives. Full calls used as statements are untouched.
- Mechanism: the typed call node carries a `partial` marker when the
  call had fewer arguments than the callee declares; the statement checker
  inspects the marker (type alone is not enough — a genuine unary function
  value as a statement is as meaningless as today, which is to say: still
  legal, unchanged).
- *Rejected*: a lint/warning — the repo's culture is hard errors
  (`#![deny(warnings)]`, clippy `-D warnings`); and a silently-created
  function value is a bug by construction.

### D11 — Copy semantics: partial values are Copy; copying duplicates the capture list

- Function values are **Copy** (SPEC §14) and stay Copy — a partial value
  is a function value and its type (`Func(remaining, ret)`) is Copy, so
  bindings, arguments, and containers consume it freely, exactly as today.
- Runtime representation: `Value::Func` grows a capture list
  (`Func { id, captures: Vec<Value> }`; a bare function is the empty
  capture list). **Copy consumption of a captured function deep-copies the
  capture list** — array/struct/global/binding copies duplicate each
  captured value (the VM's copy paths branch on a non-empty capture list).
- Soundness: each copy owns an independent snapshot of its captures; the
  final call consumes the captures of *its own* value. Two copies of a
  partial, called separately, consume separately. No shared state, no
  use-after-move.
- **Acknowledged tension**: the SPEC's ownership posture refuses hidden
  deep copies for owned values (a bare owned local read must be explicit
  `clone`). The deep copy here rides on a Copy type — the function type —
  whose Copy-ness is the SPEC's own rule, and it fires only when a partial
  is duplicated. *Rejected alternative*: banning owned (non-Copy) captures
  (only `Copy` arguments may be captured) — sound and cheaper, but it
  deletes the bread-and-butter use (`prefix("hello")`-style partials over
  strings) and makes the feature a demo.
- With all-Copy captures the deep copy is trivial; with none (bare) it is
  the current behavior. The rule is uniform: one code path, branch on
  emptiness.
- *Locks*: `Type::Func` stays unconditionally Copy at the type level
  (bare-function code is unchanged everywhere); the deep copy is a runtime
  copy-path concern, invisible to the type system.

### D12 — `async` interaction: partial application before the future exists

- `async` functions participate in partial application. The signature
  annotates the **body's** return type (as today): `async half: int ->
  Result[int, string]`; the *caller* sees `Future<Result[int, string]>`.
  A partial of `half` has type `int -> Future<Result[int, string>>` — the
  future is created by the final call, never at the partial site.
- `Future<T>` is non-Copy and move-only; a partial *value* is Copy (D11) —
  the two compose without interaction, because the future exists only
  after the final call. `e await` / `e await?` apply to the final call's
  result as always.
- Capture rules are those of D11 plus D9 (no `ref`, no `ptr`); a partial
  value crossing an await/channel boundary is a normal value move (the
  capture list travels inside the `Value::Func`).
- *Locks*: no new boundary rule; the 0.0.4 boundary set (`RefInAsync`,
  channel payload rules) is untouched.

### D13 — Bytecode: `PartialApply` (0xCE) and a content-driven version bump

- New opcode **`PartialApply = 0xCE`** — the first slot of the reserved
  `0xCE–0xDF` group (BYTECODE.md §4.2; the 0.0.6 enum instructions took
  the group head `0xCB–0xCD` the same way). Operand: `u8 k` (number of
  arguments applied at this site). Stack: `f a₁…aₖ → f'` where `f'` is
  the captured function value.
- **The version bump is content-driven, not flag-driven**: writers emit
  version **7** by default and bump to **8** only when a `PartialApply`
  instruction was actually emitted. A `#feature curry` file that only uses
  the new declaration form and arrow types still produces a v7 module —
  old readers can read it, because the two-part declaration compiles to
  the identical function table.
- Readers accept `{1, 2, 3, 4, 5, 6, 7, 8}` (the established widening
  pattern; v1–v7 fixtures decode unchanged). verify gains the `PartialApply`
  stack-shape check (pops `k + 1`, pushes 1) for v8 modules.
- **The merged-module reality**: a project compiles to one bytecode module
  (BYTECODE.md §"one bytecode module"), so the version is per-module, not
  per-file — one partial call anywhere lifts the whole module to v8. The
  flag exists only at the source/semantic layer.
- *Locks*: the old `ArityMismatch`-producing flag-off path emits nothing
  new; a flag-off project's module is byte-identical to 0.0.7 output.

### D14 — Captures are runtime-only; no serialization surface

- The capture list lives in `Value::Func` at runtime and is **never
  serialized**: no constant-pool tag, no `FieldTypeTag`, no `.flnc` table
  change. This is the exact precedent of `Value::Future` / `Value::Channel`
  / `Value::Ptr` ("not serialized: there is no `.flnc` tag"). Globals and
  struct fields of function type are initialized by `__init__` code at
  runtime, not by the constant pool, so there is no round-trip surface.
- v8 differs from v7 by exactly one opcode in §5 and one line in §10 —
  the append-only compatibility promise (§10) is intact.
- *Locks*: the disk format grows by one opcode number, nothing else.

### D15 — The VM is flag-blind; support is unconditional

- The VM never learns about `#feature`. `Value::Func` carries captures,
  `CallValue` handles them, `Dup`/store paths deep-copy them — always,
  regardless of which file's code is executing.
- This is what makes the cross-flag flow work: a partial value created in
  a flag-on file flows into flag-off code (e.g. std's higher-order
  helpers) and is called there at its remaining arity — the runtime treats
  bare and captured functions uniformly.
- `CallValue` consumes `captures.len() + argc` and requires the total to
  equal the callee's `params` (today it requires `argc == params`;
  vm.rs:730). A bare function behaves exactly as today.
- *Locks*: the VM accepts v8 modules unconditionally; the frontend gates
  are the only place that decides who can *create* captured values.

### D16 — Non-breaking posture: what "not breaking anything" means here

- **Flag-off files** (no directive): lexer/parser/typeck/lower behavior,
  diagnostics, and emitted bytecode are byte-identical to 0.0.7. The
  `#` token is new but flag-off files never contain one; the arrow-chain
  type production is unreachable there.
- **Flag-on files**: the only source-visible change is in *that file* —
  the declaration form switch (D2) and the arity relaxation (D5, which
  only turns prior errors into successes). No previously-valid flag-on
  program changes behavior: full-arity calls compile identically (D13).
- **std**: zero changes, zero rewrites, compiled flag-off in every project
  (D1/D2).
- **Bytecode**: v7 by default, v8 only on content (D13); readers keep
  accepting all old versions.
- CHANGELOG carries this posture; the SPEC §13 feature matrix gains the
  per-file-flag and currying rows.
- *Locks*: this release, like 0.0.7, breaks nothing; its additive surface
  is simply opt-in.

---

## 2. Feature list

| ID | Feature | Layer | Depends on |
|----|---------|-------|------------|
| F1 | `#feature` magic statement + per-file FeatureSet (`Hash` token, header region, placement errors) | Language (lex → parse → Session) | — |
| F2 | Two-part curried declarations, strict in flag-on files; signature/body matching | Language (parse → resolve) | F1 |
| F3 | Curried arrow function types in type positions (right-assoc; `() -> T`; tuple sugar) | Language (parse → typeck) | F1 |
| F4 | Implicit partial application typing (D5) | Language (typeck) | F3 |
| F5 | Participation restrictions: variadic / `ref` / `ptr` / `unsafe` exemption (D7/D9) + inline-lowering reroute (D8) | typeck + lower | F4 |
| F6 | Anti-typo guard: discarded partial is an error (D10) | Language (typeck) | F4 |
| F7 | Copy semantics of partial values; capture deep-copy on copy paths (D11) | typeck + VM | F4 |
| F8 | `PartialApply` opcode + bytecode v8 (content-driven) + verify | compiler + verify | F4 |
| F9 | VM: captured function values, `CallValue` with captures, copy paths (D15) | VM | F8 |
| F10 | e2e matrix: per-file flag on/off × valid/invalid × cross-flag import; examples | tests | F1–F9 |
| F11 | Docs sync (DESIGN / SPEC / SYNTAX.ebnf / BYTECODE / CHANGELOG) | docs | F1–F10 |

---

## 3. Language design

### 3.1 Keywords and tokens

| Item | Status |
|------|--------|
| `#` | **New token** (`Hash`), unconditional. Not a keyword — a directive introducer. Comments (`//`, `/* */`) unchanged. |
| `curry` | A **feature name** (identifier in directive position), not a keyword. |
| — | **No new reserved keywords.** `func` is not reserved away in flag-off files; in flag-on files the legacy form is rejected by the *parser*, not by a lexer change. |

### 3.2 Syntax (EBNF additions, `SYNTAX.ebnf`)

```ebnf
(* 0.0.8 additions — feature directives + curried declarations *)

(* file structure: a header region of directives precedes all items *)
file            = { feature_directive } , { item } ;
feature_directive = "#" , "feature" , ident ;
(* any Hash token after the first item: FeatureDirectiveMisplaced  *)
(* unknown name: UnknownFeature; repeated name: DuplicateFeature   *)

(* flag-on (curry): two-part function declaration *)
curried_sig     = [ "pub" ] , [ modifier ] , ident , ":" , arrow_type ;
curried_body    = ident , "(" , [ ident , { "," , ident } ] , ")" ,
                  ( block | "=" , expr ) ;
modifier        = "async" | "unsafe" | "trusted" ;   (* signature line only *)

(* arrow function types: right-associative, ≥ 1 arrow *)
arrow_type      = func_atom , "->" , arrow_type ;
func_atom       = "(" , ")"          (* empty parameter list — only when  *)
                | "(" , type , ")"   (* immediately followed by "->" ;    *)
                | type ;             (* otherwise a plain parenthesized   *)
                                     (* type / grouping                   *)
(* zero-param: () -> int ;  unary over unit: unit -> int               *)
(* tuple sugar: ( t1 , t2 ) -> ret stays legal, canonicalizes to the   *)
(* same Type::Func as t1 -> t2 -> ret                                  *)
```

- The directive line is newline-terminated, takes no semicolon, and must
  be a whole line (existing same-line-join rules reject anything else).
- The signature line is single-line only (D3). The body may be a block or
  a `= expr` tail.
- `curried_body` is recognized at top level only, by the `Ident`-followed-
  by-`(` lookahead — top level has no call statements, so the production
  is unambiguous. Inside function bodies the two-part form is rejected
  (declarations are top-level only, as today).
- In a flag-on file, a bare top-level `name : type` is the signature
  production; if `type` is not a function type →
  `SignatureNeedsFunctionType`. In flag-off files the production does not
  exist (bare `name : type` with no `=` remains a syntax error, as today).
- Orphan halves: signature without a following body and body without a
  preceding signature are resolver-level errors (D3).

### 3.3 Type rules

- `Type::Func(params, ret)` is unchanged; the arrow chain is parse sugar;
  both spellings (D4) canonicalize to it.
- Partial application typing (D5): `f(a₁..aₖ)` with `k < n` against a
  participating callee → `Type::Func(remaining, ret)`; the typed call node
  records `partial: true` and the captured argument types.
- Participation check at the call site (D5/D7/D9): the callee must be
  fixed-arity, non-variadic, without `ref`/`ptr` parameters, and without
  the `unsafe`/`trusted` modifier; otherwise `k < n` stays
  `ArityMismatch`.
- `is_copy(Type::Func)` is unchanged (`true`) — partial values are Copy at
  the type level; the capture deep-copy is a runtime copy-path concern
  (D11).
- Argument ownership at a partial site is the full-call rule set
  (ownership.rs unchanged; owned locals still need explicit
  `move`/`clone`).
- Discarded-partial rule (D10): an expression statement whose top-level
  typed expression carries `partial: true` → `PartialApplicationDiscarded`.
- No new coercion, no subtyping (D6).

### 3.4 Declaration matching (resolver)

- Signatures register in the function table exactly as `func` declarations
  do (name, type, modifiers, visibility) — before any body is resolved, so
  recursion and forward reference work unchanged.
- Bodies are matched by name to their signature; arity must agree (D3).
- Cross-module: the export/import tables carry the signature (type +
  modifiers + visibility). An importing file's flag never affects what it
  sees; the canonical type is identical for both spellings.

---

## 4. std library changes

**None.** std is written under the `func` form, carries no `#feature`
directive, and is compiled flag-off in every project (per-file flags, D1).
std's own call sites are full-arity, so its bytecode contribution is
byte-identical to 0.0.7.

A flag-on file may partially apply std functions (`range_from(0)`,
`std.arr` helpers) — D8 covers the inline-lowering reroute those sites
need. No std source, signature, or intrinsic changes.

---

## 5. Compiler-stage changes

| Stage | Change |
|-------|--------|
| Lex | New `Hash` token (unconditional). No new keywords. |
| ASI | No new rules; directive lines precede all statements. Signature lines are single-line by grammar (D3). |
| Parse | `feature_directive` in the header region (placement/unknown/duplicate errors); `curried_sig` + `curried_body` productions, **flag-on only**; `FuncFormForbiddenInCurryFile` on the legacy form in flag-on files; `arrow_type` in type positions (flag-on only; tuple sugar unchanged). |
| Resolve | Signature/body matching by name (order enforced, D3); registration unchanged otherwise. |
| Typeck | Participation check + partial-application typing at call sites (D5/D7/D9); `partial` marker on the typed call node; discarded-partial rule (D10); arrow/tuple spelling rendering per file flag (D4). |
| Lower | Partial call sites (direct or indirect) lower to `LoadFunc` + args + `PartialApply k` (D13); inline-lowering builtins reroute on partial calls (D8); full-arity calls unchanged. |
| Codegen | `PartialApply` encoding (0xCE, u8 operand); `MODULE_VERSION = 8` **only when any `PartialApply` was emitted**, else 7 (content-driven, D13); `READ_VERSIONS = {1..8}`. |
| verify | Accepts v8; validates the `PartialApply` stack shape (pops `k+1`, pushes 1); v1–v7 checks unchanged. |

---

## 6. VM impact

- `Value::Func` grows the capture list: `Func { id: FuncId, captures:
  Vec<Value> }` (a bare function is the empty list). Display, `PayloadKind`
  mapping, and every `Value::Func` match site are updated.
- `CallValue`: consumes `captures.len() + argc`; the total must equal the
  callee's `params` (vm.rs:730 today requires `argc == params`). Bare
  functions behave exactly as today.
- Copy paths (`Dup`, array/struct/global stores) deep-copy a non-empty
  capture list (D11).
- `PartialApply k`: pops `f` and `k` arguments, pushes the captured
  function value.
- **Unconditional** — the VM is flag-blind (D15). v8 modules decode like
  v7 plus one opcode.
- No new runtime error kinds beyond the shape errors the new instructions
  need (defensive backstops, as with every instruction group).

---

## 7. Bytecode impact

- `PartialApply = 0xCE` — first slot of the reserved `0xCE–0xDF` group
  (§4.2); `u8` operand; no other opcode changes.
- Version: **content-driven** — 7 by default, 8 only when `PartialApply`
  was emitted (D13). Readers accept `{1..8}`; v1–v7 decode unchanged
  (append-only promise: one new opcode, no table change, no
  `FieldTypeTag`).
- §5 gains a `PartialApply` entry (§5.18); §10 versioning table gains the
  v8 row; §4.2 notes the slot allocation.
- Per-module, not per-file: one merged module; one partial call anywhere
  lifts the module to v8.

---

## 8. Explicitly out of scope (0.0.8)

| Item | Reason |
|------|--------|
| Closures (free variables, lambda syntax) | 0.1.0. A partial captures only its own call-site arguments; there is no enclosing-scope capture, no `(x) => …` form |
| Explicit partial-application syntax (`partial(f, 1)`, `f(1, _)` placeholders) | The design is implicit-under-flag (D5); explicit forms can lower to the same `PartialApply` later without changing it |
| Project-level / package-level flags (`flint.toml [features]`) | Per-file is the requirement (D1); the manifest's table whitelist stays untouched |
| Currying variadic builtins | D7 — no remainder type exists |
| Partial application of `ref`/`ptr`/`unsafe` functions | D9 — permanent exemption, not a TODO |
| Representational currying (nested unary frames) | D4 — notation + typing rule only; the calling convention is untouched |
| A second feature beyond `curry` | The directive machinery is general; no second feature is specified in 0.0.8 |
| Comma-list `#feature a, b` | One name per line; trivially additive later |
| Per-file `.flnc` outputs / per-file bytecode versions | The merged-module model stays; v8 is per-module (D13) |
| Function-level generics / traits / operator overloading | 0.1.0 (unchanged) |
| Recursive enums, `Result` → std migration | Unchanged carries (0.0.7 D3 / D9) |
| `clone` / `move` on partial values as new syntax | Partials are Copy (D11) — the keywords stay rejected on Copy types, as today |

---

## 9. Test matrix (planned)

| Area | Coverage |
|------|----------|
| Lex | `Hash` token; comments unchanged; `#` in flag-off files lexes fine and fails at parse (no directive production there) |
| Parser | Directive placement (first, after comments, after an item → `FeatureDirectiveMisplaced`, inside a body → error); unknown / duplicate feature; two-part declarations (0/1/2 params, `= expr` body, modifiers on the signature, `pub` on the signature); strict form rejection (`func` in a flag-on file); arrow types (1/2/3-deep chains, right-assoc grouping, `() -> T`, `unit -> T`, tuple sugar); invalid (signature without body, body without signature, arity mismatch, `pub` on body, modifier on body, multi-line signature) |
| Resolver | Signature/body matching; forward reference and recursion in flag-on files; cross-module export of curried declarations; `BodyWithoutSignature` / `SignatureWithoutBody` / `SignatureArityMismatch` |
| Typeck | Partial typing (`add(1) : int -> int`); degenerate `k = 0`; `k > n` `ArityMismatch`; participation exemptions (variadic `print`, `ref`-param, `ptr`-param, `unsafe`/`trusted`); ownership at partial sites (`move`/`clone` for owned args); discarded partial (`PartialApplicationDiscarded`); genuine unary function as statement (legal, unchanged); cross-flag import both directions; exact-match argument passing (a partial does not satisfy the full type) |
| Lower | Partial call → `LoadFunc` + args + `PartialApply k` (direct and indirect callees); full-arity call → `Call` unchanged; `range_from(0)` partial skips `MakeRange` inline; `map`/`filter` partial sites skip `InlineIter` |
| Bytecode | v7 emitted for a flag-on file without partials (content-driven bump); v8 emitted with `PartialApply`; v1–v7 fixtures decode unchanged; v8 round-trip; `PartialApply` operand encoding |
| VM | Bare function behavior unchanged; captured function called via `CallValue` (`captures + argc == params`); copy paths deep-copy captures (array store, struct field, global, `Dup`); two independent copies consumed independently |
| E2E valid | `curry_demo.fln` (flag-on: two-part declarations, partial application, std imports); `partial_flow.fln` (partial passed through a flag-off higher-order helper and called there) |
| E2E invalid | Flag-off file: too-few-args `ArityMismatch` (unchanged), `#` mid-file; flag-on file: `FuncFormForbiddenInCurryFile`, discarded partial, partial of `unsafe`/`ref`/`ptr` function, `UnknownFeature` |
| Docs | SYNTAX.ebnf additions; BYTECODE §4.2/§5/§10; DESIGN §14/§18; SPEC §13 feature matrix + §14 examples |

---

## 10. Versioning

- Workspace `Cargo.toml` → `0.0.8`; `std/VERSION` → `0.0.8`.
- Bytecode `MODULE_VERSION`: writers emit **7 or 8 by content** (D13);
  `READ_VERSIONS = {1..8}`. This is the first release where the emitted
  version depends on what the source uses, not on the release itself.
- CHANGELOG gains the `0.0.8` entry; **non-breaking additive** (D16) —
  opt-in per file, byte-identical without the directive.
- SPEC §13 gains the 0.0.8 goal paragraph and feature-matrix rows
  (`#feature` / currying); DESIGN §14 gains the curried-declaration and
  partial-application sections, §18 roadmap inserts the 0.0.8 row;
  SYNTAX.ebnf gains the directive and two-part productions; BYTECODE.md
  gains §5.18 and the v8 row.
