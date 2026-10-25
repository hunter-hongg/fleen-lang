# Fleen Language Draft v0.0.4

> **Fast, Simple, Safe, Intuitive**
> Fast to compile and run · Concise and clear · Memory safe · Intuitive and friendly
>
> **0.0.2 additions**: ownership (`move` / `clone` / `box<T>` / `ref`), `?` and `Ok`/`Err`,
> and ASI (optional semicolons) are finalized — see `docs/0.0.2/ASI.md` for decisions
> and details (authoritative), `docs/0.0.2/PLAN.md` §3.5 for a summary.
>
> **0.0.3 additions**: aggregation — `struct` (§9.4), `[T]` arrays (§9.5), `for` + `Range`
> (§6) — are implemented (V01–V04); the module system (`import` / `pub`), qualified
> cross-module paths and private-access checks are implemented (V05); the `std` library
> (intrinsics, schematic `[T]`, the prelude) is implemented (V06); `std.collection.iter`
> iterator helpers (V07) and the `flint` package manager (V08) are implemented. Design
> decisions D1–D10 are recorded as settled (§11 summary table). Bytecode in `docs/BYTECODE.md` §5.12–5.14.
> The corresponding sections below have been annotated accordingly
> (content marked "0.0.2"/"0.0.3" reflects current toolchain behavior from the ticket it landed in).
>
> **0.0.4 additions**: cooperative concurrency — `async func` + postfix `await` over
> `Future<T>` coroutine frames (§6), the `channel<T>` message-passing type (§9.7), and the
> ownership boundary that keeps the async world borrow-free (§10.6). Decisions D1–D6 are
> recorded in `docs/0.0.4/PLAN.md`; bytecode v4 (async / channel opcodes 0xC0–0xC7) is in
> `docs/BYTECODE.md` §5.15. `async` / `await` / `channel` are new reserved words (§2).

---

## 1. Positioning

Fleen is a statically typed language that compiles to bytecode. Its syntax follows Python, its ownership model follows Rust, and its design philosophy is **explicit, concise, no magic**.

> **Note**: 0.0.1 requires explicit semicolons `;`; starting with 0.0.2, ASI (automatic
> semicolon insertion) is in place and semicolons are optional (explicit semicolons
> remain legal). See §3.9 for the decision rules.

- **File extension**: `.fln`
- **Compilation target**: bytecode + VM
- **Programming style**: DOP + FP, no OOP
- **Memory model**: no GC, ownership + borrowing

---

## 2. Lexical Structure

### Comments

```fleen
// single-line comment
/* multi-line comment */
```

### Identifiers

| Category | Style | Example |
|----------|-------|---------|
| Variables/functions | `snake_case` | `some_var` |
| Types/structs | `PascalCase` | `SomeType` |
| Modules | `foo.bar` | `std.io` |

### Literals

```fleen
42           // int
3.14         // float
true / false // bool
"hello"      // string
```

### Keywords

```
func  const  struct  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
box  ref  deref  move  clone  as  true  false
async  await  channel
```

> `deref` is a new keyword in 0.0.2 (reads/writes the value inside a box, see §10.3);
> `as` is a new reserved word in 0.0.2 (type conversion, see §9 "Type Conversion `as`").
> `struct` (V02) / `for` / `in` (V04) / `pub` (V05) are new reserved words in 0.0.3 —
> programs using them as identifiers are the **only** source breaking change of 0.0.3
> (the CHANGELOG ⚠️ Breaking list; see `docs/0.0.3/PLAN.md` D4/D6).
> `async` / `await` / `channel` are new reserved words in 0.0.4 (see §6, §9.6, §9.7) —
> 0.0.3 programs using them as identifiers must rename (the CHANGELOG ⚠️ Breaking list;
> see `docs/0.0.4/PLAN.md` §11). Note `Future` is **not** reserved: only the `Future<T>`
> type form is recognized in type position, and a bare `Future` identifier still names a
> user struct.

---

## 3. Variables and Bindings

### 3.1 Core Principle

**Binding and assignment are the same kind of operation.**

`=` means "make the left side be the right side." The compiler distinguishes a first occurrence (binding) from a subsequent occurrence (assignment), but **users do not need to know the difference**.

```fleen
x = 42;        // make x be 42
x = 43;        // make x be 43
```

### 3.2 Mutable Bindings

```fleen
x = 42;
```

- Mutable by default
- Can be reassigned
- May carry a type annotation: `x: int = 42;`

### 3.3 Immutable Bindings

```fleen
const y = 42;
```

- `const` decorates the name
- Cannot be reassigned
- May carry a type annotation: `const y: int = 42;`

### 3.4 Assignment

```fleen
x = 43;       // x already exists and is mutable
```

- Assigning to an immutable variable → compile error
- Assignment cannot carry a type annotation

### 3.5 Rule Summary

| Operation | Syntax | Condition |
|-----------|--------|-----------|
| Mutable binding | `x = 42;` | `x` is not bound at the current position |
| Immutable binding | `const x = 42;` | `x` is not bound at the current position |
| Assignment | `x = 43;` | `x` is already bound at the current position and is mutable |
| Assignment inside a loop body | `while c { x = x + 1; }` | A mutable outer `x` already visible inside the loop body → **assignment**, not shadowing (§4.5) |
| Type annotation | `x: int = 42;` | First binding only |
| Assigning to a `const` | `y = 43` | ❌ compile error |
| Assignment with a type annotation | `x: int = 43` | ❌ compile error (already an assignment) |
| Rebinding a `const` | `const x = 43` | ❌ compile error (`x` already exists in the same scope) |
| Duplicate declaration of the same name | `func f` × 2, `func f(a, a)` | ❌ compile error (one name, one binding) |

### 3.6 Disambiguating the Three Meanings of `=`

`=` has three meanings. The compiler decides them in the following order — **there is exactly one decision path**:

```fleen
x = 42;    // (1) binding: no x at the current position (current scope, or a visible outer one inside a loop body)
x = 43;    // (2) assignment: x already exists at the current position
x = 44;    // (2) assignment: same as above, the same binding
```

| Position | Same-name binding present? | Meaning |
|----------|---------------------------|---------|
| Statement position | Not in current scope | (1) binding (if a same-name binding is visible in an outer scope → shadowing, see §4.3) |
| Statement position | In current scope | (2) assignment (must be mutable, no type annotation) |
| Statement position (inside a loop body) | A mutable binding exists in an outer scope (up to the function boundary) | (2) **assignment** (not shadowing, see §4.5) |
| Expression position | In current scope | (2) assignment |
| Expression position | Not present | ❌ error (`=` in expression position **never binds**) |

**In expression position, `=` has only one meaning: assignment**, and the target lookup **does not cross function boundaries**:

```fleen
x = 1;
func f() {
    y = (x = 2);   // ❌ error: x is not in f's scope chain; function boundaries are impenetrable
}
```

Chained assignment works the same way — the target on the right must already exist:

```fleen
b = 5;
a = b = 3;        // ✅ b is bound → assignment
a = b = 5;        // ❌ error: b is not bound
```

