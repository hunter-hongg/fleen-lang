# 0.0.6 T05 — evidence

## Commit

`e65391c feat(typeck,lower): choose on enums with exhaustiveness (0.0.6 T05)`
(preceded by `414b4ef` = T04).

## Verified facts

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --all-targets -- -D warnings` — clean (two `collapsible_if`
  findings were fixed with a let-chain).
- `cargo test` — green across the workspace, except 4 pre-existing
  `flint-cli` e2e failures that need git network access. Verified unrelated:
  `git stash` + `cargo test -p flint-cli` reproduces the same 4 failures
  without this branch's changes.
- New tests: 6 typeck unit tests (exhaustive / missing variant /
  guarded-only / otherwise short-circuit / payload binding type / foreign
  variant), 4 lower unit tests (2-variant `EnumIs` order, 3-variant order,
  guard conditional jump, block termination), 2 fixtures
  (`tests/typeck/valid/enum_choose_exhaustive.fln`,
  `tests/typeck/invalid/enum_choose_not_exhaustive.fln`).

## Defect 1 — HIR gap: the pattern binding name was missing

`PatternHir::Enum` (added in 0.0.6 T02) carried only the **variant** name.
The payload binding's identifier was never carried into HIR, so typeck
declared the *variant* name (`Circle`) in the arm scope instead of the
binding name (`r`). The first T05 test run failed with
`UndefinedVariable { name: "r" }` when the guard read the payload.

Fix: `PatternHir::Enum` gained a `binding_name: String` field (the shape
`PatternHir::ResultCtor` already had); the resolver fills it and typeck
declares the binding name.

Files: `fleen-compiler/src/resolver/hir.rs`, `fleen-compiler/src/resolver/mod.rs`,
`fleen-compiler/src/typeck/infer.rs`.

## Defect 2 — `EnumPayload` consumes the enum

`EnumPayload` is `e -> p` (the `UnwrapOk` analogue): it *consumes* the enum
and pushes the payload. A payload arm therefore needs no `Pop` before it.
The first draft emitted a `Pop` and then `EnumPayload`, dropping the
scrutinee before the payload was read.

## Block layout rule (the constraint that mattered)

`FnBuilder`'s conditional terminators take the branch on the target and
**fall through to the physically next block** otherwise (BYTECODE §6.3).
So block allocation order *is* the CFG. The enum choose lowering therefore
allocates every block up front — test chain, fallback, then body blocks —
so every edge can be resolved immediately with no deferred jumps.

## Out of scope (deliberate)

Reading a variant's 2nd/3rd payload inside an arm body: the pattern grammar
binds one payload and no user-visible "read payload i" expression exists
yet. A 2+ payload variant matches, but only its first payload is reachable.
