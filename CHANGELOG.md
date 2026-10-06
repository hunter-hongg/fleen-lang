# Changelog

所有显著变更记录于此。格式参考 [Keep a Changelog](https://keepachangelog.com/)，
版本语义遵循 [SPEC.md](SPEC.md) §13（`0.0.x` = 早期开发，API 不稳定）。

## 0.0.1 — Swift Fox (2026-10-06) 🦊

首个里程碑版本：从源码到字节码到执行的**完整工具链**。一门表达式优先的小型语言，
配有五阶段编译器、静态字节码验证器和栈式虚拟机。

### 语言一览

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

```console
$ fleen-vm fib.fln
0
1
1
2
3
5
8
13
21
34
```

### ✨ 语言特性

**变量与不可变性**

- `x = 42` 可变绑定与赋值；`const y = 42` 不可变，重赋值在编译期即被拒绝
- 可选类型标注：`x: int = 42`

**类型系统**

- 基础类型：`int`（64 位）/ `float` / `bool` / `string` / `unit`
- `Result<T, U>` 类型
- **函数是一等公民**，函数类型可直接标注：
  `func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)`

**控制流即表达式** —— 每个分支都产生值

- `if / elif / else` 表达式
- `while` 循环
- `choose` 模式匹配：字面量模式、带守卫的模式 `when x if x > 10`、
  `otherwise` 兜底；匹配不穷尽时编译期报错

**诊断体验**

- 所有编译期错误携带源码位置（Span），消息可操作
- 运行期错误明确归类：除零、整数溢出（checked 运算，不静默回绕）、
  超出递归深度（上限 1024）

**0.0.1 的刻意取舍**：分号 `;` 需显式书写（自动分号插入在后续版本）；
`?`、`struct`、`for`、泛型、FFI 等尚未纳入 —— 完整范围见 `SPEC.md` §15。

### 🛠 工具链

```
fib.fln ─► Lex ─► Parse ─► Resolve ─► Typeck ─► Lower ─► Codegen ─► fib.flnc
                                                                     │
                                              fleen-verify 静态验证 ◄─┘
                                                                     │
                                                               fleen-vm 执行
```

| 组件 | 职责 |
|------|------|
| `fleen-compiler` | 五阶段编译前端，输出版本化的 `.flnc` 字节码模块 |
| `fleen-verify` | 执行前的静态安全网：全路径栈深度分析、跳转目标必须是完整指令边界、常量/函数/全局索引范围检查、`const` 全局仅限模块初始化器写入、禁止 fall-off-end |
| `fleen-vm` | 栈式虚拟机：调用帧与局部槽位、一等函数调用、宿主内建函数；字节码**永不未经验证就执行** |

仓库内提供单命令入口：

```bash
cargo fln fib.fln     # 编译 → 验证 → 执行，一步到位
cargo fln fib.flnc    # 也可以直接运行字节码
```

退出码契约：`0` 成功 / `1` 编译·验证·运行错误 / `2` 用法·IO 错误，脚本可依赖。

### 🧪 质量保障

- **321 测试**：单元测试 + 各编译阶段 valid/invalid 文件驱动的集成测试 +
  端到端测试（驱动真实二进制，比对 stdout、退出码与 stderr）
- E2E 错误路径覆盖：词法/语法/名字解析/类型错误、`const` 重赋值、除零、
  整型溢出、无限递归
- 门禁四件套：`cargo fmt --check` · `cargo clippy -D warnings` ·
  `cargo test` · `cargo doc`（GitHub Actions 已配置）

### 📦 安装

从源码构建（需要 Rust 1.85+，2024 edition）：

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang
cargo build --release
./target/release/fleen-vm examples/hello.fln
```

预编译二进制：release 附件提供 linux x86_64 压缩包
（含 `fleen-compiler` / `fleen-verify` / `fleen-vm` 三个可执行文件），
解压即用：`./fleen-vm fib.fln`。

### 🔜 下一步

按 `docs/DESIGN.md` 的设计推进：自动分号插入（ASI）、`struct`、`for`、
泛型、`?` 运算符等将在后续版本陆续落地；字节码格式规范见
`docs/BYTECODE.md`，路线图见 `docs/0.0.1/TICKETS.md`。
