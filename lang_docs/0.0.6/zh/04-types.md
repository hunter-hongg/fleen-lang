# 04 — 类型

每个值都有静态确定的类型。类型在程序运行前检查；类型之间没有隐式转换。

## 4.1 基本类型

| 类型 | 取值 | 示例 |
|------|------|------|
| `int` | 64 位有符号整数 | `42`、`0 - 7` |
| `float` | 64 位浮点数 | `3.14` |
| `bool` | `true`、`false` | `true` |
| `string` | UTF-8 文本 | `"hello"` |
| `unit` | 唯一的空值 | `print(...)` 的结果 |

```fleen
i: int = 42
f: float = 3.14
b: bool = true
s: string = "hello"
```

`unit` 是表达式不携带信息时的类型，例如无返回值函数的返回值：

```fleen
func greet(name: string) {
    print("Hello, ", name);       // greet 返回 unit
}
u: unit = greet("ada");
```

## 4.2 函数类型

函数类型写作 `(T1, T2) -> R`：

```fleen
func add(a: int, b: int): int = a + b

func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)

r = apply(add, 1, 2);      // r = 3
```

无参数函数类型为 `() -> R`，无返回值函数类型为 `(…) -> unit`。函数是一等值，可以绑定、作为参数传递、作为返回值。函数值是 Copy 类型。

## 4.3 数组 `[T]`

`[T]` 是元素类型为 `T` 的数组：

```fleen
a = [1, 2, 3];              // a: [int]
names = ["ada", "bob"];     // names: [string]
empty: [int] = [];          // 空字面量必须标注类型
```

用 `[i]` 读写元素，所有索引在运行时都有边界检查：

```fleen
x = a[0];        // x = 1
a[1] = 10;       // a 变为 [1, 10, 3]
```

读取元素会产生元素的 **拷贝**。越界（包括负数）是运行时错误 `IndexOutOfRange`，而不是环绕访问。

数组即使元素是 Copy 类型也始终是 **owned** 值：整体会被 move 或深拷贝（第 08 章）：

```fleen
t = clone a;     // 深拷贝：新数组和新元素
u = move a;      // 转移：a 随后不可用
```

元素类型必须一致：

```fleen
bad = [1, "a"];        // 错误：ArrayTypeMismatch
```

## 4.5 `enum`（0.0.6）

`enum` 声明一个标签联合类型：一组有序变体，每个变体可选地携带带类型的载荷。

```fleen
enum Shape {
    Point,
    Circle(float),
    Rect(float, float),
}
```

- `enum` 是保留字（0.0.6）。
- 变体名是构造器：`Shape.Circle(2.0)`（点号形式）或 `Circle(2.0)`（无歧义时可裸写）。有载荷的变体省略括号时不是构造器。
- `is_copy(E)` 当且仅当**所有变体的所有载荷**都是 Copy；否则为 owned（D1）。
- 递归枚举在编译期被拒绝（`RecursiveEnum`）。
- 枚举不支持 `==`/`!=`，也不能 `as string`。
- 匹配：当每个变体都有分支时 `choose` 穷尽（不需要 `otherwise`，D3）。0.0.6 的模式只绑定**第一个载荷**（`when Rect(w)`；`when Rect(w, h)` 是语法错误）。
- 在 `choose` 中消费 owned 枚举应使用 `choose move <expr>`。

```fleen
func area(s: Shape): float {
    choose move s {
        when Point { 0.0 }
        when Circle(r) { 3.14159 * r * r }
        when Rect(w) { w * 0.0 }
    }
}
```

## 4.5 Range

`Range` 是表示整数区间（右端开）的内建类型。通过 `std.collection.range` 中的构造函数创建（导入后可裸用，第 13 章）：

```fleen
import std.collection.range;

r = range(3);          // 区间 0..3（0, 1, 2）
r2 = range_from(2, 5); // 区间 2..5（2, 3, 4）
```

没有 `..` 运算符语法。`Range` 用于 `for`（第 06 章 §6.3），且是 Copy 类型：

```fleen
for i in range(3) {
    print(i as string);      // 0 1 2
};
```

当 start == end 时区间为空。

## 4.5 struct

`struct` 是用户定义的聚合类型。字段用逗号分隔，字面量必须初始化全部字段：

```fleen
struct Point {
    x: int,
    y: int,
}

p = Point { x: 1, y: 2 };
n = p.x;        // 字段读取（Copy 字段复制值；owned 字段复制/转移遵循所有权）
p.y = 3;        // 字段写入（仅限局部/全局变量的直接字段）
```

`struct` 的复制性由字段决定：如果所有字段都是 Copy 类型，则 `struct` 是 Copy 类型；只要有任何 owned 字段，`struct` 就是 owned 类型。

