# 09 — 错误处理

## 9.1 `Result<T, E>`

`Result<T, E>` 是可能失败的操作的类型。它有两个变体：

- `Ok(value)` —— 成功，携带类型为 `T` 的载荷
- `Err(value)` —— 失败，携带类型为 `E` 的错误载荷

`Ok` 和 `Err` 是内置构造器表达式。它们的类型来自上下文：返回类型标注、
绑定的类型标注，或 `choose` 的被匹配值。

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}
```

`Ok`/`Err` 构造器在上下文无法确定类型时是编译错误；请补充类型标注。

## 9.2 用 `choose` 处理 Result

```fleen
res = div(10, 2);
out = choose move res {
    when Ok(v) {
        v
    }
    when Err(e) {
        0 - 1
    }
};
// out = 5
```

- `when Ok(v)` / `when Err(e)` 把载荷绑定到 `v` / `e`，作用域是该分支的
  块。
- **不带守卫的** `Ok` + `Err` 组合是穷尽的：不需要 `otherwise`。
- 带守卫的分支（`when Ok(v) if cond`）**不**计入穷尽性；要使用守卫，需
  搭配一个不带守卫的重复分支或 `otherwise`。
- 被匹配值是 owned 局部变量时写 `move res`（或 `clone res`）；新的调用
  结果不需要关键字：

  ```fleen
  out = choose div(10, 2) {
      when Ok(v) { v }
      when Err(e) { 0 - 1 }
  };
  ```

- 载荷绑定遵循移动语义：owned 载荷把所有权转移进接收它的分支。

## 9.3 `?` 运算符

`?` 在本身返回 `Result` 的函数内解包 `Result`：

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func ratio(a: int, b: int): Result<int, string> {
    q = div(a, b)?;      // div 失败时：ratio 立即返回那个 Err
    r = div(q, 2)?;      // div 成功时：r 是解包后的载荷
    Ok(r)
}
```

规则：

- `e?` 要求 `e: Result<T, E>`。
- 所在函数的返回类型必须是 `Result<T2, E>`，**错误类型 `E` 必须完全
  相同**；没有错误类型转换。
- 遇到 `Err` 时，函数立即返回那个 `Err`（这是本语言唯一的提前退出
  机制）。
- 遇到 `Ok` 时，表达式的值是解包后的载荷。
- `?` 是后缀后置符；它与其他后缀从左到右链式结合（第 05 章 §5.7），
  包括 `await`（第 10 章 §10.4）。

## 9.4 Result 绝不能被丢弃

值类型为 `Result<T, E>` 的表达式语句是编译错误（`UnhandledResult`）。
每个 `Result` 都必须被处理：

```fleen
div(1, 0);            // 错误：一个未处理的 Result
```

处理方式：绑定、用 `?` 传播，或用 `choose` 匹配：

```fleen
r = div(1, 0);
choose move r {
    when Ok(v) { print(v as string) }
    when Err(e) { print(e) }
};
```

## 9.5 `main` 返回 Result

`main` 可以返回 `Result<T, E>`。程序以 `Err` 结束时，错误载荷打印到
标准错误，进程以退出码 1 结束：

```fleen
func main(): Result<int, string> {
    if true { Err("startup failed") } else { Ok(0) }
}
```

```text
$ cargo fln demo.fln
startup failed
$ echo $?
1
```

## 9.6 运行时错误

有些失败不是 `Result` 值，而是终止程序的运行时错误：数组越界
（`IndexOutOfRange`）、算术溢出、通道死锁（第 11 章 §11.5）等。没有
捕获运行时错误的机制；panic 类失败不可恢复。

运行时错误打印带源码位置的消息，并以非零退出码结束。
