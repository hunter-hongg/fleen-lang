# Fleen 0.0.2 Feature Plan (Ownership Foundations)

> **Fast, simple, safe, intuitive**
> **Theme**: The beginning of the ownership system — heap values get owners, errors start to flow, semicolons can retire.
> **Basis**: The roadmap in `DESIGN.md §18`, `BYTECODE.md` (§3.2 value representation, §4.2 opcode allocation, §10 versioning rules),
> the known-Hack list in `DESIGN.md §18`, and the current implementation state in `docs/0.0.1/TICKETS.md`.
> **Status**: Finalized (2026-10-06). All spec documents have been synced per §11
> (BYTECODE / DESIGN / SYNTAX / SPEC / CHANGELOG / README);
> implementation starts once the work is broken down into TICKETS per §10. Until it lands, the current toolchain remains 0.0.1 behavior.

---

## 1. Overview

### 1.1 The three questions 0.0.2 answers

1. **Who owns heap values?** — `string` and `box<T>` get a single owner; `move` transfers, `clone` copies, `ref` borrows.
   This is the first landing of the "no GC, ownership + borrowing" memory model (`DESIGN.md §1`).
2. **How do errors flow?** — `Result<T, E>` gets constructor syntax (`Ok` / `Err`), a `choose` pattern, and `?` propagation,
   going from "a type with no values" to a usable error-handling mechanism.
3. **Do we still write semicolons?** — ASI (automatic semicolon insertion) lands, `;` becomes optional, and 0.0.1 programs are unaffected.

### 1.2 Feature list (from the `DESIGN.md §18` roadmap)

| # | Feature | Category | Prerequisites it depends on |
|---|------|------|----------------|
| F1 | `move` / `clone` (ownership) | Language + compiler + VM | Ownership checker (new), bytecode v2 instructions |
| F2 | `box<T>` | Language + compiler + VM | Ownership, `deref` keyword, `AllocBox` and related instructions |
| F3 | `ref` (read-only borrow, function parameters only) | Language + compiler + VM | Ownership, borrow-handle value representation |
| F4 | `?` operator | Language + compiler + VM | **`Ok`/`Err` constructors and the choose pattern** (an implicit extension, see §3.4), Result instructions |
| F5 | ASI (automatic semicolon insertion) | Compiler (Parser) | None |
| F6 | `print` builtin type-check fix (strict string checking, see §6) | typeck | **F8** (non-string args need `as string`) |
| F7 | `span_map` generation + runtime diagnostics | Codegen + VM | `SpanEntry` design (an item BYTECODE.md deferred to 0.0.2) |
| F8 | `as` type cast (a 0.0.2 temporary feature, `<value> as T`, see §3.6) | Language + compiler + VM | None |

> **Scope note**: `DESIGN.md §18` only lists six items: `move / clone / box<T> / ref / ? / ASI`.
> But without `Ok(v)` / `Err(e)` constructor syntax and the `Ok`/`Err` pattern for `choose`,
> user code **cannot construct any `Result` value at all**, making `?` a dead feature
> (the 0.0.1 state: `Result<T, E>` has a type, no values, no pattern — see the T05 note in the 0.0.1 `TICKETS.md`).
> Therefore §3.4 folds Ok/Err constructors + the Result pattern in as part of F4.
>
> **F8 is a temporary addition after planning (2026-10-07)**: the type cast `<value> as T`.
> Motivation: once the F6 print fix was finalized as "strict string checking" (§6), non-string
> values need an explicit `as string` to be printed — without `as`, scalar values cannot be stringified.
> It is positioned as a 0.0.2 temporary feature with a deliberately narrow whitelist
> (§3.6.2); the full conversion system will be revisited with generics/traits (0.1.0).

### 1.3 Debt left over from 0.0.1 (must be repaid in this release)

| Debt | Source | 0.0.2 resolution |
|------|------|-----------|
| `print` type checking was special-cased and loosened (any types, any arity) | `DESIGN.md §18`, the `HACK (0.0.1)` comment in `typeck/infer.rs` | §6: builtins get real signatures and strict checking is restored |
| `Value::Str(Rc<str>)` shared semantics conflict with the ownership model | `BYTECODE.md §3.2` ⚠️ temporary decision | §5.3: change to `Str(Box<str>)` with exclusive ownership |
| `span_map` left empty, `SpanEntry` undefined | `BYTECODE.md §2`, 0.0.1 decision | §7: define and generate it, wire it into runtime error diagnostics |
| Bytecode opcodes 0x80–0x9F reserved but unused | `BYTECODE.md §4.2` | §5.1: v2 instructions occupy the reserved groups |

### 1.4 Explicitly out of scope

| Not doing | Reason |
|------|------|
| `struct` / `for` / iterators | 0.0.3 scope (`DESIGN.md §18`); avoid bloating this release |
| Closures / `move` closure captures | No function literal syntax; the `move` keyword only applies at variable positions in this release |
| Traits / operator overloading / generics | 0.1.0 scope; `print`'s polymorphism is solved with a builtin special-case signature (§6), without introducing traits |
| User-defined `Drop` / destructors | The correctness of destruction timing depends on a more complete lifetime model; this release only performs deterministic reclamation for builtin types |
| `ref` local variables / `ref` globals / returning `ref` | Borrows are only allowed in parameter positions (`DESIGN.md §9`), which makes safety achievable without a full borrow checker (argued in §3.3) |
| Borrowing into a `box` (`ref` pointing at `deref b`) | The handle would need a second level of indirection for little benefit; `clone deref b` already covers read needs |
| `addr` / `ptr` / `unsafe` | `DESIGN.md §10/§12` explicitly excludes these from 0.0.1; unchanged for 0.0.2 |
| Changes to integer/float arithmetic semantics | Keep the 0.0.1 checked semantics |
| A full conversion matrix (`int as float`, From-like, user-defined conversions) | The 0.0.2 `as` is only the minimal whitelist for the print fix (§3.6.2); expand after generics/traits land (0.1.0) |

---

## 2. Design decision points (to be written into DESIGN.md after review)

Each decision point below presents a **recommended option** with rationale; the review may overturn it, and if so the corresponding details in §3–§5 must be updated accordingly.

### D1: `move` explicit or implicit?

| Option | Semantics | Cost |
|------|------|------|
| A. Implicit move (Rust style) | `y = x` transfers; later uses of `x` are an error; `clone x` copies | The `move` keyword has **no use** in 0.0.2 (no closures), contradicting the roadmap |
| **B. Explicit move (recommended)** | An owned value entering a binding/argument position must be written `move x` (transfer) or `clone x` (copy); bare `x` is a compile error | Slightly more verbose code; `string` assignment semantics are a breaking change relative to 0.0.1 |

**Recommend B**. Rationale:

- The roadmap explicitly lists `move` as a 0.0.2 deliverable; under option A the keyword cannot be realized;
- It matches "explicit, no magic" (`DESIGN.md §1`): every ownership transfer is visible in the source;
- Cheap to implement in the compiler: all move points are explicitly marked on the AST, and the affine check only needs to track a "moved set" —
  no Rust-style last-use liveness analysis;
