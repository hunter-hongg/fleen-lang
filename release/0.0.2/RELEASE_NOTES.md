# Changelog

所有显著变更记录于此。格式参考 [Keep a Changelog](https://keepachangelog.com/)，
版本语义遵循 [SPEC.md](SPEC.md) §13（`0.0.x` = 早期开发，API 不稳定）。

## 0.0.2 — Hermit Crab (2026-10-11) 🐚

所有权起步：**堆上的值开始有主人，错误开始流动，分号开始退休。**
值有了唯一所有者与显式的 `move` / `clone`；错误有了 `?` 传播与 `Ok` / `Err`
模式匹配；分号交给自动插入（ASI），显式分号仍然合法。

### 语言一览

```fleen
func div(a: int, b: int): Result<int, string> {
    if b == 0 { Err("division by zero") } else { Ok(a / b) }
}

func safe_half(a: int): Result<int, string> {
    q = div(a, 2)?          // Err 时整函数立即返回该 Err
    Ok(q)
}

func main(): Result<int, string> {
    s = "hello"
    t = move s              // 所有权转移，此后 s 不可用

    n = choose safe_half(10) {
        when Ok(v) { v }
        when Err(e) { 0 - 1 }
    }

    b = box n               // 堆分配，b: box<int>
    deref b = deref b + 1   // 写点内值

    print(t, " world: ", (n + deref b) as string)
    Ok(0)
}
```

```console
$ fleen-vm showcase.fln
hello world: 11
```

分号不再必填（ASI，跨行续接与语句边界规则见 `docs/0.0.2/ASI.md`），
0.0.1 的显式分号风格仍然完全合法。

### ✨ 语言特性

**所有权系统**

- `string` / `box<T>` 拥有唯一所有者；`move x` 显式转移、`clone x` 深拷贝
- Copy 类型（`int` / `float` / `bool` / `unit` / 函数值）不受影响，照旧按值复制
- 任何路径上使用已移动的绑定在编译期拒绝（`UseAfterMove`、
  `MaybeMovedAfterBranch` 等），循环内 move 后回边再用同样被拒绝
- owned 值赋值 / 传参必须显式 `move` 或 `clone`（`OwnedArgRequiresMove`，
  help 提示两种选择）

**`box<T>` 与 `deref`**

- `box expr` 堆分配（`b: box<int>`），`deref b` 读点内值（副本，owned 深拷贝）、
  `deref b = v` 写点内值；点内类型允许集合：`int` / `float` / `bool` /
  `string` / `box<U>` / `Result<T, E>`
- **不可移出 box**：`move deref b` 编译错误，需要值请 `clone deref b`
- `deref b = v` 的 `b` 必须是**局部 box**（全局 box 写点内值报
  `DerefAssignOfGlobalBox`）；确定性释放，无 GC

**`ref T` 只读借用（仅参数位置）**

- `func shout(s: ref string): int { print(s); 42 }` —— 只读、零拷贝
- 句柄不能逃逸：不能赋值、存储、返回 → 无需借用检查器，没有悬垂的可能
- 实参必须是变量（局部 / 全局 / 另一 ref 参数）；全局实参经临时槽复制承接，
  全局初始化器不支持 ref 实参（`RefArgInGlobalInit`）

**错误处理**

- `Ok(v)` / `Err(e)` 构造；`choose` 的 `Ok` / `Err` pattern，两臂齐即穷尽
  （带守卫的臂不计入穷尽性——与 Bool scrutinee 同一条规则）
- `?` 传播运算符：`q = div(a, 2)?` —— Err 时整函数立即返回该 Err
- 入口函数（`main`）可返回 `Result`：尾随 `Err` 打印 payload 到 stderr
  并以退出码 1 结束（退出码契约不变：`0` 成功 / `1` 错误 / `2` 用法·IO）

**ASI：分号可选**

- 换行敏感：同行语句拼接仍然非法；跨行续接（二元运算符、`?` 链、`as`）
  与语句边界由独立的 ASI pass 判定（`docs/0.0.2/ASI.md`）
- 显式分号处处合法，0.0.1 程序无需改动

**`as` 类型转换（0.0.2 临时，泛型后重审）**

- 白名单仅三条：`int as string`（十进制含负号）、`float as string`
  （与 print 输出一致）、`bool as string`（`"true"` / `"false"`）；
  其余一切（含恒等转换）报 `UnsupportedCast { from, to }`
- 结果是新鲜 owned string，消费位置无需 `move` / `clone`
- print 与 `as` 共用同一格式化 helper（`fleen-vm/src/fmt.rs`），两条路径不会漂移

### ⚠️ Breaking

- owned 值（`string` / `box<T>`）赋值 / 传参需显式 `move` 或 `clone`
  （0.0.1 的 `t = s` 是浅共享复制，现在是编译错误）
- `move` / `clone` / `deref` / `as` 成为保留字（0.0.1 可用作标识符）
- `print(函数值)` 由 0.0.1 的放行改为编译错误

### 🛠 工具链

```
showcase.fln ─► Lex ─► ASI ─► Parse ─► Resolve ─► Typeck(含所有权) ─► Lower ─► Codegen ─► showcase.flnc
                                                                                          │
                                                        fleen-verify 静态验证（v1+v2 规则）◄─┘
                                                                                          │
                                                                                    fleen-vm 执行
```

| 组件 | 0.0.2 变化 |
|------|------------|
| `fleen-compiler` | ASI 独立 pass；typeck 含所有权检查子遍历（`typeck/ownership.rs`，仍属同一阶段）；MIR 指令携带 Span 并下沉到字节码 |
| `fleen-verify` | v2 指令栈深分析（`AllocBox` 净效果 0 等）；v2 指令禁止出现在 version=1 模块（一次比较判定）；拒绝空函数体（0.0.1 曾 panic，现报 `EmptyFunctionBody`） |
| `fleen-vm` | 字节码 v2 执行（所有权 0x80–0x93、Result 0xA0–0xA4、`ToStr` 0xA5）；`Value::Str` 由 `Rc<str>` 改为 `Box<str>`（独占所有权）；v1 字节码保持可执行 |

- **运行时诊断**：`span_map` 转正——`.fln` 模式下运行时错误携带源码行列；
  经调用链传播的错误指向调用点（0.0.2 U10）
- **`print` 获得真实签名**：`(string...) -> unit`，非 string 实参报
  `ArgTypeMismatch` 并提示 `as string`；0.0.1 的 typeck 特判（已知 Hack）移除，
  `DESIGN.md` §18 已知 Hack 清空
- **字节码 v2 与 v1 兼容**：v2 模块可执行 v1 指令流；编号只增不改
  （规范见 `docs/BYTECODE.md` §10）
- **安全性修复**：`.flnc` 解码器不再对恶意表长度做无界预分配
  （0.0.1 起超大长度会触发进程 abort，现降级为解码错误）

### 🧪 质量保障

- **608 测试**（0.0.1 为 321）：单元测试 + 各阶段 valid/invalid 文件驱动的
  集成测试 + 端到端测试（驱动真实二进制，比对 stdout、退出码与 stderr）
- 所有权/借用/Result/`?`/cast 的错误路径均有专项 invalid fixture；
  0.0.1 全部 E2E（含显式分号版 `fib.fln`）在 0.0.2 下逐字节回归通过
- 门禁四件套：`cargo fmt --check` · `cargo clippy -D warnings` ·
  `cargo test` · `cargo doc`（GitHub Actions 已配置）

### 📦 安装

从源码构建（需要 Rust 1.85+，2024 edition）：

```bash
git clone https://github.com/hunter-hongg/fleen-lang.git
cd fleen-lang
cargo build --release
./target/release/fleen-vm showcase.fln
```

预编译二进制：release 附件提供 linux x86_64 压缩包
（含 `fleen-compiler` / `fleen-verify` / `fleen-vm` 三个可执行文件），
解压即用：`./fleen-vm showcase.fln`。

仓库内提供单命令入口：

```bash
cargo fln showcase.fln     # 编译 → 验证 → 执行，一步到位
cargo fln showcase.flnc    # 也可以直接运行字节码
```

### 🔜 下一步

0.0.3：`struct` / `for` / 迭代器（届时重审嵌套 owned 与完整转换矩阵）；
完整路线见 `docs/DESIGN.md` §18，0.0.2 的规划与实现决议记录见
`docs/0.0.2/PLAN.md` 与 `docs/0.0.2/TICKETS.md`。
