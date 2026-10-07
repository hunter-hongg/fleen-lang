# Fleen 0.0.2 特性规划（所有权起步）

> **快、简、安、直觉**
> **主题**：所有权系统起步 —— 堆值有主人，错误会流动，分号可安眠。
> **依据**：`DESIGN.md §18` 后续路线、`BYTECODE.md`（§3.2 值表示、§4.2 编号预留、§10 版本规范）、
> `DESIGN.md §18` 已知 Hack 清单、`docs/0.0.1/TICKETS.md` 实现现状。
> **状态**：已定稿（2026-10-06）。各规范文档已按 §11 同步
> （BYTECODE / DESIGN / SYNTAX / SPEC / CHANGELOG / README）；
> 实现按 §10 拆解为 TICKETS 后启动，落地前当前工具链仍为 0.0.1 行为。

---

## 1. 总览

### 1.1 0.0.2 要回答的三个问题

1. **堆上的值归谁？** —— `string`、`box<T>` 有了唯一所有者；`move` 转移、`clone` 复制、`ref` 借用。
   这是"无 GC、所有权 + 借用"内存模型（`DESIGN.md §1`）的第一次落地。
2. **错误怎么流？** —— `Result<T, E>` 有了构造语法（`Ok` / `Err`）、`choose` pattern 和 `?` 传播，
   从"有类型无值"变成可用的错误处理机制。
3. **分号还要写吗？** —— ASI（自动分号插入）落地，`;` 变为可选，0.0.1 程序不受影响。

### 1.2 特性清单（来自 `DESIGN.md §18` 路线）

| # | 特性 | 类别 | 依赖的前置扩展 |
|---|------|------|----------------|
| F1 | `move` / `clone`（所有权） | 语言 + 编译器 + VM | 所有权检查器（新）、字节码 v2 指令 |
| F2 | `box<T>` | 语言 + 编译器 + VM | 所有权、`deref` 关键字、`AllocBox` 等指令 |
| F3 | `ref`（只读借用，仅函数参数） | 语言 + 编译器 + VM | 所有权、借用句柄值表示 |
| F4 | `?` 运算符 | 语言 + 编译器 + VM | **`Ok`/`Err` 构造与 choose pattern**（隐含扩展，见 §3.4）、Result 指令 |
| F5 | ASI（自动分号插入） | 编译器（Parser） | 无 |
| F6 | `print` 内建类型检查修复（严格 string 检查，见 §6） | typeck | **F8**（非 string 实参需 `as string`） |
| F7 | `span_map` 生成 + 运行时诊断 | Codegen + VM | `SpanEntry` 设计（BYTECODE.md 推迟到 0.0.2 的事项） |
| F8 | `as` 类型转换（0.0.2 临时特性，`<value> as T`，见 §3.6） | 语言 + 编译器 + VM | 无 |

> **范围说明**：`DESIGN.md §18` 只列了 `move / clone / box<T> / ref / ? / ASI` 六项。
> 但 `?` 若没有 `Ok(v)` / `Err(e)` 构造语法与 `choose` 的 `Ok`/`Err` pattern，
> 用户代码**构造不出任何 `Result` 值**，`?` 就是死特性（0.0.1 现状：`Result<T, E>`
> 有类型、无值、无 pattern，见 0.0.1 `TICKETS.md` T05 备注）。因此 §3.4 将
> Ok/Err 构造 + Result pattern 作为 F4 的组成部分一并纳入。
>
> **F8 为规划后临时增补（2026-10-07）**：类型转换 `<value> as T`。动机：F6 print
> 修复定稿为"严格 string 检查"（§6）后，非 string 值需要显式 `as string` 才能打印
> ——没有 `as`，标量值无法字符串化。定位为 0.0.2 临时特性，白名单刻意收窄
> （§3.6.2），完整转换体系随泛型/trait（0.1.0）再议。

### 1.3 0.0.1 遗留债务（本版必须偿还）

| 债务 | 出处 | 0.0.2 处置 |
|------|------|-----------|
| `print` 类型检查被特判放宽（任意类型、任意元数） | `DESIGN.md §18`、`typeck/infer.rs` `HACK (0.0.1)` 注释 | §6：内建函数获得真实签名，恢复严格检查 |
| `Value::Str(Rc<str>)` 共享语义与所有权模型冲突 | `BYTECODE.md §3.2` ⚠️ 临时决策 | §5.3：改为 `Str(Box<str>)` 独占所有权 |
| `span_map` 留空、`SpanEntry` 未定义 | `BYTECODE.md §2` 0.0.1 决定 | §7：定义并生成，接入运行时错误诊断 |
| 字节码 0x80–0x9F 编号预留未用 | `BYTECODE.md §4.2` | §5.1：v2 指令占用预留组 |

### 1.4 明确不做（Out of Scope）

| 不做 | 原因 |
|------|------|
| `struct` / `for` / 迭代器 | 0.0.3 范围（`DESIGN.md §18`），避免本版膨胀 |
| 闭包 / `move` 闭包捕获 | 无函数字面量语法；`move` 关键字本版只作用于变量位置 |
| trait / 运算符重载 / 泛型 | 0.1.0 范围；`print` 的多态用内建特例签名解决（§6），不引入 trait |
| 用户自定义 `Drop` / 析构函数 | 析构时机的正确性依赖更完整的生命周期模型；本版只做内置类型的确定性释放 |
| `ref` 局部变量 / `ref` 全局 / 返回 `ref` | 借用仅允许出现在参数位置（`DESIGN.md §9`），使安全性无需完整借用检查器（§3.3 论证） |
| 借用 `box` 内部（`ref` 指向 `deref b`） | 句柄需二级间接，收益低；`clone deref b` 已可满足读取需求 |
| `addr` / `ptr` / `unsafe` | `DESIGN.md §10/§12` 明确 0.0.1 不做，0.0.2 不变 |
| 整数/浮点运算语义变更 | 保持 0.0.1 的 checked 语义 |
| 完整类型转换矩阵（`int as float`、From-like、用户自定义转换） | 0.0.2 的 `as` 仅为 print 修复的最小白名单（§3.6.2）；泛型/trait（0.1.0）落地后再扩 |

---

## 2. 设计决策点（评审确认后写入 DESIGN.md）

以下决策点给出**推荐方案**与理由；评审可推翻，推翻后同步修改 §3–§5 对应细则。

### D1：`move` 显式还是隐式？

| 方案 | 语义 | 代价 |
|------|------|------|
| A. 隐式移动（Rust 风格） | `y = x` 即转移，之后用 `x` 报错；`clone x` 复制 | `move` 关键字在 0.0.2 **无用武之地**（无闭包），与路线图矛盾 |
| **B. 显式移动（推荐）** | owned 值进入绑定/实参位置必须写 `move x`（转移）或 `clone x`（复制）；裸写 `x` 是编译错误 | 代码略繁；`string` 赋值语义相对 0.0.1 是破坏性变更 |

**推荐 B**。理由：

