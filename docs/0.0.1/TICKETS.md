# Fleen 0.0.1 实现任务拆解

> **目标**：Fibonacci 能跑
> **依据**：`SPEC.md §6` 编译阶段、`DESIGN.md §15` 特性范围、`BYTECODE.md` 指令集

---

## 总览

| 票号 | 板块 | 阶段 | crate | 模块 | 产出 | 依赖 |
|------|------|------|-------|------|------|------|
| T01 | B1 | Lex | fleen-compiler | `lexer/` | `Vec<Token>` | — |
| T02 | B2 | Parse | fleen-compiler | `parser/` | `Ast` | T01 |
| T03 | B3 | Resolve | fleen-compiler | `resolver/` | `Hir` | T02 |
| T04 | B3.5 | BindCheck | fleen-compiler | — | — | （已并入 T03 Resolver，不再单列） |
| T05 | B4 | Typeck | fleen-compiler | `typeck/` | `TypedHir` | T03 |
| T06 | B5 | Lower | fleen-compiler | `lower/` | `Mir` | T05 |
| T07 | B6 | Codegen | fleen-compiler | `codegen/` | `Bytecode` | T06 |
| T08 | B7 | Verify | fleen-verify | `src/` | PASS/FAIL | T07 (同步) |
| T09 | B8 | Execute | fleen-vm | `src/` | 运行结果 | T07+T08 |
| T10 | E2E | 集成 | tests/ | `e2e/fib.fln` | 输出匹配 | T09 |

---

## T01: B1 Lexer — 词法分析

### 文件创建
```
fleen-compiler/src/lexer/
├── mod.rs          # pub fn lex(source: &str) -> Result<Vec<Token>, LexError>
├── token.rs        # TokenKind, Token { kind, span }, LexError, Span
└── tests.rs        # 单元测试
```

### TokenKind 定义（对应 `SYNTAX.ebnf`）

> 实际定义见 `fleen-compiler/src/lexer/token.rs`。要点：
> - 关键字：`Func, Const, If, Elif, Else, While, Choose, When, Otherwise, Import`
> - 类型关键字：`IntType, FloatType, BoolType, StringType, UnitType, ResultType, BoxType, RefType`（类型由专属 token 携带，而非通用 `Ident`）
> - `True`/`False` 是关键字 token（而非 `BoolLit`）
> - 字面量带载荷：`Ident(String), IntLit(i64), FloatLit(f64), StringLit(String)`
> - 运算符：`Plus, Minus, Star, Slash, Percent, Eq, Ne, Lt, Gt, Le, Ge, Assign, Bang`（`!`）、`And, Or, Not`（`not`）
> - 分隔符：`LParen, RParen, LBrace, RBrace, LBracket, RBracket, Comma, Colon, Dot, Arrow, Semi`
> - 特殊：`Eof, Error`
> - 0.0.1 **不**实现的关键字（`Return, Unsafe, Trusted, Box, Ref, Move, Clone, BoolLit`）不在 TokenKind 中。
> - `Semi` 即 TICKETS 早期草案中的 `Semicolon`；0.0.1 必填，ASI 在后续版本实现。
```rust
// 关键字
Func, Const, If, Elif, Else, While, Choose,
When, Otherwise, Return, Import, Unsafe, Trusted,
Box, Ref, Move, Clone, True, False,

// 标识符/字面量
Ident, IntLit, FloatLit, BoolLit, StringLit,

// 运算符/分隔符
Plus, Minus, Star, Slash, Percent,
Eq, Ne, Lt, Gt, Le, Ge,
Assign,
And, Or, Not,
LParen, RParen, LBrace, RBrace, LBracket, RBracket,
Comma, Colon, Dot, Arrow, // -> for func type
Semicolon, // 0.0.1 必填，ASI 在后续版本实现

// 特殊
Eof, Error,
```

### 实现要求
- **预分配**：`Vec::with_capacity(source.len() / 4)`
- **Span**：`struct Span { start: u32, end: u32 }`（字节偏移，行列可后算）
- **错误**：`LexError { kind: LexErrorKind, span }`，`kind ∈ { InvalidChar(char), UnterminatedString, UnterminatedComment, InvalidEscape(char), InvalidNumber(String), IntOverflow, FloatOverflow }`；消息由 `Display` 生成
- **跳过注释**：`// ...` 和 `/* ... */`（不嵌套）

