# 16 — Unsafe and Pointers (0.0.5)

Fleen 0.0.5 adds a pointer/unsafe layer. Fleen has no borrow checker: every pointer operation is an **unsafe operation** and is only allowed inside an `unsafe` or `trusted` function body.

## 16.1 `unsafe` and `trusted` functions

```fleen
unsafe func raw_op(p: ptr<int>) {      // unsafe body; callable only from unsafe/trusted
    deref_ptr p = deref_ptr p + 1
}

trusted func safe_op(x: int): int {    // attested; callable from anywhere
    p = addr x
    raw_op(move p)
    x
}
```

- `unsafe func`: the body may perform unsafe operations. Calling an `unsafe func` from ordinary (safe) code is rejected (`UnsafeCallOutsideUnsafe`).
- `trusted func`: the body is author-attested to uphold safety; it may perform unsafe operations and is callable from safe code.
- Modifiers compose with `async` in either order: `async unsafe func` / `unsafe async func` (calling+awaiting still needs an unsafe/trusted context).
- `unsafe` and `trusted` are reserved words since 0.0.5.

## 16.2 `ptr<T>`, `addr`, and `deref_ptr`

```fleen
unsafe func demo(): int {
    x = 10
    p = addr x              // p: ptr<int> (take address of a place)
    n = deref_ptr p         // read through p (a copy out)
    deref_ptr p = n + 1     // write through p
    n
}
```

- `ptr<T>` is a raw pointer type (recognized in type position). It is **non-Copy** and move-only (`clone p` is `CloneOfPointer`). It is **non-storable** (`PtrNotStorable`): no struct fields, array elements, box cells, or globals — a pointer lives in a local, a parameter, or a return value.
- `addr place` takes the address of a place and yields `ptr<T>`. `addr` rejects immutable bases (`AddrOfImmutable` — parameters are immutable) and `ref`-parameter bases (`AddrOfRefParam`).
- `deref_ptr p` reads a copy out of the referent; `deref_ptr p = v` writes through the pointer. `deref` remains **box-only**.
- 0.0.5 defines `addr` over place forms such as locals, globals, direct struct fields, array elements, and box interiors; supported place forms can vary by toolchain build (see `examples/0.0.5/tour`).

## 16.3 Safety and boundary rules

| Check | Meaning |
|---|---|
| `UnsafeOpOutsideUnsafe` / `UnsafeDerefOutside` | `addr`/`deref_ptr` used outside an unsafe/trusted body |
| `UnsafeCallOutsideUnsafe` | `unsafe func` called from safe code |
| `AddrOfImmutable` / `AddrOfRefParam` | `addr` of an immutable or `ref` base |
| `CloneOfPointer` | `clone` of `ptr<T>` |
| `PtrNotStorable` | `ptr<T>` stored in a field/element/box/global |
| `PtrAcrossBoundary` | `ptr<T>` crosses `await`, a task, or a channel (sibling of `RefInAsync`) |

Known limit (0.0.5): an `unsafe func` called *through a function value* can escape the static call check; the VM backstop on pointer load/store is the last line of defense.

## 16.4 Bytecode and toolchain (0.0.5)

- Bytecode v5: `AddrOf` 0xC8 / `LoadPtr` 0xC9 / `StorePtr` 0xCA; readers accept versions 1–5. `Value::Ptr` is not serialized.
- CLI (D6): clap-based `--help`/`-V`/`--project`/`--quiet`/`--no-color`, exit codes 0/1/2.
- Release (D7): six-target `v*` tag pipeline (gnu/musl Linux, macOS x86_64+aarch64, Windows x86_64).
- Std distribution (D8): `FLINT_STD_PATH` → `$FLINT_HOME/std/<ver>` → embedded (strict version pinning).

## 16.5 Example

See [examples/0.0.5/tour](../../../examples/0.0.5/tour) (`addr`/`deref_ptr`, local/global places, `unsafe`/`trusted`, move-only `ptr<T>`, `async unsafe` + `PtrAcrossBoundary` note), verified with the 0.0.5 compiler.
