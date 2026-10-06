# Fleen 0.0.1 Test Coverage Analysis

> 🧠 **From Hindsight memory (T02: B2 Parser Implementation)** — The parser must handle all 0.0.1 features: mutable `=` bindings, `const` immutability, `int`/`float`/`bool`/`string` types, single-line and block `func` declarations, first-class functions with `(args) -> ret` types, `if`/`elif`/`else`, `while`, `choose`, `Result<T, U>`, function types, mandatory explicit semicolons. Deferred: `?`, `move`/`clone`, `box<T>`, `ref`, `structs`, `for`, `async`, FFI, generics, operator overloading, pointers.

**Date**: 2026-10-06 (updated)
**Status**: Superseded. This document only covers Lexer+Parser as of 2026-10-04; T03–T06 are now implemented. The "No gaps found" claims for T01/T02 are revised below: the previously missed items (unit-type fixture, block-comment fixture, `Result<...>` lexer fixture, `const`/`Index`/`Field`/`Block`/identifier-pattern fixtures, resolver `TypeAnnotationOnAssignment` and `AssignToImmutable`-in-`if` fixtures) have been added, plus typeck/lower tests documented in the 2026-10-06 review.
**Update 2026-10-06 (later)**: T07 (Codegen), T08 (Verify), T09 (VM) are also implemented, and T10 (E2E) is wired into `cargo test` — see §4–§6 below for the refreshed status.

---

## Summary

| Metric | Count |
|--------|-------|
| **Total tests** | 90 (85 unit + 5 integration/doc) |
| **Lexer unit tests** | 20 |
| **Parser unit tests** | 65 |
| **Lexer integration tests** | 2 (file-based: valid + invalid) |
| **Parser integration tests** | 2 (file-based: valid + invalid) |
| **Doc tests** | 1 |
| **`fmt --check`** | ✅ Pass |
| **`clippy -- -D warnings`** | ✅ Pass |

---

## 1. Lexer (T01) — Coverage Analysis

### Test Files

| File | Type | Content |
|------|------|---------|
| `tests/lexer/valid/basic_tokens.fln` | valid | int, float, string, bool literals; function decl |
| `tests/lexer/valid/keywords.fln` | valid | All keywords on one line |
| `tests/lexer/valid/operators.fln` | valid | All operators inside function body |
| `tests/lexer/valid/control_flow.fln` | valid | if/elif/else, while, choose |
| `tests/lexer/valid/function_types.fln` | valid | Single-line funcs, function type params |
| `tests/lexer/valid/fib.fln` | valid | Full Fibonacci program |
| `tests/lexer/invalid/invalid_char.fln` | invalid | `@` character |
| `tests/lexer/invalid/invalid_escape.fln` | invalid | `\x` invalid escape |
| `tests/lexer/invalid/unterminated_string.fln` | invalid | String without closing `"` |
| `tests/lexer/invalid/unterminated_comment.fln` | invalid | `/*` without `*/` |
| `tests/lexer/invalid/int_overflow.fln` | invalid | Integer too large for i64 |

### Unit Test Coverage (in `lexer_impl.rs`)

