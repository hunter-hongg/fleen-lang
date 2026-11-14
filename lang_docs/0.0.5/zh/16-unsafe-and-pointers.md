# 16 — 不安全与指针（0.0.5）

Fleen 0.0.5 增加了指针/unsafe 层。Fleen 没有借用检查器：每个指针操作都是**不安全操作**，只能在 `unsafe` 或 `trusted` 函数体内进行。

## 16.1 `unsafe` 与 `trusted` 函数

```fleen
unsafe func raw_op(p: ptr<int>) {      // 不安全体；只能从 unsafe/trusted 调用
    deref_ptr p = deref_ptr p + 1
}

trusted func safe_op(x: int): int {    // 已证明封装；可从安全代码调用
    p = addr x
    raw_op(move p)
    x
}
```

- `unsafe func`：函数体可执行不安全操作。从普通（安全）代码调用 `unsafe func` 会被拒绝（`UnsafeCallOutsideUnsafe`）。
- `trusted func`：作者声明维护安全性；可执行不安全操作，且可从安全代码调用。
- 可与 `async` 以任意顺序组合：`async unsafe func` / `unsafe async func`（调用并 `await` 仍需 unsafe/trusted 上下文）。
- `unsafe`、`trusted` 自 0.0.5 起为保留字。

## 16.2 `ptr<T>`、`addr` 与 `deref_ptr`

```fleen
unsafe func demo(): int {
    x = 10
    p = addr x              // p: ptr<int>（取位置地址）
    n = deref_ptr p         // 通过指针读取（拷贝出）
    deref_ptr p = n + 1     // 通过指针写入
    n
}
```

- `ptr<T>` 是原始指针类型（在类型位置识别）。**非 Copy**、仅 move（`clone p` 报 `CloneOfPointer`）。**不可存储**（`PtrNotStorable`）：不能作为结构体字段、数组元素、box 单元或全局变量，只能存在于局部变量、参数或返回值。
- `addr place` 取得位置地址并返回 `ptr<T>`。`addr` 拒绝不可变基址（`AddrOfImmutable`——参数不可变）和 `ref` 参数基址（`AddrOfRefParam`）。
- `deref_ptr p` 读出拷贝；`deref_ptr p = v` 通过指针写入。`deref` 仍**仅用于 box**。
- 0.0.5 定义了对局部变量、全局变量、直接结构体字段、数组元素、box 内部等位置形式的 `addr`；具体构建对位置形式的支持可能不同（参见 `examples/0.0.5/tour`）。

## 16.3 安全与边界规则

| 检查 | 含义 |
|---|---|
| `UnsafeOpOutsideUnsafe` / `UnsafeDerefOutside` | 在 unsafe/trusted 体外使用 `addr`/`deref_ptr` |
| `UnsafeCallOutsideUnsafe` | 在安全代码中调用 `unsafe func` |
| `AddrOfImmutable` / `AddrOfRefParam` | 对不可变基址或 `ref` 参数取址 |
| `CloneOfPointer` | 对 `ptr<T>` 执行 `clone` |
| `PtrNotStorable` | 将 `ptr<T>` 存入字段/元素/box/全局 |
| `PtrAcrossBoundary` | `ptr<T>` 跨越 `await`、任务或 channel（与 `RefInAsync` 并列） |

已知限制（0.0.5）：通过**函数值**调用 `unsafe func` 可能绕过静态调用检查；VM 对指针 load/store 的后端检查是最后防线。

## 16.4 字节码与工具链（0.0.5）

- 字节码 v5：`AddrOf` 0xC8 / `LoadPtr` 0xC9 / `StorePtr` 0xCA；读端兼容 1–5。`Value::Ptr` 不序列化。
- CLI（D6）：clap 风格 `--help`/`-V`/`--project`/`--quiet`/`--no-color`，退出码 0/1/2。
- 发布（D7）：6 目标 `v*` tag 流水线（gnu/musl Linux、macOS x86_64+aarch64、Windows x86_64）。
- std 分发（D8）：`FLINT_STD_PATH` → `$FLINT_HOME/std/<ver>` → embedded（严格版本固定）。

## 16.5 示例

参见 [examples/0.0.5/tour](../../../examples/0.0.5/tour)（`addr`/`deref_ptr`、局部/全局位置、`unsafe`/`trusted`、仅 move 的 `ptr<T>`、`async unsafe` 与 `PtrAcrossBoundary` 说明），已用 0.0.5 编译器验证。
