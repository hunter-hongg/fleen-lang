# 0.0.7 T06 — Lower 层交接文档

> 状态：**已实现**（0.0.7 T06 落地后本文档保留为实现前的分析记录；落地情况
> 以 `docs/0.0.7/TICKETS.md` §T06 的 As-built 为准——例如嵌套 or-pattern 在
> enum/Result payload 中是完整降级而非防御报错，与本文档第四节的预估不同）。
> T06 目标：把 T02（递归模式语法）/ T05（typeck 语义）放行的四种新模式形状真正编译成 MIR——多绑定臂（D4）、嵌套模式链（D5）、or-pattern 备选测试（D6）、type guard 降级为内层模式（D7）。

---

## 一、现状盘点（读代码确认，非猜测）

| 位置 | 现状 |
|------|------|
| [mod.rs:578](../fleen-compiler/src/lower/mod.rs#L578) `first_payload_binding` | 只认第一个 payload 是平 `Ident`；nested/multi-payload 直接报 `"nested/multi-payload enum pattern (0.0.7 T06)"` |
| [mod.rs:2359](../fleen-compiler/src/lower/mod.rs#L2359) | `lower_choose_enum` 顶层拒绝 `Or` / `TypeGuard` 臂 |
| [mod.rs:2099](../fleen-compiler/src/lower/mod.rs#L2099) | generic choose（字面量/ident 臂）拒绝 `Or` / `TypeGuard` |
| [mod.rs:2230](../fleen-compiler/src/lower/mod.rs#L2230) / [mod.rs:2278](../fleen-compiler/src/lower/mod.rs#L2278) | Result choose 拒绝嵌套 payload（`Ok(Some(x))` 不可降） |

三条 choose 路径（generic / Result / enum）的分派在 [mod.rs:2035](../fleen-compiler/src/lower/mod.rs#L2035)：`if matches!(e.scrutinee.ty(), Type::Enum(_) | Type::App(_, _))` 走 enum 路径，否则 generic 路径；Result 在 generic 路径内再分派。T06 三条路径都要动。

---

## 二、相关数据结构（T05 已落地，可直接依赖）

| 类型 / 字段 | 位置 | 用途 |
|------|------|------|
| `TypedPatternHir::Enum { enum_id, variant_index, name, payloads: Vec<Self>, hir_id, span }` | [typed_hir.rs:1035](../fleen-compiler/src/typeck/typed_hir.rs#L1035) | 嵌套 payload 模式；**不含类型信息**（只有 `Ident` 带 `ty`） |
| `TypedPatternHir::ResultCtor { ctor, payload: Box<Self>, hir_id, span }` | [typed_hir.rs:1028](../fleen-compiler/src/typeck/typed_hir.rs#L1028) | `Ok(p)` / `Err(p)`，payload 递归 |
| `TypedPatternHir::EnumVariant { enum_id, variant_index, name, span }` | [typed_hir.rs:1044](../fleen-compiler/src/typeck/typed_hir.rs#L1044) | 零 payload 变体，判别式匹配 |
| `EnumInfo { id, name, copy, type_param_count, payloads: Vec<Vec<Type>> }` | [typed_hir.rs:29](../fleen-compiler/src/typeck/typed_hir.rs#L29) | `payloads[i]` 是变体 i 的 payload 骨架，参数位置是 `Type::Param(i)`；lower 自行代入 use-site args 可得真实类型 |
| `Type::App(Box<EnumInfo>, Vec<Type>)` | [typed_hir.rs:93](../fleen-compiler/src/typeck/typed_hir.rs#L93) | 参数化枚举的使用点类型 |
| `Type::is_copy` / `is_owned` | [typed_hir.rs:246](../fleen-compiler/src/typeck/typed_hir.rs#L246) / [typed_hir.rs:287](../fleen-compiler/src/typeck/typed_hir.rs#L287) | `App` 上做参数代入后重判（D2） |

MIR 指令（无新增，D8）：`EnumIs(variant)`（栈顶是 enum → bool，不消费）/ `EnumPayload(variant_ref, idx)`（pop enum → push payload，**消费性**）/ `Dup` / `DupDeep` / `StoreLocal` / `BindMatch(slot)`。定义见 [mir.rs](../fleen-compiler/src/lower/mir.rs)，op 表见 [BYTECODE.md §6.8](../BYTECODE.md)。

resolver 的 `_` wildcard 走 `declare_pattern_binding`（[resolver/mod.rs:2072](../fleen-compiler/src/resolver/mod.rs#L2072)），返回真实 `BindingId`——lower 无需特判，按普通 ident 提取即可（`SlotAlloc::bind_pattern` 见 [slots.rs:344](../fleen-compiler/src/lower/slots.rs#L344)）。

---

## 三、0.0.6 T05 建立的 CFG 形状（T06 必须复用/推广）

来源：[docs/0.0.6/TICKETS.md:203–219](../docs/0.0.6/TICKETS.md#L203)。

1. **scrutinee 全程 park 在栈上**（[mod.rs:2314](../fleen-compiler/src/lower/mod.rs#L2314)）。
2. 每变体一个 test 块 `Dup/DupDeep ; EnumIs v ; JumpIfTrue body`；因 jump 的 untaken 边落到**物理下一块**（[cfg.rs:7](../fleen-compiler/src/lower/cfg.rs#L7)），块分配顺序即 CFG：test 链 → fallback → body。
3. **guard 臂 staging**：payload 从 Dup 副本 move 进自己的 slot（`Dup ; EnumPayload ; StoreLocal`）再求 guard；guard 失败跳下一 test，parked scrutinee 完好。
4. `EnumPayload` 消费 enum（`e → p`），unguarded payload 臂无需 `Pop`；零 payload 臂 `Pop` 掉 parked scrutinee。
5. 全部边立即 resolve，无 deferred jump；[cfg.rs:133](../fleen-compiler/src/lower/cfg.rs#L133) `finish()` 断言 pending 全 resolve。

⚠️ **注释过期**：[mod.rs:2322](../fleen-compiler/src/lower/mod.rs#L2322) 的 doc 说 guard staging 用 "hidden temp slot + LoadLocal tmp"，实际实现（[mod.rs:2476](../fleen-compiler/src/lower/mod.rs#L2476)）是直接 `StoreLocal` 进 binding slot。T06 重写时一并修正注释。

---

## 四、重难点（按难度排序）

### 1. park 纪律下的失败清理边（最难）
嵌套链打破"失败点只 park 了 scrutinee"的不变量：`Ok(Some(x))` 外层测试成功后外层 payload 被提取**重新 park**，内层 `EnumIs` 运行时**真的可能失败**（typeck 只验证良构，不保证匹配——`Ok(None)` 会走到这条边），失败边必须先 `Pop` 重 park 的内层 payload 再跳下一外层测试。每个失败点要知道自己脚下多 park 了几个值并逐一清理。当前代码无此边类型，是所有栈深 bug 的根源。

### 2. or-pattern 的栈深归一化
`Some(Some(x)) | Other(x)`：备选 1 成功路径 park 2 个值，备选 2 只 park 1 个。两者跳到**同一个**共享 body/guard 点时入口栈深不同，[tests.rs:1425](../fleen-compiler/src/lower/tests.rs#L1425) 的 `assert_stack_depths_consistent`（worklist 求解器）直接报 inconsistent entry depth。出路：**每个备选的提取链在自己块内完成全部递归提取并清掉自己的中间 park 值**，使所有路径到达共享点时栈深归一。提取必须内联在各备选里（不同变体 binding 的 payload 下标不同，`Some(x) | Wrapper(_, x)`），不能放共享 body。

### 3. `EnumPayload` 消费性 + 多 payload 提取编排
`Pair(a, b)` 需要 payload 0 和 1，但每次 `EnumPayload` 都 pop enum。要么 `Dup; EnumPayload v,0; StoreLocal a; EnumPayload v,1; StoreLocal b`（最后一次消费原件），要么统一 Dup 提取后补 `Pop`。与字面量/wildcard 子模式（跳过提取）、嵌套、or 叠加后，任何 Dup/Pop 配错就是栈下溢或悬空值。

### 4. 每层 Dup vs DupDeep 需 payload 类型，而 `TypedPatternHir::Enum` 不带类型
现有代码只在 scrutinee 层算一次 `is_owned()`（[mod.rs:2338](../fleen-compiler/src/lower/mod.rs#L2338)）。嵌套时重 park 的内层值可能 owned（`Result[Option<string>, string]` 的内层 `Option<string>`），外层判断不够。建议 lower 从 scrutinee 的 `Type::App(info, args)` / `Type::Result(T, E)` 自己做参数代入重算 payload 类型（`EnumInfo.payloads` + `Type::Param(i)` → args[i]），属"重算"而非新语义；备选是让 typeck 在模式节点补 payload 类型（改 T05 已落地代码，代价大）。

### 5. 块分配顺序即 CFG
嵌套链引入 alt_extract / inner test / cleanup / guard_ok 四类新块，数量 O(深度×备选数)，物理顺序错一处 fall-through 就落进错误的块。`finish()` 断言能抓住漏边。

### 6. or-pattern 与 resolver 共享 BindingId
T05 as-built：or 各备选同名 binding 共享第一个备选的 `BindingId`。lower 义务：每个备选各自 `EnumPayload vᵢ, j; StoreLocal` 进**同一个 slot**，然后共享 guard 求值点（per-arm，备选链都跳到它）→ 失败跳下一测试（parked 完好）/ 成功走 guard_ok 块（Pop 全部 park 值再进 body）。需专门 hand-checked 单测钉住"多写一 slot、单运行时路径"。

### 7. guard × 嵌套 × or 三者叠加
0.0.6 staging 先例：payload 从 Dup 副本 move 进 slot、guard 失败跳下一测试。嵌套后**每个**内层 binding 都要走各自提取链 stage 进 slot；guard_ok 与 guard 失败两条路径都要清理中间 park。可控因素：PLAN §8 明确 guard 只允许出现在臂级（`Some(x if x>0)` 非法），子模式里不会有 guard，递归 helper 不用处理这个分支。

### 8. 三条路径复用一个递归 helper
generic choose 用 `pending + chain_done`、Result choose 用 Ok/Err 两路、enum choose 用 test 链 + fallback，三套边约定不同。需统一 `lower_pattern_chain(pattern, parked 层次, on_fail 边, 共享 body)` 递归 helper：`Ident` → 提取+StoreLocal；`Literal` → Dup+Eq 测试；`Enum/ResultCtor` → EnumIs 测试+提取重 park+递归；`Or` → 每备选一条链；`TypeGuard` → 纯 erase。所有失败边统一经 helper 清理。

---

## 五、建议落地顺序

1. **D4 多绑定**（单层，最接近现状：替换 `first_payload_binding` 为递归收集 binding + Dup 编排提取）；
2. **D5 嵌套链**（引入 park 层次与失败清理边，结构性最大改动）；
3. **D6 or-pattern**（引入共享 body 与栈深归一化，依赖 2 的清理机制）；
4. **D7 type guard**（纯 erase，放最后）。

每步跑 `assert_stack_depths_consistent` + 现有 2916 行 lower 单测回归。票面要求的四张 hand-checked fixture（多绑定臂 / 2 层嵌套链 / 双备选 or / 嵌套内 guard）正好对应这四步的验收。

## 六、验收信号

- 票面 [TICKETS.md:251](../docs/0.0.7/TICKETS.md#L251) 四张 hand-checked fixture 全部通过；
- `assert_stack_depths_consistent` 在所有新 fixture 上过；
- `cargo test -p fleen-compiler`（现有 2916 行 lower 单测）无回归；
- e2e fixture（T09 落地）：`nested_pattern.fln`、`or_pattern.fln` 输出符合 expected；
- 三道闸门（[mod.rs:578](../fleen-compiler/src/lower/mod.rs#L578) / [2359](../fleen-compiler/src/lower/mod.rs#L2359) / [2099](../fleen-compiler/src/lower/mod.rs#L2099)）全部拆除或收窄为真正不可达的防御分支。

## 七、未读确认点（实现前需核对）

- `Ok/Err` 重复臂的报错位置（grep 未命中 `has_ok|has_err` 附近的重复检查，可能在 [infer.rs:2514](../fleen-compiler/src/typeck/infer.rs#L2514) 附近，需确认 lower 无需重查）；
- `Type::Param(i)` 在 `EnumInfo.payloads` 中的下标语义（是否声明级 i-th 参数），代入 use-site args 时需一致；
- `BindMatch` 与 `StoreLocal` 在新路径的选用边界（是否仅 Ident-only pattern 仍走 `BindMatch`，嵌套/binding 提取改用 `StoreLocal`）。
