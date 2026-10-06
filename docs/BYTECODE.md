# Fleen 字节码规范 v0.0.2

> **快、简、安、直觉**
> 本文档定义 Fleen 编译器（`codegen` 阶段）的输出格式，
> 以及 Fleen VM（`fleen-vm`）的执行模型。
> 编译器与 VM 必须共同遵守本文档。
>
> **v0.0.2 增量**：所有权指令（0x80–0x93）、Result 指令（0xA0–0xA4）、
> `Value` 所有权表示、`span_map` 生成。设计决议见 `docs/0.0.2/PLAN.md`；
> 实现落地前，当前工具链仍产出/执行 v1，读取端须同时接受 `{1, 2}`。

---

## 1. 总览

Fleen 编译到**自定义栈式字节码**，由自研 VM 解释执行。

- **执行模型**：栈式虚拟机（stack machine），操作数隐式位于栈顶
- **编码**：字节流，小端序（little-endian）
- **指令格式**：`opcode: u8` + 定长/变长操作数
- **结构**：一个字节码模块（`Module`）= 常量池 + 函数表 + 全局变量表 + 入口点
- **无 GC、无异常表、无元循环指令**：与语言设计一致（所有权由 v2 的 `AllocBox` /
  `MoveLocal` / `CloneLocal` 等指令承担，§5.9–5.10；释放由值的确定性析构完成）

### 阶段衔接

```
Lower (Typed HIR → MIR)    MIR 是控制流展平后的线性指令序列
Codegen (MIR → Bytecode)   MIR 指令 1:1 映射为字节码 + 常量池索引
Execute (Bytecode → 结果)  VM 解释执行
```

规则：
- Codegen 是**纯函数**：`pub fn codegen(mir: Mir) -> Result<Bytecode, CodegenError>`
- MIR 与字节码指令**基本一一对应**，不做窥孔优化（后版本可加，但格式不变）
- VM 不做类型检查——类型安全由 `typeck` 阶段保证，VM 信任字节码

---

## 2. 模块格式（Module）

```rust
pub struct Module {
    pub version: u16,          // 字节码版本，当前 1
    pub constants: Vec<Const>, // 常量池
    pub functions: Vec<Func>,  // 函数表
    pub globals: Vec<Global>,  // 全局变量表
    pub entry: FuncId,         // 入口函数：通常为 main；有全局变量时为合成的 __init__
}
```

规则：
- **常量池去重**：相同常量只存一份，用索引 `ConstId(u32)` 引用
- **函数从 0 开始连续编号**，用 `FuncId(u32)` 引用
- **全局变量从 0 开始连续编号**，用 `GlobalId(u32)` 引用
- ID 均为 newtype，避免混淆（见 SPEC.md §5）
- **入口点由 `entry` 指定**，不靠函数名约定；模块**存在全局变量**时 `entry` 指向合成的 `__init__`（见下）

### 入口与全局初始化

全局变量没有独立的“初始化段”，其初始化代码由一个**合成函数 `__init__`** 承载：

- **无全局变量**：`entry` 直接指向 `main`。
- **有全局变量**：`codegen` 额外合成一个函数 `__init__`（`params = 0`、`locals = 0`、`is_builtin = false`），放入函数表（`FuncId` 排在所有函数之后）。
  - `__init__` 按**声明顺序**执行每个全局的初始化代码（MIR 中 `MirGlobal::init`），初始化表达式在栈顶留下一个值，紧接一条 `StoreGlobal gid` 存入对应全局。
  - 全部存完后执行 `Call main_id`，再 `Return`（把 `main` 的返回值原样传出）。
  - `entry` 指向 `__init__`。
- **`main` 仍按名字查找**：函数表中没有名为 `main` 的函数时，`codegen` 报错。
- VM 的执行契约不变：**先执行 `entry`**。`entry` 为 `__init__` 时，由它在初始化完成后调用 `main`。

> `StoreGlobal` 对 `mutable: false` 全局的写入只允许发生在 `__init__`（入口函数）中，作为初始化；其余函数对 `const` 全局写入是 verify 失败（见 §8）。

### 常量池（Constant Pool）

