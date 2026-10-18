# 0.0.3 V06 std Library Tests

These `.fln` files exercise the std library directly as part of `cargo test`.
Each file imports its target module(s), runs the functions, and prints
results to stdout. The `.expected` files provide the required output.

`iter.fln` + `iter_helpers.fln` are deferred to V07 (iterator helpers).

## Test Discovery

These are not auto-discovered by `fleen-vm/tests/e2e.rs` (which only scans
`tests/e2e/{valid,invalid}/`). Instead:

- `fleen-vm/src/tests.rs` has a `std_e2e_via_compile` test that compiles
  and runs each `.fln` in this directory through the full compiler + VM
  pipeline and checks stdout against the `.expected` file.

## Files

| File | Module | Coverage |
|------|--------|----------|
| `io.fln` | `std.io` | `println`, `read_line` (EOF → empty), `print` (prelude + qualified) |
| `str.fln` | `std.str` | `len`, `concat`, `split`, `from_int`, `from_float`, `from_bool` |
| `num.fln` | `std.num` | `abs`, `min`, `max`, `sqrt`, `floor`, min-int overflow |
| `arr.fln` | `std.arr` | `len`, `get` (Ok/Err), `set`, `push` (schematic `[T]`) |
| `os.fln` | `std.os` | `args`, `exit` (exit code propagation) |
| `time.fln` | `std.time` | `now_millis` (positive sanity) |
| `range.fln` | `std.collection.range` | `range(n)`, `range_from(a, b)`, `for` over Range |
| `prelude.fln` | (prelude) | `print` without import |
