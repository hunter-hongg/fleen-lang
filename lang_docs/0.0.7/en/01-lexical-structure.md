# 01 — Lexical Structure

## 1.1 Comments

```fleen
// Single-line comment: text after // on the line is ignored

/* Multi-line
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

An identifier is a sequence of letters, digits, and underscores that does not start with a digit and is not a reserved word (§1.3).

## 1.3 Reserved words

The following words cannot be used as identifiers:

```
func  const  struct  enum  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
box  ref  deref  move  clone  as  true  false
async  await  channel
ptr  addr  deref_ptr
```

Notes:

- `return` is reserved but the language has no `return` statement.
- `unsafe` and `trusted` are reserved since 0.0.5 (chapter 16).
- `Future` is **not** a reserved word: only `Future<T>` in type position is recognized (chapter 04 §4.9); a bare `Future` identifier can still name a user-defined struct.
- `enum` is a reserved word since 0.0.6 (chapter 04 §4.8).
- `ptr` is **not** a reserved word: only `ptr<T>` in type position is recognized (chapter 16); `addr` and `deref_ptr` are builtin operations (chapter 16).
- Reserved words may be used as module path segments and member names, e.g. `import std.async;`.

## 1.4 Literals

### Integer literals

An integer literal is a sequence of digits, optionally preceded by `-`:

```fleen
0
42
007
```

`int` is a 64-bit signed integer. Negative integers are written with unary minus (chapter 05 §5.4), e.g. `0 - 1` or `-1`.

### Float literals

A float literal has a fractional part:

```fleen
3.14
0.5
2.0
```

`float` is a 64-bit floating-point number.

### Boolean literals

```fleen
true
false
```

### String literals

A string literal is text between double quotes:

```fleen
"hello"
"a b c"
""
```

Strings are UTF-8 encoded. String values are **owned** (chapter 08).

### Array literals

```fleen
[1, 2, 3]
["a", "b"]
[]
```

An empty array literal must carry a type annotation because the element type cannot be inferred:

```fleen
empty: [int] = [];
```

### Struct literals

```fleen
Point { x: 1, y: 2 }
```

All fields must be initialized. See chapter 04 §4.7.

## 1.5 Source files

- Source files use the `.fln` extension.
- A file consists of optional `import` declarations (chapter 12), `struct` declarations (chapter 04 §4.7), global bindings (chapter 03), `const` bindings, and function declarations (chapter 07).
- It may contain a function named `main`; execution starts there.

```fleen
// program.fln
import std.io;

greeting = "hello";             // global binding

func main() {
    std.io.println(greeting);
}
```
