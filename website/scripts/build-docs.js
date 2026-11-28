// T05 — 构建期：把 lang_docs/<最新版本>/{en,zh} 的 16 章 + README 生成为
// Starlight docs 集合（root 语言在 src/content/docs/ 根，zh 在 src/content/docs/zh/），
// 并生成 src/config/sidebar.js（按语言翻译 label，link 不带 locale 前缀，Starlight 自动加）。
// 零第三方依赖；缺章节、语种不一致、章节互链失效时立即退出非零（构建失败）。
//
// 单一事实来源（PLAN D2）：只读 lang_docs，不改写文案（仅把章节互链改写成 Starlight
// slug 路径）。本脚本只清掉它自己生成的文件，绝不触碰 src/content/docs/ 下
// 手写且已提交的页面（如 introduction.md）。
//
// 断言（PLAN D9，构建即校验）：
//   (a) 生成的章节互链都指向真实存在的章节 slug（坏链即失败）；
//   (c) en / zh 章节数一致且逐章序号对齐。
//   （D9(b) 版本号一致性、(d) 高亮关键字集，由 ci.yml 的 versions job 负责。）
import { readdirSync, readFileSync, writeFileSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, '..', '..'); // fleen 仓库根
const WEBSITE_DIR = resolve(SCRIPTS_DIR, '..'); // website/

// quickstart 取自 README 的上手段落（不新写一份上手文档）。该段标题按语种不同
// （en “Running a Fleen program” / zh “运行 Fleen 程序”），含 cargo fln 与
// flint run 的安装/运行说明。
const QUICKSTART_HEADING = { en: 'Running a Fleen program', zh: '运行 Fleen 程序' };
const CHAPTER_COUNT = 16;

function latestVersionIn(langDocsDir) {
  const versions = readdirSync(langDocsDir).filter((d) => /^\d+\.\d+\.\d+$/.test(d));
  if (versions.length === 0) {
    throw new Error(`T05: no versions found under ${langDocsDir}`);
  }
  return versions.sort((a, b) => {
    const pa = a.split('.').map(Number);
    const pb = b.split('.').map(Number);
    for (let i = 0; i < 3; i += 1) {
      if (pa[i] !== pb[i]) return pb[i] - pa[i];
    }
    return 0;
  })[0];
}

function readUtf8(path) {
  try {
    return readFileSync(path, 'utf8');
  } catch (err) {
    throw new Error(`T05: cannot read ${path}: ${err.code ?? err.message}`);
  }
}

function chapterFiles(dir) {
  return readdirSync(dir)
    .filter((f) => /^\d\d-.*\.md$/.test(f))
    .sort((a, b) => a.localeCompare(b));
}

