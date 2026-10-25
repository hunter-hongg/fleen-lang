# 05 — 运算符

## 5.1 优先级

从最低（结合最松）到最高（结合最紧）：

```
or
and
== !=   < > <= >=
+ - * / %
as
?                                            （postfix）
box deref move clone  -  !                   （一元前缀）
() [i] .f  await  ?                          （后缀后置符）
```

后缀后置符——调用 `()`、索引 `[i]`、字段访问 `.f`、`await`、`?`——结合
最紧，从左到右链式结合。圆括号覆盖一切。

```fleen
1 + 2 * 3          // 7
(1 + 2) * 3        // 9
1 + 2 as string    // 解析为 1 + (2 as string)：int + string —— 类型错误
```

## 5.2 算术运算符

`+ - * / %` 作用于 `int` 和 `float`。两个操作数类型必须相同；`int` 与
`float` 之间没有隐式转换。

```fleen
a = 7 + 2;        // 9
b = 7 - 2;        // 5
c = 7 * 2;        // 14
d = 7 / 2;        // 3   （int 除法向零取整）
e = 7 % 2;        // 1   （余数）

f = 7.0 / 2.0;    // 3.5
```

字符串没有 `+`；拼接用 `std.str.concat`（第 13 章）。

## 5.3 比较运算符

`== != < > <= >=` 产生 `bool`。它们作用于 `int`、`float`、`bool` 和
`string`；两个操作数类型必须相同。

```fleen
1 == 1            // true
1 != 2            // true
2 < 3             // true
3.0 >= 3.0        // true
"a" < "b"         // true（字典序）
```

比较运算是只读使用：不会转移 owned 操作数的所有权（第 08 章）。

struct、数组和 `Result` 值不支持 `==`。

## 5.4 逻辑运算符与取反

`and` / `or` 组合 `bool` 值；`!` 对 `bool` 取反。

```fleen
t = true and false;     // false
u = true or false;      // true
v = !true;              // false
w = 1 < 2 and 2 < 3;    // true
```

一元 `-` 对数字取负：

```fleen
n = -5;
m = -(3 + 4);           // -7
```

## 5.5 `as` —— 类型转换

`e as string` 把标量转换为字符串形式。支持的转换恰好是：

| 表达式 | 结果 |
|--------|------|
| `int as string` | 十进制数字，负数带前导 `-` |
| `float as string` | 与 `print` 相同的形式 |
| `bool as string` | `"true"` 或 `"false"` |

```fleen
x = 42 as string;         // "42"
y = (0 - 7) as string;    // "-7"
z = 1.5 as string;        // "1.5"
w = true as string;       // "true"
```

`print` 和 `println` 只接受字符串；打印其他标量值请用 `as string`：

```fleen
n = 42;
print(n as string);
```

其他任何转换（`string as string`、`int as float`、对 `box`、`ref`、数组、
struct 的转换、恒等转换）都是编译错误（`UnsupportedCast`）。没有隐式
数值转换：`int` 与 `float` 永不混用。

`as` 比二元运算符结合更紧，比一元前缀更松：

```fleen
deref b as string       // (deref b) as string
box 1 as string         // box (1 as string)
1 + 2 as string         // 1 + (2 as string)
```

`as string` 的结果是全新的 owned 字符串；绑定或传参时不需要 `move`/
`clone`。

## 5.6 一元前缀运算符

| 运算符 | 操作数 | 效果 |
|--------|--------|------|
| `-` | `int` / `float` | 算术取负 |
| `!` | `bool` | 逻辑取反 |
| `box` | 任意允许的内部类型 | 在堆上分配 `box<T>` |
| `deref` | `box<T>` | 读内部值（或作为赋值目标） |
| `move` | owned 局部变量 | 转移所有权（第 08 章 §8.2） |
| `clone` | owned 值 | 深拷贝（第 08 章 §8.3） |

```fleen
b = box 42;
n = deref b;          // 42
s = box "heap";
t = clone deref s;    // 内部字符串的 owned 拷贝；box 本身不受影响
```

## 5.7 后缀运算符

后缀后置符结合最紧，从左到右链式结合：

| 后缀 | 作用于 | 效果 |
|------|--------|------|
| `(args)` | 函数值 | 调用 |
| `[i]` | 数组 | 元素读（元素写是语句形式） |
| `.f` | struct | 字段读 |
| `await` | `Future<T>` | 驱动到完成，产出 `T`（第 10 章） |
| `?` | `Result<T, E>` | 取出载荷，或传播错误（第 09 章） |

```fleen
add(1, 2)              // 调用
a[0]                   // 索引读
p.x                    // 字段读
half(n) await?         // ((half(n) await)?)：先 await 再 ?
f(1)[2].x              // 从左到右链式：调用、索引、字段
```

## 5.8 赋值不是有值的运算符

表达式位置的 `=` 给已存在的变量赋值并求值为 `unit`；它从不创建绑定
（第 03 章 §3.4）：

```fleen
b = 5
a = b = 3        // b 变为 3；a 绑定到 unit
```

没有复合赋值运算符（`+=`、`++` 等）。请写 `x = x + 1`。
