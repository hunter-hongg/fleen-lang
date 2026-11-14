# 02 — Statements and Semicolons

## 2.1 Statements

A statement is one of:

- a variable binding or assignment: `x = 42;` (chapter 03)
- an `if`, `while`, `for`, or `choose` used in statement position (chapter 06)
- an expression used as a statement, e.g. a call: `print("hi");`
- a declaration: `func`, `struct`, `import`, `pub`, `const` (their own
  chapters)

## 2.2 Semicolons

Semicolons are **optional**: a statement may be terminated with an explicit
`;` or simply ended by a newline. Both programs below are equivalent:

```fleen
x = 1;
y = 2;
```

```fleen
x = 1
y = 2
```

Two declarations never take a semicolon in any version:

- a `func` declaration:

  ```fleen
  func add(a: int, b: int): int = a + b     // no semicolon after the body
  ```

- the last expression of a function body (the return value):

  ```fleen
  func f(): int {
      x = 1;
      x + 1        // no semicolon: this is the return value
  }
  ```

  Writing `x + 1;` there discards the value; the function then returns no
  value.

In a `choose` expression, the `when` clauses and the `otherwise` clause take
no semicolon; the whole `choose` takes one only when it stands alone as a
statement (chapter 06 §6.4).

## 2.3 Statement boundaries

Statement boundaries are decided at newlines. **Two statements cannot be
joined on one line**:

```fleen
x = 1 y = 2     // syntax error
```

A newline does not always end a statement — some tokens continue the
expression onto the next line:

| Next token after a newline | Effect | Example |
|----------------------------|--------|---------|
| a binary operator (`+ - * / % == != < > <= >= and or`) | continues the expression | `x = 1` ⏎ `+ 2` → `x = 1 + 2` |
| unary `-` | continues the expression | `x = 1` ⏎ `- 2` → `x = 1 - 2` |
| `(` | continues the expression (call) | `f()` ⏎ `(g())` → `f()(g())` |
| `[` | continues the expression (index) | `f()` ⏎ `[0]` → `f()[0]` |
| `.` `?` | continues the expression | |
| `else` `elif` `when` `otherwise` | attaches to the preceding construct | `if c { 1 }` ⏎ `else { 2 }` |
| an identifier, literal, `func`, `const`, `if`, `while`, `choose`, `import`, `box`, `deref`, `move`, `clone`, `!`, `{` | ends the statement; a new one starts | `x = 1` ⏎ `y = 2` → two statements |

Examples of each behavior:

```fleen
// continues across lines
x = 1 +
    2;

f()
(g());        // f()(g()) — a call chain; write `;` after f() to avoid this

if x > 0 {
    a = 1
}
else {
    a = 2
}
```

```fleen
// two separate statements
x = 1
!flag
```

```fleen
// already-terminated if cannot take a trailing else
if x > 0 {
    a = 1
};
else {
    a = 2
}             // syntax error: else after a terminated if
```

## 2.4 Block expressions as statements

A `{` at the start of a line does **not** end the previous statement (so that
Allman-style braces keep working). A block expression used as a statement
therefore needs a leading semicolon:

```fleen
x = 1
{ print(x) }      // syntax error: `{` continues the statement above
```

```fleen
x = 1
;{ print(x) }     // legal: leading semicolon
```

A block in statement position containing only bindings is legal on its own:

```fleen
{
    x = 1
}
```

## 2.5 Value-discarding rules

- An expression statement discards its value (except for `Result` values,
  which may not be discarded — chapter 09 §9.5).
- A control-flow expression (`if`, `while`, `choose`) used as a statement
  evaluates for its effect; from ASI's introduction its trailing semicolon is
  optional:

  ```fleen
  if x > 0 { print("pos") };     // explicit semicolon, legal
  if x > 0 { print("pos") }      // same statement, semicolon omitted
  while x < 3 { x = x + 1 };     // same rule
  ```
