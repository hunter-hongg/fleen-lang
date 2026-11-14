# 03 — Variables and Assignment

## 3.1 Binding

A binding introduces a name with a value, using `=`:

```fleen
x = 42
name = "ada"
```

Bindings are **mutable by default**: the name can be reassigned later with
`=`.

A binding may carry a type annotation on the name:

```fleen
x: int = 42
name: string = "ada"
empty: [int] = []
```

A binding requires a value; `x;` alone is not a binding.

## 3.2 `const` bindings

Prefixing the name with `const` makes the binding immutable:

```fleen
const y = 42
const limit: int = 10
```

Reassigning a `const` is a compile error:

```fleen
const y = 42
y = 43        // error: cannot assign to immutable variable
```

## 3.3 Assignment

When the name already exists in the current scope and is mutable, `=` is an
assignment:

```fleen
x = 42
x = 43        // assignment
x = x + 1     // assignment using the current value
```

- Assigning to a `const` is a compile error (§3.2).
- An assignment cannot carry a type annotation — `x: int = 43` on an existing
  `x` is a compile error; the annotation is only allowed on the first
  binding.

## 3.4 How `=` is interpreted

`=` has exactly one syntax. Its meaning — a new binding or an assignment —
is decided by position and by what is already bound:

| Position | The name is… | Meaning |
|----------|--------------|---------|
| Statement position | not bound in the current scope | new binding |
| Statement position | bound in the current scope | assignment |
| Statement position, inside a loop body | a mutable binding visible in the function before this point | **assignment to that outer variable** (§3.6) |
| Expression position | bound in the current scope | assignment |
| Expression position | not bound | compile error — `=` in expression position never creates a binding |

```fleen
x = 42        // binding
x = 43        // assignment
```

In expression position, `=` only assigns (and yields `unit`, not the
assigned value):

```fleen
b = 5
a = b = 3     // a = (b = 3): b is assigned 3, and a is bound to unit
```

```fleen
a = b = 3     // error: b is not bound, and expression position never binds
```

The target lookup for an expression-position assignment does not cross
function boundaries:

```fleen
x = 1
func f() {
    y = (x = 2)     // error: x is not visible inside f
}
```

## 3.5 Scopes and shadowing

Every `{}` block is a new scope. A binding created inside a block is not
visible after the block:

```fleen
if cond {
    x = 42
};
print(x)      // error: x is not in scope here
```

A function body is an independent scope, and function boundaries are
impenetrable: bindings inside a function never interact with same-named
variables outside it.

An inner scope may rebind a name that exists outside it (**shadowing**).
Shadowing must keep the same mutability:

| Outer binding | Inner binding | Allowed |
|---------------|---------------|---------|
| mutable | mutable | yes |
| `const` | `const` | yes |
| mutable | `const` | no |
| `const` | mutable | no |

```fleen
x = 42
if cond {
    x = 43         // new binding, shadows the outer x
};
print(x)           // 42 — the outer x was not modified
```

```fleen
const x = 42
if cond {
    const x = 43   // legal: const shadows const
};
```

```fleen
x = 42
if cond {
    const x = 43   // error: mutability mismatch
};
```

A function parameter may be shadowed inside its own function body, without a
mutability check:

```fleen
func f(x: int): int {
    x = 2          // legal: shadows the parameter
    x
}
```

A user binding that has the same name as a builtin function (for example
`print`) overrides it; the name then refers to the user binding.

## 3.6 Assignment inside loop bodies

The body of a `while` or `for` loop is a loop scope. When `x = expr` appears
inside a loop body (including its nested blocks):

- If `x` is already bound in the loop body itself → assignment.
- Otherwise, if a **mutable** `x` is visible in the enclosing function before
  this point → **assignment to that outer variable**.
- Otherwise → a new binding.

This makes the counter idiom assign to the outer `x`:

```fleen
x = 0
while x < 10 {
    x = x + 1      // assignment to the outer x
};
print(x as string) // 10
```

If the outer binding is immutable, the in-loop assignment is an error:

```fleen
const x = 0
while x < 10 {
    x = 1          // error: cannot assign to a const
};
```

Loop-scope lookup does not cross function boundaries. Inside a function, a
loop-body statement `x = 1` where no local `x` exists is a **new binding**,
not a write to a same-named global:

```fleen
x = 0              // global
func f(): int {
    n = 0          // local
    while n < 3 {
        x = 5      // new binding inside the loop body; the global x is unchanged
        n = n + 1
    };
    x              // returns the local x = 5
}
```

Shadowing inside a loop body is still possible, but only in nested blocks of
the body:

```fleen
x = 0
while x < 3 {
    x = x + 1
    if true {
        x = 100    // new binding, shadows the outer x, inside a nested block
    }
};
print(x as string) // 3
```

## 3.7 Globals

A binding at file top level is a **global**: visible in every function of the
module after its declaration, and importable by other modules (chapter 12).

```fleen
// config.fln
version = 3;

func get_version(): int = version
```

Rules:

- Functions can **read** globals; reading an owned global produces a deep
  copy into the function.
- Functions cannot **assign** to a global (§3.6: statement-position `=` in a
  function is always local). The only writes that can reach a global are a
  field write (`g.field = v`), an index write (`g[i] = v`) on a global
  struct/array variable, and a module-qualified write (`m.g = v` across
  modules, subject to `pub`).
- Writing `deref g = v` where `g` is a global `box` is rejected; clone the
  box into a local first.
- Inside an `async func` body, globals can be read but not written (chapter
  10 §10.7).
