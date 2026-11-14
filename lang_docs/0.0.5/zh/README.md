# Fleen 语言参考文档

本文档是 Fleen 编程语言的完整参考，覆盖当前工具链已实现的全部语法与特性，
按语法类别组织，每条语法均附示例。保留字总表、运算符优先级和运行时错误
列表见[附录](15-appendix.md)。

## 目录

| 章节 | 主题 |
|------|------|
| [01 词法结构](01-lexical-structure.md) | 注释、标识符、保留字、字面量 |
| [02 语句与分号](02-statements-and-semicolons.md) | 语句形式、分号规则、自动分号插入 |
| [03 变量与赋值](03-variables-and-assignment.md) | 绑定、`const`、类型标注、赋值、遮蔽 |
| [04 类型](04-types.md) | 全部类型：基本类型、数组、struct、Range、box、ref、Result、Future、channel、函数类型 |
| [05 运算符](05-operators.md) | 算术、比较、逻辑、`as`、一元与后缀运算符 |
| [06 控制流](06-control-flow.md) | `if`/`elif`/`else`、`while`、`for`、`choose` |
| [07 函数](07-functions.md) | 函数声明、参数、返回值、函数值 |
| [08 所有权](08-ownership.md) | Copy 与 owned 值、`move`、`clone`、`box`/`deref`、`ref` 参数 |
| [09 错误处理](09-error-handling.md) | `Result`、`Ok`/`Err`、`?` 运算符、用 `choose` 处理 `Result` |
| [10 异步与 await](10-async-await.md) | `async func`、`Future<T>`、`await`、`await?`、执行器、定时器 |
| [11 通道](11-channels.md) | `channel<T>`、`send`、`recv`、`split` |
| [12 模块](12-modules.md) | `import`、`pub`、可见性、模块解析 |
| [13 标准库](13-standard-library.md) | 每个 std 模块与函数，含签名和示例 |
| [14 工程与 flint](14-projects-and-flint.md) | 工程布局、`flint.toml`、`flint` 命令、测试 |
| [15 附录](15-appendix.md) | 保留字、运算符优先级、运行时错误、版本备注 |

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
