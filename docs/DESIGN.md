# Fleen 语言草案 v0.0.1（修订）

> **快、简、安、直觉**
> 编译运行都快 · 简洁清晰 · 内存安全 · 直觉友好
>
> **0.0.2 增量**：所有权（`move` / `clone` / `box<T>` / `ref`）、`?` 与 `Ok`/`Err`、
> ASI（分号可选）已定稿 —— 决议与细则见 `docs/0.0.2/ASI.md`（权威），
> `docs/0.0.2/PLAN.md` §3.5 为摘要，
> 正文相应小节已同步标注（标注"0.0.2"的内容实现落地前不代表当前工具链行为）。

---

## 1. 定位

Fleen 是一门静态类型、编译到字节码的编程语言。语法学 Python，所有权学 Rust，设计哲学是**显式、简洁、无魔法**。

> **注意**：0.0.1 要求显式分号 `;`；0.0.2 起 ASI（自动分号插入）落地，分号可选
> （显式分号仍合法），判定规则见 §3.9。

- **文件后缀**：`.fln`
- **编译目标**：字节码 + VM
- **编程风格**：DOP + FP，拒绝 OOP
- **内存模型**：无 GC，所有权 + 借用

---

## 2. 词法

### 注释

```fleen
// 单行注释
/* 多行注释 */
```

### 标识符

| 类别 | 风格 | 示例 |
|------|------|------|
| 变量/函数 | `snake_case` | `some_var` |
| 类型/结构 | `PascalCase` | `SomeType` |
| 模块 | `foo.bar` | `std.io` |

### 字面量

```fleen
42           // int
3.14         // float
true / false // bool
"hello"      // string
```

### 关键字

```
func  const  if  elif  else  while  choose
when  otherwise  return  import  unsafe  trusted
box  ref  deref  move  clone  as  true  false
```

> `deref` 为 0.0.2 新增关键字（box 点内值读写，见 §10.3）；
> `as` 为 0.0.2 新增保留字（类型转换，见 §9"类型转换 as"）。

---

## 3. 变量与绑定

### 3.1 核心原则

**绑定和赋值是同一类操作。**

`=` 就是“让左边是右边”。编译器区分首次出现（绑定）和再次出现（赋值），但**用户不需要知道这个区别**。

```fleen
x = 42;        // 让 x 为 42
x = 43;        // 让 x 为 43
```

### 3.2 可变绑定

```fleen
x = 42;
```

- 默认可变
- 可重新赋值
- 可带类型标注：`x: int = 42;`

### 3.3 不可变绑定

```fleen
const y = 42;
```

- `const` 修饰名字
- 不可重新赋值
- 可带类型标注：`const y: int = 42;`

### 3.4 赋值

```fleen
x = 43;       // x 已存在且可变
```

- 对不可变变量赋值 → 编译错误
- 赋值不能带类型标注

### 3.5 规则汇总

| 操作 | 语法 | 条件 |
|------|------|------|
| 可变绑定 | `x = 42;` | 当前位置未绑定 `x` |
| 不可变绑定 | `const x = 42;` | 当前位置未绑定 `x` |
| 赋值 | `x = 43;` | 当前位置已绑定 `x` 且可变 |
| 循环体内赋值 | `while c { x = x + 1; }` | 循环体内已有外层 `x` → **赋值**，不是遮蔽（§4.5） |
| 类型标注 | `x: int = 42;` | 仅首次绑定 |
| 对 `const` 赋值 | `y = 43` | ❌ 编译错误 |
| 赋值带类型 | `x: int = 43` | ❌ 编译错误（已是赋值） |
| `const` 重新绑定 | `const x = 43` | ❌ 编译错误（同作用域已有 `x`） |
| 同名重复声明 | `func f` × 2、`func f(a, a)` | ❌ 编译错误（一个名字一个绑定） |

### 3.6 `=` 的三义性判定

`=` 有三种含义，编译器按以下顺序判定，**只有一条判定路径**：

```fleen
x = 42;    // ① 绑定：当前位置（当前作用域，或循环体内可见的外层）没有 x
x = 43;    // ② 赋值：当前位置已经有 x
x = 44;    // ② 赋值：同上，同一个绑定
```

