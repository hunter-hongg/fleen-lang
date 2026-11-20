# 14 — 工程与 flint

## 14.1 工程布局

多文件程序是一个工程：一份清单加一棵源码树。

```text
myproj/
├── flint.toml          # 清单
├── src/
│   ├── main.fln        // 入口模块（包含 func main）
│   └── util.fln
└── tests/              // 可选：测试用例（§14.5）
    ├── case1.fln
    └── case1.expected
```

## 14.2 `flint.toml`

```toml
[package]
name = "myproj"
version = "0.1.0"

[dependencies]
# 依赖条目写在这里
```

- `name` 是工程的模块根，必须是合法标识符（不能带连字符）。
- 依赖是 git 仓库；解析后的构建记录在 `Flint.lock`。拉取的依赖缓存在
  `$FLINT_HOME` 之下。

## 14.3 flint 命令

| 命令 | 效果 |
|------|------|
| `flint new <name>` | 脚手架创建新工程 |
| `flint build` | 编译工程树 |
| `flint run` | 编译并执行 |
| `flint test` | 运行 `tests/` 下的用例（§14.5） |
| `flint check` | 只做类型检查，不执行 |

一站式驱动同样可以处理单文件和工程：

```bash
cargo fln hello.fln                  # 单文件：编译、验证、执行
cargo fln --project myproj           # 工程树
```

## 14.4 依赖

依赖在 `flint.toml` 中声明，从 git 拉取。依赖以 crate 名导入：

```fleen
import shylib;              // 一个依赖 crate
r = shylib.helper();
```

- 工程不能导入一个已经依赖它的 crate（依赖环会被拒绝）。
- 依赖的源码树对本次构建是只读的。

## 14.5 测试

测试用例是 `tests/` 下的一个 `.fln` 文件，配一个 `<name>.expected` 文件。
`flint test` 逐个运行用例，并把标准输出与期望文件逐行比较。

```fleen
// tests/greet.fln
import std.io;

func main() {
    std.io.println("hello test");
}
```

```text
# tests/greet.expected
hello test
```

```bash
$ flint test
greet ... ok
```

测试程序是普通的 Fleen 程序：可以导入工程的模块，使用完整语言，包括
执行器和通道。

## 14.6 示例：完整工程

```toml
# flint.toml
[package]
name = "adder"
version = "0.1.0"

[dependencies]
```

```fleen
// src/main.fln
import std.io;

func add(a: int, b: int): int = a + b

func main() {
    std.io.println(add(2, 3) as string);
}
```

```bash
$ cd adder
$ flint run
5
```
