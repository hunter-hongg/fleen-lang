# 14 — Projects and flint

## 14.1 Project layout

A multi-file program is a project: a manifest plus a source tree.

```text
myproj/
├── flint.toml          # the manifest
├── src/
│   ├── main.fln        # the entry module (contains func main)
│   └── util.fln
└── tests/              # optional: test cases (§14.5)
    ├── case1.fln
    └── case1.expected
```

## 14.2 `flint.toml`

```toml
[package]
name = "myproj"
version = "0.1.0"

[dependencies]
# dependency entries go here
```

- `name` is the module root of the project and must be a valid identifier
  (no hyphens).
- Dependencies are git repositories; a resolved build is recorded in
  `Flint.lock`. Fetched dependencies are cached under `$FLINT_HOME`.

## 14.3 The flint commands

| Command | Effect |
|---------|--------|
| `flint new <name>` | scaffold a new project |
| `flint build` | compile the project tree |
| `flint run` | compile and execute |
| `flint test` | run the `tests/` cases (§14.5) |
| `flint check` | typecheck without executing |

The one-stop driver handles single files and projects too:

```bash
cargo fln hello.fln                  # one file: compile, verify, execute
cargo fln --project myproj           # a project tree
```

## 14.4 Dependencies

Dependencies are declared in `flint.toml` and fetched from git. A dependency
is imported by its crate name:

```fleen
import shylib;              // a dependency crate
r = shylib.helper();
```

- A project may not import a crate that already depends on it (dependency
  cycles are rejected).
- A dependency's source tree is read-only to the build.

## 14.5 Tests

A test case is a `.fln` file in `tests/` paired with a `<name>.expected`
file. `flint test` runs each case and compares standard output line by line
with the expected file.

```fleen
// tests/greet.fln
import std.io;

func main() {
    std.io.println("hello test");
}
```

```text
# tests/greet.expected
hello test
```

```bash
$ flint test
greet ... ok
```

A test program is an ordinary Fleen program: it may import the project's
modules and use the full language, including the executor and channels.

## 14.6 Example: a complete project

```toml
# flint.toml
[package]
name = "adder"
version = "0.1.0"

[dependencies]
```

```fleen
// src/main.fln
import std.io;

func add(a: int, b: int): int = a + b

func main() {
    std.io.println(add(2, 3) as string);
}
```

```bash
$ cd adder
$ flint run
5
```
