# Evidence: enum/struct name collision silently accepted (0.0.6 T03)

## Symptom
`enum S { A }` + `struct S { a: int }` compiled with exit=0 (no error) in BOTH
declaration orders, though docs/0.0.6/TICKETS.md T03 requires enum-vs-struct name
conflicts to be a registration-time `DuplicateBinding` compile error.

Probed via `cargo run -q -p fleen-vm -- <file>`:
  [struct-then-enum]    exit=0   (bug)
  [enum-then-struct]    exit=0   (bug)
  [enum-vs-func]        exit=1   (correct — via scopes.get path)
  [dup-variant]         exit=1   (correct)

## Root cause
`exports_of_ast` (fleen-compiler/src/resolver/modules.rs:183-247) inserts into a
`HashMap` last-wins: one entry per name. `collect_enums`
(fleen-compiler/src/resolver/mod.rs:1557) checked only that map:
  - struct first, enum later -> map holds struct -> enum check fires (ok)
  - enum first, struct later -> map holds enum (kind==Enum) -> enum check passes;
    the struct pre-pass (mod.rs:281) never consults enums -> silent pass
A lossy last-wins name table cannot catch a same-name clash in both orders.

## Fix
collect_enums now also cross-checks the raw AST structs directly (mod.rs:1549+),
catching both orders in one place. After fix:
  [struct-then-enum]    exit=1   DuplicateBinding
  [enum-then-struct]    exit=1   DuplicateBinding
  [valid enum decl]     exit=0   (no regression)
Regression test: fleen-compiler/tests/resolver_integration.rs
  enum_name_clashes_with_struct_is_error
