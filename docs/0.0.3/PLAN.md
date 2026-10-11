# Fleen 0.0.3 Feature Plan (Aggregation and Organization)

> **Fast, simple, safe, intuitive**
> **Theme**: Values get shape — `struct` and `[T]` aggregate data; `for` and iterators give loops
> their natural form; modules give code a home; `flint` gives projects a manager.
> **Basis**: The roadmap in `DESIGN.md §18`, `BYTECODE.md` (§3.2 value representation, §4.2 opcode
> allocation, §10 versioning rules), the reserved grammar in `SYNTAX.ebnf` (`import_decl`,
> `struct_literal`, `array_type`, field access), and the planning conversation of 2026-10-11
> (four-item scope: struct/arrays, for+iterators, module system, flint + std).
> **Status**: Draft (2026-10-11). All decision points in §2 are **recommendations pending review**;
> spec documents are synced per §11 only after the plan is finalized; implementation starts once
> the work is broken down into TICKETS. Until it lands, the current toolchain remains 0.0.2 behavior.

---

## 1. Overview

### 1.1 The four questions 0.0.3 answers

1. **How do values aggregate?** — `struct` lets users define their own types; `[T]` gives
   sequences a home. Both slot into the ownership model of 0.0.2 unchanged (owned types with
   `move` / `clone`).
2. **How does iteration work without traits?** — `for x in iterable { … }` lands, iterating the
   **builtin iterables** (arrays and ranges); std ships eager higher-order helpers
   (`map` / `filter` / `fold`) as plain functions. Polymorphic iterator *protocols* wait for
   traits (0.1.0).
3. **Where does code live?** — the module system activates the `import` grammar that has been
   reserved since 0.0.1: one file = one module, dot paths map to file paths, `pub` controls
   visibility, and programs stop being single files.
4. **How do projects grow beyond one directory?** — `flint`, the package manager: a manifest, a
   lockfile, git-based dependencies, and the `new` / `build` / `run` / `test` / `check` commands.
   The standard library `std` ships alongside as ordinary Fleen source.

### 1.2 Feature list

| # | Feature | Category | Prerequisites it depends on |
|---|------|------|----------------|
| F1 | `struct` (declaration, literals, field access) | Language + compiler + VM | Ownership (0.0.2), bytecode v3 struct instructions |
| F2 | `[T]` arrays (literals, index read/write, `len`) | Language + compiler + VM | Ownership, bytecode v3 array instructions |
| F3 | `for` loop + `Range` | Language + compiler + VM | F2 (arrays are the primary iterable), `Range` builtin type |
| F4 | Iterator helpers (`map` / `filter` / `fold` / …) | `std` only (Fleen source) | F2, F3, function types (0.0.1) |
| F5 | Module system (`import` + file-as-module + `pub`) | Compiler (multi-file) | None at the language level; **span gains file identity** (§3.4.5) |
| F6 | `std` library (prelude + io/str/num/arr/collection/os/time) | New `std/` Fleen source + builtins table | F1–F5 |
| F7 | `flint` package manager CLI | New `flint-cli` crate | F5 (dependencies are multi-module projects), F6 |

> **Scope note**: the four items were agreed as one release (2026-10-11), with an internal
> two-stage order preserved by the milestones (§9): aggregation types first (F1–F3), then
> modules (F5), then std (F6), then flint (F7). F4 is deliberately **not** a language feature —
> without traits there is no iterator protocol; `map`/`filter`/`fold` are eager library
> functions returning arrays.

### 1.3 Debt left over from 0.0.2

None carried as blockers. The items 0.0.2 explicitly deferred (error-type conversion `E ≠ E2`,
the full `as` conversion matrix, polymorphic `print`) are 0.1.0 scope by decision (0.0.2 PLAN
§6, D6), not debt. 0.0.3 adds no new HACKs of the 0.0.1-`print` kind: every std function gets a
real signature in the builtins table from day one.

### 1.4 Explicitly out of scope

| Not doing | Reason |
|------|------|
| Closures / function literals | No syntax exists; `move` captures are a 0.1.0 topic (0.0.2 PLAN §1.4 unchanged) |
| Traits / generics / operator overloading | 0.1.0; this is exactly why iterator *protocols*, `for` over user types, and struct `==` are excluded here |
| `for` over user-defined iterables | Requires a trait; 0.0.3 iterates arrays and ranges only (D4) |
| Method-call syntax `x.foo()` | No receiver machinery; std stays free functions `std.arr.len(a)` |
| `break` / `continue` | The language has no `return` either (DESIGN.md §5); structured loops stay single-exit — revisit with a labelled-block design, not ad hoc |
| `==` on structs / arrays / ranges | Element-wise equality needs a recursion rule and interacts with owned fields; deferred to 0.1.0 with traits |
| Nested / multi-level field assignment (`p.a.b = e`) | Needs place-path machinery; 0.0.3 allows one-level `p.f = e` only (D2) |
| `pub(crate)` / `pub(super)` granularity | One visibility bit in 0.0.3 (D6); refine when a real user needs it |
| Glob imports (`import foo.*`), import renaming | Against the "explicit, traceable" grain; selected imports deferred |
| Separate compilation / linking | Whole-program single `.flnc` stays; module info is compile-time only and never reaches bytecode |
| flint registry, semantic versioning resolution | 0.0.3 dependencies are git-only (D9); a registry is an ecosystem-scale problem for later |
| File I/O (a `File` type), string interpolation, HashMap | Each needs generics/traits or its own design cycle; not 0.0.3 |
| `async`, `unsafe`, `ptr` / `addr` | Roadmap 0.0.4 / 0.0.5 unchanged |

---

## 2. Design decision points (to be written into DESIGN.md after review)

Each decision point presents a **recommended option** with rationale; the review may overturn it,
and if so the corresponding details in §3–§5 must be updated accordingly.

### D1: struct declaration syntax and field visibility

**Recommendation**:

```fleen
struct Point {
    x: int,
    y: int,
}

struct Wrapper {
    labels: [string],      // arrays allowed
    pos: Point,            // other structs allowed
    payload: box<int>,     // boxes allowed
}
```

- Declaration keyword `struct`, fields `name: type`, comma-separated, trailing comma allowed;
  a `struct` declaration takes no trailing semicolon (same rule as `func`).
- **Fields have no per-field visibility: a field is exactly as visible as its struct** (§1.4
  rejects `pub` fields). One visibility bit per item, per D6.
- **Recursive structs are forbidden** (directly or indirectly, including via `box`): a struct's
  size must be statically known and boxed self-reference adds cycle risk for zero 0.0.3 benefit.
  `typeck` reports `RecursiveStruct` with the cycle path. Revisit with explicit indirection
  syntax later.

