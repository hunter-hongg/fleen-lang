# Website — The Fleen Official Site (Tickets)

> Companion to `docs/website/PLAN.md`. One card = one unit of work, closed with
> its own build/link check. Decisions D1–D10 referenced here are **settled at
> planning level** (see PLAN §1); a card may reopen one only by adding a new
> decision entry, not by editing it silently.
>
> **Status: T01–T19 not started.** No file exists under `website/` yet; this
> document is the work breakdown only. Nothing here changes the language: no
> syntax, no opcode, no `std` module, no version number. The site is a
> presentation layer over the repository (PLAN D2).
>
> **Standing constraints on every card**
> - **Static only.** No SSR adapter, no server process, no database, no
>   account system (PLAN D1, D10).
> - **No second content base.** A page renders repository Markdown or
>   repository-derived data; the only hand-written data permitted is
>   `release/<version>/meta.json` (PLAN D2).
> - **Latest-release truth comes from the repo.** Version strings are injected
>   at build time from `Cargo.toml` + `std/VERSION`; nothing is typed into a
>   template by hand.
> - **The build is the check.** A card is not done when the page renders; it is
>   done when `astro build` plus the assertions added in that card pass on CI
>   (PLAN D9).

**Milestone → ticket map**

| Milestone | Tickets |
|-----------|---------|
| M0 内容盘点 | T01, T02, T03 |
| M1 骨架 | T04, T05, T06, T07, T08, T09 |
| M2 内容页 | T10, T11, T12, T13, T14, T15 |
| M3 演练场 | T16, T17 |
| M4 打磨 | T18, T19 |

---

## T01 — 版本事实的单一来源 + 内容基线检查 (D9)

- [ ] 建立"版本事实"读取层：从 `Cargo.toml` 的 `version` 与 `std/VERSION`
      读取版本号，从 `CHANGELOG.md` 头部读取代号，作为站点唯一的版本来源
      （PLAN D2 的最后一行）。
- [ ] CI 断言一：五处版本号一致 —— `Cargo.toml`、`std/VERSION`、
      `CHANGELOG.md` 最新条目、`release/` 下最大目录名、`lang_docs/` 下最大
      目录名。当前基线即不一致（`lang_docs` 到 0.0.10，其余到 0.0.11），
      断言落地后先暴露、再由 T02 修平。
- [ ] 修正 `README.md` 顶部 "Current version **0.0.10**" 与实际 0.0.11 的偏差；
      并在 SPEC §13 版本段补一句"版本号五处一致由 CI 保证"，使这条规则可被
      人读到，而不只活在 CI 里。
- [ ] 记录当前基线快照（版本号、代号、发布产物数、`lang_docs` 覆盖范围），
      写入本票据的 note，供后续回看。

**Depends on**: nothing.
**Refs**: PLAN §1 D9, §2, §8; PLAN §13.2 风险表首行。

---

## T02 — `lang_docs/0.0.11` 中英双语补齐 (D2, D4)

- [ ] 以 `lang_docs/0.0.10/{zh,en}` 为模板，补齐 `lang_docs/0.0.11/{zh,en}`
      的 16 章 + `README.md`，内容覆盖 0.0.11 的实际变更：`map { k -> v }`
      字面量、`m[k]` 返回 `Result[V, string]`、键白名单 `{int, bool, string}`、
      八个 `std.map` 内建、`insert`/`remove` 的消费语义、插入序保留、`map`
      成为保留字。
- [ ] 对既有章节做增量修订，而不是整章重写：`04-types` 增 `map[K, V]`，
      `05-operators` 增索引后缀的 map 分支，`13-standard-library` 增
      `std.map`，`15-appendix` 增保留字表一行。
- [ ] 双语对齐：`zh` 与 `en` 的章节数量与文件名逐一相同（D9 断言三）。
- [ ] 更新 `lang_docs/0.0.11/README.md` 的章节索引与"本版破坏性变更"一节。

**Depends on**: T01（基线断言先落地，补齐后断言转绿，才算闭环）。
**Refs**: PLAN §2 结论 3, §8.1; `release/0.0.11/RELEASE_NOTES.md`。

---

## T03 — `release/<版本>/meta.json` 结构化元数据 (D2)

- [ ] 定义并落地 `release/<版本>/meta.json` 的最小字段集：
      `version`、`codename`、`emoji`、`status`（`released` / `implemented` /
      `planned`）、`bytecode_version`、`breaking_changes[]`
      （每条 `{ item, decision_ref }`）。
