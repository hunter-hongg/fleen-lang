# 07 — 函数

## 7.1 声明

函数用 `func` 声明。声明**不写**尾部分号。

单行形式——函数体是 `=` 之后的一个表达式：

```fleen
func add(a: int, b: int): int = a + b
```

多行形式——函数体是一个块；**不带分号的最后一个表达式**就是返回值：

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}
```

不带返回类型的函数返回 `unit`：

```fleen
func greet(name: string) {
    print("Hello, ", name);
}
```

## 7.2 参数

参数写作 `name: type`：

```fleen
func point_sum(p: Point, q: Point): int = p.x + p.y
```

参数类型可以是：

- 任意 Copy 或 owned 类型——`int`、`string`、`[T]`、struct、`box<T>`、
  `Result<T, E>`、`Future<T>`、`channel<T>`、函数类型；
- 借用 `ref T`——只读参数（第 08 章 §8.5）。

owned 实参遵循第 08 章的所有权规则：新值或 Copy 值直接传递；owned 变量
在调用处必须写 `move x` 或 `clone x`：

```fleen
func length(s: string): int = std.str.len(s)

name = "ada";
n = length(move name);      // name 被移动；想保留请写 clone name
```

## 7.3 返回值

- 返回类型写在参数列表之后的 `: ` 后；省略表示返回 `unit`。
- 单行函数体的值是 `=` 之后的表达式。
- 多行函数体的值是它**不带分号**的最后一个表达式：

  ```fleen
  func f(): int {
      x = 1;
      x + 1          // 返回 2
  }
  ```

  写成 `x + 1;` 会丢弃该值。

- 没有 `return` 语句。唯一的提前退出机制是 `Result` 上的 `?` 运算符
  （第 09 章 §9.3）。
- 声明的返回类型是 `Result<T, E>` 时，每条退出路径都必须产生一个
  `Result`。

## 7.4 调用

调用写作 `name(args)`：

```fleen
r = add(1, 2);
greet("bob");
```

- 实参类型必须与形参类型完全一致；没有隐式转换，也没有默认参数。
- 没有函数重载：一个名字对应一个函数。
- 函数可以调用自己（递归）：

  ```fleen
  func fact(n: int): int {
      if n <= 1 { 1 } else { n * fact(n - 1) }
  }
  ```

- 调用 `async func` 产生 Future，不执行函数体；见第 10 章。

## 7.5 函数作为值

函数是一等值，具有函数类型（第 04 章 §4.2）：可以绑定、传参、返回。

```fleen
func add(a: int, b: int): int = a + b
func sub(a: int, b: int): int = a - b

func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)

op = add;
r = apply(op, 10, 3);       // 13
r2 = apply(sub, 10, 3);     // 7
```

函数值是 Copy 类型：绑定或传递时拷贝。

## 7.6 匿名函数

没有匿名函数语法。定义具名函数并传递其名字：

```fleen
func twice(n: int): int = n * 2

xs = [1, 2, 3];
ys = std.collection.iter.map(xs, twice);    // [2, 4, 6]
```

## 7.7 函数声明的位置

- 文件顶层（模块函数；加 `pub` 导出，第 12 章）。
- 函数不能声明在另一个函数内部或块内部。
- 函数名可以覆盖内置函数（例如自定义 `print`）；在其作用域内以用户定义
  为准。
