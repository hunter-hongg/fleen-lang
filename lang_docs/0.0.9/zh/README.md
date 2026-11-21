# Fleen 0.0.9 · Mending Weaverbird 🪡 语言参考文档

本文档是 Fleen 编程语言 0.0.9 的完整参考，覆盖当前工具链已实现的全部语法与特性，
按语法类别组织，每条语法均附示例。保留字总表、运算符优先级和运行时错误
列表见[附录](15-appendix.md)。

相对 0.0.8，0.0.9 新增/变更重点：**`break` / `continue`**——两个新的保留字，
只能在当前函数的 `while` / `for` 循环体内使用；它们是语句而不是表达式
（第 06 章 §6.5）。**零载荷变体模式是变体引用**——`when P(None, None)` 可以
编译并正常匹配（BUG-2 修复，第 06 章 §6.4）；无守卫的多个分支对同一变体的
**联合覆盖**被接受（构造子拆分），而跨位置的覆盖空洞仍然被拒绝（D7）。
**Result-`choose` 补全**——`Result` 审视值上 `otherwise` 与一般分支合法，
0.0.2 时代的拒绝分支已删除（第 09 章 §9.2）。**std.str 批量内建**——八个新
内建函数：`parse_int` / `parse_float` / `trim` / `contains` / `starts_with` /
`ends_with` / `index_of` / `replace`（第 13 章 §13.2）。字节码不变：版本保持
v8，无新操作码、无表变更（D6）。

破坏性变更集：`break` 和 `continue` 成为保留字——把它们用作标识符的程序必须
改名（沿 0.0.5/0.0.6 先例）。

历史重点：**逐文件特性标志 `#feature curry`**、两段式柯里化声明、柯里化箭头
函数类型、隐式部分应用（0.0.8，第 01/04/07/08 章）、
**参数化枚举 `enum Option<T>`** 与递归模式（0.0.7，第 04/06 章）、
**枚举 `enum`**（0.0.6，第 04/06 章）、
**`unsafe`/`trusted` 与指针 `ptr<T>`/`addr`/`deref_ptr`**（0.0.5，第 16 章）、
数组库 `std.arr.concat`/`flatten` 与迭代 `foldl`/`foldr`（第 13 章）。

## 目录

| 章节 | 主题 |
|------|------|
| [01 词法结构](01-lexical-structure.md) | 注释、标识符、保留字（**`break` / `continue`，0.0.9**）、字面量、`#feature` 头部指令（0.0.8） |
| [02 语句与分号](02-statements-and-semicolons.md) | 语句形式（**`break` / `continue`，0.0.9**）、分号规则、自动分号插入 |
| [03 变量与赋值](03-variables-and-assignment.md) | 绑定、`const`、类型标注、赋值、遮蔽 |
| [04 类型](04-types.md) | 全部类型：基本类型、数组、struct、enum 与类型参数（0.0.7）、Range、box、ref、Result、Future、channel、ptr、函数类型（柯里化箭头拼写，0.0.8） |
| [05 运算符](05-operators.md) | 算术、比较、逻辑、`as`、一元与后缀运算符、指针运算符 |
| [06 控制流](06-control-flow.md) | `if`/`elif`/`else`、`while`、`for`、**`break` / `continue`（0.0.9）**、`choose`（含 `Result` 与 `enum` 穷尽匹配、递归模式、**零载荷变体模式与联合覆盖，0.0.9**） |
| [07 函数](07-functions.md) | 函数声明、两段式柯里化声明（0.0.8）、参数、返回值、隐式部分应用（0.0.8）、函数值 |
| [08 所有权](08-ownership.md) | Copy 与 owned 值、偏应用的捕获语义（0.0.8）、`move`、`clone`、`box`/`deref`、`ref` 参数、异步/指针边界 |
| [09 错误处理](09-error-handling.md) | `Result`、`Ok`/`Err`、`?` 运算符、用 `choose` 处理 `Result`（**`otherwise` 与一般分支，0.0.9**） |
| [10 异步与 await](10-async-await.md) | `async func`、`Future<T>`、`await`、`await?`、执行器、定时器 |
| [11 通道](11-channels.md) | `channel<T>`、`send`、`recv`、`split` |
| [12 模块](12-modules.md) | `import`、`pub`、可见性、模块解析 |
| [13 标准库](13-standard-library.md) | 每个 std 模块与函数，含签名和示例（含 `concat`/`flatten`、`foldl`/`foldr`、**0.0.9 std.str 批量内建**） |
| [14 工程与 flint](14-projects-and-flint.md) | 工程布局、`flint.toml`、`flint` 命令、测试 |
| [15 附录](15-appendix.md) | 保留字、运算符优先级、运行时/编译期错误、版本备注（**0.0.9 错误码**） |
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
