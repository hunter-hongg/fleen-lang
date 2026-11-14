# 12 — Modules

## 12.1 Files and modules

Every `.fln` file is a module. A project is a tree of modules under a source
directory (by convention `src/`):

```text
myproj/
└── src/
    ├── main.fln        // the entry module (contains func main)
    ├── auth.fln
    └── net/
        └── tcp.fln
```

## 12.2 `import`

An `import` declaration brings another module's exported items into scope as
qualified names:

```fleen
// main.fln
import auth;
import net.tcp;

func main() {
    token = auth.make_token();
    n = net.tcp.connect();
}
```

- The path is dot-separated and resolves to a file: `auth` → `src/auth.fln`
  (or `src/auth/mod.fln`); `net.tcp` → `src/net/tcp.fln` (or
  `src/net/tcp/mod.fln`).
- Importing `net.tcp` also makes `net` itself addressable, so `net.tcp.x`
  works, and items declared on the `net` module are reachable as `net.<item>`.
- Std modules are imported the same way: `import std.io;` (chapter 13).
- Reserved words are legal as path segments: `import std.async;`,
  `import std.concurrent;`.

## 12.3 `pub` — exporting

A declaration is **private by default**: only its own module can use it.
Prefixing a top-level declaration with `pub` makes it importable:

```fleen
// auth.fln
pub func make_token(): string = "t-1"

func helper(): int = 1          // private: other modules cannot call helper
```

Rules:

- `pub` may appear on a `func`, a `const` or global binding, or a `struct`
  declaration.
- `pub` is only valid at file top level; `pub` inside a block, or in front of
  an `import`, is a syntax error.
- A struct's visibility covers all its fields: there are no per-field access
  modifiers.
- An `async func` exports like any function: `pub async func f(...)`.

## 12.4 Accessing items across modules

Items are reached with the qualified path `module.item`:

```fleen
import net.tcp;

r = net.tcp.connect();
```

- Each non-final segment of a path must be a module; the final segment is a
  function, global, `const`, or struct.
- Reaching a private item in another module is a compile error
  (`PrivateAccess`); mark the declaration `pub`.
- Assigning to another module's **global** uses the qualified form
  `m.g = v` (subject to `pub` and to the async-write rules of chapter 10
  §10.7).

## 12.5 Resolution errors

| Situation | Error |
|-----------|-------|
| No file matches the path | `ModuleNotFound` (the message lists the tried candidates) |
| Two files resolve to the same module path | `DuplicateModule` |
| A cycle in the import graph | `CircularImport` (the cycle is reported) |
| A private item reached from another module | `PrivateAccess` |

## 12.6 Example: a three-module program

```fleen
// src/mathutil.fln
pub func square(n: int): int = n * n

func internal(): int = 99      // not visible outside
```

```fleen
// src/main.fln
import mathutil;
import std.io;

func main() {
    std.io.println(mathutil.square(7) as string);   // 49
    // mathutil.internal()                          // error: PrivateAccess
}
```

```text
$ cargo fln --project myproj
49
```