### 测试
- `tests/lexer/valid/*.fln`：每个 token 类型 ≥1 用例
- `tests/lexer/invalid/*.fln`：错误位置准确、消息可读

### 验收
```
cargo test -p fleen-compiler lexer::tests
cargo fmt -- --check && cargo clippy -- -D warnings
```

---

## T02: B2 Parser — 语法分析

### 文件创建
```
fleen-compiler/src/parser/
├── mod.rs          # pub fn parse(tokens: Vec<Token>) -> Result<Ast, ParseError>
├── ast.rs          # AST 节点定义
├── expr.rs         # 表达式解析（优先级 climbing）
├── stmt.rs         # 语句/声明解析
├── error.rs        # ParseError { span, message, expected, found }
└── tests.rs
```

### AST 节点（对应 `SYNTAX.ebnf`，命名按 `SPEC.md §3`：`ExprIf`、`StmtLet` 等）
```rust
// 程序
pub struct Ast { pub items: Vec<Item>, pub span: Span }

pub enum Item {
    Import(ImportDecl),
    Decl(Decl),
    Expr(Expr), // 表达式语句
}

// 声明
pub enum Decl {
    Func(FuncDecl),
    VarBinding(VarBinding),
    ConstDecl(ConstDecl),
}

pub struct FuncDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret_type: Option<Type>,
    pub body: FuncBody,
    pub span: Span,
}

pub enum FuncBody {
    SingleExpr(Box<Expr>),  // = expr
    Block(Block),           // { ... }
}

pub struct Param { pub name: Ident, pub ty: Type, pub span: Span }

pub struct VarBinding {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Box<Expr>,
    pub span: Span,
}

pub struct ConstDecl {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Box<Expr>,
    pub span: Span,
}

// 类型
pub enum Type {
    Base(BaseType),           // int, float, bool, string, unit
    Array(Box<Type>),         // [T]
    Box(Box<Type>),           // box<T> (0.0.1 解析但不生成指令)
    Ref(Box<Type>),           // ref T (0.0.1 解析但不生成指令)
    Result(Box<Type>, Box<Type>), // Result<T, E>
    Func(Vec<Type>, Box<Type>),   // (T...) -> T
}

// 表达式（按优先级从低到高）
pub enum Expr {
    Assign(Box<Expr>, Box<Expr>),  // lhs = rhs
    If(ExprIf),
    While(ExprWhile),
    Choose(ExprChoose),
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Mod(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),     // func(args...)
    Index(Box<Expr>, Box<Expr>),    // arr[idx]
    Field(Box<Expr>, Ident),        // obj.field
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Ident(Ident),
    Unit,
    Block(Block),
}

pub struct ExprIf {
    pub condition: Box<Expr>,
    pub then_branch: Block,
    pub elif_branches: Vec<(Box<Expr>, Block)>,
    pub else_branch: Option<Block>,
    pub span: Span,
}

pub struct ExprWhile {
    pub condition: Box<Expr>,
    pub body: Block,
    pub span: Span,
}

pub struct ExprChoose {
    pub scrutinee: Box<Expr>,
    pub arms: Vec<ChooseArm>,
    pub span: Span,
}

pub struct ChooseArm {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Block,
}

pub enum Pattern {
    Literal(Expr), // int/float/bool/string
    Ident(Ident),  // 绑定变量
}

pub struct Block { pub stmts: Vec<Stmt>, pub span: Span }

pub enum Stmt {
    Decl(Decl),
    Expr(Expr),
}
```

### 实现要求
- **递归下降 + 优先级 climbing**（`expr.rs`）
- **错误恢复**：遇到错误继续解析，收集多个错误（可选，MVP 只报第一个）
- **Span 传递**：每个节点带 `span`

### 测试
- `tests/parser/valid/*.fln`：每条文法规则 ≥1 用例，AST 结构断言
- `tests/parser/invalid/*.fln`：错误带正确 Span

### 验收
```
cargo test -p fleen-compiler parser::tests
cargo fmt -- --check && cargo clippy -- -D warnings
```

---

## T03: B3 Resolver — 名字解析 / 作用域

