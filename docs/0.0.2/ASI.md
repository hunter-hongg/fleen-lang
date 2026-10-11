# ASI (Automatic Semicolon Insertion) Design — 0.0.2 / F5

> This document is the **authoritative design** for optional semicolons (ASI) in 0.0.2.
> `DESIGN.md` §3.9 and `PLAN.md` §3.5 give the semantic overview; this document
> governs the implementation details.
>
> **Core decisions (finalized 2026-10-07)**:
> 1. **Newline-sensitive** semantics — statement boundaries are decided at newline trigger points; joining statements on the same line is illegal;
> 2. **Front-running pass** architecture — a standalone ASI pass is added between Lex and Parse;
>    the pass's output token stream is isomorphic to 0.0.1's (semicolons complete, no newlines), so the Parse stage largely keeps its original logic.

---

## 1. Architecture

```text
Source ──Lex──▶ Token stream (with Newline)──ASI pass──▶ Token stream (semicolons complete, no Newline)──Parse──▶ AST
                                ▲
                     parser/asi.rs (pure function, independently unit-tested)
```

The ASI pass is a pure function `Vec<Token> -> Result<Vec<Token>, ParseError>`:

- **Consumes** all `Newline` tokens;
- **Inserts** explicit `Semi` tokens at statement boundaries (zero-width span, attached to the end of the preceding token);
- Reports `ExpectedSemiOrNewStmt` for tokens that can neither continue a statement nor start one.

The pass runs inside the `parse()` entry point, so callers of `parse(tokens)` are unaffected;
external code that calls `tokenize()` directly will see `Newline` tokens (a contract change, see §6).

**Why a front-running pass rather than judgment embedded in the parser**:

- Semantically, newline sensitivity requires the lexer to preserve newline information, so the token stream is no longer "pure";
  rather than having the parser make the call at every wrap point, it is better to concentrate the decision in one standalone, testable pass;
- The pass's output is isomorphic to the 0.0.1 token stream, so the parser's `expect(Semi)` backbone is preserved,
  shrinking the change surface to four accompanying adjustments (§4);
- Golden-style unit tests are intuitive: a token stream with newlines goes in, a stream with semicolons inserted comes out.

---

## 2. Token Contract

### Lexer (new)

| Item | Rule |
|------|------|
| `TokenKind::Newline` | New variant, `Display` as `newline` |
| Emission timing | `\n` (`\r\n` counts as one newline) |
| Folding | Runs of whitespace/blank lines emit only **one** `Newline` |
| Start of file | Newlines at the very start of the source are not emitted |
| Comments | The newline at the end of a line comment **is emitted** (a comment does not terminate a statement); newlines inside block comments are not emitted |
| Strings | A bare newline inside a string is still an `unterminated_string` error (unchanged) |
| EOF | No Newline at end of file (the EOF trigger point is handled by the pass) |

### ASI pass (new)

| Item | Rule |
|------|------|
| Input | Output of `tokenize()` (may contain `Newline`; always ends with `Eof`) |
| Output | No `Newline`; `Semi` inserted at statement boundaries; all other tokens kept in order, unchanged |
| Span of inserted Semi | Zero-width: `Span::new(prev.end, prev.end)` |
| Error | `ParseError { kind: ExpectedSemiOrNewStmt { found }, span }` |

---

## 3. Decision Rules

### 3.1 Trigger Points

Every **Newline** (after folding) and **EOF**. Define