- 路线图明确把 `move` 列为 0.0.2 交付物，方案 A 下该关键字无法落地；
- 符合"显式、无魔法"（`DESIGN.md §1`）：每一次所有权的转移在源码上可见；
- 编译器实现便宜：移动点全部显式标注在 AST 上，仿射检查只需跟踪"已移动集合"，
  无需 Rust 式的 last-use 活性分析；
- `Copy` 类型（int/float/bool/函数值）完全不受影响，0.0.1 代码的主体（数值计算）零迁移。

### D2：`deref b` 的读取语义

**推荐**：`deref b` 产生点内值的**副本**——`T: Copy` 按位复制；`T` 为 owned 时深拷贝。
写回用赋值形式 `deref b = expr;`（旧点内值被释放）。

理由：box 的所有权属于 box 本身，读取点内值不等于取走所有权；若要求
`clone deref b` 才能读 owned 点内值，`print`、比较等场景会非常啰嗦。
"显式 clone"规则（D1）只约束**变量位置**的复制。

`clone box` 的语义：产生**新 box**（先克隆点内值再装箱），与 Rust `Box: Clone` 一致。

### D3：只读使用集合（自动复制，不转移）

下列位置对 owned 值是**只读**使用，编译器自动复制，调用方变量保持可用：

| 位置 | 处理 |
|------|------|
| 比较运算符操作数（`== != < > <= >=`） | codegen 为 owned 操作数生成深拷贝后执行 `Eq` 等指令 |
| `print` 实参 | print 是内建、宿主执行，约定**内建不拥有实参**；codegen 为 owned 实参生成副本 |
| `deref b` 读取 | 见 D2 |

其余值位置（绑定/赋值 RHS、实参、`choose` scrutinee）按 D1/D7 需要显式 `move`/`clone`。
0.0.2 的"只读集合"刻意保持极小：每放开一个位置都要在规范里写明。

### D4：字符串字面量的求值语义

**推荐**：字符串字面量每次求值产生一个**新的 owned 值**（从常量池复制）。
`while … { print("hi"); }` 每轮分配一次——0.0.2 接受此开销（正确性优先），
将来可做字面量驻留 / 写时复制优化，语义不变。

### D5：owned 类型的全局变量

**推荐**：允许，但语义与局部不同——

- 写入（含 `__init__` 初始化）：替换整个值，旧值释放；
- 读取：**自动深拷贝**（等价 `clone`），全局永远保持"有值"状态；
- `move g`（把值移出全局）：**编译错误**——全局没有"被移走"的状态，
  提示改用 `clone g`。

理由：跨函数跟踪全局的"已移动"状态是过程间分析，复杂度与收益不成比例；
自动复制读取与 0.0.1 的使用直觉一致，现有程序零迁移。

### D6：`Result` 的类型推断与错误类型统一

**推荐**：

- `Ok(v)` / `Err(e)` 的类型由**上下文**确定（函数返回类型标注、绑定类型标注、
  `?` 所在函数的返回类型、`choose` scrutinee 类型）；
- 无任何上下文时编译错误，提示补类型标注；
- `?` 传播要求所在函数返回 `Result<T2, E>`，且 `E` 与被传播的 `E` **完全相等**
  （0.0.2 无错误类型转换，`From`-like 机制随 trait 进入 0.1.0）。

### D7：owned 类型的 `choose` scrutinee

**推荐**：scrutinee 位置是**消费位置**——scrutinee 为 owned 局部变量时必须写
`move s` / `clone s`（与 D1 一致）；scrutinee 是普通表达式（如函数调用）时无需关键字。
`when Ok(x)` / `when Err(e)` 的绑定是**转移**（payload 从 `Result` 值中移出）。

### D8：`main` 返回 `Result`

**推荐**：`main` 允许返回 `Result<T, E>`。VM 执行入口得到 `Err(e)` 时，
将 `e` 打印到 stderr 并以退出码 1 结束（`Ok` 行为与现在一致，退出码取整数值或 0）。
错误处理从第一条命令就"会流动"。

---

## 3. 语言特性设计

### 3.1 所有权模型（F1：`move` / `clone`）

#### 3.1.1 值的三分类

| 类别 | 类型 | 赋值/传参语义 |
|------|------|---------------|
| **Copy** | `int` `float` `bool` `unit` 函数值（`FuncId` 引用）、`ref T`（句柄） | 隐式按位复制，一切照旧 |
| **Owned** | `string` `box<T>`、含 owned 分量的 `Result<T, E>` | 唯一所有者；转移需 `move`，复制需 `clone` |
| **Borrow** | `ref T`（仅函数参数位置，见 §3.3） | 只读借用，不转移 |

判定规则（typeck 实现）：`is_copy(T)`——基本数值类型为真；`string`、`box<_>` 为假；
`Result<T, E>` = `is_copy(T) && is_copy(E)`；函数类型为真（`FuncId` 是引用，非拥有数据）。

#### 3.1.2 语法

```fleen
s = "hello";          // 绑定 owned 值（RHS 是新值，无需关键字）
t = move s;           // 转移：s 之后不可再用
t = clone s;          // 深拷贝：s 仍可用
s = "again";          // 合法：s 已重新绑定（此为遮蔽/重新赋值，与 move 无关）
```

`move` / `clone` 是**前缀关键字表达式**，与 `-`、`!` 同级（一元层），操作数是
**位置（place）**：

| 表达式 | 操作数 | 效果 |
|--------|--------|------|
| `move ident` | 局部变量 | 转移所有权，原变量进入"已移动"状态 |
| `clone ident` | 局部变量 | 深拷贝，原变量不变 |
| `clone deref b` | box 点内值 | 深拷贝点内值，box 不变 |
| `clone g`（g 为全局） | 全局 | 深拷贝（等价读取语义，见 D5） |

**禁止**：`move deref b`（不可移出 box）、`move g`（不可移出全局）、
`move s`（s 为 `ref` 参数——不可移出借用）、`move x`（x 为 Copy 类型——无意义，
报错提示去掉 `move`）。

#### 3.1.3 核心规则

1. **Copy 类型的使用永远不需要 `move` / `clone`**（规则 0，覆盖 0.0.1 绝大多数代码）。
2. owned 局部变量的值进入**消费位置**时必须显式 `move` / `clone`：
   - 绑定/赋值 RHS 顶层：`y = move x;` / `y = clone x;`
   - 实参：`f(move x)` / `f(clone x)`
   - `choose` scrutinee：`choose move s { … }`
   - 裸写 `y = x;`（x 为 owned）→ 编译错误，help 提示补 `move` 或 `clone`。
3. **产出位置隐式转移**（值顺着"出口"流走，无需关键字）：
   - 函数体尾表达式、块尾表达式、`if`/`choose` 分支尾表达式；
   - `func greet(): string = s` 中的 `s` 被隐式转移（此后 `s` 已移动）；
   - 分支尾的转移是**条件性转移**：`y = if c { s } else { t };` 之后使用 `s`
     报"可能已移动"（§3.1.4 第二个示例）。
4. **消费位置不接受"非位置"的 owned 新值**之外的裸变量以外的东西：
   `y = fib(n);`（返回 owned 串）合法——函数返回值是新鲜值，天然转移。
5. **已移动不可用**：任何路径上使用已移动的绑定 → `UseAfterMove`；
   部分路径上已移动 → `MaybeMovedAfterBranch`。
