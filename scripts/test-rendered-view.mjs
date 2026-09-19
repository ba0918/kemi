// 描画表示（R-RENDER）のブラウザ自動化。実際の kemi バイナリを worktree モードで起動し、
// agent-browser でページを操作して確かめる。
//
//   node scripts/test-rendered-view.mjs <kemi-bin>
//
// 1 回目の起動: ブロックの数と値、上限超えの切り替えの無効、読み込み直しでの描画表示の
// 保持と上限超えでのソース表示への戻り、2 列表示でも 1 段、表の切り替えと見出し行、
// SVG と PNG の切り替えの有無と既定、画像の並びと "unchanged"、壊れた画像の枠、symlink の
// 相対パス画像の枠。保留した後の `kemi --resume` で描画表示・相対パス画像・切り替えの既定。
// 最後の起動: 新側と消したブロックから付けたコメントが submit の JSON に行コメントとして入る。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { deflateSync, crc32 } from 'node:zlib';
import { mkdtemp, writeFile, appendFile, mkdir, symlink } from 'node:fs/promises';
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

/** 1×1 の PNG。色ごとに違うバイト列になる。 */
function png(r, g, b) {
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const length = Buffer.alloc(4);
    length.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([length, body, crc]);
  };
  const header = Buffer.alloc(13);
  header.writeUInt32BE(1, 0);
  header.writeUInt32BE(1, 4);
  header.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk('IHDR', header),
    chunk('IDAT', deflateSync(Buffer.from([0, r, g, b]))),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

const svg = (fill) => `<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><circle cx="10" cy="10" r="8" fill="${fill}"/></svg>\n`;
const IMAGES_MD = '# Images\n\n![shot](img/shot.png)\n\n![linked](img/link.png)\n\n![nowhere](img/missing.png)\n\n![base](img/base.png)\n';
/** 旧では段落が表を 2 つに割り、新ではその段落（と 2 つ目の見出し行）だけ消えて 1 つの表になる。 */
const OLD_TABLE_MD = '| a | b |\n|---|---|\n| 1 | 2 |\n\nBetween.\n\n| a | b |\n|---|---|\n| 3 | 4 |\n';
const NEW_TABLE_MD = '| a | b |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n';

async function makeFixture() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-render-'));
  await run('bash', [join(scripts, 'gen-fixture.sh'), dir, '--files', '2', '--lines', '3']);
  const git = (...args) => run('git', ['-C', dir, ...args], {
    env: { ...process.env, GIT_AUTHOR_DATE: '2026-01-01T00:00:00+00:00', GIT_COMMITTER_DATE: '2026-01-01T00:00:00+00:00' },
  });
  const write = (relative, content) => writeFile(join(dir, relative), content);
  for (const sub of ['docs/img', 'art', 'data']) {
    await mkdir(join(dir, sub), { recursive: true });
  }
  await write('docs/guide.md', OLD_GUIDE);
  await write('docs/images.md', '# Images\n');
  await write('docs/table.md', OLD_TABLE_MD);
  await write('docs/img/base.png', png(9, 9, 9));
  await symlink('base.png', join(dir, 'docs/img/link.png'));
  await write('art/photo.png', png(255, 0, 0));
  await write('art/broken.png', png(0, 255, 0));
  await write('art/same.png', png(0, 0, 255));
  await write('art/icon.svg', svg('red'));
  await write('data/rows.csv', 'name,note\nx,"fine"\ny,two\n');
  await git('add', '-A');
  await git('commit', '-q', '-m', 'base');
  await write('docs/guide.md', NEW_GUIDE);
  await write('docs/images.md', IMAGES_MD);
  await write('docs/table.md', NEW_TABLE_MD);
  await write('docs/img/shot.png', png(200, 100, 0));
  await write('docs/big.md', Array.from({ length: 10_001 }, (_, i) => `line ${i + 1}\n`).join(''));
  await write('art/photo.png', png(0, 0, 255));
  await write('art/broken.png', Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x00, 0x62, 0x72, 0x6f, 0x6b, 0x65, 0x6e]));
  await write('art/icon.svg', svg('blue'));
  await write('data/rows.csv', 'name,note\nx,"fine"\ny,three\n');
  await git('mv', 'art/same.png', 'art/renamed.png');
  return dir;
}