Rationale: the literal grammar is already reserved in `SYNTAX.ebnf`
(`struct_literal = ident , "{" , field_init … }`), so declaration syntax is chosen to make the
two mirror each other. Per-field `pub` is the single largest privacy-design money pit; shipping
without it costs nothing until a real use case appears.

### D2: struct ownership and field access semantics

**Recommendation**: structs follow the 0.0.2 value classes verbatim:

- `is_copy(S)` = **all fields are Copy** (the same rule already used for `Result<T, E>`).
  A struct with any owned field (`string`, `box`, array, owned struct) is an owned type:
  binding/argument transfer needs `move s` / `clone s`.
- **Field read `p.f` is a copy of the field** — bitwise for Copy fields; a deep copy for owned
  fields. This mirrors 0.0.2 D2 (`deref b` reads copy). Deep-copying on read is the 0.0.3
  trade-off for having no borrow of struct interiors; it matches "correctness first" (0.0.2 D4).
- **Cannot move out of a struct field**: `move p.f` is a compile error (`MoveOutOfStruct`,
  mirroring `MoveOutOfBox`). Copying out is `clone p.f`.
- **Field assignment `p.f = e`** is a legal assignment target when `p` is a local or global
  struct binding and `f` is a direct field (one level, per §1.4). The old field value is
  released. `e` owned → needs `move` / `clone` as usual.
- Consuming a whole struct passes it with `move p` / `clone p`; no field-wise partial moves
  (the moved-set stays per-binding, exactly as in 0.0.2 — no per-field liveness).

Rationale: zero new ownership machinery. The 0.0.2 checker tracks bindings, not places;
keeping that invariant means the ownership checker changes only in `is_copy`, not in its
algorithm.

### D3: array ownership and element semantics

**Recommendation**:

- `[T]` is an **owned type** for every `T` (even `[int]`): the whole array moves with
  `move a` / deep-copies with `clone a`. (Unlike structs, there is no Copy case — array identity
  is heap state, and "Copy array" would silently fork on write.)
- **Element read `a[i]` is a bounds-checked copy** of the element (deep copy for owned element
  types). Out of bounds → runtime error `IndexOutOfRange` carrying a span (§7 mechanism).
- **Cannot move out of an element**: `move a[i]` is a compile error (`MoveOutOfArray`).
- **Element write `a[i] = e`** is a legal assignment target (local/global array binding);
  out-of-bounds → the same runtime error; owned `e` → `move` / `clone` as usual.
- Arrays are legal: as struct fields (D1), as `box` pointees (extends the 0.0.2 pointee
  whitelist with `[T]` and struct types), as `Result<T, E>` components, as parameters
  (owned → `move`/`clone`; `ref [T]` allowed, parameters only), as globals (0.0.2 D5 rules).

Rationale: same shape as D2 — the existing checker and VM value semantics absorb arrays with
new instructions, not new rules.

### D4: `for` loop syntax, iterables, and consumption

**Recommendation**:

```fleen
for i in range(0, 10) {
    print(i as string);
}

for name in names {          // names: [string]; elements are copied, `names` is NOT consumed
    print(name);
}
```

- Syntax `for ident in expr block` — a statement (like `while`), taking a semicolon per the
  0.0.2 ASI rules when used as an expression statement; the loop variable is a fresh immutable
  binding per iteration, scoped to the body (loop-scope rules of DESIGN.md §4.5 apply to
  assignments inside the body).
- **Iterable set (0.0.3, closed)**: arrays `[T]` and `Range` (D5). Anything else →
  `NotIterable { got }` with help naming the two options. User-defined iterables arrive with
  traits (0.1.0).
- **`for` does not consume the iterable**: elements are read as copies (D2/D3 read semantics).
  Iterating an owned array leaves it usable — the safe default, consistent with `ref` being
  read-only.
- **No `break` / `continue`** (§1.4); single-exit loops like `while`.
- Lowering desugars `for` into an index loop over the array length / range bounds (§5.4) —
  there are **no iterator objects at runtime** in 0.0.3.

Rationale: without traits, a closed iterable set is the honest design; desugaring keeps the VM
and verify untouched by iteration. The no-consume rule avoids inventing a "for-by-value vs
for-by-ref" distinction.

### D5: `Range` — a builtin type, constructed via std, no `..` syntax

**Recommendation**: `Range` is a builtin type (like `Result`: users cannot destructure it, only
construct via the provided functions). It holds `start: int` and `end: int` and iterates
`start, start+1, …, end-1` (end-exclusive, so `range(0, 0)` and `range(5, 5)` iterate zero
times — no off-by-one surprises). Construction lives in `std.collection.range`:

```fleen
range(n)          // 0..n
range_from(a, b)  // a..b
```

- No `a..b` operator syntax in 0.0.3 — it would add a precedence level and an ASI continuation
  entry for a thing two functions express; the operator can be added later without breaking
  anything (the type, not the syntax, is the commitment).
- `Range` values can be bound, passed, and stored like any Copy value (two ints);
  `==` on ranges is excluded (§1.4). `range_from(a, b)` with `a > b` is legal and empty
  (no error — the empty-iteration semantics make it harmless).

Rationale: mirrors how `Result` was introduced (type in 0.0.1, constructors in 0.0.2): the
compiler knows the type; std owns the vocabulary.

### D6: module mapping, visibility, and import semantics

**Recommendation**:

```
myproj/
├── flint.toml
└── src/
    ├── main.fln          ← program entry (crate root)
    ├── io.fln            ← module `io`
    └── net/
        ├── mod.fln       ← module `net` (a directory's entry is mod.fln, as in Rust)
        └── tcp.fln       ← module `net.tcp`
```

- **One file = one module; a directory = a submodule container with `mod.fln` as its entry.**
  The module path maps 1:1 to the file path from `src/` — **no `mod` declaration statements**;
  the file system is the module tree. (One fewer keyword than Rust; matches the reserved
  `import … ;` grammar.)
- **Import paths are absolute from the crate root**, except the reserved root `std`:
  `import net.tcp;` reads `src/net/tcp.fln`; `import std.io;` resolves into the shipped std
  (D8). No relative imports in 0.0.3.
- **Visibility: private by default; `pub` is the single visibility modifier** —
  `pub func`, `pub const`, `pub struct`, `pub` on global bindings. Non-`pub` top-level items
  are visible only within their own module (and its submodules? **No — own module only**;
  strictest simple rule). Accessing a private item from another module →
  `PrivateAccess { item, module }`.
- **Import semantics**: `import net.tcp;` makes the path `net.tcp` usable (`net.tcp.connect(…)`).
  No selected imports, no renaming, no globs (§1.4). Duplicate imports of the same path are
  legal (idempotent); two imports clashing with a local binding → resolve error.
