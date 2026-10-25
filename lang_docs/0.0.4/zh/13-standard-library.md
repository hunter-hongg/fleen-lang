# 13 — 标准库

标准库随编译器一起发布，按叶子模块导入：`import std.io;` 引入 `std.io`。
唯一的预置名（无需任何导入即可使用）是 `print`。没有再导出机制：用哪个
模块就导入哪个。

## 13.1 std.io —— 控制台与输入

```fleen
import std.io;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `print` | `(string, ...) -> unit` | 依次打印每个参数，不加换行 |
| `std.io.println` | `(string, ...) -> unit` | 依次打印每个参数，然后换行 |
| `std.io.read_line` | `() -> string` | 从标准输入读取一行（不含末尾换行）；输入结束时返回 `""` |
| `std.io.sleep` | `(int) -> unit` | 定时器，单位毫秒——见第 10 章 §10.6 |

- `print`/`println` 的参数必须是字符串；标量请用 `as string` 转换
  （第 05 章 §5.5）。
- `print` 是预置名：`print(...)` 与 `std.io.print(...)` 是同一个函数，
  但 `print` 不需要导入。

```fleen
import std.io;

print("a", "b");            // ab（不换行）
std.io.println("c");        // c\n

name = std.io.read_line();
std.io.println("hello ", name);
```

```fleen
import std.io;

n = 42;
std.io.println("n = ", n as string);
```

## 13.2 std.str —— 文本

```fleen
import std.str;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `from_int` | `(int) -> string` | 整数的十进制字符串 |
| `from_float` | `(float) -> string` | 浮点数的字符串形式 |
| `from_bool` | `(bool) -> string` | `"true"` / `"false"` |
| `len` | `(string) -> int` | 字符数（Unicode 标量值，不是字节） |
| `concat` | `(string, string) -> string` | 两个字符串拼接 |
| `split` | `(string, string) -> [string]` | 按分隔符切分的子串数组，保留空段 |

```fleen
import std.str;

s = std.str.from_int(42);          // "42"
n = std.str.len("héllo");          // 5
t = std.str.concat("foo", "bar");  // "foobar"
parts = std.str.split("a,,b", ",");// ["a", "", "b"]
```

`from_int` / `from_float` / `from_bool` 是 `as string` 的类型化入口；
日常代码优先用 `as string`。

## 13.3 std.num —— 数值

```fleen
import std.num;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `abs` | `(int) -> int` | 绝对值 |
| `min` | `(int, int) -> int` | 较小的参数 |
| `max` | `(int, int) -> int` | 较大的参数 |
| `sqrt` | `(float) -> float` | 非负平方根 |
| `floor` | `(float) -> int` | 向下取整到最近的整数 |

```fleen
import std.num;

a = std.num.abs(0 - 5);        // 5
b = std.num.min(3, 7);         // 3
c = std.num.max(3, 7);         // 7
d = std.num.sqrt(2.0);         // 1.4142…
e = std.num.floor(3.9);        // 3
```

`std.num.abs` 作用于最小 `int` 时溢出，是运行时算术错误。

## 13.4 std.arr —— 数组

四个函数都作用于任意元素类型的 `[T]`；元素类型在每次调用处推断。数组是
owned 值，因此 `set` 和 `push` 返回**新**数组，不修改它们的参数。

```fleen
import std.arr;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `len` | `([T]) -> int` | 元素个数 |
| `get` | `([T], int) -> Result<T, string>` | 索引处的元素；界内为 `Ok`，越界为 `Err` |
| `set` | `([T], int, T) -> [T]` | 替换第 `i` 个元素后的新数组 |
| `push` | `([T], T) -> [T]` | 末尾追加该值后的新数组 |

```fleen
import std.arr;

a = [1, 2, 3];
n = std.arr.len(a);                     // 3
r = std.arr.get(a, 1);                  // Ok(2)
b = std.arr.set(a, 0, 9);               // [9, 2, 3]；a 仍是 [1, 2, 3]
c = std.arr.push(b, 4);                 // [9, 2, 3, 4]
bad = std.arr.get(a, 99);               // Err(...)
```

`std.arr.get` 返回 `Result`，因此可能的越界读取可以用 `choose` 或 `?`
处理，而不必崩溃。

## 13.5 std.collection.range —— Range 构造器

```fleen
import std.collection.range;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `range` | `(int) -> Range` | 区间 `0..n` |
| `range_from` | `(int, int) -> Range` | 区间 `a..b` |

两端都是 exclusive；起点等于终点时区间为空。导入之后，这两个名字也可以
不带 `std.collection.range.` 前缀使用。

```fleen
import std.collection.range;
import std.io;

for i in range(3) {
    std.io.println(i as string);        // 0 1 2
};
for i in range_from(2, 5) {
    std.io.println(i as string);        // 2 3 4
};
```

## 13.6 std.collection.iter —— 即时迭代助手

```fleen
import std.collection.iter;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `map` | `([T], (T) -> U) -> [U]` | 对每个元素应用 `f` |
| `filter` | `([T], (T) -> bool) -> [T]` | 保留 `f` 为 true 的元素 |
| `fold` | `([T], A, (A, T) -> A) -> A` | 用累加器从左向右折叠 |
| `take` | `([T], int) -> [T]` | 前 `n` 个元素 |

所有助手都是**即时的**（数组进、数组出），并且以拷贝方式读元素：源数组
永不被消耗，传入的函数也永远不会拿到原始元素。

```fleen
import std.collection.iter;

func twice(n: int): int = n * 2
func is_even(n: int): bool { n % 2 == 0 }
func add(a: int, b: int): int = a + b

xs = [1, 2, 3, 4];
doubled = std.collection.iter.map(xs, twice);       // [2, 4, 6, 8]
evens   = std.collection.iter.filter(xs, is_even);  // [2, 4]
total   = std.collection.iter.fold(xs, 0, add);     // 10
head    = std.collection.iter.take(xs, 2);          // [1, 2]
```

## 13.7 std.async —— 执行器

```fleen
import std.async;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `run` | `([Future<unit>]) -> unit` | 把每个 future 运行到完成（第 10 章 §10.5） |
| `yield_now` | `() -> unit` | 当前任务让出一轮调度；同步代码中无效果 |

```fleen
import std.async;

async func work() { /* … */ }

func main() {
    std.async.run([work()]);
}
```

## 13.8 std.concurrent —— 通道

```fleen
import std.concurrent;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `split` | `(channel<T>) -> channel<T>` | 指向同一缓冲区的第二个句柄（第 11 章 §11.4） |

通道**构造器**是保留字 `channel<T>(cap)`，不是本模块的函数。

```fleen
import std.concurrent;

ch = channel<int>(2);
peer = std.concurrent.split(ch);
```

## 13.9 std.os —— 进程

```fleen
import std.os;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `args` | `() -> [string]` | 程序参数，不含程序名 |
| `exit` | `(int) -> unit` | 以给定退出码终止进程；永不返回 |

```fleen
import std.os;

args = std.os.args();
if std.arr.len(args) == 0 {
    std.os.exit(1);        // 立即以退出码 1 停止
};
```

## 13.10 std.time —— 时钟

```fleen
import std.time;
```

| 函数 | 签名 | 说明 |
|------|------|------|
| `now_millis` | `() -> int` | 墙钟时间，单位毫秒 |

```fleen
import std.time;

start = std.time.now_millis();
/* … 工作 … */
elapsed = std.time.now_millis() - start;
```