async function startKemi(dir, state, args = ['--worktree']) {
  const child = spawn(binary, [...args, '--port', '0', '--no-open'], {
    cwd: dir,
    env: { ...process.env, XDG_STATE_HOME: state, HOME: join(state, 'home') },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  const url = await new Promise((resolve, reject) => {
    child.stderr.on('data', () => {
      const match = stderr.match(/^kemi: (http:\/\/\S+)/m);
      if (match) resolve(match[1]);
    });
    child.on('error', reject);
    child.on('exit', (code) => reject(new Error(`kemi exited before serving (${code}): ${stderr}`)));
  });
  const exited = new Promise((resolve) => child.on('exit', (code, signal) => resolve({ code, signal, stdout, stderr: () => stderr })));
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
/** 括弧で囲む: `&&` と並べる呼び出し側で `||` が外へ漏れ、前後の条件が効かなくなるのを防ぐ。 */
const notLoading = `(!document.querySelector('#notice') || document.querySelector('#notice').hidden || !/Loading|Rendering/.test(document.querySelector('#notice').textContent))`;

const at = (path) => `document.querySelector('#file-header .path')?.textContent === ${JSON.stringify(path)}`;

/** ツリーからファイルを選び、取得が終わるのを待つ。 */
async function selectFile(path) {
  const name = path.split('/').pop();
  await evaluate(`Array.from(document.querySelectorAll('#tree button')).find(b => b.textContent.includes(${JSON.stringify(name)})).click(); true`);
  await waitFor(`${at(path)} && ${notLoading}`);
}

async function openGuideRendered(url) {
  await browser('open', url);
  await waitFor(`document.querySelectorAll('#tree button').length > 0`);
  await selectFile('docs/guide.md');
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
  await selectFile('docs/big.md');
  assert.equal(await evaluate(`document.querySelector('#file-header .view-rendered').disabled`), true);
  assert.match(await evaluate(`document.querySelector('#file-header .view-rendered').title`), /Cannot render/);
  await browser('press', 'r');
  assert.equal(await evaluate(`document.querySelector('#rendered-doc').hidden`), true);
  console.log('PASS 上限超えの Markdown では切り替えが無効');

  // (3) 外部から変えて読み込み直しても描画表示のまま。上限を超えたらソース表示に戻る。
  await selectFile('docs/guide.md');
  await waitFor(renderedShown);
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

  // (6) 表: 切り替えがあり、1 行目が <th>、書き換えの行は旧新が上下に並ぶ。
  await selectFile('data/rows.csv');
  assert.equal(await evaluate(`document.querySelectorAll('#file-header .view-switch button').length`), 2);
  assert.equal(await evaluate(`document.querySelector('#file-header .view-rendered').getAttribute('aria-pressed')`), 'false');
  await browser('press', 'r');
  await waitFor(`${renderedShown} && document.querySelectorAll('#rendered-doc tr').length === 4`);
  const rows = await evaluate(`Array.from(document.querySelectorAll('#rendered-doc tr')).map(tr => [tr.dataset.kemiBlock, tr.className, Array.from(tr.children).map(c => c.tagName + ':' + c.textContent).join('|')])`);
  assert.deepEqual(rows, [
    ['new:1-1', 'kb', 'TH:name|TH:note'],
    ['new:2-2', 'kb', 'TD:x|TD:fine'],
    ['old:3-3', 'kb kb-del', 'TD:y|TD:two'],
    ['new:3-3', 'kb kb-add', 'TD:y|TD:three'],
  ]);
  console.log('PASS 表は見出し行が <th> で、書き換えの行が上下に並ぶ');

  // (7) SVG は切り替えありで既定が描画表示、PNG は切り替え無しで描画表示。
  await selectFile('art/icon.svg');
  await waitFor(`${renderedShown} && document.querySelectorAll('#rendered-doc .kb-image img').length === 2`);
  assert.deepEqual(
    await evaluate(`Array.from(document.querySelectorAll('#file-header .view-switch button')).map(b => b.textContent + ':' + b.getAttribute('aria-pressed'))`),
    ['Rendered:true', 'Source:false'],
  );
  await browser('press', 'r');
  await waitFor(`document.querySelector('#rendered-doc').hidden && document.querySelectorAll('[data-kemi-row]').length > 0`);
  await selectFile('art/photo.png');
  await waitFor(`${renderedShown} && document.querySelectorAll('#rendered-doc .kb-image img').length === 2`);
  assert.equal(await evaluate(`document.querySelectorAll('#file-header .view-switch').length`), 0);
  console.log('PASS SVG は切り替えありで既定が描画表示、PNG は切り替え無し');

  // (8) 画像は 2 列で左右、1 列で上下に並び、バイト数が付く。改名で同じバイト列なら 1 枚と "unchanged"。
  await waitFor(`Array.from(document.querySelectorAll('#rendered-doc .kb-image img')).every(i => i.complete && i.naturalWidth === 1)`);
  assert.deepEqual(
    await evaluate(`Array.from(document.querySelectorAll('#rendered-doc .kb-image')).map(f => f.className + '/' + f.querySelector('figcaption').textContent)`),
    ['kb-image kb-image-old/old69 B', 'kb-image kb-image-new/new69 B'],
  );
  await browser('press', 's');
  await waitFor(`getComputedStyle(document.querySelector('#rendered-doc .kb-images')).flexDirection === 'row'`);
  await browser('press', 'u');
  await waitFor(`getComputedStyle(document.querySelector('#rendered-doc .kb-images')).flexDirection === 'column'`);
  await selectFile('art/renamed.png');
  await waitFor(`${renderedShown} && document.querySelector('#rendered-doc .kb-image-same') !== null`);
  assert.equal(await evaluate(`document.querySelector('#rendered-doc .kb-image-same figcaption').textContent`), 'unchanged69 B');
  console.log('PASS 画像は 2 列で左右・1 列で上下に並び、改名で同じなら 1 枚と unchanged');

  // (9) 壊れた画像はバイト数だけの枠になる。
  await selectFile('art/broken.png');
  await waitFor(`${renderedShown} && document.querySelector('#rendered-doc .kb-image-new .kb-image-broken') !== null`);
  assert.equal(await evaluate(`document.querySelector('#rendered-doc .kb-image-new .kb-img-path').textContent`), '11 B');
  assert.equal(await evaluate(`document.querySelector('#rendered-doc .kb-image-old img').naturalWidth`), 1);
  console.log('PASS 読み込みに失敗した側はバイト数だけの枠になる');

  // (10) symlink・無いファイルの相対パス画像は 404 で枠、レビュー対象とリポジトリの画像は出る。
  await selectFile('docs/images.md');
  await browser('press', 'r');
  await waitFor(`${renderedShown} && document.querySelectorAll('#rendered-doc .rendered-body .kb-img-frame').length === 2 && Array.from(document.querySelectorAll('#rendered-doc .rendered-body img')).every(i => i.complete && i.naturalWidth === 1)`);
  const images = await evaluate(`Array.from(document.querySelectorAll('#rendered-doc .rendered-body img, #rendered-doc .rendered-body .kb-img-frame')).map(e => e.tagName === 'IMG' ? 'img:' + e.getAttribute('src').replace(/review[/]f[0-9]+/, 'review/<id>').replace(/repo[/]f[0-9]+/, 'repo/<id>') : 'frame:' + e.title)`);
  assert.deepEqual(images, ['img:api/image/review/<id>/new', 'frame:img/link.png', 'frame:img/missing.png', 'img:api/image/repo/<id>/new/docs/img/base.png']);
  console.log('PASS symlink と無いファイルの相対パス画像は枠になり、他は出る');

  // (12) 表の途中で消えた段落は表の中に書き出されず、ページのブロックの並び（data-kemi-block）が
  // api/render の blocks の並びと一致する（ページは DOM の並びの添字で blocks を引く）。
  await selectFile('docs/table.md');
  await browser('press', 'r');
  await waitFor(`${renderedShown} && document.querySelectorAll('#rendered-doc [data-kemi-block]').length === 5`);
  const served = await evaluate(`fetch('api/review').then(r => r.json())
    .then(review => review.groups.flatMap(g => g.files).find(f => f.path === 'docs/table.md'))
    .then(file => fetch('api/render/' + encodeURIComponent(file.id)).then(r => r.json()))
    .then(data => data.blocks.map(b => b.side + ':' + b.start + '-' + b.end))`);
  assert.equal(served.length, 5, served.join(','));
  assert.ok(served.includes('old:5-5'), served.join(','));
  assert.deepEqual(await blocks(), served);
  assert.equal(await evaluate(`document.querySelectorAll('#rendered-doc table > :not(thead, tbody), #rendered-doc tbody > :not(tr), #rendered-doc thead > :not(tr)').length`), 0);
  console.log('PASS 表の途中で消えた段落が表の中に入らず、ブロックの並びが一致する');
} finally {
  kemi.child.kill('SIGTERM');
}
const paused = await kemi.exited;
assert.equal(paused.code, 130, paused.stderr());
const resumeId = paused.stderr().match(/kemi: resume with: kemi --resume (\S+)/)?.[1];
assert.ok(resumeId, paused.stderr());

// (11) 保留したセッションを復元すると Markdown を描画表示にでき、相対パス画像はレビュー対象の
// ものだけ出て他は枠になり、切り替えはソース表示から始まる。
kemi = await startKemi(fixture, state, ['--resume', resumeId]);
try {
  await browser('open', kemi.url);
  await waitFor(`document.querySelectorAll('#tree button').length > 0`);
  await selectFile('docs/images.md');
  assert.equal(await evaluate(`document.querySelector('#file-header .view-rendered').getAttribute('aria-pressed')`), 'false');
  assert.equal(await evaluate(`document.querySelector('#rendered-doc').hidden`), true);
  await browser('press', 'r');
  await waitFor(`${renderedShown} && document.querySelectorAll('#rendered-doc .rendered-body .kb-img-frame').length === 3 && Array.from(document.querySelectorAll('#rendered-doc .rendered-body img')).every(i => i.complete && i.naturalWidth === 1)`);
  const restored = await evaluate(`Array.from(document.querySelectorAll('#rendered-doc .rendered-body img, #rendered-doc .rendered-body .kb-img-frame')).map(e => e.tagName === 'IMG' ? 'img:' + e.getAttribute('src').replace(/review[/]f[0-9]+/, 'review/<id>') : 'frame:' + e.title)`);
  assert.deepEqual(restored, ['img:api/image/review/<id>/new', 'frame:img/link.png', 'frame:img/missing.png', 'frame:img/base.png']);
  await selectFile('docs/guide.md');
  await browser('press', 'r');
  await waitFor(renderedShown);
  assert.deepEqual(await blocks(), EXPECTED_BLOCKS);
  console.log('PASS 復元した Markdown を描画表示にでき、相対パス画像はレビュー対象のものだけ出る');
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
