# Fleen 编译器 & 语言项目规范（统一）

> **统一规范**：本文件是 `AGENTS.md` 与 `CLAUDE.md` 的同一份来源。
> 将 `AGENTS.md` 与 `CLAUDE.md` 都设为指向本文件的软链接。
>
> 覆盖范围：Fleen 编译器自身的代码规范 + Fleen 语言的设计约定。
> 目标：可读、可维护、可扩展、不产生技术债务。

---

## 1. 项目概览

### 仓库结构

```
fleen/
├── Cargo.toml              # workspace 根
├── Cargo.lock
├── SPEC.md                 # ← 本规范（AGENTS.md / CLAUDE.md 软链接指向此）
├── AGENTS.md               # symlink → SPEC.md
├── CLAUDE.md               # symlink → SPEC.md
├── docs/
│   ├── DESIGN.md           # Fleen 语言设计草案
│   └── SYNTAX.ebnf         # Fleen EBNF 文法
├── fleen-compiler/         # 编译器 crate
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs
│       ├── lexer/          # 词法分析
│       ├── parser/         # 语法分析
│       ├── resolver/       # 名字解析 / 作用域
│       ├── typeck/         # 类型推导 & 检查
│       ├── lower/          # HIR → MIR
│       ├── codegen/        # MIR → 字节码
│       └── lib.rs
├── fleen-vm/               # 虚拟机 crate
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs          # 库入口：Vm / Value / RuntimeError
│       ├── main.rs         # CLI：.fln 一站式（编译→验证→执行）或 .flnc
│       ├── error.rs
│       ├── frame.rs
│       ├── value.rs
│       ├── vm.rs
│       └── tests.rs
├── fleen-verify/           # 验证/工具 crate
│   ├── Cargo.toml
│   └── src/
│       └── main.rs
└── tests/
    ├── lexer/
    │   ├── valid/
    │   └── invalid/
    ├── parser/
    │   ├── valid/
    │   └── invalid/
    ├── resolver/
    │   ├── valid/
    │   └── invalid/
    ├── typeck/
    │   ├── valid/
    │   └── invalid/
    └── e2e/
        ├── valid/          # <name>.fln + <name>.expected（stdout 逐行比对）
        └── invalid/        # <name>.fln + <name>.exit + <name>.stderr
```

> **单命令运行**：`cargo fln <file.fln>` 一站式完成编译 → 验证 → 执行
> （等价于 `cargo run -q -p fleen-vm -- <file.fln>`，别名定义在 `.cargo/config.toml`）。

### crate 职责

| crate | 职责 |
|-------|------|
| `fleen-compiler` | 词法 → 语法 → 语义 → 字节码 |
| `fleen-vm` | 执行字节码；CLI 兼作一站式驱动（`.fln`：编译 → 验证 → 执行） |
| `fleen-verify` | 静态分析 / 验证工具 |

---

## 2. 语言与工具链

| 项目 | 规定 |
|------|------|
| 实现语言 | **Rust**（2024 edition） |
| 格式化 | `rustfmt`，默认配置 |
| Lint | `clippy`，`#![deny(warnings)]` |
| 测试 | `cargo test`，单元 + 集成 |
| 文档 | `cargo doc`，公开 API 必须有文档注释 |

### 编译检查

```bash
# 必须在 CI / 开发机器上通过
cargo fmt -- --check       # 格式检查
cargo clippy -- -D warnings  # clippy，禁止警告
cargo test                 # 全部测试
cargo doc --no-deps        # 文档构建
```

---

## 3. 命名规范

### Rust 代码

| 类别 | 风格 | 示例 |
|------|------|------|
| crate | `kebab-case` | `fleen-lexer` |
| 模块 | `snake_case` | `token_stream` |
| 类型 | `PascalCase` | `TokenKind` |
| trait | `PascalCase` | `Visitor` |
| 函数/方法 | `snake_case` | `parse_expr` |
| 变量 | `snake_case` | `current_token` |
| 常量 | `SCREAMING_SNAKE_CASE` | `MAX_DEPTH` |
| 生命周期 | 短小写 | `'a`, `'src` |

### AST/IR 节点

| 类别 | 风格 | 示例 |
|------|------|------|
| 节点类型 | `PascalCase` + 后缀 | `ExprIf`, `StmtLet` |
| 枚举变体 | `PascalCase` | `Expr::If`, `Type::Int` |
| 字段 | `snake_case` | `condition`, `then_branch` |

**规则：** AST 节点名与文法规则对应。EBNF 里 `if_expr`，AST 里就 `ExprIf`。

### Fleen 语言层命名