6. `const` 与可变性规则不变：`const s = "hi";` 同样可被 `move`（移动是最后一次使用）；
   对已 `move` 的变量赋值同样报错（已移动 ≠ 未绑定，错误消息不同）。
7. 函数参数按声明类型的类别处理：参数为 owned → 实参处需 `move`/`clone`；
   参数为 Copy → 照旧；参数为 `ref T` → 传借用（§3.3），不要 `move`。
8. 所有权检查以函数为单位，**不穿越函数边界**（与作用域规则一致）。

#### 3.1.4 错误信息示例

```
error: use of moved value `s`
  --> demo.fln:4:11
   |
 3 | t = move s;
   |          - `s` moved here
 4 | print(s);
   |       ^ `s` was moved
   |
help: clone the value if you still need it: `t = clone s;`
```

```
error: `s` may be moved in one of the branches
  --> demo.fln:6:11
   |
 4 | y = if c { s } else { t };
   |            - `s` moved in this branch
 6 | print(s);
   |       ^ use of possibly-moved `s`
   |
help: write `clone s` in the branch, or `clone s` here
```

```
error: cannot move out of `x` without `move`
  --> demo.fln:8:5
   |
 8 | y = x;
   |     ^ `x` owns heap data (string)
   |
help: transfer ownership: `y = move x;`, or copy: `y = clone x;`
```

#### 3.1.5 对 0.0.1 程序的影响（迁移清单）

| 0.0.1 代码 | 0.0.2 行为 | 迁移 |
|-----------|-----------|------|
| 数值/布尔计算、控制流、函数 | 不变 | 无 |
| `s2 = s1;`（string 赋值） | 编译错误 | `s2 = move s1;` 或 `s2 = clone s1;` |
| `print(任意值)` | 不变（print 参数只读，见 D3） | 无 |
| `print(func_value)` | **编译错误**（0.0.1 因 Hack 放行） | 按设计拒绝 |
| `clone` / `move` / `deref` 用作变量名 | **编译错误**（成为保留关键字） | 改名（0.0.1 允许、0.0.2 保留） |
| 显式分号 | 仍合法 | 无 |

> 版本策略（SPEC §13）：`0.0.x` 为 API 不稳定期，上述破坏性变更可接受，
> 全部记录于 CHANGELOG 的 "Breaking" 小节。

### 3.2 box<T>（F2）

#### 3.2.1 语法与语义

```fleen
b = box 42;                  // b: box<int>，堆分配
n = deref b;                 // 读：副本（Copy 类型按位复制）
deref b = n + 1;             // 写：替换点内值，旧值释放
c = clone b;                 // 深拷贝：新 box + 新点内值（D2）
s = box "heap";              // s: box<string>，点内值为 owned
t = clone deref s;           // 深拷贝点内值；box 本身不动
```

- `box expr`：前缀关键字表达式，一元层。`box` 已是类型关键字（`BoxType`），
  依据语法位置区分：类型位置 `box<T>`，表达式位置 `box expr`，无歧义。
- `deref`：新增关键字（前缀，一元层），操作数为 postfix 表达式（0.0.2 即标识符）。
- 赋值目标扩展：`deref b = expr` 是新的赋值目标形态；`deref b` 不可带类型标注
  （与 §3.4"赋值不能带类型标注"一致）。
- 点内值类型 `T` 的 0.0.2 允许集合：`int` `float` `bool` `string` `box<U>`
  `Result<T2, E>`（无数组、无 struct）。
- **不可移出 box**：`move deref b` 编译错误——box 唯一所有权不因读取而破坏；
  需要值请 `clone deref b`。

#### 3.2.2 释放时机（确定性析构，无 GC）

| 事件 | 处置 |
|------|------|
| 持有者变量离开作用域（帧销毁） | box 连同点内值释放 |
| 持有者被 `move` 转移 | 释放责任随值转移，原槽清空 |
| 持有者被重新赋值 / `deref b =` 覆盖 | 旧值先释放 |
| 表达式语句丢弃 box 值 | 立即释放 |
| `clone b` | 新旧 box 独立，互不影响 |

不存在循环引用：box 唯一所有权、`ref` 不可存储 → 不需要 GC，与 `DESIGN.md §14` 一致。

#### 3.2.3 错误信息示例

```
error: cannot move out of a `box`
  --> box.fln:5:5
   |
 5 | y = move deref b;
   |     ^^^^^^^^^^^^ box owns its pointee exclusively
   |
help: copy the pointee instead: `y = clone deref b;`
```

### 3.3 ref（F3：只读借用，仅函数参数）

#### 3.3.1 语法与语义

```fleen
func shout(s: ref string): int {
    print(s);            // 只读使用
    42
}

func main(): int {
    name = "fleen";
    shout(name);         // 传借用：name 仍可用
    shout(name);         // 再借一次，合法
    0
}
```

- `ref T` 只能出现在**函数参数类型**位置（`DESIGN.md §9` 即如此约定）。
  `ref` 局部变量、`ref` 全局、`ref` 返回值、嵌套 `ref ref T` 一律编译错误
  （错误 kind：`RefNotAllowedHere`）。
- 参数声明为 `ref T` 后，函数体内该参数：
  - 只读——赋值、`move` 均编译错误（`AssignToRefParam` / `MoveOfBorrowed`）；
  - `clone s` 得到 owned `T`（脱离借用）；
  - 可作为 `ref T` 实参转发给下一个函数（句柄流动，零拷贝）。
- 实参位置：传**不转移**，`name` 不需要 `move`/`clone`（与 D3 精神一致：
  借用是安全的默认）。实参可以是：局部变量、全局变量、另一个 `ref` 参数。
  不支持 `deref b` 借用 box 内部（§1.4）。

#### 3.3.2 为什么不需要借用检查器

借用句柄只存活于**一次调用**期间：

1. 被借用的槽位于调用方帧（或全局表），调用期间调用方挂起，槽内容不会被改写；
2. 被调方对句柄只有只读操作，无法把句柄存到任何存活更久的地方
   （不能赋值、不能存全局、不能返回）；
3. 调用返回，句柄消亡——**没有悬垂的可能**。

因此 0.0.2 的"借用检查"退化为几条静态合法性规则（上表），不需要生命周期标注，
不需要 CFG 级借用分析。这是"仅参数借用"设计的核心红利。

#### 3.3.3 类型规则

- `ref T` 是独立类型：`ref string ≠ string`。把 `ref string` 赋给 `string`
  绑定是类型错误，提示 `clone s`。
- `ref Copy类型`（如 `ref int`）允许但无意义（等价 `int`），不报错（规则一致性优先）。
- 传 owned 实参给 owned 参数却写成借用是**类型错误**（参数表决定），
  提示 `move` / `clone`；反之传 `move x` 给 `ref` 参数报错（借用不接受转移）。

### 3.4 Result：Ok/Err 构造、choose pattern 与 `?`（F4）

