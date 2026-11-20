# Fleen 0.0.8 · Cheeky Hamster 🐹

Per-file feature flags + function currying: an opt-in `#feature curry` directive
switches a file's function declaration form to the two-part curried declaration
(`add: int -> int -> int` / `add(a, b) = a + b`), makes curried arrow function
types the canonical notation, and enables implicit partial application (a
short-arity call yields a function over the remaining parameters). Bytecode v8
adds `PartialApply` (0xCE) and is **content-driven**: a module is v8 only when a
partial call was actually encoded, so a flag-on file without partial calls stays
v7 and every flag-off file is byte-identical to 0.0.7 (D16).

Decisions: `docs/0.0.8/PLAN.md` (D1–D16). Tickets: `docs/0.0.8/TICKETS.md`
(T01–T11). Non-breaking additive (D16) — the flag is per file, never per project,
and an importing file's flag never affects what an imported function is (D5/D6).

## Highlights

- **Per-file feature flags (D1/D6)** — the `#feature curry` directive is accepted
  only in a file's header region (before the first item; comments and blank lines
  do not end the region). Directive lines take no semicolon; unknown features are
  a hard error (`UnknownFeature`), duplicates are `DuplicateFeature`, a directive
  after the first item is `FeatureDirectiveMisplaced`. The directive is
  unconditional grammar — it is what carries the flag in flag-on and flag-off
  files alike — and `curry` is the only registered name in 0.0.8.
- **Two-part curried declarations (D2/D3, strict per file)** — in a flag-on file
  a function is declared as a single-line signature holding an arrow function
  type, then a named-parameter body (`add: int -> int -> int` / `add(a, b) = a + b`).
  The signature carries the modifiers (`async`/`unsafe`/`trusted`) and `pub`; the
  body line takes none (`PubOnCurriedBody` / `ModifierOnCurriedBody`), duplicate
  body parameters are `DuplicateParam`. The legacy `func` form is a parse error in
  a flag-on file (`FuncFormForbiddenInCurryFile`); in flag-off files the two-part
  productions do not exist. The resolver matches bodies to signatures by name and
  arity (`BodyWithoutSignature`, `SignatureWithoutBody`,
  `SignatureArityMismatch`), registers signatures before bodies resolve, and
  synthesizes a matched pair into an ordinary function — typeck, lower, codegen
  and the project-mode entry wiring see a normal `func`. Cross-module: the
  export/import tables carry the signature exactly as `func` exports do.
- **Curried arrow function types (D4)** — `->` is right-associative
  (`int -> int -> int` is one function of two parameters, canonicalizing to
  `Type::Func([int, int], int)`); `() -> T` is a zero-parameter function
  (`unit -> T` is unary over `unit`); `(int -> int) -> int` groups a param-side
  chain; the legacy tuple spelling `(int, int) -> int` stays legal sugar. Type
  rendering picks the spelling per the *file's* flag, so flag-off diagnostics
  keep the pre-0.0.8 text byte-identical.
- **Implicit partial application (D5/D7/D9)** — a call with fewer arguments than
  the callee's parameter count produces a function value over the remaining
  parameters (`inc = add(1)` → `int -> int`; a zero-argument call yields the
  function itself, D11). Participation is a call-site rule — the callee's file
  flag is irrelevant (D6) — but the callee must be fixed-arity, non-variadic,
  without `ref`/`ptr<T>` parameters and without `unsafe`/`trusted` modifiers
  (permanent exemption, D9, keeping the §12 static unsafe check total). Fixed and
  schematic builtins participate (`range_from(0)` is `int -> Range`); a partial
  is Copy like any function value (D11) and its captures are deep-copied on the
  Copy paths; `async` callees participate uniformly (a partial is
  `(rem) -> Future<T>`, D12); an expression statement discarding a partial is
  `PartialApplicationDiscarded` (D10).