> **About `x = x + 1`**: an error on first occurrence (use before binding, §3.1); once bound, it is a normal assignment (2). Inside a loop body, `x = x + 1` is an assignment to the **outer** `x`; see §4.5.

> **Semicolon note**: 0.0.1 requires an explicit semicolon `;` at the end of statements.
> Starting with 0.0.2, ASI (automatic semicolon insertion) is in place and semicolons
> may be omitted (explicit semicolons remain legal). See §3.9 for the decision rules.
>
> **Semicolon rules for expression statements**:
> - When control-flow expressions such as if, while, or choose are used as statements, 0.0.1 requires a trailing semicolon (optional from 0.0.2 on)
> - In a choose-when construct, `when` clauses and the `otherwise` clause themselves take no semicolon
> - Only when the `choose` expression as a whole is used as an expression statement does it take a trailing semicolon
> - A trailing expression in a function body (the return value) takes no semicolon

### 3.7 Error Messages

#### Assigning to a `const`

```
error: cannot assign to immutable variable
  --> fib.fln:3:5
   |
 2 | const y = 42
   | ^^^^^ `y` is immutable
 3 | y = 43
   | ^ cannot assign
   |
help: use `y = 43` without `const` if you need mutability
```

#### Assignment with a type annotation

```
error: type annotation not allowed on assignment
  --> fib.fln:3:5
   |
 2 | x = 42
 3 | x: int = 43
   | ^ type annotation is only allowed on first binding
   |
help: remove the type annotation: `x = 43`
```

### 3.8 Ownership and Bindings (0.0.2)

Values fall into three categories (see §10.1 for the full model):

| Category | Types | Binding/assignment/parameter-passing semantics |
|----------|-------|------------------------------------------------|
| Copy | `int` `float` `bool` `unit`, function values | Implicit bitwise copy; everything as before (0.0.1 rules unchanged) |
| Owned | `string` `box<T>`, `Result<T, E>` containing an owned component | Single owner; moving requires `move`, copying requires `clone` |
| Borrow | `ref T` (function parameter position only, §10.4) | Read-only borrow, no transfer |

**Core rules from the binding perspective:**

```fleen
s = "hello";       // binds an owned value (the RHS is a fresh value, no keyword needed)
t = move s;        // move: s is no longer usable afterwards
t = clone t;       // deep copy: the original value remains usable
y = fib(n);        // legal: a function's return value is a fresh value, naturally transferred
```

1. Copy types **never** require `move` / `clone`;
2. An owned local variable entering a **consumption position** (top level of a binding/assignment RHS, an actual argument, a `choose` scrutinee)
   must be explicitly `move`-ed / `clone`-ed; writing it bare is a compile error (the help message suggests the missing keyword);
3. **Production positions transfer implicitly**: values at the end of a function, a block, or an if/choose branch flow out through the "exit";
   a transfer at the end of a branch is conditional, and later use reports "possibly moved";
4. Comparison operators and `print` arguments are **read-only uses**: copied automatically, never transferred;
5. Forbidden: `move deref b` (moving out of a box), `move g` (moving out of a global — reading an owned global is a deep copy and writing it replaces it),
   `move s` (where s is a `ref` parameter).

Ownership checking is a flow-sensitive analysis inside typeck (`typeck/ownership.rs`, new in 0.0.2),
per function, never crossing function boundaries.

### 3.9 Semicolons and ASI (0.0.2)

Starting with 0.0.2, semicolons are optional and the semantics are **newline-sensitive**: statement boundaries are decided at newlines, and joining statements on one line is illegal.
Architecturally, a separate **ASI pass** (a pure function in `parser/asi.rs`) is inserted between Lex and Parse:
it consumes all `Newline` tokens, inserts explicit `;` at statement boundaries, and reports `ExpectedSemiOrNewStmt`
for tokens that can neither continue nor start a statement. The token stream the pass emits is isomorphic
to 0.0.1's (all semicolons present), so the Parse phase keeps its existing logic.

> Once a statement is complete (at a newline or EOF) —
> 1. If the next token is in the **implicit terminator set** → insert `;` (terminate the statement);
> 2. If the next token is in the **continuation set** or a constructor continuation set (`else` `elif` `when` `otherwise`) → do not terminate; continuation takes precedence;
> 3. If the previous token cannot end a statement (after an operator, `=`, `,`, or a prefix keyword) → do not terminate; continue across lines;
> 4. Otherwise → error `ExpectedSemiOrNewStmt` (with a Span and a hint about what was expected).

| Set | Members |
|-----|---------|
| New-statement start set | `func` `const` `if` `while` `choose` `import`, identifiers, literals, `box` `deref` `move` `clone` `not` `!` `-` `(` `{` |
| Continuation set | `+` `-` `*` `/` `%` `==` `!=` `<` `>` `<=` `>=` `and` `or` `?` `.` `[` `(` |
| Terminator set | `}` EOF |

> Implicit terminator set = start set ∖ {`(`, `-`, `{`} ∪ {`}`, EOF}: `(` and `-` favor continuation,
> and the exemption of `{` keeps Allman style from terminating a statement (the cost: block expression
> statements need a leading semicolon).

**Trap cases** (must go into the spec and tests):

| Code | Parse result | Explanation |
|------|--------------|-------------|
| `x = 1` ⏎ `y = 2` | Two statements | Identifiers are in the start set |
| `x = 1 y = 2` (same line) | Syntax error | Newline-sensitive: joining on one line is illegal, so typos are not silently swallowed |
| `x = 1` ⏎ `- 2` | `x = (1 - 2)` | `-` is in the continuation set; binary continuation takes precedence |
| `f()` ⏎ `(g())` | `f()(g())` call chain | `(` is in the continuation set (call); write `;` to separate them |
| `f()` ⏎ `[0]` | `f()[0]` index | `[` is in the continuation set |
| `x = 1` ⏎ `!flag` | Two statements | `!` is not a binary continuation; `!flag` is in the start set |
| `if c { 1 }` ⏎ `else { 2 }` | One if-else expression | `else` attaches to that if (cross-line joining is legal) |
| `if c { 1 };` ⏎ `else { 2 }` | Syntax error | `else` does not continue an if statement that is **already terminated** (by a semicolon) |
| `{ x = 1 }` | Legal (a binding statement) | No semicolon before `}`; the parser distinguishes a trailing expression from a binding structurally |
| `x = 1` ⏎ `{ print(1) }` | Syntax error | A block expression statement starting with `{` needs a leading semicolon (`{` is not in the implicit terminator set — this is also the cost of keeping Allman style safe) |

Invariants: the rules that a `func` declaration takes no trailing semicolon, a block's trailing
expression takes no semicolon, and `when`/`otherwise` take no semicolon are unchanged;
the presence or absence of semicolons is invisible after the AST (it does not affect MIR or bytecode).
For the architecture, the precise definitions of the sets, and the full trap table, see
**`docs/0.0.2/ASI.md`** (the authoritative design).

---

## 4. Scopes and Shadowing

### 4.1 Block Scopes

Every `{}` is a **new scope**.

```fleen
if cond {
    x = 42;      // binding inside the block
};
print(x);        // error: x is not in scope
```

### 4.2 Function Scopes

A function body is an **independent scope**.