| 位置 | 有无同名绑定 | 含义 |
|------|------------|------|
| 语句位置 | 当前作用域没有 | ① 绑定（若外层可见同名 → 遮蔽，见 §4.3） |
| 语句位置 | 当前作用域有 | ② 赋值（必须可变、不带类型标注） |
| 语句位置（循环体内） | 外层（至函数边界）有可变绑定 | ② **赋值**（不是遮蔽，见 §4.5） |
| 表达式位置 | 当前作用域有 | ② 赋值 |
| 表达式位置 | 没有 | ❌ 错误（表达式位置的 `=` **从不绑定**） |

**表达式位置的 `=` 只有赋值一种含义**，且目标查找**不跨越函数边界**：

```fleen
x = 1;
func f() {
    y = (x = 2);   // ❌ 错误：x 不在 f 的作用域链内，函数边界不可穿透
}
```

链式赋值同理，右侧目标必须已存在：

```fleen
b = 5;
a = b = 3;        // ✅ b 已绑定 → 赋值
a = b = 5;        // ❌ 错误：b 未绑定
```

> **关于 `x = x + 1`**：首次出现时报错（绑定前使用，§3.1）；已绑定时是正常赋值（②）。循环体内的 `x = x + 1` 是对**外层** `x` 的赋值，见 §4.5。

> **分号说明**：0.0.1 要求语句末尾显式写分号 `;`。0.0.2 起 ASI（自动分号插入）
> 落地，分号可省略（显式分号仍合法），判定规则见 §3.9。
>
> **表达式语句分号规则**：
> - if、while、choose 等控制流表达式作为语句使用时，0.0.1 必须加分号（0.0.2 起可省略）
> - choose-when 结构中，`when` 子句和 `otherwise` 子句本身不带分号
> - 只有 `choose` 整体作为表达式语句使用时，末尾才加分号
> - 函数体内的尾表达式（返回值）不加分号

### 3.7 错误信息

#### 对 `const` 赋值

```
error: cannot assign to immutable variable
  --> fib.fln:3:5
   |
 2 | const y = 42
   | ^^^^^ `y` is immutable
 3 | y = 43
   | ^ cannot assign
   |
help: use `y = 43` without `const` if you need mutability
```

#### 赋值带类型标注

```
error: type annotation not allowed on assignment
  --> fib.fln:3:5
   |
 2 | x = 42
 3 | x: int = 43
   | ^ type annotation is only allowed on first binding
   |
help: remove the type annotation: `x = 43`
```

### 3.8 所有权与绑定（0.0.2）

值分三类（完整模型见 §10.1）：

| 类别 | 类型 | 绑定/赋值/传参语义 |
|------|------|--------------------|
| Copy | `int` `float` `bool` `unit` 函数值 | 隐式按位复制，一切照旧（0.0.1 规则不变） |
| Owned | `string` `box<T>`、含 owned 分量的 `Result<T, E>` | 唯一所有者；转移需 `move`，复制需 `clone` |
| Borrow | `ref T`（仅函数参数位置，§10.4） | 只读借用，不转移 |

**绑定视角的核心规则：**

```fleen
s = "hello";       // 绑定 owned 值（RHS 是新值，无需关键字）
t = move s;        // 转移：s 之后不可再用
t = clone t;       // 深拷贝：原值仍可用
y = fib(n);        // 合法：函数返回值是新鲜值，天然转移
```

1. Copy 类型的使用**永远**不需要 `move` / `clone`；
2. owned 局部变量进入**消费位置**（绑定/赋值 RHS 顶层、实参、`choose` scrutinee）
   必须显式 `move` / `clone`，裸写是编译错误（help 提示补关键字）；
3. **产出位置隐式转移**：函数尾、块尾、if/choose 分支尾的值顺"出口"流走；
   分支尾的转移是条件性转移，之后使用报"可能已移动"；
4. 比较运算与 `print` 实参是**只读使用**：自动复制，不转移；
5. 禁止 `move deref b`（移出 box）、`move g`（移出全局——owned 全局读取即深拷贝、
   写入即替换）、`move s`（s 为 `ref` 参数）。

所有权检查在 typeck 内做流敏感分析（`typeck/ownership.rs`，0.0.2 新增），
以函数为单位、不穿越函数边界。

### 3.9 分号与 ASI（0.0.2）

