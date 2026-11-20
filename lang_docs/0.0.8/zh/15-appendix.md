# 15 — 附录

## 15.1 保留字

```
func  const  struct  enum  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
box  ref  deref  move  clone  as  true  false
async  await  channel
ptr  addr  deref_ptr
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
| `TypeConstructorNeedsArgs`（0.0.7） | 类型位置的裸参数化枚举名（裸写 `Option`） | 补上实参：`Option<int>` |
| `ArityMismatch` / `ArityMismatchInPattern`（0.0.7） | 类型实参过多 / 模式载荷数不匹配 | 对齐声明里的参数/载荷数量 |
| `RecursiveEnum`（0.0.6，0.0.7 覆盖参数化载荷） | 枚举（含经类型实参的）递归引用自身 | 用 `box` 打破尺寸循环 |
| `OrPatternBindingMismatch`（0.0.7） | 或模式各备选项绑定的名字/类型不一致 | 让各备选项绑定同名同型的绑定 |
| `FeatureDirectiveMisplaced`（0.0.8） | `#feature` 指令写在第一条声明之后 | 把 `#feature` 移到文件头部区（第 01 章 §1.5） |
| `UnknownFeature`（0.0.8） | `#feature` 的特性名不被识别 | 只写已实现的特性，例如 `#feature curry` |
| `DuplicateFeature`（0.0.8） | 同一特性在同一文件重复开启 | 删掉多余的 `#feature` 行 |
| `FuncFormForbiddenInCurryFile`（0.0.8） | `#feature curry` 文件里用 `func` 声明 | 改用两段式声明（第 07 章 §7.1.1） |
| `SignatureNeedsFunctionType`（0.0.8） | 签名行写的不是函数类型 | 写箭头函数类型，如 `add: int -> int -> int` |
| `SignatureWithoutBody`（0.0.8） | 只有签名行，没有对应函数体 | 补上带同名同元数的函数体 |
| `BodyWithoutSignature`（0.0.8） | 标志开启文件里只有函数体没有签名 | 补上签名行 |
| `SignatureArityMismatch`（0.0.8） | 签名类型的参数个数与函数体参数个数不一致 | 对齐签名与函数体的参数数量 |
| `DuplicateParam`（0.0.8） | 函数体参数名重复 | 重命名重复的参数 |
| `PubOnCurriedBody`（0.0.8） | `pub` 写在两段式声明的函数体上 | 把 `pub` 移到签名行 |
| `ModifierOnCurriedBody`（0.0.8） | `async`/`unsafe`/`trusted` 写在函数体上 | 把修饰符移到签名行 |
| `PartialApplicationDiscarded`（0.0.8） | 偏应用只作为被丢弃的表达式语句 | 绑定到名字或用返回值（第 07 章 §7.4.1） |

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
| 0.0.6 | `enum` | 用它作标识符的程序需要改名 |
| 0.0.7 | 无（增量式） | 非破坏性更新（D10） |
| 0.0.8 | 无（`feature` 不是保留字） | 非破坏性更新（D16）；`#feature curry` 是头部指令，由 `#` 识别 |

各特性的可用版本：

| 特性 | 起始版本 |
|------|----------|
| 核心表达式、`func`、`if`/`while`/`choose`、`Result` 类型 | 0.0.1 |
| `move`/`clone`、`box`/`deref`、`ref` 参数、`?`、`Ok`/`Err`、分号可选、`as string` | 0.0.2 |
| `struct`、数组、`for` + Range、模块（`import`/`pub`）、标准库、`flint` | 0.0.3 |
| `async`/`await`/`Future<T>`、执行器、`channel<T>` | 0.0.4 |
| `unsafe`/`trusted`、`ptr<T>`/`addr`/`deref_ptr` | 0.0.5 |
| `enum`、`foldl`/`foldr`、`std.arr.concat`/`flatten` | 0.0.6 |
| 参数化 `enum Option<T>`、完整多载荷/嵌套/或模式、类型守卫 | 0.0.7 |
| `#feature curry`、两段式声明、柯里化箭头类型、隐式偏应用 | 0.0.8 |


## 0.0.8 说明

- **逐文件特性标志**（0.0.8）：`#feature curry` 写在文件头部区，只影响所在文件
  （D6）。`feature` 不是保留字。
- **两段式声明**（0.0.8）：标志开启的文件里 `func` 被禁止，改用签名行 + 函数体
  （第 07 章 §7.1.1）。
- **柯里化箭头类型**（0.0.8）：`int -> int -> int` 右结合，与 `(int, int) -> int`
  等价；`() -> T` 是零参数函数类型（第 04 章 §4.2）。
- **隐式偏应用**（0.0.8）：少于元数的调用得到剩余函数；偏应用是 Copy，被捕获的
  参数深拷贝（D11）。标志关闭的文件保持 0.0.7 的 `ArityMismatch`（D16）。
- 字节码 v8 增加一条 `PartialApply`（0xCE），且是**内容驱动**的：只有真正编码了
  偏应用的模块才是 v8；没有偏应用的标志开启文件仍是 v7，所有标志关闭的模块与
  0.0.7 逐字节相同（D13/D14）。非破坏性更新（D16）。
- 0.0.7：参数化枚举、递归模式，见下方 §0.0.7 说明。

## 0.0.7 说明

- **参数化枚举**（0.0.7）：`enum` 头部可声明类型参数，使用点必须完整应用；
  尖括号与方括号两种拼写等价（D9）。`is_copy` 按实参判定（D2）。
- **递归模式**（0.0.7）：完整多载荷解构、嵌套模式、或模式、类型守卫（第 06 章 §6.4）。
  注意：单个模式内同一裸名只能出现一次（`when P(None, None)` 是重复声明错误）。
- 无新增保留字；`Result[T, E]` 保持内建（D9）。非破坏性更新（D10）。
- 0.0.6：`enum` 为保留字；标签联合类型，`choose` 穷尽匹配。
- 0.0.5：`unsafe`/`trusted`、`ptr<T>`/`addr`/`deref_ptr`，见第 16 章。

### 历史版本备注

| 版本 | 新增特性 |
|------|----------|
| 0.0.6 | `enum`（模式仅绑定第一个载荷）、`fold` → `foldl` + `foldr`、`std.arr.concat`/`flatten` |
| 0.0.5 | `unsafe`/`trusted`、`ptr<T>`/`addr`/`deref_ptr`、CLI 标准、发布管线、std 分发 |
