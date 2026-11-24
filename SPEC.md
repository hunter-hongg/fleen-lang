# Fleen Compiler & Language Project Specification (Unified)

> **Unified specification**: This file is the single source of truth for `AGENTS.md` and `CLAUDE.md`.
> Both `AGENTS.md` and `CLAUDE.md` are symlinks pointing to this file.
>
> Scope: coding standards for the Fleen compiler itself + design conventions for the Fleen language.
> Goal: readable, maintainable, extensible, and free of technical debt.

---

## 1. Project Overview

### Repository Structure

```
fleen/
├── Cargo.toml              # workspace root
├── Cargo.lock
├── SPEC.md                 # ← this specification (AGENTS.md / CLAUDE.md are symlinks to it)
├── AGENTS.md               # symlink → SPEC.md
├── CLAUDE.md               # symlink → SPEC.md
├── docs/
│   ├── DESIGN.md           # Fleen language design draft
│   ├── SYNTAX.ebnf         # Fleen EBNF grammar
│   ├── BYTECODE.md         # bytecode format specification
│   └── 0.0.1/ 0.0.2/ 0.0.3/  # per-milestone plans, tickets and decision records
├── examples/               # runnable examples, grouped by release
├── LESSON.md               # behavior lessons — see §LESSON.md below
├── CHANGELOG.md            # version history
├── README.md               # user-facing overview and quick start
├── fleen-compiler/         # compiler crate
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── lexer/          # lexical analysis
│       ├── parser/         # syntax analysis
│       ├── resolver/       # name resolution / scopes
│       ├── typeck/         # type inference & checking
│       ├── lower/          # HIR → MIR
│       ├── codegen/        # MIR → bytecode
│       └── lib.rs
├── fleen-vm/               # virtual machine crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # library entry: Vm / Value / RuntimeError
│       ├── main.rs         # CLI: one-stop .fln (compile→verify→execute) or .flnc
│       ├── error.rs
│       ├── frame.rs
│       ├── value.rs
│       ├── vm.rs
│       └── tests.rs
├── fleen-verify/           # verification/tooling crate
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
├── flint-cli/              # package manager (0.0.3 V08): manifest, git deps, new/build/run/test/check
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── manifest.rs     # flint.toml parse + validation (minimal TOML subset)
│       ├── lock.rs         # Flint.lock read/write
│       ├── resolve.rs      # git fetch, cache ($FLINT_HOME), dep graph + cycles
│       └── build.rs        # project-mode compile via Session
├── std/                    # the standard library, written in Fleen (embedded at compile time)
└── tests/
    ├── std/                # std library e2e tests (run by `cargo test`)
    ├── lexer/
    │   ├── valid/
    │   └── invalid/
    ├── parser/
    │   ├── valid/
    │   └── invalid/
    ├── resolver/
    │   ├── valid/
    │   └── invalid/
    ├── typeck/
    │   ├── valid/
    │   └── invalid/
    └── e2e/
        ├── valid/          # <name>.fln + <name>.expected (line-by-line stdout comparison)
        ├── invalid/        # <name>.fln + <name>.exit + <name>.stderr
        └── projects/       # multi-file project-mode fixtures (netdemo, circular, globaldemo, …)
```

> **Single-command run**: `cargo fln <file.fln>` performs compile → verify → execute in one step
> (equivalent to `cargo run -q -p fleen-vm -- <file.fln>`; the alias is defined in `.cargo/config.toml`).

### Crate Responsibilities

| crate | Responsibility |
|-------|----------------|
| `fleen-compiler` | lexing → parsing → semantics → bytecode |
| `fleen-vm` | executes bytecode; its CLI doubles as the one-stop driver (`.fln`: compile → verify → execute) |
| `fleen-verify` | static analysis / verification tooling |
| `flint-cli` | package manager: `flint.toml`/`Flint.lock`, git dependencies, `new`/`build`/`run`/`test`/`check` |

---

## 2. Language & Toolchain

| Item | Rule |
|------|------|
| Implementation language | **Rust** (2024 edition) |
| Formatting | `rustfmt`, default configuration |
| Lint | `clippy`, `#![deny(warnings)]` |
| Testing | `cargo test`, unit + integration |
| Documentation | `cargo doc`, public APIs must have doc comments |

### Build Checks

```bash
# Must pass on CI / dev machines
cargo fmt -- --check       # format check
cargo clippy -- -D warnings  # clippy, no warnings allowed
cargo test                 # all tests
cargo doc --no-deps        # documentation build
```

---

## 3. Naming Conventions

### Rust Code

| Category | Style | Example |
|----------|-------|---------|
| crate | `kebab-case` | `fleen-lexer` |
| module | `snake_case` | `token_stream` |
| type | `PascalCase` | `TokenKind` |
| trait | `PascalCase` | `Visitor` |
| function/method | `snake_case` | `parse_expr` |
| variable | `snake_case` | `current_token` |
| constant | `SCREAMING_SNAKE_CASE` | `MAX_DEPTH` |
| lifetime | short lowercase | `'a`, `'src` |

### AST/IR Nodes

| Category | Style | Example |
|----------|-------|---------|
| node type | `PascalCase` + suffix | `ExprIf`, `StmtLet` |
| enum variant | `PascalCase` | `Expr::If`, `Type::Int` |
| field | `snake_case` | `condition`, `then_branch` |

**Rule:** AST node names correspond to grammar rules. If the EBNF says `if_expr`, the AST node is `ExprIf`.

### Fleen Language-Level Naming

| Category | Style | Example |
|----------|-------|---------|
| variable/function | `snake_case` | `some_var` |
| type/struct | `PascalCase` | `SomeType` |
| module | `foo.bar` | `std.io` |

---

## 4. Error Handling

### Inside the Compiler

```rust
// Don't panic; use Result
fn parse_expr(&mut self) -> Result<Expr, ParseError> { ... }

// Panic only for "impossible" situations
fn unreachable_case(&self) -> ! {
    unreachable!("parser guarantees this cannot happen")
}
```

**Rules:**

- **Internal compiler errors**: use `Result` + a custom `Error` type
- **User code errors**: use `Diagnostic` (with Span, message, suggestions)
- **Invariant violations**: use `unreachable!` / `debug_assert!`, but explain in a comment why it cannot happen

### User Errors

```rust
pub struct Diagnostic {
    pub span: Span,
    pub kind: DiagnosticKind,
    pub message: String,
    pub notes: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}
```

**Rules:**

- Every error must carry a **Span** (source location)
- Error messages must be **actionable** (tell the user how to fix it)
- Never use `panic!` for user errors

---

## 5. Data Structures

### AST / IR

