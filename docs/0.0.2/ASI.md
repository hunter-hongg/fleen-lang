# ASI（自动分号插入）设计 — 0.0.2 / F5

> 本文是 0.0.2 分号可选（ASI）的**权威设计**。`DESIGN.md` §3.9 与
> `PLAN.md` §3.5 概述语义，实现细节以本文为准。
>
> **核心决策（2026-10-07 定稿）**：
> 1. **换行敏感**语义 —— 语句边界以换行为触发点判定，同行语句拼接非法；
> 2. **前置 pass** 架构 —— Lex 与 Parse 之间增加独立的 ASI pass，
>    pass 输出的 Token 流与 0.0.1 同构（分号齐全、无换行），Parse 阶段基本沿用原逻辑。

---

## 1. 架构

```text
源码 ──Lex──▶ Token 流（含 Newline）──ASI pass──▶ Token 流（分号齐全，无 Newline）──Parse──▶ AST
                                ▲
                     parser/asi.rs（纯函数，独立单测）
```

ASI pass 是 `Vec<Token> -> Result<Vec<Token>, ParseError>` 的纯函数：

- **消费**全部 `Newline` token；
- 在语句边界**插入**显式 `Semi` token（零宽 span，贴前一 token 末尾）；
- 对既不能续接也不能起始语句的 token 报 `ExpectedSemiOrNewStmt`。

pass 挂在 `parse()` 入口内部运行，`parse(tokens)` 的调用方无感；
直接调用 `tokenize()` 的外部代码会看到 `Newline` token（契约变化，见 §6）。

**为什么选前置 pass 而非 parser 内嵌判定**：

- 语义上换行敏感必须让 lexer 保留换行信息，token 流不再"纯粹"，
  与其让 parser 在收尾点判断，不如把判定集中在一个独立可测的 pass；
- pass 输出与 0.0.1 token 流同构，parser 的 `expect(Semi)` 主干逻辑保留，
  改动面缩小为四处配套（§4）；
- golden 式单测直观：带换行的 token 流进、插好分号的流出。

---

## 2. Token 契约

### Lexer（新增）

| 项 | 规定 |
|----|------|
| `TokenKind::Newline` | 新增变体，`Display` 为 `newline` |
| 发射时机 | `\n`（`\r\n` 视为一个换行） |
| 折叠 | 连续空白/空行只发**一个** `Newline` |
| 文件起始 | 源码开头的换行不发 |
| 注释 | 行注释末尾的换行**照发**（注释不断句）；块注释内的换行不发 |
| 字符串 | 字符串内裸换行仍为 `unterminated_string` 错误（现状不变） |
| EOF | 文件末尾不发 Newline（EOF 触发点由 pass 处理） |

### ASI pass（新增）

| 项 | 规定 |
|----|------|
| 输入 | `tokenize()` 输出（可含 `Newline`，末尾必有 `Eof`） |
| 输出 | 无 `Newline`；语句边界插入 `Semi`；其余 token 原样保序 |
| 插入 Semi 的 span | 零宽：`Span::new(prev.end, prev.end)` |
| 错误 | `ParseError { kind: ExpectedSemiOrNewStmt { found }, span }` |

---

## 3. 判定规则

### 3.1 触发点

每个 **Newline**（折叠后）与 **EOF**。设

- `p` = 换行前最后一个非 Newline token（pass 输出流的末尾），
- `n` = 换行后下一个 token（Eof 触发点时 `n = Eof`），
- 栈 = 未闭合括号栈（`(` `[` `{`，遇匹配闭括号弹出）。

按顺序判定：

| # | 条件 | 动作 |
|---|------|------|
| 1 | 栈顶是 `(` 或 `[` | 丢弃换行（表达式分组内无语句边界） |
| 2 | `n` 是 `}` | 丢弃换行（**永不在 `}` 前插**，见 §3.4） |
| 3 | `p` ∉ 可结尾集 | 丢弃换行（表达式未完，跨行续接） |
| 4 | `n` ∈ 隐式结束集 | **插入 Semi** |
| 5 | `n` ∈ 续接集 ∪ 构造延续集 ∪ {`;`, `{`} | 丢弃换行（续接优先） |
| 6 | 其余 | **报 `ExpectedSemiOrNewStmt`** |

### 3.2 集合定义

