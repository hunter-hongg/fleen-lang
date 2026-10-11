# Fleen 0.0.2 Implementation Ticket Breakdown

> **Goal**: Ownership foundations — get `move` / `clone` / `box<T>` / `ref` / `?` (including `Ok`/`Err`) / ASI working end-to-end,
> plus the temporary `as` type cast (U13, a prerequisite for the print fix), and pay down three debts from 0.0.1: the print hack, `Rc<str>`, and `span_map`.
> **Basis**: `docs/0.0.2/PLAN.md` (finalized, decision points D1–D8 frozen), `docs/BYTECODE.md` v0.0.2,
> `docs/DESIGN.md` §3.8–3.9 / §8 / §10, `docs/0.0.1/TICKETS.md` (current implementation state)
> **Status** (2026-10-10): U01–U07 and U13 are complete. U06 landed in `lower/`: MirInstr carries
> Span, 13 new MIR instructions, lowering of box/deref/move/clone/`?`/Ok/Err/Result-choose, ref arguments
> (globals go through a temporary slot), and rejection of global ref arguments (`RefArgInGlobalInit`).
> U07 landed in `codegen/` + `fleen-verify`: encoding of 13 v2 instructions (0x80–0xA4), `Module.version = 2`,
> per-instruction `span_map` generation, `flnc` accepting `{1,2}`, verify stack-effect table + slot/gid validation + version gate.
> **Findings during U07** (see the "Implementation notes" in the U07 section): the stack effect of `AllocBox`
> should be `v → b` (net 0); the original table in PLAN §5.2 / BYTECODE.md §8 writing `+1` was a typo and has been corrected;
> v2 instructions inside a v1 module are explicitly rejected by `V2OpcodeInV1Module` (not "naturally fine");
> the VM side needs a wildcard opcode arm + the version gate widened to `{1,2}`, otherwise the new
> `Opcode` variants break the fleen-vm build.
> **U07 review hardening** (implementation notes items 5–6): fixed two malformed-input defects dating back to 0.0.1 —
> verify panicking on an empty function body (now reports `EmptyFunctionBody`) and unbounded preallocation in
> `flnc` decoding (now clamped via `capacity_hint`); added a VM `InvalidOpcode` regression test and per-instruction
> valid/malformed verify cases.
> **Result-choose guard / `otherwise`**: the U07 decision is to **defer further** (see "Scope decision" in the U07 section);
> it will be done with U09 or a separate ticket as a general arm-chain rewrite.
> Spec docs were synced during planning; U12 only has post-implementation wrap-up left (version numbers, the official CHANGELOG);
> the `DESIGN.md` §18 Hack section was emptied early during the U07 review (see the U12 checkboxes).

---

## Overview

| Ticket | Area | Phase | Crate | Module | Deliverable | Depends on |
|------|------|------|-------|------|------|------|
| U01 | Lexing | Lex | fleen-compiler | `lexer/` | Move/Clone/Deref/Question tokens | — |
| U02 | Parsing | Parse | fleen-compiler | `parser/` | New AST nodes + EBNF alignment | U01 |
| U03 | F5 ASI | Lex + Parse | fleen-compiler | `lexer/` + `parser/asi.rs` | Optional semicolons (prerequisite pass, newline-sensitive, see ASI.md) | U02 |
| U13 | F8 `as` cast (temporary) | All phases | fleen-compiler, fleen-vm | `lexer/` `parser/` `typeck/` `lower/` `codegen/` verify `vm.rs` | `As` token, cast layer, `ToStr` (0xA5), whitelist checks | U02 (**implemented before U04**) |
| U04 | F4+F6 | Typeck | fleen-compiler | `typeck/` | Ok/Err construction, Result patterns, `?`, print fix | U02, U13 |
| U05 | F1 ownership | Typeck | fleen-compiler | `typeck/ownership.rs` | is_copy + flow-sensitive affinity checks | U04 |
| U06 | Foundation | Lower | fleen-compiler | `lower/` | New MIR instructions + MirInstr carries Span | U04, U05 |
| U07 | Foundation | Codegen/Verify | fleen-compiler, fleen-verify | `codegen/`, `verify.rs` | Bytecode v2 + span_map generation + stack-effect table | U06 |
| U08 | Foundation | VM-Value | fleen-vm | `value.rs` etc. | Value ownership representation refactor | — (can run in parallel with U01–U05) |
| U09 | F1–F4 execution | Execute | fleen-vm | `vm.rs` | Execution of 13 new instructions + entry Err | U07, U08 |
| U10 | F7 diagnostics | Codegen/VM | fleen-vm | `diag.rs` (new) | Runtime errors carry source line/column | U07 |
| U11 | Testing | Integration | tests/, all crates | Whole repo | Test matrix + E2E + v1 fixture | U09 |
| U12 | Wrap-up | Release | Whole repo | Docs/Cargo.toml | 0.0.2 release | U11 |

> **Phase boundaries unchanged** (SPEC §6): the ownership check is a sub-pass within Typeck
> (`typeck/ownership.rs`) and ASI is decision logic within the Parser; neither adds a new independent phase.

---

## U01: Lex — New Keywords and `?`

### Files involved
```
fleen-compiler/src/lexer/
├── token.rs         # 4 new TokenKind variants
└── lexer_impl.rs    # keyword/punctuation mapping tables + unit tests (mod tests)
```

> Note: since 0.0.1 the tests live in `lexer_impl.rs`'s `mod tests`; there is no separate `tests.rs`.

### New TokenKind entries
```rust
/// `move` keyword (0.0.2: ownership transfer)
Move,
/// `clone` keyword (0.0.2: deep copy)
Clone,
/// `deref` keyword (0.0.2: read/write the value inside a box)
Deref,
/// `?` (0.0.2: Result propagation suffix)
Question,
```

### Implementation requirements
- `lexer_impl.rs`: `"move" => Move`, `"clone" => Clone`, `"deref" => Deref`; `'?' => Question`
- `BoxType` (`box`) / `RefType` (`ref`) already exist, **leave them alone**; `box` in expression
  position reuses the `BoxType` token (the parser distinguishes by syntactic position, see U02)
- `is_keyword()` / `keyword_str()` / `Display` updated accordingly
- **Breaking change**: `move` / `clone` / `deref` change from valid identifiers to reserved words
  (`box` / `ref` have been reserved words since 0.0.1). The lexer itself only emits keyword tokens
  without erroring; "reserved word used as a binding name" (`move = 1;`) is reported by the parser
  where an identifier is expected, with cases in `tests/parser/invalid/` (see U02)

### Tests
- `tests/lexer/valid/`: at least 1 case per new token (including the `?` immediately following an
  expression, i.e. `x?`)
- All existing lexer tests pass unchanged (no token encoding changes)

### Acceptance
```
cargo test -p fleen-compiler lexer
cargo fmt -- --check && cargo clippy --all-targets -- -D warnings
```

---

## U02: Parse — New Expressions, Result Patterns, deref Assignment Targets

### Files involved
```
fleen-compiler/src/parser/
├── ast.rs           # new AST nodes
├── expr.rs          # unary prefix + ? suffix + assignment targets
├── stmt.rs          # pattern extension
├── error.rs         # new ParseErrorKind (if needed)
└── tests.rs
```

### New AST nodes
> Implementation note (as-built): follows the existing 0.0.1 AST style — simple nodes use tuple
> variants (`Expr::Move(Box<Expr>, Span)`), structured nodes use named structs (`ExprIf` etc.).

```rust
// Unary prefix (same level as Neg/Not, right-associative)
Expr::Move(Box<Expr>, Span),    // move <unary>, span covers the whole expression
Expr::Clone(Box<Expr>, Span),   // clone <unary>
Expr::Box(Box<Expr>, Span),     // box <expr>
Expr::Deref(Box<Expr>, Span),   // deref <unary> (like - / !, operand at the unary level)

// postfix (same level as Call/Index/Field)
Expr::Question(Box<Expr>, Span), // <expr>?
```