```rust
pub enum Const {
    Int(i64),
    Float(f64),
    Bool(bool),      // bool 可不入池（用指令编码），预留
    Str(Box<str>),   // UTF-8 字符串
}
```

规则：
- `int` / `float` / `string` 字面量入池；小整数优化见 §7
- `bool` 用 `True` / `False` 指令直接编码，不入池
- `unit` 无值，用 `Unit` 指令编码

### 函数表（Function Table）

```rust
pub struct Func {
    pub name: ConstId,        // 指向常量池中的字符串
    pub params: u16,          // 参数个数
    pub locals: u16,          // 局部变量槽总数（含参数）
    pub code: Box<[u8]>,      // 指令字节流
    pub span_map: Box<[SpanEntry]>, // 指令偏移 → 源码位置（用于运行时诊断）
    pub is_builtin: bool,     // 宿主内置函数（如 print），由 VM 直接执行，不跑 code
}
```

> **SpanEntry（0.0.2 定义并启用）**：
>
> ```rust
> pub struct SpanEntry {
>     pub offset: u32,  // 指令起始字节偏移（相对函数 code）
>     pub start: u32,   // 源码起始字节偏移
>     pub end: u32,     // 源码结束字节偏移
> }
> ```
>
> 0.0.1 决定暂不生成（留空 `Box::new([])`）。0.0.2 起 codegen 逐指令填充
> `span_map`（数据来源：MIR 指令携带的 Span），供运行时错误诊断使用；
> 读取端对空表与非空表都必须接受。

规则：
- **内置函数用 `is_builtin: true` 标记**，不靠函数名判断：语言允许用户遮蔽内建名（如自定义 `func print`），仅凭名字无法区分宿主内置与用户函数。
- `is_builtin` 为 `true` 时，VM 直接执行宿主实现并忽略 `code`；`code` 仍保留占位体（`Unit; Return`）以维持结构良构，并通过 verify。
- `codegen` 将 MIR 的 `MirFunc::is_builtin` 原样搬运到此字段。

### 全局变量表（Global Table）

```rust
pub struct Global {
    pub name: ConstId,   // 指向常量池中的字符串
    pub mutable: bool,   // 是否可变（const 为 false）
}
```

规则：
- 全局变量的**不可变性在 VM 层强制执行**：对 `mutable: false` 的全局执行 `StoreGlobal` 是 VM 错误
- 全局变量初始化顺序 = 表中顺序；初始化代码位于合成的 `__init__` 函数中（见 §2「入口与全局初始化」）
- `__init__` 中初始化 `const` 全局的 `StoreGlobal` 是允许的；其余位置的 `StoreGlobal` 目标必须 `mutable: true`

---

## 3. 运行时模型（VM）

### 3.1 栈

- **操作数栈**（operand stack）：存放临时值，指令的输入输出
- **调用栈**（call stack）：每层调用一个 `CallFrame`：

```rust
pub struct CallFrame {
    pub func: FuncId,
    pub ip: usize,          // 指令指针，指向 code 中的字节偏移
    pub stack_base: usize,  // 本帧的局部变量在栈中的起始槽位
}
```

- 局部变量**不单独使用栈槽**，而是按 `stack_base + slot` 在操作数栈上定位
- `LoadLocal i` = 取 `stack_base + i`；`StoreLocal i` = 写 `stack_base + i`