```fleen
x = 42;
func foo() {
    x = 43;      // a new binding, unrelated to the outer x
}
```

### 4.3 Shadowing

#### Shadowing is allowed

An inner scope may rebind a variable with the same name.

```fleen
x = 42;
if cond {
    x = 43;      // a new binding, shadowing the outer x
};
print(x);        // 42 (the outer x was not modified)
```

#### Mutability must match

**When shadowing, the inner and outer bindings must have the same mutability.**

| Outer | Inner | Allowed? |
|-------|-------|----------|
| Mutable | Mutable | ✅ |
| `const` | `const` | ✅ |
| Mutable | `const` | ❌ |
| `const` | Mutable | ❌ |

```fleen
// allowed
x = 42;
if cond {
    x = 43;          // mutable shadowing mutable
};

const x = 42;
if cond {
    const x = 43;    // immutable shadowing immutable
};

// not allowed
x = 42;
if cond {
    const x = 43;    // error: mutability mismatch
};

const x = 42;
if cond {
    x = 43;          // error: mutability mismatch
};
```

#### Semantics of shadowing

**Shadowing is "replacement of the same kind", not "define new variables however you like".**

- The inner `x = 43` is a **new binding** and does not affect the outer one
- The outer `x` still exists but is not visible in the inner scope
- After leaving the inner scope, the outer `x` becomes visible again

#### Functions do not participate

A function body is an independent scope and is **not subject to the outer shadowing rules for user variables**. Function boundaries are **impenetrable**: bindings inside a function are never compared for shadowing against same-named user variables outside it.

```fleen
x = 42;
func foo() {
    const x = 43;    // legal: inside a function is a new scope, not shadowing
}
```

#### Parameters may be shadowed (an in-function exception)

Function parameters are bindings within the function body and are **not protected by the "functions do not participate" rule**. A function parameter may be shadowed inside the body by a new binding of the same name, with **no mutability-consistency check** (a parameter is not an outer user declaration):

```fleen
func f(x: int): int {
    x = 2;      // legal: inside the function body this shadows the parameter, not an assignment
    x
}
```

#### Exception to shadowing: builtin function names

A user binding with the same name as a builtin function (such as `print`) does not count as a conflict and simply overrides it: the user may declare their own `print` with `print = 42;`, after which the name refers to the user binding.

---

### 4.5 Assignment Rules Inside Loop Bodies

The body of a `while` (and of a `for`, 0.0.3) is a **loop scope**. When `x = expr` appears inside a loop body (including its nested blocks):

- If `x` already exists in the current scope (the loop body itself) → (2) assignment (as usual)
- Otherwise, if an `x` that is **already visible within the current function scope before the current position** exists and is a **mutable user variable** → (2) **assignment**, not shadowing
- Otherwise → (1) binding (a new shadow or a first binding)

This makes the counter idiom work as expected:

```fleen
x = 0;
while x < 10 {
    x = x + 1;    // an assignment to the outer x, not shadowing
};
```

Conversely, if the outer `x` is immutable (`const` or a parameter), `x = expr` inside the loop body is a **compile error** (assignment to an immutable binding):

```fleen
const x = 0;
while x < 10 {
    x = 1;      // ❌ compile error: cannot assign to a const
};
```

Lookup in a loop scope does not cross function boundaries: a loop inside a function body cannot assign to a global or to a same-named outer binding. If the function has no corresponding local binding, `x = 1` is a **new binding within the loop scope** (shadowing semantics), not an assignment to the outer variable:

```fleen
x = 0;          // global x
func f() {
    while true {
        x = 1;  // a new binding (inside the loop body); does not write the global x
    };
}
```

In expression position, `=` inside a function **cannot** bind a global: in `y = (x = 1);`, if `x` is not declared inside the function, it is a compile error (§3.6).

> Shadowing inside a loop body is still available, but can only occur **in nested blocks of the loop body**. When the outer binding is `const`, `while c { const x = ... }` is a shadowing error due to a mutability mismatch (§4.3).

### 4.4 Shadowing Error Messages

#### Shadowing with a mutability mismatch

```
error: shadowing must preserve mutability
  --> fib.fln:3:5
   |
 2 | x = 42
   | ^ outer `x` is mutable
 3 |     const x = 43
   |     ^^^^^ inner `const` does not match outer mutability
   |
help: use `x = 43` for a mutable shadow, or rename the variable
```

---

## 5. Functions

```fleen
// single-line function
func add(a: int, b: int): int = a + b

// multi-line function
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

// no return value
func greet(name: string) {
    print("Hello, ", name);
}

// functions are first-class values
func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)
result = apply(add, 1, 2);
```

**Rules:**
- The `func` keyword defines a function
- Single-line: `= expr`
- Multi-line: `{ block }`, where the last expression is returned automatically
- Functions are first-class values: they can be assigned, passed, and returned
- No `return` keyword (0.0.1 does not implement early returns for now)

---

## 6. Control Flow

### if / elif / else

```fleen
if x > 0 {
    "positive"
} elif x < 0 {
    "negative"
} else {
    "zero"
};
```

`if` is an expression and has a return value. **Note: 0.0.1 requires a semicolon at the end of expression statements (from 0.0.2 on, ASI makes it optional, §3.9).**

### while

```fleen
while x < 10 {
    x = x + 1;
};
```

**Note: 0.0.1 requires a semicolon at the end of expression statements (from 0.0.2 on, ASI makes it optional, §3.9).**

**0.0.1 only implements `while`; `for` did not exist until 0.0.3 (V04, §6 below).**

### for (0.0.3)

```fleen
// Sum an array
total = 0;
for x in [1, 2, 3, 4] {
    total = total + x;
};

// Iterate a range (constructed via std.collection.range; there is no `..`
// operator syntax in 0.0.3 — D5, PLAN §2): `range(n)` is 0..n,
// `range_from(a, b)` is a..b (both end-exclusive)
for i in range_from(0, 10) {
    print(i as string);
};
```

**Rules:**
- The iterable must be an **array** `[T]` or a **`Range`**; a `Range` iterates `start, start+1, …, end-1` (end-exclusive, so the range is empty when `start == end`)
- The loop variable is a **fresh immutable binding** per iteration — the body cannot assign to it
- The iterable expression is **evaluated once**, before the loop begins
- The loop variable is **out of scope** after the loop (it is a fresh binding, not an outer binding)
- `for` yields `unit` (it is a statement, not an expression with a return value)
- Assignment to an outer mutable variable inside the loop body works like a `while` body (§4.5)
- `for` and `in` are now **reserved keywords** (breaking change from 0.0.2)

### async / await (0.0.4)

```fleen
// async function: the caller sees Future<T>; the body still returns T
async func producer(c: channel<string>) {
    c.send("hi");              // a fresh value transfers naturally (§9.7, D6)
}

async func fib(n: int): int {
    if n < 2 { n }
    else { fib(n - 1) await + fib(n - 2) await }   // await binds like a postfix suffix
}

// driving a future
f = fib(10);
n = f await;                   // postfix: Future<int> → int (drives to completion)
fib(10) await;                 // statement form: drive and discard the payload

// `e await?` composes with ? — inside a Result-returning function:
func halve(n: int): Result<int, string> {
    v = half(n) await?;        // (half(n) await)? — the §8 propagation rules
    Ok(v)
}
```

