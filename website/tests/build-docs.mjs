// T05 generator assertions (run without a build; Node stdlib only).
// Covers the logic the D9 build gate depends on:
//   - chapter cross-link rewriting + the D9(a) broken-link assertion
//   - quickstart extraction from the README's getting-started section
//   - end-to-end run(): chapter/quickstart/sidebar emission + loud failure
//     on a missing chapter (D9c) and on a missing quickstart section.
// Run: node --test tests/build-docs.mjs
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { brokenCrossLinks, extractQuickstart, run } from '../scripts/build-docs.js';

const README_BODY = `# Fleen 0.0.0 · Test

Intro line.

## Contents

A table.

## Running a Fleen program

Source files use the \`.fln\` extension.

\`\`\`bash
cargo fln hello.fln
\`\`\`
`;

test('brokenCrossLinks flags chapter links that point at no generated chapter', () => {
  const files = [
    { name: '01-a.md', body: 'see [b](/02-b) and [missing](/99-nope)' },
    { name: '02-b.md', body: 'back to [a](/01-a#anchor)' },
  ];
  assert.deepEqual(brokenCrossLinks(files), ['01-a.md -> /99-nope']);
});

test('brokenCrossLinks accepts the README root link and anchored chapter links', () => {
  const files = [
    { name: '01-a.md', body: '[home](/) [b](/02-b#section)' },
    { name: '02-b.md', body: '[a](/01-a)' },
  ];
  assert.deepEqual(brokenCrossLinks(files), []);
});

test('brokenCrossLinks checks both locales', () => {
  const files = [
    { name: '01-a.md', body: '[ok](/02-b)' },
    { name: 'zh/01-a.md', body: '[broken](/03-nope)' },
    { name: '02-b.md', body: '' },
  ];
  assert.deepEqual(brokenCrossLinks(files), ['zh/01-a.md -> /03-nope']);
});

test('extractQuickstart returns only the getting-started section body', () => {
  const qs = extractQuickstart(README_BODY);
  assert.match(qs, /^Source files use/);
  assert.ok(!qs.includes('## Contents'));
  assert.ok(!qs.includes('# Fleen 0.0.0'));
  assert.ok(!qs.startsWith('#')); // the heading itself is not included
});

test('extractQuickstart returns null when the section is absent', () => {
  assert.equal(extractQuickstart('# T\n\n## Contents\n\nx\n'), null);
});

// Build a fake lang_docs/<version>/{en,zh} tree with 16 chapters + a README
// whose getting-started section is present, then run the generator into temp dirs.
function makeFakeRepo({ dropChapter16 = false, dropQuickstart = false } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'fleen-docsgen-'));
  const versionDir = join(root, 'lang_docs', '0.0.0');
  for (const loc of ['en', 'zh']) {
    mkdirSync(join(versionDir, loc), { recursive: true });
    const heading = loc === 'en' ? 'Running a Fleen program' : '运行 Fleen 程序';
    writeFileSync(
      join(versionDir, loc, 'README.md'),
      dropQuickstart ? '# T\n\n## Contents\n\nx\n' : `${README_BODY.replace('Running a Fleen program', heading)}\n`,
    );
    for (let i = 1; i <= (dropChapter16 ? 15 : 16); i += 1) {
      const n = String(i).padStart(2, '0');
      const next = i === 16 ? '01' : String(i + 1).padStart(2, '0');
      const title = loc === 'en' ? `Chapter ${n}` : `第 ${n} 章`;
      writeFileSync(join(versionDir, loc, `${n}-c.md`), `# ${title}\n\nLink to [other](/${next}-c).\n`);
    }
  }
  return { root };
}

test('run() emits 16 chapters, quickstart and an 18-entry sidebar', () => {
  const { root } = makeFakeRepo();
  const langDocs = join(root, 'lang_docs');
  const contentDir = join(root, 'content');
  const sidebarPath = join(root, 'sidebar.js');

  const result = run({ langDocsDir: langDocs, contentDir, sidebarPath });
  assert.equal(result.version, '0.0.0');
  assert.equal(result.chapters, 16);
  assert.equal(result.sidebarEntries, 18); // 16 chapters + Overview + Quick Start

  for (let i = 1; i <= 16; i += 1) {
    const n = String(i).padStart(2, '0');
    assert.ok(existsSync(join(contentDir, `${n}-c.md`)), `missing en ${n}`);
    assert.ok(existsSync(join(contentDir, 'zh', `${n}-c.md`)), `missing zh ${n}`);
  }
  assert.ok(existsSync(join(contentDir, 'index.md')));
  assert.ok(existsSync(join(contentDir, 'quickstart.md')));

  const sidebar = readFileSync(sidebarPath, 'utf8');
  assert.match(sidebar, /export const docsSidebar = \[/);
  assert.match(sidebar, /"\/quickstart"/);
  assert.match(sidebar, /"\/01-c"/);

  // the rewritten cross-link in the generated chapter resolves (D9a)
  const gen = readFileSync(join(contentDir, '01-c.md'), 'utf8');
  assert.match(gen, /\]\(\/02-c\)/);

  rmSync(root, { recursive: true, force: true });
});

test('run() fails loudly when a chapter is missing (D9c)', () => {
  const { root } = makeFakeRepo({ dropChapter16: true });
  const langDocs = join(root, 'lang_docs');
  assert.throws(
    () => run({ langDocsDir: langDocs, contentDir: join(root, 'content'), sidebarPath: join(root, 'sidebar.js') }),
    /expected 16 chapters per locale/,
  );
  rmSync(root, { recursive: true, force: true });
});

test('run() fails loudly when the README lacks a quickstart section (ticket /quickstart)', () => {
  const { root } = makeFakeRepo({ dropQuickstart: true });
  const langDocs = join(root, 'lang_docs');
  assert.throws(
    () => run({ langDocsDir: langDocs, contentDir: join(root, 'content'), sidebarPath: join(root, 'sidebar.js') }),
    /Running a Fleen program/,
  );
  rmSync(root, { recursive: true, force: true });
});

test('run() preserves hand-written tracked pages in contentDir (does not rmSync the tree)', () => {
  const { root } = makeFakeRepo();
  const langDocs = join(root, 'lang_docs');
  const contentDir = join(root, 'content');
  mkdirSync(contentDir, { recursive: true });
  const handwritten = join(contentDir, 'introduction.md');
  writeFileSync(handwritten, '---\ntitle: Introduction\n---\n\nhand-written, must survive a rebuild\n');

  run({ langDocsDir: langDocs, contentDir, sidebarPath: join(root, 'sidebar.js') });

  // A previous version rmSync'd the whole contentDir, deleting this tracked page.
  assert.ok(existsSync(handwritten), 'generator deleted a hand-written page');
  assert.equal(readFileSync(handwritten, 'utf8'), '---\ntitle: Introduction\n---\n\nhand-written, must survive a rebuild\n');

  rmSync(root, { recursive: true, force: true });
});