> **The parser only recognizes form; semantics belong to typeck** (SPEC §11: no type checking in
> the parser):
> - The operand of `move` accepts any unary expression; typeck verifies it must be an `Ident` (U05);
> - The operand of `clone` is verified to be an `Ident` or `ExprDeref` (U05);
> - `box` reuses `TokenKind::BoxType`: in type position (`box<T>`) it goes through the existing
>   type parsing; in expression position (`box expr`) it goes through the unary-prefix branch —
>   the two are unambiguous.

### Pattern extension
```rust
pub enum Pattern {
    Literal(Expr),
    Ident(Ident),
    NegLiteral(...),                       // already present since 0.0.1 P5
    ResultCtor { ctor: ResultCtor, binding: Ident, span },  // new in 0.0.2
}

pub enum ResultCtor { Ok, Err }
```
- Syntactic form: `Ok(ident)` / `Err(ident)`; the parser does not attach semantics to `Ok`/`Err`
  (they remain `Ident`s; typeck recognizes the form during pattern checking)

### Assignment target extension
- The legal set of assignment LHS grows from `{ Ident }` to `{ Ident, ExprDeref }`
  (`deref b = expr;` writes the value inside a box); the `InvalidAssignmentTarget` check is
  updated accordingly
- `deref b = expr` cannot carry a type annotation (consistent with §3.4; rejected by
  parser/Resolver)

### Precedence (written into `expr.rs`, aligned with `SYNTAX.ebnf` v0.0.2)
```text
... (all 0.0.1 layers unchanged)
postfix:  primary { call | index | field | "?" }     ← ? is highest (suffix)
unary:    [ box | deref | move | clone | - | ! ] postfix
```
- `clone deref b` = `clone(deref(b))` (prefix right-associativity gives this naturally)
- `x?` is a postfix and binds tighter than unary: `deref b?` = `deref(b?)`

### Tests
- `tests/parser/valid/`: prefix chains (`clone deref b`, `box box 1`), `?` chains
  (`f(x)??` — nested Result), `deref b = v`, `Ok(v)`/`Err(e)` patterns,
  `move`/`clone` as arguments `f(move x)`, unary operand boundaries (`deref -x`, `move move x`)
- `tests/parser/invalid/`: dangling `?`, `move` (no operand), `deref` (no operand),
  `deref b: int = v` (with type annotation), `Ok()`/`Ok(v, w)`/`Ok(v` (malformed
  Result patterns), `box = 1`/`ref = 1`/`move: int = 1` (reserved words as binding names)
- All existing parser tests pass unchanged

### Acceptance
```
cargo test -p fleen-compiler parser
```

---

## U03: Parse — ASI (Optional Semicolons, Prerequisite Pass)

> **The authoritative design is in `docs/0.0.2/ASI.md`**. Architecture: Lex → **ASI pass** → Parse,
> with **newline-sensitive** semantics (statement boundaries are decided at newlines; joining
> statements on one line is illegal).
> The lexer gains a `Newline` token; the pass consumes all Newlines and inserts explicit
> `Semi` at statement boundaries; the output token stream is isomorphic to 0.0.1's, and Parse
> keeps its original logic plus four companion changes.

### Files involved
```
fleen-compiler/src/
├── lexer/
│   ├── token.rs           # new TokenKind::Newline
│   └── lexer_impl.rs      # emit Newline (collapse; none at file start; none inside block comments)
└── parser/
    ├── asi.rs             # new: ASI pass (pure function) + three predicate functions (independently unit-testable)
    ├── mod.rs             # parse() runs the pass; bindings/const hugging } skip the semicolon; statement-entry guard; unified finish
    ├── expr.rs            # postfix suffix extraction; climb after block-like headers; skip inserted semicolon after choose patterns
    ├── error.rs           # new ExpectedSemiOrNewStmt
    └── tests.rs
```