```rust
// Use enums for variants, structs for nodes
pub enum Expr {
    Int(ExprInt),
    Bool(ExprBool),
    Ident(ExprIdent),
    If(ExprIf),
    // ...
}

pub struct ExprIf {
    pub span: Span,
    pub condition: Box<Expr>,
    pub then_branch: Block,
    pub elif_branches: Vec<(Box<Expr>, Block)>,
    pub else_branch: Option<Block>,
}
```

**Rules:**

- **Every node carries a Span**
- **Use `Box` for recursion**
- **Use `Vec` for lists**
- **Use `Option` for optionality**
- **No `Rc` / `RefCell`** (unless there is a clear reason)

### IDs and Indexing

```rust
// Wrap IDs in newtypes to avoid confusion
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct FuncId(u32);

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct LocalId(u32);
```

**Rules:**

- **Use a distinct newtype for each kind of ID**
- **IDs start at 0 and are allocated contiguously**
- **Store in an `IndexVec` or `Vec`, indexed by ID**

---

## 6. Compilation Phases

### Inputs and Outputs of Each Phase

```rust
// Each phase is a pure function
pub fn asi(tokens: Vec<Token>) -> Result<Vec<Token>, ParseError>;  // 0.0.2: between Lex and Parse
pub fn parse(tokens: Vec<Token>) -> Result<Ast, ParseError> { ... }  // runs asi internally first
pub fn resolve(ast: Ast) -> Result<Hir, ResolveError> { ... }
pub fn typeck(hir: Hir) -> Result<TypedHir, TypeckError> { ... }
pub fn lower(typed_hir: TypedHir) -> Result<Mir, LowerError> { ... }
pub fn codegen(mir: Mir) -> Bytecode { ... }
```

**Rules:**

- **Each phase has explicit inputs and outputs**
- **Phases share no mutable state**
- **Each phase can be tested in isolation**
- **Never skip phases** (no "type-check while parsing" shortcuts)

### Phase Naming

| Phase | Input | Output | Responsibility |
|-------|-------|--------|----------------|
| Lex | source code | Token stream | lexing (from 0.0.2, newlines are kept as `Newline` tokens) |
| ASI | Token stream (with Newlines) | Token stream (semicolons complete, no Newlines) | semicolon insertion (new standalone pass in 0.0.2, `parser/asi.rs`, see `docs/0.0.2/ASI.md`) |
| Parse | Token stream | AST | syntax |
| Resolve | AST | HIR | name resolution, scopes |
| Typeck | HIR | Typed HIR | type inference, checking (from 0.0.2 includes the ownership-check sub-pass `typeck/ownership.rs`; still the same phase, no new standalone phase) |
| Lower | Typed HIR | MIR | control-flow flattening; the `for`-over-`[T]`/`Range` desugar is a sub-pass here (0.0.3, never in the parser — no phase is skipped) |
| Codegen | MIR | Bytecode | bytecode generation |
| Execute | Bytecode | result | VM execution |

---

## 7. Testing Standards

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_int_literal() {
        let tokens = lex("42").unwrap();
        let expr = parse_expr(&mut tokens.into_iter()).unwrap();
        assert!(matches!(expr, Expr::Int(_)));
    }
}
```

### Integration Tests

```
tests/
├── lexer/
│   ├── valid/
│   └── invalid/
├── parser/
│   ├── valid/
│   └── invalid/
├── resolver/
│   ├── valid/
│   └── invalid/
├── typeck/
│   ├── valid/
│   └── invalid/
└── e2e/
    ├── valid/      # <name>.fln + <name>.expected
    └── invalid/    # <name>.fln + <name>.exit + <name>.stderr
```

**Rules:**

- **At least one test per grammar rule**
- **At least one test per error**
- **End-to-end tests use `.fln` files + expected output**
- **Regression tests**: when fixing a bug, write the test first

### Test Naming

```rust
// Good: describes behavior
#[test]
fn parse_if_with_elif() { ... }

#[test]
fn typeck_rejects_const_reassignment() { ... }

// Bad: describes implementation
#[test]
fn test_parse_1() { ... }
```

---

## 8. Documentation Standards

### Public APIs

````rust
/// Parses an expression.
///
/// # Arguments
/// - `tokens`: the token stream produced by lexical analysis
///
/// # Returns
/// - `Ok(Expr)`: parsing succeeded
/// - `Err(ParseError)`: parsing failed, with Span and error message
///
/// # Example
/// ```
/// let expr = parse_expr(tokens)?;
/// ```
pub fn parse_expr(tokens: &mut TokenStream) -> Result<Expr, ParseError> { ... }
````

### Internal Comments

```rust
// Explain "why", not "what"
// Good: use a Vec instead of a HashMap here, because order matters
// Bad: create a Vec

// TODO: support generics
// FIXME: this edge case is not handled
// SAFETY: this unsafe is safe, because...
```

### LESSON.md (Behavior Lessons)

`LESSON.md` is the operating record of the coding agent's **process/behavior
mistakes** on this repo — the "don't do that again" list.  It is separate from
the docs (`docs/`) because it records *how work is done*, not *what the product
is*.

Rules:
- **Read `LESSON.md` before planning or editing docs/commits.**  Each entry
  is a specific past failure plus the rule that prevents it.
- The lessons are the agent's contract with the maintainer: they exist so a
  mistake is not repeated, not as a changelog of blame.
- They **do not override** any SPEC rule — they exist to help keep SPEC rules
  (e.g. "English commit messages", "verify before asserting").  Where a lesson
  conflicts with SPEC, SPEC wins.
- Entries are append-only once merged; a lesson is marked resolved only by a
  structural fix (a guard, a checklist item), not by elapsed time.

Typical triggers for a new lesson: writing files the user didn't ask for,
choosing "the easy fork today" to defer a real decision, asserting a bytecode
value / commit fact that wasn't checked against the authoritative source
(BYTECODE.md, `git log -1 --format=%B`).

---

## 9. Performance Standards

### What to Avoid

```rust
// Bad: frequent allocations
fn parse(&self) -> Vec<Token> {
    let mut tokens = Vec::new();
    for c in self.chars { tokens.push(...); }
    tokens
}

// Good: preallocate
fn parse(&self) -> Vec<Token> {
    let mut tokens = Vec::with_capacity(self.chars.len());
    ...
}
```

### Rules

- **Avoid `clone()` on hot paths**
- **Use `&str` instead of `String`** (unless ownership is needed)
- **Preallocate with `Vec::with_capacity`**
- **Use `Box<[T]>` instead of `Vec<T>`** for fixed-size data
- **Use `SmallVec` / `ArrayVec`** for small collections
- **Profile before optimizing**

---

## 10. Git Standards

### Commit Messages

```
<type>: <description>

<body>

<footer>
```

