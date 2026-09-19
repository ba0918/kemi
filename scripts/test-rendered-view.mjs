// 描画表示（R-RENDER）のブラウザ自動化。実際の kemi バイナリを worktree モードで起動し、
// agent-browser でページを操作して確かめる。
//
//   node scripts/test-rendered-view.mjs <kemi-bin>
//
// 1 回目の起動: ブロックの数と値、上限超えの切り替えの無効、読み込み直しでの描画表示の
// 保持と上限超えでのソース表示への戻り、2 列表示でも 1 段。
// 2 回目の起動: 新側と消したブロックから付けたコメントが submit の JSON に行コメントとして入る。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, writeFile, appendFile, mkdir } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
if (!process.argv[2]) {
  console.error('usage: node scripts/test-rendered-view.mjs <kemi-bin>');
  process.exit(2);
}
const binary = resolve(process.argv[2]);
const scripts = dirname(fileURLToPath(import.meta.url));
const session = `kemi-render-${process.pid}`;
const browser = async (...args) => {
  const { stdout } = await run('agent-browser', ['--session', session, '--json', ...args], { maxBuffer: 8 * 1024 * 1024 });
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, JSON.stringify(result));
  return result.data;
};
const evaluate = async (code) => (await browser('eval', '-b', Buffer.from(code).toString('base64'))).result;
/** 待ちが尽きたら、ページの状態を添えて失敗にする。 */
const waitFor = async (code) => {
  try {
    await browser('wait', '--fn', code);
  } catch (error) {
    const snapshot = await evaluate(`JSON.stringify({
      path: document.querySelector('#file-header .path')?.textContent,
      notice: document.querySelector('#notice')?.textContent,
      renderedHidden: document.querySelector('#rendered-doc')?.hidden,
      blocks: document.querySelectorAll('#rendered-doc [data-kemi-block]').length,
      rows: document.querySelectorAll('[data-kemi-row]').length,
      badgeHidden: document.querySelector('#update-badge')?.hidden,
      overlay: document.querySelector('#overlay')?.hidden ? null : document.querySelector('#overlay-card')?.textContent,
      tail: document.querySelector('#rendered-doc .rendered-body')?.textContent.trim().slice(-120),
      values: Array.from(document.querySelectorAll('#rendered-doc [data-kemi-block]')).map(e => e.dataset.kemiBlock),
    })`).catch(() => 'no snapshot');
    throw new Error(`wait failed for: ${code}\npage: ${snapshot}`, { cause: error });
  }
};

const OLD_GUIDE = `---
title: Guide
---

# Guide

Intro paragraph with *emphasis* and a [link](https://example.com).

- first item
- second item
  - nested item

| a | b |
|---|---|
| 1 | 2 |
| 3 | 4 |

\`\`\`rust
let a = 1;
\`\`\`

> quoted line

Old only paragraph.

Closing paragraph here.
`;
const NEW_GUIDE = OLD_GUIDE
  .replace('[link](https://example.com).', '[link](https://example.com) now.')
  .replace('| 3 | 4 |\n', '| 3 | 5 |\n')
  .replace('Old only paragraph.\n\n', '')
  .replace('Closing paragraph here.\n', 'Closing paragraph here.\n\nNew paragraph with ![shot](img/shot.png).\n');
const EXPECTED_BLOCKS = [
  'new:1-3', 'new:5-5', 'new:7-7', 'new:9-9', 'new:10-10', 'new:11-11',
  'new:13-13', 'new:15-15', 'old:16-16', 'new:16-16', 'new:18-20', 'new:22-22',
  'new:24-24', 'old:26-26', 'new:26-26',
];

async function makeFixture() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-render-'));
  await run('bash', [join(scripts, 'gen-fixture.sh'), dir, '--files', '2', '--lines', '3']);
  const git = (...args) => run('git', ['-C', dir, ...args], {
    env: { ...process.env, GIT_AUTHOR_DATE: '2026-01-01T00:00:00+00:00', GIT_COMMITTER_DATE: '2026-01-01T00:00:00+00:00' },
  });
  await mkdir(join(dir, 'docs'), { recursive: true });
  await writeFile(join(dir, 'docs', 'guide.md'), OLD_GUIDE);
  await git('add', '-A');
  await git('commit', '-q', '-m', 'guide');
  await writeFile(join(dir, 'docs', 'guide.md'), NEW_GUIDE);
  await writeFile(join(dir, 'docs', 'big.md'), Array.from({ length: 10_001 }, (_, i) => `line ${i + 1}\n`).join(''));
  return dir;
}