- **Bytecode v8 — `PartialApply` (0xCE) (D13/D14)** — one new opcode in the
  reserved `0xCE–0xDF` group with a `u8` operand (the applied argument count);
  the version bump is content-driven: writers emit version 8 **iff** at least one
  `PartialApply` was emitted, else 7, and readers accept `{1..8}` — v1–v7 decode
  unchanged. Verify validates the stack shape (pops `k+1`, pushes 1); the VM is
  unconditional and flag-blind (D15); captures are never serialized, so a loaded
  v8 module never carries a capture list.
- **VM captured function values (D11/D15)** — `Value::Func` carries a capture
  list (`Func { id, captures }`; bare functions are unchanged);
  `PartialApply k` pops the function and `k` arguments and pushes the captured
  value; `CallValue` consumes `captures.len() + argc`, and the total must equal
  the callee's parameter count. Copy paths deep-copy a non-empty capture list, so
  two copies of a partial consume their own snapshots.

## E2E / examples

- The whole matrix runs in **both modes** — every pre-0.0.8 fixture passes
  byte-identically (the non-breaking gate, D16) — plus flag-on fixtures
  (`curry_demo`, `partial_flow`) and per-mode invalid fixtures
  (`FuncFormForbiddenInCurryFile`, discarded partial, partial of an `unsafe`
  function, `UnknownFeature`, `#` mid-file, the unchanged flag-off
  `ArityMismatch`). Project fixtures pin cross-flag import in both directions
  and the per-module v7/v8 split.
- `examples/0.0.8/` — the single-file `curry_demo.fln` (two-part declarations,
  partial application, a partial through std `map` and through a higher-order
  parameter) and the multi-module `curry_tour/` project (flag-on `main`
  partially applying flag-off `helpers` exports, a partial flowing into the
  flag-off `twice` helper) with README and the `curry_tour` flint-test case.

## Breaking changes

None (non-breaking additive, D16). Flag-off files compile to the same bytecode as
0.0.7; flag-on files keep the `ArityMismatch` they already had for too-few
arguments in pre-0.0.7 form (`PartialApplicationDiscarded` and the two new
diagnostics are new error kinds, not removals).

## Known limitations

- A partial over an **inline-lowered builtin** (`range`/`range_from`/`map`/
  `filter`/`foldl`/`foldr`/`take`) lowers through the D8 reroute but fails at
  runtime with `UnknownBuiltin` — captured function values for inline-lowered
  builtins are the recorded T08 gap (`docs/0.0.8/TICKETS.md`, T09 deviations).
  Partials over user functions (local or imported) run end to end.

## Artifacts

The release workflow (`.github/workflows/release.yml`, triggered by pushing the
`v0.0.8` tag) builds and publishes these assets:

| Asset | Platform |
|---|---|
| `fleen-0.0.8-x86_64-linux-gnu.tar.gz` | Linux x86_64 (glibc) |
| `fleen-0.0.8-x86_64-linux-musl.tar.gz` | Linux x86_64 (musl) |
| `fleen-0.0.8-aarch64-linux.tar.gz` | Linux aarch64 (glibc) |
| `fleen-0.0.8-x86_64-macos.tar.gz` | macOS Intel |
| `fleen-0.0.8-aarch64-macos.tar.gz` | macOS Apple Silicon |
| `fleen-0.0.8-x86_64-windows.zip` | Windows x86_64 |

Each archive ships `fleen-vm`, `fleen-compiler`, `fleen-verify`, `flint` and the
version-pinned `std/` tree. (`tar.gz`/`zip` binaries are gitignored under
`release/`.)

## Docs

- `docs/0.0.8/PLAN.md` — decisions D1–D16
- `docs/0.0.8/TICKETS.md` — T01–T11
- `docs/DESIGN.md` §5 / §9 / §10.8 / §17 / §18 — curried declarations, arrow
  types, capture semantics, the both-modes matrix and the roadmap
- `docs/BYTECODE.md` — `PartialApply` (0xCE) and the content-driven v8
- `docs/SYNTAX.ebnf` — `feature_directive` / `curried_signature` / `arrow_type`
- `examples/0.0.8/curry_demo.fln` and `examples/0.0.8/curry_tour/` — runnable tours