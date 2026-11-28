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
- The body may contain `break` and `continue` (0.0.9, §6.5).
- Used as a statement, the trailing semicolon is optional.

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
- The body may contain `break` and `continue` (0.0.9, §6.5).
- `for` is a statement; it produces `unit`.
- `for` and `in` are reserved words.
- The only iterables are `[T]` and `Range`. There is **no `for` over a
  `map`** (0.0.11): iterate `std.map.keys(m)` / `std.map.values(m)` instead
  (chapter 13 §13.12).

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
| zero-payload variant name (0.0.9) | that enum variant | `when None { … }` |

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
- A bare identifier naming a **zero-payload variant** of an enum in scope is
  a variant reference, not a binding (0.0.9, D1): it matches that variant and
  binds nothing. Lookup order: bindings registered earlier in the same
  pattern → enclosing locals → zero-payload variant → unresolved. See §6.7.
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
`otherwise` is needed — but `otherwise` is also legal (0.0.9): it is the
always-matching fallback of a Result scrutinee, and it makes any arm set
exhaustive. Guarded arms never count toward coverage: a guard may be false at
runtime, so a guarded arm needs an unguarded duplicate or an `otherwise`.
See chapter 09 §9.2 for the full rules, including `move` on the scrutinee.

## 6.5 break and continue (0.0.9)

`break` exits the innermost enclosing loop; `continue` jumps to its next
iteration. Both are **statements**, not expressions:

```fleen
x = 0;
while true {
    x = x + 1;
    if x > 3 { break; }
};
// x = 4

odd_sum = 0;
for i in range_from(0, 10) {
    if i % 2 == 0 { continue; }
    odd_sum = odd_sum + i;
};
// odd_sum = 25
```

Rules:

- `break` / `continue` bind to the **innermost** enclosing `while` or `for`
  of the same function. Nested loops bind to the inner one; use a helper
  function or a flag to exit an outer loop (there is no labeled break).
- They are compile errors outside a loop body (`BreakOutsideLoop` /
  `ContinueOutsideLoop`): at top level, in a function that contains no loop,
  and in a `choose` arm body — even when that `choose` stands inside a loop.
  Neither keyword may cross a function or `async` boundary.
- They are **not expressions**: a loop has no break-with-value form, so
  `x = break` is a parse error (`break` in expression position is rejected).
- A semicolon after them is optional (ASI, chapter 02): `break;` and `break`
  are the same statement.
- No new bytecode: both lower to a `Jump` to the loop's exit / continue edge
  (0.0.9 D6).

```fleen
func main() {
    x = break;        // parse error: break is not an expression
}
```

```fleen
func f() {
    if true { break; }     // error: `break` outside of a loop
}
```

## 6.6 What is not provided

The language has no `switch`, no `match`, no labeled `break`, no `break`
with a value, no `goto`, and no `try`/`catch`. Loops terminate through their
condition or through `break` (§6.5); error propagation is `Result` + `?`
(chapter 09); multi-way dispatch is `choose`.

## 6.7 Matching `enum` (0.0.6 / 0.0.7 / 0.0.9)

`choose` is exhaustive when every enum variant has an arm (no `otherwise` needed):

```fleen
enum Color { Red, Green, Blue }

func to_int(c: Color): int {
    choose c {
        when Red { 0 }
        when Green { 1 }
        when Blue { 2 }
    }
}
```

0.0.7 patterns are recursive:

- **Full multi-payload destructuring** (D4): `when Rect(w, h)` binds every payload.
- **Nested patterns** (D5): `when Ok(Some(x))`, `when Some(P(a, b))`.
- **Or-patterns** (D6): `when 1 | 2 | 3`, `when First(v) | Second(v)` — alternatives must bind the same names with the same types (otherwise `OrPatternBindingMismatch`); exhaustiveness counts the union of the alternatives' coverage.
- **Type guards** (D7): `when v: int if v > 10` — `v: int` is a compile-time ascription, not a runtime test.

```fleen
func area(s: Shape): float {
    choose move s {           // owned enum needs move
        when Point { 0.0 }
        when Circle(r) { 3.14159 * r * r }
        when Rect(w, h) { w * h }   // full destructuring (0.0.7)
    }
}

func get(o: Option<int>): int {
    choose o {                // Option<int> is Copy, no move needed
        when Some(v) { v }
        when None { 0 - 1 }
    }
}
```

**Zero-payload variant patterns (0.0.9, D1)**: inside a pattern, a bare
identifier that names a zero-payload variant of an enum in scope is a
**variant reference** and does not occupy the binding namespace. The natural
spelling of "both payloads empty" compiles and matches:

```fleen
enum Opt { Some(int), None }
enum P2 { P(Opt, Opt) }

func describe(p: P2): string {
    choose p {
        when P(None, None) { "empty" }     // 0.0.9: legal (was a 0.0.7 error)
        when P(Some(a), None) { "left" }
        when P(None, Some(b)) { "right" }
        when P(Some(c), Some(d)) { "both" }
    }
}
```

Pattern-binding rules:

- A bare *binding* name may appear at most once within one pattern: repeated
  binding names are still a duplicate-declaration error — `when P(Some(x), Some(x))`
  is rejected, and `when P(x, x)` with a local `x` binds twice and is
  rejected for the same reason. Zero-payload variant **references** may
  repeat freely (`when P(None, None)`).
- Lookup order for a bare identifier in a pattern (D1): bindings registered
  earlier in the same pattern → enclosing locals → zero-payload variant →
  unresolved. A binding in the same position wins over a variant reference;
  bind with a fresh name to avoid the clash.
- In nested patterns like `when Ok(Some(x))`, variant names are written bare (`Some`, not `Option.Some`).
- Or-patterns compose with variant references: `when None | Some(0)` binds
  nothing and covers both alternatives (binding consistency is trivial for
  an empty binding set).

**Joint coverage (0.0.9, D7)**: the 0.0.7 rule required every arm to be
individually total for its variant. Exhaustiveness now also accepts
guard-free arms that cover one variant **jointly** — the checker splits the
coverage matrix constructor-by-constructor, so complementary arms compose:

```fleen
func size(p: P2): string {
    choose p {
        when P(None, None) { "empty" }          // 4 arms jointly cover P
        when P(Some(a), None) { "left" }        // — each arm alone is partial
        when P(None, Some(b)) { "right" }
        when P(Some(c), Some(d)) { "both" }
    }
}
```

The composition is strict: every program exhaustive under 0.0.7 stays
exhaustive, and coverage holes across payload positions are still rejected —
`P(Some(a), None)` + `P(None, Some(b))` alone is **not** exhaustive, because
`P(Some, Some)` matches neither arm (`ChooseNotExhaustive`).

Consuming an owned enum uses `choose move <expr>`.