**Rules:**
- `async func f(...): T` — calling `f` does **not** run the body: it packages the arguments
  into a coroutine frame and returns a `Future<T>`; the body runs when something drives the
  future (`e await`, or the executor via `std.async.run`). `Future<T>` is non-Copy — it moves
  like any owned value, and it can be driven only once
- `await` is a **postfix operator** (reserved word), binding with the tightest suffixes
  (`() [i] .f await ?` — chain left-to-right, same layer as `?`), so `Future<T>` stays a
  first-class, passable value and `fib(n - 1) await + fib(n - 2) await` needs no parentheses
- **Cooperative executor** (single-threaded, in the VM): a task runs until a suspension
  source — `std.io.sleep(n)`, `std.async.yield_now()`, or a channel `send`/`recv` that cannot
  proceed — yields; ready tasks always run before timer-parked ones. `std.async.run(tasks:
  [Future<unit>])` spawns all the roots, runs them to completion round-robin, and joins
  (usable inside an async body too — a nested, re-entrant join)
- A **top-level** `e await` (or `std.async.run(...)`) in `main` boots a blocking drive that
  runs the scheduler until the root completes — a normal program "just works", preserving
  the single-command simplicity
- Inside an async body a suspension source needs **no explicit await**: the compiler lowers
  `std.io.sleep(n)` to the call followed by a `Suspend` (the task parks until its timer
  fires); a synchronous caller gets a no-op
- A panicked/failed future surfaces its error at the awaiting site as a runtime error, like
  any other
- **Out of scope in 0.0.4** (deferred to 0.1.0 with traits/generics): `async {}` blocks,
  `select`/`join!` fan-out, language-level composing/generic async (`dyn Future`-style
  polymorphism) — see `docs/0.0.4/PLAN.md` §9
- The ownership boundary of async code (no `ref`, no global writes) is §10.6; the bytecode
  (coroutine frames, opcodes 0xC0–0xC7) is `docs/BYTECODE.md` §5.15; decisions D1–D6 are in
  `docs/0.0.4/PLAN.md`

---

## 7. Pattern Matching: choose

```fleen
choose value {
    when 0 { "zero" }
    when 1 { "one" }
    when x if x > 10 { "big" }
    otherwise { "other" }
};
```

**Rules:**
- No `fallthrough`
- `otherwise` must be present (or the compiler can prove exhaustiveness)
- Guard conditions with `if` are supported
- `when` patterns support literals (including negative numbers) and identifier bindings
- No `match` / `switch`
- **Semicolon rules**: `when` clauses and the `otherwise` clause themselves take no semicolon; only when the `choose` expression as a whole is used as a statement does it take a trailing semicolon

**Pattern syntax (P5 addition):**
```text
pattern = neg_pattern | literal | ident
neg_pattern = "-" , (int_lit | float_lit)
```

---

## 8. Error Handling

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}

func ratio(a: int, b: int): Result<int, string> {
    q = div(a, b)?;      // on Err, the whole function returns that Err immediately
    r = div(q, 2)?;
    Ok(r)
}

res = choose ratio(10, 2) {
    when Ok(v) { v }
    when Err(e) {
        print("error: ", e);
        0 - 1
    }
};
```

> **Version note**: 0.0.1 has no `Ok`/`Err` constructor syntax or patterns (`Result<T, E>` is
> type-only and requires a nil binding + explicit handling via otherwise); the syntax in the
> example above and `?` were officially adopted in **0.0.2**
> (see `docs/0.0.2/PLAN.md` §3.4 for the decision).
>
> **Lexical classification** (0.0.2): `move` / `clone` / `deref` are **keywords**
> (`is_keyword()` returns true; they change from legal identifiers to reserved words, and
> "using a reserved word as a binding name" is reported by the parser where an identifier is
> expected); `?` is a **punctuation token** (`TokenKind::Question`,
> `is_keyword()` returns false) and does not participate in reserved-word checks.

**0.0.2 semantics:**

- `Ok(expr)` / `Err(expr)` are builtin constructor expressions (users may shadow them, same mechanism as `print`);
  their types are determined by **context** (a function's return type annotation, a binding's type annotation, the containing function's return type for `?`,
  or the choose scrutinee); with no context → compile error, with a hint to add a type annotation
- `choose` supports the `when Ok(v)` / `when Err(e)` patterns, with bindings following **move** semantics;
  an `Ok` + `Err` pair is exhaustive on its own, no `otherwise` needed; guarded arms do not count toward exhaustiveness
- The `?` postfix operator: the operand type must be `Result<T, E>`, and the containing function's return type must be
  `Result<T2, E>` (`E` must be exactly equal; 0.0.2 has no error type conversion); on `Err`, the whole function returns that `Err` immediately
- If an expression statement's value type is `Result<T, E>` → compile error (`UnhandledResult`):
  Results may not be silently discarded; the hint is to bind it or add `?`
- `main` may return `Result<T, E>`: when the VM receives an `Err`, it prints to stderr and exits with code 1
- No `return` keyword (§5): `?` is the only early-return mechanism
- `Result<T, E>` is a lightweight algebraic type (angle brackets, consistent with the EBNF); errors must be handled explicitly;
  `panic` is only for unrecoverable errors and cannot be caught

---

## 9. Types

### Primitive Types

```fleen
int
float
bool
string
unit
```

### Composite Types

```fleen
[T]              // array (0.0.3): owned for every T; literals `[e1, e2, …]`, `a[i]` read/write, bounds-checked
struct S { … }   // user-defined aggregate (0.0.3); a struct name is a type; is_copy = all fields Copy
Range            // builtin Copy type (0.0.3 V04): an int interval, end-exclusive; constructed
                 // via std.collection.range (no `..` syntax in source); `for x in r` iterates it
box<T>           // heap pointer, single ownership; box expr allocates, deref reads/writes (0.0.2, §10.3)
ref T            // read-only borrow, function parameters only (0.0.2, §10.4)
Result<T, E>     // error handling (Ok/Err/? officially adopted in 0.0.2, §8)
Future<T>        // a suspended async coroutine (0.0.4, §9.6): non-Copy, driven once by `e await`
channel<T>       // message-passing channel (0.0.4, §9.7): non-Copy, move-only, owns its buffer
```

> For struct / array syntax and rules see the §9.4 and §9.5 subsections below; the bytecode for
> each is in `docs/BYTECODE.md` §5.12–5.13 and the design decisions are in `docs/0.0.3/PLAN.md` D1–D5.

### 9.4 struct (0.0.3)

```fleen
struct Point {
    x: int,
    y: int,
}