- [ ] 为 `0.0.1`–`0.0.11` 各写一份。历史取值以各版本 `RELEASE_NOTES.md`
      与 `CHANGELOG.md` 为准，不凭记忆填写；写入前逐条核对原文。
- [ ] 校验脚本：字段齐全、枚举值合法、`breaking_changes[].decision_ref`
      指向真实存在的 `docs/<版本>/PLAN.md` 决策编号。
- [ ] 不引入 JSON schema 校验库以外的依赖；脚本与其断言并进 CI（D9）。

**Depends on**: nothing（可与 T01 并行）。
**Refs**: PLAN §1 D2, §7.6, §8。

---

## T04 — `website/` 骨架与 CI 构建 (D1, D9)

- [ ] 初始化 `website/`（Astro + Starlight），提交 `package.json`、
      `astro.config.mjs`、最小的 `src/pages/index.astro`。
- [ ] 锁定静态输出：不使用任何 SSR adapter；构建产物为纯静态目录。
- [ ] 新增 `.github/workflows/site.yml`：安装依赖 → `astro build` → 链接与
      断言检查 → 上传产物。Node 版本与缓存策略与既有 `ci.yml` 保持一致。
- [ ] 记录构建产物体积基线（首次可行构建后写入本票据 note），后续卡对照。

**Depends on**: nothing.
**Refs**: PLAN §1 D1, §10 技术选型表, §11。

---

## T05 — 文档路由：`/docs` 由 `lang_docs` 生成 (D2)

- [ ] 构建期读取 `lang_docs/<最新版本>/{en,zh}/01..16`，按文件名前缀的序号
      排序生成侧栏；`README.md` 作为 `/docs` 首页。
- [ ] 章节标题取自 Markdown 一级标题，不另立一份目录清单（否则即第二份内容源）。
- [ ] 页内锚点、上一章/下一章、代码块复制按钮。
- [ ] 新增 `/docs/quickstart`：内容取自 `README.md` 的安装与 `flint new`
      段落，不新写一份上手文档。
- [ ] 断言：16 章全部可访问；任一章节缺失则构建失败（D9）。

**Depends on**: T02, T04.
**Refs**: PLAN §2, §7.1, §7.2, §8 单一事实来源表。

---

## T06 — 版本选择器、旧版本横幅与 canonical (D3)

- [ ] 版本列表来自 `lang_docs/` 目录扫描，只列出真实存在的版本。
- [ ] `/docs/<version>/<章节>` 作为旧版本固定路径；`/docs/<章节>` 指向最新。
- [ ] 旧版本页面顶部横幅："你正在看 <旧版本> 的文档，最新为 <当前版本>"，
      链接指向同章节的最新版。
- [ ] 旧版本页面的 `canonical` 指向最新版对应页；`/docs/<章节>` 自身
      canonical 指向自己。
- [ ] 断言：每个 `/docs/<章节>` 在旧版本路径下都存在同名页，否则构建失败。

**Depends on**: T05.
**Refs**: PLAN §1 D3, §8.1。

---

## T07 — 双语路由与 hreflang (D4)

- [ ] 英文无前缀（`/docs/...`）、中文 `/zh/docs/...`；语言切换保留当前页面
      与当前版本。
- [ ] 每页输出 `hreflang="en"` 与 `hreflang="zh-CN"` 双向声明。
- [ ] 断言：`zh` 与 `en` 章节数量一致；缺一侧时该页隐藏语言切换并让构建
      失败的策略二选一，并在本票据 note 记录所选。
- [ ] 语言切换不通过 cookie 或客户端脚本实现（D4 拒绝项）。

**Depends on**: T05.
**Refs**: PLAN §1 D4, §8.1。

---

## T08 — Pagefind 搜索 (D6)

- [ ] 在 `astro build` 之后运行 Pagefind 索引，产出 `_pagefind/` 静态资源。
- [ ] 搜索框接入 Starlight 的搜索钩子；中文内容使用中文分词/索引设置。
- [ ] 搜索结果按版本分区，默认只搜当前最新版。
- [ ] 首屏 JS 增量测量：搜索框不得把文档页 JS 推过 100KB（gzip）预算。

**Depends on**: T05, T07.
**Refs**: PLAN §1 D6, §10, §11 性能预算。

---

## T09 — 首页七段结构 (D2)

- [ ] Hero、语言一览、工具链流水线、五分钟上手、版本带、诚实的现状、页脚。
- [ ] 代码块内容取自 `examples/` 或 `README.md`，不新写示例。
- [ ] 安装命令与版本带由构建期版本注入生成，模板内不出现字面量版本号。
- [ ] 工具链图用 HTML/CSS + 内联 SVG 实现，不引入图表库。
- [ ] 页面在窄屏（375px）下不出现横向滚动。

