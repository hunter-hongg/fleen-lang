# 0.0.7 — Parameterized Types + Pattern Matching Completion

> **Codename**: Mantis Shrimp 🦐 (0.0.1 Swift Fox 🦊, 0.0.4 Patient Ants 🐜,
> 0.0.5 Careful Hedgehog 🦔, 0.0.6 Busy Beaver 🦫).
>
> **Goal**: let Fleen users declare **parameterized enums** — a type constructor
> that takes type arguments and must be fully applied at every use site
> (`Option<int>`, `Pair[string, int]`) — and finish the pattern-matching story
> the 0.0.6 enum deliberately left partial: full multi-payload destructuring,
> **nested patterns**, **or-patterns**, and **type guards**. One coherent
> theme: *ADT + pattern matching land together.*
>
> **Theme**: two gaps closed in one release. The **type** gap: 0.0.6 enums
> are monomorphic — every payload type is written literally at the
> declaration, so `enum Option { Some(int), None }` cannot abstract over the
> payload. The **pattern** gap: the 0.0.6 pattern grammar binds **one**
> payload per arm (`Ok(x)` — PLAN 0.0.6 D3 kept the one-binding limit), cannot
> nest, cannot branch (`a | b`), and cannot refine (`x: T`). 0.0.7 lifts both
> at once, because a parameterized enum you can construct but not fully
> destructure is a half-feature.
>
> **Status**: planning. Decisions D1–D10 below are settled at planning level;
> each names the alternative it rejected and what the choice locks. They may
> be revised only by a new entry here, not silently in a ticket card.
>
> **Monads are explicitly NOT here.** No `Functor` / `Monad` / `map` /
> `filter` over parameterized enums — those need closures or function-level
> generics (see §8), and this release owns neither.

---

## 0. Relationship to adjacent versions

| Version | Scope |
|---------|-------|
| 0.0.6 | complete (release published — tag and archives cut) |
| **0.0.7** | **parameterized enums (user-defined type constructors) + multi-payload destructuring + nested patterns + or-patterns + type guards** |
| 0.1.0 | function-level generics / traits / operator overloading / closures; recursive types revisit; extended string escapes |

The parameterized-enum layer is a **slice of generics**, not generics
themselves: only **enum declarations** take type parameters, and only in the
head position (`enum Option<T> { … }`). Functions still do not take type
parameters (`func id<T>(x: T): T` is still 0.1.0); traits and operator
overloading are untouched. The pattern enhancements are orthogonal and land
with the parameterized enums because the full multi-payload destructuring
that parameterized enums want is exactly the same work.

Recursive enums (self-referential payloads, directly or via `box` / `[T]`)
remain **excluded** — D3 carries 0.0.6 D2 forward unchanged.

---

## 1. Settled decisions (D1–D10)

### D1 — Parameterized enums: user-defined type constructors, fully-applied at every use

```fleen
enum Option<T> {
    Some(T),
    None,
}

enum Pair<A, B> {
    Pair(A, B),
}

// Use sites must fill every parameter — no bare `Option` as a type:
func find(x: int, xs: [int]): Option<int> {
    for y in xs {
        if y == x { return Option.Some(y); };
    };
    Option.None
}

let p: Pair[string, int] = Pair.Pair("age", 42);
```

- An `enum` may declare a list of **type parameters** in the head:
  `enum Name<T> { … }`, `enum Name<A, B> { … }`. The names live in a
  **declaration-local type-parameter scope** — they are not values, not
  runtime entities, and not visible outside the declaration.
- A **type constructor must be fully applied at every use site**. `Option`
  alone (bare, in type position) is a compile error
  (`TypeConstructorNeedsArgs`); `Option<int>` is a type. There is **no**
  implicit instantiation, no partial application, no higher-kinded use.
- Inside the declaration body, the parameters are used bare: `Some(T)` refers
  to the parameter `T`, not to a type named `T`.