0.0.2 起分号可选，语义为**换行敏感**：语句边界在换行处判定，同行语句拼接非法。
架构上，Lex 与 Parse 之间增加独立的 **ASI pass**（`parser/asi.rs` 纯函数）：
消费全部 `Newline` token，在语句边界插入显式 `;`，对既不能续接也不能起始
语句的 token 报错 `ExpectedSemiOrNewStmt`。pass 输出的 Token 流与 0.0.1
同构（分号齐全），Parse 阶段沿用原有逻辑。

> 语句完成后（换行或 EOF 处）——
> 1. 下一 token ∈ **隐式结束集** → 插入 `;`（断句）；
> 2. 下一 token ∈ **续接集** 或构造延续集（`else` `elif` `when` `otherwise`）→ 不断句，续接优先；
> 3. 上一 token 不能结尾语句（运算符、`=`、`,`、前缀关键字之后）→ 不断句，跨行续接；
> 4. 其余 → 报错 `ExpectedSemiOrNewStmt`（带 Span 与期望提示）。

| 集合 | 成员 |
|------|------|
| 新语句起始集 | `func` `const` `if` `while` `choose` `import`、标识符、字面量、`box` `deref` `move` `clone` `not` `!` `-` `(` `{` |
| 续接集 | `+` `-` `*` `/` `%` `==` `!=` `<` `>` `<=` `>=` `and` `or` `?` `.` `[` `(` |
| 终止集 | `}` EOF |

> 隐式结束集 = 起始集 ∖ {`(`, `-`, `{`} ∪ {`}`, EOF}：`(` `-` 续接优先，
> `{` 的豁免使 Allman 风格不断句（代价：块表达式语句需前导分号）。

**陷阱用例**（必须进规范与测试）：

| 代码 | 解析结果 | 说明 |
|------|---------|------|
| `x = 1` ⏎ `y = 2` | 两条语句 | 标识符 ∈ 起始集 |
| `x = 1 y = 2`（同行） | 语法错误 | 换行敏感：同行拼接非法，typo 不被静默吞掉 |
| `x = 1` ⏎ `- 2` | `x = (1 - 2)` | `-` ∈ 续接集，二元续接优先 |
| `f()` ⏎ `(g())` | `f()(g())` 调用链 | `(` ∈ 续接集（调用）；想分开请写 `;` |
| `f()` ⏎ `[0]` | `f()[0]` 索引 | `[` ∈ 续接集 |
| `x = 1` ⏎ `!flag` | 两条语句 | `!` 非二元续接，`!flag` ∈ 起始集 |
| `if c { 1 }` ⏎ `else { 2 }` | 一个 if-else 表达式 | `else` 归属该 if（跨行连接合法） |
| `if c { 1 };` ⏎ `else { 2 }` | 语法错误 | `else` 不续接**已完结**（有分号）的 if 语句 |
| `{ x = 1 }` | 合法（绑定语句） | `}` 前免分号，由 parser 结构区分尾表达式与绑定 |
| `x = 1` ⏎ `{ print(1) }` | 语法错误 | 以 `{` 开头的块表达式语句需前导分号（`{` 不在隐式结束集，亦是 Allman 风格安全的代价） |

不变量：`func` 声明后无分号、块尾表达式无分号、`when`/`otherwise` 无分号的规则不变；
分号的有无在 AST 之后不可见（不影响 MIR 与字节码）。
架构、集合精确定义与完整陷阱表见 **`docs/0.0.2/ASI.md`**（权威设计）。

---

## 4. 作用域与遮蔽

### 4.1 块作用域

每个 `{}` 是一个**新作用域**。

```fleen
if cond {
    x = 42;      // 块内绑定
};
print(x);        // 错误：x 不在作用域内
```

### 4.2 函数作用域

函数体是**独立作用域**。

```fleen
x = 42;
func foo() {
    x = 43;      // 新绑定，与外部 x 无关
}
```

### 4.3 遮蔽

#### 允许遮蔽

内层可以重新绑定同名变量。

```fleen
x = 42;
if cond {
    x = 43;      // 新绑定，遮蔽外层 x
};
print(x);        // 42（外层 x 未被修改）
```

#### 可变性必须一致

**遮蔽时，内层和外层的可变性必须相同。**

| 外层 | 内层 | 允许？ |
|------|------|--------|
| 可变 | 可变 | ✅ |
| `const` | `const` | ✅ |
| 可变 | `const` | ❌ |
| `const` | 可变 | ❌ |

