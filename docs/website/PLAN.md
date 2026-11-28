# Website — The Fleen Official Site (PLAN)

> **Goal**: one static site that turns the repository into a readable interface —
> the 16-chapter language reference (bilingual, per-version), the examples, the
> release notes, the download artifacts, the bytecode spec, and a browser-side
> playground. No account, no backend, no database; the only dynamic capability
> runs inside the user's own browser.
>
> **Theme**: the project's assets already exist (`lang_docs/`, `examples/`,
> `release/`, `std/`, `docs/DESIGN.md`, `docs/BYTECODE.md`, `SPEC.md`). What is
> missing is routing, rendering, versioning, translation switching, and a
> playground. The site is therefore a **presentation layer over existing
> Markdown**, not a second content base.
>
> **Status**: planning. Decisions D1–D10 below are settled at planning level;
> each names the alternative it rejected and what the choice locks. The work
> breakdown lives in `docs/website/TICKETS.md` (T01–T19, **none started**).
> They may be revised only by a new entry here, not silently in a ticket card.
>
> **What this is not**: no content management system, no user accounts, no
> comments, no forum, no backend execution of Fleen code, no third-party
> analytics, no rewritten marketing copy that duplicates the repository's
> Markdown. This document designs the site; it does not implement it.

---

## 0. Relationship to adjacent work

This is a **repository-level** effort, not a language milestone: it adds no
syntax, no opcode, no `std` module, and no version number of its own. The site
tracks language versions instead of being one.

| Item | Scope |
|------|-------|
| 0.0.1 – 0.0.11 | the language releases the site must be able to present |
| `lang_docs/0.0.4` – `0.0.10` | the only versions with documentation today (16 chapters, zh + en) — the version switcher may list only these until T02 lands |
| **website** | M0 content baseline → M1 skeleton → M2 content pages → M3 playground → M4 polish |
| 0.1.0 (language) | unrelated to site scope; the roadmap page merely marks `Generics` / `Operator overloading` / `FFI` as "after 0.1.0" without dates |

**Nothing is blocked by the site, and the site blocks nothing.** The one
coupling is content freshness: the site renders whatever `lang_docs/` contains,
so a release without documentation shows up as a hole (see D2 and T02).

---

## 1. Settled decisions (D1–D10)

### D1 — The site lives in this repository, built with Astro + Starlight, output static

`website/` is a directory in the same repo. Astro + Starlight is the framework:
it ships a documentation layout (sidebar, i18n, version routes, search hook) out
of the box and emits pure static files.

- **Rejected (a separate `fleen-website` repo)**: every release would then need
  a cross-repo sync of version numbers and Markdown, which is exactly the drift
  this plan is trying to prevent.
- **Rejected (VitePress)**: lighter, but multi-version documentation and i18n
  routing would be hand-built — work that Starlight already does.
- **Locks**: the build is `astro build` over a Node toolchain; no SSR adapter is
  ever added, so "static only" is a standing constraint on every later ticket.

### D2 — Repository Markdown is the only content source; hand-written data is isolated

`/docs/*` renders `lang_docs/<version>/<lang>/*.md`; `/releases/<version>`
renders `release/<version>/RELEASE_NOTES.md`; `/examples` is generated from
`examples/<version>/`; `/docs/std` is derived from `std/*.fln` + `std/VERSION`;
the version number and codename are injected from `Cargo.toml` + `std/VERSION`.

- **The only permitted hand-written data** is one small `meta.json` next to each
  release's notes (declared breaking set, bytecode version, codename, status),
  because that structure cannot be recovered from prose.
- **Rejected (a `website/content/` copy of the docs)**: guarantees two sources
  and guarantees they diverge.
- **Locks**: a page whose data has no repository source is out of scope until
  that source exists — which is why M0 (content) precedes M1 (site).

### D3 — Version routing: `/docs` is always the latest stable, old versions are pinned paths

- `/docs/<chapter>` serves the newest version present in `lang_docs/`.
- `/docs/<version>/<chapter>` serves that exact version and stays stable as a
  permalink.
- The version switcher lists **only versions that exist on disk** (today
  0.0.4–0.0.10; 0.0.11 appears after T02) — never a version that would 404.
- Old-version pages carry a banner naming the current version, and their
  `canonical` points at the latest equivalent page so stale pages do not win
  search results.
- **Rejected (one URL per version with a redirect from the bare path)**: it
  makes the common case pay a hop and makes "latest" ambiguous at build time.