| 类别 | 风格 | 示例 |
|------|------|------|
| 变量/函数 | `snake_case` | `some_var` |
| 类型/结构 | `PascalCase` | `SomeType` |
| 模块 | `foo.bar` | `std.io` |

---

## 4. 错误处理

### 编译器内部

```rust
// 不 panic，用 Result
fn parse_expr(&mut self) -> Result<Expr, ParseError> { ... }

// 只有"不可能发生"的情况才 panic
fn unreachable_case(&self) -> ! {
    unreachable!("parser guarantees this cannot happen")
}
```

**规则：**

- **编译器内部错误**：用 `Result` + 自定义 `Error` 类型
- **用户代码错误**：用 `Diagnostic`（带 Span、消息、建议）
- **不变量违反**：用 `unreachable!` / `debug_assert!`，但要在注释里说明为什么不可能

### 用户错误

```rust
pub struct Diagnostic {
    pub span: Span,
    pub kind: DiagnosticKind,
    pub message: String,
    pub notes: Vec<String>,
    pub suggestions: Vec<Suggestion>,
}
```

**规则：**

- 每个错误必须带 **Span**（源码位置）
- 错误信息必须**可操作**（告诉用户怎么改）
- 不用 `panic!` 处理用户错误

---

## 5. 数据结构

### AST / IR

```rust
// 用 enum 表示变体，用 struct 表示节点
pub enum Expr {
    Int(ExprInt),
    Bool(ExprBool),
    Ident(ExprIdent),
    If(ExprIf),
    // ...
}

pub struct ExprIf {
    pub span: Span,
    pub condition: Box<Expr>,
    pub then_branch: Block,
    pub elif_branches: Vec<(Box<Expr>, Block)>,
    pub else_branch: Option<Block>,
}
```

**规则：**

- **每个节点带 Span**
- **用 `Box` 处理递归**
- **用 `Vec` 处理列表**
- **用 `Option` 处理可选**
- **不用 `Rc` / `RefCell`**（除非有明确理由）

### ID 与索引

```rust
// 用 newtype 包裹 ID，避免混淆
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct FuncId(u32);

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct LocalId(u32);
```

**规则：**

- **不同种类的 ID 用不同 newtype**
- **ID 从 0 开始，连续分配**
- **用 `IndexVec` 或 `Vec` 存储，用 ID 索引**

---

## 6. 编译阶段

### 每个阶段的输入输出

```rust
// 每个阶段是一个纯函数
pub fn parse(tokens: Vec<Token>) -> Result<Ast, ParseError> { ... }
pub fn resolve(ast: Ast) -> Result<Hir, ResolveError> { ... }
pub fn typeck(hir: Hir) -> Result<TypedHir, TypeckError> { ... }
pub fn lower(typed_hir: TypedHir) -> Result<Mir, LowerError> { ... }
pub fn codegen(mir: Mir) -> Bytecode { ... }
```

**规则：**

- **每个阶段输入输出明确**
- **阶段之间不共享可变状态**
- **每个阶段可单独测试**
- **不跳过阶段**（不做"解析时直接类型检查"的优化）

### 阶段命名

| 阶段 | 输入 | 输出 | 职责 |
|------|------|------|------|
| Lex | 源码 | Token 流 | 词法 |
| Parse | Token 流 | AST | 语法（0.0.2 起含 ASI 判定） |
| Resolve | AST | HIR | 名字解析、作用域 |
| Typeck | HIR | Typed HIR | 类型推导、检查（0.0.2 起含所有权检查子遍历 `typeck/ownership.rs`，仍是同一阶段，不新增独立阶段） |
| Lower | Typed HIR | MIR | 控制流展平 |
| Codegen | MIR | Bytecode | 字节码生成 |
| Execute | Bytecode | 结果 | VM 执行 |

---

## 7. 测试规范

### 单元测试

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_int_literal() {
        let tokens = lex("42").unwrap();
        let expr = parse_expr(&mut tokens.into_iter()).unwrap();
        assert!(matches!(expr, Expr::Int(_)));
    }
}
```

### 集成测试

```
tests/
├── lexer/
│   ├── valid/
│   └── invalid/
├── parser/
│   ├── valid/
│   └── invalid/
├── resolver/
│   ├── valid/
│   └── invalid/
├── typeck/
│   ├── valid/
│   └── invalid/
└── e2e/
    ├── valid/      # <name>.fln + <name>.expected
    └── invalid/    # <name>.fln + <name>.exit + <name>.stderr
```

**规则：**

- **每个文法规则至少一个测试**
- **每个错误至少一个测试**
- **端到端测试用 `.fln` 文件 + 期望输出**
- **回归测试**：修 bug 时先写测试

### 测试命名

```rust
// 好：描述行为
#[test]
fn parse_if_with_elif() { ... }