```fleen
// 允许
x = 42;
if cond {
    x = 43;          // 可变遮蔽可变
};

const x = 42;
if cond {
    const x = 43;    // 不可变遮蔽不可变
};

// 不允许
x = 42;
if cond {
    const x = 43;    // 错误：可变性不一致
};

const x = 42;
if cond {
    x = 43;          // 错误：可变性不一致
};
```

#### 遮蔽的语义

**遮蔽是“同类型替换”，不是“新变量随便定义”。**

- 内层 `x = 43` 是**新绑定**，不影响外层
- 外层 `x` 仍然存在，但在内层不可见
- 退出内层后，外层 `x` 恢复可见

#### 函数不生效

函数体是独立作用域，**不受外层用户变量的遮蔽规则约束**。函数边界**不穿透**：函数内的绑定不与函数外的同名用户变量做遮蔽对比。

```fleen
x = 42;
func foo() {
    const x = 43;    // 合法：函数内是新作用域，不是遮蔽
}
```

#### 参数可被遮蔽（函数内例外）

函数参数是函数体内的绑定，**不受"函数不生效"保护**。函数参数可以在函数体内用同名的新绑定遮蔽，**不做可变性一致性检查**（参数不是外层用户声明）：

```fleen
func f(x: int): int {
    x = 2;      // 合法：函数体内是对参数的遮蔽，不是赋值
    x
}
```

#### 遮蔽的例外：内建函数名

内建函数（如 `print`）的同名用户绑定不算冲突，直接覆盖：用户可以用 `print = 42;` 声明自己的 `print`，之后该名字指向用户绑定。

---

### 4.5 循环体内的赋值规则

`while`（及未来的 `for`）的循环体是**循环作用域**。循环体内（含其嵌套块）出现 `x = expr` 时：

- 若当前作用域（循环体本身）已有 `x` → ② 赋值（和平常一样）
- 否则，若**当前函数作用域内、当前位置之前已可见**的 `x` 存在，且它是**可变的用户变量** → ② **赋值**，而不是遮蔽
- 否则 → ① 绑定（新遮蔽或首次绑定）

这使计数器惯用法按预期工作：

```fleen
x = 0;
while x < 10 {
    x = x + 1;    // 对外层的 x 进行赋值，不是遮蔽
};
```

反之，若外层 `x` 不可变（`const` 或参数），循环体内的 `x = expr` 是**编译错误**（赋值给不可变绑定）：

```fleen
const x = 0;
while x < 10 {
    x = 1;      // ❌ 编译错误：不能赋值给 const
};
```

循环作用域的查找不会穿透函数边界：函数体内的循环不能赋值给全局或同名的外层绑定。若函数内没有对应的局部绑定，`x = 1` 就是对**循环作用域内的新绑定**（遮蔽语义），不是对外层变量的赋值：

```fleen
x = 0;          // 全局 x
func f() {
    while true {
        x = 1;  // 新绑定（循环体内），不写全局 x
    };
}
```

表达式位置的 `=` 在函数内**不能**绑定全局：`y = (x = 1);` 中若 `x` 未在函数内声明，是编译错误（§3.6）。

> 循环体内的遮蔽仍然可用，但**只能在循环体的嵌套块中**发生。外层 `const` 时，`while c { const x = ... }` 是可变性不一致的遮蔽错误（§4.3）。

### 4.4 遮蔽错误信息

#### 可变性不一致的遮蔽

```
error: shadowing must preserve mutability
  --> fib.fln:3:5
   |
 2 | x = 42
   | ^ outer `x` is mutable
 3 |     const x = 43
   |     ^^^^^ inner `const` does not match outer mutability
   |
help: use `x = 43` for a mutable shadow, or rename the variable
```

---

## 5. 函数

