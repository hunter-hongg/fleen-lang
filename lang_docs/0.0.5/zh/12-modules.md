# 12 — 模块

## 12.1 文件与模块

每个 `.fln` 文件是一个模块。工程是源目录（约定为 `src/`）下的模块树：

```text
myproj/
└── src/
    ├── main.fln        // 入口模块（包含 func main）
    ├── auth.fln
    └── net/
        └── tcp.fln
```

## 12.2 `import`

`import` 声明把另一个模块导出的项以限定名引入作用域：

```fleen
// main.fln
import auth;
import net.tcp;

func main() {
    token = auth.make_token();
    n = net.tcp.connect();
}
```

- 路径用点分隔，解析到文件：`auth` → `src/auth.fln`（或
  `src/auth/mod.fln`）；`net.tcp` → `src/net/tcp.fln`（或
  `src/net/tcp/mod.fln`）。
- 导入 `net.tcp` 之后 `net` 本身也可寻址，所以 `net.tcp.x` 可用，声明在
  `net` 模块上的项也能通过 `net.<item>` 访问。
- 标准库模块以同样方式导入：`import std.io;`（第 13 章）。
- 保留字可以作为路径段：`import std.async;`、`import std.concurrent;`。

## 12.3 `pub` —— 导出

声明**默认私有**：只有其所在模块能使用。在顶层声明前加 `pub` 使其可被
导入：

```fleen
// auth.fln
pub func make_token(): string = "t-1"

func helper(): int = 1          // 私有：其他模块不能调用 helper
```

规则：

- `pub` 可以出现在 `func`、`const` 或全局绑定、`struct` 声明上。
- `pub` 只在文件顶层有效；块内的 `pub`，或 `import` 前面的 `pub`，都是
  语法错误。
- struct 的可见性覆盖其所有字段：没有按字段的访问修饰。
- `async func` 与普通函数一样导出：`pub async func f(...)`。

## 12.4 跨模块访问项

项通过限定路径 `module.item` 访问：

```fleen
import net.tcp;

r = net.tcp.connect();
```

- 路径的每个非末段必须是模块；末段是函数、全局变量、`const` 或 struct。
- 跨模块访问私有项是编译错误（`PrivateAccess`）；请给声明加 `pub`。
- 给其他模块的**全局变量**赋值用限定形式 `m.g = v`（受 `pub` 以及
  第 10 章 §10.7 的异步写入规则约束）。

## 12.5 解析错误

| 情形 | 错误 |
|------|------|
| 路径没有匹配的文件 | `ModuleNotFound`（消息列出尝试过的候选路径） |
| 两个文件解析到同一模块路径 | `DuplicateModule` |
| 导入图中有环 | `CircularImport`（报告环路径） |
| 从其他模块访问私有项 | `PrivateAccess` |

## 12.6 示例：三模块程序

```fleen
// src/mathutil.fln
pub func square(n: int): int = n * n

func internal(): int = 99      // 对外不可见
```

```fleen
// src/main.fln
import mathutil;
import std.io;

func main() {
    std.io.println(mathutil.square(7) as string);   // 49
    // mathutil.internal()                          // 错误：PrivateAccess
}
```

```text
$ cargo fln --project myproj
49
```