### 3.2 值表示

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),                 // 0.0.2：独占所有权（0.0.1 临时用的 Rc<str> 已按计划替换）
    Boxed(Box<Value>),             // 0.0.2：box<T>，唯一所有权
    Ref { base: u32, slot: u16 },  // 0.0.2：借用句柄，仅存在于被调帧生命期
    Ok(Box<Value>),                // 0.0.2：Result 构造
    Err(Box<Value>),               // 0.0.2：Result 构造
    Unit,           // 空值，占位
    Func(FuncId),   // 一等值函数引用
}
```

规则：
- 值**自描述类型**，VM 据此做算术指令的快速分派
- 类型不匹配的算术指令（如 `int + string`）是 **VM 错误**（`RuntimeError`），但正常编译通过的字节码不应出现——那是 `typeck` 的职责
- **所有权纪律由 typeck 静态保证**；VM 侧 Rust 的移动/丢弃语义即运行时实现，
  `Drop` 确定性完成（帧销毁 truncate、槽位覆盖、`Pop`），无 GC
- `Eq` 语义：`Str` 按内容比较；`Boxed` 比较点内值（与 Rust `Box: PartialEq` 一致）；
  `Ref` 解引用后比较
- `Ref` 句柄不可逃逸：仅由 `MakeRefLocal` 在实参准备时生成，只读、不可存储、
  随调用结束消亡（越界读取 → `RuntimeError::BorrowOutOfRange`，防御性）
- `Value` 不落盘：常量池无 `Ref` / `Boxed` / `Ok` / `Err` 形态

> **0.0.2 决定**：0.0.1 的 `Str(Rc<str>)` 是临时决策——字符串不可变、无循环引用风险，
> `Rc` 的确定性析构不算 GC。引入 `move` / `clone` 后 `Rc` 的共享语义与所有权模型冲突
> （`clone` 是深拷贝，`Rc::clone` 是计数 +1），已按 `docs/0.0.2/PLAN.md` §5.3
> 替换为 `Str(Box<str>)`：`clone` 是深拷贝，计数共享不复存在。

### 3.3 调用约定

- **直接调用**：按顺序将 `params` 个实参压栈 → 执行 `Call FuncId`
- **间接调用（一等值）**：先压 `f: Func(FuncId)`，再按顺序压实参 → 执行 `CallValue argc`（`argc = params`）
- **被调方**：实参占据本帧 `slot 0..params`，多余的槽是局部变量；`f` 留在调用方栈上，由 `CallValue` 消耗
- **返回**：`Return` 弹出栈顶 1 个值作为返回值，弹出整个调用帧，返回值**压回调用方栈顶**
- `unit` 返回值的函数也必须显式 `Return`（压入 `Unit` 再返回）——保持约定统一
- **内置函数调用**：被调函数 `is_builtin == true` 时，VM 不建立普通帧执行 `code`，而由宿主按预导入映射执行（如 `print` → `println!`），再按普通调用约定把返回值压回调用方栈

### 3.4 VM 错误

```rust
pub enum RuntimeError {
    StackUnderflow,          // 操作数栈下溢
    InvalidOpcode(u8),       // 未知指令
    ConstOutOfRange(ConstId),
    FuncOutOfRange(FuncId),
    GlobalOutOfRange(GlobalId),
    ImmutableGlobal(GlobalId),   // 对 const 全局赋值
    ArithTypeMismatch,       // 类型不匹配的算术
    DivisionByZero,
    CallDepthExceeded,       // 递归深度上限
}
```

规则：
- VM 错误**不可捕获**（对应语言设计：panic 仅用于不可恢复错误）
- `CallDepthExceeded` 上限默认 `1024` 层，防止无限递归爆栈

---

## 4. 指令编码

### 4.1 编码规则

- `opcode`：1 字节（`u8`）
- 操作数：
  - `u8`：1 字节，小端
  - `u16`：2 字节，小端
  - `u32`：4 字节，小端
  - `i64` / `f64`：8 字节，小端
- 指令总长 = 1 + 各操作数字节数之和，**变长但可静态确定**（解码时按 opcode 查表得知长度）

### 4.2 编号分配

按功能分组预留编号空间，方便扩展（0.0.2+ 的 `box` / `move` 等指令插入组内空位）：

```
0x00–0x0F  常量与栈操作
0x10–0x1F  局部变量 / 全局变量
0x20–0x2F  算术（int）
0x30–0x3F  算术（float）
0x40–0x4F  比较 / 逻辑
0x50–0x5F  控制流
0x60–0x6F  函数调用
0x70–0x7F  选择（choose 辅助）
0x80–0x83  box / ref（0.0.2，§5.9）
0x84–0x8F  预留
0x90–0x93  move / clone（0.0.2，§5.10）
0x94–0x9F  预留
0xA0–0xA4  Result / 错误处理（0.0.2，§5.11）
0xA5–0xFF  预留
```

---

## 5. 指令集（v0.0.1 基础 + v0.0.2 增量）

### 5.1 常量与栈操作

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `Const` | `u32` (ConstId) | 5 | `→ v` | 从常量池加载 int/float/string |
| `True` | — | 1 | `→ true` | 压入 bool true |
| `False` | — | 1 | `→ false` | 压入 bool false |
| `Unit` | — | 1 | `→ unit` | 压入空值 |
| `Pop` | — | 1 | `v →` | 丢弃栈顶（丢弃语句值） |
| `Dup` | — | 1 | `v → v v` | 复制栈顶 |

### 5.2 变量

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `LoadLocal` | `u16` (slot) | 3 | `→ v` | 读局部变量 `stack_base + slot` |
| `StoreLocal` | `u16` (slot) | 3 | `v →` | 写局部变量 `stack_base + slot` |
| `LoadGlobal` | `u16` (GlobalId) | 3 | `→ v` | 读全局变量 |
| `StoreGlobal` | `u16` (GlobalId) | 3 | `v →` | 写全局变量（const 则 VM 错误） |

### 5.3 算术（int）

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `IAdd` | — | 1 | `a b → a+b` | int 加法，溢出为 VM 错误（见下） |
| `ISub` | — | 1 | `a b → a-b` | int 减法 |
| `IMul` | — | 1 | `a b → a*b` | int 乘法 |
| `IDiv` | — | 1 | `a b → a/b` | int 除法，除以 0 → `DivisionByZero` |
| `IMod` | — | 1 | `a b → a%b` | int 取模，符号随被除数（Rust `%` 语义） |

**整数溢出**：`IAdd` / `ISub` / `IMul` 使用 `checked_*`，溢出为 VM 错误。理由：显式、可预测——静默回绕违背「安」的哲学。（后续版本可加 `unchecked` 变体放 `unsafe` 块内。）

### 5.4 算术（float）

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `FAdd` | — | 1 | `a b → a+b` | IEEE 754 加法 |
| `FSub` | — | 1 | `a b → a-b` | |
| `FMul` | — | 1 | `a b → a*b` | |
| `FDiv` | — | 1 | `a b → a/b` | 除以 0 产生 `inf`/`NaN`（IEEE 语义，非错误） |

### 5.5 比较 / 逻辑

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `Eq` | — | 1 | `a b → bool` | 相等 |
| `Ne` | — | 1 | `a b → bool` | 不等 |
| `Lt` | — | 1 | `a b → bool` | 小于 |
| `Gt` | — | 1 | `a b → bool` | 大于 |
| `Le` | — | 1 | `a b → bool` | 小于等于 |
| `Ge` | — | 1 | `a b → bool` | 大于等于 |
| `Not` | — | 1 | `b → !b` | 逻辑非（一元 `!`） |
| `NegI` | — | 1 | `a → -a` | int 取负（一元 `-`） |
| `NegF` | — | 1 | `a → -a` | float 取负 |

规则：
- **没有 `And` / `Or` 指令**：`and` / `or` 必须短路求值，由控制流指令实现（见 §5.6 与 §6.3）

### 5.6 控制流

跳转操作数 `u16` 是**目标指令的绝对字节偏移**（相对 `code` 起始）。

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `Jump` | `u16` | 3 | `→` | 无条件跳转 |
| `JumpIfFalse` | `u16` | 3 | `b →` | 栈顶为 false 则跳转（并弹出） |
| `JumpIfTrue` | `u16` | 3 | `b →` | 栈顶为 true 则跳转（并弹出） |

规则：
- 跳转目标必须**落在指令边界上**（不对齐 → VM 错误），`fleen-verify` 负责静态校验
- `JumpIfTrue` 用于 `or` 短路，`JumpIfFalse` 用于 `if` 条件与 `and` 短路
- 无 `for` / `break` / `continue` 指令（0.0.1 无此语法）

### 5.7 函数调用

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `Call` | `u16` (FuncId) | 3 | `args... → ret` | 调用函数（见 §3.3） |
| `LoadFunc` | `u16` (FuncId) | 3 | `→ Func(id)` | 函数作为一等值压栈 |
| `CallValue` | `u8` (argc) | 2 | `f args... → ret` | 间接调用栈上的函数值 |
| `Return` | — | 1 | `ret →` | 返回（见 §3.3） |

规则：
- `Call` 用于**直接调用**（编译期已知目标，typeck 解析后 MIR 保证）
- `CallValue` 用于**一等值调用**（`f(a, b)` 其中 `f` 是变量），`argc` 为实参个数
- **栈上顺序**：`f` 必须在实参之前压栈，即 `f arg0 arg1 ...`（`f` 在栈底方向，实参在栈顶方向）
- 0.0.1 `argc` 用 `u8`（最多 255 参数）；语言层后续若需要更多，加 `CallValueWide`，不改现有编码

### 5.8 choose 辅助

`choose` 的模式匹配在 MIR 层被展平为**比较链 + 跳转**（见 §6.4），仅需要一个辅助指令绑定守卫变量：

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `BindMatch` | `u16` (slot) | 3 | `v → v` | 复制栈顶绑定到守卫槽（`when x if ...` 的 `x`） |

规则：
- `BindMatch` 不弹出栈顶：被匹配值还要参与后续比较或作为分支值来源

### 5.9 box / ref（v0.0.2）

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `AllocBox` | — | 1 | `v → b` | 堆分配，值的所有权转入 box |
| `DerefBox` | — | 1 | `b → b v` | 读点内值（副本/深拷贝），box 仍在栈上 |
| `StoreDerefBox` | — | 1 | `b v →` | 写点内值，旧值释放 |
| `MakeRefLocal` | `u16` (slot) | 3 | `→ ref` | 生成借用句柄指向本帧 `stack_base + slot`（实参准备） |

规则：
- `box` 唯一所有权：**没有**"移出 box"指令——读取产生副本，需要值由语言层
  `clone deref b`（`DerefBox`）承担
- `MakeRefLocal` 的 slot 校验同 `LoadLocal`/`StoreLocal`（slot < `locals`）；
  句柄值 `Ref { base, slot }` 中的 `base` 由 VM 在调用时绑定调用方帧，
  不是编码的一部分

### 5.10 move / clone（v0.0.2）

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `DupDeep` | — | 1 | `v → v v` | 深拷贝栈顶（`clone` 的栈上形态） |
| `MoveLocal` | `u16` (slot) | 3 | `→ v` | **消耗性读**：取出槽值，槽清为 `Unit` |
| `CloneLocal` | `u16` (slot) | 3 | `→ v` | 非消耗读：深拷贝槽值，槽不动 |
| `CloneGlobal` | `u16` (GlobalId) | 3 | `→ v` | 非消耗读全局（owned 全局的唯一读法） |

规则：
- 0.0.1 的 `LoadLocal` / `LoadGlobal`（非消耗读、浅复制）**保持不变**，
  仅用于 Copy 类型；owned 类型的消耗性读用 `MoveLocal`、非消耗读用
  `CloneLocal`/`CloneGlobal`，由 codegen 按静态类型选择
- "已移动"槽位的内容对 VM 不可见（typeck 保证不再读取）；`MoveLocal` 用
  `mem::replace` 清槽，避免旧值延迟到帧销毁才释放

### 5.11 Result / 错误处理（v0.0.2）

| 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|------|--------|------|
| `PackOk` | — | 1 | `v → ok(v)` | 构造 `Ok` |
| `PackErr` | — | 1 | `v → err(e)` | 构造 `Err` |
| `IsErr` | — | 1 | `r → bool` | 测试 `Err` |
| `UnwrapOk` | — | 1 | `r → v` | 取出 Ok payload（转移） |
| `UnwrapErr` | — | 1 | `r → e` | 取出 Err payload（转移） |

规则：
- `UnwrapOk` 遇 `Err`（或反之）→ `RuntimeError::ResultMismatch`（防御性，
  合法编译产物不会出现——分支由 `IsErr` + 跳转保证）
- payload 的取出是**转移**：`Result` 值被消耗

---

## 6. 源码构造 → 字节码对照

> **分号说明**：0.0.1 需显式写分号 `;`；0.0.2 起 ASI 落地、分号可选——
> 分号的有无在 AST 之后不可见，本节对照不受影响。
> **函数声明不加分号**：`func` 声明末尾无分号，语法见 `SYNTAX.ebnf`。
> **返回值**：函数体最后一个表达式无分号，否则会丢弃返回值。

### 6.1 表达式语句

```fleen
fib(3);
```

```text
Const    3            ; 实参
Call     fib
Pop                   ; 语句值被丢弃
```

### 6.2 绑定与赋值

```fleen
x = 42;
x = 43;
```

```text
Const    42
StoreLocal 0          ; x 绑定到 slot 0
Const    43
StoreLocal 0          ; x 赋值（同一 slot）
```

规则：
- **遮蔽（shadowing）在编译期处理**：新绑定分配新 slot，旧 slot 不再引用——VM 无需感知遮蔽

### 6.3 if / and / or 短路

```fleen
if a and b { 1 } else { 2 }
```

```text
LoadLocal a
JumpIfFalse L_else    ; and 左侧短路
LoadLocal b
JumpIfFalse L_else    ; and 右侧短路
True
Jump       L_end
L_else:
False
L_end:
JumpIfFalse L2
Const 1
Jump L3
L2:
Const 2
L3:
```

规则：
- `if` 链：条件 `JumpIfFalse` → 下一分支；分支末 `Jump` 到出口
- 无 `else` 且条件不成立时，`if` 表达式值为 `Unit`

### 6.4 while

```fleen
while x < 10 {
    x = x + 1;
}
```

```text
L_cond:
LoadLocal x
Const    10
Lt
JumpIfFalse L_end
LoadLocal x
Const    1
IAdd
StoreLocal x
Jump     L_cond
L_end:
Unit                  ; while 表达式值为 Unit
```

### 6.5 choose

```fleen
choose value {
    when 0 { "zero" }
    when x if x > 10 { "big" }
    otherwise { "other" }
};
```

**分号规则**：`when` 子句和 `otherwise` 子句本身不带分号，只有 `choose` 整体作为语句使用时末尾加分号。

```text
LoadLocal value
L_m0:                     ; when 0
Dup
Const    0
Eq
JumpIfFalse L_m1
Pop                       ; 丢弃原始被匹配值（bool 已被 JumpIfFalse 消耗）
Const    "zero"
Jump      L_end