async function startKemi(dir, state) {
  const child = spawn(binary, ['--worktree', '--port', '0', '--no-open'], {
    cwd: dir,
    env: { ...process.env, XDG_STATE_HOME: state, HOME: join(state, 'home') },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  const url = await new Promise((resolve, reject) => {
    let stderr = '';
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
      const match = stderr.match(/^kemi: (http:\/\/\S+)/m);
      if (match) resolve(match[1]);
    });
    child.on('error', reject);
    child.on('exit', (code) => reject(new Error(`kemi exited before serving (${code}): ${stderr}`)));
  });
  const exited = new Promise((resolve) => child.on('exit', (code, signal) => resolve({ code, signal, stdout })));
  return { child, url, exited };
}

/** 入れ子のスクロール領域の中の要素は、見える位置へ送ってから押す（画面の外への click は届かない）。 */
const scrollTo = (selector) => evaluate(`document.querySelector(${JSON.stringify(selector)}).scrollIntoView({ block: 'center' }); true`);
const clickVisible = async (selector) => {
  await scrollTo(selector);
  await browser('click', selector);
};
const openBlockEditor = async (block) => {
  await scrollTo(`#rendered-doc [data-kemi-block="${block}"]`);
  await browser('hover', `#rendered-doc [data-kemi-block="${block}"]`);
  await browser('click', '.kb-plus');
  await waitFor(`document.querySelector('#rendered-doc .editor textarea[data-editor-field="body"]') !== null`);
};

const blocks = () => evaluate(`Array.from(document.querySelectorAll('#rendered-doc [data-kemi-block]')).map(e => e.dataset.kemiBlock)`);
const renderedShown = `!document.querySelector('#rendered-doc').hidden`;
const notLoading = `!document.querySelector('#notice') || document.querySelector('#notice').hidden || !/Loading|Rendering/.test(document.querySelector('#notice').textContent)`;

async function openGuideRendered(url) {
  await browser('open', url);
  await waitFor(`document.querySelectorAll('#tree button').length > 0`);
  await waitFor(`document.querySelector('#file-header .path')?.textContent === 'docs/guide.md' && ${notLoading}`);
  await browser('click', '#file-header .view-rendered');
  await waitFor(renderedShown);
}