### 文件创建
```
fleen-compiler/src/resolver/
├── mod.rs          # pub fn resolve(ast: Ast) -> Result<Hir, ResolveError>；import 解析内联
├── scope.rs        # 作用域栈、绑定表
├── hir.rs          # HIR 节点（带 BindingId）
└── error.rs        # 错误类型

fleen-compiler/tests/
└── resolver_integration.rs  # 集成测试（内联 + 目录测试）
```

### 核心类型
```rust
// ID newtypes（按 SPEC.md §5）
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct BindingId(u32);

// 绑定信息
pub struct Binding {
    pub id: BindingId,
    pub kind: BindingKind,  // Variable | Parameter | Function | Builtin
    pub mutable: bool,      // const = false
    pub span: Span,         // 声明处的 span
    pub hir_id: HirId,
}

// 作用域栈
pub struct ScopeStack {
    scopes: Vec<Scope>,
    function_stack: Vec<usize>,  // 函数边界位置（嵌套保存/恢复）
}
```

ScopeKind 有四种：`Global` / `Function` / `Loop` / `Block`。`Loop` 是 `while` 体的作用域，§4.5 的赋值规则在其中生效。

### HIR 节点（AST → HIR：Ident 变为已解析的 Ident）
```rust
pub enum ExprHir {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Str(String, Span),
    Ident { name: String, binding_id: BindingId, hir_id: HirId, span: Span },  // ← 关键变化
    // ... 其他同 AST 但子节点是 ExprHir
}

// 函数声明带 BindingId
pub struct FuncDeclHir {
    pub name: String,
    pub params: Vec<ParamHir>,
    pub ret_type: Option<Type>,
    pub body: FuncBodyHir,
    pub hir_id: HirId,
    pub span: Span,
}
```

### 实现要求
- **名字解析**：把 `Ident` → `Ident { name, binding_id, hir_id, span }`（HIR 中每个标识带**使用处** span）
- **`=` 三义性判定**：绑定 / 赋值 / 遮蔽，**只有一条判定路径**（§3.6）；语句位置与表达式位置共享同一套目标查找
- **循环体赋值**（§4.5）：`while` 体（含嵌套块）中对外层可变用户绑定的 `x = e` 是**赋值**，不是遮蔽
- **遮蔽可变性一致性**（§4.3，原 T04 的 B3.5 范围已并入 B3）：同名遮蔽变量时内层/外层可变性必须相同
- **const 语义检查**（原 T04 范围）：const 重新绑定、const 被赋值、赋值带类型标注均为错误
- **同一作用域重复声明**：`func`、参数、`const`、变量重复 → `DuplicateBinding { name, first_span }`
- **函数边界**：遮蔽检查与循环赋值目标查找均不穿越函数边界（"函数不生效"）
- **作用域栈健壮**：错误路径（尾 expr 失败、单 expr 函数体失败）后作用域必须正确退出，不得泄漏
- **导入**：记录 `import std.io` → `ImportDecl { path: Vec<String> }`，不加载模块（0.0.1 无模块系统）
- **错误必须带真实 Span**：不得使用 `Span::new(0, 0)` 占位（SPEC §4/§11）。唯一例外：错误恢复路径的占位符节点（`StmtHir::Error`、`PatternHir::Error`）使用 `HirId(0)` 和 `Span::new(0, 0)`，因为错误已记录，HIR 整体被丢弃

### 错误
- `ResolveErrorKind::UndeclaredVariable { name }`
- `ResolveErrorKind::DuplicateBinding { name, first_span }`
- `ResolveErrorKind::AssignToImmutable { name }`
- `ResolveErrorKind::TypeAnnotationOnAssignment`
- `ResolveErrorKind::ShadowingMutabilityMismatch { outer_mutable, inner_mutable }`
- `ResolveErrorKind::InvalidAssignmentTarget`

### 测试
- 作用域嵌套、遮蔽（含可变性校验）、函数边界屏障、循环体赋值、错误路径作用域泄漏、Span 正确性

### 验收
```
cargo test -p fleen-compiler resolver
cargo fmt -- --check && cargo clippy --all-targets -- -D warnings
```

---

## T04: B3.5 BindCheck — 遮蔽/Const 语义检查（已并入 T03）

