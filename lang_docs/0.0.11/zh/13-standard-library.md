# 13 — 标准库

标准库以 Fleen 源码编写，嵌入编译器，在编译期一并解析。

**预导入（prelude）**是 `print`，以及自 0.0.10 起的 **Result 三元组**
`Result` / `Ok` / `Err`（§13.10）。其余所有函数必须通过 `import` 按名称引入。

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
| `print` | `(string, ...) -> unit` | 输出到 stdout（预导入，无需 import） |
| `println` | `(string, ...) -> unit` | 输出并换行 |
| `read_line` | `() -> string` | 从 stdin 读取一行（不含换行符）；输入结束时返回 `""` |
| `sleep` | `(int) -> unit` | 睡眠指定毫秒数；在 async 任务内会挂起任务（awaitable） |

## 13.2 `std.str`

```fleen
import std.str;
```

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `len` | `(string) -> int` | 字符串长度（Unicode 标量值个数，不是字节数） |
| `from_int` | `(int) -> string` | 整数转字符串 |
| `from_float` | `(float) -> string` | 浮点转字符串 |
| `from_bool` | `(bool) -> string` | 布尔转字符串 |
| `concat` | `(string, string) -> string` | 连接两个字符串 |
| `split` | `(string, string) -> [string]` | 按分隔符切分，保留空段 |
| `parse_int` | `(string) -> Result<int, string>` | 解析十进制整数（0.0.9） |
| `parse_float` | `(string) -> Result<float, string>` | 解析十进制浮点（0.0.9） |
| `trim` | `(string) -> string` | 去除首尾 Unicode 空白（0.0.9） |
| `contains` | `(string, string) -> bool` | 是否包含子串（0.0.9） |
| `starts_with` | `(string, string) -> bool` | 前缀判定（0.0.9） |
| `ends_with` | `(string, string) -> bool` | 后缀判定（0.0.9） |
| `index_of` | `(string, string) -> Result<int, string>` | 子串首次出现位置（0.0.9） |
| `replace` | `(string, string, string) -> string` | 把 `from` 的每次出现替换为 `to`（0.0.9） |

注意：不能写 `"hello".len()`；`string` 不支持字段访问运算符，应使用 `std.str.len("hello")` 或相应导入形式。

**0.0.9 批量内建（D3）**：

- 所有位置索引都按 **Unicode 标量值**计数（与 `len` 同一单位，绝不是字节偏移）。
- 可能失败的地方，失败是一个值（`Err`），绝不会崩溃：用 `choose` 或 `?` 提取。
- `parse_int` 只接受十进制（整数可带前导 `-` 或 `+`）；空串、前后空白、非数字
  输入和 i64 溢出都是 `Err` 值——可能有空白时先 `trim`。
- `index_of` 返回 `Result` 而不是 `-1` 哨兵值：失败是值（与 `std.arr.get` 同一
  姿态）；`contains` / `starts_with` / `ends_with` 返回 `bool`，因为问题的答案
  是全的。空串子串包含于任何字符串、是任何字符串的前缀/后缀、并在 `Ok(0)`
  处找到。
- `replace` 不拆开任何标量值；`from` 为空串时是空操作（有意不采用 Rust 的
  每个边界插入行为）。
- `substring` / `slice` / `chars` 在 0.0.9 中**不提供**（推迟到 0.1.0 的字符串
  工作）；`for` 遍历字符串同样不提供。

```fleen
import std.str;

r = std.str.parse_int("42");            // Ok(42)
bad = std.str.parse_int("4x2");         // Err("invalid int: 4x2")
s = std.str.trim("  hi  ");             // "hi"
has = std.str.contains("hello", "ell"); // true
ix = std.str.index_of("héllo", "l");    // Ok(2)——标量值索引
rep = std.str.replace("aaa", "a", "b"); // "bbb"
```

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

## 13.10 `std.result` —— `Result` 声明（0.0.10）

```fleen
import std.result;    // 可选——三元组已是预导入
```

`std/result.fln` 持有 `Result` 的声明（0.0.10，D1）——语言层的事实来源：

```fleen
pub enum Result<T, E> {
    Ok(T),
    Err(E),
}
```