L_m1:                     ; when x if x > 10
BindMatch 1               ; 绑定到守卫 slot 1
LoadLocal 1
Const    10
Gt
JumpIfFalse L_m2
Const    "big"
Jump      L_end

L_m2:                     ; otherwise
Pop
Const    "other"
L_end:
```

规则：
- 展平为**比较链**，每个 `when` 一个比较 + `JumpIfFalse`
- 守卫 `if` 紧跟在模式比较之后
- 无 `otherwise` 且全部不匹配 → 表达式值为 `Unit`（穷尽性由 `typeck` 检查）

> **标签约定**：文档中的 `L_m0`、`L_m1`、`L_end` 等标签仅是记法。
> 字节码二进制里**没有标签**——跳转操作数是函数内**绝对字节偏移**（§5.6）。
> Codegen 内部用单调递增计数器生成全局唯一标签（`L0`、`L1`、`L2` …），
> 嵌套 `choose` 也只是继续递增编号，天然不会重名。最终汇编阶段将标签解析为偏移量。

### 6.6 函数定义

```fleen
func add(a: int, b: int): int = a + b
```

```text
; 函数表中 Func { name: "add", params: 2, locals: 2, code: ... }
LoadLocal 0
LoadLocal 1
IAdd
Return
```

```fleen
func greet(name: string) {
    print("Hello, ", name);
}
```

```text
; 函数表中 Func { name: "greet", params: 1, locals: 1, code: ... }
LoadLocal 0
Call     print
Unit                  ; unit 函数显式压入 Unit
Return
```

### 6.7 `?` 传播（v0.0.2）

```fleen
q = div(a, b)?;
```

```text
<eval div(a, b)>      ; r: Result
Dup                   ; r r
IsErr                 ; r bool
JumpIfTrue  L_err
UnwrapOk              ; r → v
StoreLocal q
Jump        L_cont
L_err:
UnwrapErr             ; r → e
Return                ; 提前返回 Err（函数级）
L_cont:
```

### 6.8 choose（Result scrutinee，v0.0.2）

```fleen
choose div(10, 2) {
    when Ok(v) { v }
    when Err(e) { 0 - 1 }
}
```

```text
<eval div(10, 2)>     ; r
Dup
IsErr
JumpIfTrue  L_err
UnwrapOk              ; payload v（转移）
StoreLocal v          ; when Ok(v) 绑定
<ok arm body>         ; 尾值
Jump        L_end
L_err:
UnwrapErr             ; payload e（转移）
StoreLocal e
<err arm body>
L_end:
```

规则：
- `Ok`/`Err` 臂的 payload 由 `UnwrapOk`/`UnwrapErr` **转移**后 `StoreLocal` 绑定，
  无需 `BindMatch`（被匹配值已被消耗，不再参与比较链）
- 带 guard（`when Ok(x) if x > 0`）时，codegen 需在 guard 求值前保留 payload
  可回退（实现期确定具体序列，验收标准：语义等价 + verify 通过）
- 穷尽性由 `typeck` 检查：`Ok` + `Err` 两臂齐即穷尽

---

## 7. 完整示例

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}
```