| type | Purpose |
|------|---------|
| `feat` | new feature |
| `fix` | bug fix |
| `refactor` | refactoring |
| `docs` | documentation |
| `test` | tests |
| `chore` | miscellaneous |

**Language: commit messages must be in English** (subject, body, and footer are all English; code identifiers and proper nouns in the body are exempt).

**Example:**
```
feat: support if/elif/else expressions

- Add ExprIf node
- Implement parse_if
- Add typeck rules

Closes #12
```

### Branches

```
main          # stable
dev           # development
feat/xxx      # feature branch
fix/xxx       # fix branch
```

---

## 11. Prohibitions

| Prohibited | Reason |
|------------|--------|
| `unwrap()` in non-test code | may panic |
| `expect()` in non-test code | same as above |
| `panic!` for user errors | should use Diagnostic |
| `Rc` / `RefCell` abuse | hides mutability |
| Global mutable state | concurrency issues |
| Skipping compilation phases | technical debt |
| Type checking in the parser | blurred responsibilities |
| New features without tests | unmaintainable |
| Undocumented public APIs | unusable |

---

## 12. Code Review Checklist

Check before submitting a PR:

- [ ] `cargo fmt` passes
- [ ] `cargo clippy` has no warnings
- [ ] `cargo test` all pass
- [ ] New features have tests
- [ ] Public APIs are documented
- [ ] No `unwrap()` / `expect()`
- [ ] No global mutable state
- [ ] Commit messages follow the convention
- [ ] No compilation phases skipped
- [ ] Errors carry a Span

---

## 13. Versioning

| Version | Meaning |
|---------|---------|
| `0.0.x` | early development, unstable API |
| `0.x.0` | contains breaking changes |
| `1.0.0` | stable API |

**0.0.1 goal: Fibonacci runs.**

**0.0.2 goal: foundations of ownership (plan in `docs/0.0.2/PLAN.md`).**

**0.0.3 goal: aggregation and organization — `struct`, `[T]` arrays, `for` + `Range`,
the module system, `std`, and the `flint` package manager (plan in `docs/0.0.3/PLAN.md`,
tickets in `docs/0.0.3/TICKETS.md`). V01–V08 (`struct`, arrays, `for` + `Range`,
bytecode v3, module system, std, iterator helpers, flint) are implemented and green;
V09 integrated and gated the full matrix; V10 synced the docs and bumped the version (release
publication — tag/archives — is separate and pending).**

**0.0.4 goal: cooperative concurrency — `async` / `await` over `Future<T>` coroutine frames,
the single-threaded cooperative executor, and message-passing `channel<T>` with the
borrow-free boundary (plan in `docs/0.0.4/PLAN.md`, decisions D1–D6 settled, tickets in
`docs/0.0.4/TICKETS.md`). T01–T09 are implemented and green (keywords, async frontend,
boundary rules, lowering, opcodes 0xC0–0xC7, executor, channels, hardening, integration
fixtures); T10 synced the docs and bumped the version to 0.0.4 (release publication —
tag/archives — is separate and not yet performed).**

**0.0.5 goal: the pointer/unsafe layer — `unsafe` / `trusted` func modifiers, `ptr<T>` /
`addr` / `deref_ptr` gated behind unsafe contexts (no borrow checker: every pointer use is
an unsafe op, D2), pointers never crossing the async world (`PtrAcrossBoundary`, D4) —
plus toolchain maturity: a clap-based CLI standard (T07), a six-target tag-triggered
release pipeline (T09), and std distribution with strict version pinning (T08; plan in
`docs/0.0.5/PLAN.md`, decisions D1–D8 settled, tickets in `docs/0.0.5/TICKETS.md`).
T01–T09 are implemented and green (keywords, ptr/addr frontend, unsafe gating, boundary
rules, opcodes 0xC8–0xCA + bytecode v5, the VM pointer representation, the CLI overhaul,
std distribution, the release pipeline); T10 synced the docs and bumped the version to
0.0.5 (release publication — tag/archives — is separate and not yet performed).**

**0.0.6 goal: tagged unions — `enum` declarations with ordered variants and typed payloads,
generalized `choose` exhaustiveness (the `Result` rule applied to enums, D3), bytecode v6
(`EnumNew`/`EnumIs`/`EnumPayload` + the `enum_defs` table), and an array library completion
(`concat`/`flatten`, `fold` → `foldl` plus `foldr`) (plan in `docs/0.0.6/PLAN.md`,
decisions D1–D6 settled, tickets in `docs/0.0.6/TICKETS.md`). T01–T08 are implemented and
green (keywords, enum decl/constructor, resolution, typeck + `is_copy` + `RecursiveEnum` +
exhaustiveness, choose lowering, bytecode v6 + verify/VM, array helpers + `foldl`/`foldr`,
integration fixtures/examples, docs/version sync). Release published (tag and archives cut).**

**0.0.7 goal: parameterized types + pattern matching completion — user-defined type constructors
(`enum Option<T>` …, fully applied at every use site; no bare `Option` in type position),
argument-dependent `is_copy`, full multi-payload destructuring, nested patterns, or-patterns
(`p₁ | p₂`), and type guards (`when v: T`); bytecode v7 (`enum_defs` gains one `type_param_count:
u32` per enum, no new opcodes, no `FieldTypeTag` change). `Result[T, E]` retained as a builtin;
`Result` → std migration deferred to 0.1.0. Non-breaking additive (plan in `docs/0.0.7/PLAN.md`,
decisions D1–D10 settled, tickets in `docs/0.0.7/TICKETS.md`). T01–T10 are implemented and
green (keywords, parameterized-enum frontend, resolution, typeck + argument-dependent
`is_copy` + `RecursiveEnum` + exhaustiveness, recursive-pattern typeck, choose lowering,
bytecode v7 + verify/VM, e2e fixtures/examples, the tour example + CHANGELOG entry); T11
synced the docs and the version was bumped to `0.0.7` (`Cargo.toml` / `std/VERSION`).
Version 0.0.7 Mantis Shrimp 🦐 (release published — tag and archives cut).**

**0.0.8 goal: per-file feature flags + function currying — an opt-in `#feature curry`
directive switches a file's function declaration form to the two-part curried declaration
(`add: int -> int -> int` / `add(a, b) = a + b`), makes curried arrow function types the
canonical notation, and enables implicit partial application (a short-arity call yields a
function over the remaining parameters). Bytecode v8 adds `PartialApply` (0xCE) and is
**content-driven**: a module is v8 only when a partial call was actually encoded, so a
flag-on file without partial calls stays v7 and every flag-off file is byte-identical to
0.0.7 (D16). The flag is per file, not per project: a flag-off file can import and call a
flag-on file's functions at full arity, but cannot partially apply them. Non-breaking
additive (plan in `docs/0.0.8/PLAN.md`, decisions D1–D16 settled, tickets in
`docs/0.0.8/TICKETS.md`). T01–T10 are implemented and green (the `#feature` directive +
per-file `FeatureSet`, two-part declarations, curried arrow types, resolver matching,
typeck partial typing + participation + the discarded-partial guard, lower's partial-call
reroute, codegen/verify/VM `PartialApply` with captured function values, the both-modes
e2e matrix, examples + CHANGELOG); T11 synced the docs and bumped the version to `0.0.8`
(`Cargo.toml` / `std/VERSION`).
Version 0.0.8 Cheeky Hamster 🐹 (release published — tag and archives cut).**

