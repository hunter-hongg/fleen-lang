# 13 — 标准库

标准库以 Fleen 源码编写，嵌入编译器，在编译期一并解析。

**预导入（prelude）** 只有 `print`。其余所有函数必须通过 `import` 按名称引入。

```fleen
import std.io;
import std.collection.range;

func main() {
    std.io.println("hi");
    for i in range_from(0, 3) { print(i as string); }
}
```

## 13.1 `std.io`

```fleen
import std.io;
```

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `print` | `(string) -> unit` | 输出到 stdout（预导入，无需 import） |
| `println` | `(string) -> unit` | 输出并换行 |
| `read_line` | `() -> Result<string, string>` | 从 stdin 读取一行 |
| `sleep` | `(int) -> unit` | 睡眠指定毫秒数；在 async 任务内会挂起任务（awaitable） |

## 13.2 `std.str`

```fleen
import std.str;
```

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `len` | `(string) -> int` | 字符串长度 |
| `from_int` | `(int) -> string` | 整数转字符串 |
| `from_float` | `(float) -> string` | 浮点转字符串 |
| `from_bool` | `(bool) -> string` | 布尔转字符串 |

注意：不能写 `"hello".len()`；`string` 不支持字段访问运算符，应使用 `std.str.len("hello")` 或相应导入形式。

## 13.3 `std.num`

```fleen
import std.num;
```

提供数值相关辅助函数（具体 API 以当前工具链为准）。

## 13.4 `std.arr`

```fleen
import std.arr;
```

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `len` | `([T]) -> int` | 数组长度 |
| `push` | `([T], T) -> [T]` | 追加元素（返回新数组/按实现语义） |
| `concat` | `([T], [T]) -> [T]` | 连接两个数组（0.0.6，VM 内建） |
| `flatten` | `([[T]]) -> [T]` | 展平一层嵌套数组（0.0.6，VM 内建） |

```fleen
import std.arr;

a = [1, 2];
b = [3, 4];
c = std.arr.concat(a, b);      // [1, 2, 3, 4]

nested = [[1, 2], [3], [4]];
flat = std.arr.flatten(nested); // [1, 2, 3, 4]
```

## 13.5 `std.collection.range`

```fleen
import std.collection.range;
```

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `range` | `(int) -> Range` | `0..n`（不含 `n`） |
| `range_from` | `(int, int) -> Range` | `start..end`（不含 `end`） |

`Range` 是 Copy 类型，用于 `for` 循环。

```fleen
import std.collection.range;

for i in range(5) { }           // 0,1,2,3,4
for i in range_from(2, 5) { }   // 2,3,4
```

## 13.6 `std.collection.iter`

```fleen
import std.collection.iter;
```

提供数组/范围上的高阶迭代辅助（0.0.3+）。0.0.6 变更：

- **`fold` 重命名为 `foldl`**（左折叠，破坏性变更，无别名）。
- 新增 **`foldr`**（右折叠）。

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `foldl` | `([T], A, (A,T)->A) -> A` | 左折叠（0.0.6；原 `fold`） |
| `foldr` | `([T], A, (T,A)->A) -> A` | 右折叠（0.0.6） |

```fleen
import std.collection.iter;

func sub(a: int, b: int): int = a - b

xs = [1, 2, 3];
l = std.collection.iter.foldl(xs, 0, sub);  // ((0-1)-2)-3 = -6
r = std.collection.iter.foldr(xs, 0, sub);  // 1-(2-(3-0)) = 2
```

## 13.7 `std.os` 与 `std.time`

```fleen
import std.os;
import std.time;
```

提供操作系统与时间相关的辅助（具体 API 以当前工具链为准）。

## 13.8 `std.async`（0.0.4）

```fleen
import std.async;
```

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `run` | `([Future<unit>]) -> unit` | 协作式执行器：轮转调度任务直至全部完成 |

## 13.9 `std.concurrent`（0.0.4）

```fleen
import std.concurrent;
```

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `split` | `(channel<T>) -> channel<T>` | 分叉通道句柄，返回指向同一缓冲区的第二个句柄（唯一显式分叉点） |
