# 06 — 控制流

## 6.1 `if` / `elif` / `else`

`if` 是一个表达式（有返回值）。所有分支必须返回兼容的类型。

```fleen
func classify(x: int): string {
    if x > 0 { "positive" }
    elif x < 0 { "negative" }
    else { "zero" }
}
```

用作语句时尾部分号可选（第 02 章 §2.5）：

```fleen
if x > 0 { print("pos") }
```

`elif` 可以有任意多个；`else` 可选（缺失时表达式结果为 `unit`）。

## 6.2 `while`

`while` 重复执行一个块直到条件为 `false`。结果为 `unit`。

```fleen
i = 0
while i < 5 {
    print(i as string)
    i = i + 1
}
```

循环体内的 `=` 对外层可变变量执行**赋值**，而不是创建新绑定/遮蔽
（第 03 章 §3.6）。循环体内可以使用 `break` 和 `continue`（0.0.9，§6.5）。

## 6.3 `for` 循环

`for` 遍历数组 `[T]` 或 `Range`。

```fleen
for x in [1, 2, 3] {
    print(x as string)
}
```

```fleen
import std.collection.range;

for i in range_from(0, 10) {   // 0..9，不含 10
    print(i as string)
}
```

循环变量是每次迭代的新不可变绑定。可迭代对象只求值一次。循环体内可以
使用 `break` 和 `continue`（0.0.9，§6.5）。

`for` 和 `in` 是保留字。

## 6.4 `choose` 表达式

`choose` 对一个值执行模式匹配。

```fleen
func describe(x: int): string {
    choose x {
        when 0 { "zero" }
        when 1 { "one" }
        when x if x > 10 { "big" }
        otherwise { "other" }
    }
}
```

规则：

- 每个 `when` 子句和 `otherwise` 子句本身不写分号。
- 整个 `choose` 用作语句时尾部分号可选。
- `when` 可以带守卫 `if <expr>`。

### 用 `choose` 处理 `Result`

当 `Result` 的两个变体 `Ok` 和 `Err` 都被（不带守卫地）匹配时，`choose`
是穷尽的，不需要 `otherwise`：

```fleen
res = choose safe_half(10) {
    when Ok(v) { v }
    when Err(e) { 0 - 1 }
}
```

0.0.9 起 `otherwise` 与一般分支在 `Result` 审视值上也合法（第 09 章 §9.2）：
`otherwise` 是总是命中的兜底分支，它使任意分支集合成为穷尽；多个 `Ok`
分支、多个 `Err` 分支与 `otherwise` 可以自由组合。带守卫的分支不计入
穷尽性，守卫需要搭配不带守卫的重复分支或 `otherwise`。

### 用 `choose` 处理 `enum`（0.0.6 / 0.0.7 / 0.0.9）

当所有 `enum` 变体都被匹配时，`choose` 是穷尽的，不需要 `otherwise`：

```fleen
enum Color { Red, Green, Blue }

func to_int(c: Color): int {
    choose c {
        when Red { 0 }
        when Green { 1 }
        when Blue { 2 }
    }
}
```

0.0.7 的模式是递归的：

- **完整多载荷解构**（D4）：`when Rect(w, h)` 绑定每一个载荷；
- **嵌套模式**（D5）：`when Ok(Some(x))`、`when Some(P(a, b))`；
- **或模式**（D6）：`when 1 | 2 | 3`、`when First(v) | Second(v)`——各备选项必须绑定同名同型的绑定（否则 `OrPatternBindingMismatch`）；穷尽性按备选项的并集计算；
- **类型守卫**（D7）：`when v: int if v > 10`——`v: int` 是编译期标注，不参与运行时匹配。

```fleen
enum Shape {
    Point,
    Circle(float),
    Rect(float, float),
}

func area(s: Shape): float {
    choose move s {           // owned enum 需要 move
        when Point { 0.0 }
        when Circle(r) { 3.14159 * r * r }
        when Rect(w, h) { w * h }   // 0.0.7 完整解构
    }
}

func get(o: Option<int>): int {
    choose o {                // Option<int> 是 Copy，无需 move
        when Some(v) { v }
        when None { 0 - 1 }
    }
}
```

模式绑定规则：

- **零载荷变体模式是变体引用（0.0.9，D1）**：模式内的裸标识符若命名了作用域内
  枚举的零载荷变体，则解析为**变体引用**，不占据绑定名字空间。`when P(None, None)`
  可以编译并正常匹配（0.0.7 中这是重复声明错误）。查找顺序（D1）：同一模式中
  先注册的绑定 → 外层局部变量 → 零载荷变体 → 未解析。同一位置的绑定优先于
  变体引用；想绑定时用一个新名字即可。
- 一个模式内，同一个裸**绑定**名仍然只能出现一次——`when P(Some(x), Some(x))`
  是重复声明错误；零载荷变体**引用**可以自由重复（`when P(None, None)`）。
- **联合覆盖（0.0.9，D7）**：0.0.7 要求每个分支对它的变体单独穷尽；0.0.9 起还接受
  无守卫分支对同一变体的**联合覆盖**——检查器按构造子逐一拆分覆盖矩阵，互补的
  分支可以合成整体覆盖。该合成是严格增量的：0.0.7 下穷尽的程序仍然穷尽，而
  跨载荷位置的覆盖空洞仍然被拒绝——单独的 `P(Some(a), None)` +
  `P(None, Some(b))` **不是**穷尽（`P(Some, Some)` 两个分支都匹配不上，
  `ChooseNotExhaustive`）。
- 或模式可以与变体引用组合：`when None | Some(0)` 不绑定任何名字且覆盖两个
  备选项（空绑定集的绑定一致性自然成立）。
- `when Ok(Some(x))` 这类嵌套模式中，变体名用裸写形式（`Some`，不是 `Option.Some`）。

匹配 owned 枚举时，如果 `choose` 要消费该值，应使用 `choose move <expr>`。

## 6.5 `break` 和 `continue`（0.0.9）

`break` 退出最内层的循环；`continue` 跳到它的下一次迭代。两者都是
**语句**，不是表达式：

```fleen
x = 0
while true {
    x = x + 1
    if x > 3 { break }
}
// x = 4

odd_sum = 0
for i in range_from(0, 10) {
    if i % 2 == 0 { continue }
    odd_sum = odd_sum + i
}
// odd_sum = 25
```

规则：

- `break` / `continue` 绑定到**当前函数内最内层**的 `while` 或 `for`。
  嵌套循环绑定到内层；退出外层循环用辅助函数或标志位（没有带标签的
  break）。
- 在循环体之外它们是编译错误（`BreakOutsideLoop` /
  `ContinueOutsideLoop`）：顶层、不含循环的函数体，以及 `choose` 分支体——
  即使那个 `choose` 位于循环之内。两个关键字都不能跨越函数或 `async`
  边界。
- 它们**不是表达式**：循环没有带值的 break 形式，`x = break` 是解析错误
  （`break` 出现在表达式位置会被拒绝）。
- 之后的分号可选（ASI，第 02 章）：`break;` 与 `break` 是同一条语句。
- 不新增字节码：两者都下沉为跳到循环出口/继续边界的 `Jump`（0.0.9 D6）。

## 6.6 本语言不提供

本语言没有 `switch`、没有 `match`、没有带标签的 `break`、没有带值的
`break`、没有 `goto`、也没有 `try`/`catch`。循环通过条件或 `break`
（§6.5）结束；错误传播是 `Result` + `?`（第 09 章）；多路分发是
`choose`。