- **Type arguments are types** (`int`, `string`, `[T]`, `Result[int, string]`,
  another parameterized enum application `Option<Pair[A, B]>`), not
  expressions, not integer constants, not values. This is the same shape the
  builtin `Result[T, E]` has always had — see D9.
- *Rejected*: implicit parameter inference at use sites ("write `Option` and
  let the compiler figure it out") — it makes every error message ambiguous
  and forces a unification pass across the whole program; the explicit
  `Option<int>` spelling is the same posture `Result[T, E]` already takes.
- *Rejected*: requiring a keyword to distinguish "parameterized enum" from
  "monomorphic enum" (`parameterized enum`, `type`, `data`) — the head
  parameter list is the syntax; the declaration shape tells you which you
  have.
- *Locks*: enum names in the module table carry an **arity**; every use-site
  type application locks the arity at registration; a later change to
  "partial application allowed" would be a breaking change, accepted here as
  0.0.x.

### D2 — `is_copy` becomes parameter-dependent (a function, not a constant)

- 0.0.6 D1 fixed `is_copy(E) = every payload of every variant is Copy` — a
  constant for a monomorphic enum. With parameters, the answer depends on the
  arguments: `Option<int>` is Copy, `Option<string>` is not.
- New rule: `is_copy(App(T1, …, Tn))` is computed by **substituting the
  arguments into the payload types and re-evaluating the payload-Copy rule**.
  The declaration-time `is_copy` flag on `EnumInfo` becomes a **function of
  the arguments**, not a bit.
- Concretely:
  - `Option<int>`: payload `int` is Copy ⇒ Copy.
  - `Option<string>`: payload `string` is owned ⇒ owned; `move` / `clone`
    needed at consuming positions.
  - `Pair[string, int]`: any owned argument ⇒ owned.
- Non-parameterized enums keep the 0.0.6 constant rule as the `n = 0` case.
- *Rejected*: keeping `is_copy` as a declaration-time constant by
  "conservatively owned if any payload mentions a parameter" — it would make
  `Option<int>` needlessly owned, break the struct rule's promise that Copy is
  computed from field/payload types, and force users to `move` values that
  are safe to copy.
- *Rejected*: a `Copy` trait or `impl` declaration the user writes to opt in —
  traits are 0.1.0.
- *Locks*: the `is_copy` signature in typeck (arguments in, bool out);
  ownership check (`typeck/ownership.rs`) must call it after substitution.

### D3 — Recursive-parameter ban is unchanged (`RecursiveEnum` carries forward)

- A payload type may not (transitively) reach the enum being declared —
  directly, via `box`, or via `[T]` — exactly as 0.0.6 D2 rules. The
  parameterization does **not** change this: `enum List<T> { Nil, Cons(T,
  List<T>) }` is rejected at typeck (`RecursiveEnum` with the full path),
  because `List<T>` in the payload reaches `List`.
- What *is* legal: the parameter appears in a payload (`Some(T)`); the
  parameter appears in another parameterized enum application
  (`Wrap(Option<T>)`) as long as the outer enum is not reached.
- *Rejected*: lifting the ban alongside parameterization (letting
  `List<T>` work because the parameter "looks like it should make
  recursion safe") — the heap-representation and drop-order questions
  0.0.6 D2 records are still open, and parameterization does not answer
  them. Revisit at 0.1.0 with recursive types generally.
- *Locks*: nothing structural — same reasoning as 0.0.6 D2, cheap to
  reverse later.

### D4 — Multi-payload destructuring in patterns: the 0.0.6 "one-binding" limit is lifted

- 0.0.6 D3 / PLAN 0.0.6 §3.2 kept patterns at the `Ok(x)` one-binding level:
  `Circle(r)` binds one payload; a 2+ payload variant matched positionally
  but only the first payload was reachable. 0.0.7 lifts this.
- New pattern shape: `Variant(p₁, p₂, …, pₙ)` binds **every** payload, in
  declaration order. Each `pᵢ` is itself a **full pattern** (D5), so
  `Pair(a, b)`, `Some(Some(x))`, and `Some(_)`, `Pair(a, _)` all work.
- A variant pattern must name exactly as many sub-patterns as the variant
  has payloads (`ArityMismatchInPattern`).
- Zero-payload variants match bare (`Point`, not `Point()`).
- *Rejected*: keeping the one-binding limit and adding a `when Circle(r) {
  let b = payload_read(1); … }` escape hatch — 0.0.6 T05's as-built note
  already flags "no user-visible 'read payload i' expression" as the reason
  the one-binding limit exists; adding a payload-read expression would be a
  parallel mechanism for the same job. Patterns are the mechanism.
- *Rejected*: named-field patterns (`when Circle { radius: r }`) — enum
  variants are positional (D1 in 0.0.6); introducing names would be a
  second variant grammar.
- *Locks*: the `Pattern::Enum` payload field becomes `Vec<Pattern>`; the
  HIR / lowering for a multi-binding arm is locked by D8.

### D5 — Nested patterns: `Pattern` becomes recursive

- Every sub-pattern position accepts any pattern, including another
  constructor pattern: `Some(Some(x))`, `Ok(Pair(a, b))`, `Some(None)`.
- Depth is bounded by the type — typeck walks the scrutinee type alongside
  the pattern and reports `PatternTypeMismatch` / `EnumPatternMismatch` (the
  0.0.6 T05 diagnostic, generalized) when a sub-pattern's constructor does
  not match the payload type at that position.
- **Bindings collected from all nesting levels** are declared in the arm
  scope; a duplicate binding name across levels is `DuplicateBinding` (same
  rule as 0.0.2 shadowing).
- *Rejected*: flattening nested patterns into a dot-form (`when
  Some.x.y`) — it breaks the constructor-pattern shape the 0.0.6 grammar
  already established and does not generalize to or-patterns (D6).
- *Locks*: the pattern grammar is recursive from T02 onward; lowering is a
  recursive walk (D8).

### D6 — Or-patterns: `p₁ | p₂` matches either, with shared bindings

```fleen
choose x {
    when 1 | 2 { "small" }
    when Some(v) | Other(v) { "has v" }
    otherwise { "other" }
}
```

- `p₁ | p₂` matches when either alternative matches. Alternatives are full
  patterns.
- **All alternatives must bind the same set of names with the same types**
  (`OrPatternBindingMismatch`); the binding is the one the matching
  alternative produces.
- Exhaustiveness: an or-pattern counts as covering the union of its
  alternatives' coverage. `when 1 | 2` covers `{1, 2}`, not all of `int`;
  `when Some(_) | None` covers all of `Option<T>`.
- Precedence: `|` binds looser than a constructor application, tighter than
  the arm-level `if` guard (`when p₁ | p₂ if guard { … }`).
- *Rejected*: requiring parentheses around every or-pattern — visual noise;
  the precedence rules are enough.
- *Rejected*: allowing different bindings per alternative (`when Some(x) |
  None` where `x` is absent in the second) — the arm body cannot know which
  alternative matched; this is exactly Haskell's / Rust's rule.
- *Locks*: `|` is **not** a reserved word — it is only meaningful inside a
  pattern, so no lexer-level keyword reservation; but the `|` token in
  pattern position becomes significant (it currently has no pattern meaning).

### D7 — Type guards: `when pattern: T`

```fleen
choose x {
    when v: int { "int: " + (v as string) }   // v: int binding
    when v: string { v }                       // v: string binding
    otherwise { "other" }
}
```

- A pattern may carry a **type ascription**: `pattern: T`. The pattern
  matches values whose type is (compatible with) `T`, and any bindings the
  pattern produces are ascribed type `T`.
- For a bare identifier (`v: int`) this is a typed binding: `v` matches any
  value of type `int`.
- For a constructor pattern (`Some(v): Option<int>`) the ascription must be
  the scrutinee's type; typeck checks compatibility and reports
  `PatternTypeMismatch` if it fails. This is mostly a documentation /
  exhaustiveness aid on constructor patterns.
- **This is NOT a runtime type test.** Fleen is statically typed — every
  value already has one type. A type guard only documents/refines the type
  the pattern binds; it never inspects the value at runtime. (Contrast with
  dynamic languages' `when x is T`.)
- *Rejected*: a runtime type test on a value (`when x: T` matching by
  runtime tag) — Fleen values do not carry a runtime type tag except enum
  discriminants, and using the discriminant for a "type test" would
  conflate the enum machinery with a nominal-type check.
- *Rejected*: making the type guard the primary way to bind from an enum
  payload (`when payload: T`) — constructor patterns (D4) are the mechanism;
  type guards are for the identifier case and for explicit type refinement.
- *Locks*: `:` in pattern position introduces a type ascription; the
  pattern grammar gains a `pattern ":" type` production.

### D8 — Bytecode: version 7, one appended field to `enum_defs`; no new opcodes, no `FieldTypeTag` change

Numbers below are checked against `BYTECODE.md` §4.2 / §2 / §9 (current
`FieldTypeTag` ends at `Enum = 12`; current `.flnc` version is 6).

- **No new opcodes.** `EnumNew` / `EnumIs` / `EnumPayload` (0xCB–0xCD) are
  unchanged: a parameterized enum's runtime value is still
  `Value::Enum { variant, payload }` and the parameter is a compile-time
  fact. All of D1–D7 is typeck / pattern / lowering work.
- The `.flnc` version bumps to **7** because `enum_defs` needs to carry the
  parameter arity per enum (so verify can sanity-check the declaration-side
  metadata and so a future tool can reconstruct the type). Readers accept
  `{1..7}`; v1–v6 fixtures are byte-identical.
- `enum_defs` (v7 layout) gains one field per enum entry, after
  `variant_count`:
  - `type_param_count: u32` — 0 for a monomorphic enum.
  - (The parameter *names* are not serialized — they are declaration-local
    and have no runtime meaning. Only the arity is.)
- `FieldTypeTag` is unchanged (no new tag; parameters do not appear in
  disk payload types — they are substituted away before codegen).
- *Rejected*: skipping the bytecode bump and not serializing the arity —
  the arity is a public fact of the type (a caller in another module needs
  it to apply the constructor), and `enum_defs` is the natural place for it.
  A "no disk format change" shortcut would push the arity into a side
  channel.
- *Rejected*: adding a `TypeConstructorApply` opcode — the parameter is
  compile-time, the runtime value shape is unchanged.
- *Locks*: version 7 encoding of `enum_defs` (the new u32 slot), readers
  accept `{1..7}`. Everything else in the v1–v6 layout is frozen per
  BYTECODE §10.

### D9 — `Result[T, E]` syntax is retained unchanged; whether `Result` becomes a std enum is deferred

- The user prefers the bracket form `Result[T, E]` for parameterized types.
  **For 0.0.7, user-defined parameterized enums use angle brackets**
  (`Option<T>`, `Pair[A, B]` — see EBNF §3.2; both `<>` and `[]` are
  accepted at use sites, mirroring the existing builtin spellings).
- **`Result` itself does not move to std in 0.0.7.** Making `Result` a
  user-defined `enum Result<T, E> { Ok(T), Err(E) }` would require
  re-plumbing `?` propagation, the `Ok` / `Err` patterns, `Result` in
  `main`'s return, and the `Result[T, E]` type-spelling — all to a
  user-defined enum. That is a coherent 0.1.0 cleanup when traits land and
  `Result` needs a `Monad`-style interface anyway; doing it now would
  dominate this release.
- Consequence: 0.0.7 has **two** parameterized-enum spellings in the
  language — builtin `Result[T, E]` and user-defined `Option<T>` /
  `Pair[A, B]`. This inconsistency is **accepted as 0.0.x debt** and
  recorded here so it is a decision, not an accident.
- *Rejected*: doing the `Result` → std migration in 0.0.7 — the `?` /
  pattern / `main` return / spelling re-plumbing is a release-sized chunk on
  its own; folding it into the type-constructor work makes both harder to
  land green. Defer to 0.1.0 with a written note in §8.
- *Rejected*: forbidding user parameterized enums to use the bracket
  spelling `[]` (keeping only `<>` for users) — arbitrary; both spellings
  already exist for builtins.
- *Locks*: both spellings are legal at use sites from T02; the `Result` →
  std migration is recorded as an open item for 0.1.0 in §8.

### D10 — Breaking-change posture: `enum`-declaration params, no deprecation shims

- The only source-breaking change is that an enum declaration may now carry
  a parameter list — **this is additive**, not breaking; no existing program
  changes meaning.
- The new pattern syntax (`p₁ | p₂`, `pattern: T`, multi-binding
  `Variant(a, b)`) is **additive** — no existing pattern changes meaning.
- The one subtle behavior change: `is_copy` on a parameterized enum is now
  argument-dependent (D2). No existing program is affected (no existing enum
  has parameters).
- No deprecation aliases, no opt-out flags. 0.0.x is the unstable-API window
  (SPEC §13).
- *Locks*: the CHANGELOG entry (T11) records this release as **non-breaking
  additive** — a first for the 0.0.x line.

---

## 2. Feature list

| ID | Feature | Layer | Depends on |
|----|---------|-------|------------|
| F1 | Type parameters on `enum` declarations + fully-applied use-site types | Language (parse → typeck) | — |
| F2 | Parameterized constructor typing + argument-dependent `is_copy` | Language (typeck) | F1 |
| F3 | Multi-payload destructuring in variant patterns | Language (parse → typeck → lower) | — |
| F4 | Nested patterns (recursive `Pattern`) | Language (parse → typeck → lower) | F3 |
| F5 | Or-patterns `p₁ \| p₂` | Language (parse → typeck → lower) | F3 |
| F6 | Type guards `pattern: T` | Language (parse → typeck) | F3 |
| F7 | Exhaustiveness with or-patterns + full multi-payload arms | Language (typeck) | F3, F5 |
| F8 | Bytecode v7 (`enum_defs` arity field) + codegen + verify | compiler + verify | F1 |
| F9 | Docs sync (DESIGN / SPEC / SYNTAX.ebnf / BYTECODE / CHANGELOG) | docs | F1–F8 |

---

## 3. Language design

### 3.1 Keywords

| Word | Status |
|------|--------|
| — | **No new reserved keywords.** `enum` is already reserved (0.0.6). Type parameter names are declaration-local identifiers, not keywords. `|` and `:` gain meaning in pattern position only — no lexer changes. |

### 3.2 Syntax (EBNF additions, `SYNTAX.ebnf`)

```ebnf
(* 0.0.7 additions — parameterized enum declarations + pattern enhancements *)

(* declaration: the head gains an optional parameter list *)
enum_decl       = "enum" , ident , [ type_params ] , "{" ,
                  variant , { "," , variant } , [ "," ] , "}" ;
type_params     = "<" , ident , { "," , ident } , [ "," ] , ">" ;
variant         = ident , [ "(" , type , { "," , type } , ")" ] ;

(* types: an enum name may be applied to type arguments *)
type            = ... | named_type ;       (* existing *)
named_type      = ident , [ type_args ] ;
type_args       = ( "<" , type , { "," , type } , ">" )
                | ( "[" , type , { "," , type } , "]" ) ;

(* pattern: recursive, with alternatives and type ascription *)
pattern         = or_pattern ;
or_pattern      = typed_pattern , { "|" , typed_pattern } ;
typed_pattern   = constructor_pattern , [ ":" , type ] ;
constructor_pattern =
                  literal_pattern
                | ident_pattern
                | ident , "(" , [ pattern , { "," , pattern } ] , ")" ;  (* Variant(p₁, …, pₙ) *)
ident_pattern   = ident | "_" ;           (* binding or wildcard *)
```

- Declaration parameters are **identifiers only** — no defaults, no bounds,
  no associated types.
- Type arguments at use sites: both `<…>` (Rust / ML style) and `[…]`
  (Fleen's existing `Result[T, E]` spelling) are accepted, per D9.
- Patterns are **recursive** (D5). `pattern` is the new top of the pattern
  grammar; the 0.0.6 `enum_pattern` and `result_pattern` productions become
  special cases of `constructor_pattern`.
- Or-pattern precedence (D6): `|` binds looser than a constructor
  application, tighter than the arm-level `if` guard.
- Type guard (D7): `:` binds tighter than `|` (`when a: T | b: U` parses as
  `(a: T) | (b: U)`).
- Trailing comma allowed in parameter lists and in variant pattern
  sub-pattern lists (mirror the declaration).

### 3.3 Type rules

- A parameterized enum's name carries an **arity** (0 for a monomorphic
  enum). Use-site types must supply exactly that many arguments
  (`TypeConstructorNeedsArgs` if too few, `ArityMismatch` if too many).
- Type arguments are checked for well-formedness (no `ptr<T>` where storage
  is not allowed, no `RecursiveEnum` cycle, no undeclared parameter names).
- Inside the declaration body, payload types may mention the parameters;
  outside, every parameter must be substituted.
- Constructor expressions: the constructor's enum is the fully-applied type
  — `Option.Some(1)` infers `Option<int>` from the payload (the parameter is
  determined by the argument, not stated at the constructor site).
- **Parameter determination at constructor sites**: when the parameter
  appears only in the payload (`Some(T)`), the argument's type fixes it.
  When a parameter appears in a variant with no payload (rare but legal —
  `enum Weird<T> { Empty(T), Nothing }`), a bare `Weird.Nothing` is
  ambiguous; the surrounding context must fix `T` (annotated `let x:
  Weird<int> = Weird.Nothing;`) or `TypeParameterAmbiguous` is reported.
- `is_copy(App(T₁, …, Tₙ))`: substitute and re-evaluate (D2).
- Storage / payload rules: unchanged from 0.0.6 D1 (ptr rejected, Future /
  channel inherit boundary rules, arrays and nested parameterized enums
  legal).
- No `==`, no `as string` on parameterized enums (same as monomorphic).

### 3.4 Pattern matching & exhaustiveness

- Scrutinee type `App(E, T₁, …, Tₙ)`: enumerate the declared variants;
  substitute the parameters into each variant's payload types; typecheck
  each arm's pattern against the substituted variant.
- **Multi-payload arms** (D4): each sub-pattern typechecks against its
  payload type; bindings are collected with their substituted types.
- **Nested arms** (D5): recursive walk; each level collects bindings.
- **Or-arms** (D6): typecheck each alternative; require identical binding
  name sets with identical types; the arm's coverage is the union.
- **Type-guard arms** (D7): check the ascription against the scrutinee;
  bindings carry the ascribed type.
- **Exhaustiveness**: guard-free arms covering **all variants** of the
  scrutinee enum are exhaustive. Or-patterns contribute the union of their
  alternatives. The missing-variants report (`ChooseNotExhaustive`)
  enumerates the uncovered variants by name. Guards never count; `otherwise`
  short-circuits as today.
- **Lowering** (generalizes the 0.0.6 T05 shape):
  - The scrutinee is parked on the stack for the whole `choose`.
  - A multi-payload arm lowers as a chain of `EnumIs v; JumpIfTrue body`
    tests (one per alternative for or-patterns), and the body block starts
    with one `EnumPayload v i; StoreLocal xᵢ` per binding.
  - Nested patterns lower as a *chain*: the outer constructor test
    succeeds, the outer payload is extracted and **re-parked**, and the
    inner pattern becomes a new `choose`-shaped test on the parked payload.
    This is the same shape `Ok(Some(x))` would have if `Result` were
    recursive — and 0.0.7 makes that work.
  - Guard arms stage payloads as in 0.0.6 T05 (move out of a `Dup` copy
    into the binding's slot before the guard; jump to the next test on
    failure).
  - Every edge is resolved immediately (0.0.6 T05 rule — no deferred
    jumps).

---

## 4. std library changes

**None.** 0.0.7 adds no std functions and no intrinsics. The parameterized
enum work is purely language-level; the pattern enhancements are purely
language-level.

The std's own `Result[T, E]` spelling is unchanged (D9); std does not yet
define any user-facing parameterized enum of its own.

---

## 5. Compiler-stage changes

| Stage | Change |
|-------|--------|
| Lex | No new keywords. `|` and `:` tokens already exist; they gain meaning in pattern position (no lexer change). |
| Parse | `type_params` in `enum_decl`; `type_args` in `named_type`; recursive `pattern` grammar (replaces flat `enum_pattern` / `result_pattern` special cases; `Ok` / `Err` keep their `ResultCtor` shortcut at the parser level and are normalized in typeck) |
| Resolve | Type parameters register in a declaration-local scope; use-site type applications resolve the enum name and check arity; parameterized enum names join the module export/import machinery as before, now with arity |
| Typeck | Type-parameter substitution table; fully-applied-use check; argument-dependent `is_copy`; constructor argument-determination; pattern recursive typecheck; or-pattern binding consistency; type-guard compatibility; exhaustiveness with or-patterns |
| Lower | Multi-binding arms; nested-pattern chains; or-pattern alternative tests; type guards lower as the pattern they refine (no runtime work) |
| Codegen | `enum_defs` emits the per-enum `type_param_count: u32` (D8); `MODULE_VERSION = 7`; `READ_VERSIONS = {1..7}` |
| verify | `enum_defs` v7 reader (one more u32 per enum); no new cross-checks beyond that |

---

## 6. VM impact

**None.** The runtime value shape `Value::Enum { variant, payload }` is
unchanged. All new opcodes are absent — `EnumNew` / `EnumIs` /
`EnumPayload` are the same instructions with the same encodings and the same
stack effects. The v7 `.flnc` is decoded by the existing v6 VM reader with
one extra u32 per enum skipped / validated in the `enum_defs` reader.

Defensive errors `EnumIndexOutOfRange` / `EnumVariantOutOfRange` /
`EnumPayloadMismatch` are unchanged.

---

## 7. Bytecode impact

- Version 6 → **7**; readers accept `{1..7}`; v1–v6 fixtures unchanged
  (append-only rules respected: one new u32 per enum in `enum_defs`, no new
  opcodes, no new `FieldTypeTag`).
- `§2 enum_defs` gains the `type_param_count: u32` field; `§9` layout
  updated; `§10` versioning table gains the v7 row.
- No new §5.x sections — no new opcodes.

---

## 8. Explicitly out of scope (0.0.7)

| Item | Reason |
|------|--------|
| Function-level type parameters (`func id<T>(x: T): T`) | This release parameterizes enums only; functions wait for 0.1.0 |
| Traits / `impl` blocks / operator overloading | 0.1.0 |
| Closures (`(A, T) -> A` first-class lambdas) | 0.1.0 |
| `Functor` / `Monad` / `map` / `filter` / `bind` over parameterized enums | Needs closures or function-level generics; 0.1.0 (carries 0.0.6 §8 forward) |
| Recursive parameterized enums (`enum List<T> { Cons(T, List<T>) }`) | D3 — carries 0.0.6 D2 forward; heap-representation / drop-order question still open |
| `Result` migrated to a std enum (`enum Result<T, E>`) | D9 — deferred to 0.1.0 with traits; doing it here dominates the release |
| Higher-kinded types (`enum Functor<F, T>`) | 0.1.0 at the earliest; likely never |
| Default type parameters (`enum Option<T = int>`) | Not standard; explicit is better |
| Type-parameter bounds (`enum Ord<T: Compare>`) | Needs traits; 0.1.0 |
| Struct parameterization (`struct Pair<A, B>`) | Could be added symmetrically; not in this release to keep scope bounded — revisit at 0.1.0 alongside struct traits |
| Pattern macros / pattern aliases (`when Foo(x)` shorthand for a fixed nested pattern) | Needs a macro system; not scheduled |
| Struct / array destructuring in patterns (`when Point { x, y }` / `when [a, b]`) | Carries 0.0.3 §8 forward; a coherent future release on its own |
| Guard inside a sub-pattern (`when Some(x if x > 0)`) | Guards stay clause-level (`when Some(x) if x > 0`), as today |
| Runtime type tags on non-enum values | Fleen is statically typed; no runtime type tests outside enum discriminants (D7) |

---

## 9. Test matrix (planned)

| Area | Coverage |
|------|----------|
| Lex | No new keyword tokens; `|` and `:` in pattern position only (no lexer change verified) |
| Parser | Parameterized enum decls (1-param, 2-param, trailing comma); use-site `<T>` and `[T]` spellings; nested constructor patterns; or-patterns; type guards; multi-binding arms; invalid (bare `Option` as a type, arity mismatch in use, arity mismatch in pattern, orphan `|`, orphan `:`, mismatched or-pattern bindings) |
| Resolver | Parameterized enum import / pub; arity preserved cross-module; dotted constructor paths on parameterized enums |
| Typeck | Parameter determination at constructors (from payload, from context for empty-payload variants); argument-dependent `is_copy` (`Option<int>` Copy, `Option<string>` owned); fully-applied-use check; `RecursiveEnum` with parameters (`List<T>` rejected); pattern recursive typecheck; or-pattern binding consistency; type-guard compatibility; exhaustiveness with or-patterns and multi-binding arms |
| Lower | Multi-binding arm shape (one `EnumPayload ; StoreLocal` per binding); nested-pattern chain (outer test, re-park, inner test); or-pattern alternative tests; guard staging unchanged |
| E2E valid | `option_demo.fln` (`Option<int>` / `Option<string>`, construct + choose + payload use); `pair_demo.fln` (multi-payload destructuring); `nested_pattern.fln` (`Ok(Some(x))`-style); `or_pattern.fln`; `type_guard.fln` |
| E2E invalid | Bare type constructor in use (`TypeConstructorNeedsArgs`); arity mismatch at use and at pattern; `RecursiveEnum` with parameter; or-pattern mismatched bindings; type-guard mismatch |
| Bytecode | v7 write / read round-trip; v1–v6 fixture decode unchanged; parameterized `enum_defs` arity field round-trips |

---

## 10. Versioning

- Workspace `Cargo.toml` → `0.0.7`; `std/VERSION` → `0.0.7` (strict pinning
  keeps matching).
- Bytecode `MODULE_VERSION = 7`; `READ_VERSIONS` gains 7.
- CHANGELOG gains the `0.0.7` entry; **non-breaking additive** release (D10)
  — a first for the 0.0.x line.
- SPEC §13 gains the 0.0.7 goal paragraph; DESIGN §9 enum section gains the
  parameterized-enum subsection; §17 feature table gains parameterized-enum
  and pattern-enhancement rows; §18 roadmap inserts the 0.0.7 row.