**Depends on**: T04, T01。
**Refs**: PLAN §6, §4 视觉方向。

---

## T10 — Fleen TextMate 语法 + Shiki 集成 (D5)

- [ ] 编写 `fleen.tmLanguage.json`：关键字、类型、字面量、注释、字符串插值
      `${…}`、`\$` 转义。
- [ ] 关键字集合**从编译器的关键字表生成**（`fleen-compiler/src/lexer/`），
      语法文件里不手写关键字列表；新增保留字时重新生成。
- [ ] 断言四（D9）：生成的语法文件关键字集 == lexer 关键字表；不一致则
      构建失败。
- [ ] 用 `examples/0.0.11/` 的真实文件目视抽查高亮：字符串插值、`map` 字面量、
      `choose` 模式、`#feature curry` 指令行。

**Depends on**: T04。
**Refs**: PLAN §1 D5, §10 技术选型表。

---

## T11 — 下载页 `/download` (D2)

- [ ] 构建期扫描 `release/<最新版本>/` 目录，生成六目标表（`x86_64-linux-gnu`、
      `x86_64-linux-musl`、`aarch64-linux`、`x86_64-macos`、`aarch64-macos`、
      `x86_64-windows`），文件名与哈希不手写。
- [ ] 每行：产物名、大小、SHA256、可复制的下载/校验命令。
- [ ] 附 std 解析顺序说明（`FLINT_STD_PATH` → `$FLINT_HOME/std/<version>/`
      → 编译期内嵌）与"版本不匹配是硬错误"。
- [ ] 断言：六目标齐全；缺任一目标则构建失败（D9）。

**Depends on**: T04, T01。
**Refs**: PLAN §7.7, §8。

---

## T12 — 发布记录页 `/releases` 与 `/releases/<版本>` (D2)

- [ ] 列表页：版本号、代号 + emoji、日期、一句话摘要、是否含破坏性变更
      （数据来自 T03 的 `meta.json`）。
- [ ] 详情页：渲染 `release/<版本>/RELEASE_NOTES.md`，末尾追加结构化字段
      （破坏性变更集合、字节码版本、发布产物数）。
- [ ] 列表按版本倒序；`status` 为 `planned` / `implemented` 的版本加状态徽章，
      不得呈现为已发布。
- [ ] `/releases/<版本>` 页面输出结构化数据（D9 的 SEO 要求）。

**Depends on**: T03, T04。
**Refs**: PLAN §7.6, §8。

---

## T13 — 示例库 `/examples` (D2)

- [ ] 构建期扫描 `examples/0.0.1`–`examples/0.0.11`，按版本分组列表。
- [ ] 单例页：带行号源码 + 预期输出 + 对应 `tests/e2e/valid/<名称>.expected`
      的链接。
- [ ] 精选位：`examples/0.0.11/tour/`，展示 `flint.toml`、`src/` 目录树与
      跨模块调用片段。
- [ ] 多文件示例（`tour`）明确标注"需 `--project` / `flint run`，浏览器内
      演练场暂不支持"，避免与 T17 的能力混淆（D8）。

**Depends on**: T04。
**Refs**: PLAN §7.5, §8。

---

## T14 — 标准库参考 `/docs/std` (D2)

- [ ] 解析 `std/*.fln` 的导出签名，按模块生成页面：`io` / `str` / `num` /
      `arr` / `collection` / `os` / `time` / `async` / `concurrent` / `map`。
- [ ] 每项标注：签名、是否 intrinsic、是否为 0.0.x 新增（取自 `CHANGELOG.md`）。
- [ ] 版本号取自 `std/VERSION`；与 `Cargo.toml` 不一致时构建失败（复用 T01 断言）。
- [ ] 不手抄任何签名；解析失败（无法识别的导出形式）时构建失败而非静默跳过。

**Depends on**: T05。
**Refs**: PLAN §7.3, §8。

---

## T15 — 路线图 / 实现内幕 / 贡献指南 (D2, D10)

- [ ] `/roadmap`：由 `docs/<版本>/PLAN.md` 与 T03 的 `meta.json` 生成时间线；
      `Generics` / `Operator overloading` / `FFI` 标为"0.1.0 之后"，不给日期。
- [ ] `/internals` 三页：编译流水线（九个阶段的输入输出）、字节码与校验器
      （v1→v9 演进 + 内容驱动版本规则）、虚拟机（栈式执行、帧布局、错误行列号）。
      内容以 `docs/DESIGN.md`、`docs/BYTECODE.md`、`SPEC.md` 为源。