p = Point { x: 1, y: 2 };    // literal — all fields must be initialized
q = p.x;                     // field read (a copy; deep for owned fields)
p.y = 3;                     // field write (one level, on a local/global struct only)
```

**Rules:**
- `is_copy(S)` = **all fields are Copy**; a struct with any owned field (`string`, `box`, array,
  owned struct) is an owned type: consuming positions need `move S` / `clone S`
- **Field read `p.f` is a copy** of the field (deep for owned fields) — `move p.f` is forbidden
  (`MoveOutOfStruct`); a read uses `clone`-free deep-copy semantics
- **Field write `p.f = e`** is a legal assignment target when `p` is a local/global struct binding
  and `f` is a direct field; owned `e` needs `move` / `clone` as usual
- **Recursive structs are forbidden** (directly or via `box`) → `RecursiveStruct`
- No `==` / no `choose` patterns on structs in 0.0.3 (§1.4, PLAN)

### 9.5 arrays `[T]` (0.0.3)

```fleen
a = [1, 2, 3];            // a: [int]; empty literals need a type annotation (`empty: [int] = [];`)
x = a[0];                 // read: bounds-checked copy (deep for owned elements)
a[1] = 10;                // write: bounds-checked
t = clone a;              // deep copy (new array + new elements)
u = move a;               // transfer; a is unusable afterwards
```

**Rules:**
- `[T]` is an **owned type for every `T`** (even `[int]`): the whole array moves / deep-copies
  (PLAN D3) — there is no Copy array
- Element type is **uniform**: `ArrayTypeMismatch` if later elements differ; empty literals need a
  type annotation (`CannotInferArrayType`)
- All index operations are **bounds-checked at runtime**: out of bounds → runtime
  `IndexOutOfRange` (with source span); `a[-1]` is out of range, never a huge index
- **`move a[i]` is forbidden** (`MoveOutOfArray`); element reads are copies, writes are in place
- Arrays are legal as struct fields, `box<[T]>` pointees, `Result` components, parameters
  (owned → `move`/`clone`; `ref [T]` allowed, parameters only), and globals

### 9.6 `Future<T>` (0.0.4)

```fleen
async func half(n: int): Result<int, string> {
    if n == 0 { Err("cannot halve 0") } else { Ok(n / 2) }
}

func halve(n: int): Result<int, string> {
    v = half(n) await?;          // drive, then propagate (§8 rules)
    Ok(v)
}
```

**Rules:**
- `Future<T>` is the type of a **suspended async coroutine** (a heap frame holding the
  function, its locals, the saved operand stack, and the resume PC — D1,
  `docs/0.0.4/PLAN.md`): `e await` with `e: Future<T>` yields `T`
- **Non-Copy, moves like an owned value, driven once**: a future can be passed to another
  function or stored, but not cloned and not driven twice
- `Future` itself is **not** a reserved word — a bare `Future` identifier remains a named
  type (a user struct); only the `Future<T>` parameterized form is recognized in type
  position
- `Future<T>` composes with `Result`: `e await?` on a `Future<Result<T, E>>` drives the
  future and propagates the `Err` (the `?` machinery of §8, applied after the await)
- Copy types (`int` / `float` / `bool` / `unit` / function values) remain Copy inside async
  functions; the async body works exclusively on owned + Copy values (§10.6)

### 9.7 `channel<T>` (0.0.4)

```fleen
ch = channel<int>(16);            // bounded buffer of 16 (reserved-word constructor)
ch = channel<string>(0);          // rendezvous: a send parks until a receiver takes the value
peer = std.concurrent.split(ch);  // the one explicit fork point: a second handle, same buffer

async func producer(c: channel<string>) {
    c.send("hi");                 // fresh value transfers naturally
}
async func consumer(c: channel<string>) {
    s = c.recv();                 // s owns the received string
    std.io.println(s);
}

std.async.run([producer(move ch), consumer(move peer)]);   // move a handle into each task;
                                                           // run's tasks are Future<unit>
