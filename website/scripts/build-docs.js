// T05 — 构建期：把 lang_docs/<最新版本>/{en,zh} 的 16 章 + README 生成为
// Starlight docs 集合（root 语言在 src/content/docs/ 根，zh 在 src/content/docs/zh/），
// 并生成 src/config/sidebar.js（按语言翻译 label，link 不带 locale 前缀，Starlight 自动加）。
// 零第三方依赖；缺章节、语种不一致时立即退出非零（构建失败），满足 D9 断言。
import { readdirSync, readFileSync, writeFileSync, mkdirSync, rmSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPTS_DIR = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(SCRIPTS_DIR, '..', '..'); // fleen 仓库根
const WEBSITE_DIR = resolve(SCRIPTS_DIR, '..'); // website/
const LANG_DOCS = join(ROOT, 'lang_docs');
const CONTENT_DIR = resolve(WEBSITE_DIR, 'src', 'content', 'docs');
const CHAPTER_COUNT = 16;

// Starlight 多语言约定：root（英文）在 content/docs/ 根，zh 在 content/docs/zh/
const LOCALES = [
  { code: 'root', langDir: 'en', contentDir: CONTENT_DIR },
  { code: 'zh', langDir: 'zh', contentDir: join(CONTENT_DIR, 'zh') },
];

function latestVersion() {
  const versions = readdirSync(LANG_DOCS).filter((d) => /^\d+\.\d+\.\d+$/.test(d));
  if (versions.length === 0) throw new Error('T05: no versions found under lang_docs/');
  return versions.sort((a, b) => {
    const pa = a.split('.').map(Number);
    const pb = b.split('.').map(Number);
    for (let i = 0; i < 3; i += 1) {
      if (pa[i] !== pb[i]) return pb[i] - pa[i];
    }
    return 0;
  })[0];
}

function chapterFiles(dir) {
  return readdirSync(dir)
    .filter((f) => /^\d\d-.*\.md$/.test(f))
    .sort((a, b) => a.localeCompare(b));
}

function firstH1(md) {
  const m = md.match(/^#\s+(.+)/m);
  return m ? m[1].trim() : 'Untitled';
}

// 把章节内部互链 "01-xxx.md" -> "/01-xxx"（Starlight slug 路径），保留锚点
function rewriteLinks(md) {
  return md.replace(/]\((\d\d-[^)]*?)\.md(#.*)?\)/g, (_m, file, anchor = '') => {
    return `](/${file}${anchor || ''})`;
  });
}

function extractQuickstart(readme) {
  const start = readme.indexOf('## Quick Start');
  if (start === -1) return null;
  const rest = readme.slice(start);
  const afterTitle = rest.indexOf('\n');
  const end = rest.indexOf('\n## ', afterTitle + 1);
  const body = end === -1 ? rest.slice(afterTitle + 1) : rest.slice(afterTitle + 1, end);
  return body.trim();
}

function jsString(s) {
  return JSON.stringify(String(s));
}

function main() {
  const VERSION = latestVersion();
  const versionDir = join(LANG_DOCS, VERSION);

  // 清空旧生成内容，避免 stale 文件
  rmSync(CONTENT_DIR, { recursive: true, force: true });
  mkdirSync(CONTENT_DIR, { recursive: true });
  mkdirSync(join(CONTENT_DIR, 'zh'), { recursive: true });

  const readmeEn = readFileSync(join(versionDir, 'en', 'README.md'), 'utf8');
  const readmeZh = readFileSync(join(versionDir, 'zh', 'README.md'), 'utf8');

  const items = [];

  // README 作为文档首页（slug 根 /docs）：root(英文) 在 content/docs/ 根，zh 在 content/docs/zh/
  writeFileSync(
    join(CONTENT_DIR, 'index.md'),
    `---\ntitle: ${jsString(firstH1(readmeEn))}\n---\n\n${rewriteLinks(readmeEn)}\n`,
  );
  writeFileSync(
    join(CONTENT_DIR, 'zh', 'index.md'),
    `---\ntitle: ${jsString(firstH1(readmeZh))}\n---\n\n${rewriteLinks(readmeZh)}\n`,
  );
  items.push({
    label: firstH1(readmeEn),
    link: '/',
    zhLabel: firstH1(readmeZh),
  });

  const chapsEn = chapterFiles(join(versionDir, 'en'));
  const chapsZh = chapterFiles(join(versionDir, 'zh'));
  if (chapsEn.length !== CHAPTER_COUNT || chapsZh.length !== CHAPTER_COUNT) {
    throw new Error(`T05: expected ${CHAPTER_COUNT} chapters per locale (got ${chapsEn.length} en / ${chapsZh.length} zh)`);
  }
  for (let i = 0; i < CHAPTER_COUNT; i += 1) {
    const fEn = chapsEn[i];
    const fZh = chapsZh[i];
    const nEn = Number(fEn.slice(0, 2));
    const nZh = Number(fZh.slice(0, 2));
    if (nEn !== nZh) {
      throw new Error(`T05: chapter order mismatch en ${fEn} vs zh ${fZh}`);
    }

    const mdEn = readFileSync(join(versionDir, 'en', fEn), 'utf8');
    const mdZh = readFileSync(join(versionDir, 'zh', fZh), 'utf8');
    const titleEn = firstH1(mdEn);
    const titleZh = firstH1(mdZh);

    const link = `/${fEn.replace(/\.md$/, '')}`;
    for (const loc of LOCALES) {
      mkdirSync(loc.contentDir, { recursive: true });
    }
    writeFileSync(join(CONTENT_DIR, fEn), `---\ntitle: ${jsString(titleEn)}\n---\n\n${rewriteLinks(mdEn)}\n`);
    writeFileSync(join(CONTENT_DIR, 'zh', fZh), `---\ntitle: ${jsString(titleZh)}\n---\n\n${rewriteLinks(mdZh)}\n`);

    items.push({ label: titleEn, link, zhLabel: titleZh });
  }

  // /docs/quickstart：内容取自 README 的 ## Quick Start 段落（不新写一份上手文档）
  const qsEn = extractQuickstart(readmeEn);
  const qsZh = extractQuickstart(readmeZh);
  if (qsEn && qsZh) {
    writeFileSync(join(CONTENT_DIR, 'quickstart.md'), `---\ntitle: Quick Start\n---\n\n${qsEn}\n`);
    writeFileSync(join(CONTENT_DIR, 'zh', 'quickstart.md'), `---\ntitle: Quick Start\n---\n\n${qsZh}\n`);
    items.push({ label: 'Quick Start', link: '/quickstart', zhLabel: '快速开始' });
  }

  // 章节数量一致（D9 断言三）
  if (items.length - 1 - (qsEn && qsZh ? 1 : 0) !== CHAPTER_COUNT) {
    throw new Error(`T05: chapter count is not ${CHAPTER_COUNT}`);
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

  const cfgDir = resolve(WEBSITE_DIR, 'src', 'config');
  mkdirSync(cfgDir, { recursive: true });
  writeFileSync(join(cfgDir, 'sidebar.js'), lines.join('\n') + '\n');

  console.log(`T05 OK: ${VERSION} — ${items.length} sidebar entries (root + zh)`);
}

main();
