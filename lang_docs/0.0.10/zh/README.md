# Fleen 0.0.10 · Loom Spider 🕸️ 语言参考文档

本文档是 Fleen 编程语言 0.0.10 的完整参考，覆盖当前工具链已实现的全部语法与特性，
按语法类别组织，每条语法均附示例。保留字总表、运算符优先级和运行时错误
列表见[附录](15-appendix.md)。

相对 0.0.9，0.0.10 新增/变更重点：**字符串插值 `"${expr}"`**——`${expr}` 嵌入
完整表达式（调用、字段访问、算术、嵌套插值字符串）；`int` / `float` / `bool`
走与 `as string` 完全相同的字符串化路径，`string` 原样嵌入（深拷贝、永不移动）；
其他类型是编译错误（`InterpUnsupportedType`）；**没有格式说明符**
（第 01 章 §1.4、第 05 章 §5.5）。插值在降级时脱糖为 `std.str.concat` 调用链，
因此**不含** `${` 的字面量与 0.0.9 逐字节相同。**`\$` 转义**——唯一新增的转义，
产生字面 `$`；不加转义时 `$` 仅在紧邻 `{` 之前才有特殊含义（第 01 章 §1.4）。
**`Result` 移入 std 声明**——`std/result.fln` 声明 `pub enum Result<T, E> { Ok(T), Err(E) }`
作为语言层事实来源；`Ok` / `Err` / `Result` 在每个文件中免导入可用，且
**不可遮蔽**（`ReservedBuiltinName`）（第 09 章 §9.1、第 13 章 §13.10）。
字节码不变：版本保持 v8，无新操作码、无表变更（D7）。

破坏性变更集：**含 `${` 序列的字符串字面量**——其含义从"普通字符"变为
"插值开始"；要保留字面文本，把旧源码中的 `${` 写成 `\${`（转义美元符，
花括号照旧是普通字符）。其余全部是增量式（`\$` 在此前是 invalid-escape
错误；不含 `${` 字符串、且未遮蔽三个内建名的 0.0.9 程序编译结果不变）。

历史重点：**`break` / `continue`**、零载荷变体模式与联合覆盖、
Result-`choose` 的 `otherwise`、std.str 批量内建（0.0.9，第 02/06/09/13 章）、
**逐文件特性标志 `#feature curry`**、两段式柯里化声明、柯里化箭头
函数类型、隐式部分应用（0.0.8，第 01/04/07/08 章）、
**参数化枚举 `enum Option<T>`** 与递归模式（0.0.7，第 04/06 章）、
**枚举 `enum`**（0.0.6，第 04/06 章）、
**`unsafe`/`trusted` 与指针 `ptr<T>`/`addr`/`deref_ptr`**（0.0.5，第 16 章）、
数组库 `std.arr.concat`/`flatten` 与迭代 `foldl`/`foldr`（第 13 章）。

## 目录

| 章节 | 主题 |
|------|------|
| [01 词法结构](01-lexical-structure.md) | 注释、标识符、保留字、字面量、**字符串插值 `"${expr}"` 与 `\$` 转义（0.0.10）**、`#feature` 头部指令（0.0.8） |
| [02 语句与分号](02-statements-and-semicolons.md) | 语句形式（`break` / `continue`，0.0.9）、分号规则、自动分号插入 |
| [03 变量与赋值](03-variables-and-assignment.md) | 绑定、`const`、类型标注、赋值、遮蔽 |
| [04 类型](04-types.md) | 全部类型：基本类型、数组、struct、enum 与类型参数（0.0.7）、Range、box、ref、**Result（移入 std 声明，0.0.10）**、Future、channel、ptr、函数类型（柯里化箭头拼写，0.0.8） |
| [05 运算符](05-operators.md) | 算术、比较、逻辑、`as`（**插值的转换路径，0.0.10**）、一元与后缀运算符、指针运算符 |
| [06 控制流](06-control-flow.md) | `if`/`elif`/`else`、`while`、`for`、`break` / `continue`（0.0.9）、`choose`（含 `Result` 与 `enum` 穷尽匹配、递归模式、零载荷变体模式与联合覆盖，0.0.9） |
| [07 函数](07-functions.md) | 函数声明、两段式柯里化声明（0.0.8）、参数、返回值、隐式部分应用（0.0.8）、函数值 |
| [08 所有权](08-ownership.md) | Copy 与 owned 值、偏应用的捕获语义（0.0.8）、`move`、`clone`、`box`/`deref`、`ref` 参数、异步/指针边界 |
| [09 错误处理](09-error-handling.md) | `Result`（**移入 std 声明；预导入三元组不可遮蔽，0.0.10**）、`Ok`/`Err`、`?` 运算符、用 `choose` 处理 `Result`（`otherwise` 与一般分支，0.0.9） |
| [10 异步与 await](10-async-await.md) | `async func`、`Future<T>`、`await`、`await?`、执行器、定时器 |
| [11 通道](11-channels.md) | `channel<T>`、`send`、`recv`、`split` |
| [12 模块](12-modules.md) | `import`、`pub`、可见性、模块解析 |
| [13 标准库](13-standard-library.md) | 每个 std 模块与函数，含签名和示例（**`std.result` 与预导入三元组，0.0.10**；含 `concat`/`flatten`、`foldl`/`foldr`、0.0.9 std.str 批量内建） |
| [14 工程与 flint](14-projects-and-flint.md) | 工程布局、`flint.toml`、`flint` 命令、测试 |
| [15 附录](15-appendix.md) | 保留字、运算符优先级、运行时/编译期错误、版本备注（**0.0.10 错误码**） |
| [16 不安全与指针](16-unsafe-and-pointers.md) | `unsafe`/`trusted`、`ptr<T>`、`addr`、`deref_ptr`（0.0.5） |

## 运行 Fleen 程序

源文件使用扩展名 `.fln`。

```bash
# 编译并运行单个文件
cargo fln hello.fln

# 编译并运行工程（一个模块目录树）
cargo fln --project myproj

# 通过 flint 包管理器运行
cd myproj
flint run
```

一个最小的程序：

```fleen
// hello.fln
import std.io;

func main() {
    std.io.println("Hello, Fleen");
}
```

```text
$ cargo fln hello.fln
Hello, Fleen
```