```

| Operation | Kind | Blocks when |
|-----------|------|-------------|
| `channel<T>(capacity)` | constructor (reserved word) | — |
| `c.send(v)` | awaitable | buffer full (or, at cap 0, until a receiver takes the value) |
| `c.recv()` | awaitable | buffer empty |
| `std.concurrent.split(ch)` | plain call | — (returns a second handle) |

**Rules:**
- `channel` **is a reserved word**: it has both a type form (`channel<T>`) and a constructor
  form (`channel<T>(cap)`), so it can never be a plain identifier
- Channels are **non-Copy and move-only**: `clone ch` is a compile error
  (`ChannelNotClonable`); `split` is the **only** way a buffer gains a second handle — two
  tasks share one buffer only through handles the program explicitly forked (T07d)
- Payloads travel by **ownership move** (D3/D6): owned payloads transfer real ownership to
  the receiver; Copy payloads (`int` / `float` / `bool` / `unit` / function values) copy.
  A `ref` payload is rejected outright (`ChannelNotBorrowable`) — a borrow never crosses a
  channel (§10.6)
- An **owned variable** handed to `send` needs an explicit `move x` (or `clone x`); a fresh
  value produced on the spot (`c.send(expr)`) transfers naturally — the same rule as any
  owned function argument (D6; the `OwnedArgRequiresMove` diagnostic names `move` and
  `std.concurrent.split`)
- **No `close`, no `for` over a channel, no `select`** in 0.0.4 (deferred to 0.1.0); no
  shared-memory containers (`Mutex`/`Atomic`) — channels are the only cross-task
  communication (D3). A run ends when no task is runnable; a program whose tasks are all
  blocked on channels is a `ChannelDeadlock` runtime error

### Function Types

```fleen
(int, int) -> int
```

### Type Conversion `as` (temporary in 0.0.2, to be revisited after generics)

```fleen
x = 42 as string;    // "42"
f = 1.5 as string;   // "1.5"
b = true as string;  // "true"
```

- **Precedence**: `as` sits between unary prefix and binary operators (same as Rust):
  `deref b as string` = `(deref b) as string`; `1 + 2 as string` = `1 + (2 as string)`;
  `box 1 as string` = `box (1 as string)`
- **Whitelist (only these three)**: `int as string` (decimal, including the minus sign),
  `float as string` (matching print's float output), `bool as string` (`"true"` / `"false"`).
  Everything else (`string as string`, `int as float`, `box<T> as …`, `ref T` operands, identity conversions)
  → `UnsupportedCast { from, to }`
- **The result is a fresh owned string**: no `move` / `clone` is needed in a consumption position (consistent with "the RHS is a fresh value")
- **Temporary nature**: the 0.0.2 whitelist is only the minimal support needed to fix print; the full
  conversion matrix and a From-like mechanism will be revisited with generics (0.1.0), and 0.0.2 introduces no implicit numeric conversions
- **print accepting only strings is also temporary**: in 0.0.2, print reports `ArgTypeMismatch` for non-string
  arguments with a hint to use `as string`; print's formatting and `as string` share the same helper
  (`fleen-vm/src/fmt.rs`), so the two paths cannot drift apart

---

## 10. Pointers and Ownership

> **Version note**: not in 0.0.1; `box<T>` / `deref` / `ref` / `move` / `clone` were officially
> adopted in **0.0.2** (see `docs/0.0.2/PLAN.md` §3.1–3.3 for decisions and details).
> `ptr` / `addr` / `unsafe` remain unimplemented (0.0.5).

```fleen
box<T>    // heap pointer, safe, single ownership (0.0.2)
ref T     // read-only borrow, function parameters only (0.0.2)
ptr<T>    // raw pointer, unsafe (0.0.5)
addr x    // take the address (0.0.5)
```

### 10.1 Ownership Model

Values fall into three categories (`is_copy(T)` is determined by typeck):

| Category | Types | Assignment/parameter-passing semantics |
|----------|-------|----------------------------------------|
| Copy | `int` `float` `bool` `unit`, function values | Implicit bitwise copy; everything as before |
| Owned | `string` `box<T>`, `Result<T, E>` containing an owned component | Single owner; moving requires `move`, copying requires `clone` |
| Borrow | `ref T` (parameter position only) | Read-only borrow, no transfer |

### 10.2 move / clone

| Expression | Effect |
|------------|--------|
| `move x` | Transfers ownership of `x`; afterwards `x` is no longer usable (`UseAfterMove`) |
| `clone x` | Deep copy; `x` remains usable |
| `clone deref b` | Deep copy of the value inside the box; the box is untouched |
| `clone g` (g is a global) | Deep copy (reading an owned global is by definition a deep copy; see rule 5) |

**Core rules:**

1. Copy types never need `move` / `clone`
2. An owned local variable entering a **consumption position** (top level of a binding/assignment RHS, an actual argument, a `choose` scrutinee)
   must be explicitly `move`-ed / `clone`-ed; writing it bare is a compile error (the help message suggests the missing keyword)
3. **Production positions transfer implicitly**: values at the end of a function, a block, or an if/choose branch flow out through the "exit",
   no keyword needed; a transfer at the end of a branch is conditional, and later use reports "possibly moved"
   > **Block/branch tails in statement position** (e.g. `{ … t };` or `if c { … t } else { … u };`
   > standing alone as a statement) also produce a value, which is then immediately discarded. This is conservative
   > semantics (consistent with Rust's expression statements): the value is produced as usual and the transfer
   > rules are unchanged — nobody receives it, but the non-Copy consumption is not waived merely
   > because of "statement position"
4. Comparison operators and `print` arguments are **read-only uses**: copied automatically, never transferred
5. Forbidden: `move deref b` (cannot move out of a box), `move g` (cannot move out of a global — reading an owned
   global is a deep copy and writing it replaces it), `move s` (where s is a `ref` parameter)
6. Applying `move` / `clone` to a Copy type → compile error, with a hint to remove the keyword

```fleen
s = "hello";
t = move s;        // move; s is no longer usable afterwards
t = clone t;       // deep copy; t remains usable
```

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

### 10.3 box<T>

```fleen
b = box 42;            // b: box<int>, heap-allocated
n = deref b;           // read: a copy (an owned inner value is deep-copied)
deref b = n + 1;       // write: replaces the inner value, the old value is released
c = clone b;           // a new box + a new inner value
s = box "heap";
t = clone deref s;     // deep copy of the inner value; the box itself is untouched
```

- The `box expr` prefix keyword allocates (the type-position `box<T>` and the expression position are distinguished by syntactic position);
  the `deref` prefix keyword reads and writes; the assignment target may be `deref b` (no type annotation allowed)
- Reading `deref b` produces a **copy** of the inner value (`T: Copy` is copied bitwise, owned values are deep-copied) —
  the box owns the inner value, so reading is not the same as taking ownership; the "explicit clone" rule only constrains variable positions
- Allowed inner types (0.0.2): `int` `float` `bool` `string` `box<U>` `Result<T, E>`
- **Cannot move out of a box**: `move deref b` is a compile error; if you need the value, use `clone deref b`
- **`deref` as an assignment target**: in `deref b = v`, `b` must be a **local box variable**;
  writing the inner value of an owned global box is a compile error (0.0.2 has no safe path for mutable aliasing of
  globals, `DerefAssignOfGlobalBox`); you may first `clone` the box into a local and then write it
- **Deterministic release (no GC)**: released immediately when the holder leaves scope, is reassigned, or the value is discarded;
  no reference cycles are possible (a box has a single owner and `ref` cannot be stored) → no GC needed, consistent with §14

### 10.4 ref: Read-Only Borrow (Function Parameters Only)

```fleen
func shout(s: ref string): int {
    print(s);          // read-only use
    42
}

func main(): int {
    name = "fleen";
    shout(name);       // pass a borrow; name remains usable
    shout(name);       // borrow again; legal
    0
}
```

- `ref T` may only appear in the **function parameter type** position; a `ref` local variable, a `ref` global,
  returning a `ref`, or nested `ref ref T` are all compile errors (`RefNotAllowedHere`)
- ref parameters are read-only: assignment (`AssignToRefParam`) and `move` (`MoveOfBorrowed`) are both errors;
  `clone s` yields an owned `T`; a ref parameter can be forwarded as a `ref T` argument (the handle flows along, zero copy)
- Arguments are **not transferred**: locals, globals, and other ref parameters can all be passed without `move`/`clone`;
  borrowing into a box is not supported (`deref b` as a ref argument)
  > The argument must be a **variable** (a local, a global, or another ref parameter). Literals, `clone s`,
  > call results, and other temporaries report `RefArgNotAVariable`: a borrow has no slot to reference,
  > and a borrowed value has nowhere to be found after the call returns
- `ref T` is a distinct type (`ref string ≠ string`): assigning a `ref string` to a `string`
  binding is a type error, with a hint to use `clone s`; passing `move x` to a `ref` parameter is an error (a borrow does not accept a move)
- **No borrow checker needed**: a handle lives only for a single call — the borrowed slot is not overwritten during
  the call (the caller is suspended), and the handle cannot escape (it cannot be assigned/stored/returned) → no dangling is possible

### 10.5 0.0.2 U06 Implementation Notes (lower-side restrictions)

- **Guards on a result `choose`**: arms with `if <guard>` **do not count toward exhaustiveness** — the same rule as
  for a Bool scrutinee: a guard may evaluate false at runtime, so it cannot cover its pattern. Therefore
  `when Ok(v) if g {…}` + `when Err(e) {…}` is rejected by typeck as missing an Ok arm
  (`ChooseNotExhaustive { missing_patterns: ["Ok"] }`). To write a guard, you must pair it with
  another **unguarded duplicate** `Ok` / `Err` arm, or fall back to `otherwise`. Lowering of guard arms and
  `otherwise` is **deferred to U09 / a separate ticket** (U07 decided to keep it deferred; see
  `docs/0.0.2/TICKETS.md` U07 "scope decision"): this requires refactoring Result-choose from
  "an Ok/Err pair of arms with no fallback block" into a general arm chain + no-match fallback, while
  preserving the payload before the guard is evaluated. Until then, the U06/U07 lower only lowers unguarded
  `when Ok(..)` / `when Err(..)` arms and reports `UnsupportedFeature` on encountering a guard or `otherwise`
  (`ChooseResultNeedsOkErrArms` is a defensive check behind the typeck gate).
- **When a ref argument is a global**: the borrow must point to a local slot. When U06 lowers a call **inside a
  function body** that passes a global ref argument, it uses a temporary slot `CloneGlobal; StoreLocal tmp; MakeRefLocal tmp` to receive it;
  a **global initializer** (`lower_global_init`) has no frame that could borrow its slot, so a global-initializer
  call containing ref arguments reports `RefArgInGlobalInit` directly (typeck passes, lower rejects).

### 10.6 The async world is borrow-free (0.0.4, D2/D4/D5)

§10.4's "no borrow checker needed" argument relies on the borrowed slot being untouched for
the duration of a **single synchronous call** — the caller is suspended while the handle is
alive. A concurrent task breaks that premise (nothing suspends the rest of the program while
a task holds a handle), so 0.0.4 keeps the invariant the simple way: **borrows never reach
concurrent code**.

- An `async func` **may not take a `ref` parameter** at all (`RefInAsync`); a `ref` handle
  cannot be captured, stored, or passed across any `await`, task, or channel boundary — the
  entire static `Send` rule of 0.0.4 is this one line (D4)
- An async task **may read** globals (auto deep copy, the §10.2 rules) but **may not write**
  them (`AsyncGlobalWrite`) — a write would be a cross-task shared mutation, which the
  message-passing model excludes (D5); route the value through a channel instead
- Channels follow the same rule: a `ref` payload is rejected (`ChannelNotBorrowable`,
  §9.7) — only owned and Copy values ever cross a task boundary
- Watchword: **async works exclusively on owned and Copy values**. The async world and the
  borrowing world are cleanly separated; `Mutex`/`Atomic`-style shared-memory containers are
  not introduced in 0.0.4 (D3)

---

## 11. Modules and Imports (0.0.3 V05)

```fleen
import net.tcp;          // resolves to src/net/tcp.fln (or src/net/tcp/mod.fln)
import std.io;           // the std library ships in std/, embedded in the compiler
import shylib;           // a `file://` / git dependency crate (flint, V08)