#### 3.4.1 Ok / Err 构造表达式

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}
```

- `Ok(expr)` / `Err(expr)` 在语法层就是普通调用表达式（`Call(Ident)`），
  由 typeck 识别为内建构造子（与 `print` 同机制，允许被用户遮蔽，见 §4.3）。
- 类型推断规则（D6）：由上下文确定 `Result<T, E>`；无上下文报
  `CannotInferResultType`，提示标注。分支统一沿用现有规则（0.0.1 要求完全一致）。
- 构造是**装箱**：`Ok(v)` 把 `v` 移入 `Result` 值。

#### 3.4.2 choose 的 Ok/Err pattern

```fleen
res = choose div(10, 2) {
    when Ok(v) { v }
    when Err(e) {
        print("error: ", e);
        0 - 1
    }
};
```

- pattern 语法扩展：`result_pattern = ("Ok" | "Err") , "(" , ident , ")"`；
  绑定 `v: T`、`e: E`，绑定语义为**转移**（D7）。
- 穷尽性：`Ok` + `Err` 两臂齐 → 穷尽（无需 `otherwise`）；只有其一 → 必须补
  `otherwise` 或另一臂；带 guard 的臂不计入穷尽（与现有规则一致）。
- 现有 `DESIGN.md §8` 的示例从此转正。

#### 3.4.3 `?` 传播运算符

```fleen
func ratio(a: int, b: int): Result<int, string> {
    q = div(a, b)?;      // Err 则整函数立即返回该 Err
    r = div(q, 2)?;
    Ok(r)
}
```

- `?` 为**后缀**运算符（postfix 层，与调用/索引同层），操作数类型必须为
  `Result<T, E>`。
- 合法条件（D6）：所在函数的返回类型标注为 `Result<T2, E>`，且 `E` 与操作数的
  `E` 完全相等。条件不满足报 `QuestionOutsideResultFn`（main 返回 int 时
  `?` 报此错）或 `QuestionTypeMismatch`。
- `?` 在 `main` 中合法（main 返回 `Result<int, E>` 即可），Err 退出行为见 D8。
- 表达式语句的值类型为 `Result<T, E>` 时报 `UnhandledResult`——Result 不允许
  静默丢弃，提示绑定或加 `?`（0.0.2 唯一的"值必须被处理"规则）。

### 3.5 ASI（F5：自动分号插入）

> **权威设计见 `docs/0.0.2/ASI.md`**（2026-10-07 定稿）。
> 本节为摘要，与 ASI.md 冲突时以 ASI.md 为准。

#### 3.5.1 策略：前置 pass（换行敏感）

定稿采用 **Lex → ASI pass → Parse** 架构，语义为**换行敏感**（语句边界在
换行处判定，同行语句拼接非法）。lexer 新增 `Newline` token；ASI pass
（`parser/asi.rs` 纯函数）消费全部 Newline，在语句边界插入显式 `Semi`，
对既不能续接也不能起始语句的 token 报 `ExpectedSemiOrNewStmt`。
pass 输出的 Token 流与 0.0.1 同构（分号齐全），Parse 阶段沿用原逻辑，
仅四处配套：绑定/const 贴 `}` 免分号、Assign 形态不作尾表达式、
block-like 头之后的续接爬升、语句入口 guard。

判定摘要（换行或 EOF 处，`p` 为前一 token、`n` 为后一 token）：

> 1. 最内层未闭合括号是 `(` 或 `[` → 丢弃（`{` 内 ASI 照常生效）；
> 2. `n` 是 `}` → 丢弃（永不在 `}` 前插——尾表达式 vs 绑定由 parser 结构区分）；
> 3. `p` 不能结尾语句 → 丢弃（跨行续接）；
> 4. `n` ∈ 隐式结束集（起始集 ∖ {`(` `-`} ∪ {`}` EOF}）→ 插入 `;`；
> 5. `n` ∈ 续接集 ∪ {`else` `elif` `when` `otherwise` `;`} → 丢弃（续接优先）；
> 6. 其余 → 报错 `ExpectedSemiOrNewStmt`。

#### 3.5.2 陷阱用例（必须进规范与测试）

| 代码 | 0.0.2 解析 | 说明 |
|------|-----------|------|
| `x = 1` ⏎ `y = 2` | 两条语句 | `Ident` ∈ 起始集 |
| `x = 1 y = 2`（同行） | 语法错误 | 换行敏感：同行拼接非法 |
| `x = 1` ⏎ `- 2` | `x = (1 - 2)` | `-` ∈ 续接集，二元优先 |
| `f()` ⏎ `(g())` | `f()(g())` 调用链 | `(` ∈ 续接集（调用） |
| `f()` ⏎ `[0]` | `f()[0]` 索引 | `[` ∈ 续接集 |
| `x = 1` ⏎ `!flag` | 两条语句 | `!` 非二元续接，`!flag` ∈ 起始集 |
| `if c { 1 }` ⏎ `else { 2 }` | 一个 if-else 表达式 | `else` 归属该 if（跨行连接合法，现行 parser 亦然） |
| `if c { 1 };` ⏎ `else { 2 }` | 语法错误 | `else` 不续接**已完结**（有分号）的 if 语句 |
| `while x < 10 {` ⏎ `x = x + 1` ⏎ `}` | 合法 | 块内绑定贴 `}` 免分号 |
| `when 0 { "zero" }` ⏎ `otherwise { "o" }` | 合法 | when/otherwise 不带分号规则不变 |

#### 3.5.3 不变量

- 0.0.1 全部程序（显式分号）逐字节合法——`;` 永远被接受；
- `func` 声明后无分号、块尾表达式无分号、when/otherwise 无分号的规则不变；
- 分号的有无在 AST 之后不可见——MIR/字节码不变（`has_semi` 语义见 ASI.md §4）；
- `tokenize()` 公开 API 输出新增 `Newline` token（契约变化，见 ASI.md §6）。

### 3.6 类型转换 `as`（F8：0.0.2 临时特性，print 修复的前置）

> **定位**：为 F6（print 严格 string 检查，§6）提供最小转换手段——不引入 `as`，
> `print(42)` 之外没有任何合法的打印途径。白名单刻意收窄到"标量 → string"；
> 完整转换矩阵（`int as float`、From-like、用户自定义转换）随泛型/trait（0.1.0）再议。

#### 3.6.1 语法与优先级

```fleen
n = 42 as string;        // "42"
t = true as string;      // "true"
print(42 as string);     // 就地转换（print 仅接受 string，见 §6）
```

- `as` 为中缀关键字，新增 `TokenKind::As`（`as` 成为保留字，Breaking）；
- 新增 cast 层，介于 unary 与二元之间（与 Rust 一致：unary 比 `as` 紧，`as` 比二元紧）：
  - `deref b as string` = `(deref b) as string`（`b: box<int>` 时合法）；
  - `1 + 2 as string` = `1 + (2 as string)`；
  - `box 1 as string` = `box (1 as string)`（前缀关键字绑定更紧）。
- ASI：`As` 加入续接集（`can_continue_expr`），`x = 42` ⏎ `as string` 跨行续接，
  与二元运算符同规则（U03 架构不变，仅补谓词，ASI.md §4 续接集同步一行）。

#### 3.6.2 语义与规则

| 规则 | 内容 |
|------|------|
| 白名单 | 仅 `int / float / bool as string`；其余一切（`string as string`、`int as float`、`box<T> as …`、`ref T` 操作数、恒等转换）→ `UnsupportedCast { from, to }`，help 说明 0.0.2 仅支持标量 → string |
| 结果值 | **新鲜 owned string**——绑定/实参位置无需 `move` / `clone`（与"RHS 是新值无需关键字"一致） |
| 格式化 | int 十进制（含负号）、bool `true` / `false`、float 与 print 输出一致；格式化实现与 print **共用同一 helper**，避免两处漂移 |
| 运行时 | 新指令 `ToStr`（0xA5，`v → s`，Δ0 min 1，见 §5.1）；`as` 不引入任何隐式数值转换 |

#### 3.6.3 错误信息示例

```
error: unsupported cast: int to float
  --> cast.fln:2:9
   |
 2 | f = 42 as float;
   |         ^^^^^^^
   |
help: 0.0.2 仅支持标量到 string 的转换（`as string`）；完整转换随泛型版本提供
```

---

## 4. 编译器各阶段改动

> 阶段边界与 `SPEC.md §6` 一致：不跳阶段、不共享可变状态、每阶段可独立测试。

### 4.1 Lex（`lexer/token.rs`）

| 项 | 内容 |
|----|------|
| 新关键字 token | `Move`（`move`）、`Clone`（`clone`）、`Deref`（`deref`）、`As`（`as`，F8 §3.6，U13 增补） |
| 新标点 token | `Question`（`?`）、`Newline`（换行，ASI 用；连续折叠、文件起始不发、块注释内不发——见 ASI.md §2） |
| 已有不动 | `BoxType` / `RefType` 已存在（0.0.1 就保留字）；`Ok` / `Err` 保持 `Ident`（构造子在 typeck 识别） |
| `is_keyword` / `keyword_str` / `Display` | 同步补齐 |

### 4.2 Parse（`parser/`）

| 项 | 内容 |
|----|------|
| 新 AST 节点 | `ExprMove { place, span }`、`ExprClone { place, span }`（place = `Ident` 或 `ExprDeref`）、`ExprBox { inner, span }`、`ExprDeref { inner, span }`、`ExprQuestion { inner, span }` |
| 一元层扩展 | `unary = [ "-" \| "!" \| "box" \| "deref" \| "move" \| "clone" ] , postfix`（前缀关键字右结合；`move`/`clone` 的操作数由 typeck 校验为合法 place，parser 只认形态） |
| postfix 扩展 | `postfix = primary , { call \| index \| field \| "?" }` |
| cast 层（F8） | `cast = unary , { "as" , type }`（unary 与二元之间的新层，§3.6）；AST `Expr::Cast { inner, ty, span }`；ASI 续接集补 `As` |
| 赋值目标扩展 | `deref b = expr` 进入 assign 目标判定（`InvalidAssignmentTarget` 之外新增合法形态） |
| Pattern 扩展 | `pattern = neg_pattern \| literal \| ident \| result_pattern`；`result_pattern = ("Ok" \| "Err") , "(" , ident , ")"`（parser 只识别形态，`Ok`/`Err` 语义绑定留给 typeck） |
| ASI 重构 | **前置 pass**（`parser/asi.rs`，见 ASI.md）：lexer 新增 `Newline`；pass 插分号/消费换行；parser 配套四处（绑定贴 `}` 免分号、Assign 不作尾、block-like 头续接爬升、语句入口 guard）；`can_start_stmt` / `can_continue_expr` / `implies_stmt_end` 纯函数便于单测 |
| EBNF | `SYNTAX.ebnf` 升版 v0.0.2：新增 unary 前缀、`?` 后缀、result_pattern、deref 赋值目标；分号注释改为"可选" |

### 4.3 Resolve（`resolver/`）

| 项 | 内容 |
|----|------|
| 新绑定形态 | ref 参数进入绑定表（`BindingKind::Parameter`，新增 `is_ref: bool` 或以类型携带） |
| `Ok` / `Err` | 沿用 `print` 的内建解析机制（可遮蔽、内建标记） |
| 不变 | `=` 三义性判定、遮蔽可变性一致、循环体赋值规则全部不动 |

### 4.4 Typeck（`typeck/`）—— 本版最大新增组件

| 项 | 内容 |
|----|------|
| 类型判定 | `Type::is_copy()`（§3.1.1）；`Type::Box` / `Type::Ref` 从"解析但 Unsupported"转正 |
| **所有权检查器** | 新模块 `typeck/ownership.rs`：对 TypedHir 做**流敏感**遍历，维护每个绑定的 `Alive / Moved { span } / MaybeMoved` 状态（详见下） |
| Ok/Err 构造检查 | 上下文类型统一；无上下文 → `CannotInferResultType` |
| Result pattern | 绑定类型 = `T` / `E`；穷尽性：Ok+Err 齐即穷尽 |
| `?` 检查 | D6 条件校验；表达式语句 Result → `UnhandledResult` |
| `as` 转换检查 | 白名单 §3.6.2（仅标量 → string）；非法 → `UnsupportedCast` |
| print 修复 | 见 §6 |
| 新错误 kind | `UseAfterMove`、`MaybeMovedAfterBranch`、`MoveOfCopyType`、`MoveOutOfBox`、`MoveOutOfGlobal`、`MoveOfBorrowed`、`AssignToRefParam`、`RefNotAllowedHere`、`OwnedArgRequiresMove`（help: `move`/`clone`）、`BorrowArgWithMove`、`CannotInferResultType`、`QuestionOutsideResultFn`、`QuestionTypeMismatch`、`UnhandledResult`、`BoxInnerTypeNotAllowed`、`UnsupportedCast` |

**所有权检查器算法（草案）**：

```text
对每个函数体：
  state: Map<BindingId, MoveState>   // Alive | Moved{span} | MaybeMoved{spans}
  遍历 TypedHir（语句序 + 控制流结构感知）：
    - 绑定/赋值（RHS 顶层为 move/clone）→ 按 RHS 形态登记转移或复制
    - 消费位置出现裸 owned ident → 报 OwnedArgRequiresMove（D1 规则 2）
    - move ident → state[ident] = Moved{span}；之后再读/写/移动 → UseAfterMove
    - if / elif / else：各分支从快照独立推导，汇合点合并：
        全部分支 Moved → Moved；部分分支 Moved → MaybeMoved；否则 Alive
    - while / choose：入口快照；体内转移在**回边/出口**处取最悲观
      （循环体内移动 ⇒ 出口 MaybeMoved；回边处必须 Alive，否则报错——
        循环内 move 后下一轮再用即 UseAfterMove）
    - 函数边界：state 清空（不穿越）
  尾表达式（产出位置）的隐式转移同样登记（规则 3）
```

### 4.5 Lower（`lower/`）

| 项 | 内容 |
|----|------|
| MIR 指令扩展 | `AllocBox`、`DerefBox`、`StoreDerefBox`、`MakeRefLocal(u16)`、`DupDeep`、`MoveLocal(u16)`、`CloneLocal(u16)`、`CloneGlobal(u16)`、`PackOk`、`PackErr`、`IsErr`、`UnwrapOk`、`UnwrapErr`、`ToStr`（F8 §3.6）（§5.1 全表） |
| **Span 下沉** | `MirInstr` 由裸 enum 改为 `struct MirInstr { kind: MirInstrKind, span: Span }`（或每变体携带 span），Lower 从 TypedHir 逐指令携带 span——这是 §7 span_map 的数据来源 |
| 降载规则新增 | `box e` → `<e>; AllocBox`；`deref b` → `<b>; DerefBox`；`deref b = v` → `<b>; <v>; StoreDerefBox`；`move x`/`clone x` → `MoveLocal/CloneLocal slot`；`x?` → §5.6 序列；`f(x)`（owned 参数）→ `MoveLocal/CloneLocal` + `Call`；比较/print 的 owned 操作数 → `CloneLocal/CloneGlobal` 先行（D3）；`e as string` → `<e>; ToStr`（§3.6） |
| choose（Result scrutinee） | `IsErr + UnwrapOk/UnwrapErr` 展平，payload 直接 `StoreLocal` 绑定（无需 `BindMatch`，见 §5.6） |

### 4.6 Codegen（`codegen/`）

| 项 | 内容 |
|----|------|
| Opcode | 新增 v2 指令与 0.0.1 指令 1:1 对应（§5.1）；`version` 字段升为 `2` |
| span_map | 逐指令写入 `SpanEntry { offset, start, end }`（格式 §9 已预留，无需改 .flnc 布局） |
| 常量池 | 不变（字符串仍入池；`Const` 指令执行时复制为 owned 值，见 D4） |
| 反序列化 | `from_bytes` 接受 `version ∈ {1, 2}`（向后兼容旧 .flnc） |

### 4.7 Verify（`fleen-verify/`）

| 项 | 内容 |
|----|------|
| 栈效果表 | 新增 13 条指令的 `Δdepth` / 最小深度（§5.2 全表） |
| 新校验 | `MakeRefLocal` 的 slot < 当前函数 `locals`（与 LoadLocal 同规则）；`StoreDerefBox` 最小深度 2；其余结构校验沿用 |
| 版本门 | 接受 `version ∈ {1, 2}`；v2 指令出现在 v1 模块 → `BadOpcode`（自然成立） |

### 4.8 VM（`fleen-vm/`）

| 项 | 内容 |
|----|------|
| Value 重构 | §5.3 |
| Move 语义 | `MoveLocal` = `mem::replace(&mut stack[slot], Value::Unit)`；被移空槽后续只会被重新 `StoreLocal`（typeck 保证不读） |
| 释放时机 | `Vec::truncate`（帧销毁）、槽位覆盖、`Pop` 处由 Rust `Drop` 天然完成——VM 不写显式析构循环，`debug_assert!` 辅助验证"已移动槽不被读" |
| 句柄解引用 | `Value::Ref { base, slot }` 读取统一走 helper（比较/print/clone 自动穿透）；越界 → `RuntimeError::BorrowOutOfRange`（防御） |
| Result 指令 | `UnwrapOk` 遇 `Err` → `RuntimeError::ResultMismatch`（防御，合法字节码不会发生） |
| 转换指令 | `ToStr`：pop 标量 → 格式化为 string（与 print 共用 `fmt` helper）→ 压栈（§3.6） |
| 入口 Err | D8：entry 返回 `Err(e)` → stderr 打印、退出码 1 |
| 新 RuntimeError | `ResultMismatch`、`BorrowOutOfRange` |

---

## 5. 字节码 v2 规范增量（将并入 BYTECODE.md）

### 5.1 新增指令（占满 0x80–0x83、0x90–0x93；Result 组启用 0xA0–0xA4；`as` 转换占用 0xA5）

| opcode | 助记符 | 操作数 | 字节 | 栈效果 | 说明 |
|--------|--------|--------|------|--------|------|
| 0x80 | `AllocBox` | — | 1 | `v → b` | 堆分配，值的所有权转入 box |
| 0x81 | `DerefBox` | — | 1 | `b → b v` | 读点内值（副本/深拷贝），box 仍在栈上 |
| 0x82 | `StoreDerefBox` | — | 1 | `b v →` | 写点内值，旧值释放 |
| 0x83 | `MakeRefLocal` | `u16` slot | 3 | `→ ref` | 生成借用句柄指向本帧 `stack_base + slot`（实参准备） |
| 0x90 | `DupDeep` | — | 1 | `v → v v` | 深拷贝栈顶（`clone` 的栈上形态） |
| 0x91 | `MoveLocal` | `u16` slot | 3 | `→ v` | **消耗性读**：取出槽值，槽清为 `Unit` |
| 0x92 | `CloneLocal` | `u16` slot | 3 | `→ v` | 非消耗读：深拷贝槽值，槽不动 |
| 0x93 | `CloneGlobal` | `u16` gid | 3 | `→ v` | 非消耗读全局（owned 全局的唯一读法） |
| 0xA0 | `PackOk` | — | 1 | `v → ok(v)` | 构造 `Ok` |
| 0xA1 | `PackErr` | — | 1 | `v → err(e)` | 构造 `Err` |
| 0xA2 | `IsErr` | — | 1 | `r → bool` | 测试 `Err` |
| 0xA3 | `UnwrapOk` | — | 1 | `r → v` | 取出 Ok payload（转移） |
| 0xA4 | `UnwrapErr` | — | 1 | `r → e` | 取出 Err payload（转移） |
| 0xA5 | `ToStr` | — | 1 | `v → s` | 标量转 string（`as` 转换，§3.6；结果为新鲜 owned string） |

不变量：已有指令（0x00–0x7F）编号与编码**一字不改**（`BYTECODE.md §10` 规则）；
v1 模块在 0.0.2 VM/verify 上继续可执行。

### 5.2 栈深分析补充（verify 用）

| 指令 | Δdepth | 最小执行前深度 |
|------|--------|----------------|
| `AllocBox` / `MakeRefLocal` / `MoveLocal` / `CloneLocal` / `CloneGlobal` | +1 | 1 / 0 / 0 / 0 / 0 |
| `DerefBox` / `DupDeep` | +1 | 1 |
| `StoreDerefBox` | -2 | 2 |
| `PackOk` / `PackErr` / `IsErr` / `UnwrapOk` / `UnwrapErr` / `ToStr` | 0 | 1 |

### 5.3 Value 表示（替换 `Rc<str>`，偿还 BYTECODE.md §3.2 临时决策）

```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Box<str>),                 // ← 由 Rc<str> 改为独占所有权（深拷贝即真拷贝）
    Boxed(Box<Value>),             // box<T>：唯一所有权
    Ref { base: u32, slot: u16 },  // 借用句柄（仅存在于被调帧生命期）
    Ok(Box<Value>),                // Result 构造
    Err(Box<Value>),
    Unit,
    Func(FuncId),
}
```

- 所有权纪律由 **typeck 静态保证**；VM 侧 Rust 的移动/丢弃语义即运行时实现，
  `Drop` 天然确定性，无 GC。
- `Eq` 语义：`Str` 按内容；`Boxed` 比较点内值（与 Rust `Box: PartialEq` 一致）；
  `Ref` 自动解引用后比较。
- `Value::Ref` 不可序列化（值不落盘，常量池无此形态）。

### 5.4 .flnc 格式

**零布局变更**：`span_map` 字段在 0.0.1 §9 已预留（`span_map_len: u32` +
`offset/start/end` 三元组），0.0.2 只是开始**填充**。`version` 升为 `2`，
读取端接受 `{1, 2}`。未来版本仍只在末尾追加新表。

### 5.5 `?` 的降载与字节码对照

```fleen
q = div(a, b)?;
```

```text
<eval div(a, b)>      ; r: Result
Dup                   ; r r
IsErr                 ; r bool
JumpIfTrue  L_err     ; bool 消耗
UnwrapOk              ; r → v
StoreLocal q
Jump        L_cont
L_err:
UnwrapErr             ; r → e
Return                ; 提前返回 Err（函数级）
L_cont:
```

### 5.6 choose（Result scrutinee）对照

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

> guard（`when Ok(x) if x > 0`）：`StoreLocal x` 后先取 guard 求值，
> 不成立则跳入 `Err` 臂——此时 payload 已被消费，需要 `DupDeep` 预留一份
> （codegen 细节：guard 存在时先 `Dup` + `IsErr` 判定前后保持 payload 可回退，
> 具体序列实现期确定，验收标准是"语义等价 + verify 通过"）。

---

## 6. print 内建修复（F6，偿还 DESIGN.md §18 已知 Hack）

现状：`typeck/infer.rs` 对 `print` 特判，跳过参数类型与元数检查；
`check_builtin_print_wrong_arg` 被改为期望编译通过（`typeck/tests.rs`）。

0.0.2 方案（**严格 string 检查**，依赖 F8 `as` 转换，§3.6）：

1. typeck 建立内建签名表（`BuiltinSig`），`print` 的真实签名为
   **`(string...) -> unit`**——只接受 `string` 实参；
2. 非 string 实参（int / float / bool / 函数值 / `box<T>` / `Result`）→
   `ArgTypeMismatch`，help 提示显式转换：`print(x as string)`
   （标量 → string 的转换见 §3.6）；
3. 元数 ≥ 0（`print()` 合法，打印空行）；
4. 内建不拥有实参（D3）：owned string 实参由 codegen 先行复制；
5. 测试回正：`print("a")` / `print("a", "b")` / `print(42 as string)` → Ok；
   `print(1)` / `print(fib)` → `ArgTypeMismatch`（help: `as string`）；
   原 `check_builtin_print_wrong_arg` 更名/改写为语义准确的用例集；
6. 移除 `infer.rs` 的 `print` 特判分支与对应 HACK 注释。

> **临时性（定稿，写入 DESIGN.md）**：print 收紧为"仅 string + 手动 `as string`"
> 是 0.0.2 的临时取舍——用最小机制换回严格类型检查，且转换/打印的格式化
> 实现共用同一 helper。0.1.0 泛型/trait 落地后重审：或以 `Display`-like trait
> 恢复多态打印，或维持严格签名；在此之前，非字符串打印一律要求显式 `as string`。

---

## 7. span_map 与运行时诊断（F7）

### 7.1 SpanEntry 定义（BYTECODE.md 推迟事项落地）

```rust
pub struct SpanEntry {
    pub offset: u32,  // 指令起始字节偏移（相对函数 code）
    pub start: u32,   // 源码起始字节偏移
    pub end: u32,     // 源码结束字节偏移
}
```

- codegen 逐指令发射（数据来自 §4.5 的 MIR span 下沉）；
- 二进制布局即 0.0.1 §9 预留格式，无需改动。

### 7.2 运行时诊断

- VM 执行出错时按 `当前帧 ip - 1` 查 `span_map`，`RuntimeError` 携带可选 span；
- `fleen-vm` CLI 在 **`.fln` 一站式模式**下持有源码，把字节偏移换算为行列：

```console
$ cargo fln crash.fln
runtime error: division by zero
  --> crash.fln:5:9
   |
 5 |     q = a / b;
   |         ^^^^^
