# 15 — 附录

## 15.1 保留字

```
func  const  struct  enum  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
break  continue
box  ref  deref  move  clone  as  true  false
async  await  channel
ptr  addr  deref_ptr
map
```

- `return` 是保留字，但没有 `return` 语句。
- `break` 和 `continue` 为 0.0.9 起保留（第 06 章 §6.5）。
- `map` 为 0.0.11 起保留——`map { k -> v }` 字面量（第 04 章 §4.13）。
- 0.0.10 **没有新增保留字**——它的破坏性变更是词法层面的：含 `${` 的字符串
  字面量成为插值字符串（第 01 章 §1.4）。
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
| `DuplicateBinding` | 同一模式内同一裸**绑定**名出现两次（`when P(Some(x), Some(x))`） | 给其中一个改名；零载荷变体**引用**可以自由重复（0.0.9） |
| `BreakOutsideLoop`（0.0.9） | `break` 出现在循环体之外：顶层、不含循环的函数或 `choose` 分支体 | 移入当前函数最内层的 `while`/`for` 循环体（第 06 章 §6.5） |
| `ContinueOutsideLoop`（0.0.9） | `continue` 出现在循环体之外（范围与 `BreakOutsideLoop` 相同） | 同上 |
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
| `ReservedBuiltinName`（0.0.10） | 绑定、`const`、参数、函数、struct 或枚举被命名为 `Ok`、`Err` 或 `Result` | 改名——预导入三元组不可遮蔽（第 09 章 §9.1） |
| `InterpUnsupportedType`（0.0.10） | `${expr}` 嵌入值不是 `int` / `float` / `bool` / `string` | 显式字符串化（例如逐字段 `as string`），或只嵌入可转换类型（第 01 章 §1.4） |
| `UnclosedInterpolation`（0.0.10） | `${…}` 内遇到 EOF——没有匹配的 `}` | 在结束引号前补上嵌入表达式的 `}` |
| `InvalidEscape`（0.0.10） | 七个合法转义之外的 `\` 序列（第 01 章 §1.4） | 使用合法转义；反斜杠写 `\\`，美元符写 `\$` |
| `ComplexGlobalInit` | 全局初始化器不是简单表达式——包括插值字符串（0.0.10） | 把计算移入函数体 |
| `MapKeyNotComparable`（0.0.11） | `map[K, V]` 的键类型不在白名单 `{int, bool, string}` 内 | 改用白名单内的键类型（其他键类型的深度 `==` 要等 0.1.0） |

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
| 0.0.9 | `break`、`continue` | 用它们作标识符的程序需要改名（D4） |
| 0.0.10 | 无 | 破坏性变更不在保留字层面，而在词法层面：含 `${` 的字符串字面量成为插值（D4） |
| 0.0.11 | `map` | 用它作标识符的程序需要改名（D1） |

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
| `break`/`continue`、零载荷变体模式与联合覆盖、Result-`choose` 的 `otherwise`、std.str 批量内建 | 0.0.9 |
| 字符串插值 `"${expr}"`、`\$` 转义、`Result` 移入 std 声明（预导入三元组、不可遮蔽） | 0.0.10 |
| `map[K, V]` 类型、`map { k -> v }` 字面量、`m[k]` 访问、`std.map` 八辅助函数批量 | 0.0.11 |


## 0.0.11 说明

- **`map` 类型**（0.0.11，D1/D2）：`map { k -> v }` 字面量构造始终 owned 的
  `map[K, V]`（数组姿态——绑定/赋值/传参深拷贝，`move` 转移，`clone` 深拷贝）。
  键被白名单限制在 `{int, bool, string}`（否则 `MapKeyNotComparable`，D5）；
  值槽不允许 `Future` / `channel` / `ptr`（D6）；字面量重复键原地覆盖；
  保留插入顺序（D8）。
- **`map` 是保留字**（0.0.11，D1）：唯一的源码破坏性变更——把绑定或类型命名
  为 `map` 的程序必须改名。
- **`m[k]` 访问**（0.0.11，D3/D4）：骑在既有索引后缀上（解析器零改动），
  返回 `Result[V, string]`；缺失的键是 `Err("key not found")`，而不是崩溃。
  `m[k]?` 与 `?` 组合；链式 `m["row"]["x"]` 骑在 `Ok` 载荷上。`m[k]` 与
  `std.map.get(m, k)` 是同一个操作。
- **`std.map` 批量**（0.0.11，D3）：`get` / `insert` / `remove` / `contains` /
  `keys` / `values` / `len` / `clone`——八个 VM 内建，零新操作码。
  `insert` / `remove` 函数式且消费输入：输入 map 被移动进调用，返回全新 map
  （`std.map.insert(clone m, …)` 可以保住原 map）。没有对 map 的 `for`
  迭代——迭代 `std.map.keys` / `std.map.values`（第 13 章 §13.11）。
- **字节码 v9，内容驱动**（0.0.11，D7）：一条新操作码（`NewMap`，0xAF）和
  一个新 `FieldTypeTag`（`Map = 13`）；只有真正编码了 `NewMap` 的模块才是
  v9——所有不含 map 的模块（包括每一个 `std` 模块）保持 v8，与 0.0.10
  输出逐字节相同。
- 0.0.10：字符串插值、`\$` 转义、`Result` 移入 std 声明——见下方
  §0.0.10 说明。

## 0.0.10 说明

- **字符串插值**（0.0.10，D4–D6）：`${expr}` 嵌入完整表达式；`int` /
  `float` / `bool` 走与 `as string` 完全相同的路径，`string` 原样嵌入
  （深拷贝——读上下文永不移动）；其他类型是 `InterpUnsupportedType`；
  没有格式说明符。降级脱糖为 `std.str.concat` 调用链，因此不含 `${` 的
  字面量与 0.0.9 逐字节相同。全局初始化器暂不支持插值
  （`ComplexGlobalInit`）。词法错误：`UnclosedInterpolation`
  （`${…}` 内遇到 EOF）、`InvalidEscape`（游离的 `\`）。
- **`\$` 转义**（0.0.10，D4）：唯一新增的转义，产生字面 `$`。`$` 仅在
  紧邻 `{` 之前有特殊含义——`"cost: $5"` 不变。只有要让字面 `$` 直接出现
  在 `{` 前才需要转义：`"a\${b}c"` 的字面值是 `a${b}c`。
- **`Result` 移入 std 声明**（0.0.10，D1/D2）：`std/result.fln` 声明
  `pub enum Result<T, E> { Ok(T), Err(E) }` 作为语言层事实来源；编译器把
  `std.result.Result` 映射到内部表示（后端不动、纯声明模块——无
  `enum_defs` 表项）。`Ok` / `Err` / `Result` 是每个文件免导入可用的
  预导入名，且不可遮蔽（`ReservedBuiltinName`）——这是相对 ≤0.0.9
  （允许覆盖）的行为变更。`import std.result;` 引入同一组名字
  （第 13 章 §13.10）。
- **`?` 与 Result 上的 `choose` 不变**（0.0.10，D3）：语义与降级保持
  0.0.2/0.0.9 原样。
- **字节码不变**（0.0.10，D7）：版本保持 v8，无新操作码、无表变更，
  `BYTECODE.md` 不动。不含 `${` 的字面量与 0.0.9 输出逐字节相同。
- 0.0.9：`break`/`continue`、零载荷变体模式、Result-`choose` 补全、
  std.str 批量内建——见下方 §0.0.9 说明。

## 0.0.9 说明

- **`break` / `continue`**（0.0.9，D4）：保留字语句，只能出现在当前函数最内层的
  `while`/`for` 体内；不是表达式，没有标签、没有带值的 break。循环体之外和
  `choose` 分支体内是编译错误（`BreakOutsideLoop` / `ContinueOutsideLoop`）。
  不新增字节码——两者下沉为 `Jump`（D6）。
- **零载荷变体模式**（0.0.9，D1/D7）：命名零载荷变体的裸标识符是变体引用而非
  绑定——`when P(None, None)` 可以编译并正常匹配。重复的**绑定**名仍是
  `DuplicateBinding`。穷尽性还接受无守卫分支对同一变体的**联合覆盖**（覆盖矩阵
  上的构造子拆分）；跨位置的覆盖空洞仍被拒绝（`P(Some, None)` + `P(None, Some)`
  不是穷尽）。
- **Result-`choose` 补全**（0.0.9，D2）：`Result` 审视值上 `otherwise` 与一般分支
  合法；`ChooseResultNeedsOkErrArms` 退役（穷尽性关卡是 typeck 的
  `ChooseNotExhaustive`，与其他审视值类型一致）。守卫分支本就可用；守卫 +
  `otherwise` 现在是文档化的标准写法。
- **std.str 批量内建**（0.0.9，D3）：`parse_int`、`parse_float`、`trim`、
  `contains`、`starts_with`、`ends_with`、`index_of`、`replace`——可能失败处
  返回 `Result`；索引按 Unicode 标量值计数（第 13 章 §13.2）。
- **字节码不变**（0.0.9，D6）：版本保持 v8，无新操作码、无表变更。内容驱动的
  版本规则不变；编译出的模块流变大只是因为每个模块头部内建表携带了八个新的
  内建名。
- 0.0.8：逐文件特性标志、柯里化，见下方 §0.0.8 说明。

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
  偏应用的模块才是 v8；没有偏应用的标志开启文件仍是 v7（D13/D14）。非破坏性
  更新（D16）。（0.0.8 发布时以"标志关闭/std 模块与 0.0.7 逐字节相同"作钉；
  自 0.0.9 起每个模块流因八个新内建名而变大，0.0.9 用自己的输出重新取钉——
  跨版本的逐字节等同不再成立，内容驱动的规则仍然成立。）
- 0.0.7：参数化枚举、递归模式，见下方 §0.0.7 说明。

## 0.0.7 说明

- **参数化枚举**（0.0.7）：`enum` 头部可声明类型参数，使用点必须完整应用；
  尖括号与方括号两种拼写等价（D9）。`is_copy` 按实参判定（D2）。
- **递归模式**（0.0.7）：完整多载荷解构、嵌套模式、或模式、类型守卫（第 06 章 §6.4）。
  注意：单个模式内同一裸**绑定**名只能出现一次——且 0.0.7 中零载荷变体名同样
  占据绑定名字空间（`when P(None, None)` 是重复声明错误），0.0.9 已修复
  （第 06 章 §6.4）。
- 无新增保留字；`Result[T, E]` 保持内建（D9）。非破坏性更新（D10）。
- 0.0.6：`enum` 为保留字；标签联合类型，`choose` 穷尽匹配。
- 0.0.5：`unsafe`/`trusted`、`ptr<T>`/`addr`/`deref_ptr`，见第 16 章。

### 历史版本备注

| 版本 | 新增特性 |
|------|----------|
| 0.0.6 | `enum`（模式仅绑定第一个载荷）、`fold` → `foldl` + `foldr`、`std.arr.concat`/`flatten` |
| 0.0.5 | `unsafe`/`trusted`、`ptr<T>`/`addr`/`deref_ptr`、CLI 标准、发布管线、std 分发 |