### D4 — Bilingual by URL prefix: English unprefixed, Chinese under `/zh/`

`/docs/...` is English (the repository's default language), `/zh/docs/...` is
Chinese, with reciprocal `hreflang` and a language toggle that preserves the
current page. Both trees are built from `lang_docs/<version>/{en,zh}/`.

- **Rejected (a cookie or client-side switch)**: unshareable URLs and no
  crawlable Chinese pages.
- **Locks**: every content page needs both language versions to exist, or the
  toggle is hidden for that page — enforced by D9's CI assertion.

### D5 — Syntax highlighting is Shiki with a hand-written Fleen TextMate grammar, generated from the lexer's keyword list

There is no published `.fln` grammar, so the site owns one, covering keywords,
literals, types, comments, and `${}` interpolation. The keyword section is
**generated from the compiler's keyword table** and checked in CI, so a new
reserved word never silently renders as plain text.

- **Rejected (reuse the Rust grammar)**: wrong keywords, wrong string rules
  (`"${expr}"` is Fleen-specific), and it silently drifts from the language.
- **Rejected (server-side highlighting at build via a Rust highlighter)**:
  adds a second toolchain to the build for no user-visible gain.

### D6 — Search is Pagefind, built at build time, no search backend

Pagefind indexes the generated HTML during `astro build`; the client downloads
only the indexes it needs. Zero cost, zero service, works on any static host.

- **Rejected (Algolia DocSearch)**: an application process and an external
  dependency for a documentation site whose entire corpus is a few hundred pages.

### D7 — The playground compiles `fleen-compiler` + `fleen-vm` to wasm and runs it in a Web Worker with a hard timeout

One entry point `run(source) -> { stdout, stderr, exit_code }`. The worker is
**terminated and recreated on timeout (2s)** — non-negotiable, because the
language permits `while true` and a playground without terminate wedges the tab.

- **Rejected (executing on a server)**: needs a sandbox, a machine, and money,
  and turns a static site into a service.
- **Rejected (no timeout)**: a single hung snippet makes the page unusable.
- **Locks**: all Fleen execution happens inside the browser; std is the
  compile-time embedded copy, never fetched at runtime.

### D8 — Playground scope is staged P1 → P2 → P3, and stops at single-file execution until P3

- **P1**: 3–5 fixed snippets on the home page with a "run → see output" button.
- **P2**: a full `/play` with an editor, diagnostics (line/column + message),
  and preset examples.
- **P3**: a virtual file system for multi-file projects and URL-encoded sharing.
- **Out of scope at every stage**: `read_line` returns an empty string,
  `std.io.sleep` returns immediately, `os.args` is empty, and no `flint`/git
  dependency resolution exists in the browser.
- **Rejected (multi-file from the start)**: `import` resolution plus a VFS is a
  larger job than the entire P1+P2, and it is not needed to evaluate the language.

### D9 — Quality is a build gate: broken links, version agreement, and zh/en parity fail the build

CI runs `astro build`, then asserts: (a) no broken internal links or anchors;
(b) every version string in the repo agrees (`Cargo.toml`, `std/VERSION`,
`CHANGELOG.md` head, latest `release/` dir, latest `lang_docs/` dir);
(c) `lang_docs/<version>/zh` and `.../en` have the same chapter count; (d) the
generated highlight grammar's keyword set equals the lexer's.

- **Rejected ("warn on broken links")**: a docs site whose value is its links
  cannot ship known-broken ones.
- **Locks**: a release that forgets to update one of the five version locations
  turns the site build red — the drift source is removed structurally.

### D10 — No tracking, no accounts, no backend: the first version's scope is closed

No analytics script, no cookies, no login, no comment system, no CMS, no online
sharing service. If traffic numbers are needed, the hosting provider's aggregate
view is enough.

- **Rejected (a lightweight self-hosted analytics beacon)**: it is still a
  decision the project has not made, and it is not needed to ship a docs site.

---

## 2. 项目现状盘点（方案的事实基础）

官网不是凭空设计的，它必须先和仓库里已经存在的资产对齐。下表是当前仓库可直接用于官网的内容：

| 资产 | 位置 | 官网用途 | 现状 |
|------|------|----------|------|
| 语言设计稿 | `docs/DESIGN.md` | "为什么这样设计" / 实现内幕 | 完整，含各版本 D 编号决策 |
| 语法定义 | `docs/SYNTAX.ebnf` | 语法参考页、语法高亮规则来源 | 完整 |
| 字节码规范 | `docs/BYTECODE.md` | 实现内幕、`.flnc` 格式说明 | 完整（含 v9 注记） |
| 语言参考（双语） | `lang_docs/<版本>/{zh,en}/01..16` | **文档站主体** | 0.0.4–0.0.10，每版 16 章 |
| 版本计划与票 | `docs/<版本>/{PLAN,TICKETS}.md` | 路线图、"为什么这么做" | 0.0.1–0.0.11 |
| 发布说明 | `release/<版本>/RELEASE_NOTES.md` | 发布记录页 | 0.0.1–0.0.11 |
| 发布产物 | `release/0.0.11/*.tar.gz` / `.zip` | 下载页 | 六个目标全在 |
| 可运行示例 | `examples/0.0.1`–`0.0.11` | 示例库、演练场预置 | 11 个版本目录 |
| 工程型 demo | `examples/0.0.11/tour/` | "真实项目长什么样" | 多模块 + `flint.toml` + 测试用例 |
| 标准库源码 | `std/` | std 文档（由源码生成） | 编译期内嵌；`std/VERSION` = 0.0.11 |
| 工程规范 | `SPEC.md`（`AGENTS.md`/`CLAUDE.md` 为符号链接） | 贡献指南、能力矩阵 | 含 §15 特性矩阵 |
| 变更记录 | `CHANGELOG.md` | 版本历史 | 0.0.1–0.0.11 |

关键结论：

1. **文档已经是双语的、已经按版本分目录的**，官网文档系统不需要新建内容体系，只需要一个渲染层和一层版本/语言路由（D2、D3、D4）。
2. **版本号在仓库里出现五处**（`Cargo.toml`、`std/VERSION`、`CHANGELOG.md`、`release/<版本>/`、`lang_docs/<版本>/`），官网必须有一处"版本事实"的单一来源，否则必然漂移（D9）。
3. **已有内容缺口**（M0 前置任务，见 §12 与 T01/T02）：`lang_docs/` 最新只到 `0.0.10`，而 `0.0.11` 已实现并按 `Cargo.toml` / `std/VERSION` 升版；`README.md` 顶部仍写着 "Current version 0.0.10"。

---

## 3. 定位与设计原则

### 3.1 一句话定位

> **Fleen 是一个小型、表达式优先的语言，从源码到手写编译器、字节码校验器、虚拟机全部齐备。**

官网要同时服务三类人，但他们优先级不同：

| 受众 | 想知道什么 | 对应入口 |
|------|-----------|----------|
| 语言好奇者 / 学习者 | "这语言长什么样？五行能不能看懂？" | 首页代码块、演练场 |
| 想用的人 | "怎么装？怎么建项目？std 有什么？" | 下载页、快速开始、文档 |
| 编译器同好 / 贡献者 | "为什么这么设计？字节码怎么编？" | 实现内幕、路线图、贡献指南 |

优先级：**能跑起来 > 好看 > 完备**。首版宁可少一个页面，也不要出现"点进去是空的"。

### 3.2 六条原则

1. **代码是第一文案**。首页首屏必须是可读、可运行的 Fleen 片段，而不是抽象的形容词。
2. **一个概念只写一次**。同一事实（如"`map` 是 0.0.11 的保留字"）只在某处完整陈述，其余位置链接过去（D2）。
3. **诚实标注状态**。凡未实现的设计不得以"已支持"的语气呈现；未发布版本一律标 `planned`。
4. **不假装是成熟语言**。0.0.x 阶段就把"API 不稳定、语义可能变"写在显眼处。
5. **静态优先**。全站可静态构建，无账号、无后端、无数据库；唯一动态能力是浏览器内的演练场（D1、D10）。
6. **与仓库同版本演进**。官网内容不另建一套文案库，直接消费仓库内 Markdown。

---

## 4. 品牌与视觉方向

| 项 | 建议 | 理由 |
|----|------|------|
| 名称 | Fleen | 沿用现有名称，不加副标题式标语 |
| 版本代号体系 | 每个版本一个"动物 + emoji"（🦊 🐚 🦫 🐜 🦔 🦐 🐹 🪡 🕸️ 🗺️） | 仓库已稳定使用，是天然的视觉资产 |
| 主色 | 一个高对比强调色（建议青绿/teal，与"工具链、终端"气质一致）+ 近黑背景的暗色主题 | 文档站长阅读，暗色为主、亮色为辅 |
| 字体 | 正文用系统无衬线栈；代码用 JetBrains Mono / IBM Plex Mono 一类等宽字体，自托管子集 | 代码是主要内容，字号与行高优先于装饰 |
| 图形语言 | 不用插图；用**字节码片段、栈示意图、流水线箭头**作为装饰元素 | 与项目本体一致，且不会过时 |
| 图标 | 版本代号 emoji + 少量线性图标 | 零成本、零版权风险 |

首屏不放大图。留白 + 一段代码 + 一行安装命令，比任何 hero 图都更有说服力。

---

## 5. 信息架构（站点地图）

```
/                              首页
/play                          演练场（全屏）
/docs/                         文档首页（默认最新稳定版）
/docs/quickstart               快速开始
/docs/<章节>                   语言参考（对应 lang_docs/16 章）
/docs/std                      标准库参考
/docs/errors                   错误码与诊断说明
/docs/versions                 版本选择说明
/examples                      示例浏览
/examples/<版本>/<名称>         单例（代码 + 说明 + 输出）
/releases                      发布记录列表
/releases/<版本>                单版本页（= RELEASE_NOTES.md 渲染）
/download                      下载与校验
/roadmap                       路线图
/internals                     实现内幕（编译器/字节码/VM）
/contribute                    贡献指南
/zh/...                        以上全部页面的中文镜像
```

首版**不做**：账号、评论、论坛、CMS、在线分享短链、搜索后端、社区博客聚合（D10）。

### 5.1 为什么文档是站点的中心而不是首页

这个项目的主要资产是 16 章语言参考 + 决策记录，首页只承担"三分钟内决定要不要读下去"的作用。因此路由与导航都以 `/docs` 为主干，首页的新鲜度不做硬要求。

---

## 6. 首页设计（七段结构）

| 段 | 内容 | 目的 |
|----|------|------|
| 1. Hero | 一行定位句 + 一段 `fib` 或 `choose` 代码 + "Run in playground" 按钮 + 安装命令（单行，可复制） | 三秒内看懂这是什么 |
| 2. 语言一览 | 4–6 个卡片：所有权/`Result`/`enum`+`choose`/`async`/`map`/模块与 `flint`。每张卡片一小段真代码 | 展示能力广度，全部可点击进文档 |
| 3. 工具链 | 一张横向流水线图：Lex → ASI → Parse → Resolve → Typeck → Lower → Codegen → Verify → Execute，标注哪一步有独立可执行工具（`fleen-vm` / `fleen-compiler` / `fleen-verify` / `flint`） | 区别于"随便写个解释器"的项目 |
| 4. 五分钟上手 | 三行命令：装 → `flint new` → `flint run`；旁列目录树 | 降低第一次成功的成本 |
| 5. 版本带 | 当前版本卡（版本号 + 代号 emoji + 三个要点）+ 上一个版本 + "全部发布记录" | 让人知道项目在动 |
| 6. 诚实的现状 | 一句 `0.0.x：API 不稳定，语义可能变`，加链接到 `/roadmap` | 建立信任 |
| 7. 页脚 | 文档 / 示例 / 下载 / 贡献 / CHANGELOG / License | 收口 |

安装命令在首屏与 `/download` 必须**由构建时读取版本号生成**，不手写（D2）。

---

## 7. 关键页面设计

### 7.1 文档首页（`/docs`）

- 顶部三段式：**学习路径**（刚接触 → 写第一段代码 → 用所有权 → 写 async）、**参考**（按 `lang_docs` 16 章排序）、**工程**（`flint`、模块、std）。
- 每章一张卡片：标题 + 一句话摘要 + 是否含破坏性变更标记。
- 右侧/顶部固定：版本选择器、语言切换、搜索框。

### 7.2 文档阅读页（`/docs/<章节>`）

- 左侧：当前版本完整目录（16 章 + std + errors）。
- 右侧：页内锚点（H2/H3）。
- 页头：版本徽章 + 语言切换 + "本页最后对应版本"。
- 代码块：Shiki 高亮 + 复制按钮 + 可选的 "Open in playground"（把代码块内容塞进演练场）。
- 章末：上一章 / 下一章 + "本章的决策来源"链接（指向 `docs/<版本>/PLAN.md` 的具体小节）。

> 最后这一点是本方案的差异化：**语言文档的每个非平凡规则，都能一键跳到它当初的决策记录。** 实现方式是在 Markdown 里用约定的自定义标记（如 `<!-- decision: D3 -->`）生成链接，不改正文。

### 7.3 标准库参考（`/docs/std`）

- 数据来源：`std/*.fln` 源码 + `std/VERSION`。
- 每个模块一页：`io` / `str` / `num` / `arr` / `collection` / `os` / `time` / `async` / `concurrent` / `map`。
- 每个导出项：签名、是否为内建（intrinsic）、示例、是否为 0.0.x 新增。
- 因为 std 是 Fleen 自己写的，签名要从源码解析生成，不手抄——否则每版必然漏。

### 7.4 演练场（`/play`，详见 §9）

- 左侧编辑器，右侧输出（stdout + 退出码）。
- 预置示例下拉：`fib`、`choose` 穷尽匹配、所有权 `move`/`clone`、`async` 双任务、`std.map`、`errors`。
- 顶部常驻提示："在浏览器内本地执行，代码不离开你的设备。"

### 7.5 示例库（`/examples`）

- 按版本分组的列表，对应 `examples/0.0.1`–`0.0.11`。
- 单例页：源码（带行号）+ 预期输出 + "在演练场打开" + 该示例对应的 `tests/` 用例链接。
- 精选位：`examples/0.0.11/tour/`，以"一个多模块项目"的形态展示，附目录树与 `flint.toml`。

### 7.6 发布记录（`/releases`、`/releases/<版本>`）

- 列表：版本号、代号 + emoji、日期、一句话摘要、是否含破坏性变更。
- 详情：直接渲染 `release/<版本>/RELEASE_NOTES.md`，末尾追加"破坏性变更集合"与"字节码版本"两个结构化字段。
- **唯一手写的地方**是这两个字段的元数据（`release/<版本>/meta.json`，D2），其余全自动。

### 7.7 下载页（`/download`）

- 六个目标一张表：`x86_64-linux-gnu`、`x86_64-linux-musl`、`aarch64-linux`、`x86_64-macos`、`aarch64-macos`、`x86_64-windows`。
- 每行：产物名、大小、SHA256、"复制命令"。
- 附两条说明：std 解析顺序（`FLINT_STD_PATH` → `$FLINT_HOME/std/<version>/` → 编译期内嵌）；版本不匹配是硬错误。
- 构建脚本从 `release/<最新版本>/` 目录读取文件名与校验和，文件名与哈希不手写。

### 7.8 路线图（`/roadmap`）

- 纵向时间线：已完成 / 进行中 / 计划中。
- 每个节点：版本号 + 代号 + 一句话主题 + 状态（`released` / `implemented` / `planned`）。
- 明确标注：`Generics`、`Operator overloading`、`FFI` 均在 0.1.0 之后，不给出日期承诺。

### 7.9 实现内幕（`/internals`）

面向编译器爱好者，三篇长文式的页面：

1. **编译流水线**：九个阶段的输入输出，以及"为什么不在解析期做类型检查"。
2. **字节码与校验器**：`docs/BYTECODE.md` 的可读化版本，含 v1→v9 的演进表与"内容驱动版本"规则（只有真正编码了 `NewMap` 的模块才是 v9）。
3. **虚拟机**：栈式执行、帧布局、运行时错误如何携带行列号。

### 7.10 贡献指南（`/contribute`）

- 从 `SPEC.md` 提炼：分支模型、提交信息规范（Conventional Commits，英文）、`cargo fmt` / `clippy -D warnings` / `test` / `doc` 四道门。
- 测试目录约定（`tests/{lexer,parser,resolver,typeck}/` 与 `tests/e2e/{valid,invalid,projects}`）与"修 bug 先写测试"的规则。
- 一处显式提示：内部文档草稿先商定决策再落盘（对应 `LESSON.md` L1）。

---

## 8. 文档系统的单一事实来源

| 官网位置 | 事实来源 | 生成方式 |
|----------|----------|----------|
| `/docs/<章节>` | `lang_docs/<版本>/<lang>/*.md` | 构建期读取，按 16 章顺序生成路由 |
| `/docs/std` | `std/*.fln` + `std/VERSION` | 解析导出签名生成 |
| `/releases/<版本>` | `release/<版本>/RELEASE_NOTES.md` + `meta.json` | 渲染 Markdown |
| `/download` | `release/<最新>/` 目录清单 | 构建期扫描文件名与校验和 |
| `/examples` | `examples/<版本>/` | 构建期扫描，输出与源码并排 |
| 能力矩阵 | `SPEC.md` §15 | 解析表格生成 `/docs/features`（可选） |
| 版本号 / 代号 | `Cargo.toml` + `std/VERSION` | 构建期注入全站 |

**硬规则**：以上任一项都不允许在官网代码里手抄第二份（D2）。凡是需要手写的（发布元数据），单独成一个可 diff 的小文件。

### 8.1 版本与语言策略

- **版本目录独立构建**：`/docs/` 永远指向最新稳定版；`/docs/<版本>/...` 形式固定供旧链接使用（D3）。
- **旧版本的 canonical 指向最新版**（SEO 上避免旧版本抢占）；旧版本页面顶部显示"你正在看 0.0.9 的文档，最新为 0.0.11"横幅。
- **双语用 URL 前缀**：英文默认无前缀（`/docs/...`），中文 `/zh/docs/...`；`hreflang` 双向声明（D4）。
- **版本切换器只列出 `lang_docs/` 真实存在的版本**，避免把用户导向 404（当前即 0.0.4–0.0.10）。

---

## 9. 在线演练场方案（分三期）

难度全部集中在"把编译器搬进浏览器"，因此分期落地，每一期都能独立上线（D8）。

### 9.1 执行模型

- 把 `fleen-compiler` + `fleen-vm` 编译为 `wasm32-unknown-unknown`，暴露一个入口：`run(source: &str) -> RunOutput { stdout, stderr, exit_code }`。
- 运行在 **Web Worker** 中，主线程不阻塞；超时（建议 2s）后 `worker.terminate()` 再重建 worker。**这是必须项**：语言允许无限 `while`，没有 terminate 的演练场会永久卡死页面标签（D7）。
- 交互式输入不做。`read_line` 一律返回空串；`std.io.sleep` 直接返回；`os.args` 返回空数组。所有桩函数在 wasm 侧实现，不改 VM 语义。
- std 走编译期内嵌，不做远程拉取。

### 9.2 三期划分

| 期 | 形态 | 取舍 |
|----|------|------|
| P1 | 首页 3–5 个**固定片段**的"运行 → 看输出"按钮 | 不暴露编辑器，几乎零风险，1 天内可上线 |
| P2 | 完整 `/play`：编辑 + 运行 + 预置示例 + 错误诊断（行列号 + 消息） | 覆盖 90% 使用场景；不含文件/模块 |
| P3 | 单文件 → 多文件（虚拟文件系统，支持 `import`）/ 可分享的 URL 编码源码 | 需要 VFS + 路由压缩，最后做 |

### 9.3 明确不做

- 不做后端执行（无沙箱逃逸风险，也无服务器成本）。
- 不做包管理窗口（`flint` 的 git 依赖在浏览器内无意义）。
- 不做持久化账号（localStorage 存草稿即可）。

---

## 10. 技术选型

| 层 | 选择 | 理由 | 备选 |
|----|------|------|------|
| 站点框架 | **Astro + Starlight** | 文档站开箱即用（侧栏、版本、i18n、搜索），静态输出，无运行时框架负担 | VitePress（更轻，但 i18n/多版本文档需自己搭） |
| 内容 | 直接读仓库 Markdown，不改写 | 避免第二份文案（D2） | — |
| 语法高亮 | **Shiki + 自写 Fleen TextMate 语法** | Shiki 与 Astro 原生集成；`.fln` 目前无现成语法，需按 `docs/SYNTAX.ebnf` 写一份（关键词、字面量、类型、注释、字符串插值 `${}` 五类），关键字表与 lexer 同源（D5） | 先用近似 Rust 高亮临时顶替 |
| 搜索 | **Pagefind**（构建期索引，纯静态） | 零后端、零成本；Starlight 内置支持（D6） | Algolia DocSearch（需申请，首版不必） |
| 演练场 | wasm-pack 产物 + Web Worker | 见 §9 | — |
| 图表 | 纯 HTML/CSS + 少量内联 SVG | 流水线图、栈示意图需求简单，引入图表库不划算 | — |
| 托管 | GitHub Pages 或 Cloudflare Pages（静态） | 免费、支持自定义域、可预览分支 | — |
| CI | 复用现有 GitHub Actions：构建站点 + 链接检查 | 与既有 `ci.yml` 同仓（D9） | — |

> 备注：托管建议为通用判断，未经过服务选型工具核实；站点目录放在同一仓库的 `website/`（D1）。

---

## 11. 质量要求

- **构建即校验**：内链检查 + 锚点检查 + 版本一致性 + zh/en 章节数一致 + 高亮关键字集一致，任一失败即构建失败（D9）。
- **性能预算**：文档页 JS ≤ 100KB（gzip）；演练场单独代码分割，不进文档页。
- **可访问性**：暗/亮主题对比度 ≥ 4.5:1；代码块可键盘复制；跳过导航链接。
- **SEO**：每页 `title`/`description`/canonical/hreflang；`/releases` 输出结构化数据。
- **不追踪**：不加第三方分析脚本；若需度量，用托管平台自带的聚合访问量（D10）。

---

## 12. 里程碑

| 阶段 | 交付 | 完成标准 |
|------|------|----------|
| **M0 内容盘点** | 补齐 `lang_docs/0.0.11`（中英各 16 章）、修正 `README.md` 版本号、`release/0.0.11/meta.json` | 站点所需内容 100% 存在于仓库，无手抄 |
| **M1 骨架** | `website/`（Astro + Starlight）、首页、`/docs` 文档路由、版本切换、双语、Pagefind | 全部文档页可访问，搜索可用，CI 绿 |
| **M2 内容页** | `/download`、`/releases`、`/examples`、`/roadmap`、`/internals`、`/contribute`、`/docs/std` | 所有页面由仓库文件生成，零手动填数 |
| **M3 演练场** | P1 → P2 | 超时可回收、stdin 已打桩、5 个预置示例可运行 |
| **M4 打磨** | 语法高亮完善、a11y 与性能预算达标、P3 多文件演练场、决策交叉链接 | 四大 Lighthouse 项 ≥ 95 |

M0 是硬前置：**文档缺一个版本，官网上就会出现一个"最新版无文档"的空洞**（T01–T03）。

---

## 13. 度量与风险

### 13.1 度量（不引入第三方追踪，取可得指标）

| 指标 | 目标 | 来源 |
|------|------|------|
| 首次成功时间（看到代码 → 本地跑通 `fib`） | < 5 分钟 | 快速开始页的步骤数人工核验 |
| 文档覆盖率 | 已发布版本的每一章都有对应页面 | CI 断言 |
| 坏链接数 | 0 | CI 链接检查 |
| 演练场失败率（非用户代码错误导致的失败） | < 1% | wasm 侧错误分类计数（仅本地聚合） |
| GitHub Release 到官网同步的延迟 | < 1 天 | 人工 |

### 13.2 风险

| 风险 | 影响 | 应对 |
|------|------|------|
| **内容漂移**（版本号在五处、`README.md` 已落后于 `Cargo.toml`） | 官网显示过期信息，损害可信度 | D9 的 CI 版本一致性断言 + T01 |
| `lang_docs` 落后一个版本 | 最新版文档页 404 或空白 | T02 补齐；版本切换器只列真实存在的版本 |
| Fleen 语法高频演进（0.0.x 每版都有破坏性变更） | 旧文档页与旧示例迅速失效 | 版本化目录 + 每版"破坏性变更"结构化字段 + 旧版本横幅 |
| 演练场无限循环 | 浏览器标签卡死 | D7：Web Worker + 超时 terminate |
| 自写语法高亮与语言不同步 | 新关键字不高亮 | D5：语法文件与 lexer 关键字表同源，进 CI |
| 模块导入在浏览器内不可用 | 演练场无法演示 `examples/0.0.11/tour` | P2 明确只支持单文件；多文件放到 P3 |
| 文档双语不同步 | 中文页有而英文页无 | D9：CI 对比 `zh`/`en` 章节数量一致 |

---

## 14. 结论

Fleen 的官网不该是"又一个语言主页"，而应该是**这个仓库的一个可读界面**：文档、示例、发布记录、下载、字节码规范都已经存在，站点要做的是把它们路由、渲染、版本化、双语化，并加一个浏览器内的演练场。

优先级排序（若只能做三件事）：

1. `/docs` 文档站 + 快速开始（把已有 16 章变成可搜索的网站）。
2. `/download` + `/releases`（让人能真的装上当前版本）。
3. 演练场 P1（首页代码块可直接运行）。

其余页面都排在这三件之后。具体拆分见 `docs/website/TICKETS.md`。