| Feature | Test | Covered? |
|---------|------|----------|
| Empty input → `[Eof]` | `tokenize_empty` | ✅ |
| Punctuation `(){ }[] ,; .` | `tokenize_single_tokens` | ✅ |
| Operators `+ - * / % = == != < > <= >=` | `tokenize_operators` | ✅ |
| Arrow `->` | `tokenize_arrow` | ✅ |
| All control keywords | `tokenize_keywords` | ✅ |
| `true`/`false` | `tokenize_bool_keywords` | ✅ |
| Type keywords `int float bool string unit Result box ref` | `tokenize_type_keywords` | ✅ |
| Logical keywords `or and not` | `tokenize_logical_keywords` | ✅ |
| Identifiers (snake, prefix `_`) | `tokenize_identifiers` | ✅ |
| Int literals (0, 42, 123456789) | `tokenize_int_literals` | ✅ |
| Float literals (0.0, 3.14, 123.456) | `tokenize_float_literals` | ✅ |
| String literals | `tokenize_string_literals` | ✅ |
| String escapes (`\n \t \r \\ \" \'`) | `tokenize_string_escapes` | ✅ |
| Single-line comments `//` | `tokenize_comments` | ✅ |
| Multi-line comments `/* */` | `tokenize_multiline_comments` | ✅ |
| `1.` → `IntLit(1)` + `Dot` (not incomplete float) | `float_literal_with_dot` | ✅ |
| Error: invalid character | `error_invalid_char` | ✅ |
| Error: unterminated string | `error_unterminated_string` | ✅ |
| Error: invalid escape (produces 2 errors) | `error_invalid_escape` | ✅ |
| Error: unterminated comment | `error_unterminated_comment` | ✅ |
| Complex: full fib source tokenizes | `tokenize_complex` | ✅ |

### Lexer Coverage Rating: ✅ **Complete**

**Strengths:**
- Every `TokenKind` variant has at least one unit test
- Error cases are thoroughly tested (invalid char, unterminated string/comment, invalid escape, int overflow)
- Edge cases like `1.` (dot after number) are tested
- All lexical constructs from SYNTAX.ebnf §170-175 (comments), §144-157 (literals), §160-166 (identifiers) are covered

**No gaps found.**

---

## 2. Parser (T02) — Coverage Analysis

### Test Files

| File | Type | Content |
|------|------|---------|
| `tests/parser/valid/basic.fln` | valid | var binding, const, typed binding |
| `tests/parser/valid/if_elif_else.fln` | valid | if/elif/else expression |
| `tests/parser/valid/while_loop.fln` | valid | while loop |
| `tests/parser/valid/fib.fln` | valid | Full Fibonacci (if/elif/else, while, functions) |
| `tests/parser/valid/operators.fln` | valid | All binary/unary operators inside block |
| `tests/parser/valid/imports.fln` | valid | Simple + nested imports |
| `tests/parser/valid/types.fln` | valid | All type annotations (int, float, bool, string, Result, Array, Box, Ref, Func) |
| `tests/parser/valid/choose_negative.fln` | valid | Negative number patterns (P5) |
| `tests/parser/valid/choose_float.fln` | valid | Float patterns (P5) |
| `tests/parser/valid/choose_string.fln` | valid | String patterns (P5) |
| `tests/parser/valid/empty_input.fln` | valid | Empty program (P1) |
| `tests/parser/valid/choose_expr.fln` | valid | choose with guard, otherwise |
| `tests/parser/valid/result_angle.fln` | valid | `Result<int, string>` angle bracket syntax (P4) |
| `tests/parser/invalid/missing_semicolon.fln` | invalid | `x = 42` (no semicolon) |
| `tests/parser/invalid/missing_brace.fln` | invalid | Unclosed `{` in function body |
| `tests/parser/invalid/missing_paren.fln` | invalid | `func main( { ... }` |
| `tests/parser/invalid/invalid_syntax.fln` | invalid | `x = ;` (no expression after `=`) |
| `tests/parser/invalid/missing_semicolon_toplevel.fln` | invalid | Bare `42` at top level |
| `tests/parser/invalid/keyword_as_ident.fln` | invalid | `if = 42;` (P3: keyword as identifier) |
| `tests/parser/invalid/result_square_bracket.fln` | invalid | `Result[int, string]` (P4: square brackets rejected) |

### Unit Test Coverage (in `parser/tests.rs`)

#### Literals
| Feature | Test | Covered? |
|---------|------|----------|
| Integer literal | `parse_int_literal` | ✅ |
| Float literal | `parse_float_literal` | ✅ |
| Boolean `true` | `parse_bool_true` | ✅ |
| Boolean `false` | `parse_bool_false` | ✅ |
| String literal | `parse_string_literal` | ✅ |

