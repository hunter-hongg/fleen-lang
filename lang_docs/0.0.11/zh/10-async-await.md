# 10 — 异步与 await

## 10.1 `async func`

`async` 把函数标记为异步函数：

```fleen
async func half(n: int): Result<int, string> {
    if n == 0 { Err("cannot halve 0") } else { Ok(n / 2) }
}
```

签名规则：**函数体返回 `T`，而调用者得到 `Future<T>`**。调用该函数不会
执行函数体——它返回一个可以稍后驱动的 future 值：

```fleen
f = half(10);                 // f: Future<Result<int, string>>；函数体尚未执行
```

异步函数的函数体是普通的 Fleen 代码：绑定、`choose`、`?`、`for`、对其他
函数的调用。声明可以与 `pub` 组合用于跨模块使用（第 12 章）：

```fleen
pub async func add(a: int, b: int): int {
    a + b
}
```

没有 `async {}` 块形式；只有 `async func` 声明。

## 10.2 `Future<T>`

`Future<T>` 是调用函数体返回 `T` 的 `async func` 所得到的值的类型。

| 性质 | 行为 | 示例 |
|------|------|------|
| owned | 用 `move` 移动，`clone` 深拷贝——但 `clone f` 被拒绝 | `g = move f;` |
| 只能驱动一次 | 一次 `await` 消耗它；不能驱动第二次 | `v = f await;` |
| 可存于局部变量 | 驱动前可以绑定和传递 | `f: Future<int> = add(20, 22);` |
| 不可 clone | `clone f` 是编译错误 | |
| 不能作 struct 字段 / box 单元 / 全局变量 | 编译期拒绝 | |
| 容器内不可索引/遍历 | 对 `[Future<T>]` 的 `ts[i]` 和 `for` 都被拒绝 | |

```fleen
f: Future<int> = add(20, 22);
v = f await;                  // v = 42
```

`Future` 不是保留字：只有类型位置的参数化形式 `Future<T>` 会被识别。

## 10.3 `await`

`e await` 把 future `e` 驱动到完成并产出其载荷。它是**后缀**运算符，处于
最紧的后缀层，与 `()`、`[i]`、`.f`、`?` 从左到右链式结合：

```fleen
n = add(20, 22) await;              // n = 42
add(20, 22) await;                  // 语句形式：驱动，丢弃载荷
r = fib(n - 1) await + fib(n - 2) await;   // 不需要括号
```

规则：

- `e await` 要求 `e: Future<T>` 并产出 `T`；对其他类型使用 `await` 是
  编译错误。
- `await` 消耗该 future；之后再用它是 use-after-move 错误。
- 如果该 future 的执行失败了，错误在 `await` 处抛出。
- 顶层的 `e await`（例如在 `main` 中）会运行执行器直到该 future 完成
  ——程序在此阻塞直到它结束。

```fleen
import std.io;

async func add(a: int, b: int): int { a + b }

func main() {
    v = add(20, 22) await;
    std.io.println((v as string));
}
```

## 10.4 `await?`

`?` 与 `await` 组合。在返回 `Result<T, E>` 的函数内：

```fleen
func halve(n: int): Result<int, string> {
    v = half(n) await?;      // 解析为 (half(n) await)?
    Ok(v)
}
```

`half(n) await?` 先驱动 future（得到 `Result<int, string>`），再应用
`?`：`Err` 立即传出 `halve`，`Ok` 载荷继续向下执行。

## 10.5 执行器：`std.async.run`

`std.async.run(tasks)` 接收一个 future 数组，把所有任务运行到完成：

```fleen
import std.async;

async func task_a() { /* … */ }
async func task_b() { /* … */ }

func main() {
    std.async.run([task_a(), task_b()]);
}
```

- 参数是 `[Future<unit>]`：函数体返回 `unit` 的异步函数。调用
  `task_a()` 并不执行它；`run` 才会执行。
- `run` 只在列表中**每个**任务都结束后才返回（join）。
- 任务在单个执行器上逐个运行，按**轮转（round-robin）**顺序。任务只在
  挂起点（§10.6）让出；两个挂起点之间任务不中断地运行。
- `run` 也可以在异步函数体内调用；此时外层任务等待内层批次完成后再
  继续：

  ```fleen
  async func orchestrator() {
      std.async.run([inner_a(), inner_b()]);   // 等两者都完成
      more_work();
  }
  ```

- 如果任务永远无法取得进展（全部阻塞在通道上），`run` 抛出
  `ChannelDeadlock` 运行时错误（第 11 章 §11.5）。

## 10.6 挂起点

异步函数体内，任务只在这些操作处让出执行器：

| 操作 | 行为 |
|------|------|
| `std.io.sleep(ms)` | 挂起任务，直到 `ms` 毫秒过去 |
| `std.async.yield_now()` | 挂起任务一轮调度 |
| 满的通道上的 `c.send(v)` | 挂起，直到腾出空间（第 11 章） |
| 空的通道上的 `c.recv()` | 挂起，直到有值到达（第 11 章） |

在异步函数体内这些操作**不写** `await`——编译器会插入挂起：

```fleen
import std.async;
import std.io;

async func worker(label: string) {
    std.io.sleep(5);            // 任务在此挂起
    std.io.println(label, " woke");
}

async func ticker() {
    for i in range(3) {
        std.io.println(i as string);
        std.async.yield_now();  // 让其他任务运行
    }
}
```

在**同步**（非异步）上下文中，`std.io.sleep` 和 `std.async.yield_now`
什么也不做。

调度顺序是确定的：

- 已就绪的任务总是先于到期定时器运行；
- 同时到期的定时器按创建顺序唤醒；
- 挂在通道上的任务按挂起顺序唤醒。

因此同一程序每次运行产生相同的输出。

## 10.7 异步函数体内的规则

- 参数和值必须是 owned 或 Copy：异步函数不能声明 `ref` 参数，`ref`
  句柄不能跨越 `await`、经通道发送或交给任务。
- 全局变量可以**读取**（每次读取深拷贝），但不能在异步函数体内**写入**；
  触达全局的字段写、索引写、限定名写都是编译错误。请改用通道发送该值
  （第 11 章）。
- 异步代码调用的同步函数仍然可以带 `ref` 参数：借用只存活于该次调用。

```fleen
struct Counter { n: int }
count = Counter { n: 0 };      // 全局

async func bump() {
    count.n = 1;               // 编译错误：异步代码写全局变量
}

async func report(out: channel<int>) {
    out.send(count.n);         // 没问题：读取，然后通信
}
```