// 章节标题取自 Markdown 一级标题（ticket：不另立一份目录清单）。缺 H1 即失败，
// 不静默回退成 “Untitled”（那会让侧栏出现无意义条目而不被发现）。
function firstH1(md, source) {
  const m = md.match(/^#\s+(.+)/m);
  if (!m) throw new Error(`T05: ${source} has no H1 heading; cannot derive its sidebar title`);
  return m[1].trim();
}

// 把章节内部互链 "01-xxx.md" -> "/01-xxx"（Starlight slug 路径），保留锚点。
// 只改写链接目标，不改动任何文字内容（D2：读仓库 Markdown，不改写文案）。
function rewriteLinks(md) {
  return md.replace(/]\((\d\d-[^)]*?)\.md(#.*)?\)/g, (_m, file, anchor = '') => `](/${file}${anchor || ''})`);
}

// D9(a)：改写后的互链必须指向真实生成的章节。返回失效目标列表（空 = 全部可解析）。
export function brokenCrossLinks(mdFiles) {
  const slugs = new Set(mdFiles.map((f) => `/${f.name.replace(/\.md$/, '')}`));
  slugs.add('/'); // README 首页
  const broken = [];
  for (const { name, body } of mdFiles) {
    for (const [, target] of body.matchAll(/\]\((\/\d\d-[^)#]*)(#[^)]*)?\)/g)) {
      if (!slugs.has(target)) broken.push(`${name} -> ${target}`);
    }
  }
  return broken;
}

// 从 README 抽取 quickstart 段落正文（不含该级标题本身）。缺失即返回 null，
// 由调用方决定是否让构建失败（本脚本：缺失即失败，避免页面静默消失）。
export function extractQuickstart(readme, heading = QUICKSTART_HEADING.en) {
  const headingRe = /^##\s+(.+)$/gm;
  let start = -1;
  let m;
  while ((m = headingRe.exec(readme)) !== null) {
    if (m[1].trim() === heading) {
      start = m.index;
      break;
    }
  }
  if (start === -1) return null;
  const rest = readme.slice(start);
  const afterTitle = rest.indexOf('\n');
  const end = rest.indexOf('\n## ', afterTitle + 1);
  return (end === -1 ? rest.slice(afterTitle + 1) : rest.slice(afterTitle + 1, end)).trim();
}

function jsString(s) {
  return JSON.stringify(String(s));
}

// 清理“上一轮生成的同名文件”，只删生成物本身；手写页面（introduction.md 等，
// 已提交）保持原样不受影响 —— 绝不整目录 rmSync。
function cleanGenerated(contentDir, zhContentDir) {
  if (existsSync(contentDir)) {
    for (const entry of readdirSync(contentDir)) {
      if (/^\d\d-.*\.md$/.test(entry) || entry === 'index.md' || entry === 'quickstart.md') {
        rmSync(join(contentDir, entry), { force: true });
      }
    }
  }
  rmSync(zhContentDir, { recursive: true, force: true });
  mkdirSync(zhContentDir, { recursive: true });
}

// 主流程，返回统计供测试断言；不依赖 process.exit。
// paths 覆盖默认目录，供测试注入临时 lang_docs / 输出目录（其余调用方用真实仓库布局）。
export function run(paths = {}) {
  const websiteDir = paths.websiteDir ?? WEBSITE_DIR;
  const langDocsDir = paths.langDocsDir ?? join(paths.root ?? ROOT, 'lang_docs');
  const contentDir = paths.contentDir ?? join(websiteDir, 'src', 'content', 'docs');
  const zhContentDir = join(contentDir, 'zh');
  const sidebarPath = paths.sidebarPath ?? join(websiteDir, 'src', 'config', 'sidebar.js');

  const VERSION = latestVersionIn(langDocsDir);
  const versionDir = join(langDocsDir, VERSION);

  cleanGenerated(contentDir, zhContentDir);

  const readmeEn = readUtf8(join(versionDir, 'en', 'README.md'));
  const readmeZh = readUtf8(join(versionDir, 'zh', 'README.md'));

  const items = [];
  const generated = [];

  // README 作为文档首页（slug 根 /）：root(英文) 在 content/docs/ 根，zh 在 content/docs/zh/
  // 侧栏 label 用固定词 Overview，而非把整条版本横幅 H1 塞进导航。
  writeFileSync(join(contentDir, 'index.md'), `---\ntitle: ${jsString(firstH1(readmeEn, 'en/README.md'))}\n---\n\n${rewriteLinks(readmeEn)}\n`);
  writeFileSync(join(zhContentDir, 'index.md'), `---\ntitle: ${jsString(firstH1(readmeZh, 'zh/README.md'))}\n---\n\n${rewriteLinks(readmeZh)}\n`);
  items.push({ label: 'Overview', link: '/', zhLabel: '概览' });

  const chapsEn = chapterFiles(join(versionDir, 'en'));
  const chapsZh = chapterFiles(join(versionDir, 'zh'));
  if (chapsEn.length !== CHAPTER_COUNT || chapsZh.length !== CHAPTER_COUNT) {
    throw new Error(
      `T05: expected ${CHAPTER_COUNT} chapters per locale (got ${chapsEn.length} en / ${chapsZh.length} zh)`,
    );
  }

  const chapterNum = (f) => Number(f.slice(0, 2));

  for (let i = 0; i < CHAPTER_COUNT; i += 1) {
    const fEn = chapsEn[i];
    const fZh = chapsZh[i];
    if (chapterNum(fEn) !== chapterNum(fZh)) {
      throw new Error(`T05: chapter order mismatch en ${fEn} vs zh ${fZh}`);
    }

    const mdEn = readUtf8(join(versionDir, 'en', fEn));
    const mdZh = readUtf8(join(versionDir, 'zh', fZh));
    const titleEn = firstH1(mdEn, fEn);
    const titleZh = firstH1(mdZh, fZh);
    const linkedEn = rewriteLinks(mdEn);
    const linkedZh = rewriteLinks(mdZh);

    writeFileSync(join(contentDir, fEn), `---\ntitle: ${jsString(titleEn)}\n---\n\n${linkedEn}\n`);
    writeFileSync(join(zhContentDir, fZh), `---\ntitle: ${jsString(titleZh)}\n---\n\n${linkedZh}\n`);
    generated.push({ name: fEn, body: linkedEn });
    generated.push({ name: `zh/${fZh}`, body: linkedZh });

    items.push({ label: titleEn, link: `/${fEn.replace(/\.md$/, '')}`, zhLabel: titleZh });
  }

  // /docs/quickstart：内容取自 README 的上手段落（不新写一份上手文档）。
  // 两侧任一缺失即构建失败（D9：fail loudly，不静默跳过 ticket 要求的路由）。
  // 该段在 README（文档首页）里也渲染，为遵守 D2「一个概念只写一次，其余位置链接过去」，
  // quickstart 页顶部指向完整参考首页，而非当作第二份独立上手文档。
  const qsEn = extractQuickstart(readmeEn, QUICKSTART_HEADING.en);
  const qsZh = extractQuickstart(readmeZh, QUICKSTART_HEADING.zh);
  if (!qsEn) throw new Error(`T05: en README has no '## ${QUICKSTART_HEADING.en}' section for /quickstart`);
  if (!qsZh) throw new Error(`T05: zh README has no '## ${QUICKSTART_HEADING.zh}' section for /quickstart`);
  writeFileSync(
    join(contentDir, 'quickstart.md'),
    `---\ntitle: Quick Start\n---\n\nAn excerpt of the [full reference](/). See that page for the complete guide.\n\n${qsEn}\n`,
  );
  writeFileSync(
    join(zhContentDir, 'quickstart.md'),
    `---\ntitle: Quick Start\n---\n\n本文是[完整参考](/zh/)的节选。完整的语言指南请见该页。\n\n${qsZh}\n`,
  );
  items.push({ label: 'Quick Start', link: '/quickstart', zhLabel: '快速开始' });

  // D9(a)：章节互链必须都能解析到真实章节，否则构建失败。
  const broken = brokenCrossLinks(generated);
  if (broken.length > 0) {
    throw new Error(`T05: ${broken.length} broken chapter cross-link(s):\n  ${broken.join('\n  ')}`);
  }

  // 生成侧栏配置（Starlight 期待数组；label 英文，translations.zh 中文；link 根路径）
  const lines = [
    '// Auto-generated by scripts/build-docs.js (T05). Do not edit by hand.',
    'export const docsSidebar = [',
  ];
  for (const it of items) {
    lines.push(
      `  { label: ${jsString(it.label)}, link: ${jsString(it.link)}, translations: { zh: ${jsString(it.zhLabel)} } },`,
    );
  }
  lines.push('];');

  mkdirSync(dirname(sidebarPath), { recursive: true });
  writeFileSync(sidebarPath, lines.join('\n') + '\n');

  return { version: VERSION, chapters: CHAPTER_COUNT, sidebarEntries: items.length };
}

// 仅在作为脚本直接执行时运行（测试通过 import { run } 复用，避免副作用）。
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const { version, chapters, sidebarEntries } = run();
  console.log(`T05 OK: ${version} — ${chapters} chapters, ${sidebarEntries} sidebar entries (root + zh)`);
}
