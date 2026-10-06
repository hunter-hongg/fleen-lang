# Fleen

**Fleen** 是一门表达式优先的小型编程语言，配有从源码到字节码到执行的完整工具链：
五阶段编译器（词法 → 语法 → 名字解析 → 类型检查 → 降级与字节码生成）、
静态字节码验证器和栈式虚拟机。

> 当前版本 **0.0.1 · Swift Fox** —— "Fibonacci 能跑" 里程碑。

## 语言一览

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0;
    const limit = 10;

    while x < limit {
        print(fib(x));
        x = x + 1;
    }

    0
}
```

特性概览：可变绑定 `=` 与 `const` 不可变、`int` / `float` / `bool` / `string` / `unit`、
`Result<T, U>`、一等函数与函数类型、`if` / `elif` / `else` 表达式、`while`、
`choose` 模式匹配（支持守卫）。0.0.1 需显式分号，`struct` / `for` / 泛型等
尚未纳入 —— 完整范围见 [SPEC.md](SPEC.md) §15。

## 快速开始

需要 Rust 1.85+（2024 edition）。

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang

# 构建
cargo build --release

# 单命令编译 + 验证 + 执行（仓库内可用 cargo 别名）
cargo fln tests/e2e/valid/fib.fln

# 或直接运行二进制
./target/release/fleen-vm tests/e2e/valid/fib.fln
```

三个可执行文件：

| 命令 | 用法 |
|------|------|
| `fleen-vm` | `fleen-vm <file.fln \| file.flnc>`：一站式编译→验证→执行，或直接执行字节码 |
| `fleen-compiler` | `fleen-compiler <file.fln> [-o <out.flnc>]`：只编译成字节码 |
| `fleen-verify` | `fleen-verify <file.flnc>`：静态验证字节码 |

退出码契约：`0` 成功 / `1` 编译·验证·运行错误 / `2` 用法·IO 错误。

## 测试

```bash
cargo test          # 单元 + 集成 + 端到端（含真实二进制 E2E）
cargo clippy --workspace --all-targets -- -D warnings
cargo doc --no-deps
```

## 文档

- [SPEC.md](SPEC.md) —— 项目与代码规范、语言快速参考
- [docs/DESIGN.md](docs/DESIGN.md) —— 语言设计草案
- [docs/SYNTAX.ebnf](docs/SYNTAX.ebnf) —— EBNF 文法
- [docs/BYTECODE.md](docs/BYTECODE.md) —— 字节码格式规范
- [CHANGELOG.md](CHANGELOG.md) —— 版本日志

## 许可证

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE)