### asi.rs (new file, pure functions)
```rust
/// ASI pass: consumes Newlines and inserts Semi at statement boundaries (see ASI.md §3).
pub(crate) fn insert_semis(tokens: Vec<Token>) -> Result<Vec<Token>, ParseError>;

/// Whether this token can start a new statement (used at block start; includes `(` `-` `!` `{`).
pub(crate) fn can_start_stmt(kind: &TokenKind) -> bool;

/// After a statement completes, whether this token implies an "implicit end" (insert a semicolon).
/// Start set ∖ { LParen, Minus, LBrace } ∪ { RBrace, Eof }.
pub(crate) fn implies_stmt_end(kind: &TokenKind) -> bool;

/// Whether this token continues the current expression (binary operator / `?` / `.` / `[` / `(`).
pub(crate) fn can_continue_expr(kind: &TokenKind) -> bool;
```

### Decision rules (at a newline or EOF; `p` is the previous token, `n` the next)
```text
1. Innermost unclosed bracket is ( or [      → discard the newline (ASI inside { works as usual)
2. n is RBrace                               → discard (never insert before })
3. p ∉ the endable set                       → discard (continuation across lines)
4. implies_stmt_end(n)                       → insert Semi
5. can_continue_expr(n) ∪ {Else, Elif, When, Otherwise, LBrace, Semi} → discard (continuation wins)
6. Otherwise                                 → report ExpectedSemiOrNewStmt { span, found }
```

### Companion changes on the Parse side (ASI.md §4)
- `parse_var_binding` / `parse_const_decl`: skip `;` when next is RBrace
- Tail-expression determination in `parse_block`: Assign-shaped statements (including
  `deref b = v`) are not treated as tail expressions
- `parse_expr_or`: after if/while/choose headers, resume the postfix + binary continuation climb
- Statement-entry guard in `parse_program` / `parse_block`: cannot start a statement →
  `ExpectedSemiOrNewStmt`
- `parse_choose`: skip the semicolon the pass inserted after a pattern (guards may span lines)

### Implementation requirements
- The "no semicolon" rules for `func` declarations, block tail expressions, and
  `when`/`otherwise` are **unchanged**
- `;;` reports `ExpectedSemiOrNewStmt` at the second `;` (0.0.1 also errors; the baseline is unchanged)
- Semicolon presence is invisible after the AST: the `has_semi` field is kept;
  after the pass fills it in, expression statements are always `true` (the only `false`:
  Assign hugging `}`); MIR/bytecode unchanged
- `tokenize()` output gains `Newline` (public API contract change; document it)

### Tests (pitfall cases one by one, see the §5.1 table in `ASI.md`)
- valid: `x = 1` ⏎ `y = 2`; `while { x = x + 1 }` without semicolons; when/without semicolons;
  `{ x = 1 }` binding hugging `}`; `{ 42 }` tail expression; Allman style (`if c` ⏎ `{`);
  `if c { 1 }` ⏎ `else { 2 }` (cross-line if-else is legal); `when Ok(v)` ⏎ guard across lines;
  mixed styles with explicit semicolons; all 0.0.1 valid cases pass unchanged (regression floor)
- invalid: `x = 1` ⏎ `)` (can neither continue nor start) reports `ExpectedSemiOrNewStmt`;
  `x = 1 y = 2` (same-line joining); `if c { 1 };` ⏎ `else { 2 }` (else after a completed if);
  `x = 1` ⏎ `{ print(1) }` (a block expression statement needs a leading semicolon); `;;`
- Dedicated unit tests: exhaustive decision tables for the three predicates over every
  `TokenKind` + pass golden cases
- Migration: the original `missing_semicolon*.fln` cases flip to legal once a semicolon is
  inserted at EOF → rewrite them

### Acceptance
```
cargo test -p fleen-compiler parser
cargo test   # full 0.0.1 regression
```

---

## U04: Typeck — Ok/Err Construction, Result Patterns, `?`, print Fix

> **Depends on U13**: the print fix is finalized as a strict string check (PLAN §6) — a non-string
> argument reports `ArgTypeMismatch` with a hint to use `as string`; the companion cases depend on
> `as` being in place, so U13 comes first.

### Files involved
```
fleen-compiler/src/typeck/
├── infer.rs         # remove the print special case; add construction/`?` checks
├── builtins.rs      # new: builtin signature table
├── typed_hir.rs     # TypedExprHir::Question and other new nodes
├── error.rs         # new TypeckErrorKind entries
└── tests.rs         # restore check_builtin_print_wrong_arg
```

### builtins.rs (new file: builtin signature table, replacing the print special case)
```rust
pub enum BuiltinParam {
    /// A single fixed type sequence (for future builtin extension)
    Fixed(&'static [Type]),
    /// Variable arity; each argument ∈ the set
    Variadic(&'static [Type]),
}

pub struct BuiltinSig {
    pub name: &'static str,
    pub param: BuiltinParam,
    pub ret: Type,
}

pub const BUILTINS: &[BuiltinSig] = &[
    // print: (string...) -> unit; 0.0.2 strict string check (PLAN §6)
    // A non-string argument reports ArgTypeMismatch, help: `x as string` (U13)
    BuiltinSig { name: "print", param: Variadic(&[Type::String]), ret: Type::Unit },
];
```
- `typeck_call` consults this table: a non-string argument (`print(1)` / `print(fib)` /
  `print(box)` / `print(Ok(..))`) → `ArgTypeMismatch`, help suggests explicit `x as string` (U13);
  arity unrestricted (`print()` is legal)
- **Remove the `print` special-case branch and HACK comment from `infer.rs`**
- Split and restore the `check_builtin_print_wrong_arg` test:
  `print("a")` / `print("a", "b")` / `print(42 as string)` → Ok;
  `print(1)` / `print(fib)` (non-string) → `ArgTypeMismatch`

### Ok / Err construction checks (minimal bidirectional checking)
0.0.1's inference is one-directional (infer first, then compare); `Ok(v)`/`Err(e)` need the
contextual type (PLAN D6). This is implemented as **expected-type threading** (checking mode),
through only three paths:

```rust
// typeck_expr gains an expected parameter (None = pure inference)
fn typeck_expr(&mut self, expr: ExprHir, expected: Option<&Type>) -> Result<TypedExprHir, ()>
```
1. **Function bodies** (block tails and single-expression bodies): expected = the declared return
   type (when annotated); expected continues to flow through the tails of `if/elif/else` branches,
   the tails of `choose` arms, and block tails
2. **Annotated binding initializers**: `res: Result<int, string> = Ok(42);`
3. **The operand of `?`**: its own type is already `Result` (no expected needed)

Checking rules for `Ok(arg)` / `Err(arg)`:
- expected = `Result<T, E>` → `Ok` checks `arg: T`, `Err` checks `arg: E`;
- expected = None → `CannotInferResultType { span }`, help suggests adding an annotation
- Arity 0 or ≥2 → `ArityMismatch`
- `Ok`/`Err` shadowed by the user (§4.3 builtin shadowing rules) → treated as a normal function
  call, no error

### Result patterns (choose)
- When the scrutinee type = `Result<T, E>`: `ResultCtor::Ok` binds `v: T`,
  `Err` binds `e: E` (transfer semantics, registered by U05); guards can use the bindings
- **Exhaustiveness**: both `Ok` + `Err` arms present (guard arms don't count) → exhaustive,
  no `otherwise` needed; only one → `ChooseNotExhaustive { missing_patterns: [Ok/Err] }`
- A `Result`-less scrutinee with an `Ok/Err` pattern → `PatternTypeMismatch`

### `?` checks
- Operand type `Result<T, E>`, otherwise `QuestionOnNonResult { found }`
- The enclosing function's return type annotation must be `Result<T2, E>` with `E` **exactly
  equal** (D6):
  - Function without a return annotation / not a Result → `QuestionOutsideResultFn` (help:
    change the function return type or handle it explicitly with `choose`)
  - `E` differs → `QuestionTypeMismatch { expected, found }`
- `?` is legal in `main` returning `Result<int, string>` (entry Err behavior, see U09)

### UnhandledResult
- An expression statement (`Stmt::Expr`) whose value type is `Result<T, E>` → `UnhandledResult`
  (a Result may not be silently discarded); hint: bind it, consume it with `choose`, or add `?` to propagate

### New TypeckErrorKind entries
```rust
QuestionOutsideResultFn,
QuestionOnNonResult { found },
QuestionTypeMismatch { expected, found },
CannotInferResultType { ctor },          // ctor: Ok | Err
UnhandledResult { ty },
PatternTypeMismatch { pattern, found },
ArgTypeMismatch { .. },                  // reuse the existing kind (usable again after the print fix)
```

### Tests
- valid: the `div/ratio` chain (PLAN §3.4 example verbatim), annotated bindings, `main` returning
  Result, `print("x")`, `print(1 as string)`, `print()`, user shadowing `Ok`
- invalid: context-free `Ok(1)`, `?` in an int main, E mismatch, discarding a Result,
  `print(1)` / `print(fib)` (non-string), non-exhaustive Result scrutinee
- **Regression floor**: existing typeck tests like `check_builtin_print` keep their semantics

### Acceptance
```
cargo test -p fleen-compiler typeck
grep -n "print" src/typeck/infer.rs   # only the normal call path remains; no special case/HACK
```

---

## U05: Typeck — Ownership Checker

### Files involved
```
fleen-compiler/src/typeck/
├── ownership.rs     # new: flow-sensitive affinity check (the core of this ticket)
├── typed_hir.rs     # Ident nodes gain an access annotation
├── infer.rs         # is_copy(), type predicates wired in
├── error.rs         # new error family
└── tests.rs
```

### Type predicates (infer.rs)
```rust
impl Type {
    /// Copy types: int/float/bool/unit/function values; Result<T,E> = is_copy(T) && is_copy(E)
    pub fn is_copy(&self) -> bool;
    /// owned: string, box<_>, and Results containing an owned component
    pub fn is_owned(&self) -> bool;
}
```
- `Type::Box` / `Type::Ref` graduate from "parsed but Unsupported": remove the
  `UnsupportedType` special cases (the 0.0.1 paths where `codegen`/`lower` errored on box/ref
  are replaced by U06's real lowering)

### Access annotations (typed_hir.rs)
```rust
/// New field on Ident nodes: the ownership access form of this use
pub enum Access {
    Copy,    // read of a Copy type (default)
    Move,    // move x (consuming, slot emptied)
    Clone,   // clone x / read-only use of an owned value (deep copy, slot untouched)
}
// TypedExprHir::Ident { name, binding_id, hir_id, span, access }
```
- Back-filled by the ownership checker; lower (U06) uses it to choose `MoveLocal`/`CloneLocal`/`LoadLocal`
- ref arguments need no Access: lower decides to emit `MakeRefLocal` at `Call` based on the
  callee's parameter type

### Ownership state machine (ownership.rs)
```rust
enum MoveState {
    Alive,
    Moved { span: Span },          // definitely moved
    MaybeMoved { spans: Vec<Span> }, // moved on some branches
}

struct OwnershipChecker {
    state: HashMap<BindingId, MoveState>,
    errors: &mut Vec<TypeckError>,
}
```

**Traversal rules (one-to-one with the six rules in `DESIGN.md` §10.2):**

| Construct | Handling |
|------|------|
| Binding/assignment RHS top level is `move x` | Register `x → Moved`; subsequent read/write/move of `x` → `UseAfterMove`; reassigning `x` → `AssignToMoved` (error message distinguished from unbound) |
| RHS top level is `clone x` / `clone deref b` | Deep-copy semantics, state unchanged |
| RHS top level is a bare `x` (owned) | `OwnedArgRequiresMove` (help: `move` or `clone`) |
| Argument is a bare owned `x` | Same as above (checked at `Call` parameters) |
| `choose` scrutinee is a bare owned `x` | Same as above (D7); `move x`/`clone x` is legal |
| Bare owned `x` in a function tail/block tail/branch tail | Production position: implicit transfer, register Moved/conditional transfer |
| `if/elif/else` | Each branch infers independently from a snapshot; at the join: all Moved → Moved, some → MaybeMoved, otherwise Alive |
| `while` | **Conservative rule**: `move x` in the body ⇒ `x` is MaybeMoved at the exit; using `x` after moving it in the body (including at the back edge) → `UseAfterMove`; using a variable in the guard that the body may move → `MaybeMovedAfterBranch` |
| `choose` | Same as if (arms joined in order); Result pattern bindings registered as transfers |
| Function boundaries | State is cleared, does not cross |
| Comparison operators / `print` arguments | Read-only use (D3): owned operands are automatically annotated `Access::Clone`, state unchanged |
| `deref b` (read) | Copy semantics, state of `b` unchanged |

**Place validity:**

| Expression | Verdict |
|--------|------|
| `move x` (x is Copy) | `MoveOfCopyType` (help: drop `move`) |
| `move deref b` | `MoveOutOfBox` (help: `clone deref b`) |
| `move g` (owned global) | `MoveOutOfGlobal` (help: `clone g`) |
| `move s` (s is a ref parameter) | `MoveOfBorrowed` |
| `clone` operand not an Ident/ExprDeref | `InvalidClonePlace` |
| Argument `move x` for a ref parameter | `BorrowArgWithMove` (a borrow does not accept a transfer) |
| Assigning to a ref parameter | `AssignToRefParam` |
| `ref T` in a non-parameter type position | `RefNotAllowedHere` (local/global/return/nested `ref ref T`; **this ticket checks it at the Resolver stage**, see below) |

**Resolver division of labor**: `RefNotAllowedHere` depends on declaration form
(binding/const/return type position), checked by `resolver/mod.rs` while traversing type
annotations, with the error folded into `ResolveErrorKind`;
all other ownership errors live in typeck.

**Implementation limitation discovered during design review (added by this ticket)**: the
assignment target of `deref b = v` must be a **local** box — if `b` is an owned global, reading
it is `CloneGlobal` (copying the whole box), and a write through it would only change the copy.
Typeck reports `DerefAssignOfGlobalBox` for deref assignment to a global box
(a 0.0.2 limitation; lifting it would require a new `StoreGlobalDerefBox` instruction, left for a
later version).
U12 records this limitation in `DESIGN.md` §10.3.

### New TypeckErrorKind / ResolveErrorKind entries
```rust
// typeck
UseAfterMove { name, use_span, moved_span },
MaybeMovedAfterBranch { name, use_span, moved_spans },
AssignToMoved { name, span },
MoveOfCopyType { name, ty },
MoveOutOfBox,
MoveOutOfGlobal { name },
MoveOfBorrowed { name },
InvalidClonePlace,
OwnedArgRequiresMove { name, ty },   // help: `move x` / `clone x`
BorrowArgWithMove { name },
DerefAssignOfGlobalBox,
// resolver
RefNotAllowedHere { span, context },
```

### Tests
- valid: all PLAN §3.1 examples, no use after a conditional transfer, no use within the loop
  after a move inside the loop, reads of owned globals, `clone deref b`
- invalid: at least 1 case for each of the six core rules (corresponding to every error kind in
  the tables above), use after the while back edge, `x = move x`, global box deref assignment
- Exhaustive `is_copy`: assert over every `Type` variant

### Acceptance
```
cargo test -p fleen-compiler typeck resolver
```

---

## U06: Lower — New MIR Instructions + Span Sinking

### Files involved
```
fleen-compiler/src/lower/
├── mir.rs           # MirInstr wrapping refactor + new instructions
├── mod.rs           # lowering rules
├── slots.rs         # (if needed) slot comments for owned bindings
└── tests.rs
```

### MirInstr refactor (Span sinking, PLAN §4.5)
```rust
// Changed from a bare enum to a struct wrapper:
pub struct MirInstr {
    pub kind: MirInstrKind,
    pub span: Span,          // 0.0.1's MIR had no span; this ticket carries it per-instruction from TypedHir
}

pub enum MirInstrKind {
    // …… all 0.0.1 variants migrated as-is
    // new in 0.0.2:
    AllocBox,
    DerefBox,
    StoreDerefBox,
    MakeRefLocal(u16),
    DupDeep,
    MoveLocal(u16),
    CloneLocal(u16),
    CloneGlobal(u16),
    PackOk,
    PackErr,
    IsErr,
    UnwrapOk,
    UnwrapErr,
}
```
- Mechanical refactor blast radius: `lower/mod.rs`, `cfg.rs`, `codegen/encoder.rs`, all lower
  tests — done in a single PR, **changing no assertion semantics** (0.0.1 regression floor)

### Lowering rules (TypedHir → MIR)

| Source construct | MIR sequence |
|--------|----------|
| `box e` | `<e>`; `AllocBox` |
| `deref b` (read) | `<b>`; `DerefBox` (the box stays on the stack, a copy of the inner value is pushed) |
| `deref b = v` | `<b>`; `<v>`; `StoreDerefBox` |
| `move x` (Access::Move) | `MoveLocal slot` |
| `clone x` (Access::Clone) | `CloneLocal slot` |
| owned global read (read-only/clone) | `CloneGlobal gid` |
| owned argument (Access::Move/Clone) | `MoveLocal`/`CloneLocal` + `Call`/`CallValue` |
| ref argument (place = local/global/ref parameter) | `MakeRefLocal slot` + `Call` (for globals, first… see note below) |
| `x?` | `Dup`; `IsErr`; `JumpIfTrue L_err`; `UnwrapOk`; `Jump L_cont`; `L_err: UnwrapErr`; `Return`; `L_cont:` (`BYTECODE.md` §6.7) |
| owned operands of comparisons / print | `CloneLocal`/`CloneGlobal` first (D3 read-only use) |
| choose (Result scrutinee) | `Dup`; `IsErr`; `JumpIfTrue L_err`; `UnwrapOk`; `StoreLocal v`; arm body; … (`BYTECODE.md` §6.8) |

> **A ref argument pointing at a global**: the `MakeRefLocal` handle binds to **this frame's**
> `stack_base + slot` and cannot point at the global table. Implementation: for a global ref
> argument, first **copy** the value into a temporary local slot (`CloneGlobal` + `StoreLocal`),
> then `MakeRefLocal` — semantically equivalent (the borrow is read-only and the caller will not
> write that global in the meantime; the copy cost is acceptable). This trade-off is recorded in
> the U12 doc checklist.

### Tests
- `tests/parser/`… not involved; `lower/tests.rs`: at least 1 case per lowering rule,
  compared instruction-by-instruction against the shapes in `BYTECODE.md` §5.9–5.11 / §6.7–6.8
- Span assertions: each MIR instruction's span covers the corresponding source range

### Acceptance
```
cargo test -p fleen-compiler lower
```

---

## U07: Codegen / Verify — Bytecode v2

> **Status**: complete (2026-10-10). Spec corrections and implementation notes are at the end of this section.

### Files involved
```
fleen-compiler/src/codegen/
├── bytecode.rs      # Opcode additions + Module.version + V2_OPCODE_BASE
├── encoder.rs       # encoding + span_map generation (EncodedFunc)
├── flnc.rs          # version accepts {1, 2}
└── tests.rs

fleen-verify/src/
├── verify.rs        # new instruction validation rules + version gate + slot/gid validation
├── stack_analysis.rs # Δdepth table (provided via stack_effect)
└── tests.rs

# Changes outside the ticket scope but required in lockstep (otherwise it won't compile / all e2e go red):
fleen-vm/src/vm.rs          # add a wildcard arm to the `Opcode` match; widen the version gate to {1, 2}
fleen-compiler/src/lexer/token.rs  # Span::SYNTHETIC (span_map contract)
```

### Opcode (bytecode.rs, byte-for-byte identical to `BYTECODE.md` §5.9–5.11)
```rust
// 0x80–0x83 box / ref
AllocBox      = 0x80,
DerefBox      = 0x81,
StoreDerefBox = 0x82,
MakeRefLocal  = 0x83,
// 0x90–0x93 move / clone
DupDeep       = 0x90,
MoveLocal     = 0x91,
CloneLocal    = 0x92,
CloneGlobal   = 0x93,
// 0xA0–0xA4 Result
PackOk        = 0xA0,
PackErr       = 0xA1,
IsErr         = 0xA2,
UnwrapOk      = 0xA3,
UnwrapErr     = 0xA4,
```
- Operand widths: `MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal` =
  u16 (3-byte instruction); the rest are 1 byte
- `Module.version` written as `2` (`codegen::MODULE_VERSION`); `flnc::from_bytes` accepts
  `{1, 2}` (old v1 files remain readable), `to_bytes` writes `m.version` as-is
- Export `V2_OPCODE_BASE = 0x80`: all v1 numbers are below it and all v2 numbers at or above it;
  verify uses it as the version gate

### span_map generation (encoder.rs)
- `encode_func` returns `EncodedFunc { code, span_map }` (no longer just bytes);
  during the second emission pass it records `(code.len(), span)` in order, so offsets are
  naturally exact
- **Contract** (written into BYTECODE.md §2): covers only MIR instructions; a terminator without
  a span produces no entry; synthetic glue instructions use `Span::SYNTHETIC` and produce no
  entry; builtin placeholder bodies always have an empty span_map;
  `offset == frame.ip at the moment of the VM error`; a miss means no source position
  (consumers degrade gracefully)
- `emit_instr` returns the number of bytes actually written and `debug_assert_eq!`s it against
  `Opcode::instr_len()` — preventing drift between the operand table and the emitted code
  (drift would silently misalign jump offsets and span_map offsets)
- For 0.0.1 artifacts the `span_map` is always empty — the serialization format is unchanged;
  it just goes from "left empty" to "filled in"

### verify.rs / stack_analysis.rs
- `stack_effect()` additions (the `BYTECODE.md` §8 table, `AllocBox` already corrected):
  - `AllocBox`: **Δ0** (`v → b`), min 1
  - `MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal`: Δ+1, min 0
  - `DerefBox` / `DupDeep`: Δ+1, min 1
  - `StoreDerefBox`: Δ−2, min 2
  - `PackOk` / `PackErr` / `IsErr` / `UnwrapOk` / `UnwrapErr` / `ToStr`: Δ0, min 1
- Slot validation: slots of `MakeRefLocal` / `MoveLocal` / `CloneLocal` must be < `locals`
  (same rule as `LoadLocal`); `CloneGlobal`'s gid must be < the globals table length
- Version gate: a v2 instruction in a version=1 module → **explicit** `V2OpcodeInV1Module`
  (one comparison); version=2 modules go through the same full set of checks

### Tests
- codegen: assert the numbering/width of all 13 v2 instructions one by one (including `from_byte`
  round-trips, unknown reserved slots), u16 operand little-endian round-trips, jump offsets
  advancing by the 3-byte instruction width, the five span_map cases
  (real span recorded / synthetic span skipped / no entry for terminators / empty table for
  builtins / `__init__` keeps only the initializer span), flnc `{1,2}` round-trip
- verify: valid/malformed modules for each new instruction (stack mismatch, slot out of bounds,
  gid out of bounds), v1 module regression, v2-in-v1 rejection, and a **full-chain smoke requiring
  compiler output to pass verify**
  (covering all lowering paths: move/clone/box/deref/ref arguments/`?`/Result-choose/`as`)
- Compatibility: a v1 module (in-memory form) verify PASSes; binary fixtures belong to U11

### Implementation notes (discovered while landing U07; all are spec corrections or seam fixes)
1. **`AllocBox` stack-effect correction**: the original table in PLAN §5.2 / BYTECODE.md §8 put
   `AllocBox` on the same row as `DupDeep` with `+1`, but the lowering `<e>; AllocBox` requires it
   to **replace** the top stack value (net 0).
   Lower's depth table was already 0; verify applying `+1` would over-count every depth after
   `box e` by 1.
   Found by the "compiler output must pass verify" full-chain test; fixed to `v → b` and the docs
   corrected in both places.
2. **The version gate is not "naturally fine"**: after `from_byte` registers 0x80–0xA4, those
   bytes in a v1 module also decode and `BadOpcode` never triggers.
   Making the rule hold requires an explicit comparison; implemented as `V2OpcodeInV1Module`.
3. **Out-of-scope VM changes are unavoidable**: after `Opcode` gains 13 variants, the
   `match opcode` in `fleen-vm/src/vm.rs` **fails to compile** without a wildcard arm; and the
   `version != 1` gate in `run()` would refuse to execute every newly compiled v2 module (all e2e
   red). Minimal fix: add the `_ => Err(RuntimeError::InvalidOpcode(op))` wildcard arm + widen
   the version gate to `{1,2}`; **executing** v2 instructions is left to U09 (until then they
   surface as hard errors like `invalid opcode 0x91`, not silent no-ops).
4. **span_map offset contract**: the VM's `step()` only writes back `frame.ip` at the end of the
   success path; on error, `ip` still points at the first byte of the failing instruction. So the
   contract is "`offset == `frame.ip` at the time of the error",
   and U10 looks up the table accordingly (the draft's `frame.ip - 1` was corrected; this item is
   consistent with U10's design).
 5. **Review finding: verify panics on an empty function body (pre-existing in 0.0.1)**:
    `walk_function` lets a zero-length `code` through directly (no instructions, so there is no
    "fell off the bottom of the function" instruction to report), then `stack_analysis` starts at
    pc 0 with an empty instruction table and `by_pc[0]` panics out of bounds.
    Both the `fleen-verify` CLI and `fleen-vm`'s `.flnc` path crash (exit 101).
    Fixed: `walk_function` adds an `EmptyFunctionBody` check (an empty body means no `Return`,
    exactly the §8 "every path ends with Return" violation), and `analyze`'s indexing falls back
    to `.get()`. Regression cases: `empty_function_body_is_rejected` (in-memory construction) and
    `empty_function_body_from_bytes_is_rejected` (full `.flnc` decode chain).
 6. **Review finding: `flnc::from_bytes` preallocates on untrusted lengths → OOM abort (pre-existing in 0.0.1)**:
    the four table length fields (constants/functions/span_map/globals) come straight from the
    file, and `Vec::with_capacity(n)` ran before the "is there enough data" check;
    with `n = 0xFFFFFFFF` the allocation fails and aborts outright (≈51–275 GB), violating
    "malformed input never panics" (SPEC §4/§11; e2e already has a corrupt-bytecode contract).
    Fixed: `R::capacity_hint(count, min_elem)` clamps capacity to the remaining byte count
    (valid files keep the same preallocation granularity; oversized lengths degrade to a
    truncation error).
    Regression case: `flnc_rejects_oversized_table_lengths`.

### Scope decision: Result-choose guard / `otherwise` deferred further
- Current state (U06 as-built + working-tree changes): `otherwise`, literal arms, and guarded
  arms report `UnsupportedFeature` at the `lower` level; the exhaustiveness rule is "guard arms
  don't count".
- This ticket **does not implement**: refactoring Result-choose from "Ok/Err two arms, no
  fallback block" into a **general arm chain + no-match fallback** (a failed guard must be able
  to fall through to later arms), plus handling payload retention before the guard (`DupDeep`).
  The workload approaches half a ticket and is a change to the lower shape, outside this
  ticket's codegen/verify scope.
- Decision: **defer to U09 or a separate ticket**; until then lower keeps rejecting, and U11 must
  add one invalid case each for "guard / otherwise" to lock the behavior in.

### Acceptance
```
cargo test -p fleen-compiler codegen
cargo test -p fleen-verify
cargo test   # full 0.0.1 regression (fib.fln / cast.fln byte-for-byte unchanged)
```

---

## U08: VM — Value Ownership Representation Refactor (can run in parallel with the frontend)

### Files involved
```
fleen-vm/src/
├── value.rs         # Value enum refactor
├── vm.rs            # paths affected by Rc<str> → Box<str>
├── error.rs         # reserve two new variants (used by U09)
└── tests.rs
```

### Value refactor (`BYTECODE.md` §3.2 v0.0.2)
```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),                 // ← changed from Rc<str> to exclusive ownership
    Boxed(Box<Value>),             // box<T>
    Ref { base: u32, slot: u16 },  // borrow handle (used by U09)
    Ok(Box<Value>),                // Result
    Err(Box<Value>),
    Unit,
    Func(FuncId),
}
```

### Implementation requirements
- Remove `use std::rc::Rc`; complete `Display`: `Boxed` prints the inner value, `Ref` prints
  `<ref>`, `Ok/Err` print `Ok(v)` / `Err(e)`
- Keep the `Clone` / `PartialEq` derives: `Box<Value>` deep-copies/compares structurally
  automatically (`Eq` instruction semantics: `Str` by content, `Boxed` by inner value, `Ref` by
  dereference — the dereference logic happens in U09's instruction dispatch; this ticket only
  guarantees the derive behavior doesn't break v1 instructions)
- **Zero v1 semantic change**: all 36 of 0.0.1's opcodes behave observably identically under
  `Box<str>` as under `Rc<str>` (strings are immutable and no aliasing escape path exists); all
  0.0.1 VM tests pass unchanged
- Reserve in `error.rs`:
  ```rust
  ResultMismatch,      // UnwrapOk hit an Err (defensive)
  BorrowOutOfRange,    // handle base+slot out of bounds (defensive)
  ```

### Acceptance
```
cargo test -p fleen-vm
grep -rn "Rc<" src/ | wc -l   # = 0
```

---

## U09: VM — New Instruction Execution and Entry Err

> **Left over from U07 (this ticket must handle it)**: `Opcode` currently has 13 v2 variants that
> are only encoded, not yet executed — `vm.rs` returns `RuntimeError::InvalidOpcode` for them.
> This ticket replaces that wildcard arm with real implementations. Known affected paths:
> **owned local/global variables as `print` arguments** (D3 read-only use →
> `CloneLocal` / `CloneGlobal`), for example `print(u)` in the verify full-chain smoke source
> (an owned local after `u = clone t`);
> after U07 such programs reporting `invalid opcode 0x92` is expected, not an U07 regression.
> Also pending implementation: `move` tail expressions (`MoveLocal` 0x91), box/deref, `?`, etc.

### Files involved
```
fleen-vm/src/
├── vm.rs            # step() dispatch extension + handle dereference helper
├── error.rs         # ResultMismatch / BorrowOutOfRange go live
├── main.rs          # entry Err → stderr + exit code 1
└── tests.rs
```

> **Implementation note (U09's actual scope exceeded the table above)**: during integration two
> lower defects in U06/U07 were found; without fixing them this ticket's acceptance (multi-arg
> `print`, borrow/owned arguments, `deref` writes) could not run, so they were fixed with this
> ticket and recorded in BYTECODE.md §5.7/§5.9 "U09 corrections":
> (1) variadic builtins are instantiated as one `MirFunc` per observed argument count (`Call`
> pops exactly `callee.params` operands; sharing a `params == 1` instance would leave surplus
> arguments on the stack);
> (2) `deref b = v` on a local box now emits `v; AllocBox; StoreLocal b` (`LoadLocal` deep-copies,
> so `StoreDerefBox`'s write never landed back into the box in the slot).

### Instruction dispatch (vm.rs `step()`)

| Instruction | Implementation |
|------|------|
| `AllocBox` | `let v = pop(); push(Value::Boxed(Box::new(v)))` |
| `DerefBox` | `let b = peek()`; `push(inner.clone())` — when `b` is a `Ref`, resolve the handle first (see below) |
| `StoreDerefBox` | `let v = pop(); let b = pop()`; `*inner = v` (the old value is released by the assignment's Drop) |
| `MakeRefLocal` | If the target slot already holds a `Ref` → push another copy as-is (borrow forwarding; implementation fix: check the slot's contents, not the stack top — with multiple `ref` arguments the stack top holds an earlier prepared argument); otherwise push `Value::Ref { base: frame.stack_base, slot }` |
| `DupDeep` | `let v = peek().clone(); push(v)` (`Box<Value>` deep-copies automatically) |
| `MoveLocal` | `let v = std::mem::replace(&mut stack[base+slot], Value::Unit); push(v)` |
| `CloneLocal` | `push(stack[base+slot].clone())`; when the value is a `Ref`, deep-copy the **handle target** |
| `CloneGlobal` | `push(globals[id].clone())` — **does not resolve handles** (implementation fix: typeck forbids `Ref` entering globals; the fallback is at the consumption point, see BYTECODE.md §5.10) |
| `PackOk` / `PackErr` | `pop`, wrap in `Ok/Err`, push back |
| `IsErr` | `pop r; push(Bool(matches!(r, Err(_))))` |
| `UnwrapOk` | `pop r`; `Ok(v) → push(v)`; `Err → RuntimeError::ResultMismatch` (defensive) |
| `UnwrapErr` | Symmetric |

### Handle dereference (unified helper)
```rust
/// Read-only dereference: Ref → the slot value it points to; Boxed → itself; anything else → itself.
/// A handle may only point at a slot in a frame "further down the current call chain" (smaller base).
fn read_ref(&self, v: &Value) -> Result<&Value, RuntimeError> {
    match v {
        Value::Ref { base, slot } => self.stack.get(*base + *slot as usize)
            .ok_or(RuntimeError::BorrowOutOfRange),
        other => Ok(other),
    }
}
```
- Use sites: `Eq`/`Ne` (both sides), ordering comparisons (both sides), `exec_builtin` (print
  arguments), and the source values of `CloneLocal`/`DupDeep`/`DerefBox` — instructions consuming
  a `Ref` must dereference first (`CloneGlobal` is the exception, see the table above)
- **Discipline**: a `Ref` may not appear inside `Boxed` / `Ok` / `Err` (guaranteed by typeck);
  on violation, defensively return `BorrowOutOfRange` at runtime (during U09 implementation this
  was upgraded from `debug_assert!` to an unconditional check: `fleen-verify` has no type view of
  stack values, so release builds need the fallback too)

### Drop semantics verification (no new code, must be verified)
- Frame destruction: after `Return`, `stack.truncate(stack_base)` — Rust `Drop`s the truncated
  elements one by one, so owned values (`Str`/`Boxed`/`Ok`/`Err`) are released deterministically ✓
- Slot overwrite: indexed assignment via `StoreLocal`/`StoreGlobal` releases the old value ✓
- `Pop` discards a statement value: `Vec::pop` Drops ✓
- Unit tests: after cloning, rewriting the inner value does not affect the other; after a move
  the original slot is `Unit`; deep release of nested boxes
  (implementation note: the VM has no allocation-counting mechanism, so "deep release" is
  evidenced by a nested deep-copy case plus the per-element `Drop` semantics of `truncate` after
  `Return`; no direct observation was made)

### Entry Err (main.rs, D8)
```rust
match vm.run() {
    Ok(Value::Err(e)) => { eprintln!("error: {e}"); process::exit(1); }
    Ok(v) => { /* current behavior unchanged */ }
    Err(e) => { /* current behavior unchanged (from U10 on, e carries a span) */ }
}
```

### Acceptance
```
cargo test -p fleen-vm
```

---

## U10: Diagnostics — span_map Consumption and Runtime Line/Column

### Files involved
```
fleen-vm/src/
├── diag.rs          # new: byte offset → (line, col)
├── error.rs         # RuntimeError carries an optional span
├── vm.rs            # on error, look up the span_map
└── main.rs          # .fln mode prints the source position; .flnc mode degrades to function name + offset
```

### Design
```rust
// diag.rs
/// Source byte offset → (1-based line, 1-based col); O(n) single-pass scan, error-path only.
pub fn line_col(source: &str, offset: u32) -> (u32, u32);
```
- `RuntimeError` gains `span: Option<(u32, u32)>`: on error, look up the current function's
  `span_map` by `frame.ip` (`offset` is monotonic; binary search or linear), constructing
  `(start, end)`
- CLI output (the `.fln` one-stop mode holds the source):
  ```console
  runtime error: division by zero
    --> crash.fln:5:9
     |
   5 |     q = a / b;
     |         ^^^^^
  ```
- Running `.flnc` directly: no source, so degrade to `runtime error: … (in `div`, pc 0x12)`
- **Exit-code contract unchanged** (0 success / 1 error / 2 usage·IO)

### Tests
- Span correctness for divide-by-zero / overflow / deep-recursion errors (at least 1 case each
  for line and column)
- With an empty span_map (v1 module): no position printed, no panic
- stderr snapshots in e2e invalid cases (`.stderr` keywords)

### Implementation notes (discovered while landing U10)
1. **`RuntimeError` changed from enum to struct**: the ticket's "`RuntimeError` gains
   `span: Option<(u32, u32)>`" landed as `RuntimeError { kind, span, func, pc }`:
   the original enum was renamed `RuntimeErrorKind` (all variants and messages unchanged), and
   `func`/`pc` are required for the `.flnc` degraded output `(in `func`, pc 0x..)` (after
   `Vm::new(module)` the CLI no longer holds the module, so it cannot look up the name/pc itself).
   `Display` only outputs the message; position rendering belongs to the CLI;
   `From<RuntimeErrorKind>` covers bare construction.
2. **The span is attached at a single exit in `run()`**: the VM's internal functions uniformly
   return `RuntimeErrorKind`, and errors from `run_inner` get their position attached once via
   `locate_error`. Per the U07 contract (`step` only writes back `frame.ip` on the success path):
   the top frame is the failing frame; errors on the `call` path (depth exceeded, builtin) happen
   before the new frame is pushed, so the position naturally lands on the call site. A test locks
   this in: a deep recursion's span = the deepest recursive call site.
3. **Known limitation: a binary expression's span covers only the left operand**: the AST binary
   variants (`Add`/`Div`/… 13 of them) carry no span of their own and `Expr::span()` delegates to
   the lhs (`parser/ast.rs`), so the divide-by-zero caret for `a / b` points at `a` rather than
   the whole `a / b` (the ticket's example drew the full expression). Fixing it requires adding a
   span field to the binary variants and threading it through parser/typeck/lower — an
   independent compiler-side change, not folded into U10; existing tests lock in the current
   behavior, and widening it later is an intentional change.
4. **CRLF column counts**: the lexer treats `\r` as whitespace (`\r\n` collapses into one newline,
   ASI.md §2), so `line_col`/caret rendering doesn't count `\r` as a column, consistent with the
   byte offsets of token spans.

### Acceptance
```
cargo test -p fleen-vm
cargo fln tests/e2e/invalid/e2e_div_zero.fln   # manually verify the output format
```

---

## U11: Testing — Matrix, E2E, and v1 Compatibility Fixtures

### File-driven tests (valid/invalid per phase, per PLAN §8.1)
```
tests/lexer/{valid,invalid}/        # U01 new tokens
tests/parser/{valid,invalid}/       # U02 new syntax + U03 ASI pitfall table
tests/resolver/{valid,invalid}/     # U05 RefNotAllowedHere
tests/typeck/{valid,invalid}/       # all error kinds from U04 + U05
tests/e2e/{valid,invalid}/          # see below
```

### New E2E cases
```
tests/e2e/valid/
├── fib.fln              # 0.0.1 regression: passes as-is (explicit-semicolon style)
├── fib_asi.fln          # same program, no semicolons: output byte-for-byte identical to fib.expected
├── ownership.fln        # move/clone/conditional transfer
├── box_demo.fln         # allocation/read-write/clone independence/nested boxes
├── ref_demo.fln         # pass-by-borrow; the original variable remains usable
├── cast.fln             # as casts + print(x as string) (U13/F6)
└── question.fln         # Ok/Err/?/choose-Result full chain

tests/e2e/invalid/
├── use_after_move.fln       # .exit=1 + .stderr keyword (compile error)
├── owned_assign_without_move.fln
├── unhandled_result.fln
├── question_in_int_main.fln
├── print_func_value.fln
└── ref_out_of_param.fln     # ref local variable
```

**question.fln** (the end-to-end benchmark program, doubling as a doc example):
```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func safe_half(a: int): Result<int, string> {
    q = div(a, 2)?;
    Ok(q)
}

func main(): int {
    res = choose safe_half(10) {
        when Ok(v) { v }
        when Err(e) {
            print("error: ", e);
            0 - 1
        }
    };
    print(res as string);
    0
}
```
`question.expected`: `5`

### v1 compatibility fixture
- `tests/fixtures/v1/fib.flnc`: compile `fib.fln` with the **0.0.1 toolchain** (before the
  current code changes) and commit the result as a binary fixture
- Test: `fleen-verify` PASSes + `fleen-vm` execution output == `fib.expected`
  — locking in the promise that "v2 does not break v1" (`BYTECODE.md` §10)
- Home: `fleen-vm/tests/v1_compat.rs` (new integration test file)

### Implementation notes (discovered while landing U11)
1. **The v1 fixture must be compiled from the v0.0.1 version of `fib.fln`**: the current
   `fib.fln` uses `as string` (U13 syntax), which the 0.0.1 toolchain cannot compile. The fixture
   source is taken from `git show v0.0.1:tests/e2e/valid/fib.fln` (the `print(fib(x))` form; at
   the time print accepted any scalar); its output is byte-for-byte identical to the current
   `fib.expected` (runtime scalars are rendered via the `fmt_scalar` fallback). Method: check out
   the `v0.0.1` tag with `git worktree`, build a release compiler, then remove the worktree
   immediately after compiling.
2. **Binary fixture protection**: added a root `.gitattributes`
   (`tests/fixtures/v1/*.flnc binary`) to prevent autocrlf and other text-normalization from
   rewriting the bytecode.
3. **`box_demo`'s nested reads are place-limited**: `deref`'s operand must be a local variable of
   box type (consistent with `ownership_deref_invalid_operand`), so `deref deref nested` is
   rejected; nested boxes are observed via `clone deref outer` (a deep copy of the inner box).
   The fixture is designed this way; not a defect.
4. **`question_in_int_main` produces two errors**: `?` outside a Result function +
   `q` undefined (after `?` is rejected the binding is never established); the `.stderr` needle
   only anchors on the first one.

### Regression floor
- `cargo test` all green, with 0.0.1's 321 tests **zero semantic change** (only mechanical
  adjustments allowed, e.g. construction sites of the MirInstr wrapper)

### Acceptance
```
cargo test
cargo fln tests/e2e/valid/question.fln
```

---

## U12: Wrap-up — Doc Verification, Version, and Release

### Doc verification (synced during planning; verify the diffs after implementation)
- [x] `DESIGN.md` §10.3 records the "local box only" limitation of `deref` assignment targets
  (found by U05)
- [x] `DESIGN.md` §10.4 records the trade-off of "global ref arguments copied via a temporary
  slot" (found by U06;
  verified against `lower/mod.rs`'s global ref argument `CloneGlobal; StoreLocal tmp; MakeRefLocal tmp`)
- [x] `DESIGN.md` §18 known-hacks section **emptied** (print is fixed; done early during the U07 review)
- [x] `DESIGN.md` adds the "type cast `as` (0.0.2 temporary, revisit after generics)" subsection
  and the note that print is string-only as a temporary measure (U13/F6; §9 already in place)
- [x] `docs/0.0.2/ASI.md` §4 continuation set gains `As` (U13; already in place)
- [x] `BYTECODE.md` proofread so the v2 instruction table matches the final opcodes/encoding
  (three U12 verification fixes: §5.9's `deref b = v` sequence gains a trailing `Unit`;
  §6.7/§6.8's owned Result examples `Dup` → `DupDeep`)
- [x] `SPEC.md` §14 quick reference gains the `?` example (if the implemented form differs;
  U12 added Result/`?`/`as` examples and the `as` row of the precedence table)
- [x] New error message samples compared against actual output (`UseAfterMove` etc.)

### Version and release
- [ ] Bump the three crates' `Cargo.toml` versions → `0.0.2`
- [ ] `CHANGELOG.md`: change `0.0.2 — Unreleased (planned)` into a proper entry
  (date, drop the "planned" marker, add behavioral details discovered during implementation)
- [ ] `README.md` version line update (`0.0.2`, consider a codename — 0.0.1 was Swift Fox 🦊)
- [ ] `cargo doc --no-deps` with no warnings; complete docs for public APIs (`fleen_vm::Value` etc.)
- [ ] After the four CI checks are all green, tag and produce release archives (follow the 0.0.1 process)

### Acceptance
```
cargo fmt -- --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test && cargo doc --no-deps
```

---

## U13: Full Chain — `as` Type Cast (0.0.2 Temporary Feature, F8)

> **Implementation order**: inserted before U04 — the print fix (strict string check, PLAN §6)
> depends on this ticket; the non-string argument error help and companion cases both need
> `as string` (PLAN §3.6 / §6).
> **Temporariness**: 0.0.2's whitelist is only "scalar → string", the minimal companion to the
> print fix; the full conversion matrix and a From-like mechanism will be revisited with
> generics (0.1.0), and this positioning is written into DESIGN.md.

### Files involved
```
fleen-compiler/src/
├── lexer/token.rs        # TokenKind::As ("as" becomes a reserved word, Breaking)
├── lexer/lexer_impl.rs   # "as" => As + is_keyword/keyword_str/Display
├── parser/ast.rs         # Expr::Cast(Box<Expr>, Type, Span)
├── parser/expr.rs        # cast layer (between unary and binary)
├── parser/asi.rs         # can_continue_expr gains As (cross-line continuation)
├── parser/tests.rs
├── typeck/infer.rs       # whitelist check + UnsupportedCast
├── lower/                # MirInstrKind::ToStr
└── codegen/              # Opcode 0xA5 ToStr (1 byte); span_map carried as usual

fleen-verify/src/stack_analysis.rs   # ToStr: Δ0, min 1
fleen-vm/src/
├── fmt.rs               # new: scalar formatting helper (shared by print and ToStr)
└── vm.rs                # ToStr execution
```

### Syntax and precedence (aligned with `SYNTAX.ebnf` v0.0.2)
```text
cast:  unary { "as" type }        ← new layer; unary binds tighter than as, as tighter than binary (like Rust)
```
- `deref b as string` = `(deref b) as string`; `1 + 2 as string` = `1 + (2 as string)`;
  `box 1 as string` = `box (1 as string)` (the prefix keyword binds tighter)
- ASI: `As` joins `can_continue_expr` — `x = 42` ⏎ `as string` continues across lines,
  same rule as binary operators; sync the §4 continuation-set line in `ASI.md`
- `as` must be followed by a type, parsed with the existing type parser; `42 as` (missing type)
  is a parse error

### typeck whitelist (only these three; everything else rejected)
| Expression | Result |
|--------|------|
| `int as string` | Decimal (including the minus sign) |
| `float as string` | Identical to print's float output |
| `bool as string` | `"true"` / `"false"` |

- Everything else (`string as string`, `int as float`, `box<T> as …`, `ref T` operands,
  identity casts) → `UnsupportedCast { from, to }`,
  help: 0.0.2 only supports scalar-to-string casts; the full set arrives with the generics release
- The result is a **fresh owned string**: consumption sites need no `move` / `clone` (consistent
  with "the RHS is a new value")
- The formatting helper is extracted into `fleen-vm/src/fmt.rs`, shared by print and `ToStr`, to
  avoid drift between the two

### Lowering / Bytecode / VM
| Construct | MIR / bytecode |
|------|--------------|
| `e as string` | `<e>`; `ToStr` (0xA5, 1 byte, `v → s`, Δ0 min 1) |

- VM execution: pop the scalar → format via `fmt` → push `Value::Str` (a fresh owned value)
- `as` introduces no implicit numeric conversions (0.0.2 has no `int as float`)

### Tests
- lexer: the `as` token; `as` as a binding name (`as = 1;`) goes into `tests/parser/invalid/`
  (reserved word)
- parser valid: the three precedence cases (above), `f(x) as string`, ASI cross-line continuation
  (`x = 42` ⏎ `as string`); invalid: `42 as` (missing type)
- typeck valid: the three whitelist entries × (binding / argument / print in place);
  invalid: `1 as float`, `s as string`, `b as string`, `42 as int` (identity)
- verify: `ToStr` stack-depth analysis; vm: three formatting-output unit tests (negative numbers,
  floats matching print's output)
- e2e: `tests/e2e/valid/cast.fln` (`as` + `print(x as string)` full chain)

### Acceptance
```
cargo test
cargo fln tests/e2e/valid/cast.fln
```

---

## Dependencies and Parallelization

```
U01 (Lex)
  ↓
U02 (Parse) ──→ U03 (ASI)                    # U03 only depends on U02, parallel with U04/U05
  ↓
U13 (as cast) → U04 (Typeck-signatures) ──→ U05 (Typeck-ownership)
                     ↓                    ↓
U06 (MIR+Span) ←────────┘
  ↓
U07 (Codegen/Verify)                          # U08 is fully independent, can start first
  ↓                    ┌─────────────┐
U09 (VM-execution) ←──── U08 (VM-Value)           # once U08 is done it can merge in
  ↓                    └─────────────┘
U10 (diagnostics) ←── U07                            # only depends on span_map generation
  ↓
U11 (testing) ←── U09 + U10
  ↓
U12 (wrap-up)
```

- **Suggested serial order for a solo developer**: U08 → U01 → U02 → U03 → U13 → U04 → U05 → U06 → U07 → U09 → U10 → U11 → U12
  (pay the `Rc<str>` debt first, then push the frontend straight through once the VM foundation is
  ready; U13 before U04 — the print fix depends on `as`)
- **Two developers in parallel**: A takes the frontend U01–U07, B takes U08 → meets A at U09, and
  B then does U10

---

## Quality Gates (every ticket must pass)

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

- The prohibited list still applies (SPEC §11): no `unwrap()`/`expect()`, no `Rc`/`RefCell`
  (`Value::Boxed(Box<Value>)` uses `Box`, no fallback to `Rc`), errors carry Spans
- **Run the full 0.0.1 regression before merging every ticket** — `fib.fln` E2E output staying
  byte-for-byte unchanged is a hard gate

---

## Milestones

| Milestone | Tickets | Marker |
|--------|------|------|
| M1 frontend | U01–U03 | New syntax parses; all ASI pitfall cases green; 0.0.1 regression unchanged |
| M2 semantics | U13, U04–U05 | `as` whitelist + Ok/Err/`?`/print checks complete; ownership checker invalid cases all green |
| M3 backend and runtime | U06–U09 | `question.fln` / `box_demo.fln` / `ref_demo.fln` run end-to-end; v1 fixture passes |
| M4 diagnostics and release | U10–U12 | Runtime errors carry line/column; doc verification complete; 0.0.2 released |

---

## Notes

- **0.0.2 still does not implement**: `struct` / `for` / iterators (0.0.3), closures, trait /
  operator overloading / generics (0.1.0), user-defined `Drop`, `ref` locals/globals/returns,
  borrowing into the interior of a box, `addr` / `ptr` / `unsafe` (0.0.5), error type conversions
  (`E` must be exactly equal), the full `as` conversion matrix (`int as float` etc.; 0.0.2 only
  scalar → string, extended after generics)
- **Decisions frozen**: D1–D8, see `PLAN.md` §2. If an implementation needs to overturn a
  decision, change PLAN and the spec docs first, then the code — code deviating from docs is not
  allowed
- **Breaking change list** (the implementation counterpart of the CHANGELOG ⚠️ section):
  owned assignment requires `move`/`clone`; `move`/`clone`/`deref`/`as` become reserved words;
  non-string `print` arguments (including `print(1)`, `print(function value)`) become compile
  errors, help: `as string`
- **v1 compatibility promise**: `.flnc` v1 modules remain executable on the 0.0.2 verify/VM
  (locked by U07/U11); 0.0.1 source programs compile as-is except for the three items on the
  Breaking list
- **The print function**: still hosted in the VM (the `is_builtin` mechanism is unchanged); the
  only addition is typeck's `builtins.rs` signature table