> **状态：已合并进 T03，不再独立实现。**
>
> 遮蔽可变性一致性（DESIGN.md §4.3）与 const 赋值/重复声明检查（§3.5）
> 已在 `resolver/mod.rs` 中一并完成（统一的 `=` 三义性判定路径），
> 不再单列 `binder/` crate。理由：这些检查的措辞（`ShadowingMutabilityMismatch`、
> `AssignToImmutable`、`DuplicateBinding`）直接依赖 resolver 判定的是"绑定还是赋值"，
> 拆到独立阶段会让同一份 scope 状态被遍历两次且容易漂移。错误信息格式如下：

```
error: shadowing must preserve mutability
  --> file.fln:3:5
   |
 2 | x = 42
   | ^ outer `x` is mutable
 3 |     const x = 43
   |     ^^^^^ inner `const` does not match outer mutability
```

### 验收
```
cargo test -p fleen-compiler resolver
```

---

## T05: B4 Typeck — 类型推导 & 检查

### 文件创建
```
fleen-compiler/src/typeck/
├── mod.rs          # pub fn typeck(hir: Hir) -> Result<TypedHir, Vec<TypeckError>>
├── infer.rs        # 类型推导（单向、Hindley-Milner 子集）+ choose 穷尽检查
├── error.rs        # TypeckError / TypeckErrorKind
├── typed_hir.rs    # TypedHIR（每节点带 Type）
├── unify.rs        # 类型统一
└── tests.rs
```

### 类型系统（0.0.1）
```rust
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Unit,
    Func(Vec<Type>, Box<Type>),   // (params...) -> ret
    Result(Box<Type>, Box<Type>), // Result<T, E>
    Array(Box<Type>),           // 解析支持，类型检查保留，codegen 报 UnsupportedType
    Box(Box<Type>),             // 同上
    Ref(Box<Type>),             // 同上
    Unsupported(String),        // 不支持特性的占位
}
```

### 推导规则
- **字面量**：已知类型
- **标识符**：查绑定的类型（Resolver 已绑定）
- **二元运算**：两侧类型必须一致，结果类型同侧
- **比较**：两侧类型一致，结果 `Bool`
- **逻辑**：两侧 `Bool`，结果 `Bool`
- **一元 `-`**：`Int`/`Float` → 同类型
- **一元 `!`**：`Bool` → `Bool`
- **函数调用**：实参类型匹配形参，返回函数返回类型
- **If**：条件 `Bool`，then/elif/else 类型必须统一（或可隐式转换，0.0.1 要求完全一致）
- **While**：条件 `Bool`，体类型忽略（值为 `Unit`）
- **Choose**：scrutinee 类型 `T`，每个 when pattern 类型兼容 `T`，所有 arm 体类型统一
- **Result**：`Ok(v)` → `Result<T, E>` where `v: T`，`Err(e)` → `Result<T, E>` where `e: E`（0.0.1 无 `Ok`/`Err` 构造语法，此规则推迟到后续版本）

### Choose 穷尽性检查（关键）
- **模式类型**：
  - 字面量模式 → 该字面量类型（如 `0` → `Int`）
  - 标识符模式 → 绑定变量，类型为 scrutinee 类型（绑定模式）
- **穷尽判定**：
  - 有 `otherwise` → 穷尽
  - 无 `otherwise`：仅当 scrutinee 类型为 `Bool` 且覆盖 `true`/`false` 视为穷尽；0.0.1 无用户定义枚举，故只有 `Bool` 情况
  - 0.0.1 暂无 `Ok`/`Err` pattern 语法，因此 `Result<T, E>` scrutinee 也必须写 `otherwise`（`Ok`/`Err` pattern 与穷尽识别计划在后续版本实现）
  - 带 guard 的 arm 不计入穷尽判定（守卫可能不成立）

### 错误
- 错误类型：`TypeckError { kind: TypeckErrorKind, span: Span }`，收集为 `Vec`
- 主要 kind：`TypeMismatch { expected, found }`、`AssignTypeMismatch { expected, found }`、`ChooseNotExhaustive { scrutinee_type, missing_patterns }`、`ArgTypeMismatch { index, expected, found }`、`ArityMismatch { expected, found }`、`ConditionNotBool { found }`、`InvalidOperand { op, ty }`、`NotCallable { ty }`、`UnsupportedType { ty }`、`UnsupportedFeature { feature }`、`UndefinedVariable { name }`、`InternalError { message }`（后两种为不变量兜底：resolver/typeck 不一致时报错而非静默）

### 验收
```
cargo test -p fleen-compiler typeck::tests
```

