# Fleen 0.0.2 实现任务拆解

> **目标**：所有权起步 —— `move` / `clone` / `box<T>` / `ref` / `?`（含 `Ok`/`Err`）/ ASI 端到端跑通，
> 偿还 0.0.1 的 print Hack、`Rc<str>`、`span_map` 三笔债务
> **依据**：`docs/0.0.2/PLAN.md`（已定稿，决策点 D1–D8 冻结）、`docs/BYTECODE.md` v0.0.2、
> `docs/DESIGN.md` §3.8–3.9 / §8 / §10、`docs/0.0.1/TICKETS.md`（实现现状）
> **状态**（2026-10-07）：全部待实现。规范文档已在规划阶段同步完毕，
> U12 仅剩实现后的收尾（版本号、正式 CHANGELOG、Hack 清空）。

---

## 总览

| 票号 | 板块 | 阶段 | crate | 模块 | 产出 | 依赖 |
|------|------|------|-------|------|------|------|
| U01 | 词法 | Lex | fleen-compiler | `lexer/` | Move/Clone/Deref/Question token | — |
| U02 | 语法 | Parse | fleen-compiler | `parser/` | 新 AST 节点 + EBNF 对齐 | U01 |
| U03 | F5 ASI | Parse | fleen-compiler | `parser/stmt.rs` | 分号可选（三集合判定） | U02 |
| U04 | F4+F6 | Typeck | fleen-compiler | `typeck/` | Ok/Err 构造、Result pattern、`?`、print 修复 | U02 |
| U05 | F1 所有权 | Typeck | fleen-compiler | `typeck/ownership.rs` | is_copy + 流敏感仿射检查 | U04 |
| U06 | 基础 | Lower | fleen-compiler | `lower/` | 新 MIR 指令 + MirInstr 携带 Span | U04, U05 |
| U07 | 基础 | Codegen/Verify | fleen-compiler, fleen-verify | `codegen/`, `verify.rs` | 字节码 v2 + span_map 生成 + 栈效果表 | U06 |
| U08 | 基础 | VM-Value | fleen-vm | `value.rs` 等 | Value 所有权表示重构 | —（可与 U01–U05 并行） |
| U09 | F1–F4 执行 | Execute | fleen-vm | `vm.rs` | 13 条新指令执行 + 入口 Err | U07, U08 |
| U10 | F7 诊断 | Codegen/VM | fleen-vm | `diag.rs`（新） | 运行时错误携带源码行列 | U07 |
| U11 | 测试 | 集成 | tests/, 各 crate | 全仓 | 测试矩阵 + E2E + v1 fixture | U09 |
| U12 | 收尾 | 发布 | 全仓 | 文档/Cargo.toml | 0.0.2 发布 | U11 |

> **阶段边界不变**（SPEC §6）：所有权检查是 Typeck 内的子遍历（`typeck/ownership.rs`），
> ASI 是 Parser 内的判定逻辑，都不新增独立阶段。

---

## U01: Lex — 新关键字与 `?`

### 涉及文件
```
fleen-compiler/src/lexer/
├── token.rs         # TokenKind 新增 4 个变体
└── lexer_impl.rs    # 关键字/标点映射表 + 单元测试（mod tests）
```

> 注：0.0.1 起测试就位于 `lexer_impl.rs` 的 `mod tests`，无独立 `tests.rs`。

### TokenKind 新增
```rust
/// `move` 关键字（0.0.2：所有权转移）
Move,
/// `clone` 关键字（0.0.2：深拷贝）
Clone,
/// `deref` 关键字（0.0.2：box 点内值读写）
Deref,
/// `?`（0.0.2：Result 传播后缀）
Question,
```

### 实现要求
- `lexer_impl.rs`：`"move" => Move`、`"clone" => Clone`、`"deref" => Deref`；`'?' => Question`
- `BoxType`（`box`）/ `RefType`（`ref`）已存在，**不动**；表达式位置的 `box` 复用
  `BoxType` token（parser 按语法位置区分，见 U02）
- `is_keyword()` / `keyword_str()` / `Display` 同步补齐
- **破坏性变更**：`move` / `clone` / `deref` 从合法标识符变为保留字
  （`box` / `ref` 在 0.0.1 已是保留字）。lexer 本身只产出 keyword token 不报错，
  "保留字用作绑定名"（`move = 1;`）由 parser 在"期望标识符"处报错，
  用例放 `tests/parser/invalid/`（见 U02）

### 测试
- `tests/lexer/valid/`：每个新 token ≥1 用例（含 `?` 紧跟表达式的形态 `x?`）
- 既有 lexer 测试全部原样通过（无 token 编码变化）

### 验收
```
cargo test -p fleen-compiler lexer
cargo fmt -- --check && cargo clippy --all-targets -- -D warnings
```

---

## U02: Parse — 新表达式、Result pattern、deref 赋值目标

### 涉及文件
```
fleen-compiler/src/parser/
├── ast.rs           # 新 AST 节点
├── expr.rs          # 一元前缀 + ? 后缀 + 赋值目标
├── stmt.rs          # pattern 扩展
├── error.rs         # （如需）新 ParseErrorKind
└── tests.rs
```

### AST 新增节点
```rust
// 一元前缀（与 Neg/Not 同层，右结合）
ExprMove  { place: Box<Expr>, span },   // move <place>
ExprClone { place: Box<Expr>, span },   // clone <place>
ExprBox   { inner: Box<Expr>, span },   // box <expr>
ExprDeref { inner: Box<Expr>, span },   // deref <postfix>

// postfix（与 Call/Index/Field 同层）
ExprQuestion { inner: Box<Expr>, span }, // <expr>?
```