**0.0.9 goal: small repairs — zero-payload variant patterns resolve as variant references
(the BUG-2 fix, D1, with the joint-coverage exhaustiveness disjunct recorded as D7),
Result-`choose` completion (`otherwise` and general arms, D2 — `ChooseResultNeedsOkErrArms`
retired), the `break` / `continue` keywords with loop-bound lowering (D4 — the source
breaking set), and a std.str intrinsic batch (eight, D3). No bytecode change (D6): the
version stays v8, no new opcodes, no table changes. T01–T07 implemented and green (T07:
the example, CHANGELOG, version bump and the full gate suite landed; **release published —
the `v0.0.9` tag and the six target archives are cut; `release/0.0.9/` mirrors the 0.0.8
layout**). The T07 sweep found and fixed a latent
Result-`choose` dispatch bug — two general-path Result `choose`s (`otherwise`/guards/
nested payloads) in one function body failed bytecode verification; the dispatch now
spills before its conditional (CHANGELOG `[0.0.9]` Fixed). See `docs/0.0.9/PLAN.md`
decisions D1–D7 and `docs/0.0.9/TICKETS.md`.**

**0.0.10 goal: `Result` declared in std + string interpolation — (1) `Result` migrates from a
compiler builtin to a **declared parameterized enum** (`std/result.fln` is the language-level
source of truth, D1); `Ok` / `Err` / `Result` are available in every file without an import and
are **non-shadowable** (`ReservedBuiltinName`, D2 — the one behavior change); `?` and
choose-on-Result keep their exact 0.0.2/0.0.9 lowering (D3); the backend is untouched (D7).
(2) String interpolation `"${expr}"` — `${expr}` embeds a full expression, `\$` is a literal
`$`, and `$` is special only immediately before `{` (D4); embedded values stringify via the
`as string` path (`int`/`float`/`bool`) or embed verbatim (`string`), with no format
specifiers (D5); lowering desugars to a `std.str.concat` chain, so a literal without `${`
compiles byte-identically (D6). No bytecode change (D7): the version stays v8, no new
opcodes, no table changes — `docs/BYTECODE.md` is untouched. The only source breaking set is
a string literal containing the exact sequence `${`. T01–T07 implemented and green; see
`docs/0.0.10/PLAN.md` decisions D1–D7 and `docs/0.0.10/TICKETS.md`.**

---

## 14. Fleen Language Quick Reference

### Basic Types

```fleen
int      // 64-bit integer
float    // 64-bit float
bool     // true / false
string   // UTF-8 string
unit     // empty-value type

[T]      // array (0.0.3, owned even for [int]) | deeply covered in "Aggregation Types" below
Range    // builtin Copy type, start..end end-exclusive (0.0.3 V04)
MyStruct // a user-defined struct name is a type (0.0.3 V02)
MyEnum   // a user-defined enum name is a type (0.0.6)
Future<T>  // a suspended async coroutine, non-Copy, driven once by `e await` (0.0.4)
channel<T> // message-passing channel, non-Copy and move-only (0.0.4)
ptr<T>     // raw pointer, non-Copy and move-only, never storable; every use is an
           // unsafe op inside an unsafe/trusted body (0.0.5)
```

### Declarations

```fleen
x = 42;              // mutable binding
const y = 42;        // immutable binding
x = 43;              // assignment, legal
y = 43;              // error: y is immutable

x: int = 42;         // type annotation
const y: int = 42;   // type annotation + immutable
```

> For the three-way disambiguation of `=` (binding / assignment / shadowing), see `docs/DESIGN.md` §3.6 and §4.5.

### Ownership (0.0.2)

```fleen
s = "hello";
t = move s;        // transfers ownership; s is unusable afterwards
t = clone t;       // deep copy
b = box 42;        // heap allocation, b: box<int>
n = deref b;       // read the value inside the box (a copy)
deref b = n + 1;   // write the value inside the box

func shout(s: ref string): int { print(s); 42 }  // read-only borrow, parameters only
```

> Copy types (`int` / `float` / `bool` / `unit` / function values) are unaffected.
> For details see `docs/DESIGN.md` §10 and `docs/0.0.2/PLAN.md` §3.1–3.3.

### Functions

```fleen
// Single line
func add(a: int, b: int): int = a + b

// Multi-line
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

// No return value
func greet(name: string) {
    print("Hello, ", name);
}

// Functions are first-class values
func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)
result = apply(add, 1, 2);
```

### Feature Flags and Curried Declarations (0.0.8)

```fleen
#feature curry      // per-file opt-in, only before the first declaration (D1)

// In a flag-on file a function is declared in two parts: a single-line
// signature holding a curried arrow type, then a body with named parameters.
// `func` is forbidden in these files (FuncFormForbiddenInCurryFile, D3).
add: int -> int -> int
add(a, b) = a + b

greet: string -> string
greet(name) { name }

// the signature is the body's type: an `async` function's signature names
// the Result the body returns, and callers still see `Future<T>`
async half: int -> int -> Result[int, string]
half(n, d) {
    if d == 0 { Err("zero") } else { Ok(n / d) }
}

// `pub` and the `async` / `unsafe` / `trusted` modifiers belong on the
// signature line only (PubOnCurriedBody / ModifierOnCurriedBody)
pub unsafe peek: int -> int
peek(x) = x

// arrow types are right-associative: `int -> int -> int` is one function of
// two parameters. A parenthesized parameter-side arrow keeps its grouping
// (`(int -> int) -> int` takes a function); `() -> T` is a zero-parameter
// function (`unit -> T` is a one-parameter function of `unit`). The legacy
// tuple spelling `(int, int) -> int` stays legal (D4).
apply_twice: (int -> int) -> int -> int
apply_twice(f, x) = f(f(x))

// implicit partial application (D5): a call with fewer arguments than the
// callee's parameters yields a function over the remaining ones. Only fixed-
// arity user functions participate; variadic builtins (`print`, `println`),
// functions with `ref` / `ptr<T>` parameters, and `unsafe` / `trusted`
// functions do not (D9 keeps the §12 static unsafe check total).
inc = add(1);        // inc: int -> int
five = inc(4);       // 5
copy_me = add;       // the function itself (D11: partials are Copy)

// a partial bound to a name is fine; a partial used only as a discarded
// expression statement is PartialApplicationDiscarded (D10)
add(1);              // error: the partial's value is discarded
```

