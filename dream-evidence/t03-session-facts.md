# 0.0.7 T02/T03 implementation session — verified facts (2026-11-10)

Session: T02 commit + T03 implementation (`git log`: d8adf74 parser T02, d02daac resolver T03).
All facts below were directly observed in tool output during this session; no secret material included.

## 1. flint-cli e2e needs a writable FLINT_HOME (re-confirmed, workaround verified)

- Same failure as documented in t07-review-env-quirks.md §3: plain `cargo test` → 4 flint-cli
  e2e failures, panic `could not create work tree dir '/home/hunter/.flint/...': Read-only file system`.
- Verified workaround in this session: run the suite with a writable temp home —
  `export FLINT_HOME=$(mktemp -d /tmp/flinthome.XXXXXX)` → full `cargo test` green (0 failures).

## 2. An `import` declaration produces one HIR item

- Reproduced: `resolve_str("import std.io;")` → `hir.items.len() == 1`.
- A module source `import lib; func f(...)...` therefore resolves to **2** HIR items
  (import + func). Hand-built cross-module test `param_enum_import_resolves_cross_module`
  initially asserted `items.len() == 1` and failed with `left: 2, right: 1`; fixed by
  asserting 2.
- Lesson for future module-level tests: count the import decl itself when asserting on
  `hir.items.len()`.

## 3. T03 → T04 handoff facts (commit d02daac)

- Resolver field `Resolver.type_param_scope: Vec<String>` (declaration-local enum param
  scope, set in `collect_enums` around payload-type resolution, cleared after; params do
  NOT enter binding scope).
- `resolve_type_name_str` bare-name path: param-name match returns the name unmangled
  BEFORE the module.items lookup — a payload `T` shadows a module item `T`.
  Typeck T04 must map such `Named("T")` → `Type::Param(i)`.
- `ExportDef` gained `pub type_param_count: usize` (declared arity; 0 for non-enum /
  monomorphic), filled by `exports_of_ast` from the raw AST head — cross-module use-site
  arity checking (T04, per TICKETS deferral) reads it from there.
- Duplicate type params in an enum head → `ResolveError::DuplicateBinding`.
