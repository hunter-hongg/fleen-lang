# 08 — 所有权：move、clone、box、ref

每个值要么是 **Copy** 要么是 **owned**（第 04 章 §4.11）。本章描述这种
区分在代码中的具体表现。

## 8.1 Copy 值

`int`、`float`、`bool`、`unit`、函数值、`Range`，以及字段全为 Copy 的
struct 都是 **Copy** 值。绑定、赋值、传参、返回都隐式拷贝它们，永远
不需要关键字：

```fleen
a = 42
b = a              // 拷贝；两者都可用
c = b              // 再次拷贝
```

对 Copy 值写 `move` 或 `clone` 是编译错误（修改方法是删掉关键字）。

## 8.2 owned 值与 `move`

`string`、数组、`box<T>`、含 owned 字段的 struct、含 owned 成分的
`Result`、`Future<T>`、`channel<T>` 都是 **owned** 值：任何时刻只有一个
所有者。

当 owned **局部变量**出现在*消费位置*时，转移必须显式。消费位置是：

- 绑定或赋值右值的顶层：`t = <这里>`
- 调用实参：`f(<这里>)`
- `choose` 的被匹配值：`choose <这里> { … }`

写 `move x` 转移所有权，或写 `clone x` 传入深拷贝并保留 `x`：

```fleen
s = "hello";
t = move s;        // 所有权移交给 t；此后 s 不可用
```

```fleen
s = "hello";
t = clone s;       // 深拷贝；s 仍可用
```

在消费位置直接写裸的 owned 变量是编译错误，错误信息会指出缺少的关键字：

```fleen
s = "hello";
t = s;             // 错误：owned 值 `s` 必须显式转移
                   // 提示：写 `move s` 或 `clone s`
```

`move` 之后，源变量不能再使用：

```fleen
s = "hello";
t = move s;
print(s);          // 错误：使用已移动的值 `s`
```

## 8.3 隐式转移的值

- **新值不需要关键字。** 字面量、调用结果、`as string` 结果、struct/
  数组字面量、`Ok(...)` 之类的构造器——这些值此前不存在，天然转移：

  ```fleen
  s = "hello";          // 新值：不需要 move
  t = clone s;
  u = concat(t, "!");   // 新的调用结果：不需要 move
  v = Point { x: 1, y: 2 };
  ```

- **产出位置隐式转移**：函数、块或分支的最后一个表达式流出时不需要
  关键字：

  ```fleen
  func make(): string {
      s = "fresh";
      s             // 不需要 move：它流出函数
  }
  ```

  *分支*末尾的转移是条件性的；分支之后继续使用该变量会报告它可能已被
  移动。

- **只读使用永不转移**：比较运算符和 `print`/`println` 参数自动拷贝：

  ```fleen
  s = "hello";
  if s == "hello" {     // 只读：s 仍可用
      print(s);         // 只读：s 仍可用
  };
  ```

## 8.4 不能移出的位置

`move` 无法伸进容器或全局变量——读取形式产生的是拷贝：

| 表达式 | 结果 |
|--------|------|
| `move deref b` | 编译错误——用 `clone deref b` |
| `move a[i]` | 编译错误——读是拷贝；`a[i] = v` 原地写 |
| `move p.f` | 编译错误——读是拷贝；`p.f = v` 原地写 |
| `move g`（全局变量） | 编译错误——读是深拷贝；写是整体替换 |

```fleen
b = box "inner";
s = clone deref b;    // 内部值的 owned 拷贝；b 仍可用
```

```fleen
a = ["x", "y"];
e = a[0];             // 拷贝；a 仍可用
a[1] = "z";           // 原地写
```

```fleen
g = "global text";
h = g;                // 读取 owned 全局是深拷贝；不需要关键字
```

其他规则：

- `deref b = v`（穿过 box 写入）要求 `b` 是**局部** box 变量。穿过全局
  box 写入会被拒绝；先把 box clone 到局部。
- 对 `ref` 参数写 `move`/`clone` 中的 `move` 是错误；对 ref 参数写
  `clone s` 会产生被借用值的 owned 拷贝。

## 8.5 `ref` —— 只读借用参数

`ref T` **只能**出现在函数参数类型位置。它让函数读取调用者的值而不取得
所有权：

```fleen
func shout(s: ref string): int {
    print(s);          // 只读使用
    42
}

func main(): int {
    name = "fleen";
    shout(name);       // 借用：不需要 move/clone，name 仍可用
    shout(name);       // 再次借用：合法
    0
}
```

规则：

- ref 参数是**只读的**：对它赋值或 move 都是编译错误。函数内的
  `clone s` 产生一份 owned 拷贝。
- 借用句柄可以转发给另一个函数的 ref 参数；不能存储、返回或赋值给变量。
- `ref T` 参数的实参必须是**变量**——局部变量、全局变量或另一个 ref
  参数。字面量、调用结果和其他临时值会被拒绝：调用结束后借用无处指向。
- `ref string` 与 `string` 是不同的类型：把借用赋给 owned 绑定是类型
  错误（用 `clone s`）；在期望 `ref` 的地方传 `move x` 也是错误。
- 借用实参**不被转移**，Copy/owned 局部变量保持可用。

```fleen
func shout(s: ref string) {
    print(s);          // 合法：只读
    s = "x";           // 错误：不能给被借用的参数赋值
}
```

## 8.6 box：分配、读、写、拷贝

`box expr` 在堆上分配；`deref b` 读取；`deref b = v` 写入；`clone b`
复制整个 box。

```fleen
b = box 42;            // b: box<int>
n = deref b;           // 42
deref b = n + 1;       // 单元内变为 43
c = clone b;           // 新 box、新内部值
deref c = 100;         // b 仍是 43；两个 box 相互独立
```

```fleen
s = box "heap";
t = clone deref s;     // 内部字符串的 owned 拷贝；box 不受影响
```

允许的内部类型：`int`、`float`、`bool`、`string`、`box<U>`、
`Result<T, E>`。`box` 和 `deref` 是保留字；类型位置写作 `box<T>`，
代码中用作前缀表达式。

## 8.7 速查表

| 写法 | 含义 |
|------|------|
| `x = expr`（新值） | 绑定；任何类型都不需要关键字 |
| `x = y`（owned 局部 `y`） | 错误——写 `move y` 或 `clone y` |
| `x = y`（Copy 的 `y`） | 隐式拷贝 |
| `x = move y` | 转移所有权；此后 `y` 不可用 |
| `x = clone y` | 深拷贝；`y` 仍可用 |
| `f(y)`（owned 局部 `y`） | 错误——写 `f(move y)` 或 `f(clone y)` |
| `f(y)`，其中形参为 `ref T` | 借用；无关键字，`y` 仍可用 |
| `deref b` | box 内部值的拷贝 |
| `deref b = v` | 替换内部值（仅限局部 box `b`） |
| `move deref b` / `move a[i]` / `move p.f` / `move g` | 编译错误——改用 clone |