> **Rules (0.0.8)**: the flag is **per file**, never per project — two files in one
> project may disagree (`docs/DESIGN.md` D6). An importer's flag does not change what an
> imported function is; partial application is decided at the **call site**, so a flag-on
> file may partially apply an ordinary flag-off `func`, while a flag-off file keeps its
> pre-0.0.8 `ArityMismatch` for too-few arguments (D5/D16). A `call()` with zero
> arguments yields the function itself, not a partial. Functions with parameters of a
> non-Copy type keep the ordinary full-call ownership rules for the arguments that are
> supplied. Captured arguments live in the function value at runtime; partials are Copy
> and their captures are deep-copied by `clone` / assignment (D11). Bytecode v8 is
> content-driven: a module reaches v8 only if a `PartialApply` was actually encoded
> (D14), so a flag-on file without partials stays v7 and every flag-off module is
> byte-identical to 0.0.7. The 0.0.8 known limitation (a partial over an
> inline-lowered builtin, for example `range_from(0)`, failed at runtime with
> `UnknownBuiltin`) was fixed by commit `8dbbc8b` — inside the `v0.0.8` tag:
> captured builtins run end to end (fix record: CHANGELOG `[0.0.8]`).
> See `docs/DESIGN.md` §5/§9/§10.8
> and `docs/0.0.8/PLAN.md` D1–D16.

### Aggregation Types (0.0.3) and Enums (0.0.6)

```fleen
// struct (0.0.3) — a user-defined aggregate; is_copy(S) = all fields Copy
struct Point {
    x: int,
    y: int,
}
p = Point { x: 1, y: 2 };    // literal; all fields must be initialized
n = p.x;                     // field read (a copy; deep for owned fields)
p.y = 3;                     // field write (direct field of a local/global only)

// arrays [T] (0.0.3) — an owned type for every T (even [int])
a = [1, 2, 3];               // a: [int]; empty literals need a type annotation
x = a[0];                    // read: bounds-checked copy
a[1] = 10;                   // write: bounds-checked
t = clone a;                 // deep copy
u = move a;                  // transfer; a is unusable afterwards

// Range (0.0.3, bytecode v3) — a builtin Copy type; source construction is
// std.collection.range (V06), iterated by `for` (D4/D5)

// enum (0.0.6) — a tagged union; is_copy(E) = all payloads of all variants Copy
// 0.0.7: enums may declare type parameters — see "Parameterized Types" below
enum Shape {
    Point,
    Circle(float),
    Rect(float, float),
}
s = Shape.Circle(2.0);       // variant constructor (dotted form); Rect(3.0, 4.0) also works
func area(s: Shape): float {
    choose move s {                          // an owned enum is consumed by `choose`
        when Point { 0.0 }
        when Circle(r) { 3.14159 * r * r }
        when Rect(w, h) { w * h }            // full multi-payload destructuring (0.0.7)
    }                                        // exhaustive (one arm per variant), no otherwise needed
}
// `enum` is a reserved word; recursive enums are rejected (`RecursiveEnum`);
// enums are not comparable (`==` / `as string` are type errors).
```

### Parameterized Types (0.0.7)

```fleen
enum Option<T> {
    Some(T),
    None,
}
enum Pair<A, B> {
    Pair(A, B),
}

o1: Option<int> = Option.Some(42);   // fully applied at every use site (D1)
o2 = Option.Some("hi");              // the argument's type fixes T = string
p: Pair[int, string] = Pair.Pair(1, "x");   // bracket spelling, Result[T, E] style (D9)
// bare `Option` in type position is `TypeConstructorNeedsArgs`
// `Option<int, string>` (too many args) is `ArityMismatch`

func get(o: Option<int>): int {
    choose o {                            // `Option<int>` is Copy — no `move` needed
        when Some(v) { v }                // multi-payload + nested patterns work too
        when None { 0 - 1 }
    }
}
n2 = get(Option.Some(7));                 // n2 = 7

n = choose 3 {                            // or-patterns (D6) and type guards (D7)
    when 1 | 2 | 3 { 1 }
    when v: int if v > 100 { 2 }
    otherwise { 0 }
};
```

> **Rules (0.0.7)**: type parameters are declaration-local identifiers (they are not
> values); every use site is **fully applied** — both `<>` and `[]` spellings accepted
> (D9). `is_copy(Option<T>)` is argument-dependent (D2): `Option<int>` is Copy,
> `Option<string>` is owned (needs `move` / `clone`). Recursive parameterized enums stay
> rejected (`enum List<T> { Cons(T, List<T>) }` is `RecursiveEnum`, D3). Or-pattern
> alternatives must bind the same names with the same types
> (`OrPatternBindingMismatch`); a type guard is compile-time only. Bytecode v7 adds one
> descriptive `type_param_count: u32` per `enum_defs` entry; no new opcodes (D8).
> `Result[T, E]` stays builtin (D9). See `docs/DESIGN.md` §7/§9.8 and
> `docs/0.0.7/PLAN.md` D1–D10.

### Modules and the std Library (0.0.3)

```fleen
// Modules (V05): `import` resolves a dotted path to a `.fln` file; top-level
// `pub` controls cross-module visibility. `--project <dir>` compiles a whole tree.
import auth;             // src/auth.fln (or auth/mod.fln)
import net.tcp;          // src/net/tcp.fln
pub func router(): int = net.tcp.make()   // pub: importable by other modules

// std (V06): the prelude is `print` only; everything else is imported by name
// (leaf modules — there is no `pub use` re-export in 0.0.3).
import std.io;                 // print / println / read_line
import std.collection.range;   // the Range constructors: range(n) / range_from(a, b)
for i in range_from(0, 10) { std.io.println(i as string); };   // 0..10, end-exclusive
```

### Async / await and Channels (0.0.4)

```fleen
// async func: the caller sees Future<T>; the body still returns T
async func half(n: int): Result<int, string> {
    if n == 0 { Err("cannot halve 0") } else { Ok(n / 2) }
}

r = half(10) await;               // postfix `e await` drives the future (Future<T> → T)
// `e await?` composes with ?: drive, then propagate the Err — inside a
// Result-returning function:
func halve(n: int): Result<int, string> {
    v = half(n) await?;
    Ok(v)
}

std.async.run([task_a(), task_b()]);   // cooperative executor: spawn + join (round-robin);
                                       // the tasks are [Future<unit>]
std.io.sleep(5);                  // awaitable timer (parks the task inside an async body)

ch = channel<string>(4);          // bounded buffer; channel<T>(0) is a rendezvous channel
peer = std.concurrent.split(ch);  // the one explicit fork point (a second handle, same buffer)
async func producer(c: channel<string>) { c.send("hi"); }   // fresh values transfer naturally
async func consumer(c: channel<string>) {
    s = c.recv();                 // receiver takes ownership of the payload
    std.io.println(s);
}
std.async.run([producer(move ch), consumer(move peer)]);    // move a handle into each task
std.async.run([producer(move ch), consumer(move peer)]);    // move a handle into each task
```

