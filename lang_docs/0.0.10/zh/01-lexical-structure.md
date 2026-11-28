# 01 — 词法结构

## 1.1 注释

```fleen
// 单行注释：本行 // 之后的内容被忽略

/* 多行
   注释 */
```

## 1.2 标识符

| 类别 | 风格 | 示例 |
|------|------|------|
| 变量、函数 | `snake_case` | `some_var`、`read_line` |
| 类型、struct | `PascalCase` | `Point`、`SomeType` |
| 模块 | 小写、点分 | `std.io` |

```fleen
user_name = "ada";
total_count = 3;
```

标识符是由字母、数字、下划线组成且不以数字开头、不与保留字（§1.3）
相同的序列。

## 1.3 保留字

以下单词不能用作标识符：

```
func  const  struct  enum  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
break  continue
box  ref  deref  move  clone  as  true  false
async  await  channel
ptr  addr  deref_ptr
```

说明：

- `return` 是保留字，但本语言没有 `return` 语句。
- `break` 和 `continue` 为 0.0.9 起保留，是循环语句（第 06 章 §6.5）——把它们
  用作标识符的程序必须改名。
- `unsafe` 和 `trusted` 为 0.0.5 起保留（见第 16 章）。
- `Future` **不是**保留字：只有在类型位置的 `Future<T>` 形式会被识别（第 04 章 §4.10），裸的 `Future` 标识符仍然可以是用户定义的 struct 名。
- `enum` 是 0.0.6 起的保留字（第 04 章 §4.4）。
- `ptr` **不是**保留字：只有在类型位置的 `ptr<T>` 形式会被识别（第 16 章）；`addr` 和 `deref_ptr` 是内建操作符/函数（第 16 章）。
- `feature` **不是**保留字：`#feature curry` 是 0.0.8 的头部指令，由头部位置的 `#` 识别，`feature` 本身可以继续用作标识符（第 07 章 §7.1）。
- 保留字可以作为模块路径段和成员名使用，例如 `import std.async;`。

## 1.4 字面量

### 整数字面量

整数字面量是数字序列，前面可以带 `-`：

```fleen
0
42
007
```

`int` 类型是 64 位有符号整数。负整数用一元负号书写（第 05 章 §5.4），
如 `0 - 1` 或 `-1`。

### 浮点字面量

浮点字面量带有小数部分：

```fleen
3.14
0.5
2.0
```

`float` 类型是 64 位浮点数。

### 布尔字面量

```fleen
true
false
```

### 字符串字面量

字符串字面量是双引号之间的文本：

```fleen
"hello"
"a b c"
""
```

字符串是 UTF-8 编码。字符串值是 **owned**（第 08 章）。字符串字面量是
单行构造：字面量内部出现裸换行（无论在字面块里还是 `${…}` 里）都是
字符串未终止错误。

#### 转义序列

| 转义 | 产生 |
|------|------|
| `\n` | 换行 |
| `\t` | 制表符 |
| `\r` | 回车 |
| `\\` | 反斜杠 |
| `\"` | 双引号 |
| `\'` | 单引号 |
| `\$` | 美元符（0.0.10 新增） |

其他任何 `\` 序列都是编译错误（`InvalidEscape`）。0.0.10 之前 `\$` 正是
这类错误；该转义随字符串插值（见下）一同加入。

#### 字符串插值（0.0.10）

含 `${` 的字符串字面量是**插值字符串**：字面块与嵌入表达式交错。
`${expr}` 嵌入一个完整表达式：

```fleen
func double(n: int): int = n * 2     // 供下方 ${…} 调用的函数

n = 42;
name = "world";
print("n=${n}!");                          // n=42!
print("row ${n + 8} of ${n}");             // 完整表达式
print("hello ${name}, len=${std.str.len(name)}");   // 调用也可以
print("nested: ${"inner=${n}"}");          // 嵌套插值字符串
print("d=${double(21)}");                  // 嵌入函数调用
```

规则：

- `${expr}` 嵌入**完整表达式**——调用、字段访问、算术、嵌套字符串字面量——
  按普通表达式文法解析。求值从左到右，每部分恰好求值一次，因此有副作用的
  嵌入表达式按源码顺序执行。
- **转换**：`int`、`float`、`bool` 走与 `as string` 完全相同的字符串化
  （第 05 章 §5.5）；`string` 原样嵌入。其他任何类型（`enum`、`struct`、
  `[T]`、`box<T>`、`Result<T, E>`、`Future<T>`、`channel<T>`、`ptr<T>`、
  函数值）都是编译错误（`InterpUnsupportedType`，附提示信息）。
  **没有格式说明符**：`${x:04}` 是解析错误。
- **读上下文**：嵌入的 `string` 会被深拷贝——插值永不移动变量，绑定在
  插值之后仍然可用（第 08 章）。
- **转义**：`$` **仅在紧邻 `{` 之前**才有特殊含义。其他位置都是普通字符：
  `"a$b"`、`"100$"`、`"$ { spaced }"` 与 0.0.9 相同。要让字面 `$` 直接出现在
  `{` 前，转义美元符——`"a\${b}c"` 的字面值是 `a${b}c`（转义吃掉 `$`，
  `{` 保持普通字符）。
- **降级**：插值字符串脱糖为对合并后字面块的 `std.str.concat` 调用链——
  无新操作码。**不含** `${` 的字面量与 0.0.9 逐字节相同。
- **全局变量**：全局初始化器暂不支持插值——`const s = "x${i}";` 会被拒绝
  （`ComplexGlobalInit`）。请在函数体内插值。
- 错误：`${…}` 内遇到 EOF 是 `UnclosedInterpolation`；游离的 `\` 是
  `InvalidEscape`。

**破坏性变更（0.0.10 的破坏性变更集）**：含 `${` 序列的字符串字面量此前
表示两个普通字符 `$` `{`，现在表示插值开始。旧源码中的 `${` 写成 `\${`
即可保留字面文本。

### 数组字面量

```fleen
[1, 2, 3]
["a", "b"]
[]
```

空数组字面量必须带类型标注，因为元素类型无法推断：

```fleen
empty: [int] = [];
```

### struct 字面量

```fleen
Point { x: 1, y: 2 }
```

所有字段都必须初始化。见第 04 章 §4.6。

## 1.5 源文件

- 源文件使用扩展名 `.fln`。
- **头部指令区（0.0.8）**：文件的第一个声明之前可以有零条或多条 `#feature` 指令，
  每条独占一行。`#` 不是注释——`#feature` 是唯一的 `#` 形式。注释和空行不会结束
  头部区，第一条声明（`import`、`struct`、绑定或函数）才结束它；此后再出现 `#`
  是 `FeatureDirectiveMisplaced`。指令重复写是 `DuplicateFeature`，未知特性名是
  `UnknownFeature`。特性标志是**逐文件**的，不是逐工程的（第 07 章 §7.1、第 12 章）。
- 文件由可选的 `import` 声明（第 12 章）、`struct` 声明（第 04 章 §4.6）、
  全局绑定（第 03 章）、`const` 绑定和函数声明（第 07 章）组成。
- 其中可以有一个名为 `main` 的函数；程序从这里开始执行。

```fleen
// program.fln
#feature curry        // 0.0.8：逐文件开启柯里化（见第 07 章）

import std.io;
import std.str;

greeting = "hello";             // 全局绑定

greet: string -> string
greet(name) = std.str.concat("Hello, ", name)   // Fleen 的字符串没有 `+`

main: () -> unit
main() {
    std.io.println(greet(greeting));
}
```
