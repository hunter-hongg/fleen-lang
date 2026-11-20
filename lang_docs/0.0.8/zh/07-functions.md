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

### 7.1.1 两段式柯里化声明（0.0.8）

在开启了 `#feature curry` 的文件里，函数改用**两段式**声明：第一行是只写类型的
**签名**（一个柯里化箭头函数类型，第 04 章 §4.2），随后是带命名参数的**函数体**；
这种文件里 `func` 关键字被禁止（`FuncFormForbiddenInCurryFile`）：

```fleen
#feature curry

add: int -> int -> int          // 签名行：单行，只写类型
add(a, b) = a + b               // 函数体：`=` 后一个表达式

greet: string -> string
greet(name) { name }            // 函数体：一个块

main: () -> unit
main() {
    std.io.println(greet("bob"));
}
```

- 函数体是 `=` 后的单个表达式，或是一个块——和 `func` 的两种形式一致。
- 函数名写在**函数体**的开头，签名行不写名字；名字和参数个数（元数）把签名和
  函数体配对起来，在名字解析阶段匹配。名字不匹配是 `BodyWithoutSignature` /
  `SignatureWithoutBody`，元数不符是 `SignatureArityMismatch`，参数名重复是
  `DuplicateParam`。
- 签名行必须是一个函数类型，否则是 `SignatureNeedsFunctionType`；只有签名没有
  函数体或相反都是错误。
- `pub` 和 `async` / `unsafe` / `trusted` 修饰符**只写在签名行**：

  ```fleen
  pub unsafe peek: int -> int
  peek(x) = x
  ```

  写在函数体上分别是 `PubOnCurriedBody` / `ModifierOnCurriedBody`。
- `async` 函数的签名写函数体返回的 `Result`，调用方仍看到 `Future<T>`（第 10 章）：

  ```fleen
  async half: int -> int -> Result[int, string]
  half(n, d) {
      if d == 0 { Err("zero") } else { Ok(n / d) }
  }
  ```

- `import` 的使用不受标志影响；标志是**逐文件**的（第 01 章 §1.5）。

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

### 7.4.1 隐式偏应用（0.0.8）

在 `#feature curry` 开启的文件中，以**少于**被调用函数参数个数的实参去调用，
得到一个**接收剩余参数**的函数——即隐式偏应用：

```fleen
#feature curry

add: int -> int -> int
add(a, b) = a + b

inc = add(1);         // inc: int -> int
five = inc(4);        // 5
copy_me = add;        // 函数本身（不是偏应用）
zero = add();         // 零个实参得到函数本身，也不是偏应用
```

- 实参数少于 1（`add()`）时得到**函数本身**，不是偏应用；只有"少于元数且大于 0"
  的调用才是偏应用（D5）。
- 只有**定元数的用户函数**参与偏应用：变参内建函数（`print`、`println`）、带
  `ref` / `ptr<T>` 参数的函数、`unsafe` / `trusted` 函数都不参与，保持普通调用
  规则（D9）。
- 偏应用**没有子类型**：得到的函数类型就是"剩余参数 → 返回类型"。
- 偏应用绑定到名字是合法的；只在被**丢弃的表达式语句**位置出现偏应用是
  `PartialApplicationDiscarded`（D10）：

  ```fleen
  inc2 = add(1);      // 合法
  add(1);             // 错误：偏应用的值被丢弃
  ```

- 偏应用是**逐调用位置**决定的，不看所在文件的标志：标志关闭的文件可以全元数
  调用一个标志开启文件导出的函数，但不能对它做偏应用；反之，标志开启的文件也
  可以对普通 `func` 做偏应用（D5/D16）。
- 被捕获的参数住在运行期的函数值里；见第 08 章 §8.1 的捕获语义（D11）。

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
