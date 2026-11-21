# 16 — Unsafe and Pointers (0.0.5)

0.0.5 introduces the raw pointer layer: `ptr<T>`, `addr`, `deref_ptr`, and the `unsafe` / `trusted` function modifiers. Fleen has no borrow checker for pointers; **every pointer operation is an unsafe operation** and must occur inside an `unsafe` or `trusted` function body.

## 16.1 `unsafe` and `trusted` functions

```fleen
unsafe func raw_op(x: int): int {
    p = addr x
    n = deref_ptr p
    deref_ptr p = n + 1
    n
}

trusted func safe_wrapper(x: int): int = raw_op(x)
```

- `unsafe func`: the body may use `addr`/`deref_ptr` and call other `unsafe` functions. **Calling an `unsafe` function from safe code is a compile-time error** (`UnsafeCallOutsideUnsafe`).
- `trusted func`: the author attests that the unsafe operations inside are correctly encapsulated; a `trusted` function **may be called from anywhere** (including safe code), and its body may also perform unsafe operations.
- Modifier order: both `async unsafe func` and `unsafe async func` are accepted (equivalent, 0.0.5 D4).

## 16.2 The `ptr<T>` type

`ptr<T>` is a raw pointer to `T`.

```fleen
// Only inside an unsafe/trusted function body
unsafe func demo(x: int): int {
    p: ptr<int> = addr x
    deref_ptr p
}
```

`ptr<T>` rules (0.0.5):

| Rule | Meaning |
|------|---------|
| owned, move-only | cannot `clone` (`CloneOfPointer`) |
| **non-storable** | cannot be stored in struct fields, array elements, `box<T>` cells, or globals; may only live in locals, parameters, or return values (`PtrNotStorable`) |
| no boundary crossing | cannot cross `await`, task, or channel boundaries (`PtrAcrossBoundary`, sibling of `RefInAsync`) |

## 16.3 `addr` — take an address

`addr <place>` returns a `ptr<T>` to that place. Only valid inside an `unsafe`/`trusted` function body.

```fleen
unsafe func f(): int {
    x = 10
    p = addr x        // p: ptr<int>
    deref_ptr p
}
```

Restrictions:
- Cannot take the address of an immutable binding (`AddrOfImmutable`). Function parameters are immutable; taking addresses that would create unsafe aliasing is restricted (e.g. `AddrOfRefParam`).
- The operand must be an addressable place (e.g. a local variable).

## 16.4 `deref_ptr` — read/write through a pointer

`deref_ptr <ptr-expr>` reads the pointed-to value (a copy out); `deref_ptr <ptr-expr> = <value>` writes to the pointed-to place. Only valid inside an `unsafe`/`trusted` function body.

```fleen
unsafe func inc(x: int): int {
    p = addr x
    n = deref_ptr p
    deref_ptr p = n + 1
    deref_ptr p
}
```

## 16.5 Known limitations

- An `unsafe func` called *through a function value* may escape the static call check; the VM backstop on `LoadPtr`/`StorePtr` is the last line of defense.
- The non-storable design prevents long-lived aliasing structures, simplifying the 0.0.5 safety model.

## 16.6 Related errors

| Error | Meaning |
|-------|---------|
| `UnsafeCallOutsideUnsafe` | calling an `unsafe` function from safe code |
| `CloneOfPointer` | `clone` on `ptr<T>` |
| `PtrNotStorable` | storing `ptr<T>` in a field/array/box/global |
| `PtrAcrossBoundary` | `ptr<T>` crossing `await`/task/channel boundary |
| `AddrOfImmutable` | taking address of an immutable place |
| `AddrOfRefParam` | taking address of a `ref` parameter (restricted) |
