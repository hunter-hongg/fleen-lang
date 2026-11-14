# 08 — 所有权

本章描述 Fleen 值的所有权规则。

## 8.1 Copy 类型与 owned 类型

| 类型 | 类别 | 说明 |
|------|------|------|
| `int` | Copy | 标量，赋值/传递时按值复制 |
| `float` | Copy | 标量 |
| `bool` | Copy | 标量 |
| `unit` | Copy | 标量 |
| `string` | owned | 赋值/传递会移动所有权 |
| `[T]` | owned | 对所有 `T` 都是 owned（包括 `[int]`） |
| `box<T>` | owned | 堆分配的所有者 |
| `struct` | 取决于字段 | 所有字段都是 Copy → Copy；否则 owned |
| `enum` | 取决于载荷 | 所有变体的所有载荷都是 Copy → Copy；否则 owned（0.0.6） |
| `Range` | Copy | 内建标量对 |
| `Result<T, E>` | 取决于 `T`/`E` | |
| `Future<T>` | owned | `await` 恰好消费一次（0.0.4） |
| `channel<T>` | owned, move-only | `clone` 是错误（0.0.4） |
| `ptr<T>` | owned, move-only, non-storable | 只能在 `unsafe`/`trusted` 中使用（0.0.5） |
| 函数值 | Copy | |

Copy 类型的变量可以自由使用多次；owned 类型的变量在移动后不可再用。

## 8.2 `move`

`move` 将 owned 值的所有权转移给新的绑定或调用点。移动后原变量不可再用。

```fleen
s = "hello"
t = move s     // s 不再可用
// print(s)    // 错误：s 已被移动
print(t)
```

对 Copy 类型使用 `move` 是无意义的（会产生错误或警告）。

## 8.3 `clone`

`clone` 创建 owned 值的深拷贝。原值保持可用。

```fleen
s = "hello"
t = clone s    // s 仍然可用
print(s)
print(t)
```

`channel<T>` 不支持 `clone`（0.0.4）；`ptr<T>` 不支持 `clone`（0.0.5）。

## 8.4 `box<T>` 和 `deref`

`box` 在堆上分配值，返回 `box<T>`（owned）。

```fleen
b = box 42        // b: box<int>
n = deref b       // 读出内部值的副本
deref b = n + 1   // 写入内部值
```

## 8.5 `ref` 参数（只读借用）

函数参数可以标记为 `ref`，表示只读借用（不转移所有权，调用者仍可继续使用该值）。

```fleen
func shout(s: ref string): int {
    print(s)
    42
}

msg = "hello"
shout(msg)       // msg 仍然可用（借用）
```

`ref` 只允许出现在函数参数上，不能用于局部变量、返回值、字段或全局绑定。

## 8.6 借用与异步边界（0.0.4）

`ref` 不能跨越 `await`、任务边界或通道边界传递；异步函数体中不能持有 `ref` 参数跨越 `await` 点。通道消息以所有权移动传递。

## 8.7 指针与不安全边界（0.0.5）

`ptr<T>` 是 owned、move-only、non-storable 类型：
- 不能存储在 struct 字段、数组元素、`box` 单元或全局变量中；
- 只能存在于局部变量、参数或返回值中；
- 不能跨越 `await`、任务边界或通道边界传递（`PtrAcrossBoundary`）。

`addr` / `deref_ptr` 只能在 `unsafe` 或 `trusted` 函数体内使用；从安全代码调用 `unsafe` 函数是错误（`UnsafeCallOutsideUnsafe`）。详见第 16 章。