> **Ownership boundary (0.0.4)**: `async` / `await` / `channel` are reserved words; the async
> world is borrow-free — no `ref` crosses an await/task/channel boundary and async bodies may
> read but never write globals; channels are move-only (`clone ch` is an error) and payloads
> travel by ownership `move`. See `docs/DESIGN.md` §6/§9.6/§9.7/§10.6 and `docs/0.0.4/PLAN.md` D1–D6.

### Pointers and Unsafe (0.0.5)

```fleen
// creating and using a pointer is an unsafe operation (D2): it needs an
// unsafe / trusted function body — Fleen has no borrow checker
unsafe func raw_op(x: int): int {
    p = addr x;            // p: ptr<int> — take the address of a place
    n = deref_ptr p;       // read through p (a copy out)
    deref_ptr p = n + 1;   // write through p
    n
}

trusted func safe_wrapper(x: int): int = raw_op(x)   // attested; callable from anywhere
result = safe_wrapper(42);    // ok — trusted is callable from safe code
// raw_op(42)                // error: UnsafeCallOutsideUnsafe

async unsafe func risky(n: int): int { n }   // D4: `unsafe async` (either order)
```

> **Pointer rules (0.0.5)**: `ptr<T>` is non-Copy and move-only (`clone p` is
> `CloneOfPointer`) and **non-storable** (`PtrNotStorable` — no struct fields, array
> elements, box cells, or globals: a pointer lives in a local, a parameter, or a return
> value). `addr` rejects immutable bases (`AddrOfImmutable` — parameters are immutable)
> and `ref`-parameter bases (`AddrOfRefParam`). A `ptr<T>` never crosses an await/task/
> channel boundary (`PtrAcrossBoundary`, D4 — sibling of `RefInAsync`). `ptr` / `addr` /
> `deref_ptr` / `unsafe` / `trusted` are reserved words (the source breaking change of
> 0.0.5). Known limit: an `unsafe func` called *through a function value* escapes the
> static call check — the VM backstop on `LoadPtr` / `StorePtr` is the last line.
> See `docs/DESIGN.md` §10.7/§12 and `docs/0.0.5/PLAN.md` D1–D8.

### Control Flow

> **Note on semicolons**: 0.0.1 requires explicit semicolons `;`; from 0.0.2 ASI (automatic semicolon insertion) is in place,
> so semicolons are optional (explicit semicolons remain legal), with line-break-sensitive semantics (joining statements on one line is illegal).
> See `docs/0.0.2/ASI.md` for the architecture and decision rules (summary in `docs/DESIGN.md` §3.9).
> **Note**: a `func` declaration has no trailing semicolon; the last expression in a function body takes no semicolon, otherwise its return value is discarded.
> **Expression-statement rules**:
> - When control-flow expressions such as if, while, or choose are used as statements, 0.0.1 requires a semicolon (omittable from 0.0.2)
> - In a choose-when construct, the `when` clauses and the `otherwise` clause themselves take no semicolon
> - Only when the whole `choose` is used as an expression statement does it take a trailing semicolon
>
> **Semantics of `=` in `while` bodies** (DESIGN.md §4.5): inside the loop body, `x = expr` is an **assignment to the outer mutable `x`**, not a new binding/shadowing. See `docs/DESIGN.md` for details.

```fleen
// if expression (has a return value)
if x > 0 { "positive" } elif x < 0 { "negative" } else { "zero" };

// while loop
while x < 10 {
    x = x + 1;    // assignment to the outer x (§4.5), not shadowing
};

// for loop (0.0.3) — over an array [T] or a Range; loop var is a fresh
// immutable binding per iteration; the iterable is evaluated once
for x in [1, 2, 3] {
    total = total + x;
};
for i in range_from(0, 10) {   // Range (std.collection.range, 0.0.3 V06)
    print(i as string);
};

// choose expression
choose x {
    when 0 { "zero" }
    when 1 { "one" }
    otherwise { "other" }
};
```

### choose (Pattern Matching)

> **0.0.7 (implemented)**: patterns are recursive — multi-payload destructuring
> `when Pair(a, b)`, nested patterns `when Ok(Some(x))`, or-patterns `when 1 | 2`
> (alternatives bind the same names with the same types), and type guards
> `when v: int` (compile-time ascription). Exhaustiveness treats an or-pattern as
> the union of its alternatives' coverage. See `docs/DESIGN.md` §7 and
> `docs/0.0.7/PLAN.md` D4–D7; examples in "Parameterized Types" above.
>
> **Known limits (post-release findings, see `docs/0.0.7/BUGS.md`)**:
> (1) a `choose` arm over an **enum** scrutinee whose block ends in an
> `if`/`else` expression fails bytecode verification (stack-depth mismatch);
> the scalar-scrutinee path is unaffected — **fixed in 0.0.8** (commit
> `a5b8bd7`, inside the `v0.0.8` tag; fixture
> `tests/e2e/valid/choose_arm_if_else.fln`). (2) Within one pattern a bare
> name may appear at most once — zero-payload variant patterns occupied
> their name, so `when P(None, None)` was a `DuplicateBinding` resolve
> error — **fixed in 0.0.9** (T01: zero-payload variant patterns are
> variant references, not bindings; `docs/0.0.9/PLAN.md` D1/D7). Repeated
> *binding* names still error.

```fleen
choose value {
    when 0 { "zero" }
    when 1 { "one" }
    when x if x > 10 { "big" }
    otherwise { "other" }
};
```

**Semicolon rule**: the `when` clauses and the `otherwise` clause themselves take no semicolon; only when the whole `choose` is used as a statement does it take a trailing semicolon.

### String Interpolation (0.0.10)

