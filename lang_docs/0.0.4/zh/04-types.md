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

`unit` 是不携带信息的表达式的类型，例如无返回值函数的调用结果：

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

无参数的函数类型是 `() -> R`；无返回值的函数类型是 `(…) -> unit`。函数是
一等值：可以绑定、传参、返回。函数值是 Copy 类型。

## 4.3 数组 `[T]`

`[T]` 是元素类型为 `T` 的数组：

```fleen
a = [1, 2, 3];              // a: [int]
names = ["ada", "bob"];     // names: [string]
empty: [int] = [];          // 空字面量需要类型标注
```

用 `[i]` 读写元素；所有索引操作都在运行时做边界检查：

```fleen
x = a[0];        // x = 1
a[1] = 10;       // a 变为 [1, 10, 3]
```

读元素得到的是元素的**拷贝**。越界索引（包括负数）是运行时错误
（`IndexOutOfRange`），不会回绕访问。

数组永远是 **owned** 值，即使元素类型是 Copy：整个数组被移动或深拷贝
（第 08 章）：

```fleen
t = clone a;     // 深拷贝：新数组 + 新元素
u = move a;      // 转移：此后 a 不可用
```

元素类型必须一致：

```fleen
bad = [1, "a"];        // 错误：ArrayTypeMismatch
```

## 4.4 Range

`Range` 是内置类型，表示一个结束端 exclusive 的整数区间。`Range` 值
只能通过 `std.collection.range` 的两个构造函数创建（导入后也可裸用，
第 13 章）：

```fleen
r = range(3);          // 区间 0..3（0、1、2）
r2 = range_from(2, 5); // 区间 2..5（2、3、4）
```

没有 `..` 运算符语法。`Range` 配合 `for` 使用（第 06 章 §6.3），是
Copy 类型：

```fleen
for i in range(3) {
    print(i as string);      // 0 1 2
};
```

起点等于终点时区间为空。

## 4.5 box\<T\>

`box<T>` 是单所有者的堆上单元：

```fleen
b = box 42;             // b: box<int>
n = deref b;            // 读：42（拷贝）
deref b = n + 1;        // 写：单元内变为 43
c = clone b;            // 新 box + 新内部值
```

`deref b` 读取产生内部值的拷贝。允许的内部类型：`int`、`float`、`bool`、
`string`、`box<U>`、`Result<T, E>`。把值移出 box（`move deref b`）会被
拒绝——用 `clone deref b` 获得一份 owned 拷贝。完整规则见第 08 章 §8.4。

## 4.6 ref T

`ref T` 是只读借用，**只能**出现在函数参数类型位置（第 08 章 §8.5）：

```fleen
func shout(s: ref string): int {
    print(s);           // 只读使用被借用的字符串
    42
}

name = "fleen";
shout(name);            // 借用 name；name 仍可用
```

## 4.7 struct

`struct` 声明定义聚合类型；struct 名字本身就是类型：

```fleen
struct Point {
    x: int,
    y: int,
}

p = Point { x: 1, y: 2 };    // struct 字面量：每个字段都必须初始化
q = p.x;                     // 字段读：拷贝
p.y = 3;                     // 字段写
```

规则：

- 字段访问用 `.name`；嵌套 struct 可以链式访问。
- 字段读是字段的拷贝（owned 字段为深拷贝）。
- 字段写 `p.f = v` 在 `p` 是局部或全局 struct 变量且 `f` 是直接字段时
  合法；`v` 是 owned 值时照常需要 `move`/`clone`。
- 字段全为 Copy 类型的 struct 本身是 Copy 类型；含任何 owned 字段
  （`string`、`box`、数组、含 owned 字段的 struct）的 struct 是 owned
  类型：传给消费位置需要 `move S` 或 `clone S`（第 08 章）。
- struct 不能包含自身，直接或经 `box` 都不行。
- 可见性以 struct 为单位：声明加 `pub` 使整个 struct 可导入（第 12 章）；
  没有按字段的访问修饰。
- struct 不支持 `==`，也不支持 `choose` 模式。

```fleen
struct Wrapper {
    inner: Point,
}

w = Wrapper { inner: Point { x: 1, y: 2 } };
n = w.inner.x;         // 1 —— 链式字段访问
```

## 4.8 Result\<T, E\>

`Result<T, E>` 表示可能失败的操作：要么是 `Ok(value)`（成功，载荷类型为
`T`），要么是 `Err(value)`（失败，错误载荷类型为 `E`）：

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}
```

`Result` 不允许被静默丢弃；必须用 `?` 或 `choose` 处理。见第 09 章。

## 4.9 Future\<T\>

`Future<T>` 是调用 `async func` 得到的值的类型：一个被驱动后产生 `T` 的
挂起计算：

```fleen
async func half(n: int): Result<int, string> {
    if n == 0 { Err("cannot halve 0") } else { Ok(n / 2) }
}

f = half(10);           // f: Future<Result<int, string>>
v = f await;            // v: Result<int, string>
```

`Future<T>` 是 owned 值，不能 clone，只能被驱动一次。见第 10 章。

## 4.10 channel\<T\>

`channel<T>` 是并发任务之间的通信端点：

```fleen
ch = channel<int>(4);        // 携带 int 值、容量为 4 的通道
```

通道是 owned 值，不能 clone，只能用保留字构造器 `channel<T>(cap)` 创建。
见第 11 章。

## 4.11 Copy 与 owned 类型（小结）

每个类型要么是 **Copy** 要么是 **owned**；这决定了绑定与传参行为
（第 08 章）：

| 类别 | 类型 | 绑定 / 赋值 / 传参语义 |
|------|------|------------------------|
| Copy | `int`、`float`、`bool`、`unit`、函数值、`Range`、字段全为 Copy 的 struct | 隐式拷贝 |
| owned | `string`、`[T]`（永远）、`box<T>`、含 owned 字段的 struct、含 owned 成分的 `Result<T, E>`、`Future<T>`、`channel<T>` | 单一所有者；移动需要 `move`，拷贝需要 `clone` |

`ref T` 两者都不是：它是借用，且只存在于参数类型位置。

```fleen
i = 42
j = i             // Copy：隐式拷贝，i 仍可用

s = "hello"
t = move s;       // owned：显式 move，此后 s 不可用
u = clone t;      // owned：深拷贝，t 仍可用
```
