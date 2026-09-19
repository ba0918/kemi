// 狭い画面（R-NARROW）のブラウザ自動化。実際の kemi バイナリをコミット範囲モードで起動し、
// agent-browser のビューポートを 390px（狭い画面）と 1280px（広い画面）で切り替えて確かめる。
//
//   node scripts/test-narrow-screen.mjs <kemi-bin>
//
// 1 回目の起動: 引き出し、1 列と折返し、折返しの記憶と localStorage、ファイルヘッダの 2 行、
// 吹き出しの左端と画像の並び、n での引き出し、幅をまたいだ表示モード、幅をまたいだときの
// 選択・下書き・上端の行、上部バーの 2 段、「…」のメニュー、320px の進捗、title と
// コメント一覧のシート、広い画面の上部バー。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { deflateSync, crc32 } from 'node:zlib';
import { mkdtemp, writeFile, mkdir, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';

const run = promisify(execFile);
if (!process.argv[2]) {
  console.error('usage: node scripts/test-narrow-screen.mjs <kemi-bin>');
  process.exit(2);
}
const binary = resolve(process.argv[2]);
const scripts = dirname(fileURLToPath(import.meta.url));
const session = `kemi-narrow-${process.pid}`;
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
      width: window.innerWidth,
      path: document.querySelector('#file-header .path')?.textContent,
      notice: document.querySelector('#notice')?.textContent,
      rows: document.querySelectorAll('[data-kemi-row]').length,
      drawer: document.querySelector('#tree')?.dataset.drawer,
      wrap: document.querySelector('#diff-content')?.dataset.wrap,
      treePosition: getComputedStyle(document.querySelector('#tree')).position,
    })`).catch(() => 'no snapshot');
    throw new Error(`wait failed for: ${code}\npage: ${snapshot}`, { cause: error });
  }
};

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

const LONG_PATH = '.github/workflows/deeply/nested/directory/with/a/long/name/ci.yml';
const LINES = (count, prefix) => Array.from({ length: count }, (_, i) => `${prefix} line ${i + 1}\n`).join('');

/**
 * gen-fixture の 2 コミットの上に、画像を足すコミットと、範囲に入れる 1 コミット（画像の
 * 変更、長いパスの追加、行の削除と書き換え）を積む。範囲は画像を足したコミットから HEAD。
 */
async function makeFixture() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-narrow-'));
  await run('bash', [join(scripts, 'gen-fixture.sh'), dir, '--files', '4', '--lines', '40', '--commits', '2']);
  const git = (...args) => run('git', ['-C', dir, ...args], {
    env: { ...process.env, GIT_AUTHOR_DATE: '2026-01-01T00:00:00+00:00', GIT_COMMITTER_DATE: '2026-01-01T00:00:00+00:00' },
  });
  const write = (relative, content) => writeFile(join(dir, relative), content);
  await git('checkout', '--', '.');
  await mkdir(join(dir, 'art'), { recursive: true });
  await write('art/photo.png', png(255, 0, 0));
  await git('add', '-A');
  await git('commit', '-q', '-m', 'add photo');
  const from = (await git('rev-parse', 'HEAD')).stdout.trim();
  await mkdir(join(dir, dirname(LONG_PATH)), { recursive: true });
  await write(LONG_PATH, LINES(30, 'step'));
  await write('art/photo.png', png(0, 0, 255));
  const file0 = (await readFile(join(dir, 'src/dir0/file0.txt'), 'utf8')).split('\n');
  // 5〜8 行目を消し、20 行目を書き換える。旧側だけの行と、折りたたみで分かれた 2 つの塊ができる。
  const changed = [...file0.slice(0, 4), ...file0.slice(8, 19), 'file 0 line 20 changed', ...file0.slice(20)];
  await write('src/dir0/file0.txt', changed.join('\n'));
  await git('add', '-A');
  await git('commit', '-q', '-m', 'feat: narrow fixture');
  return { dir, from };
}

async function startKemi(dir, state, args) {
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

const NARROW = ['390', '844'];
const WIDE = ['1280', '800'];
const narrowApplied = `getComputedStyle(document.querySelector('#tree')).position === 'fixed' && document.querySelector('#diff-content').dataset.wrap === 'on'`;
const wideApplied = `getComputedStyle(document.querySelector('#tree')).position !== 'fixed'`;
const notLoading = `!document.querySelector('#notice') || document.querySelector('#notice').hidden || !/Loading|Rendering/.test(document.querySelector('#notice').textContent)`;
const at = (path) => `document.querySelector('#file-header .path')?.textContent === ${JSON.stringify(path)}`;
const drawerOpen = `document.querySelector('#tree').dataset.drawer === 'open'`;
const drawerClosed = `document.querySelector('#tree').dataset.drawer !== 'open'`;
const rect = (selector) => evaluate(`JSON.stringify(document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect())`).then(JSON.parse);
const isShown = (selector) => evaluate(`(() => { const e = document.querySelector(${JSON.stringify(selector)}); return e !== null && getComputedStyle(e).display !== 'none' && e.getClientRects().length > 0; })()`);
const storageSnapshot = () => evaluate(`JSON.stringify(Object.entries(localStorage).sort())`);
/** 表示領域の上端に見えている行の、旧・新の行番号。 */
const topRow = () => evaluate(`(() => {
  const top = document.querySelector('#diff-viewport').getBoundingClientRect().top;
  const block = Array.from(document.querySelectorAll('#diff-content .row-block')).find(b => b.getBoundingClientRect().bottom > top + 1);
  return block ? Array.from(block.querySelectorAll('.num')).map(n => n.textContent).join('|') : null;
})()`);

/** 画面の座標を実際のマウスで押す（引き出しの外の覆いなど、中心が別の要素に覆われているもの）。 */
async function clickAt(x, y) {
  await browser('mouse', 'move', String(x), String(y));
  await browser('mouse', 'down');
  await browser('mouse', 'up');
}

/** ツリーからファイルを選び、取得が終わるのを待つ。 */
async function selectFile(path) {
  const name = path.split('/').pop();
  await evaluate(`Array.from(document.querySelectorAll('#tree button.file')).find(b => b.title === ${JSON.stringify(path)} || b.textContent.includes(${JSON.stringify(name)})).click(); true`);
  await waitFor(`${at(path)} && ${notLoading}`);
}

/** 新側の行番号の要素。 */
const newNumber = (number) => `Array.from(document.querySelectorAll('#diff-content .row .no-cell:nth-child(2) .num')).find(n => n.textContent === ${JSON.stringify(String(number))})`;

/**
 * 広い画面のドラッグ: 新側の行番号を押し下げてから別の行へ動かし、範囲を選ぶ。行にホバー
 * すると `+` が出て行番号が左へずれるので、乗せてから位置を測り直して押す。
 */
async function dragSelect(fromNumber, toNumber) {
  const moveTo = async (number) => {
    for (let pass = 0; pass < 2; pass += 1) {
      const box = await evaluate(`JSON.stringify(${newNumber(number)}.getBoundingClientRect())`).then(JSON.parse);
      await browser('mouse', 'move', String(Math.round(box.left + box.width / 2)), String(Math.round(box.top + box.height / 2)));
    }
  };
  await moveTo(fromNumber);
  await browser('mouse', 'down');
  await moveTo(toNumber);
  await browser('mouse', 'up');
}

const { dir: fixture, from } = await makeFixture();
const state = await mkdtemp(join(tmpdir(), 'kemi-narrow-state-'));
let kemi = await startKemi(fixture, state, ['--from', from]);
try {
  await browser('set', 'viewport', ...WIDE);
  await browser('open', kemi.url);
  await waitFor(`document.querySelectorAll('#tree button').length > 0`);
  await selectFile('src/dir0/file0.txt');
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
  const wideTreeCount = await evaluate(`document.querySelectorAll('#tree button').length`);
  // 吹き出しの検査のために、広い画面のドラッグで範囲コメントを 1 つ付けておく。
  await dragSelect(2, 3);
  await browser('hover', `#diff-content .row .num.selected`);
  await evaluate(`document.querySelector('#diff-content .row:has(.num.selected) .line-add-btn').click(); true`);
  await waitFor(`document.querySelector('#diff-content .editor textarea[data-editor-field="body"]') !== null`);
  await browser('fill', '#diff-content .editor textarea[data-editor-field="body"]', 'wide range comment');
  await browser('click', '#diff-content .editor button[type="submit"]');
  await waitFor(`document.querySelector('#comment-count').textContent === '1'`);

  // 幅を 390px にすると、再読込なしに狭い画面になる（matchMedia の change が届く）。
  await browser('set', 'viewport', ...NARROW);
  await waitFor(narrowApplied);

  // (1) ツリーが見えず、引き出しで開き、中身が同じで、ファイル選択と外を押すことで閉じる。
  const treeBox = await rect('#tree');
  assert.ok(treeBox.right <= 0, `tree should be off screen: ${JSON.stringify(treeBox)}`);
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('#tree')).visibility`), 'hidden');
  const mainBox = await rect('#main');
  assert.equal(Math.round(mainBox.left), 0);
  assert.equal(Math.round(mainBox.width), 390);
  await browser('click', '#btn-tree');
  await waitFor(`${drawerOpen} && document.querySelector('#tree').getBoundingClientRect().left === 0`);
  assert.equal(await evaluate(`document.querySelectorAll('#tree button').length`), wideTreeCount);
  await selectFile(LONG_PATH);
  await waitFor(drawerClosed);
  await browser('click', '#btn-tree');
  // 開く動きが終わってから幅を測る。
  await waitFor(`${drawerOpen} && document.querySelector('#tree').getBoundingClientRect().right >= 329`);
  const drawerBox = await rect('#tree');
  assert.ok(drawerBox.right < 390, `the drawer should leave room outside: ${JSON.stringify(drawerBox)}`);
  await clickAt(Math.round((drawerBox.right + 390) / 2), 400);
  await waitFor(drawerClosed);
  console.log('PASS 390px でツリーは引き出しになり、ファイル選択と外を押すことで閉じる');

  // (2) 行は 1 列で折り返され、2 列への切り替えが見えない。
  assert.ok(await evaluate(`document.querySelectorAll('[data-kemi-row]').length`) > 0);
  assert.equal(await evaluate(`document.querySelectorAll('[data-kemi-row] .row.split').length`), 0);
  assert.equal(await evaluate(`document.querySelector('#diff-content').dataset.wrap`), 'on');
  assert.equal(await isShown('#btn-split'), false);
  assert.equal(await isShown('#btn-unified'), false);
  console.log('PASS 390px では 1 列で折り返され、2 列の切り替えが見えない');

  // (3) 折返しの切り替えは別のファイルへ移っても保たれ、再読込で既定に戻り、localStorage は変わらない。
  const storageBefore = await storageSnapshot();
  await browser('press', 'w');
  await waitFor(`document.querySelector('#diff-content').dataset.wrap !== 'on'`);
  await selectFile('src/dir0/file0.txt');
  assert.notEqual(await evaluate(`document.querySelector('#diff-content').dataset.wrap`), 'on');
  assert.equal(await storageSnapshot(), storageBefore);
  await browser('reload');
  await waitFor(`document.querySelectorAll('#tree button').length > 0 && document.querySelectorAll('[data-kemi-row]').length > 0`);
  assert.equal(await evaluate(`document.querySelector('#diff-content').dataset.wrap`), 'on');
  console.log('PASS 390px の折返しはページを開いている間だけ覚え、localStorage に入れない');

  // (4) ファイルヘッダは 2 行で、長いパスは先頭が省略記号で切れて末尾が見える。
  await browser('click', '#btn-tree');
  await waitFor(drawerOpen);
  await selectFile(LONG_PATH);
  await waitFor(drawerClosed);
  const pathBox = await rect('#file-header .path');
  const firstButtonBox = await rect('#file-header button');
  assert.ok(pathBox.bottom <= firstButtonBox.top + 1, `path should be above the actions: ${pathBox.bottom} vs ${firstButtonBox.top}`);
  const clipped = await evaluate(`(() => {
    const path = document.querySelector('#file-header .path');
    const text = path.querySelector('bdi').firstChild;
    const range = document.createRange();
    range.setStart(text, 0); range.setEnd(text, 1);
    const first = range.getBoundingClientRect();
    range.setStart(text, text.length - 1); range.setEnd(text, text.length);
    const last = range.getBoundingClientRect();
    const box = path.getBoundingClientRect();
    return JSON.stringify({ overflow: path.scrollWidth > path.clientWidth, ellipsis: getComputedStyle(path).textOverflow, firstLeft: first.left - box.left, lastRight: last.right - box.right });
  })()`).then(JSON.parse);
  assert.equal(clipped.overflow, true, JSON.stringify(clipped));
  assert.equal(clipped.ellipsis, 'ellipsis');
  assert.ok(clipped.firstLeft < 0, `the head should be cut: ${JSON.stringify(clipped)}`);
  assert.ok(clipped.lastRight <= 1, `the tail should be visible: ${JSON.stringify(clipped)}`);
  console.log('PASS 390px のファイルヘッダは 2 行で、長いパスは先頭が省略記号で切れる');

  // (5) 吹き出しの左端が本文の左端と一致し、画像が上下に並ぶ。
  await browser('click', '#btn-tree');
  await waitFor(drawerOpen);
  await selectFile('src/dir0/file0.txt');
  // 再読込で札に畳まれているので、押して吹き出しにしてから測る。
  await waitFor(`${drawerClosed} && document.querySelector('#diff-content .bal-row .cchip') !== null`);
  await evaluate(`document.querySelector('#diff-content .bal-row .cchip').click(); true`);
  await waitFor(`document.querySelector('#diff-content .bal') !== null`);
  const balloonBox = await rect('#diff-content .bal');
  const contentBox = await rect('#diff-content');
  assert.equal(Math.round(balloonBox.left), Math.round(contentBox.left));
  assert.ok(balloonBox.width <= 390, `balloon should fit: ${balloonBox.width}`);
  await browser('click', '#btn-tree');
  await waitFor(drawerOpen);
  await selectFile('art/photo.png');
  await waitFor(`${drawerClosed} && document.querySelectorAll('#rendered-doc .kb-image').length === 2`);
  assert.equal(await evaluate(`getComputedStyle(document.querySelector('#rendered-doc .kb-images')).flexDirection`), 'column');
  console.log('PASS 390px の吹き出しは本文の左端から始まり、画像は上下に並ぶ');

  // (6) n でファイルをまたいでも引き出しは開かない。
  await browser('click', '#btn-tree');
  await waitFor(drawerOpen);
  await selectFile(LONG_PATH);
  await waitFor(drawerClosed);
  for (let step = 0; step < 12; step += 1) {
    await browser('press', 'n');
    assert.equal(await evaluate(drawerClosed), true, `drawer opened after n (${step})`);
    if (await evaluate(at('src/dir0/file0.txt'))) {
      break;
    }
  }
  await waitFor(`${at('src/dir0/file0.txt')} && ${notLoading}`);
  assert.equal(await evaluate(drawerClosed), true);
  console.log('PASS n でファイルをまたいでも引き出しは開かない');

  // (7) 1280px で 2 列を選び、390px で 1 列、1280px に戻すと 2 列のまま。
  await browser('set', 'viewport', ...WIDE);
  await waitFor(wideApplied);
  await browser('press', 's');
  await waitFor(`document.querySelectorAll('[data-kemi-row] .row.split').length > 0`);
  await browser('set', 'viewport', ...NARROW);
  await waitFor(`${narrowApplied} && document.querySelectorAll('[data-kemi-row] .row.split').length === 0`);
  await browser('set', 'viewport', ...WIDE);
  await waitFor(`${wideApplied} && document.querySelectorAll('[data-kemi-row] .row.split').length > 0`);
  assert.notEqual(await evaluate(`document.querySelector('#diff-content').dataset.wrap`), 'on');
  await browser('press', 'u');
  await waitFor(`document.querySelectorAll('[data-kemi-row] .row.split').length === 0`);
  console.log('PASS 幅をまたいでも広い画面の表示モードは覚えたまま');

  // (8) 390px で選択と下書きを作り、引き出しを開いてから 1280px にすると、引き出しは閉じ、
  //     選択と下書きは残り、上端に見えていた行が同じ位置にある。
  await browser('set', 'viewport', ...NARROW);
  await waitFor(narrowApplied);
  // 全行を展開して本文を長くし、上端の行が先頭でない位置まで送る。
  await evaluate(`document.querySelector('#file-header button[aria-label="Expand all lines"]').click(); true`);
  await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 30 && ${notLoading}`);
  await evaluate(`document.querySelector('#diff-viewport').scrollTop = 120; true`);
  await waitFor(`document.querySelector('#diff-viewport').scrollTop === 120`);
  await evaluate(`(() => {
    const num = Array.from(document.querySelectorAll('#diff-content .row .no-cell:nth-child(2) .num')).find(n => n.textContent === '17');
    num.scrollIntoView({ block: 'center' });
    return true;
  })()`);
  await evaluate(`(() => {
    const num = Array.from(document.querySelectorAll('#diff-content .row .no-cell:nth-child(2) .num')).find(n => n.textContent === '17');
    num.closest('.row').querySelector('.line-add-btn').click();
    return true;
  })()`);
  await waitFor(`document.querySelector('#diff-content .editor textarea[data-editor-field="body"]') !== null`);
  await browser('fill', '#diff-content .editor textarea[data-editor-field="body"]', 'kept draft');
  const selectedBefore = await evaluate(`Array.from(document.querySelectorAll('#diff-content .num.selected')).map(n => n.textContent).join(',')`);
  assert.equal(selectedBefore, '17');
  const topBefore = await topRow();
  assert.ok(topBefore, 'no top row');
  await browser('click', '#btn-tree');
  await waitFor(drawerOpen);
  await browser('set', 'viewport', ...WIDE);
  await waitFor(`${wideApplied} && ${drawerClosed}`);
  assert.equal(await evaluate(`Array.from(document.querySelectorAll('#diff-content .num.selected')).map(n => n.textContent).join(',')`), '17');
  assert.equal(await evaluate(`document.querySelector('#diff-content .editor textarea[data-editor-field="body"]').value`), 'kept draft');
  assert.equal(await topRow(), topBefore);
  console.log('PASS 幅をまたぐと引き出しは閉じ、選択と下書きと上端の行は残る');

  // (9) 390px の上部バーは 2 段。1 段目に kemi・title・ツリー・コメント一覧の入口・更新バッジ、
  //     2 段目にグループ単位の切り替え（左端）・見たの進捗・Approve・Request changes。
  await browser('set', 'viewport', ...NARROW);
  await waitFor(narrowApplied);
  const bar = await evaluate(`JSON.stringify(Object.fromEntries(['.brand', '#btn-title', '#btn-tree', '#btn-comments', '#btn-more', '#unit-switch', '#progress', '#btn-approve', '#btn-changes'].map(s => {
    const box = document.querySelector(s).getBoundingClientRect();
    return [s, { top: box.top, bottom: box.bottom, left: box.left, right: box.right, mid: (box.top + box.bottom) / 2 }];
  })))`).then(JSON.parse);
  const firstRowBottom = Math.max(bar['.brand'].bottom, bar['#btn-tree'].bottom, bar['#btn-more'].bottom);
  for (const key of ['.brand', '#btn-title', '#btn-tree', '#btn-comments', '#btn-more']) {
    assert.ok(bar[key].mid < firstRowBottom && bar[key].right <= 390, `${key} should be on the first row: ${JSON.stringify(bar[key])}`);
  }
  for (const key of ['#unit-switch', '#progress', '#btn-approve', '#btn-changes']) {
    assert.ok(bar[key].top >= firstRowBottom - 1 && bar[key].right <= 390, `${key} should be below the first row: ${JSON.stringify(bar[key])}`);
  }
  assert.ok(bar['#unit-switch'].left <= bar['#progress'].left, 'the unit switch should be at the left of the second row');
  assert.equal(await evaluate(`document.querySelector('#update-badge').hidden`), true);
  assert.equal(await isShown('#review-subtitle'), false);
  assert.equal(await isShown('#review-meta'), false);
  console.log('PASS 390px の上部バーは 2 段で、1 段目と 2 段目の要素が仕様の並び');

  // (10) 「…」を押すと折返し・重要のみ・変更量順・テーマがこの順で文字ラベル付きに出る。
  await browser('click', '#btn-more');
  await waitFor(`document.querySelector('#view-menu').matches(':popover-open')`);
  const menuLabels = await evaluate(`Array.from(document.querySelectorAll('#view-menu button')).map(b => b.textContent.trim())`);
  assert.deepEqual(menuLabels.slice(0, 3), ['Wrap', 'Important only', 'Sort by change size']);
  assert.match(menuLabels[3], /^Theme: /);
  assert.equal(menuLabels.length, 4);
  assert.equal(await evaluate(`document.querySelector('#view-menu button').getAttribute('aria-pressed')`), 'true');
  await browser('press', 'Escape');
  await waitFor(`!document.querySelector('#view-menu').matches(':popover-open')`);
  console.log('PASS 「…」のメニューに表示の操作が文字ラベル付きで並ぶ');

  // (11) 320px では見たの進捗が数字だけになり、送信ボタンの文言は変わらない。
  await browser('set', 'viewport', '320', '844');
  await waitFor(`document.querySelector('#progress').dataset.compact !== undefined`);
  assert.equal(await isShown('#progress .bar'), false);
  assert.equal(await isShown('#progress-text'), true);
  assert.match(await evaluate(`document.querySelector('#progress-text').textContent`), /^Seen \d+ \/ \d+$/);
  assert.equal(await evaluate(`document.querySelector('#btn-approve').textContent.trim()`), 'Approve');
  assert.equal(await evaluate(`document.querySelector('#btn-changes').textContent.trim()`), 'Request changes');
  assert.equal(await evaluate(`document.querySelector('#btn-changes').getBoundingClientRect().right <= 320`), true);
  await browser('set', 'viewport', ...NARROW);
  await waitFor(narrowApplied);
  console.log('PASS 320px では見たの進捗が数字だけになり、送信ボタンの文言は変わらない');

  // (12) title のシートとコメント一覧のシートが開いて閉じる。
  await browser('click', '#btn-title');
  await waitFor(`document.querySelector('#title-sheet').matches(':popover-open')`);
  const sheetBox = await rect('#title-sheet');
  assert.deepEqual([sheetBox.left, sheetBox.top, sheetBox.width, sheetBox.height].map(Math.round), [0, 0, 390, 844]);
  assert.equal(await evaluate(`document.querySelector('#sheet-title').textContent`), await evaluate(`document.querySelector('#review-title').textContent`));
  assert.ok(await evaluate(`document.querySelectorAll('#sheet-meta span').length`) > 0, 'the sheet should carry the meta');
  await browser('click', '#sheet-close');
  await waitFor(`!document.querySelector('#title-sheet').matches(':popover-open')`);
  await browser('click', '#btn-comments');
  await waitFor(`!document.querySelector('#comment-list').hidden`);
  const listBox = await rect('#comment-list');
  assert.deepEqual([listBox.left, listBox.top, listBox.width, listBox.height].map(Math.round), [0, 0, 390, 844]);
  await browser('click', '#comment-list .cl-close');
  await waitFor(`document.querySelector('#comment-list').hidden`);
  console.log('PASS title のシートとコメント一覧のシートが開いて閉じる');

  // (13) 1280px では、文字ラベルを持つ上部バーの操作がグループ単位・送信・更新バッジ・
  //      コメント一覧の入口だけで、subtitle と meta が上部に出て、狭い画面専用の操作が見えない。
  await browser('set', 'viewport', ...WIDE);
  await waitFor(wideApplied);
  const labelled = await evaluate(`Array.from(document.querySelectorAll('.topbar button')).filter(b => b.textContent.trim() !== '' && getComputedStyle(b).display !== 'none' && b.getClientRects().length > 0).map(b => b.id || b.className)`);
  assert.deepEqual([...new Set(labelled)].sort(), ['btn-approve', 'btn-changes', 'btn-comments', 'unit-button']);
  assert.equal(await isShown('#review-meta'), true);
  assert.ok(await evaluate(`document.querySelectorAll('#review-meta span').length`) > 0);
  assert.equal(await evaluate(`document.querySelector('#review-subtitle').hidden || getComputedStyle(document.querySelector('#review-subtitle')).display !== 'none'`), true);
  for (const selector of ['#btn-tree', '#btn-title', '#btn-more', '#view-menu', '#title-sheet', '#drawer-scrim']) {
    assert.equal(await isShown(selector), false, `${selector} should be hidden on a wide screen`);
  }
  assert.equal(await isShown('#btn-split'), true);
  console.log('PASS 1280px では上部バーの文字ラベルの操作が 4 種類だけで、狭い画面専用の操作が見えない');
} finally {
  kemi.child.kill('SIGTERM');
  await kemi.exited;
}
await run('agent-browser', ['--session', session, 'close']);
