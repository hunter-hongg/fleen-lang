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

函数类型有两种拼写。传统拼写写作 `(T1, T2) -> R`——参数列表加返回类型：

```fleen
func add(a: int, b: int): int = a + b

func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)

r = apply(add, 1, 2);      // r = 3
```

**柯里化箭头拼写（0.0.8）**：开启 `#feature curry` 的文件中，函数类型写作
`T1 -> T2 -> R`——`->` **右结合**，`int -> int -> int` 是一个有两个参数的函数的
类型，与 `(int, int) -> int` 完全等价；传统拼写在标志开启的文件中依然合法，
两者规范化为同一个函数类型。参数一侧的括号保留分组：`(int -> int) -> int`
接收一个函数，`((int, int) -> int) -> int` 接收一个两参数函数。`() -> T` 是
**零参数**函数类型，`unit -> T` 是**一个**类型为 `unit` 的参数——两者不同。

```fleen
#feature curry

add: int -> int -> int
add(a, b) = a + b

inc: int -> int
inc(n) = n + 1

// 参数一侧的括号保留分组：(int -> int) -> int 接收一个函数
apply_twice: (int -> int) -> int -> int
apply_twice(f, x) = f(f(x))

t = apply_twice(inc, 1);    // t = 3
```

`() -> T` 是**零参数**函数类型，`unit -> T` 是**一个**类型为 `unit` 的参数——
两者是不同的类型。函数是一等值，可以绑定、作为参数传递、作为返回值。函数值是
Copy 类型。

> **标志门控（D16）**：箭头拼写**只在 `#feature curry` 开启的文件中是类型语法**。
> 标志关闭的文件保持 0.0.7 的类型语法原样，`int -> int` 在那里仍是旧的解析错误；
> 标志开启但从未使用箭头类型的文件不受影响。诊断信息按所在文件的标志渲染——
> 标志开启的文件报 `int -> int -> int`，标志关闭的文件报 `(int, int) -> int`。

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

## 4.4 `enum` 与类型参数（0.0.6 / 0.0.7）

`enum` 声明一个标签联合类型：一组有序变体，每个变体可选地携带带类型的载荷。
0.0.7 起，`enum` 头部还可以声明**类型参数**，成为类型构造器：

```fleen
enum Shape {
    Point,
    Circle(float),
    Rect(float, float),
}

enum Option<T> {
    Some(T),
    None,
}

enum Pair<A, B> {
    P(A, B),
}
```

- `enum` 是保留字（0.0.6）。
- 变体名是构造器：`Shape.Circle(2.0)`（点号形式）或 `Circle(2.0)`（无歧义时可裸写）。有载荷的变体省略括号时不是构造器。
- **类型参数**（0.0.7，D1）只存在于声明内部：它们不是值，只在载荷类型中被引用，并遮蔽同名的外层项。每个使用点都必须**完整应用**类型实参——裸写 `Option` 出现在类型位置是 `TypeConstructorNeedsArgs`，实参过多是 `ArityMismatch`。
- **两种使用点拼写**（0.0.7，D9）等价：尖括号 `Option<int>` 与方括号 `Pair[int, string]`（内建 `Result[T, E]` 的风格）；构造器位置由实参类型推断参数。
- **`is_copy` 按类型实参判定**（0.0.7，D2）：`Option<int>` 是 Copy（实参 Copy），`Option<string>` 是 owned；`Pair[int, int]` 是 Copy，`Pair[int, string]` 是 owned。非参数化枚举沿用 0.0.6 规则（所有载荷 Copy → Copy）。
- **递归枚举在编译期被拒绝**（`RecursiveEnum`），对参数化载荷同样生效：`enum List<T> { Cons(T, List<T>) }` 是编译错误（0.0.7，D3）。
- 枚举不支持 `==`/`!=`，也不能 `as string`。
- 匹配：当每个变体都有分支时 `choose` 穷尽（不需要 `otherwise`，D3）。0.0.7 的模式支持**完整多载荷解构** `when Rect(w, h)`、**嵌套模式** `when Ok(Some(x))`、**或模式** `when 1 | 2` 与**类型守卫** `when v: int`（第 06 章 §6.4）。
- 在 `choose` 中消费 owned 枚举应使用 `choose move <expr>`；`Option<int>` 这类 Copy 枚举不需要。

```fleen
func area(s: Shape): float {
    choose move s {
        when Point { 0.0 }
        when Circle(r) { 3.14159 * r * r }
        when Rect(w, h) { w * h }
    }
}

func get(o: Option<int>): int {
    choose o {          // Option<int> 是 Copy，无需 move
        when Some(v) { v }
        when None { 0 - 1 }
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

## 4.6 struct

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

**移入 std 声明（0.0.10，D1）**：`Result` 现在是一个真实声明的参数化枚举——
`std/result.fln` 声明 `pub enum Result<T, E> { Ok(T), Err(E) }`，是语言层的
事实来源。它的行为与 0.0.7 的参数化枚举（第 04 章 §4.4）一致，但无需导入：
`Result`、`Ok`、`Err` 是每个文件都可用的预导入名（第 09 章 §9.1、第 13 章
§13.10）。编译器把 `std.result.Result` 按规范路径映射到内部表示，运行时
编码不变——该声明不产生 `enum_defs` 表项。

`Result` 是 owned 类型（除非 `T` 和 `E` 都是 Copy 且不持有 owned 成分的特例）。构造器 `Ok` 和 `Err` 是保留形式（且不可遮蔽，第 09 章 §9.1）。用 `?` 运算符传播，用 `choose` 匹配（第 09 章）。它的类型实参同样接受两种拼写：`Result<int, string>` 与 `Result[int, string]`。

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
| Copy | `int`、`float`、`bool`、`unit`、函数值、`Range`、所有字段均为 Copy 的 `struct`、所有载荷均为 Copy 的 `enum`（参数化枚举按实参判定，0.0.7） | 隐式拷贝 |
| owned | `string`、`[T]`（始终）、`box<T>`、含有任何 owned 字段的 `struct`、含有 owned 成分的 `Result<T,E>`、`Future<T>`、`channel<T>`、按载荷/实参含 owned 成分的 `enum`、`ptr<T>`（非 Copy，只能 move） | 单一所有者；`move` 转移，`clone` 深拷贝 |

`ref T` 既非 Copy 也非 owned，它是借用，只能用作参数类型。指针 `ptr<T>`（0.0.5）是 owned、move-only 的非 Copy 类型——见 §4.12 与[第 16 章](16-unsafe-and-pointers.md)。