```

- `.flnc` 直接执行时源码未知：退化为打印函数名 + 指令偏移；
- 退出码契约不变（0/1/2）。

---

## 8. 测试计划

> 遵循 SPEC §7：每条规则至少一个测试、每个错误至少一个测试、修 bug 先写测试。
> 文件驱动测试沿用 `tests/<stage>/valid|invalid/` 布局与 e2e 的
> `.fln + .expected / .exit + .stderr` 契约。

### 8.1 分阶段新增用例（节选）

| 阶段 | valid 必测 | invalid 必测 |
|------|-----------|--------------|
| lexer | `move` `clone` `deref` `as` `?` 的 token 化 | — |
| parser | 前缀关键字链（`clone deref b`）、`?` 后缀链、cast 优先级（`deref b as string`）、`deref b = v` 赋值、Ok/Err pattern、ASI 全部陷阱用例（§3.5.2） | `move deref b` 形态、悬空 `?`、`42 as`（缺类型）、ASI 歧义报错 |
| resolver | ref 参数作用域、`Ok`/`Err` 可遮蔽 | ref 局部/全局/返回、重复借用形态 |
| typeck | Ok/Err 上下文推断、Ok+Err 穷尽、`?` 链、cast 白名单三条 | `UseAfterMove`、`MaybeMovedAfterBranch`、`MoveOfCopyType`、`MoveOutOfBox`、`MoveOutOfGlobal`、`MoveOfBorrowed`、`OwnedArgRequiresMove`、`QuestionOutsideResultFn`、`UnhandledResult`、`UnsupportedCast`、`print(函数值)`、`print(1)` |
| lower/codegen | §5.5–5.6 对照序列逐条比对 | — |
| verify | v2 全指令栈深分析、v1 旧模块回归 | 伪造 v2 畸形指令（栈不匹配、句柄 slot 越界） |
| vm | Move/Clone/Drop 时序、Ref 只读、`Boxed` 深拷贝独立性、entry Err 退出码 | `ResultMismatch`、`BorrowOutOfRange` 防御路径 |

### 8.2 E2E 新增

```
tests/e2e/valid/
├── fib.fln                  # 0.0.1 回归：必须原样通过
├── ownership.fln            # move/clone/条件移动
├── box_demo.fln             # box 分配/读写/clone 独立性
├── ref_demo.fln             # 借用传参，原变量仍可用
├── question.fln             # Ok/Err/?/choose-Result 全链路
├── asi.fln                  # 全程无显式分号
└── cast.fln                 # as 转换 + print(x as string)（F8/F6）