```fleen
// 单行函数
func add(a: int, b: int): int = a + b

// 多行函数
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

**规则：**
- `func` 关键字定义函数
- 单行：`= expr`
- 多行：`{ block }`，最后表达式自动返回
- 函数是一等值，可赋值、传递、返回
- 无 `return` 关键字（0.0.1 暂不做提前返回）

---

## 6. 控制流

### if / elif / else

```fleen
if x > 0 {
    "positive"
} elif x < 0 {
    "negative"
} else {
    "zero"
};
```

`if` 是表达式，有返回值。**注意：0.0.1 要求表达式语句末尾加分号（0.0.2 起 ASI 使其可选，§3.9）。**

### while

```fleen
while x < 10 {
    x = x + 1;
};
```

**注意：0.0.1 要求表达式语句末尾加分号（0.0.2 起 ASI 使其可选，§3.9）。**

**0.0.1 只做 `while`，`for` 依赖迭代器，后做。**

---

## 7. 模式匹配：choose

```fleen
choose value {
    when 0 { "zero" }
    when 1 { "one" }
    when x if x > 10 { "big" }
    otherwise { "other" }
};
```

**规则：**
- 无 `fallthrough`
- `otherwise` 必须存在（或编译器能证明穷尽）
- 支持守卫条件 `if`
- `when` pattern 支持字面量（含负数）、标识符绑定
- 无 `match` / `switch`
- **分号规则**：`when` 子句和 `otherwise` 子句本身不带分号，只有 `choose` 整体作为语句使用时末尾加分号

**pattern 语法（P5 补充）：**
```text
pattern = neg_pattern | literal | ident
neg_pattern = "-" , (int_lit | float_lit)
```

---

## 8. 错误处理

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") }
    else { Ok(a / b) }
}

func ratio(a: int, b: int): Result<int, string> {
    q = div(a, b)?;      // Err 则整函数立即返回该 Err
    r = div(q, 2)?;
    Ok(r)
}

res = choose ratio(10, 2) {
    when Ok(v) { v }
    when Err(e) {
        print("error: ", e);
        0 - 1
    }
};
```

> **版本说明**：0.0.1 无 `Ok`/`Err` 构造语法与 pattern（`Result<T, E>` 仅有类型，
> 须用空值绑定 + otherwise 显式处理）；上例语法与 `?` 自 **0.0.2** 起转正
> （决议见 `docs/0.0.2/PLAN.md` §3.4）。
>
> **词法定性**（0.0.2）：`move` / `clone` / `deref` 是**关键字**
> （`is_keyword()` 返回 true，从合法标识符变为保留字，"保留字用作绑定名"
> 由 parser 在期望标识符处报错）；`?` 是**标点 token**（`TokenKind::Question`，
> `is_keyword()` 返回 false），不参与保留字判定。

**0.0.2 语义：**

- `Ok(expr)` / `Err(expr)` 是内建构造表达式（可被用户遮蔽，机制同 `print`），
  类型由**上下文**确定（函数返回类型标注、绑定类型标注、`?` 所在函数返回类型、
  choose scrutinee）；无上下文 → 编译错误，提示补类型标注
- `choose` 支持 `when Ok(v)` / `when Err(e)` pattern，绑定按**转移**语义；
  `Ok` + `Err` 两臂齐即穷尽，无需 `otherwise`；带 guard 的臂不计入穷尽
- `?` 后缀运算符：操作数类型为 `Result<T, E>`，所在函数返回类型须为
  `Result<T2, E>`（`E` 完全相等，0.0.2 无错误类型转换）；`Err` 时整函数立即返回该 `Err`
- 表达式语句的值类型为 `Result<T, E>` → 编译错误（`UnhandledResult`）：
  Result 不允许静默丢弃，提示绑定或加 `?`
- `main` 可返回 `Result<T, E>`：VM 收到 `Err` 时打印 stderr 并以退出码 1 结束
- 无 `return` 关键字（§5）：`?` 是唯一的提前返回机制
- `Result<T, E>` 是轻量代数类型（尖括号，与 EBNF 一致）；错误必须显式处理；
  `panic` 仅用于不可恢复错误，不可捕获

---

## 9. 类型

### 基本类型

```fleen
int
float
bool
string
unit
```

### 复合类型

```fleen
[T]              // 数组（0.0.3）
box<T>           // 堆指针，唯一所有权；box expr 分配、deref 读写（0.0.2，§10.3）
ref T            // 只读借用，仅函数参数（0.0.2，§10.4）
Result<T, E>     // 错误处理（0.0.2 起 Ok/Err/? 转正，§8）
```

### 函数类型

```fleen
(int, int) -> int
```

### 类型转换 `as`（0.0.2 临时，泛型后重审）

```fleen
x = 42 as string;    // "42"
f = 1.5 as string;   // "1.5"
b = true as string;  // "true"
```

- **优先级**：`as` 介于一元前缀与二元运算符之间（同 Rust）：
  `deref b as string` = `(deref b) as string`；`1 + 2 as string` = `1 + (2 as string)`；
  `box 1 as string` = `box (1 as string)`