---

## T06: B5 Lower — 控制流展平 → MIR

### 文件创建
```
fleen-compiler/src/lower/
├── mod.rs          # pub fn lower(typed_hir: TypedHir) -> Result<Mir, LowerError>
├── mir.rs          # MIR 指令、基本块、函数体
├── cfg.rs          # 控制流图构建
├── slots.rs        # 局部变量栈槽分配（无 SSA）
└── tests.rs
```

### MIR 设计（栈式、按 `BYTECODE.md §6` 形态）
```rust
pub struct Mir {
    pub funcs: Vec<MirFunc>,
    pub globals: Vec<MirGlobal>,
}

pub struct MirFunc {
    pub func_id: FuncId,
    pub name: String,
    pub params: u16,
    pub locals: u16,           // 总槽数（含参数 slot 0..params-1）
    pub entry: BlockId,
    pub blocks: Vec<MirBlock>,
    pub is_builtin: bool,      // print 等内置函数（VM 宿主侧执行）
    pub ret_type: Type,
}

pub struct MirGlobal {
    pub global_id: GlobalId,
    pub name: String,
    pub mutable: bool,
    pub ty: Type,
    pub init: Vec<MirInstr>,   // 留一个初始值在栈顶
}

pub struct MirBlock {
    pub id: BlockId,
    pub instrs: Vec<MirInstr>,
    pub terminator: Terminator,
}

pub enum Terminator {
    Return,                    // 返回
    Jump(BlockId),             // 无条件跳转
    JumpIfFalse(BlockId),      // 栈顶 bool，false 跳转
    JumpIfTrue(BlockId),       // 栈顶 bool，true 跳转
    // 0.0.1 无 Switch/Call terminator（Call 是普通指令）
}

pub enum MirInstr {
    // 常量
    ConstInt(i64),
    ConstFloat(f64),
    ConstStr(String),
    True,
    False,
    Unit,
    // 栈操作
    Pop,
    Dup,
    // 局部变量（栈槽）
    LoadLocal(u16),   // slot
    StoreLocal(u16),
    // 全局变量
    LoadGlobal(u16),  // GlobalId
    StoreGlobal(u16),
    // 算术 Int
    IAdd, ISub, IMul, IDiv, IMod,
    // 算术 Float
    FAdd, FSub, FMul, FDiv,
    // 比较/逻辑
    Eq, Ne, Lt, Gt, Le, Ge,
    Not,
    NegI, NegF,
    // 函数调用
    Call(FuncId),         // 直接调用
    LoadFunc(FuncId),     // 函数值压栈
    CallValue(u8),        // 间接调用，argc
    // choose 辅助
    BindMatch(u16),       // 守卫变量绑定到 slot
}
```

### Lower 规则（对应 `BYTECODE.md §6`）
- **if/elif/else** → 条件 `JumpIfFalse` → 下一分支；分支末 `Jump` 到合流块
- **while** → 条件块 `JumpIfFalse` 退出，体末 `Jump` 回条件块
- **choose** → 比较链：每个 `when` 生成比较 + `JumpIfFalse`；守卫 `if` 紧跟比较后
- **绑定/赋值** → `StoreLocal(slot)`，**遮蔽在编译期处理**：新绑定分配新 slot
- **函数** → 参数占 `slot 0..params-1`，局部变量从 `params` 开始分配
- **表达式语句** → 计算值 → `Pop`

### 栈槽分配（无 SSA）
- 遍历函数体，收集所有绑定（含遮蔽），每个绑定分配唯一 slot
- 同一作用域内同名遮蔽 → 新 slot，旧 slot 不再使用
- 参数固定在 `0..params-1`

### 验收
```
cargo test -p fleen-compiler lower::tests
# 对比生成的 MIR 与 BYTECODE.md §6 示例形态一致
```

---

## T07: B6 Codegen — MIR → 字节码

### 文件创建
```
fleen-compiler/src/codegen/
├── mod.rs          # pub fn codegen(mir: Mir) -> Bytecode
├── bytecode.rs     # Bytecode 结构、指令编码
├── encoder.rs      # MIR 指令 → 字节流、常量池去重、跳转偏移解析
└── tests.rs
```