const fixture = await makeFixture();
const state = await mkdtemp(join(tmpdir(), 'kemi-render-state-'));
let kemi = await startKemi(fixture, state);
try {
  await openGuideRendered(kemi.url);
  // (1) 文書全体のブロックが仮想化も折りたたみも無しに載る。
  assert.deepEqual(await blocks(), EXPECTED_BLOCKS);
  assert.equal(await evaluate(`document.querySelectorAll('[data-kemi-row]').length`), 0);
  console.log('PASS ブロックの数と値が文書全体と一致する');

  // (2) 10,001 行の Markdown では切り替えが無効で、理由がツールチップに出る。
  await browser('press', 'j');
  await waitFor(`document.querySelector('#file-header .path')?.textContent === 'docs/big.md' && ${notLoading}`);
  assert.equal(await evaluate(`document.querySelector('#file-header .view-rendered').disabled`), true);
  assert.match(await evaluate(`document.querySelector('#file-header .view-rendered').title`), /Cannot render/);
  await browser('press', 'r');
  assert.equal(await evaluate(`document.querySelector('#rendered-doc').hidden`), true);
  console.log('PASS 上限超えの Markdown では切り替えが無効');

  // (3) 外部から変えて読み込み直しても描画表示のまま。上限を超えたらソース表示に戻る。
  await browser('press', 'k');
  await waitFor(`document.querySelector('#file-header .path')?.textContent === 'docs/guide.md' && ${renderedShown}`);
  await appendFile(join(fixture, 'docs', 'guide.md'), '\nAppended paragraph after reload.\n');
  await waitFor(`!document.querySelector('#update-badge').hidden`);
  await browser('click', '#update-badge');
  // 行の整列は文書が伸びると変わりうるので、数ではなく、描画表示のままで足した段落
  // （28 行目）のブロックが出ていることを見る。
  await waitFor(`${renderedShown} && document.querySelector('#rendered-doc [data-kemi-block="new:28-28"]') !== null`);
  assert.equal(await evaluate(`document.querySelector('#file-header .view-rendered').getAttribute('aria-pressed')`), 'true');
  await appendFile(join(fixture, 'docs', 'guide.md'), Array.from({ length: 10_001 }, (_, i) => `x ${i}\n`).join(''));
  await waitFor(`!document.querySelector('#update-badge').hidden`);
  await browser('click', '#update-badge');
  await waitFor(`document.querySelector('#rendered-doc').hidden && document.querySelectorAll('[data-kemi-row]').length > 0`);
  assert.equal(await evaluate(`document.querySelector('#file-header .view-rendered').disabled`), true);
  console.log('PASS 読み込み直しで描画表示を保ち、描画できなくなったらソース表示に戻る');

  // (4) 2 列表示にしても描画表示は 1 段で、由来の行が無い。
  await writeFile(join(fixture, 'docs', 'guide.md'), NEW_GUIDE);
  await waitFor(`!document.querySelector('#update-badge').hidden`);
  await browser('click', '#update-badge');
  await waitFor(`${renderedShown} && document.querySelectorAll('#rendered-doc [data-kemi-block]').length === ${EXPECTED_BLOCKS.length}`);
  await browser('press', 's');
  assert.equal(await evaluate(renderedShown), true);
  assert.equal(await evaluate(`document.querySelectorAll('#rendered-doc .split, #rendered-doc .origin-row, #diff-content .row').length`), 0);
  await browser('press', 'u');
  console.log('PASS 2 列表示でも描画表示は 1 段で由来の行が無い');
} finally {
  kemi.child.kill('SIGTERM');
  await kemi.exited;
}

// (5) 別の起動で、新側のブロックと消したブロックからコメントを付けて submit する。
const state2 = await mkdtemp(join(tmpdir(), 'kemi-render-state-'));
kemi = await startKemi(fixture, state2);
await openGuideRendered(kemi.url);
await openBlockEditor('new:7-7');
assert.equal(
  await evaluate(`document.querySelector('#rendered-doc .kb-quote').textContent`),
  'Intro paragraph with *emphasis* and a [link](https://example.com) now.',
);
await browser('fill', '#rendered-doc .editor textarea[data-editor-field="body"]', 'from the new block');
await clickVisible('#rendered-doc .editor button[type="submit"]');
await waitFor(`document.querySelector('#comment-count').textContent === '1'`);
await openBlockEditor('old:16-16');
assert.equal(await evaluate(`document.querySelector('#rendered-doc .editor .suggestion-row')`), null);
await browser('fill', '#rendered-doc .editor textarea[data-editor-field="body"]', 'from the deleted row');
await clickVisible('#rendered-doc .editor button[type="submit"]');
await waitFor(`document.querySelector('#comment-count').textContent === '2'`);
await browser('click', '#btn-approve');
await waitFor(`!document.querySelector('#modal').hidden`);
await browser('click', '#modal-ok');
const { code, stdout } = await kemi.exited;
assert.equal(code, 0);
const result = JSON.parse(stdout);
assert.equal(result.verdict, 'approved');
const [first, second] = result.comments;
assert.equal(first.side, 'new');
assert.equal(first.start_line, 7);
assert.equal(first.end_line, 7);
assert.deepEqual(first.quote, ['Intro paragraph with *emphasis* and a [link](https://example.com) now.']);
assert.equal(first.body, 'from the new block');
assert.equal(second.side, 'old');
assert.equal(second.start_line, 16);
assert.equal(second.end_line, 16);
assert.deepEqual(second.quote, ['| 3 | 4 |']);
assert.equal(second.suggestion, null);
console.log('PASS 描画表示から付けたコメントが submit の JSON に行コメントとして入る');
await run('agent-browser', ['--session', session, 'close']);
