# 09 — 错误处理

## 9.1 `Result<T, E>`

`Result<T, E>` 是可能失败的操作的类型。它有两个变体：

- `Ok(value)` —— 成功，携带类型为 `T` 的载荷
- `Err(value)` —— 失败，携带类型为 `E` 的错误载荷

**移入 std 声明（0.0.10，D1）**：`Result` 现在是一个真实声明的参数化枚举——
`std/result.fln` 声明 `pub enum Result<T, E> { Ok(T), Err(E) }`，是语言层的
事实来源。编译器按规范路径识别 `std.result.Result` 并映射到内部表示；
运行时编码（`Result` 操作码、VM 的 `Ok`/`Err` 值）不变，也不产生
`enum_defs` 表项——该模块是纯声明模块（第 13 章 §13.10）。

**预导入三元组（0.0.10，D2）**：`Result`、`Ok`、`Err` 在每个文件中
**免导入**可用。`import std.result;` 同样可用（引入的是同一组名字），
是给想显式标注来源的读者准备的写法。三元组**不可遮蔽**：把绑定、`const`、
参数、函数、struct 或枚举命名为 `Ok`、`Err` 或 `Result` 是编译错误：

```text
`Ok` is a builtin name (declared in `std.result`) and cannot be shadowed —
choose a different name
```

（`ReservedBuiltinName`。这是相对 ≤0.0.9 的行为变更——此前这些名字可以
像 Python 那样被覆盖。）

`Ok` 和 `Err` 是构造器表达式。它们的类型来自上下文：返回类型标注、
绑定的类型标注，或 `choose` 的被匹配值。表达式位置上未施加载荷的裸
`Ok` / `Err` 仍然被拒绝。

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}
```

`Ok`/`Err` 构造器在上下文无法确定类型时是编译错误；请补充类型标注。

`Result` 的类型实参接受两种等价拼写（0.0.7，D9）：`Result<int, string>` 与
`Result[int, string]`。作为参数化枚举（第 04 章 §4.4）它与它们使用同一套
类型实参规则，且可以用嵌套模式 `when Ok(Some(x))` 匹配（第 06 章 §6.4）。

**map 查找是 Result（0.0.11）**：对 `map[K, V]` 用 `m[k]` 索引——以及显式
调用 `std.map.get(m, k)`——返回 `Result[V, string]`。缺失的键是值
`Err("key not found")`，而不是崩溃；`m[k]?` 像任何其他 `Result` 一样传播，
`choose` 用普通 `Ok`/`Err` 分支处理它（第 04 章 §4.13、第 13 章 §13.11）。

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
- **`otherwise` 在 `Result` 审视值上也合法（0.0.9，D2）**：它是总是命中的
  兜底分支，使任意分支集合成为穷尽；可以与 Ok/Err 分支自由组合，多个
  `Ok` 分支与多个 `Err` 分支在同一次匹配中按尝试链组合：

  ```fleen
  func classify(r: Result<int, string>): string {
      choose move r {
          when Ok(v) if v > 100 { "big" }    // 带守卫：不覆盖 Ok
          when Ok(v) { "small" }             // 不带守卫的重复分支：覆盖 Ok
          when Err(e) { e }
          otherwise { "unreachable" }        // 合法；此处冗余
      }
  }
  ```

- 带守卫的分支（`when Ok(v) if cond`）**不**计入穷尽性；要使用守卫，需
  搭配一个不带守卫的重复分支或 `otherwise`。0.0.9 之前这个配方无法编译
  （当时只接受纯 `Ok`+`Err` 分支集）；守卫 + `otherwise` 现在是标准写法：

  ```fleen
  h = choose move parse_result {
      when Ok(v) if v % 2 == 0 { v }    // 守卫：0.0.7 起可用
      otherwise { 0 - 1 }               // 0.0.9：合法兜底
  };
  ```

- 嵌套模式走同一条匹配路径（`when Ok(Some(y))`、或模式分支、载荷守卫）。
- 被匹配值是 owned 局部变量时写 `move res`（或 `clone res`）；新的调用
  结果不需要关键字：

  ```fleen
  out = choose div(10, 2) {
      when Ok(v) { v }
      when Err(e) { 0 - 1 }
  };
  ```

- 载荷绑定遵循移动语义：owned 载荷把所有权转移进接收它的分支。
- 缺失覆盖时与其他审视值类型一样报 typeck 错误：
  `choose is not exhaustive for type Result<...>`（0.0.2 时代的
  `ChooseResultNeedsOkErrArms` 关卡在 0.0.9 退役）。
- 同一函数体内多个走一般路径的 Result `choose` 是支持的（0.0.9 修复的
  分发问题；每个 `choose` 独立通过验证）。

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