- **白名单（仅此三条）**：`int as string`（十进制，含负号）、
  `float as string`（与 print 的 float 输出一致）、`bool as string`（`"true"` / `"false"`）。
  其余一切（`string as string`、`int as float`、`box<T> as …`、`ref T` 操作数、恒等转换）
  → `UnsupportedCast { from, to }`
- **结果是新鲜 owned string**：消费位置无需 `move` / `clone`（与"RHS 是新值"一致）
- **临时性**：0.0.2 白名单仅为 print 修复的最小配套；完整转换矩阵与
  From-like 机制随泛型（0.1.0）再议，0.0.2 不引入任何隐式数值转换
- **print 仅接受 string 也是临时的**：0.0.2 print 对非 string 实参报
  `ArgTypeMismatch` 并提示 `as string`；print 的格式化与 `as string` 共用
  同一 helper（`fleen-vm/src/fmt.rs`），两条路径不会漂移

---

## 10. 指针与所有权

> **版本说明**：0.0.1 不做；`box<T>` / `deref` / `ref` / `move` / `clone` 自 **0.0.2** 起
> 转正（决议与细则见 `docs/0.0.2/PLAN.md` §3.1–3.3）。
> `ptr` / `addr` / `unsafe` 仍不做（0.0.5）。

```fleen
box<T>    // 堆指针，安全，唯一所有权（0.0.2）
ref T     // 只读借用，仅函数参数（0.0.2）
ptr<T>    // 裸指针，unsafe（0.0.5）
addr x    // 取地址（0.0.5）
```

### 10.1 所有权模型

值分三类（`is_copy(T)` 由 typeck 判定）：

| 类别 | 类型 | 赋值/传参语义 |
|------|------|---------------|
| Copy | `int` `float` `bool` `unit` 函数值 | 隐式按位复制，一切照旧 |
| Owned | `string` `box<T>`、含 owned 分量的 `Result<T, E>` | 唯一所有者；转移需 `move`，复制需 `clone` |
| Borrow | `ref T`（仅参数位置） | 只读借用，不转移 |

### 10.2 move / clone

| 表达式 | 效果 |
|--------|------|
| `move x` | 转移 `x` 的所有权，此后 `x` 不可再用（`UseAfterMove`） |
| `clone x` | 深拷贝，`x` 仍可用 |
| `clone deref b` | 深拷贝 box 点内值，box 不动 |
| `clone g`（g 为全局） | 深拷贝（owned 全局的读取语义即深拷贝，见规则 5） |

**核心规则：**

1. Copy 类型永远不需要 `move` / `clone`
2. owned 局部变量进入**消费位置**（绑定/赋值 RHS 顶层、实参、`choose` scrutinee）
   必须显式 `move` / `clone`；裸写是编译错误（help 提示补关键字）
3. **产出位置隐式转移**：函数尾、块尾、if/choose 分支尾的值顺"出口"流走，
   无需关键字；分支尾的转移是条件性转移，之后使用报"可能已移动"
   > **语句位置的块尾/分支尾**（如 `{ … t };` 或 `if c { … t } else { … u };`
   > 单独成句）同样产出值，随后立即丢弃。这是保守语义（与 Rust 的表达式
   > 语句一致）：值照常生成、转移规则不变，只是无人接收；不因为"在语句
   > 位置"就免除非 Copy 消费
4. 比较运算与 `print` 实参是**只读使用**：自动复制，不转移
5. 禁止：`move deref b`（不可移出 box）、`move g`（不可移出全局——owned 全局
   读取即深拷贝、写入即替换）、`move s`（s 为 `ref` 参数）
6. `move` / `clone` 作用于 Copy 类型 → 编译错误，提示去掉关键字

```fleen
s = "hello";
t = move s;        // 转移，s 之后不可用
t = clone t;       // 深拷贝，t 仍可用
```

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

### 10.3 box<T>

```fleen
b = box 42;            // b: box<int>，堆分配
n = deref b;           // 读：副本（owned 点内值为深拷贝）
deref b = n + 1;       // 写：替换点内值，旧值释放
c = clone b;           // 新 box + 新点内值
s = box "heap";
t = clone deref s;     // 深拷贝点内值，box 本身不动
```

- `box expr` 前缀关键字分配（类型位置 `box<T>` 与表达式位置按语法位置区分）；
  `deref` 前缀关键字读写；赋值目标可为 `deref b`（不可带类型标注）