```fleen
n = 42;
greeting = "n=${n}!";            // "n=42!" — `${expr}` embeds a full expression
table = "row ${a + b} of ${n}";  // any expression; nested string literals work too
price = "cost: $5";              // a `$` NOT before `{` is literal — unchanged
pay = "owed: \$${amount}";       // `\$` is a literal `$` before an interpolation
name = "x${i}";                  // type error: `i` must be int/float/bool/string
```

> **Rules (0.0.10)**: a string literal containing `${` is lexed into an interpolation token —
> literal chunks interleaved with embedded expressions — and lowered to a `std.str.concat`
> chain over the chunks (D6), so a literal **without** `${` compiles byte-identically to 0.0.9.
> `${expr}` embeds a full expression (recursive; nested interpolating strings allowed, D4);
> `int` / `float` / `bool` stringify via exactly the `as string` path and `string` embeds
> verbatim (deep-copied, never moved) — any other type is `InterpUnsupportedType`, and there are
> **no format specifiers** (D5). `\$` is the only new escape (a literal `$`); without it `$` is
> special **only immediately before `{`**. EOF inside `${…}` is `UnclosedInterpolation`; a stray
> `\` is `InvalidEscape`. A global initializer cannot interpolate yet (`ComplexGlobalInit`) —
> interpolate in a function body. The only breaking set: a source string containing the exact
> sequence `${`. See `docs/DESIGN.md` §2/§9.9 and `docs/0.0.10/PLAN.md` D4–D6.

### Result and `?` Propagation (0.0.2; `Result` declared in std as of 0.0.10)

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func safe_half(a: int): Result<int, string> {
    q = div(a, 2)?;    // on Err the whole function returns that Err immediately; on Ok the payload is extracted
    Ok(q)
}

// Ok/Err patterns in choose: both arms present means exhaustive, no otherwise needed
res = choose safe_half(10) {
    when Ok(v) { v }
    when Err(e) { 0 - 1 }
};

// print accepts only string; convert scalars with `as string` (temporary in 0.0.2, to be revisited after generics)
print(res as string);
```

> **0.0.10 (implemented)**: `Result` is now a **declared parameterized enum** in std
> (`std/result.fln`, D1) rather than a compiler builtin; `Ok` / `Err` / `Result` are available
> in every file **without an import** and are **non-shadowable** (`ReservedBuiltinName`, D2 —
> a behavior change from ≤0.0.9, where they could be overwritten). The syntax, `?` propagation,
> choose-on-Result semantics, and lowering are **unchanged** (D3), and the backend is untouched
> (D7). To import explicitly: `import std.result;`. See `docs/DESIGN.md` §8 and
> `docs/0.0.10/PLAN.md` D1–D3.

### Operator Precedence (low to high)

```
or
and
== !=
< > <= >=
+ -
* / %
as     (type cast, 0.0.2: binds tighter than binary operators, looser than unary prefixes, same as Rust)
?      (postfix, 0.0.2)
box deref move clone addr deref_ptr - !    (unary prefix; box/deref/move/clone new in 0.0.2, addr/deref_ptr in 0.0.5)
() [i] .f await ?   (postfix suffixes: call, index, field access (0.0.3), `await` (0.0.4), and `?` — bind tightest, chain left-to-right)
;       (explicit semicolon: required in 0.0.1, optional from 0.0.2)
```

---

## 15. Feature Scope

| Feature | 0.0.1 | 0.0.2 | 0.0.3 | 0.0.4 | 0.0.5 | 0.0.6 | 0.0.7 | 0.0.8 | 0.0.9 | 0.0.10 |
|---------|-------|-------|-------|-------|-------|-------|-------|-------|-------|---------|
| `=` binding/assignment | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `const` immutability | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `int` / `bool` / `string` / `float` | ✅ | ✅ (string moves to owned semantics) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `func` single-line / multi-line | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `if` / `elif` / `else` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `while` | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `choose` | ✅ | ✅ (+ `Ok`/`Err` patterns) | ✅ | ✅ | ✅ | ✅ (generalized to enums) | ✅ (+ recursive patterns) | ✅ | ✅ (+ zero-payload variant patterns, D1/D7) | ✅ |
| `Result` | ✅ (type only) | ✅ (+ `Ok`/`Err` constructors, `?`) | ✅ | ✅ | ✅ | ✅ | ✅ (stays builtin, D9) | ✅ | ✅ (+ `otherwise` on a Result scrutinee, D2) | ✅ (declared in std, non-shadowable; D1/D2) |
| Function types | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ (curried arrow notation behind the flag) | ✅ | ✅ |
| Semicolon `;` | ✅ required | ✅ optional (ASI) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `?` | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `move` / `clone` | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `box<T>` | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `ref` | ❌ | ✅ (parameters only) | ✅ | ✅ (async borrow-free) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `struct` | ❌ | ❌ | ✅ (V02) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `[T]` arrays | ❌ | ❌ | ✅ (V03: literal, `a[i]` read/write, bounds-checked) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `Range` | ❌ | ❌ | ✅ (V04: builtin Copy type, end-exclusive; iterated by `for`) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `for` loop | ❌ | ❌ | ✅ (V04: over `[T]` or `Range`) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Bytecode v3 | ❌ | ❌ | ✅ (V02–V04: struct 0xB0–0xB3, array 0xB4–0xB8, range 0xB9–0xBB, `struct_defs` table) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Modules (`import` / `pub`) | ❌ | ❌ | ✅ (V05) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `std` library | ❌ | ❌ | ✅ (V06: `io`/`str`/`num`/`arr`/`collection`/`os`/`time`) | ✅ (+ `async`/`concurrent`) | ✅ | ✅ (+ `concat`/`flatten`/`foldl`/`foldr`) | ✅ (untouched in 0.0.7) | ✅ (untouched in 0.0.8) | ✅ (+ `std.str` batch: 8 intrinsics) | ✅ |
| `flint` package manager | ❌ | ❌ | ✅ (V08) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `async` / `await` / `Future<T>` | ❌ | ❌ | ❌ | ✅ (T02/T05) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Cooperative executor | ❌ | ❌ | ❌ | ✅ (T06) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Channels (`channel<T>`) | ❌ | ❌ | ❌ | ✅ (T07) | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `unsafe` / `trusted` / `ptr` | ❌ | ❌ | ❌ | ❌ | ✅ (T01–T06) | ✅ | ✅ | ✅ | ✅ | ✅ |
| `enum` | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T01–T06: typed payloads, `choose` exhaustiveness, v6 `EnumNew`/`EnumIs`/`EnumPayload`) | ✅ | ✅ | ✅ | ✅ |
| Bytecode v7 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T07: `enum_defs` gains one `type_param_count: u32`, no new opcodes) | ✅ | ✅ | ✅ |
| Parameterized enums (`enum Option<T>`) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T01/T03/T04: fully applied use sites, argument-dependent `is_copy`, D1–D3) | ✅ | ✅ | ✅ |
| Nested / or / type-guard patterns | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T02/T05/T06: multi-payload, nested, `p₁ \| p₂`, `when v: T`) | ✅ | ✅ | ✅ |
| Per-file feature flags (`#feature curry`) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T01: `#feature` header directive, per-file `FeatureSet`, D1/D6) | ✅ | ✅ |
| Curried declarations + arrow types | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T02/T03: two-part signature/body, right-associative `->`, `() -> T`, D2–D4) | ✅ | ✅ |
| Implicit partial application | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T05/T06: fixed-arity user functions, captured arguments, Copy partials, D5/D9–D11) | ✅ | ✅ |
| Bytecode v8 | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T07: `PartialApply` 0xCE, content-driven version — only modules that encode a partial reach v8, D13/D14) | ✅ (stays content-driven — no new opcode in 0.0.9, D6) | ✅ (stays content-driven — no new opcode in 0.0.10, D7; version stays v8) |
| `break` / `continue` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T03/T04: reserved words, loop-exit/continue lowering, no new opcode, D4) | ✅ |
| String interpolation `"${expr}"` | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ (T04–T06: `${expr}` + `\$`, lowers to `std.str.concat`, literals without `${` byte-identical; D4–D6) |
| FFI | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Generics | ❌ | ❌ | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) |
| Operator overloading | ❌ | ❌ | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) | ❌ (0.1.0) |

