# Fleen 语言草案 v0.0.1（修订）

> **快、简、安、直觉**
> 编译运行都快 · 简洁清晰 · 内存安全 · 直觉友好

---

## 1. 定位

Fleen 是一门静态类型、编译到字节码的编程语言。语法学 Python，所有权学 Rust，设计哲学是**显式、简洁、无魔法**。

> **注意**：0.0.1 版本要求显式分号 `;`，ASI（自动分号插入）计划在后续版本实现。

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
box  ref  move  clone  true  false
```

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

> **分号说明**：0.0.1 要求语句末尾显式写分号 `;`。ASI（自动分号插入）计划在后续版本实现，届时分号可省略。
>
> **表达式语句分号规则**：
> - if、while、choose 等控制流表达式作为语句使用时，必须加分号
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

`if` 是表达式，有返回值。**注意：0.0.1 要求表达式语句末尾加分号。**

### while

```fleen
while x < 10 {
    x = x + 1;
};
```

**注意：0.0.1 要求表达式语句末尾加分号。**

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
// 注意：0.0.1 暂不支持 `Ok`/`Err` pattern，以下为后续版本语法；
// 0.0.1 中必须用空值绑定 + otherwise 分支显式处理。
res = choose may_fail() {
    when Ok(value) { value }
    when Err(e) {
        log("Error: ", e)
        return -1
    }
}
```

**0.0.1 只做 `choose`，不做 `?` 语法糖。`?` 计划在 0.0.2。**

- `Result<T, E>` 是轻量代数类型（P4: 使用尖括号，与 EBNF 保持一致）
- 错误必须显式处理
- `panic` 仅用于不可恢复错误，不可捕获

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
[T]              // 数组
box<T>           // 堆指针
ref T            // 只读借用（仅函数参数）
Result<T, E>     // 错误处理
```

### 函数类型

```fleen
(int, int) -> int
```

---

## 10. 指针

```fleen
ptr<T>    // 裸指针，unsafe
box<T>    // 堆指针，安全，唯一所有权

addr x        // 取地址，安全
deref p       // 解引用 ptr，unsafe
deref b       // 解引用 box，安全
```

**0.0.1 不做。**

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

**0.0.1 不做。**

---

## 13. 命名风格

| 类别 | 风格 | 示例 |
|------|------|------|
| 变量/函数 | `snake_case` | `some_var` |
| 类型/结构 | `PascalCase` | `SomeType` |
| 模块 | `foo.bar` | `std.io` |

**不支持函数重载，支持运算符重载（后续版本）。**

**0.0.1 分号规则**：语句末尾需显式写分号 `;`。ASI 在后续版本实现。

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

## 17. 0.0.1 范围

| 特性 | 0.0.1 |
|------|-------|
| `=` 绑定/赋值 | ✅ |
| `const` 不可变 | ✅ |
| `int` / `bool` / `string` / `float` | ✅ |
| `func` 单行 / 多行 | ✅ |
| `if` / `elif` / `else` | ✅ |
| `while` | ✅ |
| `choose` | ✅ |
| `Result` | ✅ |
| 函数类型 | ✅ |
| 分号 `;` | ✅ |
| `?` | ❌ |
| `move` / `clone` | ❌ |
| `box<T>` | ❌ |
| `ref` | ❌ |
| `struct` | ❌ |
| `for` | ❌ |
| `async` | ❌ |
| FFI | ❌ |
| 泛型 | ❌ |
| 运算符重载 | ❌ |

**0.0.1 目标：Fibonacci 能跑。**

---

## 18. 后续路线

| 版本 | 内容 |
|------|------|
| 0.0.2 | `move` / `clone` / `box<T>` / `ref` / `?` / ASI |
| 0.0.3 | `struct` / `for` / 迭代器 |
| 0.0.4 | `async` / `await` |
| 0.0.5 | `unsafe` / `trusted` |
| 0.1.0 | 泛型 / 运算符重载 |

### 已知 Hack（必须在 0.0.2 修复）

- **`print` 类型检查被放宽**：typeck 中 `print` 的签名仍是 `(string) -> unit`，但
  `typeck_call` 对 `print` 特判，跳过参数类型检查（任意类型、任意元数）。动机：
  DESIGN.md 的 Fibonacci 示例需要 `print(fib(x))`（int），而 typeck 原定义为 string。
  正确做法（0.0.2）：内建函数支持可变参数/多态签名（如按类型分派的 trait 或
  `any` 参数转换），恢复严格检查，并把 `check_builtin_print_wrong_arg` 测试改回
  期望 `ArgTypeMismatch`。

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