pub func make(): int = net.tcp.n()

func main() {
    std.io.print("hello");   // qualified access: <module>.<item>
    print(net.tcp.n() as string);
}
```

**Rules (as built):**
- `import` performs no repeated header-file parsing; module paths are dot-separated and
  resolve **left to right** through the module table (an `import net.tcp;` also makes `net`
  available so `net.tcp.f()` and, where declared, items on `net` itself are reachable).
- Each non-final path segment must be a module; each final segment is a function / global /
  const / struct. A path reaching a private item in another module → `PrivateAccess`
  (mark the declaration `pub` to expose it).
- A module path with no matching file → `ModuleNotFound` (the message lists the tried
  candidates `<dir>/<path>.fln` and `<dir>/<path>/mod.fln`); two files resolving to one path
  → `DuplicateModule`; a cycle in the import graph → `CircularImport` (the cycle path is
  reported).
- **Cross-crate rules** (V08): a crate may not import a crate that already depends on it —
  either direction across the dependency graph → `CrossCrateImport`; a dependency's source
  tree is read-only to a build and may not reach back into the depending project. Whether
  `<crate>.<module>` is addressable at all is manifest data (`flint.toml`) — flint's job, not
  the compiler's.
- `pub` is a **top-level-only** visibility modifier (on `func` / `const` / `struct` / global
  `var`); `pub` inside a block, or preceding `import`, is a parse error. Struct fields have no
  per-field visibility — a struct is public or it is not.
- Module names are dot-separated lowercase; there are no C FFI imports.
- **std and the prelude (D7)**: the prelude is `print` only (an implicit import, the same
  builtin as `std.io.print`). Everything else in the std library is pulled in by its **leaf**
  module (`std/` has no re-export mechanism in 0.0.3 — there is no `pub use`):
  `std.io` (`print` / `println` / `read_line`), `std.str`, `std.num`,
  `std.arr` (schematic `[T]` signatures), `std.collection.range` (`range` / `range_from`,
  the two `Range` constructors, also reachable bare), `std.collection.iter`
  (`map` / `filter` / `fold` / `take`, V07), `std.os`, `std.time`. `std/` is written in Fleen
  and embedded into the compiler binary; only VM-irreducible primitives are intrinsics.
  std is always compiled and exercised by `cargo test`, so it cannot rot.

### Design decisions settled in 0.0.3 (D1–D10)

The decision points in `docs/0.0.3/PLAN.md` §2 are **settled** (each was finalized in the ticket
that built it and folded back here / into `docs/BYTECODE.md`):

| D | Decision settled | Where |
|---|------------------|-------|
| D1 | struct declaration syntax; `pub` on the whole struct, no per-field visibility | §9.4 (this doc) |
| D2 | field read is a copy (no `move p.f`, `MoveOutOfStruct`); field write one level, local/global receiver | §9.4 + BYTECODE `GetField` (pop form) |
| D3 | `[T]` is owned for every `T`; element reads copy; `a[-1]` out of range | §9.5 + BYTECODE §5.13 |
| D4 | `for` iterates `{ [T], Range }` only; loop var fresh & immutable; iterable evaluated once | §6 + PLAN §5.4 |
| D5 | `Range` is a builtin Copy type via `start..end`, constructed through std (`range`) | §9 (composite types) + BYTECODE §5.14 |
| D6 | module paths → files, pub visibility, module table sub-pass, cycle detection | §11 + PLAN §4.3 |
| D7 | prelude = `print` only; std modules imported by name | §11 |
| D8 | std ships as Fleen sources embedded in the compiler; intrinsics only where the VM must do it | §11 + PLAN §6 |
| D9 | flint: git-only dependencies, `$FLINT_HOME` cache, in-tree CLI | PLAN §3.6 |
| D10 | bytecode v3 keeps a single appended `struct_defs` table; **no bytecode module table** (modules are compile-time) | BYTECODE §9 |

Known Hacks stays empty (§18): no 0.0.3 feature required a special case in `infer.rs` (the
schematic `[T]` std signatures live in the builtins table, and `grep` for std names in `infer.rs`
is empty).

---

## 12. unsafe / trusted

```fleen
unsafe raw_op = () => {
    // unsafe operation
}

trusted safe_wrapper = () => {
    // the author has verified this is safe
}

func normal() {
    safe_wrapper()      // calling trusted is allowed
    // raw_op()         // error: cannot call unsafe
}
```

**Not implemented in 0.0.1 (0.0.5).**

---

## 13. Naming Style

| Category | Style | Example |
|----------|-------|---------|
| Variables/functions | `snake_case` | `some_var` |
| Types/structs | `PascalCase` | `SomeType` |
| Modules | `foo.bar` | `std.io` |

**Function overloading is not supported; operator overloading will be supported (in a later version).**

**Semicolon rules**: 0.0.1 requires explicit semicolons `;`; from 0.0.2 on, ASI is in place and semicolons are optional (explicit semicolons remain legal, see §3.9).

---

## 14. Explicit Rejections

| Feature | Reason |
|---------|--------|
| Garbage collection | Unpredictable latency |
| `panic` catching | Should not be used as control flow |
| `match` / `switch` | Symbol-heavy / fallthrough is counterintuitive |
| `try` / `catch` / `finally` | Exception jumps break linear flow |
| `goto` | Breaks structured programming |
| Global mutable variables | Concurrent data races |
| OOP | Rejects `class` and inheritance |
| `let` / `var` / `:=` | Bindings uniformly use `=` |
| **C FFI imports** | Keeps the language pure; no external unsafety is introduced |

---

## 15. Complete Example

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0;
    const limit = 10;

    while x < limit {
        print(fib(x));
        x = x + 1;
    };

    0
}
```

