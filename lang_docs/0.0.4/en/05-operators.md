# 05 — Operators

## 5.1 Precedence

From lowest precedence (binds loosest) to highest (binds tightest):

```
or
and
== !=   < > <= >=
+ - * / %
as
?                                            (0.0.2; postfix)
box deref move clone  -  !                   (unary prefixes)
() [i] .f  await  ?                          (postfix suffixes)
```

Postfix suffixes — call `()`, index `[i]`, field access `.f`, `await`, and
`?` — bind tightest and chain left-to-right. Parentheses override everything.

```fleen
1 + 2 * 3          // 7
(1 + 2) * 3        // 9
1 + 2 as string    // parsed as 1 + (2 as string): int + string — a type error
```

## 5.2 Arithmetic operators

`+ - * / %` work on `int` and on `float`. Both operands must have the same
type; there is no implicit conversion between `int` and `float`.

```fleen
a = 7 + 2;        // 9
b = 7 - 2;        // 5
c = 7 * 2;        // 14
d = 7 / 2;        // 3   (int division truncates)
e = 7 % 2;        // 1   (remainder)

f = 7.0 / 2.0;    // 3.5
```

There is no `+` for strings; use `std.str.concat` (chapter 13).

## 5.3 Comparison operators

`== != < > <= >=` produce a `bool`. They work on `int`, `float`, `bool`, and
`string`; both operands must have the same type.

```fleen
1 == 1            // true
1 != 2            // true
2 < 3             // true
3.0 >= 3.0        // true
"a" < "b"         // true (lexicographic)
```

Comparison operators are read-only uses: they never transfer ownership of an
owned operand (chapter 08).

Structs, arrays, and `Result` values do not support `==`.

## 5.4 Logical operators and negation

`and` / `or` combine `bool` values; `!` negates a `bool`.

```fleen
t = true and false;     // false
u = true or false;      // true
v = !true;              // false
w = 1 < 2 and 2 < 3;    // true
```

Unary `-` negates a number:

```fleen
n = -5;
m = -(3 + 4);           // -7
```

## 5.5 `as` — type conversion

`e as string` converts a scalar to its string form. The supported
conversions are exactly:

| Expression | Result |
|------------|--------|
| `int as string` | decimal digits, including a leading `-` for negatives |
| `float as string` | the same form `print` uses |
| `bool as string` | `"true"` or `"false"` |

```fleen
x = 42 as string;         // "42"
y = (0 - 7) as string;    // "-7"
z = 1.5 as string;        // "1.5"
w = true as string;       // "true"
```

`print` and `println` accept only strings; use `as string` to print other
scalar values:

```fleen
n = 42;
print(n as string);
```

Any other conversion (`string as string`, `int as float`, casts of `box`,
`ref`, arrays, structs, identity casts) is a compile error
(`UnsupportedCast`). There are no implicit numeric conversions: `int` and
`float` never mix.

`as` binds tighter than the binary operators and looser than the unary
prefixes:

```fleen
deref b as string       // (deref b) as string
box 1 as string         // box (1 as string)
1 + 2 as string         // 1 + (2 as string)
```

The result of `as string` is a fresh owned string; it needs no `move`/`clone`
when bound or passed.

## 5.6 Unary prefix operators

| Operator | Operand | Effect |
|----------|---------|--------|
| `-` | `int` / `float` | arithmetic negation |
| `!` | `bool` | logical negation |
| `box` | any allowed inner type | heap-allocate a `box<T>` |
| `deref` | `box<T>` | read the inner value (or be the target of an assignment) |
| `move` | an owned local | transfer ownership (chapter 08 §8.2) |
| `clone` | an owned value | deep copy (chapter 08 §8.3) |

```fleen
b = box 42;
n = deref b;          // 42
s = box "heap";
t = clone deref s;    // an owned copy of the inner string; the box is untouched
```

## 5.7 Postfix operators

Postfix suffixes bind tightest and chain left-to-right:

| Suffix | Applies to | Effect |
|--------|------------|--------|
| `(args)` | a function value | call |
| `[i]` | an array | element read (element write is a statement form) |
| `.f` | a struct | field read |
| `await` | a `Future<T>` | drive to completion, yield `T` (chapter 10) |
| `?` | a `Result<T, E>` | unwrap, or propagate the error (chapter 09) |

```fleen
add(1, 2)              // call
a[0]                   // index read
p.x                    // field read
half(n) await?         // ((half(n) await)?): await then ?
f(1)[2].x              // chains left-to-right: call, index, field
```

## 5.8 Assignment is not an operator with a value

`=` in expression position assigns to an existing variable and yields
`unit`; it never creates a binding (chapter 03 §3.4):

```fleen
b = 5
a = b = 3        // b becomes 3; a is bound to unit
```

There are no compound assignment operators (`+=`, `++`, …). Write
`x = x + 1`.