- `deref b` 读取产生点内值的**副本**（`T: Copy` 按位复制，owned 深拷贝）——
  box 拥有点内值，读取不等于取走所有权；"显式 clone"规则只约束变量位置
- 点内类型允许集合（0.0.2）：`int` `float` `bool` `string` `box<U>` `Result<T, E>`
- **不可移出 box**：`move deref b` 编译错误，需要值请 `clone deref b`
- **`deref` 赋值目标**：`deref b = v` 的 `b` 必须是**局部 box 变量**；
  对 owned 全局 box 写点内值是编译错误（0.0.2 无全局可变别名安全路径，
  `DerefAssignOfGlobalBox`），可先把 box `clone` 到局部再写
- **确定性释放（无 GC）**：持有者离开作用域、被重新赋值、值被丢弃时立即释放；
  无循环引用可能（box 唯一所有权、`ref` 不可存储）→ 不需要 GC，与 §14 一致

### 10.4 ref：只读借用（仅函数参数）

```fleen
func shout(s: ref string): int {
    print(s);          // 只读使用
    42
}

func main(): int {
    name = "fleen";
    shout(name);       // 传借用，name 仍可用
    shout(name);       // 再借一次，合法
    0
}
```

- `ref T` 只能出现在**函数参数类型**位置；`ref` 局部变量、`ref` 全局、
  返回 `ref`、嵌套 `ref ref T` 一律编译错误（`RefNotAllowedHere`）
- ref 参数只读：赋值（`AssignToRefParam`）、`move`（`MoveOfBorrowed`）均错误；
  `clone s` 得到 owned `T`；可作为 `ref T` 实参转发（句柄流动，零拷贝）
- 实参**不转移**：传局部/全局/另一 ref 参数均可，不需要 `move`/`clone`；
  不支持借用 box 内部（`deref b` 作 ref 实参）
  > 实参必须是**变量**（局部 / 全局 / 另一 ref 参数）。字面量、`clone s`、
  > 调用结果等临时值报 `RefArgNotAVariable`：借用没有可引用的槽位，
  >  borrowed 值在调用返回后无处可寻
- `ref T` 是独立类型（`ref string ≠ string`）：把 `ref string` 赋给 `string`
  绑定是类型错误，提示 `clone s`；传 `move x` 给 `ref` 参数报错（借用不接受转移）
- **无需借用检查器**：句柄只存活于一次调用——被借槽位调用期间不被改写
  （调用方挂起）、句柄无法逃逸（不能赋值/存储/返回）→ 没有悬垂的可能

### 10.5 0.0.2 U06 实现说明（lower 端限制）

- **result `choose` 的守卫**：带 `if <guard>` 的臂**不计入穷尽性**——与 Bool
  scrutinee 同一条规则：guard 运行时可能为假，无法覆盖其模式。因此
  `when Ok(v) if g {…}` + `when Err(e) {…}` 被 typeck 判为缺少 Ok 臂而拒绝
  （`ChooseNotExhaustive { missing_patterns: ["Ok"] }`）。要写 guard，必须另配
  一个**无 guard 的重复** `Ok` / `Err` 臂，或用 `otherwise` 兜底。守卫臂与
  `otherwise` 的**降载延后到 U09 / 独立票**（U07 决定维持延后，见
  `docs/0.0.2/TICKETS.md` U07「范围决定」）：需要把 Result-choose 从
  "Ok/Err 双臂、无兜底块"重构成通用臂链 + no-match 兜底，并在 guard 求值前
  保留 payload。在此之前 U06/U07 的 lower 只降载无守卫的 `when Ok(..)` /
  `when Err(..)` 臂，遇到 guard 或 `otherwise` 报 `UnsupportedFeature`
  （`ChooseResultNeedsOkErrArms` 是 typeck 门后的防御性检查）。
- **ref 实参为全局时**：借用必须指向局部槽位。U06 在为**函数体内**调用传全局
  ref 实参时，用临时槽 `CloneGlobal; StoreLocal tmp; MakeRefLocal tmp` 承接；
  **全局初始化器**（`lower_global_init`）没有可借用其槽位的帧，故含 ref 实参
  的全局初始化调用直接报 `RefArgInGlobalInit`（typeck 通过、lower 拒绝）。

---

## 11. 模块与导入

```fleen
import std.io;

std.io.print("hello");
```

**规则：**
- `import` 无头文件重复解析
- 模块名点分小写
- 无 C FFI 导入