> **parser 只认形态，语义归 typeck**（SPEC §11：不在 parser 做类型检查）：
> - `move` 的操作数接受任意 postfix 表达式，typeck 校验必须是 `Ident`（U05）；
> - `clone` 的操作数校验为 `Ident` 或 `ExprDeref`（U05）；
> - `box` 复用 `TokenKind::BoxType`：类型位置（`box<T>`）走既有类型解析，
>   表达式位置（`box expr`）走一元前缀分支，二者无歧义。

### Pattern 扩展
```rust
pub enum Pattern {
    Literal(Expr),
    Ident(Ident),
    NegLiteral(...),                       // 0.0.1 P5 已有
    ResultCtor { ctor: ResultCtor, binding: Ident, span },  // 0.0.2 新增
}

pub enum ResultCtor { Ok, Err }
```
- 语法形态：`Ok(ident)` / `Err(ident)`；parser 不绑定 `Ok`/`Err` 的语义
  （它们仍是 `Ident`，typeck 在 pattern 检查时识别形态）

### 赋值目标扩展
- 赋值 LHS 合法集合从 `{ Ident }` 扩为 `{ Ident, ExprDeref }`
  （`deref b = expr;` 写 box 点内值）；`InvalidAssignmentTarget` 判定同步更新
- `deref b = expr` 不可带类型标注（与 §3.4 一致，parser/Resolver 拒绝）

### 优先级（写入 `expr.rs` 并与 `SYNTAX.ebnf` v0.0.2 对齐）
```text
...（0.0.1 各层不变）
postfix:  primary { call | index | field | "?" }     ← ? 最高（后缀）
unary:    [ box | deref | move | clone | - | ! ] postfix
```
- `clone deref b` = `clone(deref(b))`（前缀右结合天然成立）
- `x?` 是后缀，绑定比一元紧：`deref b?` = `deref(b?)`

### 测试
- `tests/parser/valid/`：前缀链（`clone deref b`、`box box 1`）、`?` 链
  （`f(x)??`——嵌套 Result）、`deref b = v`、`Ok(v)`/`Err(e)` pattern、
  `move`/`clone` 作实参 `f(move x)`
- `tests/parser/invalid/`：悬空 `?`、`move`（无操作数）、`deref`（非 postfix）、
  `deref b: int = v`（带类型标注）
- 既有 parser 测试全部原样通过

### 验收
```
cargo test -p fleen-compiler parser
```

---

## U03: Parse — ASI（分号可选）

### 涉及文件
```
fleen-compiler/src/parser/
├── asi.rs           # 新增：三集合纯函数（可独立单测）
├── stmt.rs          # expr_stmt / block 收尾逻辑改造
└── tests.rs
```

