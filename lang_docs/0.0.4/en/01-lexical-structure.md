# 01 — Lexical Structure

## 1.1 Comments

```fleen
// single-line comment: everything after // on this line is ignored

/* multi-line
   comment */
```

## 1.2 Identifiers

| Category | Style | Example |
|----------|-------|---------|
| Variables, functions | `snake_case` | `some_var`, `read_line` |
| Types, structs | `PascalCase` | `Point`, `SomeType` |
| Modules | lowercase, dot-separated | `std.io` |

```fleen
user_name = "ada";
total_count = 3;
```

An identifier is a sequence of letters, digits, and underscores that does not
start with a digit, and that is not one of the reserved words (§1.3).

## 1.3 Reserved words

The following words cannot be used as identifiers:

```
func  const  struct  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
box  ref  deref  move  clone  as  true  false
async  await  channel
```

Notes:

- `return` is reserved but there is no `return` statement in the language.
- `unsafe` and `trusted` are reserved for a future version.
- `Future` is **not** reserved: `Future<T>` is recognized only in type
  position (chapter 04 §4.9), and a plain `Future` identifier can still name
  a user-defined struct.
- Reserved words are legal as module path segments and member names after an
  `import`, for example `import std.async;`.

## 1.4 Literals

### Integer literal

An integer literal is a sequence of digits, optionally preceded by `-`:

```fleen
0
42
007
```

The `int` type is a 64-bit signed integer. Negative integers are written with
unary minus (chapter 05 §5.4), e.g. `0 - 1` or `-1`.

### Float literal

A float literal has a fractional part:

```fleen
3.14
0.5
2.0
```

The `float` type is a 64-bit float.

### Boolean literals

```fleen
true
false
```

### String literal

A string literal is text between double quotes:

```fleen
"hello"
"a b c"
""
```

Strings are UTF-8. String values are **owned** (chapter 08).

### Array literal

```fleen
[1, 2, 3]
["a", "b"]
[]
```

An empty array literal requires a type annotation, because the element type
cannot be inferred:

```fleen
empty: [int] = [];
```

### Struct literal

```fleen
Point { x: 1, y: 2 }
```

All fields must be initialized. See chapter 04 §4.7.

## 1.5 Source files

- Source files use the extension `.fln`.
- A file contains an optional sequence of `import` declarations (chapter 12),
  `struct` declarations (chapter 04 §4.7), global bindings (chapter 03),
  `const` bindings, and function declarations (chapter 07).
- One of the functions may be named `main`; the program starts there.

```fleen
// program.fln
import std.io;

greeting = "hello";             // global binding

func main() {
    std.io.println(greeting);
}
```