---

## 16. Design Rationale

### 16.1 Why binding and assignment are the same operation

**A person's intent is simply "make x be 42" and "make x be 43".**

Users do not need to distinguish "definition" from "assignment". The compiler knows the difference, but users never perceive it.

This follows the **intuitive and friendly** principle.

### 16.2 Why mutable by default

Python style: a variable is "a reference from a name to a value"; the name itself has no "immutable" property.

Mutable is the default; `const` is the explicit marker.

This follows the **concise and clear** principle.

### 16.3 Why shadowing must preserve mutability

**To avoid ambiguity.**

```fleen
x = 42;
if cond {
    const x = 43;    // the inner one is immutable, the outer one is mutable
};
x = 44;              // which one does this change?
```

If this were allowed, the reader would have to ask: is the inner `const x` a "new variable", or does it "turn the outer `x` immutable"?

**There is no clear answer. So it is not allowed.**

This follows the **memory safe** and **intuitive and friendly** principles.

### 16.4 Why functions do not participate

**A function body is an independent scope, not an "inner scope".**

Variables inside a function are unrelated to outer variables. A function should not be affected by the mutability of outer variables.

This follows the **concise and clear** principle.

---

## 17. Feature Matrix

| Feature | 0.0.1 | 0.0.2 | 0.0.3 | 0.0.4 |
|---------|-------|-------|-------|-------|
| `=` binding/assignment | ✅ | ✅ | ✅ | ✅ |
| `const` immutable | ✅ | ✅ | ✅ | ✅ |
| `int` / `bool` / `string` / `float` | ✅ | ✅ (string switches to owned semantics) | ✅ | ✅ |
| `func` single-line / multi-line | ✅ | ✅ | ✅ | ✅ |
| `if` / `elif` / `else` | ✅ | ✅ | ✅ | ✅ |
| `while` | ✅ | ✅ | ✅ | ✅ |
| `choose` | ✅ | ✅ (+ `Ok`/`Err` patterns) | ✅ | ✅ |
| `Result` | ✅ (type only) | ✅ (+ `Ok`/`Err` constructors, `?`) | ✅ | ✅ |
| Function types | ✅ | ✅ | ✅ | ✅ |
| Semicolon `;` | ✅ required | ✅ optional (ASI) | ✅ | ✅ |
| `?` | ❌ | ✅ | ✅ | ✅ |
| `move` / `clone` | ❌ | ✅ | ✅ | ✅ |
| `box<T>` | ❌ | ✅ | ✅ | ✅ |
| `ref` | ❌ | ✅ (parameters only) | ✅ | ✅ (async is borrow-free, §10.6) |
| `struct` | ❌ | ❌ | ✅ (V02) | ✅ |
| `[T]` arrays | ❌ | ❌ | ✅ (V03) | ✅ |
| `Range` / `for` | ❌ | ❌ | ✅ (V04) | ✅ |
| Modules (`import` / `pub`) | ❌ | ❌ | ✅ (V05) | ✅ |
| `std` library (modules + intrinsics) | ❌ | ❌ | ✅ (V06) | ✅ (+ `async` / `concurrent` modules, §6/§9.7) |
| std iterator helpers (`std.collection.iter`) | ❌ | ❌ | ✅ (V07, eager) | ✅ |
| `flint` package manager | ❌ | ❌ | ✅ (V08) | ✅ |
| `async` / `await` / `Future<T>` | ❌ | ❌ | ❌ | ✅ (T02/T05, §6/§9.6) |
| Cooperative executor (`std.async.run` / `yield_now` / `std.io.sleep`) | ❌ | ❌ | ❌ | ✅ (T06) |
| Channels (`channel<T>` + `std.concurrent.split`) | ❌ | ❌ | ❌ | ✅ (T07, §9.7) |
| FFI | ❌ | ❌ | ❌ | ❌ |
| Generics | ❌ | ❌ | ❌ (0.1.0) | ❌ (0.1.0) |
| Operator overloading | ❌ | ❌ | ❌ (0.1.0) | ❌ (0.1.0) |
| Pointers `ptr` / `addr` / `unsafe` | ❌ | ❌ | ❌ (0.0.5) | ❌ (0.0.5) |

**0.0.1 goal: Fibonacci runs.**

**0.0.2 goal: ownership foundations — see `docs/0.0.2/PLAN.md` for the plan.**

**0.0.3 goal: aggregation and organization — `struct`, `[T]`, `for` + `Range` (implemented, V01–V04),
the module system (implemented, V05), the `std` library (implemented, V06), its iterator helpers
(implemented, V07), and the `flint` package manager (implemented, V08), integrated and gated by
V09, with docs and version synced in V10
— see `docs/0.0.3/PLAN.md` and `docs/0.0.3/TICKETS.md`.**

**0.0.4 goal: cooperative concurrency — `async` / `await` over `Future<T>` coroutine frames,
the single-threaded cooperative executor, and message-passing `channel<T>` with the
borrow-free boundary (T01–T09 implemented; see `docs/0.0.4/PLAN.md` decisions D1–D6 and
`docs/0.0.4/TICKETS.md`).**

---

## 18. Roadmap

| Version | Contents |
|---------|----------|
| 0.0.2 | `move` / `clone` / `box<T>` / `ref` / `?` / ASI / `as` — **implemented; see `docs/0.0.2/PLAN.md` for planning and implementation records** |
| 0.0.3 | `struct` / `[T]` / `for` + `Range` — **implemented (V01–V04)**; modules / `pub` — **implemented (V05)**; std library — **implemented (V06)**; iterator helpers — **implemented (V07)**; flint — **implemented (V08)**; integrated & gated (V09); docs & version synced (V10). 0.0.3 is **complete** |
| 0.0.4 | `async` / `await` + `Future<T>` coroutine frames — **implemented (T01–T05)**; cooperative executor (`std.async.run` / `yield_now` / `std.io.sleep`) — **implemented (T06)**; channels (`channel<T>` / `std.concurrent.split`, message passing D3) — **implemented (T07)**; compatibility & test matrix (T08–T09) and docs & version sync (T10). See `docs/0.0.4/PLAN.md` |
| 0.0.5 | `unsafe` / `trusted` / `ptr` / `addr` |
| 0.1.0 | Generics / operator overloading |

### Known Hacks

None. (The 0.0.1 `print` special case was removed in 0.0.2 U04/U13: `print` now has a real signature
`(string...) -> unit`; the builtin signature table is in `fleen-compiler/src/typeck/builtins.rs`,
and non-string arguments report `ArgTypeMismatch` with a hint to use `as string`.)

---

## 19. Creed

> Trust the programmer,
> but do not test the programmer's memory.
> One fewer furrowed-brow symbol,
> one more measure of intuitive simplicity.
> Do not strive for omnipotence,
> but strive for:
> expressing the safest semantics
> with the most concise syntax,
> generating the most efficient code.
> Fleen is not a simplified version of any language;
> Fleen is Fleen,
> a language born for the future.