- **Circular imports are a compile error** (`CircularImport`, reporting the cycle path
  `a → b → a`) — with whole-program compilation there is no order that works, and laziness
  would be a hidden mechanism.
- **Compilation stays whole-program**: the module table is built before resolution; the output
  is one `.flnc` exactly as today. Module structure never reaches bytecode.

Rationale: file-system-as-module-tree removes a whole class of duplication bugs (declared-vs-
actual module mismatch); the strictest visibility rule can only be loosened later, never
tightened cheaply.

### D7: prelude and backward compatibility

**Recommendation**: the prelude is exactly **`print`** (plus the existing `Ok` / `Err`
constructor recognition, which is typeck-level and not a name). `print` stays usable without
any import, so **every 0.0.1/0.0.2 program compiles unchanged** (subject only to the new
reserved words, §3.6.2). `std.io.print` exists as the explicit spelling and is the same
function.

Rationale: the v1 compatibility promise (0.0.2 TICKETS Notes) extends naturally: a prelude of
one name is also the smallest thing that needs defending. The prelude is implemented as a
compiler-injected implicit `import std.io;` into the crate root — no special case in typeck.

### D8: how std ships and what is an intrinsic

**Recommendation**:

- std is **ordinary Fleen source** in the repo's `std/` directory, **embedded into the compiler
  binary at build time** (`include_str!`-style) and compiled into the user's program on demand —
  only modules actually imported (directly or via prelude) are compiled. It rides the exact same
  pipeline as user code; std is the first and most demanding user of struct/array/for/modules.
- The **only** things VM-level are the intrinsics that cannot be written in Fleen: `print`
  (existing), `println`, `read_line`, `sqrt`/`floor` (float intrinsics), `os` args/exit, clock.
  They get real signatures in the existing builtins table (`typeck/builtins.rs`) — the 0.0.2
  mechanism, extended; **no std function is ever special-cased in `infer.rs`**.
- Everything else (`std.str`, `std.num` beyond intrinsics, `std.arr`, `std.collection.*`) is
  pure Fleen over the new bytecodes.

Rationale: a std written in Fleen is a permanent integration test of the language; a std written
in VM opcodes would be a parallel implementation to keep in sync.

### D9: flint scope — git dependencies only, CLI in-tree

**Recommendation**:

- Manifest `flint.toml`; dependencies are **git URLs + version requirement**, resolved to a
  commit recorded in `Flint.lock` (committed) for reproducible builds. No registry, no
  semver resolution engine — `version = "0.2"` is advisory metadata in 0.0.3 and the lockfile is
  the actual pin.
- Cache `~/.flint/cache/<name>-<version>-<commit>/`; dependency projects must themselves be
  flint projects (recursively read their manifests).
- `flint` is a **workspace crate (`flint-cli`) in this repo**, using `fleen-compiler` and
  `fleen-vm` as libraries. Commands: `flint new / build / run / test / check`.
  `cargo fln file.fln` remains for single files; `flint run` is the project-level entry.

Rationale: a registry is an operational commitment (hosting, security, governance) far beyond a
0.0.x release; git+lockfile is how Cargo and Go both started. Keeping flint in-tree reuses the
compiler crates with zero packaging overhead.

### D10: bytecode v3 opcode allocation and module-free bytecode

