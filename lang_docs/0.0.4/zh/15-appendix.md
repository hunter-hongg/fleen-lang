# 15 — 附录

## 15.1 保留字

```
func  const  struct  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
box  ref  deref  move  clone  as  true  false
async  await  channel
```

- `return` 是保留字，但没有 `return` 语句。
- `unsafe` 和 `trusted` 为未来版本保留。
- `Future` 不是保留字；只有类型位置的 `Future<T>` 会被识别。
- 保留字可以作为模块路径段和成员名（`import std.async;`）。

## 15.2 运算符优先级

从低到高：

```
or
and
== !=   < > <= >=
+ - * / %
as
?
box deref move clone  -  !       （一元前缀）
() [i] .f  await  ?              （后缀后置符）
```

## 15.3 编译期错误速查

最常见的编译错误及修改方法：

| 错误 | 原因 | 修改 |
|------|------|------|
| cannot assign to immutable variable | 写 `const` | 如需可变，声明时去掉 `const` |
| owned value must be transferred explicitly | 消费位置出现裸的 owned 变量 | 写 `move x` 或 `clone x` |
| use of moved value | `move` 之后继续使用变量 | 改用 clone，或调整顺序 |
| move out of box / array / struct / global | `move deref b`、`move a[i]`、`move p.f`、`move g` | 改用拷贝/clone 形式 |
| type annotation not allowed on assignment | 对已存在的 `x` 写 `x: int = v` | 去掉类型标注 |
| shadowing must preserve mutability | 内外层可变性不同 | 与外层保持一致或改名 |
| unhandled Result | 表达式语句丢弃了 `Result` | 绑定、用 `?`，或 `choose` 处理 |
| `await` applies to a Future | 对非 future 使用 `await` | 只 await `async func` 的调用结果 |
| ref in async function | `async func` 声明 `ref` 参数 | 改用 owned 或 Copy 参数 |
| writing a global from async | 异步代码中触达全局的字段/索引/限定名写 | 把值经通道发送 |
| channel cannot be cloned | `clone ch` | 用 `std.concurrent.split(ch)` |
| channel send requires move | 裸发 owned 变量 | 写 `c.send(move x)` / `c.send(clone x)` |
| UnsupportedCast | 三种转换之外的 `as` | 手动转换 |
| ArgTypeMismatch | `print` 收到非字符串参数 | 加 `as string` |
| PrivateAccess | 跨模块访问私有项 | 给声明加 `pub` |
| ModuleNotFound / CircularImport | 导入路径错误 / 导入环 | 修正路径 / 打破环 |
| ArrayTypeMismatch / CannotInferArrayType | 元素类型混杂 / 空字面量 | 元素统一 / 加类型标注 |

## 15.4 运行时错误速查

| 错误 | 原因 |
|------|------|
| `IndexOutOfRange` | 数组索引超出 `0..len` |
| `ChannelDeadlock` | 剩余任务全部阻塞在无法继续的通道操作上 |
| 算术错误 | 例如除以零、对最小 `int` 取 `abs` |
| 被驱动的 future 失败 | 失败的 future 在 `await` 处重新抛出错误 |

运行时错误以非零退出码结束程序，并打印带源码位置的消息。没有恢复机制。

## 15.5 版本备注

当前工具链实现了本参考文档描述的全部特性。对以早期版本编写的程序，
需要留意的是保留字的变化：

| 版本 | 新增保留字 | 备注 |
|------|-----------|------|
| 0.0.2 | `deref`、`as` | |
| 0.0.3 | `struct`、`for`、`in`、`pub` | |
| 0.0.4 | `async`、`await`、`channel` | 用它们作标识符的程序需要改名 |

各特性的可用版本：

| 特性 | 起始版本 |
|------|----------|
| 核心表达式、`func`、`if`/`while`/`choose`、`Result` 类型 | 0.0.1 |
| `move`/`clone`、`box`/`deref`、`ref` 参数、`?`、`Ok`/`Err`、分号可选、`as string` | 0.0.2 |
| `struct`、数组、`for` + Range、模块（`import`/`pub`）、标准库、`flint` | 0.0.3 |
| `async`/`await`/`Future<T>`、执行器、`channel<T>` | 0.0.4 |