tests/e2e/invalid/
├── use_after_move.fln       # 编译错误（.exit=1 + .stderr 关键字）
├── unhandled_result.fln
├── question_in_int_main.fln
└── print_func_value.fln
```

### 8.3 兼容性夹具

- 保留一个 **v1 `.flnc` 固定字节 fixture**，断言 0.0.2 的 verify/VM 仍可执行（向后兼容）；
- `fib.fln` 显式分号版本保持 0.0.1 回归；`asi.fln` 为全程无显式分号 + 混合风格
  的独立全流程用例，输出由 `.expected` 逐行锚定。

---

## 9. 任务拆解与里程碑（延续 0.0.1 的 T 编号，本版用 U 前缀）

| 票号 | 板块 | crate/模块 | 产出 | 依赖 |
|------|------|-----------|------|------|
| U01 | Lex | `lexer/` | Move/Clone/Deref/Question token | — |
| U02 | Parse | `parser/` | 新表达式/pattern/赋值目标 + EBNF v0.0.2 | U01 |
| U03 | Parse-ASI | `lexer/` + `parser/asi.rs` + `parser/` 配套 | 前置 pass（换行敏感）+ 谓词纯函数 + 陷阱用例（§3.5、ASI.md） | U02 |
| U13 | F8 `as` 转换 | lexer→VM 全链（详见 TICKETS U13） | `As` token、cast 层、`ToStr`（0xA5）、白名单检查 | U02（**实现顺序须在 U04 之前**） |
| U04 | Typeck-签名 | `typeck/` | Ok/Err 构造、Result pattern、`?` 检查、print 修复（§6） | U02, U13 |
| U05 | Typeck-所有权 | `typeck/ownership.rs` | is_copy + 流敏感仿射检查 + 新错误族 | U02 |
| U06 | MIR+Span | `lower/` | 新 MIR 指令 + MirInstr 携带 span | U04, U05 |
| U07 | Codegen/Verify | `codegen/`, `fleen-verify/` | v2 opcode、span_map 生成、栈效果表、v1 兼容 | U06 |
| U08 | VM-Value | `fleen-vm/src/value.rs` 等 | Value 重构（Box<str>/Boxed/Ref/Ok/Err） | — （可与 U01–U05 并行） |
| U09 | VM-执行 | `fleen-vm/src/vm.rs` | 新指令执行、Move 语义、句柄解引用、入口 Err | U07, U08 |
| U10 | 诊断 | `codegen/`, `fleen-vm/` | 运行时 span 换算行列 + CLI 输出 | U07 |
| U11 | 测试 | 全仓 | §8 测试矩阵 + E2E + v1 fixture | U09 |
| U12 | 文档 | `docs/`, `CHANGELOG.md` | BYTECODE v2、DESIGN/SPEC/EBNF 升版、CHANGELOG 0.0.2 | U11 |

```
U01 → U02 → U03 ─────────────┐
        ↘                    │
          U13 → U04 ┐        │
          U05 ┴─────┘→ U06 → U07 → U09 → U11 → U12