- `p` = the last non-Newline token before the newline (the tail of the pass's output stream so far),
- `n` = the next token after the newline (at the EOF trigger point, `n = Eof`),
- stack = the stack of unclosed brackets (`(` `[` `{`, popped when the matching closing bracket arrives).

Decisions, in order:

| # | Condition | Action |
|---|-----------|--------|
| 1 | Top of stack is `(` or `[` | Discard the newline (no statement boundary inside grouped expressions) |
| 2 | `n` is `}` | Discard the newline (**never insert before `}`**, see §3.4) |
| 3 | `p` ∉ the can-end set | Discard the newline (expression incomplete, continue across lines) |
| 4 | `n` ∈ the implies-stmt-end set | **Insert Semi** |
| 5 | `n` ∈ the continue set ∪ the construct-continuation set ∪ {`;`, `{`} | Discard the newline (continuation wins) |
| 6 | Otherwise | **Report `ExpectedSemiOrNewStmt`** |

### 3.2 Set Definitions

| Set | Members |
|-----|---------|
| **Can-end set** (p side) | `Ident` `IntLit` `FloatLit` `StringLit` `true` `false` `)` `]` `}` `?` |
| **Start set** (`can_start_stmt`) | `func` `const` `if` `while` `choose` `import`, `Ident`, literals, `true`/`false`, `box` `deref` `move` `clone` `not` `!` `-` `(` `{` (block-expression statements) |
| **Implies-stmt-end set** (`implies_stmt_end`) | Start set **∖ {`(`, `-`, `{`** ∪ {`}`, EOF} |
| **Continue set** (`can_continue_expr`) | `+` `-` `*` `/` `%` `==` `!=` `<` `>` `<=` `>=` `and` `or` `?` `.` `[` `(` `as` (0.0.2 U13) |
| **Construct-continuation set** | `else` `elif` `when` `otherwise` |

Notes:

- `-` and `(` belong to both the start set and the continue set: once a statement is complete, **continuation wins** —
  `x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`; `f()` ⏎ `(g())` ⇒ `f()(g())`.
  To start a new statement, write `;`. `!`/`not` are not in the continue set, so `x = 1` ⏎ `!flag` is two statements.
- `not` and `!` are two spellings of the same operator; the start set includes both.
- The implies-stmt-end set excludes `(`, `-`, and `{`: excluding the first two keeps rule 4 and rule 5 from conflicting;
  excluding `{` keeps Allman style (`if c` ⏎ `{`) from terminating a statement,
  at the cost that block-expression statements starting with `{` need a leading semicolon (§5 #4).
- `?` is in both the can-end set and the continue set (suffix chains like `f(x)??`):
  with `p = ?` it can end a statement, with `n = ?` it continues; the two roles do not interfere.
- The start set is used in two places: rule 4's insertion decision in the pass (used indirectly via the implies-stmt-end set)
  and the statement-entry guard in the parser (ASI.md §4 #4).

### 3.3 Attribution of `}` (rule 2 and the construct-continuation set)

The pass is **grammar-blind** and cannot know whether a `}` closes a complete statement or half of a construct:

- `if c { 1 }` ⏎ `else { 2 }` — the `else` belongs to this if; must not split;
- `choose x { when 0 { a }` ⏎ `when 1 { b } }` — `when` is a choose-arm continuation; must not split;
- `func f() { … }` ⏎ `x = 2` — here a semicolon **is** needed after `}`.

Solution: after `}`, insert a semicolon only when `n` ∈ the implies-stmt-end set (start set ∖ {`(` `-`});
`else`/`elif`/`when`/`otherwise` are naturally absent from it, so no grammar knowledge is needed.

### 3.4 Never Insert Before `}` (rule 2)

Whether the last item before `}` is a **tail expression** or a **statement** is undecidable from the flat token stream:

- `{ 42 }`, the arms of `if c { 1 }` — `42`/`1` are tail expressions; inserting a semicolon would **destroy the block value**;
- `{ x = 1 }` — a binding statement; without a semicolon the parser errors.

The two are lexically indistinguishable, so the pass cannot tell them apart. Solution: the pass never inserts before `}`,
and the parser instead **exempts bindings/const immediately followed by `}` from the semicolon** (§4.1).
Bindings can never serve as tail expressions in any version, so this tolerance cannot swallow any tail expression.

### 3.5 Cross-Line Continuation (rule 3)

When `p` cannot end a statement, the newline is discarded, and the following cross-line forms connect naturally (no explicit marker needed):

```fleen
x = 1 +          // p=+ cannot end a statement
    2;
x =
    42;          // p== cannot end a statement
move             // p=move, a prefix keyword, cannot end a statement
    x;
if c &&          // p=and cannot end a statement
   d { 1 } else { 2 };
f(a,             // p=, cannot end a statement (and inside (, rule 1 discards too)
  b);
```

---

## 4. Accompanying Parser-Side Changes

After the pass produces a "semicolons complete" stream, the parser keeps the 0.0.1 backbone plus four accompanying changes:

| # | Change | Location | Reason |
|---|--------|----------|--------|
| 1 | Bindings/const immediately followed by `}` are exempt from the semicolon | `expect(Semi)` in `parse_var_binding` / `parse_const_decl` | §3.4 |
| 2 | Tail-expression determination excludes the Assign form | Tail-expression decision in `parse_block` | `Assign` lowers to `Dup+Store` (leaving the RHS value on the stack) while typeck marks it `Unit`; as a statement the value is popped, as a tail expression the leftover stack value and the type disagree. An Assign (including `deref b = v`, `x.y = v`) adjacent to `}` is always treated as a statement |
| 3 | Block-like head continuation climb | `parse_expr_or` | After `parse_if`/`parse_while`/`parse_choose` return, no operator climbing occurs; `if c { 1 }` ⏎ `- 2` ⇒ `(if-expr) - 2` requires postfix + binary climbing. Ordinary expressions are already covered by the existing greedy parse and need nothing |
| 4 | Statement-entry guard | Top of the `parse_program` / `parse_block` loops | If the current token cannot start a statement (`else`, a stray `)`, `;`, etc.) → `ExpectedSemiOrNewStmt`, replacing the previous generic "expected expression" error from falling into `parse_primary` |

There is also one **semantic tweak**: a line break is allowed between `when <pattern>` and the guard
(`when Ok(v)` ⏎ `if guard { … }`) — the pass will insert a semicolon after the pattern
(`If` ∈ the start set), and `parse_choose` skips that inserted semicolon after the pattern before checking the guard.
A `;` appearing after a pattern is always a pass insertion (no legal source form produces one), so skipping it is unambiguous.

**has_semi semantics**: after the pass fills in semicolons, expression statements always have `has_semi = true`;
the only source of `false` is change #2 (Assign adjacent to `}` treated as a statement).
lower's pop condition `has_semi || !is_last_and_no_tail` is already correct for both values;
MIR/bytecode are unaffected; the field is kept (per ticket convention) and its removal can be evaluated in a later version.

---

## 5. Semantic Resolutions and Known Pitfalls

| # | Resolution | Notes |
|---|------------|-------|
| 1 | Newline-sensitive: joining statements on one line is illegal | `x = 1 y = 2` (no separator) reports `ExpectedSemiOrNewStmt`. A typo that omits an operator is no longer silently swallowed into two statements |
| 2 | Cross-line operator continuation wins | `x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)` (consistent with the DESIGN §3.9 pitfall table) |
| 3 | Cross-line `else`/`elif` attaches to the if | `if c { 1 }` ⏎ `else { 2 }` is **one** if-else expression (the current parser already behaves this way; the old PLAN §3.5.2 table was wrong and has been corrected); the error case is writing `else` after an **already-terminated** if: `if c { 1 }; else { 2 }` |
| 4 | Block-expression statements starting with `{` need a leading semicolon | `{` is not in the implies-stmt-end set → the previous line is not terminated (Allman style `if c` ⏎ `{` is therefore safe). A block expression followed across lines as a standalone statement must be written with `;` — a known JS-style pitfall, documented explicitly |
| 5 | Annotations cannot stand alone on a line | `func f()` ⏎ `: int { …` and `const y` ⏎ `: int = 1` error at the `:`. A formatting restriction; accepted and recorded |
| 6 | Explicit semicolons are always legal | Old and new styles may be mixed; `;;` reports `ExpectedSemiOrNewStmt` at the second `;` (0.0.1 also errors) |

### 5.1 Pitfall Case Table (all go into tests)

| Code | Parses as | Decision path |
|------|-----------|---------------|
| `x = 1` ⏎ `y = 2` | Two statements | Rule 4 (`Ident` ∈ start set) |
| `x = 1` ⏎ `- 2` | `x = (1 - 2)` | Rule 5 (`-` ∈ continue set) |
| `f()` ⏎ `(g())` | `f()(g())` | Rule 5 (`(` continues) |
| `f()` ⏎ `[0]` | `f()[0]` | Rule 5 (`[` continues) |
| `x = 42` ⏎ `as string` | `x = (42 as string)` | Rule 5 (`as` ∈ continue set, U13) |
| `x = 1` ⏎ `!flag` | Two statements | Rule 4 (`!` ∈ start set, ∉ continue set) |
| `x = 1` ⏎ `@` | Syntax error | Rule 6 |
| `if c { 1 }` ⏎ `else { 2 }` | One if-else | Rule 5 (construct-continuation set) |
| `if c { 1 };` ⏎ `else { 2 }` | Syntax error | Rule 3 discards → parser statement-entry guard |
| `choose x { when 0 { a }` ⏎ `when 1 { b } }` | Legal | Rule 5 (construct-continuation set) |
| `{ x = 1 }` / `{ const y = 2 }` | Legal (binding statements) | §3.4 + parser change #1 |
| `{ 42 }` / `{ f() }` | Tail expressions | §3.4 (no insertion) + parser tail determination |
| `{ x = 1; }` | Statement, block value unit | Explicit semicolon |
| `if c { 1 }` ⏎ `- 2` | `(if-expr) - 2` | Rule 5 + parser change #3 |
| `when Ok(v)` ⏎ `if guard { … }` | Legal (guard across lines) | §4 semantic tweak |
| `x = 1 y = 2` (same line) | Syntax error | No trigger point → parser `expect(Semi)` errors |
| `x = 1;;` | Error at the second `;` | Rule 6 |
| Allman: `if c` ⏎ `{`, `func f()` ⏎ `{` | Legal | Rule 5: `{` ∉ implies-stmt-end set → no insertion |

---

## 6. External Contract Impact

| Surface | Impact |
|---------|--------|
| `tokenize()` public API | Output gains a `Newline` token — a contract change, noted in docs |
| `parse()` public API | Runs the pass internally; signature and behavior transparent to callers |
| 0.0.1 programs | All use explicit semicolons; pass output = input (Newlines consumed); MIR/bytecode unchanged byte for byte |
| AST / HIR / MIR | Unaffected; `has_semi` semantics see §4 |
| `fleen-vm` / `fleen-verify` | Call through `parse()`; unaffected |

## 7. Test Plan

- **Pass unit tests** (`parser/asi.rs`): exhaustive decision tables over `TokenKind` for the three predicate functions;
  golden cases cover the entire §5.1 table + §3.5 cross-line continuation + the bracket stack (`f({ x = 1` ⏎ `y = 2 })`).
- **Parser unit tests**: positive and negative cases for each of changes #1–#4; `else` attribution; `when` guard across lines.
- **Fixtures**: `tests/parser/{valid,invalid}/asi_*.fln`;
  the original `missing_semicolon*.fln` (flipped to legal after EOF-triggered insertion) are migrated and rewritten.
- **e2e**: `tests/e2e/valid/asi.fln` — entirely without explicit semicolons + mixed with explicit semicolons,
  as a consistency regression against 0.0.1's `fib.fln` output.
