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
│   └── SYNTAX.ebnf         # Fleen EBNF grammar
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
└── tests/
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
        └── invalid/        # <name>.fln + <name>.exit + <name>.stderr
```

> **Single-command run**: `cargo fln <file.fln>` performs compile → verify → execute in one step
> (equivalent to `cargo run -q -p fleen-vm -- <file.fln>`; the alias is defined in `.cargo/config.toml`).

### Crate Responsibilities

| crate | Responsibility |
|-------|----------------|
| `fleen-compiler` | lexing → parsing → semantics → bytecode |
| `fleen-vm` | executes bytecode; its CLI doubles as the one-stop driver (`.fln`: compile → verify → execute) |
| `fleen-verify` | static analysis / verification tooling |

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
| Lower | Typed HIR | MIR | control-flow flattening |
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

---

## 14. Fleen Language Quick Reference

### Basic Types

```fleen
int      // 64-bit integer
float    // 64-bit float
bool     // true / false
string   // UTF-8 string
unit     // empty-value type
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

// choose expression
choose x {
    when 0 { "zero" }
    when 1 { "one" }
    otherwise { "other" }
};
```

### choose (Pattern Matching)

```fleen
choose value {
    when 0 { "zero" }
    when 1 { "one" }
    when x if x > 10 { "big" }
    otherwise { "other" }
};
```

**Semicolon rule**: the `when` clauses and the `otherwise` clause themselves take no semicolon; only when the whole `choose` is used as a statement does it take a trailing semicolon.

### Result and `?` Propagation (0.0.2)

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
box deref move clone - !    (unary prefix; box/deref/move/clone are new in 0.0.2)
;       (explicit semicolon: required in 0.0.1, optional from 0.0.2)
```

---

## 15. Feature Scope

| Feature | 0.0.1 | 0.0.2 |
|---------|-------|-------|
| `=` binding/assignment | ✅ | ✅ |
| `const` immutability | ✅ | ✅ |
| `int` / `bool` / `string` / `float` | ✅ | ✅ (string moves to owned semantics) |
| `func` single-line / multi-line | ✅ | ✅ |
| `if` / `elif` / `else` | ✅ | ✅ |
| `while` | ✅ | ✅ |
| `choose` | ✅ | ✅ (+ `Ok`/`Err` pattern) |
| `Result` | ✅ (type only) | ✅ (+ `Ok`/`Err` constructors, `?`) |
| Function types | ✅ | ✅ |
| Semicolon `;` | ✅ required | ✅ optional (ASI) |
| `?` | ❌ | ✅ |
| `move` / `clone` | ❌ | ✅ |
| `box<T>` | ❌ | ✅ |
| `ref` | ❌ | ✅ (parameters only) |
| `struct` | ❌ | ❌ (0.0.3) |
| `for` | ❌ | ❌ (0.0.3) |
| `async` | ❌ | ❌ (0.0.4) |
| FFI | ❌ | ❌ |
| Generics | ❌ | ❌ (0.1.0) |
| Operator overloading | ❌ | ❌ (0.1.0) |
| Pointers `ptr` / `addr` | ❌ | ❌ (0.0.5) |

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