#[test]
fn typeck_rejects_const_reassignment() { ... }

// 坏：描述实现
#[test]
fn test_parse_1() { ... }
```

---

## 8. 文档规范

### 公开 API

````rust
/// 解析表达式。
///
/// # 参数
/// - `tokens`: 词法分析后的 Token 流
///
/// # 返回
/// - `Ok(Expr)`: 解析成功
/// - `Err(ParseError)`: 解析失败，带 Span 和错误信息
///
/// # 示例
/// ```
/// let expr = parse_expr(tokens)?;
/// ```
pub fn parse_expr(tokens: &mut TokenStream) -> Result<Expr, ParseError> { ... }
````

### 内部注释

```rust
// 解释"为什么"，不是"做什么"
// 好：这里用 Vec 而不是 HashMap，因为顺序重要
// 坏：创建一个 Vec

// TODO: 支持泛型
// FIXME: 这个边界情况没处理
// SAFETY: 这里 unsafe 是安全的，因为...
```

---

## 9. 性能规范

### 避免的

```rust
// 坏：频繁分配
fn parse(&self) -> Vec<Token> {
    let mut tokens = Vec::new();
    for c in self.chars { tokens.push(...); }
    tokens
}

// 好：预分配
fn parse(&self) -> Vec<Token> {
    let mut tokens = Vec::with_capacity(self.chars.len());
    ...
}
```

### 规则

- **热路径避免 `clone()`**
- **用 `&str` 而不是 `String`**（除非需要拥有）
- **用 `Vec::with_capacity`** 预分配
- **用 `Box<[T]>` 而不是 `Vec<T>`** 表示固定大小
- **用 `SmallVec` / `ArrayVec`** 处理小集合
- **性能优化前先 profile**

---

## 10. Git 规范

### 提交信息

```
<type>: <description>

<body>

<footer>
```

| type | 用途 |
|------|------|
| `feat` | 新功能 |
| `fix` | 修 bug |
| `refactor` | 重构 |
| `docs` | 文档 |
| `test` | 测试 |
| `chore` | 杂项 |

**示例：**
```
feat: 支持 if/elif/else 表达式

- 添加 ExprIf 节点
- 实现 parse_if
- 添加 typeck 规则

Closes #12
```

### 分支

```
main          # 稳定
dev           # 开发
feat/xxx      # 功能分支
fix/xxx       # 修复分支
```

---

## 11. 禁止事项

| 禁止 | 原因 |
|------|------|
| `unwrap()` 在非测试代码 | 可能 panic |
| `expect()` 在非测试代码 | 同上 |
| `panic!` 处理用户错误 | 应该用 Diagnostic |
| `Rc` / `RefCell` 滥用 | 隐藏可变性 |
| 全局可变状态 | 并发问题 |
| 跳过编译阶段 | 技术债务 |
| 在 parser 里做类型检查 | 职责混乱 |
| 无测试的新功能 | 不可维护 |
| 无文档的公开 API | 不可用 |

---

## 12. 代码审查清单

提交 PR 前检查：

- [ ] `cargo fmt` 通过
- [ ] `cargo clippy` 无警告
- [ ] `cargo test` 全部通过
- [ ] 新功能有测试
- [ ] 公开 API 有文档
- [ ] 无 `unwrap()` / `expect()`
- [ ] 无全局可变状态
- [ ] 提交信息符合规范
- [ ] 不跳过编译阶段
- [ ] 错误带 Span

---

## 13. 版本规范

| 版本 | 含义 |
|------|------|
| `0.0.x` | 早期开发，API 不稳定 |
| `0.x.0` | 有 breaking change |
| `1.0.0` | 稳定 API |

**0.0.1 目标：Fibonacci 能跑。**

**0.0.2 目标：所有权起步（规划见 `docs/0.0.2/PLAN.md`）。**

---

## 14. Fleen 语言快速参考

### 基本类型

```fleen
int      // 64-bit 整数
float    // 64-bit 浮点
bool     // true / false
string   // UTF-8 字符串
unit     // 空值类型
```

### 声明

```fleen
x = 42;              // 可变绑定
const y = 42;        // 不可变绑定
x = 43;              // 赋值，合法
y = 43;              // 错误：y 不可变

x: int = 42;         // 类型标注
const y: int = 42;   // 类型标注 + 不可变
```

> `=` 的三义性（绑定 / 赋值 / 遮蔽）判定详见 `docs/DESIGN.md` §3.6 与 §4.5。

### 所有权（0.0.2）

```fleen
s = "hello";
t = move s;        // 转移所有权，s 之后不可用
t = clone t;       // 深拷贝
b = box 42;        // 堆分配，b: box<int>
n = deref b;       // 读点内值（副本）
deref b = n + 1;   // 写点内值

func shout(s: ref string): int { print(s); 42 }  // 只读借用，仅参数位置
```