**Recommendation**: open a new group **`0xB0–0xBF` for struct / array / range** (the next free
group after 0xA6–0xFF's reservation; §5.1 assigns 0xB0–0xB7 concretely). `version` rises to `3`;
readers accept `{1, 2, 3}`. The `.flnc` layout **appends one new table** (`struct_defs`, for
verify's field-count checks) per the append-only rule; **no module table is added** — modules
are resolved at compile time and leave no trace in the module format.

Rationale: follows BYTECODE.md §10's monotonic numbering and append-only table rules to the
letter; keeping modules out of bytecode is what makes "whole-program compile" (D6) cheap.

---

## 3. Language Feature Design

### 3.1 struct (F1)

#### 3.1.1 Syntax

```fleen
struct Point {
    x: int,
    y: int,
}

func main(): int {
    p = Point { x: 1, y: 2 };      // literal (EBNF already reserved)
    q = clone p;                    // owned? no: both fields Copy → Point is Copy
    r = Point { x: p.x, y: 3 };    // field read
    print(r.x as string);          // nested postfix chains work: r.x is a place
    0
}
```

- The literal is the reserved `struct_literal` rule; field init order is free; **all fields must
  be initialized** (missing → `MissingField { name }`; unknown → `UnknownField { name }`);
  no field-init shorthand (`Point { x, y }`) in 0.0.3 — it silently relies on variable names,
  and explicitness is the house style.
- Field access `p.f` activates the reserved `field` postfix. Chains compose:
  `w.payload` where `payload: box<int>` is `box<int>` — combined with `deref`:
  `deref w.payload`. Field access through a `ref` parameter is a read (the copy-out semantics
  of D2 make it trivially safe).
- Type syntax: struct names are used bare (`p: Point`), joining `int / string / box<T> / [T] /
  Result<T, E>` in the type grammar.

#### 3.1.2 Rules summary

| Rule | Content |
|------|------|
| Classification | `is_copy(S)` = all fields Copy (D2); otherwise owned: `move` / `clone` at consuming positions |
| Field read | Copy of the field (deep for owned fields); `move p.f` forbidden |
| Field write | `p.f = e`, direct field of a local/global binding only; owned `e` needs `move` / `clone` |
| Equality / patterns | No `==`, no `choose` patterns on structs (§1.4) |
| In containers | `box<Point>`, `[Point]`, `Result<Point, string>`, `ref Point` parameters: all legal |

#### 3.1.3 Example error message

```
error: cannot move out of struct field `p.name`
  --> demo.fln:6:9
   |
 6 | t = move p.name;
   |     ^^^^^^^^^^^ structs own their fields exclusively
   |
help: copy the field instead: `t = clone p.name;`
```

### 3.2 arrays `[T]` (F2)

#### 3.2.1 Syntax and semantics

```fleen
a = [1, 2, 3];              // a: [int]
n = a[0];                   // read: bounds-checked copy
a[2] = 10;                  // write: bounds-checked
s = ["x", "y"];             // [string]: owned element type
t = clone s;                // deep copy: new array + new elements
u = move s;                 // transfer: s is no longer usable
big = box [0, 0, 0];        // box<[int]>: heap pointee is an array
first = deref big;          // ERROR shape — deref copies the pointee; use deref for reads:
f0 = deref big[0];          // actually: (deref big)[0] — postfix chains left-to-right
```

- Array literal `[e1, e2, …]`; empty array literals need a type annotation
  (`empty: [int] = [];`) — otherwise `CannotInferArrayType`.
- Element type is **uniform** (first element infers, later elements must unify →
  `ArrayTypeMismatch`).
- All index operations are bounds-checked at runtime; the verified bytecode cannot index out of
  bounds silently (§5.1).
- Multi-dimensional arrays are arrays of arrays: `[[1, 2], [3, 4]]: [[int]]` — falls out of the
  uniform-element rule; rows may be ragged (each row its own array).

#### 3.2.2 Interaction with ASI

`[` is in the existing continuation set (0.0.2 ASI, index chains) — array *literals* introduce a
new start-of-statement use of `[`:

- Statement start: `[` begins an array literal statement (an expression statement, e.g.
  `[1, 2, 3];` discards — legal, like any expression statement).
- Continuation across lines keeps the 0.0.2 rule: `x =` ⏎ `[1, 2];` continues (ASI rule 4 inserts
  nothing because `[` is a continuation token; the trap table of §8 must cover
  `f()` ⏎ `[0]` = index — unchanged — and the new `x =` ⏎ `[1, 2]` = literal, distinguished by
  the same predicate the parser already uses for index vs literal at the postfix position).

### 3.3 for and Range (F3)

#### 3.3.1 Semantics

- The loop variable is a **fresh binding per iteration**; assigning to it inside the body is a
  new binding per the §4.5 loop-scope rules (same as `while` bodies).
- The iterable expression is evaluated **once**, before the loop.
- Iterating an array yields elements as copies (D3); iterating a range yields `int`s.
- `for` is a statement whose value is `unit`; as an expression statement it takes a semicolon
  (omittable per ASI), exactly like `while`.

#### 3.3.2 Lowering sketch (detail in §5.4)

```text
for x in expr { body }
≈  __it = expr; __i = 0; n = len-or-bounds(__it);
   while __i < n { x = copy(__it[__i]); body; __i = __i + 1; }
```

The desugar happens in Lower (MIR), not Parse — the parser produces a real `StmtFor` node so
resolve/typeck see the loop structure (no phase skipping).

### 3.4 Module system (F5)

#### 3.4.1 Layout and resolution

Per D6. Resolution algorithm (module-table sub-pass, runs before resolve per §4.3):

1. Parse `flint.toml` → crate name → crate root `src/main.fln` (module `main`).
2. BFS over `import` declarations: module path → file path (`a.b` → `src/a/b.fln`, or
   `src/a/b/mod.fln`); `std.*` resolves to the embedded std (D8).
3. Missing file → `ModuleNotFound { path, tried }` listing both candidate paths.
4. Cycle detection on the import graph → `CircularImport` with the full path.
5. Result: a module table (module id → file, imports, exported-symbol map), consumed by
   resolve.

#### 3.4.2 Visibility

| Item kind | Private syntax | Public syntax | Visible to |
|------|------|------|------|
| `func` | `func f(…) …` | `pub func f(…) …` | own module only / everywhere |
| `const` / global binding | `const c = …` / `x = …` | `pub const c = …` / `pub x = …` | own module only / everywhere |
| `struct` | `struct S { … }` | `pub struct S { … }` | own module only / everywhere |

`pub` on anything except top-level items is a parse error (`PubNotAllowedHere`).
`import` itself needs no `pub` — modules are importable; their private contents are not.

#### 3.4.3 Name resolution across modules

- A path `a.b.f` resolves left-to-right through the module table: `a` must be imported (or the
  prelude/`std`), each segment must be a module, the final segment must be a `pub` item.
- Shadowing: an import does not bind a local name, so the existing shadowing rules are
  untouched; a local binding named `net` shadows the module path `net.…` from then on
  (resolve reports the shadowing with its usual machinery — module paths are resolved like
  any other name).
- Two modules may both define `helper`; there is no ambiguity because paths are always
  qualified in 0.0.3 (no unqualified cross-module imports).

#### 3.4.4 Example error messages

```
error: module not found: net.tpc
  --> src/main.fln:2:8
   |
 2 | import net.tpc;
   |        ^^^^^^^^
   |
help: no file matched. tried: src/net/tpc.fln, src/net/tpc/mod.fln

error: `secret` is private to module `auth`
  --> src/main.fln:7:5
   |
 7 |     auth.secret();
   |     ^^^^^^^^^^^ not marked `pub`
   |
help: add `pub` to its declaration in src/auth.fln, or use a public function
```

#### 3.4.5 Spans gain file identity (cross-cutting, do first)

Every `Span` today is a byte range into one source string. With multiple files, diagnostics
must name the file. 0.0.3 introduces **`FileId`** (a newtype per SPEC §5 ID rules) into `Span`
(`Span { file: FileId, start, end }`) or into `Diagnostic` (implementation choice settled in
TICKETS; the compiler-internal recommendation is `Span`-carried, so every existing span consumer
gets it for free). The lexer's source-map becomes a `SourceMap: Vec<SourceFile>`. This is a
mechanical refactor across all stages and **must land before module resolution** (V03) so
multi-file errors are first-class from day one.

### 3.5 std (F6)

#### 3.5.1 Layout

```
std/
├── mod.fln            # root: re-exports for the prelude (print)
├── io.fln             # print, println, read_line
├── str.fln            # len, concat, split, from_int, from_float, from_bool
├── num.fln            # abs, min, max (Fleen) · sqrt, floor (intrinsics)
├── arr.fln            # len, push, get (Result), set
├── collection/
│   ├── mod.fln
│   ├── range.fln      # range(n), range_from(a, b) → Range
│   └── iter.fln       # map, filter, fold, take, zip  (eager, pure Fleen)
├── os.fln             # args, exit
└── time.fln           # now_millis
```

#### 3.5.2 Signatures (excerpt; the full table lands in `builtins.rs` + std source)

| Module | Signature | Kind |
|------|------|------|
| `std.io` | `print(string...) -> unit` | intrinsic (existing) |
| `std.io` | `println(string...) -> unit` | intrinsic |
| `std.io` | `read_line() -> string` | intrinsic |
| `std.str` | `len(string) -> int` · `split(string, string) -> [string]` · `from_int(int) -> string` | `from_int` = existing `ToStr`; rest intrinsic |
| `std.num` | `sqrt(float) -> float` · `floor(float) -> int` | intrinsics |
| `std.arr` | `len[T]([T]) -> int` | `ArrLen` opcode (§5.1); typed in builtins per-element-type by generic-free overloading table entry `[T]` — see note |
| `std.arr` | `get[T]([T], int) -> Result<T, string>` | pure Fleen over `ArrLen`/`ArrGet` |
| `std.collection.iter` | `map[T, U]([T], (T) -> U) -> [U]`, `fold[T, A]([T], A, (A, T) -> A) -> A`, … | pure Fleen (**eager**: returns arrays) |

> **Note on polymorphic std functions**: 0.0.3 has no generics. `std.arr` / `std.collection.iter`
> are the first places that *want* them. The 0.0.3 mechanism: the builtins table gains
> **schematic (type-variable) signatures** — `len: [T]([T]) -> int` where `T` unifies with the
> argument, exactly like `Result<T, E>` is unified today. This is a bounded, internal form of
> polymorphism (no user-written generics, no inference beyond unification of one call); it is
> the same trick 0.0.2 used for `Ok`/`Err` context inference, extended to a table. User-facing
> generics remain 0.1.0.

#### 3.5.3 The prelude

`print` is visible without import (D7). Everything else requires `import std.io;` etc.
The prelude is one implicit import injected into the crate root — removal of any prelude name in
a future version is therefore a one-line CHANGELOG item, not a mechanism change.

### 3.6 flint (F7)

#### 3.6.1 Manifest and layout

```toml
# flint.toml
[package]
name = "myproj"
version = "0.1.0"

[dependencies]
jsonlite = { git = "https://github.com/user/jsonlite", version = "0.2" }
```

```
myproj/
├── flint.toml
├── Flint.lock          # generated; committed; pins name→{commit, version}
└── src/
    └── main.fln
```

- Dependency resolution: read manifests recursively (deps of deps), fetch missing commits into
  `~/.flint/cache/<name>-<commit>/`, record everything in `Flint.lock`; a dirty lockfile
  (manifest changed) triggers re-resolution. Version strings are advisory in 0.0.3 (D9).
- Dependency sources are **read-only** to the build: their modules are importable
  (`import jsonlite.parse;` — crate name is the root), their `pub` items visible, everything
  else invisible; a dependency cannot import the depending project.

#### 3.6.2 Commands

| Command | Behavior |
|------|------|
| `flint new <name>` | Scaffold: directory, `flint.toml`, `src/main.fln` hello-world, git init |
| `flint build` | Resolve deps → compile whole program (incl. std + deps) → `.flnc` |
| `flint run` | build + execute |
| `flint test` | Run `tests/` e2e cases of the project (`.fln` + `.expected` contract) |
| `flint check` | build + verify, no execution |

`cargo fln file.fln` is unchanged for single files; `flint run` compiles in project mode.

#### 3.6.3 Reserved words (breaking)

`struct`, `for`, `in`, `pub` become reserved keywords (0.0.2 programs using them as variable
names break — CHANGELOG ⚠️). `import` was already reserved; `Range` is a builtin type *name*
(like `Result`), not a keyword. `move` / `clone` / `deref` / `box` / `ref` / `as` / `?` are
unchanged from 0.0.2.

---

## 4. Changes per Compiler Stage

> Stage boundaries match SPEC §6: no stage skipping, no shared mutable state, every stage
> independently testable.

### 4.1 Lex (`lexer/`)

| Item | Content |
|----|------|
| New keyword tokens | `Struct` (`struct`), `For` (`for`), `In` (`in`), `Pub` (`pub`) |
| Existing, reused | `[` `]` `,` `.` `:` tokens already exist (index/params); `import` keyword exists (parsed but inert in 0.0.2 — now it resolves) |
| `is_keyword` / `keyword_str` / `Display` | Updated in sync |

### 4.2 Parse (`parser/`)

| Item | Content |
|----|------|
| New declarations | `struct_decl = "struct" , ident , "{" , [ field_decl , { "," , field_decl } ] , "}"` ; `pub` prefix on top-level declarations |
| New statement | `for_stmt = "for" , ident , "in" , expr , block` |
| Array literal | `array_literal = "[" , [ expr , { "," , expr } ] , "]"` at primary position (distinct from index postfix by parse position, as today) |
| Field access | Activates the reserved `field` postfix (`postfix , "." , ident`); chains compose with call/index/`?` |
| Assignment targets | `a[i] = e` (was index-chain-as-target? — 0.0.2 had no index write; add), `p.f = e`, one level each per D2/D3 |
| ASI predicate updates | `for` joins the start set and the block-like-header continuation climb (body may hug `}`); `in` joins the continuation set (the iterable may span lines); `[` literal-vs-index disambiguation documented (§3.2.2); trap cases in §8 |
| EBNF | Bumped to v0.0.3: struct_decl, pub, for_stmt, array_literal, field access, new assignment targets |

### 4.3 Resolve (`resolver/`)

| Item | Content |
|----|------|
| **FileId / SourceMap** | `Span` gains file identity (§3.4.5); mechanical refactor across stages |
| **Module table sub-pass** | New `resolver/modules.rs`: import-graph BFS, file resolution, cycle detection, module table (D6) — runs before per-module resolution; **not a new phase** (same precedent as 0.0.2's ownership-in-typeck) |
| Cross-module resolution | Qualified paths resolve through the module table; `pub` enforcement → `PrivateAccess` |
| Prelude | One implicit `import std.io;` injected at the crate root (D7) |
| Unchanged | `=` triage, shadowing-mutability consistency, loop-body assignment rules |

### 4.4 Typeck (`typeck/`)

| Item | Content |
|----|------|
| New types | `Type::Struct(StructId)`, `Type::Array(Box<Type>)`, `Type::Range`; struct table (id → name, fields, spans) |
| `is_copy` | Extends per D2/D3: struct = all fields Copy; array = never; Range = Copy |
| Field access checking | Existence, visibility, one-level-write rule; `MoveOutOfStruct` |
| Array checking | Uniform element type, literal inference, annotation for empty literals; `MoveOutOfArray`; index target place rules |
| `for` checking | Iterable ∈ {[T], Range}; loop-variable binding type; body scope per §4.5 |
| Schematic builtins | Type-variable signatures for std's `[T]` functions (§3.5.2 note) |
| New error kinds | `RecursiveStruct`, `MissingField`, `UnknownField`, `MoveOutOfStruct`, `MoveOutOfArray`, `NotIterable`, `CannotInferArrayType`, `ArrayTypeMismatch`, `IndexNotInt`, `PrivateAccess` (with resolve), `ModuleNotFound`/`CircularImport` (from the module table) |

### 4.5 Lower (`lower/`)

| Item | Content |
|----|------|
| MIR instruction extension | `StructNew(u16 id)`, `GetField(u16 offset)`, `SetFieldLocal(u16 slot, u16 offset)`, `SetFieldGlobal(u16 gid, u16 offset)`, `ArrNew(u16 n)`, `ArrLen`, `ArrGet`, `ArrSetLocal(u16 slot)`, `ArrSetGlobal(u16 gid)`, `MakeRange`, `RangeStart`, `RangeEnd` (full table in §5.1) |
| `for` desugar | §5.4; happens in Lower; MIR stays loop-flattened like `while` |
| Field/index writes | Lower to the `Set*`/`ArrSet*` slot forms (place = local/global binding, per D2/D3) |
| Spans | Carried per instruction as since 0.0.2 (§7's runtime diagnostics inherit arrays/structs for free) |

### 4.6 Codegen (`codegen/`)

| Item | Content |
|----|------|
| Opcode | v3 instructions 1:1 with MIR (§5.1); `version = 3` |
| struct table | Emits the new appended `struct_defs` table (name-less field count + field types for verify) |
| Constant pool | Unchanged (array/struct data is never pooled; literals still per-element `Const`) |
| Deserialization | `from_bytes` accepts `version ∈ {1, 2, 3}` |

### 4.7 Verify (`fleen-verify/`)

| Item | Content |
|----|------|
| Stack effect table | Δdepth/minimums for the 12 new instructions (§5.2); `StructNew`/`ArrNew` effects are **static** (the count is an operand) |
| struct table validation | `StructNew(id)`'s count must equal the `struct_defs` entry; ids in range |
| New runtime-error preconditions | `ArrGet` index < len (VM-side check; verify only checks shape) |
| Version gate | Accepts `{1, 2, 3}`; v3 instruction in a v1/v2 module → `BadOpcode` (holds naturally) |

### 4.8 VM (`fleen-vm/`)

| Item | Content |
|----|------|
| Value rework | §5.3: `Value::Struct(Box<[Value]>)`, `Value::Arr(Box<[Value]>)`, `Value::Range { start: i64, end: i64 }` |
| Execution | New instructions are mechanical: `StructNew` pops n field values; `ArrGet` bounds-checks → `RuntimeError::IndexOutOfRange` (span via §7's span_map); `RangeStart/End` destructure |
| Copy semantics | `clone` of struct/array deep-copies (existing `DupDeep` generalizes over the new Value variants) |
| Intrinsics | `println`, `read_line`, `sqrt`, `floor`, `args`, `exit`, `now_millis` join the builtins dispatch (the `print` mechanism, extended; §3.5.2) |
| New RuntimeErrors | `IndexOutOfRange` (defensive; legal bytecode from the compiler never triggers it) |

---

## 5. Bytecode v3 Spec Additions (to be merged into BYTECODE.md)

### 5.1 New instructions (open group 0xB0–0xB7; 0xB8–0xBF stay reserved)

| opcode | Mnemonic | Operands | Bytes | Stack effect | Description |
|--------|--------|--------|------|--------|------|
| 0xB0 | `StructNew` | `u16` struct_id | 3 | `f₁ … fₙ → s` | Pop n field values (order = declaration order) into struct `id`; n comes from `struct_defs` |
| 0xB1 | `GetField` | `u16` offset | 3 | `s → v` | Pop the struct, push a **copy** of field `offset` (D2 read semantics) |
| 0xB2 | `SetFieldLocal` | `u16` slot, `u16` offset | 5 | `v →` | Overwrite field `offset` of the struct in local `slot`; old field released |
| 0xB3 | `SetFieldGlobal` | `u16` gid, `u16` offset | 5 | `v →` | Same, for a global binding |
| 0xB4 | `ArrNew` | `u16` n | 3 | `e₁ … eₙ → a` | Pop n elements into a fresh array |
| 0xB5 | `ArrLen` | — | 1 | `a → a int` | Length (array stays on stack) |
| 0xB6 | `ArrGet` | — | 1 | `a i → a v` | Bounds-checked element **copy**; out of bounds → `IndexOutOfRange` |
| 0xB7 | `ArrSetLocal` | `u16` slot | 3 | `v i →` | Bounds-checked store into local `slot`'s array (`a[i] = e`) |
| — | `ArrSetGlobal` | `u16` gid | 3 | `v i →` | Same, for a global; **number TBD within 0xB8–0xBF or 0xA6–0xAF at spec time** (TICKETS finalizes; the group has room) |
| 0xB8 | `MakeRange` | — | 1 | `start end → r` | Range value (Copy: two i64) |
| 0xB9 | `RangeStart` | — | 1 | `r → r int` | Start bound (copy) |
| 0xBA | `RangeEnd` | — | 1 | `r → r int` | End bound (copy) |

Invariant: existing instructions (0x00–0xBA) keep their opcodes and encodings **unchanged, to the
letter** (BYTECODE.md §10 rule); v1/v2 modules remain executable on the 0.0.3 VM/verify.

### 5.2 Stack-depth analysis addendum (for verify)

| Instruction | Δdepth | Minimum depth before execution |
|------|--------|----------------|
| `StructNew(id)` | `1 − n` (n from the struct table) | n |
| `ArrLen` / `ArrGet` / `RangeStart` / `RangeEnd` | +1 | 1 |
| `GetField` | 0 (pops the struct, pushes the field copy) | 1 |
| `ArrNew(n)` | `1 − n` | n |
| `SetFieldLocal` / `SetFieldGlobal` | −1 (the value; the offset is an operand) | 1 |
| `ArrSetLocal` / `ArrSetGlobal` | −2 (−1 for the value, −1 for the index) | 2 |
| `MakeRange` | −1 | 2 |

All effects are static: the variable-arity instructions take their count as an operand, so
verify needs no abstract interpretation — the existing per-instruction arithmetic extends.

> **V02 erratum**: §5.1 originally gave `GetField` the peek form `s → s v` (Δ+1) and lumped
> `SetFieldLocal`/`SetFieldGlobal` into §5.2's −2 row. As built (found by the V02 verify
> cross-checks), `GetField` **pops** the struct (`s → v`, Δ0, min 1) — the peek form leaves
> the struct under the field value with no remove-below-top instruction, and `p.f.g` chains
> require the pop. `SetField*` is Δ−1, min 1 — the offset is an *operand*, not a stack item
> (the −2 row conflated them with `ArrSet*`, which does take a stack index). The corrected
> rows above are authoritative; BYTECODE.md picks them up at the V11 doc merge (see the
> TICKETS V02 completion notes).

### 5.3 Value representation

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),
    Boxed(Box<Value>),
    Ref { base: u32, slot: u16 },
    Ok(Box<Value>),
    Err(Box<Value>),
    Struct(Box<[Value]>),          // ← new: fields in declaration order
    Arr(Box<[Value]>),             // ← new
    Range { start: i64, end: i64 },// ← new
    Unit,
    Func(FuncId),
}
```

- `Eq` semantics: new variants are **not comparable** in 0.0.3 (§1.4) — `==` on them is a
  *typeck* error; the VM's `Eq` reaching a struct/array/range is a defensive
  `RuntimeError::TypeMismatch` (unreachable via legal bytecode).
- `DupDeep` generalizes: deep-copy walks `Struct`/`Arr`/`Boxed`/`Ok`/`Err` recursively.
- `Value::Range` is Copy; `Struct`/`Arr` are owned.

### 5.4 `for` lowering and bytecode mapping

```fleen
for x in names { print(x); }     // names: [string]
```

```text
<eval names>          ; a
StoreLocal __it
Const 0 ; StoreLocal __i
L_loop:
LoadLocal __it ; ArrLen          ; a n
LoadLocal __i ; IntLt            ; a n i → a bool(__i < n)
JumpIfFalse L_end
LoadLocal __it ; LoadLocal __i ; ArrGet   ; element copy
StoreLocal x
<block body>
LoadLocal __i ; Const 1 ; IntAdd ; StoreLocal __i
Jump L_loop
L_end:
```

Range iteration is the same shape with `__i = RangeStart`, bound = `RangeEnd`
(evaluated once, before the loop — D4's "iterable evaluated once" falls out of the desugar).

### 5.5 The .flnc format

- `version = 3`; readers accept `{1, 2, 3}` (BYTECODE.md §10's monotonic rule).
- **One appended table**: `struct_defs` (`count: u32`, then per struct: `field_count: u32`,
  `field_types: u8 × field_count` — a small type-tag enum sufficient for verify's count/shape
  checks). Field order/widths of all existing tables unchanged.
- **No module table** (D10): modules are compile-time; the bytecode is oblivious to them.

---

## 6. The std Library (F6, consolidating §3.5)

1. `std/` Fleen sources, embedded into the compiler binary at build time (D8);
   compiled on demand — only imported modules enter the program.
2. The builtins table gains schematic signatures for the `[T]` std functions (§3.5.2 note) and
   concrete signatures for the intrinsics; **`infer.rs` stays free of std special cases** — the
   0.0.2 print-fix discipline holds.
3. Prelude = `print` only, implemented as an implicit import (D7).
4. std's own tests live under `tests/std/` (§8.2) and run as part of `cargo test` — std is
   always compiled and exercised by the CI build, so it cannot rot.
5. `std.collection.iter` is **eager** (arrays in, arrays out); lazy iterators need user-generic
   functions and wait for 0.1.0 — documented in `std/collection/iter.fln`'s header comment.

---

## 7. Runtime Diagnostics (inheritance note)

0.0.2's span_map machinery (SpanEntry, ip→line/column, `.fln` one-stop mode) carries over
unchanged. 0.0.3 additions:

- `IndexOutOfRange` joins the spanned runtime errors (the faulting `ArrGet`'s span);
- Multi-file programs print `--> src/net/tcp.fln:12:5` — the CLI resolves `FileId` through the
  SourceMap (§3.4.5); when executing a `.flnc` directly, degrade exactly as 0.0.2 does
  (function name + instruction offset);
- Exit code contract unchanged (0/1/2).

---

## 8. Test Plan

> Follows SPEC §7: at least one test per rule, at least one test per error, tests written before
> bug fixes. The e2e harness **gains a project mode** (multi-file) — see §8.2.

### 8.1 New cases per stage (excerpt)

| Stage | Required valid tests | Required invalid tests |
|------|-----------|--------------|
| lexer | Tokenization of `struct` `for` `in` `pub` | — |
| parser | struct decl/literal, array literals (nested, empty-annotated), field chains, `for` over array/range, `p.f = e` / `a[i] = e` targets, ASI traps (§3.2.2, `for` header spans, body hugs `}`) | `pub` on local items, missing field in literal, `for x` (no `in`), dangling `[` |
| resolver/modules | module table build, `mod.fln` vs flat file, duplicate import idempotent, prelude injection | `ModuleNotFound`, `CircularImport` (2- and 3-cycle), `PrivateAccess` (func/const/struct × each) |
| typeck | is_copy for Copy-only struct vs owned-field struct, field read/write rules, array uniformity, range iteration, schematic builtins (`std.arr.len` on `[int]` and `[string]`) | `RecursiveStruct`, `MoveOutOfStruct`, `MoveOutOfArray`, `NotIterable`, `CannotInferArrayType`, `ArrayTypeMismatch`, `MissingField`, `UnknownField`, `IndexNotInt`, `==` on struct/array |
| lower/codegen | §5.4 mapping, struct/array instruction sequences item by item | — |
| verify | Stack-depth of all v3 instructions incl. operand-derived counts, struct-table cross-checks, v1/v2 module regression | Forged `StructNew` (bad id / wrong count), OOB-shape `ArrGet`, v3 opcode in v2 module |
| vm | Struct/field copy independence, array deep-copy independence, range iteration edge (empty range, single element), index OOB error path with span | `IndexOutOfRange`, defensive `TypeMismatch` on struct `Eq` |

### 8.2 New E2E

```
tests/e2e/valid/
├── fib.fln                  # 0.0.1 regression: unchanged
├── box_demo.fln …           # 0.0.2 regressions: unchanged
├── struct_demo.fln          # declaration, literal, field read/write, move/clone
├── array_demo.fln           # literal, index read/write, clone independence
├── for_range.fln            # for over range + array; empty range; no-consume check
├── iter_helpers.fln         # std.collection.iter map/filter/fold/collect chain

tests/e2e/invalid/
├── move_out_of_struct.fln
├── not_iterable.fln
├── private_access.fln       # cross-module privacy (project mode)
├── circular_import.fln      # project mode

tests/e2e/projects/          # NEW: project-mode harness (multi-file)
├── netdemo/                 # src/main.fln + src/net/mod.fln + src/net/tcp.fln + expected
└── depdemo/                 # uses a local file:// git fixture dependency; flint build/run

tests/std/                   # std exercised directly (it is always compiled in CI)
├── str.fln + str.expected
├── arr.fln + arr.expected
└── …
```

### 8.3 Compatibility fixtures

- v1/v2 `.flnc` fixed-byte fixtures remain executable (extend the 0.0.2 fixture set);
- All 0.0.1/0.0.2 e2e files pass **byte-for-byte** (only the reserved-word breaking change can
  surface, and none of the existing test names collide with `struct/for/in/pub`).

---

## 9. Task Breakdown and Milestones (continuing T/U numbering; this release uses the V prefix)

| Ticket | Area | crate/module | Deliverables | Depends on |
|------|------|-----------|------|------|
| V01 | Lex | `lexer/` | Struct/For/In/Pub tokens | — |
| V02 | Parse | `parser/` | struct decl/literal, array literal, for, pub, field access, new assignment targets, EBNF v0.0.3, ASI predicate updates | V01 |
| V03 | Span+Modules | all stages (mechanical) + `resolver/modules.rs` | FileId/SourceMap refactor; module table, cycle detection, prelude injection, pub enforcement | V02 |
| V04 | Typeck | `typeck/` | Struct/Array/Range types, is_copy, field/index rules, for checks, schematic builtins, new error family | V02 |
| V05 | Lower | `lower/` | New MIR instructions, for-desugar, place-form lowering | V03, V04 |
| V06 | Codegen/Verify | `codegen/`, `fleen-verify/` | v3 opcodes, struct_defs table, stack effects, v1/v2 compat | V05 |
| V07 | VM | `fleen-vm/` | New Value variants, instruction execution, intrinsics dispatch, IndexOutOfRange | V06 |
| V08 | std | `std/`, `typeck/builtins.rs` | All std modules + signatures + prelude + `tests/std/` | V07 (V04 for schematic signatures) |
| V09 | flint | `flint-cli/` | Manifest/lockfile, git resolution + cache, new/build/run/test/check | V08 (std must ship for flint projects to be usable) |
| V10 | Test harness | `tests/` | Project-mode e2e harness + full §8 matrix + std suite | V07 |
| V11 | Docs | `docs/`, `CHANGELOG.md` | BYTECODE v3, DESIGN/EBNF/SPEC bumps, CHANGELOG 0.0.3 (Breaking: reserved words) | V10 |
| V12 | Release | repo | Version bump, release notes, README tour | V11 |

```
V01 → V02 → V03 ──────┐
        ↘             ↓
          V04 → V05 → V06 → V07 → V08 → V09 → V10 → V11 → V12
                                       └─────┘ V08 ∥ V09 start
```

### Milestones

| Milestone | Tickets | Marker |
|--------|------|------|
| M1 Frontend | V01–V02 | struct/for/array/pub parse; ASI traps green |
| M2 Multi-file semantics | V03–V04 | FileId lands; module table + visibility checks green; all invalid cases green |
| M3 Backend and runtime | V05–V07 | `struct_demo.fln`, `array_demo.fln`, `for_range.fln` run end to end; v1/v2 fixtures pass |
| M4 std | V08 | `tests/std/` green; prelude compat; iter helpers e2e |
| M5 flint | V09 | `depdemo` project builds and runs with a git fixture dependency |
| M6 Tests, docs, release | V10–V12 | Full matrix green; docs synced; 0.0.3 released |

---

## 10. Risks and Mitigations

| Risk | Level | Mitigation |
|------|------|------|
| **Scope creep: four features at once, the largest release yet** | High | Milestone gating M1–M6 in dependency order (aggregation → modules → std → flint); the §1.4 negative list is the review gate; no ticket may add a §1.4 item "while it's in there" |
| FileId/Span refactor touches every stage's tests | High | Front-load it (V03, before any feature work depends on spans); mechanical refactor in one PR; assertions unchanged, only constructors |
| ASI × new syntax traps (`for` headers, array literal vs index) cause silent misparses | Medium | Explicit trap table (§3.2.2, §8.1); `;`-always-legal escape hatch unchanged; `in`/`[` predicate changes are pure functions, unit-tested |
| Deep-copy read semantics (struct fields, array elements) is slow for large data | Medium | Accepted 0.0.3 trade-off (correctness first, same as 0.0.2 D4); no borrowed-struct-interior scope creep; revisit with 0.1.0 traits |
| Schematic builtins ([T] std functions) drift into accidental generics | Medium | Hard rule: type variables appear **only** in the builtins table, never in user syntax; any push beyond one-call unification is rejected as 0.1.0 scope |
| flint's git dependency makes tests network-dependent | Medium | Tests use `file://` local git fixtures only; no network in CI; cache layout covered by unit tests |
| Verify mis-models variable-arity instructions | Low | Effects are operand-derived and static (§5.2); a forged `StructNew` is caught by the struct-table cross-check |
| Decision reversals (D1–D10) cause rework | Medium | §2 is reviewed and frozen before V01; features most likely to be revisited (visibility granularity, `..` syntax, eager vs lazy iterators) are all designed as later-additive |

---

## 11. Documentation Sync Checklist (V11)

| Document | Changes |
|------|------|
| `docs/BYTECODE.md` | Bump to v0.0.3: §3.2 Value variants, §4.2 opcode space (new 0xB0–0xBA group), §5 new instructions, §8 stack effects, §10 version table (v3 row); the struct_defs table becomes official |
| `docs/DESIGN.md` | §9 composite types (`[T]` official, `Range` added); a new "Structs" chapter (§3.1 made official); §6 gains `for` (§3.3); a new "Modules" chapter (§3.4); a new "std and the prelude" chapter (§3.5/§6); §17 feature table (struct/for rows → ✅); §18 roadmap marks 0.0.3 complete; known-Hacks stay empty |
| `docs/SYNTAX.ebnf` | v0.0.3: struct_decl, pub, for_stmt, array_literal, field access activation, new assignment targets |
| `SPEC.md` | §Repo structure (+ `std/`, `flint-cli/`, `tests/std/`, `tests/e2e/projects/`); crate table (+ flint-cli); §14 quick reference (struct/for/modules/std); §15 feature table 0.0.3 column |
| `docs/0.0.3/TICKETS.md` | Break §9 down per the 0.0.2 TICKETS format |
| `CHANGELOG.md` | The 0.0.3 entry (Breaking: reserved words `struct` `for` `in` `pub`) |
| `README.md` | Feature overview + flint quick start |

---

## 12. Acceptance Criteria (DoD)

- [ ] `cargo fmt -- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `cargo doc --no-deps` all green
- [ ] All 0.0.1/0.0.2 E2E regress **byte-for-byte** (subject only to the reserved-words breaking change)
- [ ] All new E2E in §8.2 pass, including project-mode cases (`netdemo`, `depdemo` with a `file://` git fixture)
- [ ] No std function is special-cased in `infer.rs`; every std signature lives in the builtins table
- [ ] `flint new` → `flint run` works out of the box on a fresh project; `cargo fln file.fln` unchanged for single files
- [ ] v1 and v2 `.flnc` fixtures remain executable on the 0.0.3 verify/VM
- [ ] Multi-file compile errors and runtime errors carry correct `file:line:column`
- [ ] All decision points in §2 (D1–D10) are settled in DESIGN.md
- [ ] std is compiled and exercised on every `cargo test` run (cannot rot)

---

> **0.0.3 in one sentence**: values get shape, loops get their natural form, code gets a home,
> and projects get a manager.
