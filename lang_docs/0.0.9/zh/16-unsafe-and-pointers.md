# 16 — 不安全与指针（0.0.5）

0.0.5 引入了原始指针层：`ptr<T>`、`addr`、`deref_ptr`，以及 `unsafe` / `trusted` 函数修饰符。Fleen 没有借用检查器覆盖指针；**每一次指针操作都是不安全操作**，必须出现在 `unsafe` 或 `trusted` 函数体内。

## 16.1 `unsafe` 与 `trusted` 函数

```fleen
unsafe func raw_op(x: int): int {
    p = addr x
    n = deref_ptr p
    deref_ptr p = n + 1
    n
}

trusted func safe_wrapper(x: int): int = raw_op(x)
```

- `unsafe func`：函数体内允许使用 `addr`/`deref_ptr`，并可调用其他 `unsafe` 函数。**从安全代码直接调用 `unsafe` 函数是编译错误**（`UnsafeCallOutsideUnsafe`）。
- `trusted func`：作者声明该函数体内的不安全操作已被正确封装；`trusted` 函数**可从任何地方调用**（包括安全代码），且其函数体内也允许不安全操作。
- 修饰符顺序：支持 `async unsafe func` 与 `unsafe async func`（两种顺序等价，0.0.5 D4）。

## 16.2 `ptr<T>` 类型

`ptr<T>` 是指向 `T` 的原始指针类型。

```fleen
// 只能在 unsafe/trusted 函数体内创建和使用
unsafe func demo(x: int): int {
    p: ptr<int> = addr x
    deref_ptr p
}
```

`ptr<T>` 的规则（0.0.5）：

| 规则 | 说明 |
|------|------|
| owned, move-only | 不能 `clone`（`CloneOfPointer`） |
| **non-storable** | 不能存储在 struct 字段、数组元素、`box<T>` 单元或全局变量中；只能存在于局部变量、参数或返回值（`PtrNotStorable`） |
| 不跨边界 | 不能跨越 `await`、任务边界或通道边界传递（`PtrAcrossBoundary`，与 `RefInAsync` 同类） |

## 16.3 `addr` — 获取地址

`addr <place>` 返回指向该位置的 `ptr<T>`。只能在 `unsafe`/`trusted` 函数体内使用。

```fleen
unsafe func f(): int {
    x = 10
    p = addr x        // p: ptr<int>
    deref_ptr p
}
```

限制：

- 不能对不可变绑定取址（`AddrOfImmutable`）。函数参数是不可变的，因此不能对 `ref` 参数或普通参数（按实现约束）取址导致不安全别名；对参数取址受限（`AddrOfRefParam` 等）。
- `addr` 的操作数必须是可寻址的“位置”（place），例如局部变量。

## 16.4 `deref_ptr` — 通过指针读写

`deref_ptr <ptr-expr>` 读取指针指向的值（返回副本）；`deref_ptr <ptr-expr> = <value>` 写入指针指向的位置。只能在 `unsafe`/`trusted` 函数体内使用。

```fleen
unsafe func inc(x: int): int {
    p = addr x
    n = deref_ptr p
    deref_ptr p = n + 1
    deref_ptr p
}
```

## 16.5 已知限制

- 通过函数值间接调用 `unsafe func` 时，静态调用检查可能无法拦截；VM 对 `LoadPtr`/`StorePtr` 的后端检查是最后一道防线。
- `ptr<T>` 不可存储的设计使其无法构成长期别名结构；这简化了 0.0.5 的安全模型。

## 16.6 相关错误

| 错误 | 含义 |
|------|------|
| `UnsafeCallOutsideUnsafe` | 从安全代码调用 `unsafe` 函数 |
| `CloneOfPointer` | 对 `ptr<T>` 使用 `clone` |
| `PtrNotStorable` | 将 `ptr<T>` 存入字段/数组/box/全局 |
| `PtrAcrossBoundary` | `ptr<T>` 跨越 `await`/任务/通道边界 |
| `AddrOfImmutable` | 对不可变位置取址 |
| `AddrOfRefParam` | 对 `ref` 参数取址（受限） |