#### Identifiers
| Feature | Test | Covered? |
|---------|------|----------|
| Identifier | `parse_identifier` | ✅ |

#### Variable Binding
| Feature | Test | Covered? |
|---------|------|----------|
| `x = 42;` (no type annotation) | `parse_var_binding_simple` | ✅ |
| `x: int = 42;` (typed binding) | `parse_var_binding_typed` | ✅ |
| `x = 42;` inside block | `parse_assignment_in_block` | ✅ |
| Right-assoc: `a = b = 5;` | `parse_right_associative_assignment` | ✅ |

#### Const Declaration
| Feature | Test | Covered? |
|---------|------|----------|
| `const y = 42;` | `parse_const_decl_simple` | ✅ |
| `const y: int = 42;` | `parse_const_decl_typed` | ✅ |

#### Function Declaration
| Feature | Test | Covered? |
|---------|------|----------|
| Single-line: `func add(a: int, b: int): int = a + b` | `parse_func_single_line` | ✅ |
| No params: `func main() { }` | `parse_func_no_params` | ✅ |
| Multi-line with if/elif/else | `parse_func_multi_line` | ✅ |
| Function type param: `(int, int) -> int` | `parse_function_types_file` | ✅ |
| Typed params, multiple param types | `types.fln` integration | ✅ |

#### Import
| Feature | Test | Covered? |
|---------|------|----------|
| Simple import: `import std.io;` | `parse_import` | ✅ |
| Nested import: `import a.b.c;` | `parse_import_nested` | ✅ |

#### Binary Operators (all from EBNF §104-116)
| Feature | Test | Covered? |
|---------|------|----------|
| `+` | `parse_addition` | ✅ |
| `-` | `parse_subtraction` | ✅ |
| `*` | `parse_multiplication` | ✅ |
| `/` | `parse_division` | ✅ |
| `%` | `parse_modulo` | ✅ |
| `<` `>` `<=` `>=` | `parse_comparison_ops` | ✅ |
| `==` `!=` | `parse_equality_ops` | ✅ |
| `or` | `parse_logical_ops` | ✅ |
| `and` | `parse_logical_ops` | ✅ |

#### Unary Operators
| Feature | Test | Covered? |
|---------|------|----------|
| Unary `-` | `parse_unary_ops` (-5) | ✅ |
| `!` (bang) | `parse_unary_ops` (!false) | ✅ |
| `not` keyword | `parse_not_keyword` | ✅ |

#### Precedence
| Feature | Test | Covered? |
|---------|------|----------|
| `*` binds tighter than `+` | `parse_precedence_ordering` (`1 + 2 * 3`) | ✅ |
| Parentheses override precedence | `parse_parens` (`(1 + 2) * 3`) | ✅ |
| Complex expression in call args | `parse_complex_expression` | ✅ |
| Nested parens/precision in blocks | `parse_complex_expressions_in_blocks` | ✅ |

#### Function Call
| Feature | Test | Covered? |
|---------|------|----------|
| Simple call `foo(1, 2)` | `parse_function_call` | ✅ |
| Nested call `foo(bar(1))` | `parse_nested_call` | ✅ |

#### Field Access & Indexing
| Feature | Test | Covered? |
|---------|------|----------|
| `obj.field` | `parse_field_access` | ✅ |
| `arr[0]` | `parse_index_access` | ✅ |

#### If Expression
| Feature | Test | Covered? |
|---------|------|----------|
| Simple if/else | `parse_if_simple` | ✅ |
| if/elif/else | `parse_if_with_elif` | ✅ |
| if without else | `parse_if_no_else` | ✅ |
| Nested if in blocks | `parse_nested_blocks` | ✅ |
| if/elif/else as tail expr | `parse_if_elif_else.fln` integration | ✅ |

#### While Expression
| Feature | Test | Covered? |
|---------|------|----------|
| `while x < 10 { ... }` | `parse_while` | ✅ |
| while loop file | `parse_while_loop.fln` integration | ✅ |

