// T04 static-site assertions (run on `dist/` after `astro build`).
// - Static-only lock (PLAN D1): no SSR adapter output — every HTML page is a
//   real file; no server entry, no per-route `*.mjs` server chunks.
// - Page inventory: the skeleton's pages exist.
// - Local-link check: every href/src in the built HTML resolves to a real
//   file in `dist/` (anchor fragments ignored; T19 adds full anchor checks).
import { readdirSync, statSync, readFileSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const dist = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'dist');
let failures = 0;
const fail = (msg) => { console.error(`FAIL: ${msg}`); failures += 1; };

const walk = (dir) =>
  readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = join(dir, e.name);
    return e.isDirectory() ? walk(p) : [p];
  });

const files = walk(dist);
const htmlFiles = files.filter((f) => f.endsWith('.html'));
if (htmlFiles.length === 0) fail('no HTML pages in dist/ — nothing was built');

// 1. Static-only: all HTML must be standalone files; SSR would leave server
//    entry chunks (`dist/server/`, `entry.mjs`) or unprerendered routes.
const serverArtifacts = files.filter(
  (f) => f.includes(`${dist}/server/`) || f.endsWith('entry.mjs'),
);
if (serverArtifacts.length > 0) {
  fail(`SSR artifacts found in a static build: ${serverArtifacts.join(', ')}`);
}

// 2. Page inventory: the M1 skeleton must expose at least its home page.
for (const page of ['index.html', '404.html']) {
  if (!files.includes(join(dist, page))) fail(`missing page: /${page}`);
}

// 3. Every local href/src must resolve inside dist/.
const exists = (rel) => {
  const target = join(dist, rel);
  if (files.includes(target)) return true;
  return statSync(target, { throwIfNoEntry: false })?.isDirectory()
    && files.includes(join(target, 'index.html'));
};
const external = /^(https?:)?\/\//;
for (const file of htmlFiles) {
  const html = readFileSync(file, 'utf8');
  for (const [, , url] of html.matchAll(/\b(href|src)="([^"]*)"/g)) {
    if (external.test(url) || url.startsWith('mailto:')) continue; // checked by T19
    const path = url.split('#')[0];
    if (path === '') continue; // pure in-page anchor
    const rel = path.replace(/^\//, '');
    if (!exists(rel)) fail(`${file}: unresolvable link ${url}`);
  }
}

if (failures > 0) {
  console.error(`\n${failures} check(s) failed`);
  process.exit(1);
}
console.log(`OK: ${htmlFiles.length} static page(s), all local links resolve`);
