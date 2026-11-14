# 06 — Control Flow

## 6.1 if / elif / else

`if` is an expression: it has a value, namely the value of the branch taken.

```fleen
x = 5;
label = if x > 0 { "positive" } else { "not positive" };
```

With `elif`:

```fleen
x = 0;
label = if x > 0 {
    "positive"
} elif x < 0 {
    "negative"
} else {
    "zero"
};
// label = "zero"
```

- The condition must be a `bool` (no truthiness).
- Every branch must produce the same type; the `if` expression's type is that
  type.
- `else` may be omitted; the `if` expression then has type `unit`.
- Used as a statement, the trailing semicolon is optional (chapter 02):

  ```fleen
  if x > 0 { print("pos"); };
  if x > 0 { print("pos"); }
  ```

- `else` can join an `if` across a newline, but not across an already
  terminated statement (chapter 02 §2.3).

## 6.2 while

```fleen
x = 0;
while x < 3 {
    print(x as string);
    x = x + 1;
};
// prints 0 1 2
```

- The condition must be a `bool`; it is re-evaluated before every iteration.
- The body is a loop scope: `x = expr` inside it assigns to a visible outer
  mutable variable (chapter 03 §3.6).
- Used as a statement, the trailing semicolon is optional.
- There is no `break` or `continue`; terminate the loop through the
  condition.

## 6.3 for

`for` iterates over an **array** `[T]` or a **`Range`**:

```fleen
total = 0;
for x in [1, 2, 3, 4] {
    total = total + x;
};
// total = 10
```

```fleen
for i in range_from(0, 3) {
    print(i as string);
};
// prints 0 1 2
```

- The iterable expression is evaluated **once**, before the loop starts.
- The loop variable is a **fresh immutable binding per iteration**: the body
  cannot assign to it, and it does not exist after the loop.
- Assignment to a visible outer mutable variable inside the body works as in
  a `while` body (chapter 03 §3.6).
- `for` is a statement; it produces `unit`.
- `for` and `in` are reserved words.

## 6.4 choose

`choose` matches a value against a series of patterns:

```fleen
x = 1;
s = choose x {
    when 0 { "zero" }
    when 1 { "one" }
    otherwise { "other" }
};
// s = "one"
```

- The scrutinee (`x` above) is an expression evaluated once.
- Patterns are tested top to bottom; the first match wins. There is no
  fallthrough.
- `otherwise` matches anything.
- The `when` and `otherwise` clauses take no semicolon; the whole `choose`
  takes one only in statement position:

  ```fleen
  choose x {
      when 0 { print("zero") }
      otherwise { print("other") }
  };
  ```

### Pattern forms

| Pattern | Matches | Example |
|---------|---------|---------|
| literal | an equal value | `when 0 { … }`, `when "a" { … }`, `when true { … }` |
| negative literal | an equal negative number | `when -1 { … }` |
| identifier | anything; binds the value to that name | `when v { … }` |
| literal `if` guard | the literal, if the guard holds | `when x if x > 10 { … }` |
| `Ok(p)` / `Err(p)` | a `Result` variant, binding its payload | `when Ok(v) { … }` |

```fleen
n = 15;
s = choose n {
    when 0 { "zero" }
    when x if x > 10 { "big" }
    otherwise { "small" }
};
// s = "big"
```

Rules:

- An identifier pattern binds the matched value; the binding is immutable and
  scoped to the arm's block.
- Guarded patterns (`when p if cond`) are only tried when `cond` is true; if
  the guard is false, matching continues with the next pattern.
- An identifier pattern always matches, so it makes any later pattern (and
  `otherwise`) unreachable.
- If no pattern matches and there is no `otherwise`, the program has a
  runtime no-match error; the compiler reports a missing-`otherwise` error
  when it can prove the match is not exhaustive.
- The value of `choose` is the value of the matched arm; all arms must have
  the same type.

### Choosing over a Result

The `Ok`/`Err` patterns handle a `Result`:

```fleen
res = div(10, 2);
out = choose move res {
    when Ok(v) { v }
    when Err(e) { 0 - 1 }
};
```

An **unguarded** `Ok` + `Err` pair covers every `Result` value, so no
`otherwise` is needed. Guarded arms do not count toward this: a guard may be
false at runtime, so it cannot cover its variant. See chapter 09 §9.4 for
`move` on the scrutinee.

## 6.5 What is not provided

The language has no `switch`, no `match`, no `break`/`continue`, no `goto`,
and no `try`/`catch`. Loops terminate through their condition; error
propagation is `Result` + `?` (chapter 09); multi-way dispatch is `choose`.