- `Copy` types (int/float/bool/function values) are entirely unaffected; the bulk of 0.0.1 code (numeric computation) needs zero migration.

### D2: Read semantics of `deref b`

**Recommendation**: `deref b` produces a **copy** of the pointee — bitwise copy for `T: Copy`; a deep copy when `T` is owned.
Writing back uses assignment form `deref b = expr;` (the old pointee is released).

Rationale: the box owns the pointee, and reading it is not the same as taking ownership; if
`clone deref b` were required to read an owned pointee, scenarios like `print` and comparison would be extremely verbose.
The "explicit clone" rule (D1) only governs copying at **variable positions**.

Semantics of `clone box`: produces a **new box** (clone the pointee first, then box it), consistent with Rust's `Box: Clone`.

### D3: The read-only use set (auto-copy, no transfer)

The following positions constitute **read-only** use of owned values; the compiler copies automatically and the caller's variable remains usable:

| Position | Handling |
|------|------|
| Comparison operator operands (`== != < > <= >=`) | codegen deep-copies owned operands before executing `Eq` and similar instructions |
| `print` arguments | print is a builtin executed by the host; by convention **builtins never own their arguments**; codegen copies owned args |
| `deref b` reads | See D2 |

All other value positions (binding/assignment RHS, arguments, the `choose` scrutinee) require explicit `move`/`clone` per D1/D7.
The 0.0.2 "read-only set" is deliberately kept tiny: every position opened up must be documented in the spec.

### D4: Evaluation semantics of string literals

**Recommendation**: each evaluation of a string literal produces a **new owned value** (copied from the constant pool).
`while … { print("hi"); }` allocates once per iteration — 0.0.2 accepts this cost (correctness first);
literal interning / copy-on-write optimizations can come later without changing the semantics.

### D5: Global variables of owned types

**Recommendation**: allowed, but with different semantics from locals —

- Write (including `__init__` initialization): replaces the entire value; the old value is released;
- Read: **automatic deep copy** (equivalent to `clone`); the global always stays "has a value";
- `move g` (moving the value out of a global): **compile error** — a global has no "moved away" state;
  the error suggests `clone g`.

Rationale: tracking the "moved" state of globals across functions is an interprocedural analysis whose complexity is not proportional to the benefit;
auto-copying on reads matches 0.0.1 usage intuition, so existing programs need zero migration.

### D6: `Result` type inference and error-type unification

**Recommendation**:

- The types of `Ok(v)` / `Err(e)` are determined by **context** (function return type annotation, binding type annotation,
  the return type of the function containing the `?`, the `choose` scrutinee type);
- With no context at all, it is a compile error prompting for a type annotation;
- `?` propagation requires the enclosing function to return `Result<T2, E>`, with `E` **exactly equal** to the propagated `E`
  (0.0.2 has no error-type conversion; the From-like mechanism arrives with traits in 0.1.0).

### D7: `choose` scrutinee of owned type

**Recommendation**: the scrutinee position is a **consuming position** — when the scrutinee is an owned local variable it must be written
`move s` / `clone s` (consistent with D1); when the scrutinee is an ordinary expression (e.g. a function call) no keyword is needed.
The bindings of `when Ok(x)` / `when Err(e)` are **transfers** (the payload is moved out of the `Result` value).

### D8: `main` returning `Result`

**Recommendation**: `main` may return `Result<T, E>`. When the VM's entry point receives `Err(e)`,
it prints `e` to stderr and exits with code 1 (the `Ok` behavior is unchanged: exit code is the integer value or 0).
Error handling "flows" from the very first program.

---

## 3. Language Feature Design

### 3.1 Ownership model (F1: `move` / `clone`)

#### 3.1.1 The three value classes

| Class | Types | Assignment/argument-passing semantics |
|------|------|---------------|
| **Copy** | `int` `float` `bool` `unit` function values (references to `FuncId`), `ref T` (handles) | Implicit bitwise copy; everything as before |
| **Owned** | `string` `box<T>`, `Result<T, E>` containing owned components | Single owner; transfer needs `move`, copy needs `clone` |
| **Borrow** | `ref T` (function parameter positions only, see §3.3) | Read-only borrow, no transfer |

Classification rule (implemented in typeck): `is_copy(T)` — true for primitive numeric types; false for `string` and `box<_>`;
`Result<T, E>` = `is_copy(T) && is_copy(E)`; true for function types (`FuncId` is a reference, not owned data).

#### 3.1.2 Syntax

```fleen
s = "hello";          // binds an owned value (the RHS is a fresh value; no keyword needed)
t = move s;           // transfer: s is no longer usable afterwards
t = clone s;          // deep copy: s remains usable
s = "again";          // legal: s has been re-bound (this is shadowing/reassignment, unrelated to move)
```

`move` / `clone` are **prefix keyword expressions**, at the same level as `-` and `!` (the unary level); their operand is a
**place**:

| Expression | Operand | Effect |
|--------|--------|------|
| `move ident` | Local variable | Transfers ownership; the original variable enters the "moved" state |
| `clone ident` | Local variable | Deep copy; the original variable is unchanged |
| `clone deref b` | The box's pointee | Deep-copies the pointee; the box is unchanged |
| `clone g` (g is a global) | Global | Deep copy (equivalent to the read semantics, see D5) |

**Forbidden**: `move deref b` (cannot move out of a box), `move g` (cannot move out of a global),
`move s` (s is a `ref` parameter — cannot move out of a borrow), `move x` (x is a Copy type — meaningless;
the error suggests removing `move`).

#### 3.1.3 Core rules

1. **Uses of Copy types never need `move` / `clone`** (rule 0; covers the vast majority of 0.0.1 code).
2. When the value of an owned local variable enters a **consuming position**, an explicit `move` / `clone` is required:
   - Top level of a binding/assignment RHS: `y = move x;` / `y = clone x;`
   - Arguments: `f(move x)` / `f(clone x)`
   - `choose` scrutinee: `choose move s { … }`
   - A bare `y = x;` (x owned) → compile error, with a help hint to add `move` or `clone`.
3. **Producing positions transfer implicitly** (values flow out through the "exit" with no keyword):
   - The tail expression of a function body, of a block, and of `if`/`choose` branch bodies;
   - In `func greet(): string = s`, the `s` is transferred implicitly (afterwards `s` is moved);
   - A transfer at a branch tail is a **conditional transfer**: after `y = if c { s } else { t };`, using `s`
     reports "possibly moved" (second example in §3.1.4).
4. **Consuming positions accept nothing but bare variables and owned fresh values**:
   `y = fib(n);` (returning an owned string) is legal — a function return value is a fresh value and transfers naturally.
5. **Moved values are unusable**: using a moved binding on any path → `UseAfterMove`;
   moved on only some paths → `MaybeMovedAfterBranch`.
6. The `const` and mutability rules are unchanged: `const s = "hi";` can likewise be `move`d (the move is the last use);
   assigning to a moved variable is also an error (moved ≠ unbound; the error messages differ).
7. Function parameters are handled according to the class of their declared type: owned parameter → argument site needs `move`/`clone`;
   Copy parameter → as before; `ref T` parameter → pass a borrow (§3.3), do not `move`.