函数表：

| FuncId | name | params | locals |
|--------|------|--------|--------|
| 0 | `fib` | 1 | 1 |

```text
L0:
LoadLocal 0
Const    <1>          ; 2
Lt
JumpIfFalse L1
LoadLocal 0            ; n
Return

L1:
LoadLocal 0
Const    <1>          ; 2
Eq
JumpIfFalse L2
Const    <0>          ; 1
Return

L2:
LoadLocal 0
Const    <0>          ; 1
ISub                   ; n-1
Call     fib
LoadLocal 0
Const    <1>          ; 2
ISub                   ; n-2
Call     fib
IAdd                   ; fib(n-1) + fib(n-2)
Return
```

常量池：

| ConstId | 值 |
|---------|-----|
| 0 | `1` |
| 1 | `2` |

---

## 8. fleen-verify 的职责

`fleen-verify` 静态校验字节码模块，**不执行**：

- [ ] 所有 `Const` / `LoadFunc` / `Call` 的索引在范围内
- [ ] 所有跳转目标落在**指令边界**上
- [ ] 每个函数每条路径以 `Return` 结束（无掉出函数底的执行路径）
- [ ] **操作数栈深度不下溢**：对每个函数做**全控制流抽象栈深度分析**——
  - 每条指令的栈效果（`Δdepth`）静态已知（见 §5 指令表）
  - 从函数入口（栈深 = 0）开始，按控制流图传播抽象栈深
  - 合并分支时：若所有前驱栈深一致，取该深度；**不一致即 verify 失败**（不做“取最大/最小”的保守近似）
  - 每条指令执行前的栈深 ≥ 该指令所需最小栈深