```fleen
struct Pair { a: int, b: int }           // Copy
struct Wrap { s: string }               // owned
```

跨模块使用需 `pub`（第 12 章）。


## 4.7 box\<T\>

`box<T>` 是堆分配的单一所有者单元格：

```fleen
b = box 42;             // b: box<int>
n = deref b;            // 读取：42（拷贝）
deref b = n + 1;        // 写入：单元格变为 43
c = clone b;            // 新 box，内层值也被拷贝
```

`deref b` 产生内层值的拷贝。允许的内层类型：`int`、`float`、`bool`、`string`、`box<U>`、`Result<T, E>`。不允许从 box 中 `move deref b` 取出值（使用 `clone deref b` 获取 owned 拷贝）。详见第 08 章 §8.4。

## 4.8 ref T

`ref T` 是只读借用，**仅**允许用作函数参数类型（第 08 章 §8.5）：

```fleen
func shout(s: ref string): int {
    print(s);           // 只读使用借用的字符串
    42
}

name = "fleen";
r = shout(name);        // 隐式借用，不转移所有权
```

## 4.9 Result\<T, E\>

`Result<T, E>` 表示成功 `Ok(value)` 或失败 `Err(error)`：

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

q = div(10, 2)?;        // 在 Result 返回函数中传播错误（第 09 章）
```

`Result` 是 owned 类型（除非 `T` 和 `E` 都是 Copy 且不持有 owned 成分的特例）。构造器 `Ok` 和 `Err` 是保留形式。用 `?` 运算符传播，用 `choose` 匹配（第 09 章）。

## 4.10 Future\<T\>

`async func` 返回 `Future<T>`。只能用 `await`（后缀运算符）驱动，驱动后得到 `T`：

```fleen
async func half(n: int): int { n / 2 }

r = half(10) await;     // r: int
```

`Future<T>` 是 owned、非 Copy 类型，且不可跨 `await`/任务/通道边界存储或转移到 async 世界之外（第 10 章）。

## 4.11 channel\<T\>

`channel<T>` 是并发任务间的通信端点：

```fleen
ch = channel<int>(4);        // 容量为 4 的 int 通道
```

`channel<T>` 由保留构造器 `channel<T>(cap)` 创建，只能 move（不能 `clone`），是 owned、非 Copy 类型（第 11 章）。

## 4.12 ptr\<T\>（0.0.5，unsafe）

`ptr<T>` 是原始指针，仅用于 `unsafe`/`trusted` 函数体内。相关操作：`addr x`（取地址）、`deref_ptr p`（解引用读写）。

规则（第 16 章）：
- `ptr<T>` 是非 Copy、只能 move 的类型，且 **不可存储**（不能放在 struct 字段、数组元素、`box` 单元格或全局变量中）。
- `ptr<T>` **不可跨越** `await`/任务/通道边界。
- 只能对可取地址的左值使用 `addr`；参数是不可变的（`AddrOfImmutable`）、`ref` 参数（`AddrOfRefParam`）等情况被拒绝。
- 每次使用指针都是 unsafe 操作，必须在 `unsafe` 或 `trusted` 函数体内。

```fleen
unsafe func raw_op(x: int): int {
    p = addr x;            // p: ptr<int>
    n = deref_ptr p;
    deref_ptr p = n + 1;
    n
}
```

## 4.13 Copy 与 owned 类型（总结）

每个类型要么是 **Copy**，要么是 **owned**。这决定赋值、传参行为（第 08 章）：

| 类别 | 类型 | 语义 |
|------|------|------|
| Copy | `int`、`float`、`bool`、`unit`、函数值、`Range`、所有字段均为 Copy 的 `struct`、所有载荷均为 Copy 的 `enum` | 隐式拷贝 |
| owned | `string`、`[T]`（始终）、`box<T>`、含有任何 owned 字段的 `struct`、含有 owned 成分的 `Result<T,E>`、`Future<T>`、`channel<T>`、含有 owned 载荷的 `enum`、`ptr<T>`（非 Copy，只能 move） | 单一所有者；`move` 转移，`clone` 深拷贝 |

`ref T` 既非 Copy 也非 owned，它是借用，只能用作参数类型。

## `ptr<T>`（0.0.5）

`ptr<T>` 是原始指针类型：owned、move-only、**non-storable**（不能存于字段/数组元素/box/全局），且不能跨越 `await`/任务/通道边界。创建或使用指针（`addr`、`deref_ptr`）只能在 `unsafe` 或 `trusted` 函数体内。见[第 16 章](16-unsafe-and-pointers.md)。