8. Ownership checking is per-function and **does not cross function boundaries** (consistent with the scoping rules).

#### 3.1.4 Example error messages

```
error: use of moved value `s`
  --> demo.fln:4:11
   |
 3 | t = move s;
   |          - `s` moved here
 4 | print(s);
   |       ^ `s` was moved
   |
help: clone the value if you still need it: `t = clone s;`
```

```
error: `s` may be moved in one of the branches
  --> demo.fln:6:11
   |
 4 | y = if c { s } else { t };
   |            - `s` moved in this branch
 6 | print(s);
   |       ^ use of possibly-moved `s`
   |
help: write `clone s` in the branch, or `clone s` here
```

```
error: cannot move out of `x` without `move`
  --> demo.fln:8:5
   |
 8 | y = x;
   |     ^ `x` owns heap data (string)
   |
help: transfer ownership: `y = move x;`, or copy: `y = clone x;`
```

#### 3.1.5 Impact on 0.0.1 programs (migration checklist)

| 0.0.1 code | 0.0.2 behavior | Migration |
|-----------|-----------|------|
| Numeric/boolean computation, control flow, functions | Unchanged | None |
| `s2 = s1;` (string assignment) | Compile error | `s2 = move s1;` or `s2 = clone s1;` |
| `print(any value)` | Unchanged (print's args are read-only, see D3) | None |
| `print(func_value)` | **Compile error** (0.0.1 allowed it via the Hack) | Rejected by design |
| `clone` / `move` / `deref` used as variable names | **Compile error** (they become reserved keywords) | Rename (allowed in 0.0.1, reserved in 0.0.2) |
| Explicit semicolons | Still legal | None |

> Version policy (SPEC §13): `0.0.x` is the API-unstable period, so the breaking changes above are acceptable;
> all of them are recorded in the "Breaking" section of the CHANGELOG.

### 3.2 box<T> (F2)

#### 3.2.1 Syntax and semantics

```fleen
b = box 42;                  // b: box<int>, heap-allocated
n = deref b;                 // read: a copy (bitwise copy for Copy types)
deref b = n + 1;             // write: replaces the pointee; the old value is released
c = clone b;                 // deep copy: new box + new pointee (D2)
s = box "heap";              // s: box<string>, the pointee is owned
t = clone deref s;           // deep-copies the pointee; the box itself is untouched
```

- `box expr`: a prefix keyword expression, at the unary level. `box` is already a type keyword (`BoxType`);
  the two are distinguished by syntactic position: type position `box<T>`, expression position `box expr`; unambiguous.
- `deref`: a new keyword (prefix, unary level); its operand is a postfix expression (an identifier, in 0.0.2).
- Assignment target extension: `deref b = expr` is a new legal assignment target form; `deref b` cannot carry a type annotation
  (consistent with §3.4 "assignments cannot carry type annotations").
- Allowed pointee types `T` in 0.0.2: `int` `float` `bool` `string` `box<U>`
  `Result<T2, E>` (no arrays, no structs).
- **Cannot move out of a box**: `move deref b` is a compile error — the box's exclusive ownership is not broken by reading;
  to obtain the value, use `clone deref b`.

#### 3.2.2 Deallocation timing (deterministic destruction, no GC)

| Event | Handling |
|------|------|
| The owning variable leaves scope (frame destruction) | The box is released along with its pointee |
| The owner is transferred via `move` | The release responsibility travels with the value; the original slot is cleared |
| The owner is reassigned / overwritten by `deref b =` | The old value is released first |
| An expression statement discards a box value | Released immediately |
| `clone b` | New and old boxes are independent; neither affects the other |

Reference cycles cannot exist: boxes have single ownership and `ref` cannot be stored → no GC needed, consistent with `DESIGN.md §14`.

#### 3.2.3 Example error message

```
error: cannot move out of a `box`
  --> box.fln:5:5
   |
 5 | y = move deref b;
   |     ^^^^^^^^^^^^ box owns its pointee exclusively
   |
help: copy the pointee instead: `y = clone deref b;`
```

### 3.3 ref (F3: read-only borrow, function parameters only)

#### 3.3.1 Syntax and semantics

```fleen
func shout(s: ref string): int {
    print(s);            // read-only use
    42
}

func main(): int {
    name = "fleen";
    shout(name);         // pass a borrow: name remains usable
    shout(name);         // borrow again; legal
    0
}
```

- `ref T` may only appear in **function parameter type** position (as agreed in `DESIGN.md §9`).
  `ref` locals, `ref` globals, `ref` return values, and nested `ref ref T` are all compile errors
  (error kind: `RefNotAllowedHere`).
- Once a parameter is declared `ref T`, within the function body that parameter:
  - is read-only — assignment and `move` are compile errors (`AssignToRefParam` / `MoveOfBorrowed`);
  - `clone s` yields an owned `T` (escaping the borrow);
  - can be forwarded as a `ref T` argument to another function (the handle flows; zero copy).
- Argument position: passing **does not transfer**; `name` needs no `move`/`clone` (in the spirit of D3:
  borrowing is the safe default). Arguments may be: a local variable, a global variable, or another `ref` parameter.
  Borrowing into a box via `deref b` is not supported (§1.4).

#### 3.3.2 Why no borrow checker is needed

A borrow handle only lives for the duration of **a single call**:

1. The borrowed slot lives in the caller's frame (or the global table); during the call the caller is suspended, so the slot's contents cannot be overwritten;
2. The callee can only perform read-only operations on the handle and cannot store the handle anywhere longer-lived
   (it cannot be assigned, stored in a global, or returned);
3. When the call returns, the handle dies — **there is no possibility of a dangling borrow**.

Therefore 0.0.2's "borrow checking" degenerates into a few static legality rules (the table above); no lifetime annotations,
no CFG-level borrow analysis. This is the core dividend of the "parameters-only borrow" design.

#### 3.3.3 Type rules

- `ref T` is a distinct type: `ref string ≠ string`. Assigning a `ref string` to a `string`
  binding is a type error, with a hint to use `clone s`.
- `ref` over a Copy type (e.g. `ref int`) is allowed but meaningless (equivalent to `int`); it is not an error (rule consistency takes priority).
- Passing an owned argument where an owned parameter is expected but written as a borrow is a **type error** (the parameter list decides),
  with a hint for `move` / `clone`; conversely, passing `move x` to a `ref` parameter is an error (borrows do not accept transfers).

### 3.4 Result: Ok/Err constructors, the choose pattern, and `?` (F4)

#### 3.4.1 Ok / Err constructor expressions

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
```

- `Ok(expr)` / `Err(expr)` are, at the syntax level, ordinary call expressions (`Call(Ident)`);
  typeck recognizes them as builtin constructors (the same mechanism as `print`; they may be shadowed by users, see §4.3).
- Type inference rules (D6): `Result<T, E>` is determined by context; with no context, report
  `CannotInferResultType` with a hint to annotate. Branch unification follows the existing rules (0.0.1 requires exact equality).
- Construction is **boxing**: `Ok(v)` moves `v` into the `Result` value.

#### 3.4.2 The Ok/Err pattern for choose

```fleen
res = choose div(10, 2) {
    when Ok(v) { v }
    when Err(e) {
        print("error: ", e);
        0 - 1
    }
};
```

- Pattern syntax extension: `result_pattern = ("Ok" | "Err") , "(" , ident , ")"`;
  it binds `v: T` and `e: E`, and the binding semantics are a **transfer** (D7).
- Exhaustiveness: with both `Ok` and `Err` arms present → exhaustive (no `otherwise` needed); with only one of them →
  an `otherwise` or the other arm must be added; arms with guards do not count toward exhaustiveness (consistent with the existing rule).
- The example in `DESIGN.md §8` becomes official from this point on.

#### 3.4.3 The `?` propagation operator

```fleen
func ratio(a: int, b: int): Result<int, string> {
    q = div(a, b)?;      // on Err, the whole function immediately returns that Err
    r = div(q, 2)?;
    Ok(r)
}
```

- `?` is a **postfix** operator (postfix level, the same level as call/index); its operand's type must be
  `Result<T, E>`.
- Legality conditions (D6): the enclosing function's return type annotation is `Result<T2, E>`, and `E` exactly equals the
  operand's `E`. If the conditions fail, report `QuestionOutsideResultFn` (when `main` returns int,
  `?` reports this error) or `QuestionTypeMismatch`.
- `?` is legal in `main` (as long as `main` returns `Result<int, E>`); the Err exit behavior is described in D8.
- When the value type of an expression statement is `Result<T, E>`, report `UnhandledResult` — a Result may not be
  silently discarded; the hint is to bind it or add `?` (0.0.2's only "value must be handled" rule).

### 3.5 ASI (F5: automatic semicolon insertion)

> **The authoritative design is `docs/0.0.2/ASI.md`** (finalized 2026-10-07).
> This section is a summary; where it conflicts with ASI.md, ASI.md wins.

#### 3.5.1 Strategy: a front pass (newline-sensitive)

The final design uses the **Lex → ASI pass → Parse** architecture with **newline-sensitive** semantics (statement boundaries are
decided at newlines; joining statements on the same line is illegal). The lexer gains a `Newline` token; the ASI pass
(the pure function in `parser/asi.rs`) consumes all Newlines, inserts explicit `Semi` at statement boundaries, and
reports `ExpectedSemiOrNewStmt` for tokens that can neither continue nor start a statement.
The pass's output token stream is isomorphic to 0.0.1's (semicolons complete), and the Parse phase reuses the original logic
with only four accommodations: bindings/const hugging `}` need no semicolon, Assign forms are not tail expressions,
continuation climbs after block-like headers, and a statement-entry guard.

Decision summary (at a newline or EOF, with `p` the previous token and `n` the next token):

> 1. The innermost unclosed bracket is `(` or `[` → discard (inside `{`, ASI still applies);
> 2. `n` is `}` → discard (never insert before `}` — tail expression vs. binding is distinguished structurally by the parser);
> 3. `p` cannot end a statement → discard (cross-line continuation);
> 4. `n` ∈ the implicit-end set (start set ∖ {`(` `-`} ∪ {`}` EOF}) → insert `;`;
> 5. `n` ∈ the continuation set ∪ {`else` `elif` `when` `otherwise` `;`} → discard (continuation wins);
> 6. Otherwise → report `ExpectedSemiOrNewStmt`.

#### 3.5.2 Trap cases (must go into the spec and tests)

| Code | 0.0.2 parse | Notes |
|------|-----------|------|
| `x = 1` ⏎ `y = 2` | Two statements | `Ident` ∈ start set |
| `x = 1 y = 2` (same line) | Syntax error | Newline-sensitive: same-line joining is illegal |
| `x = 1` ⏎ `- 2` | `x = (1 - 2)` | `-` ∈ continuation set; binary wins |
| `f()` ⏎ `(g())` | `f()(g())` call chain | `(` ∈ continuation set (call) |
| `f()` ⏎ `[0]` | `f()[0]` index | `[` ∈ continuation set |
| `x = 1` ⏎ `!flag` | Two statements | `!` is not a binary continuation; `!flag` ∈ start set |
| `if c { 1 }` ⏎ `else { 2 }` | One if-else expression | `else` attaches to that if (cross-line joining is legal; the current parser behaves the same) |
| `if c { 1 };` ⏎ `else { 2 }` | Syntax error | `else` does not continue an **already terminated** (semicolon'd) if statement |
| `while x < 10 {` ⏎ `x = x + 1` ⏎ `}` | Legal | A binding hugging `}` inside a block needs no semicolon |
| `when 0 { "zero" }` ⏎ `otherwise { "o" }` | Legal | The when/otherwise no-semicolon rule is unchanged |

#### 3.5.3 Invariants

- Every 0.0.1 program (explicit semicolons) remains valid byte-for-byte — `;` is always accepted;
- The rules that `func` declarations take no trailing semicolon, block tail expressions take none, and when/otherwise take none are unchanged;
- The presence or absence of a semicolon is invisible after the AST — MIR/bytecode are unchanged (`has_semi` semantics in ASI.md §4);
- The `tokenize()` public API's output gains the `Newline` token (a contract change; see ASI.md §6).

### 3.6 The `as` cast (F8: a 0.0.2 temporary feature, prerequisite of the print fix)

> **Positioning**: provides the minimal conversion facility for F6 (print's strict string checking, §6) — without `as`,
> there is no legal way to print anything other than `print(42)`. The whitelist is deliberately narrowed to "scalar → string";
> the full conversion matrix (`int as float`, From-like, user-defined conversions) will be revisited with generics/traits (0.1.0).

#### 3.6.1 Syntax and precedence

```fleen
n = 42 as string;        // "42"
t = true as string;      // "true"
print(42 as string);     // cast in place (print accepts only string, see §6)
```

- `as` is an infix keyword, adding `TokenKind::As` (`as` becomes a reserved word; Breaking);
- A new cast level is added between unary and binary (matching Rust: unary binds tighter than `as`, and `as` binds tighter than binary):
  - `deref b as string` = `(deref b) as string` (legal when `b: box<int>`);
  - `1 + 2 as string` = `1 + (2 as string)`;
  - `box 1 as string` = `box (1 as string)` (prefix keywords bind tighter).
- ASI: `As` joins the continuation set (`can_continue_expr`); `x = 42` ⏎ `as string` continues across lines,
  under the same rule as binary operators (U03's architecture is unchanged; only the predicate is extended, and ASI.md §4's continuation set gains one entry).

#### 3.6.2 Semantics and rules

| Rule | Content |
|------|------|
| Whitelist | Only `int / float / bool as string`; everything else (`string as string`, `int as float`, `box<T> as …`, `ref T` operands, identity casts) → `UnsupportedCast { from, to }`, with help noting that 0.0.2 only supports scalar → string |
| Result value | A **fresh owned string** — no `move` / `clone` needed at binding/argument positions (consistent with "the RHS is a fresh value, no keyword needed") |
| Formatting | int in decimal (including the minus sign), bool as `true` / `false`, float matching print's output; the formatting implementation **shares the same helper** with print to avoid divergence |
| Runtime | New instruction `ToStr` (0xA5, `v → s`, Δ0 min 1, see §5.1); `as` introduces no implicit numeric conversions |

#### 3.6.3 Example error message

```
error: unsupported cast: int to float
  --> cast.fln:2:9
   |
 2 | f = 42 as float;
   |         ^^^^^^^
   |
help: 0.0.2 only supports scalar-to-string casts (`as string`); full conversions arrive with generics
```

---

## 4. Changes per Compiler Stage

> Stage boundaries match `SPEC.md §6`: no stage skipping, no shared mutable state, every stage independently testable.

### 4.1 Lex (`lexer/token.rs`)

| Item | Content |
|----|------|
| New keyword tokens | `Move` (`move`), `Clone` (`clone`), `Deref` (`deref`), `As` (`as`, F8 §3.6, added in U13) |
| New punctuation tokens | `Question` (`?`), `Newline` (newline, for ASI; consecutive ones folded, not emitted at file start, not emitted inside block comments — see ASI.md §2) |
| Existing, unchanged | `BoxType` / `RefType` already exist (reserved words since 0.0.1); `Ok` / `Err` remain `Ident` (constructors are recognized in typeck) |
| `is_keyword` / `keyword_str` / `Display` | Updated in sync |

### 4.2 Parse (`parser/`)

| Item | Content |
|----|------|
| New AST nodes | `ExprMove { place, span }`, `ExprClone { place, span }` (place = `Ident` or `ExprDeref`), `ExprBox { inner, span }`, `ExprDeref { inner, span }`, `ExprQuestion { inner, span }` |
| Unary level extension | `unary = [ "-" \| "!" \| "box" \| "deref" \| "move" \| "clone" ] , postfix` (prefix keywords right-associative; the operands of `move`/`clone` are validated as legal places by typeck; the parser only checks the shape) |
| Postfix extension | `postfix = primary , { call \| index \| field \| "?" }` |
| Cast level (F8) | `cast = unary , { "as" , type }` (a new level between unary and binary, §3.6); AST `Expr::Cast { inner, ty, span }`; `As` added to the ASI continuation set |
| Assignment target extension | `deref b = expr` enters the assign-target determination (a new legal form beyond `InvalidAssignmentTarget`) |
| Pattern extension | `pattern = neg_pattern \| literal \| ident \| result_pattern`; `result_pattern = ("Ok" \| "Err") , "(" , ident , ")"` (the parser only recognizes the shape; binding `Ok`/`Err` semantics is left to typeck) |
| ASI rework | **Front pass** (`parser/asi.rs`, see ASI.md): the lexer gains `Newline`; the pass inserts semicolons/consumes newlines; four parser accommodations (bindings hugging `}` need no semicolon, Assign is not a tail expression, continuation climbs after block-like headers, statement-entry guard); `can_start_stmt` / `can_continue_expr` / `implies_stmt_end` as pure functions for easy unit testing |
| EBNF | `SYNTAX.ebnf` bumped to v0.0.2: new unary prefixes, the `?` postfix, result_pattern, the deref assignment target; the semicolon comment changes to "optional" |

### 4.3 Resolve (`resolver/`)

| Item | Content |
|----|------|
| New binding form | ref parameters enter the binding table (`BindingKind::Parameter`, with a new `is_ref: bool` or carried by the type) |
| `Ok` / `Err` | Reuse `print`'s builtin resolution mechanism (shadowable, builtin-flagged) |
| Unchanged | The `=` triage, shadowing-mutability consistency, and the loop-body assignment rules are all untouched |

### 4.4 Typeck (`typeck/`) — the largest new component in this release

| Item | Content |
|----|------|
| Type classification | `Type::is_copy()` (§3.1.1); `Type::Box` / `Type::Ref` promoted from "parsed but Unsupported" |
| **Ownership checker** | New module `typeck/ownership.rs`: a **flow-sensitive** traversal over the TypedHir, maintaining each binding's `Alive / Moved { span } / MaybeMoved` state (details below) |
| Ok/Err constructor checking | Context type unification; no context → `CannotInferResultType` |
| Result pattern | Binding types = `T` / `E`; exhaustiveness: Ok+Err both present is exhaustive |
| `?` checking | D6 condition validation; a Result in expression-statement position → `UnhandledResult` |
| `as` cast checking | Whitelist per §3.6.2 (scalar → string only); illegal → `UnsupportedCast` |
| print fix | See §6 |
| New error kinds | `UseAfterMove`, `MaybeMovedAfterBranch`, `MoveOfCopyType`, `MoveOutOfBox`, `MoveOutOfGlobal`, `MoveOfBorrowed`, `AssignToRefParam`, `RefNotAllowedHere`, `OwnedArgRequiresMove` (help: `move`/`clone`), `BorrowArgWithMove`, `CannotInferResultType`, `QuestionOutsideResultFn`, `QuestionTypeMismatch`, `UnhandledResult`, `BoxInnerTypeNotAllowed`, `UnsupportedCast` |

**Ownership checker algorithm (draft)**:

```text
For each function body:
  state: Map<BindingId, MoveState>   // Alive | Moved{span} | MaybeMoved{spans}
  Traverse the TypedHir (statement order + control-flow-structure aware):
    - Binding/assignment (RHS top level is move/clone) → record a transfer or copy per the RHS shape
    - A bare owned ident at a consuming position → report OwnedArgRequiresMove (D1 rule 2)
    - move ident → state[ident] = Moved{span}; a later read/write/move → UseAfterMove
    - if / elif / else: each branch is analyzed independently from a snapshot; merge at the join point:
        all branches Moved → Moved; some branches Moved → MaybeMoved; otherwise Alive
    - while / choose: snapshot at entry; transfers inside the body take the most pessimistic view
      at back edges/exits (a move inside the loop ⇒ MaybeMoved at exit; the state must be Alive at the back edge,
      otherwise report an error — moving inside the loop then using it in the next iteration is UseAfterMove)
    - Function boundaries: clear state (does not cross functions)
  Implicit transfers at tail expressions (producing positions) are recorded the same way (rule 3)
```

### 4.5 Lower (`lower/`)

| Item | Content |
|----|------|
| MIR instruction extension | `AllocBox`, `DerefBox`, `StoreDerefBox`, `MakeRefLocal(u16)`, `DupDeep`, `MoveLocal(u16)`, `CloneLocal(u16)`, `CloneGlobal(u16)`, `PackOk`, `PackErr`, `IsErr`, `UnwrapOk`, `UnwrapErr`, `ToStr` (F8 §3.6) (full table in §5.1) |
| **Span threading** | `MirInstr` changes from a bare enum to `struct MirInstr { kind: MirInstrKind, span: Span }` (or each variant carries a span); Lower carries spans instruction-by-instruction from the TypedHir — this is the data source for §7's span_map |
| New lowering rules | `box e` → `<e>; AllocBox`; `deref b` → `<b>; DerefBox`; `deref b = v` → `<b>; <v>; StoreDerefBox`; `move x`/`clone x` → `MoveLocal/CloneLocal slot`; `x?` → the §5.6 sequence; `f(x)` (owned parameter) → `MoveLocal/CloneLocal` + `Call`; owned operands of comparisons/print → `CloneLocal/CloneGlobal` first (D3); `e as string` → `<e>; ToStr` (§3.6) |
| choose (Result scrutinee) | Flattened to `IsErr + UnwrapOk/UnwrapErr`, with the payload bound directly via `StoreLocal` (no `BindMatch` needed, see §5.6) |

### 4.6 Codegen (`codegen/`)

| Item | Content |
|----|------|
| Opcode | New v2 instructions in 1:1 correspondence with 0.0.1 instructions (§5.1); the `version` field is raised to `2` |
| span_map | `SpanEntry { offset, start, end }` written per instruction (the format was reserved in §9; no .flnc layout change needed) |
| Constant pool | Unchanged (strings still enter the pool; the `Const` instruction copies to an owned value at execution, see D4) |
| Deserialization | `from_bytes` accepts `version ∈ {1, 2}` (backward compatible with old .flnc) |

### 4.7 Verify (`fleen-verify/`)

| Item | Content |
|----|------|
| Stack effect table | `Δdepth` / minimum depth for the 13 new instructions (full table in §5.2) |
| New validations | `MakeRefLocal`'s slot < the current function's `locals` (same rule as LoadLocal); `StoreDerefBox` minimum depth 2; the remaining structural validations carry over |
| Version gate | Accepts `version ∈ {1, 2}`; a v2 instruction in a v1 module → `BadOpcode` (holds naturally) |

### 4.8 VM (`fleen-vm/`)

| Item | Content |
|----|------|
| Value rework | §5.3 |
| Move semantics | `MoveLocal` = `mem::replace(&mut stack[slot], Value::Unit)`; a slot emptied by a move is only ever re-`StoreLocal`ed afterwards (typeck guarantees it is never read) |
| Reclamation timing | `Vec::truncate` (frame destruction), slot overwrites, and `Pop` are all handled naturally by Rust's `Drop` — the VM writes no explicit destruction loop; `debug_assert!` helps verify "moved slots are not read" |
| Handle dereferencing | `Value::Ref { base, slot }` reads all go through a helper (comparison/print/clone automatically see through it); out of bounds → `RuntimeError::BorrowOutOfRange` (defensive) |
| Result instructions | `UnwrapOk` on `Err` → `RuntimeError::ResultMismatch` (defensive; cannot happen with legal bytecode) |
| Cast instruction | `ToStr`: pop a scalar → format to a string (sharing print's `fmt` helper) → push (§3.6) |
| Entry Err | D8: the entry returns `Err(e)` → print to stderr, exit code 1 |
| New RuntimeErrors | `ResultMismatch`, `BorrowOutOfRange` |

---

## 5. Bytecode v2 Spec Additions (to be merged into BYTECODE.md)

### 5.1 New instructions (fills 0x80–0x83 and 0x90–0x93; the Result group activates 0xA0–0xA4; the `as` cast takes 0xA5)

| opcode | Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|--------|------|--------|------|
| 0x80 | `AllocBox` | — | 1 | `v → b` | Heap allocation; the value's ownership moves into the box |
| 0x81 | `DerefBox` | — | 1 | `b → b v` | Read the pointee (a copy/deep copy); the box stays on the stack |
| 0x82 | `StoreDerefBox` | — | 1 | `b v →` | Write the pointee; the old value is released |
| 0x83 | `MakeRefLocal` | `u16` slot | 3 | `→ ref` | Create a borrow handle to this frame's `stack_base + slot` (argument preparation) |
| 0x90 | `DupDeep` | — | 1 | `v → v v` | Deep copy of the stack top (the on-stack form of `clone`) |
| 0x91 | `MoveLocal` | `u16` slot | 3 | `→ v` | **Consuming read**: takes the slot's value and clears the slot to `Unit` |
| 0x92 | `CloneLocal` | `u16` slot | 3 | `→ v` | Non-consuming read: deep-copies the slot's value; the slot is untouched |
| 0x93 | `CloneGlobal` | `u16` gid | 3 | `→ v` | Non-consuming read of a global (the only way to read an owned global) |
| 0xA0 | `PackOk` | — | 1 | `v → ok(v)` | Construct `Ok` |
| 0xA1 | `PackErr` | — | 1 | `v → err(e)` | Construct `Err` |
| 0xA2 | `IsErr` | — | 1 | `r → bool` | Test for `Err` |
| 0xA3 | `UnwrapOk` | — | 1 | `r → v` | Extract the Ok payload (transfers) |
| 0xA4 | `UnwrapErr` | — | 1 | `r → e` | Extract the Err payload (transfers) |
| 0xA5 | `ToStr` | — | 1 | `v → s` | Scalar-to-string cast (`as`, §3.6; the result is a fresh owned string) |

Invariant: existing instructions (0x00–0x7F) keep their opcodes and encodings **unchanged, to the letter** (`BYTECODE.md §10` rule);
v1 modules remain executable on the 0.0.2 VM/verify.

### 5.2 Stack-depth analysis addendum (for verify)

| Instruction | Δdepth | Minimum depth before execution |
|------|--------|----------------|
| `AllocBox` | **0** (`v → b`: the value is consumed and replaced by the box) | 1 |
| `MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal` | +1 | 0 / 0 / 0 / 0 |
| `DerefBox` / `DupDeep` | +1 | 1 |
| `StoreDerefBox` | -2 | 2 |
| `PackOk` / `PackErr` / `IsErr` / `UnwrapOk` / `UnwrapErr` / `ToStr` | 0 | 1 |

> **U07 erratum**: this table originally listed `AllocBox` as `+1` alongside `MakeRefLocal` and friends,
> but the lowering of `box e` is `<e>; AllocBox` — `AllocBox` must **replace** the value on the stack
> with the box (net 0), otherwise the stack depth after every `box e` expression would be over-counted by 1.
> The error was found by U07's "the compiler output must pass verify" end-to-end test and has been corrected
> to `v → b` (BYTECODE.md §8 carries the same erratum).

### 5.3 Value representation (replaces `Rc<str>`, repaying the BYTECODE.md §3.2 temporary decision)

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),                 // ← changed from Rc<str> to exclusive ownership (deep copy means a real copy)
    Boxed(Box<Value>),             // box<T>: single owner
    Ref { base: u32, slot: u16 },  // borrow handle (lives only within the callee frame's lifetime)
    Ok(Box<Value>),                // Result constructors
    Err(Box<Value>),
    Unit,
    Func(FuncId),
}
```

- Ownership discipline is **statically guaranteed by typeck**; Rust's move/drop semantics on the VM side are the runtime implementation,
  with `Drop` naturally deterministic and no GC.
- `Eq` semantics: `Str` by content; `Boxed` compares pointees (consistent with Rust's `Box: PartialEq`);
  `Ref` auto-dereferences before comparing.
- `Value::Ref` is not serializable (values never hit disk; the constant pool has no such form).

### 5.4 The .flnc format

**Zero layout changes**: the `span_map` field was reserved in 0.0.1 §9 (`span_map_len: u32` +
`offset/start/end` triples); 0.0.2 simply starts **filling** it. `version` rises to `2`,
and readers accept `{1, 2}`. Future versions still only append new tables at the end.

### 5.5 `?` lowering and bytecode mapping

```fleen
q = div(a, b)?;
```

```text
<eval div(a, b)>      ; r: Result
Dup                   ; r r
IsErr                 ; r bool
JumpIfTrue  L_err     ; the bool is consumed
UnwrapOk              ; r → v
StoreLocal q
Jump        L_cont
L_err:
UnwrapErr             ; r → e
Return                ; early return of the Err (function level)
L_cont:
```

### 5.6 choose (Result scrutinee) mapping

```fleen
choose div(10, 2) {
    when Ok(v) { v }
    when Err(e) { 0 - 1 }
}
```

```text
<eval div(10, 2)>     ; r
Dup
IsErr
JumpIfTrue  L_err
UnwrapOk              ; payload v (transferred)
StoreLocal v          ; when Ok(v) binding
<ok arm body>         ; tail value
Jump        L_end
L_err:
UnwrapErr             ; payload e (transferred)
StoreLocal e
<err arm body>
L_end:
```

> Guards (`when Ok(x) if x > 0`): after `StoreLocal x`, evaluate the guard first;
> if it fails, jump into the `Err` arm — at that point the payload has already been consumed, so a `DupDeep`
> must reserve a copy (codegen detail: when a guard is present, `Dup` + `IsErr` happen before the decision so the payload
> can be rewound; the exact sequence is settled during implementation, with "semantically equivalent + passes verify" as the acceptance criterion).

---

## 6. The print Builtin Fix (F6, repaying the DESIGN.md §18 known Hack)

Current state: `typeck/infer.rs` special-cases `print`, skipping parameter type and arity checks;
`check_builtin_print_wrong_arg` was changed to expect successful compilation (`typeck/tests.rs`).

The 0.0.2 plan (**strict string checking**, depending on F8's `as` cast, §3.6):

1. typeck builds a builtin signature table (`BuiltinSig`); `print`'s real signature is
   **`(string...) -> unit`** — it accepts only `string` arguments;
2. Non-string arguments (int / float / bool / function values / `box<T>` / `Result`) →
   `ArgTypeMismatch`, with help suggesting an explicit cast: `print(x as string)`
   (the scalar → string cast is in §3.6);
3. Arity ≥ 0 (`print()` is legal and prints an empty line);
4. Builtins do not own their arguments (D3): owned string arguments are copied by codegen beforehand;
5. Tests restored to correctness: `print("a")` / `print("a", "b")` / `print(42 as string)` → Ok;
   `print(1)` / `print(fib)` → `ArgTypeMismatch` (help: `as string`);
   the original `check_builtin_print_wrong_arg` is renamed/rewritten into a semantically accurate case set;
6. Remove `infer.rs`'s `print` special-case branch and the corresponding HACK comment.

> **Temporariness (final; to be written into DESIGN.md)**: tightening print to "string only + manual `as string`"
> is a 0.0.2 trade-off — trading a minimal mechanism for strict type checking, with the cast/print formatting
> implementations sharing the same helper. After generics/traits land in 0.1.0, this will be revisited: either
> restore polymorphic printing with a `Display`-like trait, or keep the strict signature; until then, printing
> non-strings always requires an explicit `as string`.

---

## 7. span_map and Runtime Diagnostics (F7)

### 7.1 SpanEntry definition (landing the item BYTECODE.md deferred)

```rust
pub struct SpanEntry {
    pub offset: u32,  // instruction start byte offset (relative to the function's code)
    pub start: u32,   // source start byte offset
    pub end: u32,     // source end byte offset
}
```

- codegen emits it per instruction (the data comes from §4.5's MIR span threading);
- The binary layout is exactly the format reserved in 0.0.1 §9; no changes needed.

### 7.2 Runtime diagnostics

- When the VM errors out, look up `span_map` by `the current frame's ip` (the contract is `offset == frame.ip at the moment of the error`,
  see TICKETS U07 implementation note 4; the original draft's `ip - 1` has been corrected); `RuntimeError` carries an optional span;
- The `fleen-vm` CLI holds the source in **`.fln` one-stop mode** and converts byte offsets into line/column:

```console
$ cargo fln crash.fln
runtime error: division by zero
  --> crash.fln:5:9
   |
 5 |     q = a / b;
   |         ^^^^^
```

- When executing a `.flnc` directly, the source is unknown: degrade to printing the function name + instruction offset;
- The exit code contract is unchanged (0/1/2).

---

## 8. Test Plan

> Follows SPEC §7: at least one test per rule, at least one test per error, and tests written before bug fixes.
> File-driven tests keep the `tests/<stage>/valid|invalid/` layout and the e2e
> `.fln + .expected / .exit + .stderr` contract.

### 8.1 New cases per stage (excerpt)

| Stage | Required valid tests | Required invalid tests |
|------|-----------|--------------|
| lexer | Tokenization of `move` `clone` `deref` `as` `?` | — |
| parser | Prefix keyword chains (`clone deref b`), `?` postfix chains, cast precedence (`deref b as string`), `deref b = v` assignment, Ok/Err patterns, all ASI trap cases (§3.5.2) | `move deref b` shape, dangling `?`, `42 as` (missing type), ASI ambiguity errors |
| resolver | ref parameter scoping, `Ok`/`Err` shadowability | ref local/global/return, duplicate borrow shapes |
| typeck | Ok/Err context inference, Ok+Err exhaustiveness, `?` chains, the three cast whitelist cases | `UseAfterMove`, `MaybeMovedAfterBranch`, `MoveOfCopyType`, `MoveOutOfBox`, `MoveOutOfGlobal`, `MoveOfBorrowed`, `OwnedArgRequiresMove`, `QuestionOutsideResultFn`, `UnhandledResult`, `UnsupportedCast`, `print(function value)`, `print(1)` |
| lower/codegen | Compare each mapping sequence in §5.5–5.6 item by item | — |
| verify | Stack-depth analysis of all v2 instructions, regression on old v1 modules | Forged malformed v2 instructions (stack mismatch, handle slot out of bounds) |
| vm | Move/Clone/Drop ordering, Ref read-only, `Boxed` deep-copy independence, entry Err exit code | `ResultMismatch`, `BorrowOutOfRange` defensive paths |

### 8.2 New E2E

```
tests/e2e/valid/
├── fib.fln                  # 0.0.1 regression: must pass unchanged
├── ownership.fln            # move/clone/conditional moves
├── box_demo.fln             # box allocation/read-write/clone independence
├── ref_demo.fln             # borrow passing; the original variable remains usable
├── question.fln             # Ok/Err/?/choose-Result full chain
├── asi.fln                  # no explicit semicolons anywhere
└── cast.fln                 # the as cast + print(x as string) (F8/F6)

tests/e2e/invalid/
├── use_after_move.fln       # compile error (.exit=1 + .stderr keywords)
├── unhandled_result.fln
├── question_in_int_main.fln
└── print_func_value.fln
```

### 8.3 Compatibility fixtures

- Keep one **v1 `.flnc` fixed-byte fixture**, asserting that 0.0.2's verify/VM can still execute it (backward compatibility);
- `fib.fln`'s explicit-semicolon version stays as the 0.0.1 regression; `asi.fln` is an independent full-pipeline case
  with no explicit semicolons + mixed style, with output anchored line by line by `.expected`.

---

## 9. Task Breakdown and Milestones (continuing 0.0.1's T numbering; this release uses the U prefix)

| Ticket | Area | crate/module | Deliverables | Depends on |
|------|------|-----------|------|------|
| U01 | Lex | `lexer/` | Move/Clone/Deref/Question tokens | — |
| U02 | Parse | `parser/` | New expressions/patterns/assignment targets + EBNF v0.0.2 | U01 |
| U03 | Parse-ASI | `lexer/` + `parser/asi.rs` + `parser/` accommodations | Front pass (newline-sensitive) + predicate pure functions + trap cases (§3.5, ASI.md) | U02 |
| U13 | F8 `as` cast | lexer→VM full chain (details in TICKETS U13) | The `As` token, the cast level, `ToStr` (0xA5), whitelist checks | U02 (**must be implemented before U04**) |
| U04 | Typeck-signatures | `typeck/` | Ok/Err constructors, the Result pattern, `?` checks, the print fix (§6) | U02, U13 |
| U05 | Typeck-ownership | `typeck/ownership.rs` | is_copy + flow-sensitive affine checking + the new error family | U02 |
| U06 | MIR+Span | `lower/` | New MIR instructions + MirInstr carrying spans | U04, U05 |
| U07 | Codegen/Verify | `codegen/`, `fleen-verify/` | v2 opcodes, span_map generation, the stack effect table, v1 compatibility | U06 |
| U08 | VM-Value | `fleen-vm/src/value.rs` etc. | Value rework (Box<str>/Boxed/Ref/Ok/Err) | — (can run in parallel with U01–U05) |
| U09 | VM-Execution | `fleen-vm/src/vm.rs` | New instruction execution, move semantics, handle dereferencing, entry Err | U07, U08 |
| U10 | Diagnostics | `codegen/`, `fleen-vm/` | Runtime span-to-line/column conversion + CLI output | U07 |
| U11 | Tests | Whole repo | The §8 test matrix + E2E + the v1 fixture | U09 |
| U12 | Docs | `docs/`, `CHANGELOG.md` | BYTECODE v2, DESIGN/SPEC/EBNF version bumps, CHANGELOG 0.0.2 | U11 |

```
U01 → U02 → U03 ─────────────┐
        ↘                    │
          U13 → U04 ┐        │
          U05 ┴─────┘→ U06 → U07 → U09 → U11 → U12
U08 ────────────────────┘ ↗        ↗
                      U10 ─────────┘
```

> U13 is a ticket added after planning (the `as` cast, F8, 2026-10-07); the F6 print fix depends on it,
> and it must be implemented before U04 (detailed breakdown in `TICKETS.md` U13).

### Milestones

| Milestone | Tickets | Marker |
|--------|------|------|
| M1 Frontend | U01–U03 | New syntax parses; all ASI trap cases green |
| M2 Semantics | U04–U05 | Ownership/`?`/print checks complete; all invalid cases green |
| M3 Backend and runtime | U06–U09 | `question.fln`, `box_demo.fln`, `ref_demo.fln` run end to end |
| M4 Diagnostics and release | U10–U12 | Runtime errors carry source positions; docs synced; `fib.fln` regression |

---

## 10. Risks and Mitigations

| Risk | Level | Mitigation |
|------|------|------|
| The ownership checker's control-flow merge logic (conditional moves, loop back edges) is more complex than expected | High | Constrain the algorithm to the conservative rules of the §4.4 draft (a move inside a loop → MaybeMoved at exit); start with straight-line code + if, treating while conservatively; cover each rule with unit tests |
| ASI traps cause silent misparses | Medium | The whitelist approach + the full §3.5.2 trap table in tests; `;` always being legal guarantees an escape hatch |
| `Value::Ref` handles escaping via forged instructions | Medium | Statically excluded by typeck rules; the VM defends against out-of-bounds handle reads (`BorrowOutOfRange`); verify gives handles no cross-frame path |
| The string semantics change breaks existing examples/tests | Medium | The impact list (§3.1.5) + the CHANGELOG Breaking section + migration hints in error help |
| Rework from D1 decision reversals (explicit/implicit move) | Medium | §2 of this document is reviewed before work begins; U05's ownership checker is decoupled from AST shape (move/clone are already explicit in the AST) |
| The span threading change touches every existing lower/codegen test | Low | The `MirInstr` wrapper is a mechanical refactor done in one PR; 0.0.1 tests only change construction sites, not assertions |
| Scope creep (struct/closures sneaking in early) | Medium | The §1.4 negative list serves as the review gate |

---

## 11. Documentation Sync Checklist (U12)

| Document | Changes |
|------|------|
| `docs/BYTECODE.md` | Bump to v0.0.2: §3.2 Value, §4.2 opcode table, §5 new instructions, §8 stack effects, §10 version table; the §7.1 span_map decision becomes official |
| `docs/DESIGN.md` | §3 binding rules gain ownership; a new "Ownership" chapter (details in §3.1–3.3); §8 `?`/Ok/Err become official; §10 box/deref become official; a new "Type cast `as` (0.0.2 temporary, revisited after generics)" section (§3.6 made official) plus a note on print's string-only temporariness (§6); §17 scope table updated; §18's known-Hack list emptied (print is fixed); the roadmap table marks 0.0.2 complete |
| `docs/SYNTAX.ebnf` | v0.0.2: prefix keywords, `?`, result_pattern, deref assignment, the cast level (`as`), semicolons become optional |
| `SPEC.md` | §5/§6 stage descriptions (ownership.rs), §14 semicolon note changed to "optional", §14 operator precedence table gains `as`, the 0.0.2 column of the §15 feature table |
| `CHANGELOG.md` | The 0.0.2 entry (including a Breaking section: string assignment, reserved words, print(function value)) |
| `README.md` | Feature overview updated |

---

## 12. Acceptance Criteria (DoD)

- [ ] `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo doc --no-deps` all green
- [ ] All 0.0.1 E2E (including the `fib.fln` explicit-semicolon version) regress **byte-for-byte** under 0.0.2
- [ ] All new E2E in §8.2 pass; the v1 `.flnc` fixture is executable
- [ ] No `print` special case in `typeck`; the `infer.rs` HACK comment removed
- [ ] print's strict string checking is in effect: `print(1)` reports `ArgTypeMismatch`, `print(1 as string)` passes
- [ ] `Value::Str(Rc<str>)` no longer appears anywhere in the repo (grep is zero)
- [ ] Runtime error output carries source line/column (`.fln` mode)
- [ ] All decision points in §2 (D1–D8) are settled in DESIGN.md
- [ ] The `cargo fln` single command works out of the box for the new features

---

> **0.0.2 in one sentence**: heap values start to have owners, errors start to flow, and semicolons start to retire.