### asi.rs（新文件，纯函数）
```rust
/// 该 token 能否开启一条新语句（块起始位置用，含 `(` `-` `!`）。
pub(crate) fn can_start_stmt(kind: &TokenKind) -> bool;

/// 已完成一条语句后，该 token 是否意味着"隐式结束"。
/// 起始集 ∪ { RBrace, Eof }，但**不含** `(` `[` 与二元运算符（续接优先）。
pub(crate) fn implies_stmt_end(kind: &TokenKind) -> bool;

/// 该 token 是否续接当前表达式（二元运算符 / `?` / `.` / `[` / `(`）。
pub(crate) fn can_continue_expr(kind: &TokenKind) -> bool;
```

### 判定规则（expr_stmt 收尾处）
```text
语句完成后：
1. 下一 token 是 Semi            → 消费（显式分号永远合法）
2. implies_stmt_end(t)           → 隐式结束，不消费
3. can_continue_expr(t)          → 不结束，回到表达式解析（续接优先）
4. 其余                          → 报 ExpectedSemiOrNewStmt { span, found }
```

### 集合定义（与 `DESIGN.md` §3.9 表格逐条对应）
| 函数 | 成员 |
|------|------|
| 起始集（块起始语境） | `Func Const If While Choose Import`、`Ident IntLit FloatLit StringLit True False`、`BoxType Deref Move Clone`、`Bang Minus LParen` |
| implies_stmt_end | 起始集 **∖ {`LParen`, `Minus`}** `∪ { RBrace, Eof }`——`(` 与 `-` 同属续接集，续接优先，不可据此结束语句 |
| 续接集 | `Plus Minus Star Slash Percent Eq Ne Lt Gt Le Ge And Or`（二元语境）、`Question Dot LBracket LParen` |

> `-` 在语句完成后总是二元续接（`x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`）；
> 要开新的负数语句须写 `;`。`(` `[` 永远续接（调用/索引）——陷阱用例进测试。

### 实现要求
- 改造点集中在"期望 Semi"的所有位置：`expr_stmt` 收尾、block 内语句循环；
  `func` 声明后、块尾表达式、`when`/`otherwise` 的"无分号"规则**不变**
- 多余分号（`;;`）的行为以 0.0.1 现有测试为基准保持不变
- ASI 是纯 parser 行为：Token 流、AST 结构、MIR、字节码均不受影响
  （AST 的 `has_semi` 标志字段保留，typeck/lower 不感知差异）

### 测试（陷阱用例逐条，见 `DESIGN.md` §3.9 表）
- valid：`x = 1` ⏎ `y = 2`；`while { x = x + 1 }` 无分号；when/otherwise 无分号；
  显式分号混合风格；0.0.1 全部 valid 用例原样通过（回归底线）
- invalid：`x = 1` ⏎ `@`（不可续接不可起始）报 `ExpectedSemiOrNewStmt`；
  `if c { 1 }` ⏎ `else { 2 }`（else 不续接已完结的 if 语句）
- 专项单测：`can_start_stmt` / `implies_stmt_end` / `can_continue_expr`
  对每个 `TokenKind` 的判定表（穷举测试）

### 验收
```
cargo test -p fleen-compiler parser
cargo test   # 0.0.1 全量回归
```

---

## U04: Typeck — Ok/Err 构造、Result pattern、`?`、print 修复

### 涉及文件
```
fleen-compiler/src/typeck/
├── infer.rs         # 移除 print 特判；新增构造/`?` 检查
├── builtins.rs      # 新增：内建签名表
├── typed_hir.rs     # TypedExprHir::Question 等新节点
├── error.rs         # 新 TypeckErrorKind
└── tests.rs         # check_builtin_print_wrong_arg 回正
```

### builtins.rs（新文件：内建签名表，替代 print 特判）
```rust
pub enum BuiltinParam {
    /// 单一固定类型序列（未来内建扩展用）
    Fixed(&'static [Type]),
    /// 可变元数，每个参数 ∈ 集合
    Variadic(&'static [Type]),
}

pub struct BuiltinSig {
    pub name: &'static str,
    pub param: BuiltinParam,
    pub ret: Type,
}

pub const BUILTINS: &[BuiltinSig] = &[
    // print: (printable...) -> unit；printable = {int, float, bool, string, unit}
    BuiltinSig { name: "print", param: Variadic(PRINTABLE), ret: Type::Unit },
];
// PRINTABLE = [Int, Float, Bool, String, Unit]
```
- `typeck_call` 改查此表：`print(func值)` / `print(box)` / `print(Ok(..))`
  → `ArgTypeMismatch`；元数不限（`print()` 合法）
- **移除 `infer.rs` 中 `print` 的特判分支与 HACK 注释**
- `check_builtin_print_wrong_arg` 测试拆分回正：
  `print(1)` / `print("a", 1, true)` → Ok；`print(fib)`（函数值）→ `ArgTypeMismatch`

### Ok / Err 构造检查（双向检查最小实现）
0.0.1 的推导是单向的（先推后比）；`Ok(v)`/`Err(e)` 需要上下文类型
（PLAN D6）。实现为**期望类型线程**（checking mode），只穿三条路径：

```rust
// typeck_expr 增加 expected 参数（None = 纯推导）
fn typeck_expr(&mut self, expr: ExprHir, expected: Option<&Type>) -> Result<TypedExprHir, ()>
```
1. **函数体**（块尾与单表达式体）：expected = 声明的返回类型（有标注时）；
   expected 沿 `if/elif/else` 分支尾、`choose` 臂尾、块尾继续下传
2. **带类型标注的绑定初始化**：`res: Result<int, string> = Ok(42);`
3. **`?` 的操作数**：自身类型已是 `Result`（无 expected 需求）

`Ok(arg)` / `Err(arg)` 检查规则：
- expected = `Result<T, E>` → `Ok` 校验 `arg: T`，`Err` 校验 `arg: E`；
- expected = None → `CannotInferResultType { span }`，help 提示补标注
- 实参个数为 0 或 ≥2 → `ArityMismatch`
- `Ok`/`Err` 被用户遮蔽（§4.3 内建遮蔽规则）→ 按普通函数调用处理，不报错

### Result pattern（choose）
- scrutinee 类型 = `Result<T, E>` 时：`ResultCtor::Ok` 绑定 `v: T`、
  `Err` 绑定 `e: E`（转移语义，U05 登记）；guard 可用绑定
- **穷尽性**：`Ok` + `Err` 两臂齐（guard 臂不计）→ 穷尽，无需 `otherwise`；
  只有其一 → `ChooseNotExhaustive { missing_patterns: [Ok/Err] }`
- scrutinee 非 `Result` 却写 `Ok/Err` pattern → `PatternTypeMismatch`

### `?` 检查
- 操作数类型 `Result<T, E>`，否则 `QuestionOnNonResult { found }`
- 所在函数返回类型标注必须为 `Result<T2, E>` 且 `E` **完全相等**（D6）：
  - 函数无返回标注 / 非 Result → `QuestionOutsideResultFn`（help：改函数返回类型
    或用 `choose` 显式处理）
  - `E` 不等 → `QuestionTypeMismatch { expected, found }`
- `main` 返回 `Result<int, string>` 时 `?` 合法（入口 Err 行为见 U09）

### UnhandledResult
- 表达式语句（`Stmt::Expr`）的值类型为 `Result<T, E>` → `UnhandledResult`
  （Result 不允许静默丢弃）；提示：绑定、`choose` 消化，或加 `?` 传播

### 新 TypeckErrorKind
```rust
QuestionOutsideResultFn,
QuestionOnNonResult { found },
QuestionTypeMismatch { expected, found },
CannotInferResultType { ctor },          // ctor: Ok | Err
UnhandledResult { ty },
PatternTypeMismatch { expected, found },
ArgTypeMismatch { .. },                  // 复用既有（print 回正后重新可用）
```

### 测试
- valid：`div/ratio` 链（PLAN §3.4 示例逐字）、带标注绑定、`main` 返回 Result、
  `print(1)`、`print()`、用户遮蔽 `Ok`
- invalid：无上下文 `Ok(1)`、`?` 在 int main、E 不匹配、丢弃 Result、
  `print(fib)`、Result scrutinee 不穷尽
- **回归底线**：`check_builtin_print` 等既有 typeck 测试语义不变

### 验收
```
cargo test -p fleen-compiler typeck
grep -n "print" src/typeck/infer.rs   # 仅剩正常调用路径，无特判/HACK
```

---

## U05: Typeck — 所有权检查器

### 涉及文件
```
fleen-compiler/src/typeck/
├── ownership.rs     # 新增：流敏感仿射检查（本票核心）
├── typed_hir.rs     # Ident 节点增加 access 标注
├── infer.rs         # is_copy()、类型判定接入
├── error.rs         # 新错误族
└── tests.rs
```

### 类型判定（infer.rs）
```rust
impl Type {
    /// Copy 类型：int/float/bool/unit/函数值；Result<T,E> = is_copy(T) && is_copy(E)
    pub fn is_copy(&self) -> bool;
    /// owned：string、box<_>、含 owned 分量的 Result
    pub fn is_owned(&self) -> bool;
}
```
- `Type::Box` / `Type::Ref` 从"解析但 Unsupported"转正：
  移除 `UnsupportedType` 相关特判（0.0.1 的 `codegen`/`lower` 遇 box/ref 报错
  的路径由 U06 的真实降载替代）

### 访问标注（typed_hir.rs）
```rust
/// Ident 节点新增字段：本次使用的所有权访问形态
pub enum Access {
    Copy,    // Copy 类型读取（默认）
    Move,    // move x（消耗性，槽清空）
    Clone,   // clone x / owned 的只读使用（深拷贝，槽不动）
}
// TypedExprHir::Ident { name, binding_id, hir_id, span, access }
```
- 由 ownership 检查器回填；lower（U06）据此选择 `MoveLocal`/`CloneLocal`/`LoadLocal`
- ref 实参不需要 Access：lower 在 `Call` 处按被调参数类型决定发 `MakeRefLocal`

### 所有权状态机（ownership.rs）
```rust
enum MoveState {
    Alive,
    Moved { span: Span },          // 确定已移动
    MaybeMoved { spans: Vec<Span> }, // 部分分支已移动
}

struct OwnershipChecker {
    state: HashMap<BindingId, MoveState>,
    errors: &mut Vec<TypeckError>,
}
```

**遍历规则（与 `DESIGN.md` §10.2 六条规则一一对应）：**

| 构造 | 处理 |
|------|------|
| 绑定/赋值 RHS 顶层为 `move x` | 登记 `x → Moved`；之后读写/移动 `x` → `UseAfterMove`；对 `x` 再赋值 → `AssignToMoved`（错误消息与未绑定区分） |
| RHS 顶层为 `clone x` / `clone deref b` | 深拷贝语义，状态不变 |
| RHS 顶层为裸 `x`（owned） | `OwnedArgRequiresMove`（help：`move` 或 `clone`） |
| 实参为裸 owned `x` | 同上（位于 `Call` 参数检查） |
| `choose` scrutinee 为裸 owned `x` | 同上（D7）；`move x`/`clone x` 合法 |
| 函数尾/块尾/分支尾的裸 owned `x` | 产出位置：隐式转移，登记 Moved/条件转移 |
| `if/elif/else` | 各分支从快照独立推导；汇合：全 Moved → Moved，部分 → MaybeMoved，否则 Alive |
| `while` | **保守规则**：体内 `move x` ⇒ 出口 `x` 视为 MaybeMoved；体内移动后（含回边处）再用 `x` → `UseAfterMove`；guard 中使用体内会移动的变量 → `MaybeMovedAfterBranch` |
| `choose` | 同 if（臂顺序汇合）；Result pattern 绑定按转移登记 |
| 函数边界 | 状态清空，不穿越 |
| 比较运算 / `print` 实参 | 只读使用（D3）：owned 操作数自动标注 `Access::Clone`，状态不变 |
| `deref b`（读取） | 副本语义，`b` 状态不变 |

**place 合法性：**

| 表达式 | 判定 |
|--------|------|
| `move x`（x 为 Copy） | `MoveOfCopyType`（help：去掉 `move`） |
| `move deref b` | `MoveOutOfBox`（help：`clone deref b`） |
| `move g`（owned 全局） | `MoveOutOfGlobal`（help：`clone g`） |
| `move s`（s 为 ref 参数） | `MoveOfBorrowed` |
| `clone` 操作数非 Ident/ExprDeref | `InvalidClonePlace` |
| 实参 `move x` 给 ref 参数 | `BorrowArgWithMove`（借用不接受转移） |
| 给 ref 参数赋值 | `AssignToRefParam` |
| `ref T` 出现在非参数类型位置 | `RefNotAllowedHere`（局部/全局/返回/嵌套 `ref ref T`；**本票在 Resolver 阶段检查**，见下） |

**Resolver 分工**：`RefNotAllowedHere` 依赖声明形态（绑定/const/返回类型位置），
由 `resolver/mod.rs` 在遍历类型标注时检查，错误并入 `ResolveErrorKind`；
其余所有权错误都在 typeck。

**发现于设计评审的实现限制（本票新增）**：`deref b = v` 的赋值目标必须是
**局部** box——若 `b` 是 owned 全局，读取即 `CloneGlobal`（复制整个 box），
写穿透只会改副本。typeck 对全局 box 的 deref 赋值报 `DerefAssignOfGlobalBox`
（0.0.2 限制；若放开需新增 `StoreGlobalDerefBox` 指令，留待后续版本）。
U12 将此限制补录 `DESIGN.md` §10.3。

### 新 TypeckErrorKind / ResolveErrorKind
```rust
// typeck
UseAfterMove { name, use_span, moved_span },
MaybeMovedAfterBranch { name, use_span, moved_spans },
AssignToMoved { name, span },
MoveOfCopyType { name, ty },
MoveOutOfBox,
MoveOutOfGlobal { name },
MoveOfBorrowed { name },
InvalidClonePlace,
OwnedArgRequiresMove { name, ty },   // help: `move x` / `clone x`
BorrowArgWithMove { name },
DerefAssignOfGlobalBox,
// resolver
RefNotAllowedHere { span, context },
```

### 测试
- valid：PLAN §3.1 全部示例、条件转移后不使用、循环内 move 后循环内不再用、
  owned 全局读取、`clone deref b`
- invalid：六条核心规则各 ≥1 用例（对应上表每个错误 kind）、
  while 回边后使用、`x = move x`、全局 box deref 赋值
- 穷举 `is_copy`：对每种 `Type` 变体断言

### 验收
```
cargo test -p fleen-compiler typeck resolver
```

---

## U06: Lower — 新 MIR 指令 + Span 下沉

### 涉及文件
```
fleen-compiler/src/lower/
├── mir.rs           # MirInstr 包装重构 + 新指令
├── mod.rs           # 降载规则
├── slots.rs         # （如需）owned 绑定的槽位注释
└── tests.rs
```

### MirInstr 重构（Span 下沉，PLAN §4.5）
```rust
// 由裸 enum 改为 struct 包装：
pub struct MirInstr {
    pub kind: MirInstrKind,
    pub span: Span,          // 0.0.1 的 MIR 无 span，本票从 TypedHir 逐指令携带
}

pub enum MirInstrKind {
    // ……0.0.1 全部变体原样迁入
    // 0.0.2 新增：
    AllocBox,
    DerefBox,
    StoreDerefBox,
    MakeRefLocal(u16),
    DupDeep,
    MoveLocal(u16),
    CloneLocal(u16),
    CloneGlobal(u16),
    PackOk,
    PackErr,
    IsErr,
    UnwrapOk,
    UnwrapErr,
}
```
- 机械重构波及：`lower/mod.rs`、`cfg.rs`、`codegen/encoder.rs`、全部 lower 测试——
  单独一个 PR 完成，**不改任何断言语义**（0.0.1 回归底线）

### 降载规则（TypedHir → MIR）

| 源构造 | MIR 序列 |
|--------|----------|
| `box e` | `<e>`; `AllocBox` |
| `deref b`（读） | `<b>`; `DerefBox`（box 保留在栈，点内副本压栈） |
| `deref b = v` | `<b>`; `<v>`; `StoreDerefBox` |
| `move x`（Access::Move） | `MoveLocal slot` |
| `clone x`（Access::Clone） | `CloneLocal slot` |
| owned 全局读取（只读/clone） | `CloneGlobal gid` |
| owned 实参（Access::Move/Clone） | `MoveLocal`/`CloneLocal` + `Call`/`CallValue` |
| ref 实参（place = 局部/全局/ref 参数） | `MakeRefLocal slot` + `Call`（全局则先…见下注） |
| `x?` | `Dup`; `IsErr`; `JumpIfTrue L_err`; `UnwrapOk`; `Jump L_cont`; `L_err: UnwrapErr`; `Return`; `L_cont:`（`BYTECODE.md` §6.7） |
| 比较 / print 的 owned 操作数 | 先 `CloneLocal`/`CloneGlobal`（D3 只读使用） |
| choose（Result scrutinee） | `Dup`; `IsErr`; `JumpIfTrue L_err`; `UnwrapOk`; `StoreLocal v`; 臂体; …（`BYTECODE.md` §6.8） |

> **ref 实参指向全局**：`MakeRefLocal` 句柄绑定**本帧** `stack_base + slot`，
> 无法指全局表。实现：全局 ref 实参先把值**复制**到一个临时局部槽
> （`CloneGlobal` + `StoreLocal`），再 `MakeRefLocal` —— 语义等价（借用是只读的，
> 调用方在此期间不会写该全局；复制成本可接受）。此取舍记入 U12 文档核对清单。

### 测试
- `tests/parser/`… 不涉及；`lower/tests.rs`：每条降载规则 ≥1 用例，
  与 `BYTECODE.md` §5.9–5.11 / §6.7–6.8 的形态逐指令比对
- Span 断言：每条 MIR 指令的 span 覆盖对应源码区间

### 验收
```
cargo test -p fleen-compiler lower
```

---

## U07: Codegen / Verify — 字节码 v2

### 涉及文件
```
fleen-compiler/src/codegen/
├── bytecode.rs      # Opcode 新增 + Module.version
├── encoder.rs       # 编码 + span_map 生成
├── flnc.rs          # version 接受 {1, 2}
└── tests.rs

fleen-verify/src/
├── verify.rs        # 新指令校验规则
├── stack_analysis.rs # Δdepth 表
└── tests.rs
```

### Opcode（bytecode.rs，与 `BYTECODE.md` §5.9–5.11 逐字节一致）
```rust
// 0x80–0x83 box / ref
AllocBox      = 0x80,
DerefBox      = 0x81,
StoreDerefBox = 0x82,
MakeRefLocal  = 0x83,
// 0x90–0x93 move / clone
DupDeep       = 0x90,
MoveLocal     = 0x91,
CloneLocal    = 0x92,
CloneGlobal   = 0x93,
// 0xA0–0xA4 Result
PackOk        = 0xA0,
PackErr       = 0xA1,
IsErr         = 0xA2,
UnwrapOk      = 0xA3,
UnwrapErr     = 0xA4,
```
- 操作数宽度：`MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal` =
  u16（指令 3 字节）；其余 1 字节
- `Module.version` 写 `2`；`flnc::from_bytes` 接受 `{1, 2}`（v1 旧文件可读），
  `to_bytes` 写 2

### span_map 生成（encoder.rs）
- 两遍编码已存在（第一遍指令+占位标签，第二遍解析偏移）：第一遍同时记录
  每条指令的 `(待定 offset, span)`，第二遍回填 offset，产出
  `SpanEntry { offset, start, end }`（`BYTECODE.md` §2）
- 0.0.1 产物的 `span_map` 恒为空——序列化格式不变，仅从"留空"变"填充"

### verify.rs / stack_analysis.rs
- `stack_effect()` 增补（`BYTECODE.md` §8 表）：
  - `AllocBox` / `DerefBox` / `DupDeep`：Δ+1，min 1
  - `MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal`：Δ+1，min 0
  - `StoreDerefBox`：Δ−2，min 2
  - `PackOk` / `PackErr` / `IsErr` / `UnwrapOk` / `UnwrapErr`：Δ0，min 1
- slot 校验：`MakeRefLocal` / `MoveLocal` / `CloneLocal` 的 slot < `locals`
  （与 `LoadLocal` 同规则）；`CloneGlobal` 的 gid < globals 表长
- 版本门：v2 指令出现在 version=1 模块 → 既有 `BadOpcode` 自然成立；
  version=2 模块走全量同一套校验

### 测试
- codegen：13 条新指令的编码字节逐一断言；span_map 非空且 offset 落在指令边界
- verify：对每个新指令构造合法/畸形（栈不匹配、slot 越界）模块
- 兼容：v1 模块（fixture，见 U11）verify PASS、执行输出不变

### 验收
```
cargo test -p fleen-compiler codegen
cargo test -p fleen-verify
```

---

## U08: VM — Value 所有权表示重构（可与前端并行）

### 涉及文件
```
fleen-vm/src/
├── value.rs         # Value 枚举重构
├── vm.rs            # 受 Rc<str> → Box<str> 影响的路径
├── error.rs         # 预留两个新变体（U09 使用）
└── tests.rs
```

### Value 重构（`BYTECODE.md` §3.2 v0.0.2）
```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),                 // ← 由 Rc<str> 改为独占所有权
    Boxed(Box<Value>),             // box<T>
    Ref { base: u32, slot: u16 },  // 借用句柄（U09 使用）
    Ok(Box<Value>),                // Result
    Err(Box<Value>),
    Unit,
    Func(FuncId),
}
```

### 实现要求
- 移除 `use std::rc::Rc`；`Display` 补齐：`Boxed` 打印点内值、`Ref` 打印
  `<ref>`、`Ok/Err` 打印 `Ok(v)` / `Err(e)`
- `Clone` / `PartialEq` derive 保持：`Box<Value>` 自动深拷贝/按结构比较
  （`Eq` 指令语义：`Str` 按内容、`Boxed` 比点内值、`Ref` 解引用——解引用逻辑
  在 U09 的指令分派里做，本票只保证 derive 行为不破坏 v1 指令）
- **v1 语义零变化**：0.0.1 的全部 36 条 opcode 在 `Box<str>` 下行为与 `Rc<str>`
  可观测等价（字符串不可变、无别名逃逸路径）；全部 0.0.1 VM 测试原样通过
- `error.rs` 预留：
  ```rust
  ResultMismatch,      // UnwrapOk 遇 Err（防御）
  BorrowOutOfRange,    // 句柄 base+slot 越界（防御）
  ```

### 验收
```
cargo test -p fleen-vm
grep -rn "Rc<" src/ | wc -l   # = 0
```

---

## U09: VM — 新指令执行与入口 Err

### 涉及文件
```
fleen-vm/src/
├── vm.rs            # step() 分派扩展 + 句柄解引用 helper
├── error.rs         # ResultMismatch / BorrowOutOfRange 正式启用
├── main.rs          # 入口 Err → stderr + 退出码 1
└── tests.rs
```

### 指令分派（vm.rs `step()`）

| 指令 | 实现 |
|------|------|
| `AllocBox` | `let v = pop(); push(Value::Boxed(Box::new(v)))` |
| `DerefBox` | `let b = peek()`；`push(inner.clone())`——`b` 为 `Ref` 时先解句柄（见下） |
| `StoreDerefBox` | `let v = pop(); let b = pop()`；`*inner = v`（旧值由赋值 Drop 释放） |
| `MakeRefLocal` | 栈顶为 `Ref` → 原样再压一份（转发借用）；否则压 `Value::Ref { base: frame.stack_base, slot }` |
| `DupDeep` | `let v = peek().clone(); push(v)`（`Box<Value>` 自动深拷贝） |
| `MoveLocal` | `let v = std::mem::replace(&mut stack[base+slot], Value::Unit); push(v)` |
| `CloneLocal` | `push(stack[base+slot].clone())`；值为 `Ref` 时深拷贝**句柄目标** |
| `CloneGlobal` | 同 `CloneLocal`，作用于全局表 |
| `PackOk` / `PackErr` | `pop` 后包 `Ok/Err` 压回 |
| `IsErr` | `pop r; push(Bool(matches!(r, Err(_))))` |
| `UnwrapOk` | `pop r`；`Ok(v) → push(v)`；`Err → RuntimeError::ResultMismatch`（防御） |
| `UnwrapErr` | 对称 |

### 句柄解引用（统一 helper）
```rust
/// 只读解引用：Ref → 指向的槽位值；Boxed → 自身；其余 → 自身。
/// 句柄只允许指向"当前调用链更下方"（base 更小）的帧内槽位。
fn read_ref(&self, v: &Value) -> Result<&Value, RuntimeError> {
    match v {
        Value::Ref { base, slot } => self.stack.get(*base + *slot as usize)
            .ok_or(RuntimeError::BorrowOutOfRange),
        other => Ok(other),
    }
}
```
- 使用点：`Eq`/`Ne`（两侧）、`exec_builtin`（print 实参）、`CloneLocal`/`CloneGlobal`/
  `DupDeep`/`DerefBox` 的源值——消费 `Ref` 的指令必须先解引用
- **纪律**：`Ref` 不得出现在 `Boxed` / `Ok` / `Err` 内部（typeck 保证）；
  `debug_assert!` 辅助，违反即防御性 `BorrowOutOfRange`

### Drop 语义核对（无新代码，须验证）
- 帧销毁：`Return` 后 `stack.truncate(stack_base)` —— Rust 对被截断元素逐个
  `Drop`，owned 值（`Str`/`Boxed`/`Ok`/`Err`）确定性释放 ✓
- 槽位覆盖：`StoreLocal`/`StoreGlobal` 索引赋值释放旧值 ✓
- `Pop` 丢弃语句值：`Vec::pop` Drop ✓
- 单测：clone 后改写点内值互不影响；move 后原槽为 `Unit`；嵌套 box 深释放

### 入口 Err（main.rs，D8）
```rust
match vm.run() {
    Ok(Value::Err(e)) => { eprintln!("error: {e}"); process::exit(1); }
    Ok(v) => { /* 现行为不变 */ }
    Err(e) => { /* 现行为不变（U10 起 e 携带 span） */ }
}
```

### 验收
```
cargo test -p fleen-vm
```

---

## U10: 诊断 — span_map 消费与运行时行列

### 涉及文件
```
fleen-vm/src/
├── diag.rs          # 新增：字节偏移 → (line, col)
├── error.rs         # RuntimeError 携带可选 span
├── vm.rs            # 出错时查 span_map
└── main.rs          # .fln 模式打印源码位置；.flnc 模式退化为 函数名+偏移
```

### 设计
```rust
// diag.rs
/// 源码字节偏移 → (1-based line, 1-based col)；O(n) 单遍扫描，错误路径专用。
pub fn line_col(source: &str, offset: u32) -> (u32, u32);
```
- `RuntimeError` 增加 `span: Option<(u32, u32)>`：出错时按 `frame.ip - 1` 查
  当前函数 `span_map`（`offset` 单调，二分或线性），构造 `(start, end)`
- CLI 输出（`.fln` 一站式模式持有源码）：
  ```console
  runtime error: division by zero
    --> crash.fln:5:9
     |
   5 |     q = a / b;
     |         ^^^^^
  ```
- `.flnc` 直接执行：无源码，退化为 `runtime error: … (in `div`, pc 0x12)`
- **退出码契约不变**（0 成功 / 1 错误 / 2 用法·IO）

### 测试
- 除零 / 溢出 / 深递归错误的 span 正确性（行列各 ≥1 用例）
- span_map 为空（v1 模块）时不打印位置、不 panic
- stderr 快照进 e2e invalid 用例（`.stderr` 关键字）

### 验收
```
cargo test -p fleen-vm
cargo fln tests/e2e/invalid/e2e_div_zero.fln   # 手工核对输出格式
```

---

## U11: 测试 — 矩阵、E2E 与 v1 兼容夹具

### 文件驱动测试（各阶段 valid/invalid，对照 PLAN §8.1）
```
tests/lexer/{valid,invalid}/        # U01 新 token
tests/parser/{valid,invalid}/       # U02 新语法 + U03 ASI 陷阱表
tests/resolver/{valid,invalid}/     # U05 RefNotAllowedHere
tests/typeck/{valid,invalid}/       # U04 + U05 全部错误 kind
tests/e2e/{valid,invalid}/          # 见下
```

### E2E 新增
```
tests/e2e/valid/
├── fib.fln              # 0.0.1 回归：原样通过（显式分号风格）
├── fib_asi.fln          # 同程序、无分号：输出与 fib.expected 逐字节一致
├── ownership.fln        # move/clone/条件转移
├── box_demo.fln         # 分配/读写/clone 独立性/嵌套 box
├── ref_demo.fln         # 借用传参，原变量仍可用
└── question.fln         # Ok/Err/?/choose-Result 全链路

tests/e2e/invalid/
├── use_after_move.fln       # .exit=1 + .stderr 关键字（编译错误）
├── owned_assign_without_move.fln
├── unhandled_result.fln
├── question_in_int_main.fln
├── print_func_value.fln
└── ref_out_of_param.fln     # ref 局部变量
```

**question.fln**（端到端基准程序，兼作文档示例）：
```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func safe_half(a: int): Result<int, string> {
    q = div(a, 2)?;
    Ok(q)
}

func main(): int {
    res = choose safe_half(10) {
        when Ok(v) { v }
        when Err(e) {
            print("error: ", e);
            0 - 1
        }
    };
    print(res);
    0
}
```
`question.expected`：`5`

### v1 兼容夹具
- `tests/fixtures/v1/fib.flnc`：用 **0.0.1 工具链**（当前未动代码前）编译
  `fib.fln` 产出并提交为二进制 fixture
- 测试：`fleen-verify` PASS + `fleen-vm` 执行输出 == `fib.expected`
  —— 锁死"v2 不破坏 v1"的承诺（`BYTECODE.md` §10）
- 归属：`fleen-vm/tests/v1_compat.rs`（新集成测试文件）

### 回归底线
- `cargo test` 全绿，且 0.0.1 的 321 个测试**零语义改动**（仅 mechanical
  调整允许，如 MirInstr 包装的构造处）

### 验收
```
cargo test
cargo fln tests/e2e/valid/question.fln
```

---

## U12: 收尾 — 文档核对、版本与发布

### 文档核对（规划期已同步，实现后核对差异）
- [ ] `DESIGN.md` §10.3 补录 `deref` 赋值目标的"仅局部 box"限制（U05 发现）
- [ ] `DESIGN.md` §10.4 补录"全局 ref 实参经临时槽复制"的取舍（U06 发现）
- [ ] `DESIGN.md` §18 已知 Hack 节**清空**（print 已修）
- [ ] `BYTECODE.md` 校对 v2 指令表与最终 opcode/编码一致
- [ ] `SPEC.md` §14 快速参考补 `?` 示例（如实现形态有出入）
- [ ] 新增错误信息样例与实际输出比对（`UseAfterMove` 等）

### 版本与发布
- [ ] 三个 crate `Cargo.toml` 版本 → `0.0.2`
- [ ] `CHANGELOG.md`：`0.0.2 — 未发布（规划）` 改为正式条目
  （日期、去掉"规划"标注、补实现期间发现的行为细节）
- [ ] `README.md` 版本行更新（`0.0.2`，可考虑代号——0.0.1 是 Swift Fox 🦊）
- [ ] `cargo doc --no-deps` 无警告；公开 API（`fleen_vm::Value` 等）文档补齐
- [ ] CI 四件套全绿后打 tag、出 release 压缩包（沿用 0.0.1 流程）

### 验收
```
cargo fmt -- --check && cargo clippy --workspace --all-targets -- -D warnings
cargo test && cargo doc --no-deps
```

---

## 依赖关系与并行化

```
U01 (Lex)
  ↓
U02 (Parse) ──→ U03 (ASI)                    # U03 只依赖 U02，与 U04/U05 并行
  ↓
U04 (Typeck-签名) ──→ U05 (Typeck-所有权)
  ↓                    ↓
U06 (MIR+Span) ←───────┘
  ↓
U07 (Codegen/Verify)                          # U08 全程独立，可最先启动
  ↓                    ┌─────────────┐
U09 (VM-执行) ←──── U08 (VM-Value)           # U08 完成即可并入了
  ↓                    └─────────────┘
U10 (诊断) ←── U07                            # 只依赖 span_map 生成
  ↓
U11 (测试) ←── U09 + U10
  ↓
U12 (收尾)
```

- **单人串行建议顺序**：U08 → U01 → U02 → U03 → U04 → U05 → U06 → U07 → U09 → U10 → U11 → U12
  （先还 `Rc<str>` 债，VM 基础就绪后前端一路推进）
- **双人并行**：A 走前端 U01–U07，B 走 U08 → 与 A 会合于 U09，B 顺做 U10

---

## 质量闸（每票必须过）

```bash
cargo fmt -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test
cargo doc --no-deps
```

- 禁止事项照旧（SPEC §11）：无 `unwrap()`/`expect()`、无 `Rc`/`RefCell`
  （`Value::Boxed(Box<Value>)` 用 `Box`，不回退 `Rc`）、错误带 Span
- **每票合并前跑 0.0.1 全量回归**——`fib.fln` E2E 输出逐字节不变是硬闸

---

## 里程碑

| 里程碑 | 票号 | 标志 |
|--------|------|------|
| M1 前端 | U01–U03 | 新语法可解析；ASI 陷阱用例全绿；0.0.1 回归零变化 |
| M2 语义 | U04–U05 | Ok/Err/`?`/print 检查完备；所有权检查器 invalid 用例全绿 |
| M3 后端与运行时 | U06–U09 | `question.fln` / `box_demo.fln` / `ref_demo.fln` 端到端跑通；v1 fixture 通过 |
| M4 诊断与发布 | U10–U12 | 运行时错误带行列；文档核对完毕；0.0.2 发布 |

---

## 备注

- **0.0.2 仍不实现**：`struct` / `for` / 迭代器（0.0.3）、闭包、trait / 运算符重载 /
  泛型（0.1.0）、用户自定义 `Drop`、`ref` 局部/全局/返回值、借用 box 内部、
  `addr` / `ptr` / `unsafe`（0.0.5）、错误类型转换（`E` 须完全相等）
- **决策冻结**：D1–D8 见 `PLAN.md` §2。实现中如需推翻某决策，先改 PLAN 与
  规范文档，再动代码——不允许代码偏离文档
- **Breaking 变更清单**（CHANGELOG ⚠️ 小节的实现对照）：
  owned 赋值需 `move`/`clone`；`move`/`clone`/`deref` 成为保留字；
  `print(函数值)` 改为编译错误
- **v1 兼容承诺**：`.flnc` v1 模块在 0.0.2 verify/VM 上可执行（U07/U11 锁定）；
  0.0.1 源程序除 Breaking 清单三条外全部原样编译
- **print 函数**：仍内置于 VM 宿主（`is_builtin` 机制不变）；新增的只是 typeck
  侧的 `builtins.rs` 签名表