- [ ] `StoreGlobal` 的目标全部 `mutable: true`，**唯一例外**：入口函数（全局初始化的 `__init__`）中对全局的初始化写（用于给 `const` 全局赋初值）
- [ ] 常量池无重复项
- [ ] 每个函数 `locals >= params`，且所有 `LoadLocal` / `StoreLocal` 的 slot 操作数 < `locals`
  （0.0.2 起 `MakeRefLocal` / `MoveLocal` / `CloneLocal` 的 slot 同规则）

### 每指令栈深效果（Δdepth）

栈深分析以一张静态表为准（`n` 为操作数个数）：

| 指令 | Δdepth | 最小执行前深度 |
|------|--------|----------------|
| `Const` / `True` / `False` / `Unit` / `LoadLocal` / `LoadGlobal` / `LoadFunc` | +1 | 0 |
| `Pop` | -1 | 1 |
| `Dup` | +1 | 1 |
| `StoreLocal` / `StoreGlobal` | -1 | 1 |
| `IAdd`…`IMod` / `FAdd`…`FDiv` / `Eq`…`Ge` | -1 | 2 |
| `Not` / `NegI` / `NegF` | 0 | 1 |
| `Jump` | 0 | 0 |
| `JumpIfFalse` / `JumpIfTrue` | -1 | 1 |
| `Call` | 1 - p（p 为被调函数 `params` 数） | p |
| `CallValue` | -(argc + 1) + 1 = -argc | argc + 1 |
| `BindMatch` | 0 | 1 |
| `AllocBox` / `DerefBox` / `DupDeep` | +1 | 1 |
| `MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal` | +1 | 0 |
| `StoreDerefBox` | -2 | 2 |
| `PackOk` / `PackErr` / `IsErr` / `UnwrapOk` / `UnwrapErr` | 0 | 1 |
| `Return` | 视为路径终止 | 1（返回值在栈顶） |

