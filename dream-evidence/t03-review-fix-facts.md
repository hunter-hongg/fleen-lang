# 0.0.7 T03 review→fix session — verified facts (2026-11-10 evening)

Session: `/code-review 审查0.0.7 T03实现` then user said "全修掉" (fix them all).
Fix commit: `30ed027 fix: resolve the 0.0.7 T03 code-review findings` (parent `d02daac`).
All facts below were directly observed in tool output during this session; no secret material included.

## 1. Review findings → fixes (commit 30ed027, verified green)

- Spec axis found: duplicate-type-param `DuplicateBinding.first_span` pointed at the
  ENUM head (`e.span`) instead of the first parameter occurrence — root cause was T01
  making `ast::EnumDecl.type_params: Vec<String>` (no per-parameter spans).
  Fix: `type_params` became `Vec<TypeParam> { name, span }`; parser
  (`parse_enum_type_params`) records the identifier span; resolver diagnostic now passes
  first-occurrence span as `first_span` and the duplicate's own span as the error span.
  Recorded as an as-built deviation from T01's card text in `docs/0.0.7/TICKETS.md`.
- Standards axis found: the duplicate-name scan was triplicated in `collect_enums`;
  the four registration sites (module items, scope bindings, type params, variants) now
  share `Resolver::push_duplicate(name, first_span, span)`.
- `Resolver::type_param_scope` inline set/clear (fragile against early returns) is now
  installed ONLY inside `Resolver::resolve_enum_variants(params, variants)` — single
  setter, cleared before return, `debug_assert!` guards emptiness outside the window.
- `exports_of_ast`'s six-element positional tuple replaced by `ExportDef::simple` /
  `ExportDef::enum_export` constructors; public fields unchanged so hand-built test
  `ExportDef` literals still compile.
- New coverage: fixture `tests/resolver/invalid/param_type_param_not_a_value.fln` +
  tests `param_type_param_not_in_binding_scope` (UndeclaredVariable) and
  `param_enum_duplicate_type_param_points_at_first_occurrence` (first_span has width and
  differs from the error span).

## 2. Gates after the fix (observed)

- `cargo fmt -- --check` clean; `cargo clippy -p fleen-compiler --all-targets` 0 warnings.
- `cargo test -p fleen-compiler`: all suites green — 708 lib + 117 resolver_integration
  (was 115 before this session; +2 new tests) + the rest; invalid-fixture harness picks up
  the new fixture automatically (directory scan, no registration needed).
- Full-workspace `cargo test`: 4 flint-cli e2e failures ONLY — all
  `could not create work tree dir '/home/hunter/.flint/...': Read-only file system`
  (same environmental issue as `t03-session-facts.md` §1; FLINT_HOME temp-dir workaround
  documented there). Unrelated to compiler changes.

## 3. Harness quirk observed in this session

- `subagent`/`subagent_fork` return ids immediately; the children are absent from
  `list_agents` (only the Lead shows) and `wait_agent` returns
  `noProgress: no-active-peer` right away. Their reports arrive later as background
  completion messages. Practical consequence: don't block on wait_agent for subagent
  children; the review report arrives as a message on a later turn.

## 4. Deferred by design (documented, not a defect)

- PLAN §5 Resolve row says use-site applications "check arity"; the ticket assigns that
  check to typeck (T04). Resolve only carries arity (`ExportDef.type_param_count`);
  nothing reads it yet. Noted in the T03 card's as-built block so T04 implements it.