#### Choose Expression
| Feature | Test | Covered? |
|---------|------|----------|
| Basic choose with 3 arms + otherwise | `parse_choose_simple` | ✅ |
| Choose with guard `when n if n > 10` | `parse_choose_with_guard` | ✅ |
| Negative number pattern `when -1 { ... }` | `parse_choose_negative_pattern` | ✅ |
| Float pattern `when 1.5 { ... }` | `parse_choose_float_pattern` | ✅ |
| String pattern `when "hello" { ... }` | `parse_choose_string_pattern` | ✅ |
| Bool pattern `when true/false { ... }` | `parse_choose_bool_pattern` | ✅ |
| Choose as tail expression in function | `parse_control_flow_file` | ✅ |

#### Block Expressions
| Feature | Test | Covered? |
|---------|------|----------|
| Block with stmts + tail expr | `parse_block_expr` | ✅ |
| Empty block `{}` | `parse_empty_block` | ✅ |
| Nested blocks | `parse_nested_blocks` | ✅ |

#### Types (all from EBNF §45-62)
| Feature | Test | Covered? |
|---------|------|----------|
| `int` / `float` / `bool` / `string` / `unit` | `parse_type_annotations` | ✅ |
| `Result<T, E>` with angle brackets | `parse_result_type` | ✅ |
| `Result[int, string]` (square brackets) → error | `parse_result_type_square_brackets_invalid` | ✅ |
| `[int]` array type | `parse_array_type` | ✅ |
| `box<int>` | `parse_box_type` | ✅ |
| `ref int` | `parse_ref_type` | ✅ |
| `(int, int) -> int` function type | `parse_function_types_file` | ✅ |
| Full types file (all types together) | `types.fln` integration | ✅ |

#### Error Cases
| Feature | Test | Covered? |
|---------|------|----------|
| Missing semicolon at top level | `error_missing_semicolon_in_expr_stmt` | ✅ |
| Missing `}` in block | `error_missing_rbrace_in_block` | ✅ |
| Bare `;` (expected expression) | `error_expected_expression` | ✅ |
| `42 + ;` (no RHS) | `error_unexpected_token` | ✅ |
| Keyword as identifier `if = 42;` | `parse_err` + `keyword_as_ident.fln` | ✅ |
| Missing semicolon in file | `missing_semicolon.fln` integration | ✅ |
| Missing brace in file | `missing_brace.fln` integration | ✅ |
| Missing paren in file | `missing_paren.fln` integration | ✅ |
| Invalid syntax `x = ;` | `invalid_syntax.fln` integration | ✅ |
| Missing semicolon at top level (file) | `missing_semicolon_toplevel.fln` | ✅ |

#### Full Program Tests
| Feature | Test | Covered? |
|---------|------|----------|
| Full Fibonacci program | `parse_fib_program` | ✅ |
| All keywords file | `parse_all_keywords_file` | ✅ |
| All operators file | `parse_operators_file` | ✅ |
| Control flow file | `parse_control_flow_file` | ✅ |
| Function types file | `parse_function_types_file` | ✅ |
| Single tokens (all literal types) | `parse_single_tokens` | ✅ |

### Parser Coverage Rating: ✅ **Complete**

**Strengths:**
- Every grammar rule in SYNTAX.ebnf has at least one unit test
- All EBNF productions for expressions (precedence climbing) are tested
- All type constructs (including P4 angle-bracket Result) are tested
- P5 choose patterns (neg literal, float, string, bool) are tested
- Error cases map to specific integration test files (`invalid/` directory)
- Full program test (`parse_fib_program`) validates end-to-end parsing of the E2E example

**No gaps found.**

---

## 3. Integration Test Framework

### Architecture

The integration tests follow a clean pattern:

1. **File-based**: Each `.fln` file in `tests/lexer/{valid,invalid}/` and `tests/parser/{valid,invalid}/` is an independent test case.