规则：
- `Call` 的最小深度为被调函数的 `params` 个数；被调函数的返回值一定压栈，故净效果 `1 - params`。
- `CallValue` 需要栈上 `f` 加 `argc` 个实参，弹出后压入返回值，净效果 `-argc`。
- `Return` 要求执行前栈深恰好为 1（只剩返回值），多余残留视为 verify 失败。

规则：
- 该分析是**精确的而非保守的**：字节码指令栈效果固定，分析能证明不下溢
- `StackUnderflow` 在 VM 里纯粹是**防御性兜底**（verify 有 bug、或字节码经手工篡改）
- `codegen` 输出必须能通过 `fleen-verify`——这是 codegen 的验收标准

---

## 9. 二进制序列化（`.flnc`）

0.0.1 定义内存 `Module` 的二进制落盘格式，扩展名 `.flnc`。所有整数**小端**，多字节字段按下述宽度。
0.0.2 **零布局变更**：`version` 升为 `2`，读取端接受 `{1, 2}`；`span_map` 自 0.0.2 起填充（§2）。

### 文件头

| 字段 | 宽度 | 值 |
|------|------|-----|
| magic | 4 B | `"FLNC"`（0x46 0x4C 0x4E 0x43） |
| version | u16 | 字节码版本，当前 `1` |