**Status of 0.0.3**: V01–V10 complete. V01–V08 are implemented and green (`FileId`/`SourceMap`,
`struct`, arrays, `for` + `Range`, module system with `import` / `pub`, the std library,
iterator helpers, and the `flint` package manager); V09 integrated the compatibility fixtures
and the full §8 matrix; V10 synced the documentation and bumped the version to 0.0.3 (public
release — tag and archives — is separate and not yet performed).

**Status of 0.0.5**: T01–T10 complete. The pointer/unsafe layer (`ptr<T>` / `addr` /
`deref_ptr`, the `unsafe` / `trusted` func modifiers, `PtrAcrossBoundary`) and the
toolchain features (clap CLI standard, six-target release pipeline, std distribution with
strict version pinning) are implemented and green; bytecode writes v5, readers accept
`{1, 2, 3, 4, 5}`. T10 synced the documentation and bumped the version to 0.0.5 (release
publication — tag/archives — is separate and not yet performed; see
`docs/0.0.5/PLAN.md` and `docs/0.0.5/TICKETS.md`).

**Status of 0.0.6**: T01–T08 complete. Tagged unions (`enum` with ordered variants and typed
payloads, `is_copy` = all payloads Copy, D1), the `RecursiveEnum` ban (D2), generalized
`choose` exhaustiveness and constructor resolution (D3), bytecode v6 (`EnumNew` 0xCB /
`EnumIs` 0xCC / `EnumPayload` 0xCD + the `enum_defs` table, `FieldTypeTag::Enum = 12`;
writers emit version 6, readers accept `{1, 2, 3, 4, 5, 6}`, D4), and the array library
completion (`std.arr.concat`/`flatten` intrinsics, `fold` → `foldl` plus `foldr`, D5) are
implemented and green. The E2E valid `enum_demo.fln` and the invalid fixtures
(non-exhaustive choose, recursive enum, `as string` on enum, `==` on enum) are in place;
the full suite and all examples pass. Breaking set (D6): `enum` reserved; `fold` → `foldl`.
Version 0.0.6 (release published — tag and archives cut).

**Status of 0.0.7**: T01–T10 complete. Parameterized enums (type parameters in the enum
head, declaration-local scope, fully-applied use sites with both `<>` and `[]` spellings,
argument-dependent `is_copy`, the `RecursiveEnum` ban carried through parameterized
payloads), recursive patterns (multi-payload destructuring, nested patterns, or-patterns
with binding consistency, type guards as compile-time ascriptions, payload-aware
exhaustiveness), bytecode v7 (writers emit version 7, readers accept `{1, 2, 3, 4, 5, 6,
7}`; `enum_defs` gains one descriptive `type_param_count: u32` per enum; no new opcodes, no
`FieldTypeTag` change), e2e fixtures/examples, the tour project, and the CHANGELOG entry
are implemented and green. T11 synced the documentation; the version was bumped to 0.0.7
(`Cargo.toml` / `std/VERSION`) and the release was published (the `v0.0.7` tag and the six
target archives are cut; `release/0.0.7/` mirrors the 0.0.6 layout). Breaking set: none
(non-breaking additive, D10).
See `docs/0.0.7/PLAN.md` and `docs/0.0.7/TICKETS.md`.

**Status of 0.0.8**: T01–T11 complete. Per-file feature flags (the `#feature curry`
directive before the first declaration, a per-file `FeatureSet` threaded from `FileId`,
D1/D6), two-part curried declarations (single-line signature holding an arrow function
type plus a named-parameter body, matched in the resolver by name and arity, `func`
forbidden in a flag-on file, D2/D3), curried arrow types (right-associative `->`, a
zero-parameter `() -> T`, parameter-side grouping preserved, the legacy tuple spelling
retained, D4), implicit partial application (fixed-arity user functions only, a short call
typed as the remaining function, a discarded partial is `PartialApplicationDiscarded`, no
subtyping, D5/D9/D10), the D11 capture semantics (partials are Copy; their captured
arguments are deep-copied on the Copy paths), `PartialApply` (0xCE) with the content-driven
v8 version (only a module that encodes a partial is v8, D13/D14), the flag-blind VM (D15),
the both-modes e2e matrix, `examples/0.0.8/`, and the CHANGELOG entry are implemented and
green. T11 synced the docs and bumped the version to 0.0.8 (`Cargo.toml` / `std/VERSION`);
doc drift fixed while doing so: the card's "DESIGN.md §14" does not exist as a language
reference (DESIGN.md §14 is Explicit Rejections), so the two-part declaration and arrow
notation were written into DESIGN.md §5/§9, the capture rules into §10.8, the matrix into
§17 and the roadmap into §18, with the as-built notes recorded in `docs/0.0.8/TICKETS.md`.
The release was published (the `v0.0.8` tag and the six target archives are cut;
`release/0.0.8/` mirrors the 0.0.7 layout). Breaking set: none (non-breaking additive, D16). See `docs/0.0.8/PLAN.md` and `docs/0.0.8/TICKETS.md`.

---

## 16. Summary

| Aspect | Rule |
|--------|------|
| Language | Rust 2024 |
| Naming | Rust standard + AST nodes match the grammar |
| Errors | Result + Diagnostic |
| Data | enum + struct + Span + ID |
| Phases | Lex → Parse → Resolve → Typeck → Lower → Codegen → Execute |
| Testing | unit + integration + end-to-end |
| Documentation | public APIs must be documented |
| Performance | avoid clone on hot paths, preallocate |
| Git | Conventional Commits |
| Prohibited | unwrap, panic, global mutability, skipping phases |
| Semicolon | required in 0.0.1 `;`; optional from 0.0.2 (ASI) |