U08 ────────────────────┘ ↗        ↗
                      U10 ─────────┘
```

> U13 为规划后增补票（`as` 转换，F8，2026-10-07）；F6 print 修复依赖它，
> 实现顺序在 U04 之前（详细拆解见 `TICKETS.md` U13）。

### 里程碑

| 里程碑 | 票号 | 标志 |
|--------|------|------|
| M1 前端 | U01–U03 | 新语法可解析，ASI 陷阱用例全绿 |
| M2 语义 | U04–U05 | 所有权/`?`/print 检查完备，invalid 用例全绿 |
| M3 后端与运行时 | U06–U09 | `question.fln`、`box_demo.fln`、`ref_demo.fln` 端到端跑通 |
| M4 诊断与发布 | U10–U12 | 运行时错误带源码位置；文档同步；`fib.fln` 回归 |

---

## 10. 风险与缓解

| 风险 | 等级 | 缓解 |
|------|------|------|
| 所有权检查器的控制流合并逻辑（条件移动、循环回边）复杂度超预期 | 高 | 算法先限定为 §4.4 草案的保守规则（循环内移动 → 出口 MaybeMoved）；先直线 + if，while 保守化处理；单元测试逐规则覆盖 |
| ASI 陷阱导致静默错误解析 | 中 | 白名单法 + §3.5.2 陷阱表全量进测试；`;` 永远合法保证退路 |
| `Value::Ref` 句柄被伪指令逃逸 | 中 | typeck 规则静态排除；VM 对句柄读取做越界防御（`BorrowOutOfRange`）；verify 不给句柄任何跨帧通路 |
| string 语义变更破坏现有示例/测试 | 中 | 影响面清单（§3.1.5）+ CHANGELOG Breaking 小节 + 迁移提示写进错误 help |
| D1 决策反复（显式/隐式 move）返工 | 中 | 本文档 §2 先评审后动工；U05 所有权检查器与 AST 形态解耦（move/clone 在 AST 已显式标注） |
| span 下沉改动波及 lower/codegen 全部既有测试 | 低 | `MirInstr` 包装是机械重构，一次 PR 完成；0.0.1 测试只改构造处，不改断言 |
| 范围膨胀（struct/闭包提前混入） | 中 | §1.4 负面清单作为评审门禁 |

---

## 11. 文档同步清单（U12）

| 文档 | 变更 |
|------|------|
| `docs/BYTECODE.md` | 升版 v0.0.2：§3.2 Value、§4.2 编号表、§5 新指令、§8 栈效果、§10 版本表、§7.1 span_map 决定转正 |
| `docs/DESIGN.md` | §3 绑定规则补所有权；新增"所有权"章节（§3.1–3.3 细则）；§8 `?`/Ok/Err 转正；§10 box/deref 转正；新增"类型转换 `as`（0.0.2 临时，泛型后重审）"小节（§3.6 转正）与 print 仅 string 的临时性说明（§6）；§17 范围表更新；§18 已知 Hack 清空（print 已修）；路线表标注 0.0.2 完成 |
| `docs/SYNTAX.ebnf` | v0.0.2：前缀关键字、`?`、result_pattern、deref 赋值、cast 层（`as`）、分号改可选 |
| `SPEC.md` | §5/§6 阶段说明（ownership.rs）、§14 分号说明改"可选"、§14 运算符优先级表补 `as`、§15 特性表 0.0.2 列 |
| `CHANGELOG.md` | 0.0.2 条目（含 Breaking 小节：string 赋值、保留字、print(函数值)） |
| `README.md` | 特性一览更新 |

---

## 12. 验收标准（DoD）

- [ ] `cargo fmt -- --check`、`cargo clippy --all-targets -- -D warnings`、`cargo test`、`cargo doc --no-deps` 全绿
- [ ] 0.0.1 全部 E2E（含 `fib.fln` 显式分号版）在 0.0.2 下**逐字节**回归通过
- [ ] §8.2 新增 E2E 全部通过；v1 `.flnc` fixture 可执行
- [ ] `typeck` 无 `print` 特判；`infer.rs` HACK 注释移除
- [ ] print 严格 string 检查生效：`print(1)` 报 `ArgTypeMismatch`，`print(1 as string)` 通过
- [ ] `Value::Str(Rc<str>)` 在全仓不再出现（grep 为零）
- [ ] 运行时错误输出携带源码行列（`.fln` 模式）
- [ ] §2 全部决策点（D1–D8）在 DESIGN.md 中有定论
- [ ] `cargo fln` 单命令对新增特性开箱即用

---

> **0.0.2 一句话**：堆上的值开始有主人，错误开始流动，分号开始退休。