### 常量池

`constants_len: u32`，随后逐条：

| tag | 含义 | 后续字段 |
|-----|------|----------|
| 0 | `Int` | `v: i64` |
| 1 | `Float` | `bits: u64`（f64 的 bit pattern） |
| 2 | `Str` | `len: u32` + UTF-8 字节 |

### 函数表

`functions_len: u32`，随后逐条：

| 字段 | 宽度 |
|------|------|
| `name` (ConstId) | u32 |
| `params` | u16 |
| `locals` | u16 |
| `is_builtin` | u8（0/1） |
| `code_len` | u32 |
| `code` | `code_len` 字节 |
| `span_map_len` | u32 |
| `span_map` | 每条 `offset: u32, start: u32, end: u32` |

### 全局变量表

`globals_len: u32`，随后逐条：

| 字段 | 宽度 |
|------|------|
| `name` (ConstId) | u32 |
| `mutable` | u8（0/1） |

### 入口

`entry: u32`（FuncId）。

规则：
- 读写必须通过同一实现（`codegen::flnc::{to_bytes, from_bytes}`），不允许手写解析。
- 版本号不匹配、magic 不符、索引越界、长度截断均为反序列化错误。
- `is_builtin` / `mutable` 只接受 `0` 或 `1`，其他字节视为格式错误。
- 常量池 `tag` 只接受 `0/1/2`，其他视为格式错误。
- 后续版本（2+）只在**末尾追加**新表；已有表的字段顺序与宽度保持稳定。
- `version` 接受 `{1, 2}`（0.0.2 起）；v2 不改变任何 v1 已有表的字段顺序与宽度。

---

## 10. 版本规范

| 字节码版本 | 对应语言版本 | 内容 |
|-----------|-------------|------|
| 1 | 0.0.1 | 本文档定义的指令集 |
| 2 | 0.0.2 | + 所有权指令 `AllocBox` / `DerefBox` / `StoreDerefBox` / `MakeRefLocal`（0x80–0x83）、`DupDeep` / `MoveLocal` / `CloneLocal` / `CloneGlobal`（0x90–0x93）、Result 指令（0xA0–0xA4）；`span_map` 开始填充（§5.9–5.11、§2） |
| 3 | 0.0.3 | + `struct` / 数组指令 |

规则：
- 字节码版本随语言版本**单调递增**
- 新指令只插入预留编号组，**不改变已有指令的编号和编码**
- VM 拒绝执行超出其支持版本的模块