- 该模块**不含任何函数**：编译器按规范路径识别 `std.result.Result` 并映射
  到内部表示，不产生 `enum_defs` 表项、不做变体代码生成——这是一个纯声明
  模块。
- `import std.result;` 导入 `Result` 名；`Ok` / `Err` 两个变体随导出一同
  可用。它引入的与预导入中的是同一组名字，所以显式导入只是来源标注，
  不是要求。
- 该声明不可遮蔽、不可重复声明：在任何位置写 `enum Result`，或把绑定/
  参数/函数命名为 `Ok`、`Err`、`Result`，都是 `ReservedBuiltinName`
  （第 09 章 §9.1）。
- 错误处理本身见第 09 章：`?` 传播与 Result 上的 `choose` 保持 0.0.2/0.0.9
  的语义与降级不变（0.0.10，D3）。

## 13.11 `std.map` —— 键→值映射（0.0.11）

```fleen
import std.map;
```

map 类型本身是保留字字面量（第 04 章 §4.13）；本模块承载八个辅助函数。
它们全部是 VM 内建——零新操作码（0.0.9 `std.str` 批量的模板）。

| 函数 | 签名（概要） | 说明 |
|------|-------------|------|
| `get` | `(map[K, V], K) -> Result<V, string>` | 键对应的值；键不存在时返回 `Err("key not found")` |
| `insert` | `(map[K, V], K, V) -> map[K, V]` | 加入键后的**新** map（已有键被覆盖）；**消费**输入的 map |
| `remove` | `(map[K, V], K) -> map[K, V]` | 去掉键后的**新** map；**消费**输入的 map（移除不存在的键返回等值的 map） |
| `contains` | `(map[K, V], K) -> bool` | 键是否存在 |
| `keys` | `(map[K, V]) -> [K]` | 按插入顺序的键数组 |
| `values` | `(map[K, V]) -> [V]` | 按插入顺序的值数组 |
| `len` | `(map[K, V]) -> int` | 条目数 |
| `clone` | `(map[K, V]) -> map[K, V]` | 显式深拷贝（`clone` 关键字同样可用） |

> **注意**：`clone` 是保留字，且不在点分路径成员位置的关键字豁免之列
> （豁免的只有 `async` / `await` / `channel` / `map`），因此显式调用写法
> `std.map.clone(m)` 无法解析。请用 `clone` 关键字深拷贝 map：`c = clone m;`。

```fleen
import std.map;
import std.io;

m = map { "a" -> 1, "b" -> 2 };

r = std.map.get(m, "a");            // Ok(1)
nf = std.map.get(m, "z");           // Err("key not found")
has = std.map.contains(m, "b");     // true
n = std.map.len(m);                 // 2

m2 = std.map.insert(m, "c", 3);     // {"a"->1, "b"->2, "c"->3}；m 被消费
m3 = std.map.remove(m2, "b");       // {"a"->1, "c"->3}；m2 被消费
c = clone m3;                       // 深拷贝；m3 仍然可用

for k in std.map.keys(m3) {         // 迭代经由 keys/values
    std.io.println(k);              // a c
};
```

所有权说明：

- `insert` / `remove` 是**函数式且消费输入的**（D2）：输入的 map 被移动进
  调用，结果是一个全新的 map。之后继续使用输入变量是普通的
  `UseAfterMove` 错误；`std.map.insert(clone m, "c", 3)` 可以保住 `m`。
- 只读辅助（`get`、`contains`、`keys`、`values`、`len`）从不消费 map：
  `keys` / `values` 把条目深拷贝进新数组。`clone` 深拷贝用 `clone` 关键字
  书写（见上方注意）。
- 键被白名单限制在 `{int, bool, string}`（否则 `MapKeyNotComparable`）；
  值槽不允许出现 `Future` / `channel` / `ptr`。
- 插入顺序是唯一可观察的顺序（D8）：`keys` / `values` 按源码插入顺序返回，
  重复的 `insert` 原地覆盖且不改变顺序。

`m[k]` 与 `std.map.get(m, k)` 是同一个操作：索引后缀是后缀写法，`get` 是
显式调用写法（第 04 章 §4.13）。