---

## 12. unsafe / trusted

```fleen
unsafe raw_op = () => {
    // 不安全操作
}

trusted safe_wrapper = () => {
    // 作者确认安全
}

func normal() {
    safe_wrapper()      // 可以调用 trusted
    // raw_op()         // 错误：不能调用 unsafe
}
```

**0.0.1 不做（0.0.5）。**

---

## 13. 命名风格

| 类别 | 风格 | 示例 |
|------|------|------|
| 变量/函数 | `snake_case` | `some_var` |
| 类型/结构 | `PascalCase` | `SomeType` |
| 模块 | `foo.bar` | `std.io` |

**不支持函数重载，支持运算符重载（后续版本）。**

**分号规则**：0.0.1 需显式写分号 `;`；0.0.2 起 ASI 落地，分号可选（显式仍合法，见 §3.9）。

---

## 14. 明确拒绝

| 特性 | 原因 |
|------|------|
| 垃圾回收 | 不可预测延迟 |
| `panic` 捕获 | 不应作为控制流 |
| `match` / `switch` | 符号繁复 / fallthrough 反直觉 |
| `try` / `catch` / `finally` | 异常跳转破坏线性心流 |
| `goto` | 破坏结构化编程 |
| 全局可变变量 | 并发数据竞争 |
| OOP | 拒绝 `class`、继承 |
| `let` / `var` / `:=` | 绑定统一用 `=` |
| **C FFI 导入** | 保持语言纯粹，不引入外部不安全 |

---

## 15. 完整示例

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
    };

    0
}
```

---

## 16. 设计理由

### 16.1 为什么绑定和赋值是同一操作

**人的意愿就是“让 x 为 42”“让 x 为 43”。**

用户不需要区分“定义”和“赋值”。编译器知道区别，但用户不感知。

这符合**直觉友好**原则。

### 16.2 为什么默认可变

Python 风格：变量是“名字到值的引用”，名字本身没有“不可变”属性。

可变是默认，`const` 是显式标记。

这符合**简洁清晰**原则。

### 16.3 为什么遮蔽必须可变性一致

**避免歧义。**

```fleen
x = 42;
if cond {
    const x = 43;    // 内层不可变，外层可变
};
x = 44;              // 改的是哪个？
```

如果允许，读者会问：内层 `const x` 是“新变量”，还是“把外层 `x` 变成不可变”？

**没有清晰答案。所以不允许。**

这符合**内存安全**和**直觉友好**原则。

### 16.4 为什么函数不生效

**函数体是独立作用域，不是“内层”。**

函数内的变量和外层变量无关。函数不应该受外层变量的可变性影响。

这符合**简洁清晰**原则。

---

## 17. 特性范围

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

**0.0.1 目标：Fibonacci 能跑。**

**0.0.2 目标：所有权起步 —— 规划见 `docs/0.0.2/PLAN.md`。**

---

## 18. 后续路线

| 版本 | 内容 |
|------|------|
| 0.0.2 | `move` / `clone` / `box<T>` / `ref` / `?` / ASI —— **已定稿，规划见 `docs/0.0.2/PLAN.md`** |
| 0.0.3 | `struct` / `for` / 迭代器 |
| 0.0.4 | `async` / `await` |
| 0.0.5 | `unsafe` / `trusted` |
| 0.1.0 | 泛型 / 运算符重载 |

### 已知 Hack（0.0.2 修复方案已定）

- **`print` 类型检查被放宽**（0.0.1 现状）：typeck 中 `print` 的签名仍是
  `(string) -> unit`，但 `typeck_call` 对 `print` 特判，跳过参数类型检查
  （任意类型、任意元数），`check_builtin_print_wrong_arg` 测试暂期望编译通过。
  0.0.2 修复方案已定稿（`docs/0.0.2/PLAN.md` §6）：内建签名表 +
  `printable` 集合（int/float/bool/string/unit），恢复严格检查；
  实现落地后本节清空。

---

## 19. 誓言

> 信任程序员，
> 但不考验程序员的记忆。
> 少一个皱眉的符号，
> 多一份直觉的简洁。
> 不追求全能，
> 但追求：
> 用最简洁语法，
> 表达最安全语义，
> 生成最高效代码。
> Fleen 不是任何语言的简化版，
> Fleen 就是 Fleen，
> 一门，为未来而生的语言。