### Bytecode 结构（按 `BYTECODE.md §2`）
```rust
pub struct Module {
    pub version: u16,           // = 1
    pub constants: Vec<Const>,  // 常量池（去重）
    pub functions: Vec<Func>,
    pub globals: Vec<Global>,
    pub entry: FuncId,          // main，或有全局时合成的 __init__
}

pub enum Const {
    Int(i64),
    Float(f64),
    Str(Box<str>),
}

pub struct Func {
    pub name: ConstId,
    pub params: u16,
    pub locals: u16,
    pub code: Box<[u8]>,
    pub span_map: Box<[SpanEntry]>, // 0.0.1 留空 Box::new([])
    pub is_builtin: bool,           // 宿主内置（print），VM 直接执行、忽略 code
}

pub struct Global {
    pub name: ConstId,
    pub mutable: bool,
}
```

### 指令编码（按 `BYTECODE.md §4-5`）
- `opcode: u8` + 操作数（小端）
- 跳转操作数 = **目标指令绝对字节偏移**（相对 `code` 起始）
- 两遍编码：第一遍生成指令+占位标签，第二遍解析标签为偏移

### 实现要点
- **常量池去重**：`HashMap<Const, ConstId>`（需 `Const` 实现 `Hash/Eq`）
- **span_map**：`Box::new([])`（0.0.1 不生成）
- **入口点**：查找名为 `main` 的函数，若无则报错
- **全局初始化**：有全局变量时合成 `__init__`（`params=0`、`locals=0`、`is_builtin=false`），按声明顺序发出各 `MirGlobal::init` + `StoreGlobal`，末尾 `Call main` + `Return`，并令 `entry = __init__`；无全局时 `entry = main`
- **内置函数**：把 `MirFunc::is_builtin` 原样写入 `Func::is_builtin`（`print` 等由 VM 宿主执行）

### 验收
```
cargo test -p fleen-compiler codegen::tests
# 输出字节码通过 fleen-verify (T08)
```

---

## T08: B7 Verify — 静态字节码校验

### 文件创建
```
fleen-verify/src/
├── main.rs         # CLI：读 .flnc → verify → 退出码
├── verify.rs       # 7 条校验规则
├── stack_analysis.rs # 抽象栈深度精确分析
└── tests.rs
```

### 7 条校验规则（`BYTECODE.md §8`）
1. 所有 `Const`/`LoadFunc`/`Call` 索引在范围内
2. 所有跳转目标落在**指令边界**上
3. 每个函数每条路径以 `Return` 结束（无掉出函数底）
4. **操作数栈深度不下溢**：精确抽象栈深度分析
   - 每条指令静态 `Δdepth` 已知
   - 从入口（深度 0）按 CFG 传播
   - 合并分支时：**所有前驱深度必须一致**，不一致即失败
   - 每指令执行前深度 ≥ 指令所需最小深度
5. `StoreGlobal` 目标全部 `mutable: true`（例外：入口函数 `__init__` 中对全局的初始化写）
6. 常量池无重复项
7. 函数 `locals` ≥ `params`，且所有 `LoadLocal/StoreLocal` slot < `locals`

### 栈深度分析算法
```rust
// 工作列表算法
fn analyze(func: &Func) -> Result<Vec<u32>, VerifyError> {
    let mut depth_at = vec![u32::MAX; func.code.len() + 1]; // 每字节偏移的深度
    depth_at[0] = 0;
    let mut worklist = vec![0];
    
    while let Some(pc) = worklist.pop() {
        let current_depth = depth_at[pc];
        let instr = decode_at(&func.code, pc);
        let (next_pc, delta) = instr.stack_effect();
        
        // 检查前置深度
        if current_depth < instr.min_stack_depth() {
            return Err(StackUnderflow { pc });
        }
        
        let next_depth = current_depth + delta;
        if depth_at[next_pc] == u32::MAX {
            depth_at[next_pc] = next_depth;
            worklist.push(next_pc);
        } else if depth_at[next_pc] != next_depth {
            return Err(StackDepthMismatch { pc: next_pc, 
                expected: depth_at[next_pc], actual: next_depth });
        }
        
        // 跳转目标也要传播
        for target in instr.jump_targets() {
            if depth_at[target] == u32::MAX {
                depth_at[target] = next_depth;
                worklist.push(target);
            } else if depth_at[target] != next_depth {
                return Err(StackDepthMismatch { ... });
            }
        }
    }
    Ok(depth_at)
}
```

