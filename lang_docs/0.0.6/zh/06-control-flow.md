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
（第 03 章 §3.6）。

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

循环变量是每次迭代的新不可变绑定。可迭代对象只求值一次。

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

当 `Result` 的两个变体 `Ok` 和 `Err` 都被匹配时，`choose` 是穷尽的，
不需要 `otherwise`：

```fleen
res = choose safe_half(10) {
    when Ok(v) { v }
    when Err(e) { 0 - 1 }
}
```

### 用 `choose` 处理 `enum`（0.0.6）

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

带载荷的变体可以绑定第一个载荷：

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
        when Rect(w) { w * 0.0 }   // 0.0.6 只绑定第一个载荷
    }
}
```

0.0.6 限制：枚举模式只能绑定第一个载荷（写 `when Rect(w, h)` 会报语法错误）；其余载荷不可在该分支中命名。

匹配 owned 枚举时，如果 `choose` 要消费该值，应使用 `choose move <expr>`。
