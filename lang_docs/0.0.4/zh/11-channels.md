# 11 — 通道

## 11.1 创建通道

`channel<T>(cap)` 创建一个携带 `T` 类型值、缓冲容量为 `cap` 的通道：

```fleen
ch = channel<int>(16);       // 有缓冲：最多 16 个值，满之前 send 不挂起
ch = channel<string>(4);     // 携带字符串的通道
ch = channel<int>(0);        // 会合（rendezvous）：无缓冲；send 直接交付值
```

- `channel` 是保留字，有两种形式：类型位置的 `channel<T>` 和表达式位置
  的构造器 `channel<T>(cap)`。
- `cap = 0` 是**会合**通道：不缓冲任何值，`send` 挂起直到接收者取走值。
- `cap < 0` 是运行时错误。

## 11.2 发送与接收

`c.send(v)` 把值放入通道；`c.recv()` 取出一个值。

```fleen
import std.async;
import std.concurrent;
import std.io;

async func producer(c: channel<int>) {
    c.send(1);
    c.send(2);
}

async func consumer(c: channel<int>) {
    a = c.recv();            // a = 1
    b = c.recv();            // b = 2
    std.io.println(a as string, b as string);
}
```

阻塞行为：

| 操作 | 挂起时机 |
|------|----------|
| `c.send(v)` | 缓冲已满——或容量为 0 时，直到接收者取走值 |
| `c.recv()` | 缓冲为空 |

在异步函数体内这两个操作都不写 `await`；任务自动挂起，在操作可以进行时
被唤醒。值按发送顺序被接收（FIFO）。

## 11.3 载荷所有权

- **Copy** 载荷（`int`、`float`、`bool`、`unit`、函数值）经通道拷贝。
- **owned** 载荷（例如 `string`）转移所有权：`s = c.recv()` 之后，接收
  侧任务拥有该值。
- owned **变量**在发送处必须显式 move 或 clone；**新值**天然转移：

  ```fleen
  async func send_things(c: channel<string>) {
      a = "hello";
      c.send(move a);        // owned 变量：必须显式 move
      c.send("world");       // 新值：不需要关键字
  }
  ```

  对 owned 变量 `a` 写 `c.send(a)` 是编译错误；错误信息会建议
  `move a` 或 `clone a`。
- `ref` 载荷在编译期被拒绝：借用永不穿过通道。

## 11.4 `split` —— 第二个句柄

通道值只有一个所有者。`std.concurrent.split(ch)` 返回**指向同一缓冲区的
第二个句柄**；两个句柄是相互独立的值，寻址同一个通道：

```fleen
ch = channel<int>(0);
peer = std.concurrent.split(ch);

std.async.run([sender(move ch), receiver(move peer)]);
```

- `split` 是让一个通道获得两个句柄的唯一途径。
- `clone ch` 是编译错误——通道不能 clone；请用 `split`。
- 通道不能存入 struct 字段或数组元素；句柄保存在局部变量中并用
  `move` 传递。

## 11.5 死锁

通道没有 `close` 操作，`for` 也不能遍历通道。由此带来的结果：当一次
`run` 的任务集无法再取得进展——每个剩余任务都挂在某个通道 send/recv 上，
而没有任何任务会满足它——运行时**立即**抛出 `ChannelDeadlock` 错误，
而不是一直等待：

```fleen
async func lonely(c: channel<int>) {
    v = c.recv();            // 没有人会发送
}

func main() {
    ch = channel<int>(0);
    std.async.run([lonely(move ch)]);    // 运行时错误：ChannelDeadlock
}
```

由于没有 `close`，发送方用自己选定的协议值表示"发送完毕"——例如一个
`Result` 或哨兵载荷——由接收方用 `choose` 匹配。

## 11.6 完整示例

```fleen
import std.async;
import std.concurrent;
import std.io;

async func ping(ch: channel<int>) {
    n = 0;
    while n < 3 {
        ch.send(n);
        n = n + 1;
    }
}

async func pong(peer: channel<int>) {
    got = 0;
    while got < 3 {
        v = peer.recv();
        std.io.println("got ", v as string);
        got = got + 1;
    }
}

func main() {
    ch = channel<int>(0);              // 会合通道
    peer = std.concurrent.split(ch);
    std.async.run([ping(move ch), pong(move peer)]);
}
```

```text
$ cargo fln pingpong.fln
got 0
got 1
got 2
```
