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
break  continue
box  ref  deref  move  clone  as  true  false
async  await  channel
ptr  addr  deref_ptr
```

Notes:

- `return` is reserved but the language has no `return` statement.
- `break` and `continue` are reserved since 0.0.9; they are loop statements (chapter 06 §6.5) — programs using them as identifiers must rename.
- `unsafe` and `trusted` are reserved since 0.0.5 (chapter 16).
- `Future` is **not** a reserved word: only `Future<T>` in type position is recognized (chapter 04 §4.10); a bare `Future` identifier can still name a user-defined struct.
- `enum` is a reserved word since 0.0.6 (chapter 04 §4.6).
- `ptr` is **not** a reserved word: only `ptr<T>` in type position is recognized (chapter 16); `addr` and `deref_ptr` are builtin operations (chapter 16).
- `feature` is **not** a reserved word: `#feature curry` is the 0.0.8 header directive, recognized by the `#` in header position; `feature` itself remains a valid identifier (chapter 07 §7.1).
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

Strings are UTF-8 encoded. String values are **owned** (chapter 08). A string
literal is a single-line construct: a raw newline inside one is an
unterminated-string error.

#### Escapes

| Escape | Produces |
|--------|----------|
| `\n` | newline |
| `\t` | tab |
| `\r` | carriage return |
| `\\` | backslash |
| `\"` | double quote |
| `\'` | single quote |
| `\$` | dollar sign (new in 0.0.10) |

Any other `\` sequence is a compile error (`InvalidEscape`). Before 0.0.10 a
`\$` was one of those errors; the escape was added together with string
interpolation (below).

#### String interpolation (0.0.10)

A string literal containing `${` is an **interpolating string**: literal chunks
interleaved with embedded expressions. `${expr}` embeds a full expression:

```fleen
func double(n: int): int = n * 2     // a function to call from ${…} below

n = 42;
name = "world";
print("n=${n}!");                          // n=42!
print("row ${n + 8} of ${n}");             // full expressions
print("hello ${name}, len=${std.str.len(name)}");   // calls work too
print("nested: ${"inner=${n}"}");          // nested interpolating strings
print("d=${double(21)}");                  // embedded function calls
```

Rules:

- `${expr}` embeds a **full expression** — calls, field access, arithmetic,
  nested string literals — parsed with the ordinary expression grammar.
  Evaluation is left-to-right, each part evaluated exactly once, so
  side-effecting embedded expressions run in source order.
- **Conversion**: `int`, `float`, `bool` stringify via exactly the `as string`
  conversions (chapter 05 §5.5); `string` embeds verbatim. Any other type
  (`enum`, `struct`, `[T]`, `box<T>`, `Result<T, E>`, `Future<T>`,
  `channel<T>`, `ptr<T>`, function values) is a compile error
  (`InterpUnsupportedType`, with a hint naming the explicit conversion).
  There are **no format specifiers**: `${x:04}` is a parse error.
- **Read context**: an embedded `string` is deep-copied — interpolation never
  moves a variable, and the binding stays usable afterwards (chapter 08).
- **Escaping**: `$` is special **only immediately before `{`**. In every other
  position it is an ordinary character: `"a$b"`, `"100$"`, `"$ { spaced }"`
  are unchanged from 0.0.9. To put a literal `$` directly before a `{`,
  escape the dollar — `"a\${b}c"` is the literal `a${b}c` (the escape consumes
  the `$`, the `{` stays ordinary).
- **Lowering**: an interpolating string desugars to a chain of
  `std.str.concat` calls over the merged literal chunks — no new opcodes. A
  literal **without** `${` compiles byte-identically to 0.0.9.
- **Globals**: a global initializer cannot interpolate yet — `const s = "x${i}";`
  is rejected (`ComplexGlobalInit`). Interpolate in a function body instead.
- Errors: EOF inside `${…}` is `UnclosedInterpolation`; a stray `\` is
  `InvalidEscape`.

**Breaking change (the 0.0.10 breaking set)**: a string literal containing the
exact sequence `${` previously meant the two literal characters `$` `{`; it now
begins an interpolation. Write `\${` where the old source had `${` to keep the
literal text.

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

All fields must be initialized. See chapter 04 §4.5.

## 1.5 Source files

- Source files use the `.fln` extension.
- **Header directive region (0.0.8)**: before the file's first declaration there may be zero or more `#feature` directives, each on its own line. `#` is not a comment — `#feature` is the only `#` form. Comments and blank lines do not end the header region; the first declaration (`import`, `struct`, binding, or function) ends it, and any `#` after that is `FeatureDirectiveMisplaced`. Repeating a directive is `DuplicateFeature`, an unknown feature name is `UnknownFeature`. Feature flags are **per file**, not per project (chapter 07 §7.1, chapter 12).
- A file consists of optional `import` declarations (chapter 12), `struct` declarations (chapter 04 §4.5), global bindings (chapter 03), `const` bindings, and function declarations (chapter 07).
- It may contain a function named `main`; execution starts there.

```fleen
// program.fln
#feature curry        // 0.0.8: per-file opt-in for currying (see chapter 07)

import std.io;
import std.str;

greeting = "hello";             // global binding

greet: string -> string
greet(name) = std.str.concat("Hello, ", name)   // Fleen has no string `+`

main: () -> unit
main() {
    std.io.println(greet(greeting));
}
```