> Copy 类型（`int` / `float` / `bool` / `unit` / 函数值）不受影响。
> 细则见 `docs/DESIGN.md` §10 与 `docs/0.0.2/PLAN.md` §3.1–3.3。

### 函数

```fleen
// 单行
func add(a: int, b: int): int = a + b

// 多行
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

// 无返回值
func greet(name: string) {
    print("Hello, ", name);
}

// 函数是一等值
func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)
result = apply(add, 1, 2);
```

### 控制流

> **分号说明**：0.0.1 需显式写分号 `;`；0.0.2 起 ASI（自动分号插入）落地，
> 分号可选（显式分号仍合法），判定规则见 `docs/DESIGN.md` §3.9。
> **注意**：`func` 声明末尾无分号；函数体最后一个表达式无分号，否则返回值被丢弃。
> **表达式语句规则**：
> - if、while、choose 等控制流表达式作为语句使用时，0.0.1 必须加分号（0.0.2 起可省略）
> - choose-when 结构中，`when` 子句和 `otherwise` 子句本身不带分号
> - 只有 `choose` 整体作为表达式语句使用时，末尾才加分号
>
> **`while` 体的 `=` 语义**（DESIGN.md §4.5）：循环体内 `x = expr` 是对**外层可变 `x` 的赋值**，不是新绑定/遮蔽。详见 `docs/DESIGN.md`。

```fleen
// if 表达式（有返回值）
if x > 0 { "positive" } elif x < 0 { "negative" } else { "zero" };

// while 循环
while x < 10 {
    x = x + 1;    // 对外层的 x 赋值（§4.5），非遮蔽
};

// choose 表达式
choose x {
    when 0 { "zero" }
    when 1 { "one" }
    otherwise { "other" }
};
```

### choose（模式匹配）

```fleen
choose value {
    when 0 { "zero" }
    when 1 { "one" }
    when x if x > 10 { "big" }
    otherwise { "other" }
};
```

**分号规则**：`when` 子句和 `otherwise` 子句本身不带分号，只有 `choose` 整体作为语句使用时末尾加分号。

### 运算符优先级（从低到高）

```
or
and
== !=
< > <= >=
+ -
* / %
?      (后缀，0.0.2)
box deref move clone - !    (一元前缀；box/deref/move/clone 为 0.0.2 新增)
;       (显式分号：0.0.1 必填，0.0.2 起可选)
```

---

## 15. 特性范围

| 特性 | 0.0.1 | 0.0.2 |
|------|-------|-------|
| `=` 绑定/赋值 | ✅ | ✅ |
| `const` 不可变 | ✅ | ✅ |
| `int` / `bool` / `string` / `float` | ✅ | ✅（string 转为 owned 语义） |
| `func` 单行 / 多行 | ✅ | ✅ |
| `if` / `elif` / `else` | ✅ | ✅ |
| `while` | ✅ | ✅ |
| `choose` | ✅ | ✅（+ `Ok`/`Err` pattern） |
| `Result` | ✅（仅类型） | ✅（+ `Ok`/`Err` 构造、`?`） |
| 函数类型 | ✅ | ✅ |
| 分号 `;` | ✅ 必填 | ✅ 可选（ASI） |
| `?` | ❌ | ✅ |
| `move` / `clone` | ❌ | ✅ |
| `box<T>` | ❌ | ✅ |
| `ref` | ❌ | ✅（仅参数） |
| `struct` | ❌ | ❌（0.0.3） |
| `for` | ❌ | ❌（0.0.3） |
| `async` | ❌ | ❌（0.0.4） |
| FFI | ❌ | ❌ |
| 泛型 | ❌ | ❌（0.1.0） |
| 运算符重载 | ❌ | ❌（0.1.0） |
| 指针 `ptr` / `addr` | ❌ | ❌（0.0.5） |

---

## 16. 总结

| 方面 | 规定 |
|------|------|
| 语言 | Rust 2024 |
| 命名 | Rust 标准 + AST 节点对应文法 |
| 错误 | Result + Diagnostic |
| 数据 | enum + struct + Span + ID |
| 阶段 | Lex → Parse → Resolve → Typeck → Lower → Codegen → Execute |
| 测试 | 单元 + 集成 + 端到端 |
| 文档 | 公开 API 必须文档 |
| 性能 | 避免热路径 clone，预分配 |
| Git | Conventional Commits |
| 禁止 | unwrap、panic、全局可变、跳阶段 |
| 分号 | 0.0.1 必填 `;`；0.0.2 起可选（ASI） |
