# Fleen 语言草案 v0.0.1（修订）

> **快、简、安、直觉**
> 编译运行都快 · 简洁清晰 · 内存安全 · 直觉友好

---

## 1. 定位

Fleen 是一门静态类型、编译到字节码的编程语言。语法学 Python，所有权学 Rust，设计哲学是**显式、简洁、无魔法**。

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

```fleen
x = 42              // 可变绑定
const y = 42        // 不可变绑定
x = 43              // 赋值，合法
y = 43              // 错误：y 不可变

// 类型标注
x: int = 42
const y: int = 42
```

**规则：**
- `=` 既是绑定也是赋值，默认可变
- `const` 标记不可变
- 允许遮蔽（内层可重新绑定同名变量）
- 无 `let`、无 `var`、无 `:=`

---

## 4. 函数

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
    print("Hello, ", name)
}

// 函数是一等值
func apply(f: (int, int) -> int, a: int, b: int): int = f(a, b)
result = apply(add, 1, 2)
```

**规则：**
- `func` 关键字定义函数
- 单行：`= expr`
- 多行：`{ block }`，最后表达式自动返回
- 函数是一等值，可赋值、传递、返回
- 无 `return` 关键字（0.0.1 暂不做提前返回）

---

## 5. 控制流

### if / elif / else

```fleen
if x > 0 {
    "positive"
} elif x < 0 {
    "negative"
} else {
    "zero"
}
```

`if` 是表达式，有返回值。

### while

```fleen
while x < 10 {
    x = x + 1
}
```

**0.0.1 只做 `while`，`for` 依赖迭代器，后做。**

---

## 6. 模式匹配：choose

```fleen
choose value {
    when 0 { "zero" }
    when 1 { "one" }
    when x if x > 10 { "big" }
    otherwise { "other" }
}
```

**规则：**
- 无 `fallthrough`
- `otherwise` 必须存在（或编译器能证明穷尽）
- 支持守卫条件 `if`
- 无 `match` / `switch`

---

## 7. 错误处理

```fleen
res = choose may_fail() {
    when Ok(value) { value }
    when Err(e) {
        log("Error: ", e)
        return -1
    }
}
```

**0.0.1 只做 `choose`，不做 `?` 语法糖。`?` 计划在 0.0.2。**

- `Result[T, E]` 是轻量代数类型
- 错误必须显式处理
- `panic` 仅用于不可恢复错误，不可捕获

---

## 8. 类型

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
Result[T, E]     // 错误处理
```

### 函数类型

```fleen
(int, int) -> int
```

---

## 9. 指针

```fleen
ptr<T>    // 裸指针，unsafe
box<T>    // 堆指针，安全，唯一所有权

addr x        // 取地址，安全
deref p       // 解引用 ptr，unsafe
deref b       // 解引用 box，安全
```

**0.0.1 不做。**

---

## 10. 模块与导入

```fleen
import std.io

std.io.print("hello")
```

**规则：**
- `import` 无头文件重复解析
- 模块名点分小写
- 无 C FFI 导入

---

## 11. unsafe / trusted

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

## 12. 命名风格

| 类别 | 风格 | 示例 |
|------|------|------|
| 变量/函数 | `snake_case` | `some_var` |
| 类型/结构 | `PascalCase` | `SomeType` |
| 模块 | `foo.bar` | `std.io` |

**不支持函数重载，支持运算符重载（后续版本）。**

---

## 13. 明确拒绝

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

## 14. 完整示例

```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0
    while x < 10 {
        print(fib(x))
        x = x + 1
    }
    0
}
```

---

## 15. 0.0.1 范围

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

## 16. 后续路线

| 版本 | 内容 |
|------|------|
| 0.0.2 | `move` / `clone` / `box<T>` / `ref` / `?` |
| 0.0.3 | `struct` / `for` / 迭代器 |
| 0.0.4 | `async` / `await` |
| 0.0.5 | `unsafe` / `trusted` |
| 0.1.0 | 泛型 / 运算符重载 |

---

## 17. 誓言

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