- [ ] `/contribute`：从 `SPEC.md` 提炼分支模型、提交信息规范、四道门
      （`fmt` / `clippy -D warnings` / `test` / `doc`）、测试目录约定、修 bug
      先写测试；并显式提醒"内部文档先商定决策再落盘"（`LESSON.md` L1）。
- [ ] 三页均不出现"已支持"语气的未实现特性；每处能力陈述附带版本或 `planned` 标记。

**Depends on**: T04, T03。
**Refs**: PLAN §7.8, §7.9, §7.10。

---

## T16 — 演练场 P1 + wasm 执行器骨架 (D7, D8)

- [ ] 用 `wasm-pack` 把 `fleen-compiler` + `fleen-vm` 编译为 `wasm32-unknown-unknown`，
      暴露 `run(source) -> { stdout, stderr, exit_code }`。
- [ ] **Web Worker 执行器**：主线程不执行任何编译器代码；超时（2s）后
      `worker.terminate()` 并重建 worker。无限 `while` 必须可回收，这是本卡的
      第一验收项。
- [ ] 桩函数（wasm 侧实现，不改 VM 语义）：`read_line` 返回空串、
      `std.io.sleep` 立即返回、`os.args` 为空数组。
- [ ] std 使用编译期内嵌副本，运行时不发起任何网络请求。
- [ ] P1 界面：首页 3–5 个固定片段的"运行 → 看输出"，不做编辑器。
- [ ] 验收：`while true {}` 片段在 2s 内被终止，页面仍可再次运行其它片段。

**Depends on**: T09。
**Refs**: PLAN §1 D7, D8, §9.1, §9.2。

---

## T17 — 演练场 P2：`/play` 编辑器与诊断 (D8)

- [ ] `/play` 全屏页：左编辑器、右输出（stdout + 退出码 + stderr）。
- [ ] 错误诊断：把编译器的 `Diagnostic` 映射为行列号 + 消息 + 可点击定位。
- [ ] 预置示例下拉：`fib`、`choose` 穷尽匹配、`move`/`clone` 所有权、
      `async` 双任务、`std.map`、一个故意报错的样例。
- [ ] "Open in playground" 从文档/示例页携带代码跳转（T05、T13 的代码块）。
- [ ] 常驻提示"代码在浏览器内本地执行，不离开设备"。
- [ ] 仍然只支持单文件；`import` 输入给出明确提示而非静默失败。

**Depends on**: T16, T13。
**Refs**: PLAN §7.4, §9.2 P2, §9.3。

---

## T18 — 演练场 P3：多文件与分享 (D8)

- [ ] 虚拟文件系统（VFS）：内存中的多文件项目，路径与 `import` 规则一致。
- [ ] 编译入口切换到项目模式，与 `flint`/`--project` 的解析顺序保持同一语义。
- [ ] URL 编码源码分享：`#` 片段或压缩查询参数，长度上限与降级策略需在
      本票据 note 中记录。
- [ ] 明确不实现：`flint.toml` 的 git 依赖解析、`FLINT_HOME` 缓存、跨项目导入。

**Depends on**: T17。
**Refs**: PLAN §1 D8, §9.2 P3, §9.3。

---

## T19 — 质量门收尾：a11y、性能预算与决策交叉链接 (D9)

- [ ] 链接与锚点检查（含 `/zh/` 镜像与旧版本路径）接入 CI，坏链接即失败。
- [ ] 视觉与 a11y：暗/亮主题对比度 ≥ 4.5:1、跳过导航链接、代码块键盘可复制；
      窄屏（375px）无横向滚动。
- [ ] 性能：文档页 JS ≤ 100KB（gzip）；演练场独立代码分割，不进入文档页。
      **记录实测值**，不以"应该达标"代替测量。
- [ ] 决策交叉链接：支持 Markdown 中的 `<!-- decision: D3 -->` 标记，渲染为
      指向 `docs/<版本>/PLAN.md` 对应决策的链接；至少为语言参考中每章的
      非平凡规则接入一次。
- [ ] 四大 Lighthouse 项 ≥ 95，结果记入本票据 note（性能/a11y/最佳实践/SEO）。

**Depends on**: T05–T18 全部。
**Refs**: PLAN §1 D9, §11, §12 M4。

---

## 完成后

- [ ] 在 `README.md` 增加官网入口链接（发布后）。
- [ ] 在 `SPEC.md` §1 仓库结构树中登记 `website/`，保持结构与现实一致。
- [ ] 本文件与 `docs/website/PLAN.md` 的状态行同步更新为已实现。

**Refs**: PLAN §14 结论；`SPEC.md` §1。