### 验收
```
cargo test -p fleen-verify
# 对 T07 产出的所有字节码 PASS
```

---

## T09: B8 VM — 字节码执行

### 文件创建
```
fleen-vm/src/
├── main.rs         # CLI：读字节码 → Verify → 执行 → 打印结果
├── vm.rs           # 核心解释循环
├── value.rs        # Value 枚举、运算实现
├── frame.rs        # CallFrame、栈操作
├── error.rs        # RuntimeError
└── tests.rs
```

### 值表示（`BYTECODE.md §3.2`）
```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<str>),   // 0.0.1 不可变共享
    Unit,
    Func(FuncId),   // 一等值
}
```

### 执行模型
- **操作数栈**：`Vec<Value>`
- **调用栈**：`Vec<CallFrame>`
- **全局变量表**：`Vec<Value>`（按 `GlobalId` 索引）
- **指令分派**：大 `match opcode` 循环

### 调用约定（`BYTECODE.md §3.3`）
- **直接调用**：压实参 → `Call FuncId`
- **间接调用**：压 `Func(FuncId)` → 压实参 → `CallValue argc`
- **返回**：弹返回值 → 弹调用帧 → 压返回值到调用方栈

### 错误处理
- 所有 `RuntimeError` 直接退出，打印错误信息，**不可捕获**

### 入口
```rust
fn main() {
    let bytecode = read_bytecode(args[1]);
    fleen_verify::verify(&bytecode).expect("verification failed");
    let result = Vm::new(bytecode).run();
    println!("{:?}", result);
}
```

### 验收
```
cargo test -p fleen-vm
# 运行 tests/e2e/fib.fln 编译后的字节码，输出匹配 fib.expected
```

---

## T10: E2E 集成测试 — Fibonacci 跑通

### 测试文件
```
tests/e2e/
├── fib.fln         # 完整 Fibonacci 程序
└── fib.expected    # 期望输出（每行一个数字）
```

### fib.fln
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

### fib.expected
```
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

### 运行脚本
```bash
# 编译
cargo run -p fleen-compiler -- tests/e2e/fib.fln -o /tmp/fib.flnc

# 验证
cargo run -p fleen-verify -- /tmp/fib.flnc

# 执行
cargo run -p fleen-vm -- /tmp/fib.flnc > /tmp/fib.out

# 对比
diff /tmp/fib.out tests/e2e/fib.expected
```

### CI 集成
- `cargo test` 包含 E2E 测试（或单独 `cargo test --test e2e`）

---

## 依赖关系与并行化

```
T01 (Lexer)
    ↓
T02 (Parser)
    ↓
T03 (Resolver) ←─────────────┐
    ↓                         │
T04 (BindCheck)              │ 可并行：T03/T04 同人可串行，不同人可并行
    ↓                         │
T05 (Typeck)                 │
    ↓                         │
T06 (Lower)                  │
    ↓                         │
T07 (Codegen) ←──────────────┼── T08 (Verify) 同步开发，Codegen 产出即 Verify
    ↓                         │
T09 (VM) ────────────────────┘
    ↓
T10 (E2E)
```

---

## 质量闸（每票必须过）

```bash
# 格式
cargo fmt -- --check

# Lint
cargo clippy -- -D warnings

# 单元测试
cargo test -p <crate> <module>::tests

# 文档
cargo doc --no-deps
```

---

## 里程碑

| 里程碑 | 包含票号 | 标志 |
|--------|----------|------|
| M1: 前端完备 | T01–T04 | `Ast` → `CheckedHir` 无误 |
| M2: 类型系统完备 | T05 | `TypedHir` 所有类型已知 |
| M3: 后端完备 | T06–T08 | 字节码通过 Verify |
| M4: 端到端跑通 | T09–T10 | `fib.fln` 正确输出 |

---

## 备注

- **0.0.1 不实现**：`box<T>`/`ref` 指令、`?` 语法糖、`struct`、`for`、`async`、泛型、FFI、`unsafe`/`trusted`
- **0.0.1 解析但不生成指令**：`box<T>`、`ref T`、`[T]` 类型（Parser/Resolver/Typeck 识别，Lower/Codegen 遇到报 `Unimplemented`）
- **print 函数**：内置在 VM 预导入模块，`std.io.print` 调用映射到宿主 `println!`