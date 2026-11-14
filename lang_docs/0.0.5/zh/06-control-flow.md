# 06 — 控制流

## 6.1 if / elif / else

`if` 是表达式：它有值，即所走分支的值。

```fleen
x = 5;
label = if x > 0 { "positive" } else { "not positive" };
```

带 `elif`：

```fleen
x = 0;
label = if x > 0 {
    "positive"
} elif x < 0 {
    "negative"
} else {
    "zero"
};
// label = "zero"
```

- 条件必须是 `bool`（没有真值推断）。
- 每个分支必须产生相同类型；`if` 表达式的类型即该类型。
- `else` 可以省略；此时 `if` 表达式的类型是 `unit`。
- 用作语句时尾部分号可选（第 02 章）：

  ```fleen
  if x > 0 { print("pos"); };
  if x > 0 { print("pos"); }
  ```

- `else` 可以跨行挂接到 `if` 上，但不能跨过一条已经结束的语句
  （第 02 章 §2.3）。

## 6.2 while

```fleen
x = 0;
while x < 3 {
    print(x as string);
    x = x + 1;
};
// 打印 0 1 2
```

- 条件必须是 `bool`；每轮迭代前重新求值。
- 循环体是循环作用域：循环体内的 `x = expr` 会赋值给可见的外层可变变量
  （第 03 章 §3.6）。
- 用作语句时尾部分号可选。
- 没有 `break` 或 `continue`；通过条件使循环终止。

## 6.3 for

`for` 遍历**数组** `[T]` 或 **`Range`**：

```fleen
total = 0;
for x in [1, 2, 3, 4] {
    total = total + x;
};
// total = 10
```

```fleen
for i in range_from(0, 3) {
    print(i as string);
};
// 打印 0 1 2
```

- 可迭代表达式**只求值一次**，在循环开始之前。
- 循环变量是**每轮迭代的新不可变绑定**：循环体不能给它赋值，循环结束后
  它也不存在。
- 循环体内对外层可变变量的赋值与 `while` 循环体规则相同（第 03 章
  §3.6）。
- `for` 是语句；它产生 `unit`。
- `for` 和 `in` 是保留字。

## 6.4 choose

`choose` 把一个值与一系列模式依次匹配：

```fleen
x = 1;
s = choose x {
    when 0 { "zero" }
    when 1 { "one" }
    otherwise { "other" }
};
// s = "one"
```

- 被匹配值（上例的 `x`）是只求值一次的表达式。
- 模式自上而下依次尝试；第一个匹配的分支胜出。没有贯穿（fallthrough）。
- `otherwise` 匹配任何值。
- 各 `when` 和 `otherwise` 子句不写分号；只有整个 `choose` 单独作为语句
  时才写分号：

  ```fleen
  choose x {
      when 0 { print("zero") }
      otherwise { print("other") }
  };
  ```

### 模式形式

| 模式 | 匹配 | 示例 |
|------|------|------|
| 字面量 | 相等的值 | `when 0 { … }`、`when "a" { … }`、`when true { … }` |
| 负数字面量 | 相等的负数 | `when -1 { … }` |
| 标识符 | 任何值；把值绑定到该名字 | `when v { … }` |
| 字面量 `if` 守卫 | 守卫成立时的该字面量 | `when x if x > 10 { … }` |
| `Ok(p)` / `Err(p)` | `Result` 的变体，绑定其载荷 | `when Ok(v) { … }` |

```fleen
n = 15;
s = choose n {
    when 0 { "zero" }
    when x if x > 10 { "big" }
    otherwise { "small" }
};
// s = "big"
```

规则：

- 标识符模式绑定被匹配的值；绑定不可变，作用域限于该分支的块。
- 带守卫的模式（`when p if cond`）只在 `cond` 为 true 时尝试；守卫为
  false 时继续尝试下一个模式。
- 标识符模式总是匹配，因此它会使后面的模式（以及 `otherwise`）不可达。
- 没有模式匹配且没有 `otherwise` 时，程序发生运行时无匹配错误；当编译器
  能证明匹配不完整时，会报告缺少 `otherwise` 的编译错误。
- `choose` 的值是被匹配分支的值；所有分支必须类型相同。

### 对 Result 使用 choose

`Ok`/`Err` 模式处理 `Result`：

```fleen
res = div(10, 2);
out = choose move res {
    when Ok(v) { v }
    when Err(e) { 0 - 1 }
};
```

**不带守卫的** `Ok` + `Err` 组合覆盖所有 `Result` 值，因此不需要
`otherwise`。带守卫的分支不计入：守卫在运行时可能为 false，无法覆盖对应
变体。scrutinee 上的 `move` 见第 09 章 §9.2。

## 6.5 没有提供的构造

本语言没有 `switch`、`match`、`break`/`continue`、`goto`，也没有
`try`/`catch`。循环通过条件终止；错误传播用 `Result` + `?`（第 09 章）；
多路分发用 `choose`。