| 集合 | 成员 |
|------|------|
| **可结尾集**（p 侧） | `Ident` `IntLit` `FloatLit` `StringLit` `true` `false` `)` `]` `}` `?` |
| **起始集**（`can_start_stmt`） | `func` `const` `if` `while` `choose` `import`、`Ident`、字面量、`true`/`false`、`box` `deref` `move` `clone` `not` `!` `-` `(` `{`（块表达式语句） |
| **隐式结束集**（`implies_stmt_end`） | 起始集 **∖ {`(`, `-`, `{`** ∪ {`}`, EOF} |
| **续接集**（`can_continue_expr`） | `+` `-` `*` `/` `%` `==` `!=` `<` `>` `<=` `>=` `and` `or` `?` `.` `[` `(` `as`（0.0.2 U13） |
| **构造延续集** | `else` `elif` `when` `otherwise` |

说明：

- `-` 与 `(` 同属起始集和续接集：语句完成后**续接优先**——
  `x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`；`f()` ⏎ `(g())` ⇒ `f()(g())`。
  要开启新的语句请写 `;`。`!`/`not` 不在续接集，`x = 1` ⏎ `!flag` 是两条语句。
- `not` 与 `!` 是同一运算符的两种拼写，起始集同时收录两者。
- 隐式结束集剔除 `(`、`-` 和 `{`：前两者保证规则 4 与规则 5 不冲突；
  `{` 被剔除使 Allman 风格（`if c` ⏎ `{`）不断句，
  代价是 `{` 起始的块表达式语句需前导分号（§5 #4）。
- `?` 既在可结尾集又在续接集（后缀链 `f(x)??`）：
  `p = ?` 可结尾、`n = ?` 续接，两个角色互不干扰。
- 起始集用于两处：pass 的规则 4 插入判定（经隐式结束集间接使用）
  与 parser 的语句入口 guard（ASI.md §4 #4）。

### 3.3 `}` 的归属问题（规则 2 与构造延续集）

pass 是**文法盲**的，不知道一个 `}` 收掉的是完整语句还是半截结构：

- `if c { 1 }` ⏎ `else { 2 }` —— `else` 属于这个 if，不能断句；
- `choose x { when 0 { a }` ⏎ `when 1 { b } }` —— `when` 是 choose 臂延续，不能断句；
- `func f() { … }` ⏎ `x = 2` —— 这里 `}` 后**需要**分号。

解法：`}` 后只有当 `n` ∈ 隐式结束集（起始集 ∖ {`(` `-`}）才插分号；
`else`/`elif`/`when`/`otherwise` 天然不在其中，无需文法知识。

### 3.4 永不在 `}` 前插（规则 2）

`}` 前的最后一个条目是**尾表达式**还是**语句**，从平坦 token 流不可判定：

- `{ 42 }`、`if c { 1 }` 的臂 —— `42`/`1` 是尾表达式，插分号会**摧毁块值**；
- `{ x = 1 }` —— 绑定语句，不插分号则 parser 报错。

两者词法上无差别，pass 无法区分。解法：pass 永不在 `}` 前插，
parser 侧对**绑定/const 紧贴 `}` 免分号**（§4.1）。
绑定在任何版本都不能当尾表达式，因此该容忍不会吞掉任何尾表达式。

### 3.5 跨行续接（规则 3）

`p` 不能结尾语句时丢弃换行，以下跨行写法自然连上（无需显式标记）：

```fleen
x = 1 +          // p=+ 不能结尾
    2;
x =
    42;          // p== 不能结尾
move             // p=move 前缀关键字不能结尾
    x;
if c &&          // p=and 不能结尾
   d { 1 } else { 2 };
f(a,             // p=, 不能结尾（且在 ( 内，规则 1 亦丢弃）
  b);
```

---

## 4. Parse 侧配套改动

pass 输出"分号齐全"的流之后，parser 保留 0.0.1 主干，另有四处配套：

| # | 改动 | 位置 | 原因 |
|---|------|------|------|
| 1 | 绑定/const 紧贴 `}` 免分号 | `parse_var_binding` / `parse_const_decl` 的 `expect(Semi)` | §3.4 |
| 2 | 尾表达式判定排除 Assign 形态 | `parse_block` 尾表达式决策 | `Assign` 的 lower 是 `Dup+Store`（栈上残留 RHS 值）而 typeck 标 `Unit`；作语句时靠 pop 弹掉，作尾表达式则栈残留与类型错位。Assign（含 `deref b = v`、`x.y = v`）贴 `}` 一律按语句处理 |
| 3 | block-like 头续接爬升 | `parse_expr_or` | `parse_if`/`parse_while`/`parse_choose` 返回后不爬运算符；`if c { 1 }` ⏎ `- 2` ⇒ `(if-expr) - 2` 需要补后缀 + 二元爬升。普通表达式由既有贪心解析覆盖，无需处理 |
| 4 | 语句入口 guard | `parse_program` / `parse_block` 循环顶 | 当前 token 不能起始语句（`else`、杂散 `)`、`;` 等）→ `ExpectedSemiOrNewStmt`，替代原先落入 `parse_primary` 的 "expected expression" 泛化错误 |

另有一处**语义微调**：`when <pattern>` 与 guard 之间允许跨行
（`when Ok(v)` ⏎ `if guard { … }`）——pass 会在 pattern 后插入分号
（`If` ∈ 起始集），`parse_choose` 在 pattern 之后跳过该插入分号再判定 guard。
pattern 之后出现 `;` 永远是 pass 插入物（无合法源码形态），跳过无歧义。

**has_semi 语义**：pass 补齐分号后，表达式语句恒 `has_semi = true`；
唯一 `false` 的来源是改动 #2（Assign 贴 `}` 按语句处理）。
lower 的 pop 条件 `has_semi || !is_last_and_no_tail` 对两种取值均已正确，
MIR/字节码不受影响；字段保留（ticket 约定），后续版本可评估移除。

---

## 5. 语义决议与已知坑

| # | 决议 | 说明 |
|---|------|------|
| 1 | 换行敏感：同行语句拼接非法 | `x = 1 y = 2`（无分隔）报 `ExpectedSemiOrNewStmt`。漏写运算符的 typo 不再被静默吞成两条语句 |
| 2 | 跨行运算符续接优先 | `x = 1` ⏎ `- 2` ⇒ `x = (1 - 2)`（与 DESIGN §3.9 陷阱表一致） |
| 3 | 跨行 `else`/`elif` 归属 if | `if c { 1 }` ⏎ `else { 2 }` 是**一个** if-else 表达式（现行 parser 即如此，PLAN §3.5.2 旧表有误，已修正）；报错的是**已完结** if 之后再写 `else`：`if c { 1 }; else { 2 }` |
| 4 | `{` 开头的块表达式语句需前导分号 | `{` 不在隐式结束集 → 上一行不会被断句（Allman 风格 `if c` ⏎ `{` 因此安全）。块表达式作为独立语句跨行跟随时须写 `;`——JS 式已知坑，文档明示 |
| 5 | 标注不能单独成行 | `func f()` ⏎ `: int { …`、`const y` ⏎ `: int = 1` 在 `:` 处报错。格式限制，接受并记录 |
| 6 | 显式分号永远合法 | 新旧风格可混用；`;;` 在第二个 `;` 处报 `ExpectedSemiOrNewStmt`（0.0.1 亦报错） |

### 5.1 陷阱用例表（全部进测试）

| 代码 | 解析 | 判定路径 |
|------|------|---------|
| `x = 1` ⏎ `y = 2` | 两条语句 | 规则 4（`Ident` ∈ 起始集） |
| `x = 1` ⏎ `- 2` | `x = (1 - 2)` | 规则 5（`-` ∈ 续接集） |
| `f()` ⏎ `(g())` | `f()(g())` | 规则 5（`(` 续接） |
| `f()` ⏎ `[0]` | `f()[0]` | 规则 5（`[` 续接） |
| `x = 42` ⏎ `as string` | `x = (42 as string)` | 规则 5（`as` ∈ 续接集，U13） |
| `x = 1` ⏎ `!flag` | 两条语句 | 规则 4（`!` ∈ 起始集、∉ 续接集） |
| `x = 1` ⏎ `@` | 语法错误 | 规则 6 |
| `if c { 1 }` ⏎ `else { 2 }` | 一个 if-else | 规则 5（构造延续集） |
| `if c { 1 };` ⏎ `else { 2 }` | 语法错误 | 规则 3 丢弃 → parser 语句入口 guard |
| `choose x { when 0 { a }` ⏎ `when 1 { b } }` | 合法 | 规则 5（构造延续集） |
| `{ x = 1 }` / `{ const y = 2 }` | 合法（绑定语句） | §3.4 + parser 配套 #1 |
| `{ 42 }` / `{ f() }` | 尾表达式 | §3.4（不插）+ parser 尾判定 |
| `{ x = 1; }` | 语句，块值 unit | 显式分号 |
| `if c { 1 }` ⏎ `- 2` | `(if-expr) - 2` | 规则 5 + parser 配套 #3 |
| `when Ok(v)` ⏎ `if guard { … }` | 合法（guard 跨行） | §4 语义微调 |
| `x = 1 y = 2`（同行） | 语法错误 | 无触发点 → parser `expect(Semi)` 报错 |
| `x = 1;;` | 第二个 `;` 报错 | 规则 6 |
| Allman：`if c` ⏎ `{`、`func f()` ⏎ `{` | 合法 | 规则 5 中 `{` ∉ 隐式结束集 → 不插 |

---

## 6. 对外契约影响

| 面 | 影响 |
|----|------|
| `tokenize()` 公开 API | 输出新增 `Newline` token —— 契约变化，文档标注 |
| `parse()` 公开 API | 内部运行 pass，签名与行为对调用方透明 |
| 0.0.1 程序 | 全部带显式分号，pass 输出 = 输入（Newline 被消费），MIR/字节码逐字节不变 |
| AST / HIR / MIR | 不受影响；`has_semi` 语义见 §4 |
| `fleen-vm` / `fleen-verify` | 经 `parse()` 调用，无感 |

## 7. 测试计划

- **pass 单测**（`parser/asi.rs`）：三个谓词函数对 `TokenKind` 穷举判定表；
  golden 用例覆盖 §5.1 全表 + §3.5 跨行续接 + 括号栈（`f({ x = 1` ⏎ `y = 2 })`）。
- **parser 单测**：配套 #1–#4 各正反例；`else` 归属；`when` guard 跨行。
- **fixture**：`tests/parser/{valid,invalid}/asi_*.fln`；
  原 `missing_semicolon*.fln`（EOF 触发插分号后翻转为合法）迁移改写。
- **e2e**：`tests/e2e/valid/asi.fln` —— 全程无显式分号 + 显式分号混用，
  与 0.0.1 `fib.fln` 输出一致性回归。