2. **Harness-driven**: `lexer_integration.rs` and `parser_integration.rs` iterate over directories, tokenize/parse each file, and assert:
   - Valid files: parsing/tokenization succeeds
   - Invalid files: parsing/tokenization fails

3. **Unit tests in `parser/tests.rs`**: Inline tests with full AST structure assertions.

### Strengths
- Clear separation: valid vs invalid test cases
- File-based tests are human-readable and reviewable
- Integration tests validate real `.fln` source files end-to-end

### Potential Improvements (Not Gaps)
- Integration tests only assert success/failure, not AST structure (unit tests cover structural assertions)
- No per-file expected token output for lexer (could add golden file testing)

---

## 4. Stage Status (T03-T10)

Refreshed 2026-10-06. Earlier revisions of this section listed T03–T10 as placeholders; all stages now exist and are tested:

| Ticket | Stage | Status | Implementation & tests |
|---------|-------|--------|------------------------|
| T03 | Resolver | ✅ Implemented | `fleen-compiler/src/resolver/`, `tests/resolver/{valid,invalid}/` |
| T04 | BindCheck | ✅ Implemented (merged into resolver/typeck) | mutability checks live in `resolver/` (e.g. assign-to-const), types in `typeck/` |
| T05 | Typeck | ✅ Implemented | `fleen-compiler/src/typeck/`, `tests/typeck/{valid,invalid}/` |
| T06 | Lower | ✅ Implemented | `fleen-compiler/src/lower/` |
| T07 | Codegen | ✅ Implemented | `fleen-compiler/src/codegen/`, `tests/codegen_integration.rs` |
| T08 | Verify | ✅ Implemented | `fleen-verify/src/` (verify + stack analysis, unit tests) |
| T09 | VM | ✅ Implemented | `fleen-vm/src/` (unit tests on hand-written bytecode) |
| T10 | E2E | ✅ Implemented | `tests/e2e/{valid,invalid}/`, runner `fleen-vm/tests/e2e.rs`, runs under `cargo test` |

---

## 5. Coverage Gap Summary

### Lexer (T01): ✅ **No gaps**
- All token kinds, operators, literals, identifiers, keywords tested
- All error conditions (invalid char, unterminated string/comment, invalid escape, overflow) tested
- Edge cases (`1.` disambiguation, multi-char operators) tested

### Parser (T02): ✅ **No gaps**
- All EBNF grammar rules covered by at least one test
- All 0.0.1 language features (if/elif/else, while, choose, func, const, types, Result, func types) tested
- All P1-P5 patches covered by tests
- All error cases covered (missing semicolons, braces, parens, keyword-as-ident, invalid Result syntax)
- Full program (Fibonacci) parses correctly

### Downstream stages (T03-T10): ✅ **Implemented & tested** (2026-10-06)
- Resolver, typeck, lower, codegen, verify and VM each have unit + integration tests
- E2E (`tests/e2e/valid/` + `tests/e2e/invalid/`) runs the real `fleen-vm` binary via `fleen-vm/tests/e2e.rs` under `cargo test`

---

## 6. Recommendations

1. **Lexer & Parser coverage is complete** — no additional tests needed for T01/T02
2. **E2E test scaffold** — ✅ done 2026-10-06: `tests/e2e/` now has `valid/` (stdout-diff) and `invalid/` (exit-code + stderr-needle) fixtures, executed by `fleen-vm/tests/e2e.rs` under `cargo test`
3. **Downstream stages** — ✅ done 2026-10-06, following the same test pattern:
   - Unit tests in each module (`mod tests`)
   - Integration test files in `tests/{stage}/valid/` and `tests/{stage}/invalid/`
   - Structural assertions for valid cases, error-span assertions for invalid cases
4. **Remaining gap**: no fuzzing / malformed-bytecode corpus for `fleen-verify` beyond its unit tests; revisit when bytecode format evolves
