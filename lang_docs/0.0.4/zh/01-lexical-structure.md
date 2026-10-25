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
func  const  struct  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
box  ref  deref  move  clone  as  true  false
async  await  channel
```

说明：

- `return` 是保留字，但本语言没有 `return` 语句。
- `unsafe` 和 `trusted` 为未来版本保留。
- `Future` **不是**保留字：只有在类型位置的 `Future<T>` 形式会被识别
  （第 04 章 §4.9），裸的 `Future` 标识符仍然可以是用户定义的 struct 名。
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

字符串是 UTF-8 编码。字符串值是 **owned**（第 08 章）。

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

所有字段都必须初始化。见第 04 章 §4.7。

## 1.5 源文件

- 源文件使用扩展名 `.fln`。
- 文件由可选的 `import` 声明（第 12 章）、`struct` 声明（第 04 章 §4.7）、
  全局绑定（第 03 章）、`const` 绑定和函数声明（第 07 章）组成。
- 其中可以有一个名为 `main` 的函数；程序从这里开始执行。

```fleen
// program.fln
import std.io;

greeting = "hello";             // 全局绑定

func main() {
    std.io.println(greeting);
}
```
