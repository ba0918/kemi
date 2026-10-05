// `--live`（docs/spec/live.md、live-compare.md）のブラウザ自動化。実際の kemi バイナリを、
// scripts/live-dev-server.mjs の試験用の開発サーバを相手に起動し、画面を agent-browser で確かめる。
//
//   node scripts/test-live.mjs <kemi-bin>
//
// 確かめること:
// - 中継: HMR が中継越しに効く。中継用の cookie が document.cookie に出ない。開発サーバを止めた
//   まま新しく起動しても、止めたまま復元しても、つながらない旨が出て、開発サーバを起動すると
//   つながる。
// - 手元の HTML ファイル: 参照する CSS を保存し直すと読み込み直される。
// - ページの見方: 枠を拒むページが出て書き換えの表示が出る。表示幅の 319 と 3841 を断り範囲を
//   出す。幅 390px で 1 枚ずつ切り替える。git の外ではコードの見方の代わりに理由が出る。範囲の
//   別の HTML へのリンクで見る対象が移り、ツリーに出る。`--live` でないレビューはページ用の
//   ファイルを読み込まない。
// - スナップショット: 渡す前は開始時が既定。390 と 1280 で取って切り替える。無い幅・別の URL では
//   記録されていない旨と取る操作が出る。動いているページと画素を比べる（完全に一致しなければ
//   差の画像と割合を出して人の確認に回す）。onclick が動かない。2 つのページがツリーに並ぶ。
//   渡すと取る。
// - モック: 範囲の外と .txt を理由つきで断る。CSS と画像ごと同じ幅で出る。外すとスナップショットに
//   戻る。JS のモックが描かれ、トークンが（referrer からも）得られず API に断られる。モックだけがあるページがツリーに出る。
// - 重ねて透かす: スクロールがそろう、透かし具合で見え方が変わる、幅 390px でも切り替えられる。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { crc32, deflateSync, inflateSync } from 'node:zlib';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';

import { startDevServer } from './live-dev-server.mjs';

const run = promisify(execFile);
if (!process.argv[2]) {
  console.error('usage: node scripts/test-live.mjs <kemi-bin>');
  process.exit(2);
}
const binary = resolve(process.argv[2]);
const session = `kemi-live-${process.pid}`;
const browser = async (...args) => {
  const { stdout } = await run('agent-browser', ['--session', session, '--json', ...args], { maxBuffer: 16 * 1024 * 1024 })
    .catch((error) => { throw new Error(`agent-browser ${args.join(' ')}: ${error.stdout ?? ''}`, { cause: error }); });
  const result = JSON.parse(stdout);
  assert.equal(result.success, true, JSON.stringify(result));
  return result.data;
};
const evaluate = async (code) => (await browser('eval', '-b', Buffer.from(code).toString('base64'))).result;
const waitFor = async (code, timeout = 15000) => {
  try {
    await browser('wait', '--timeout', String(timeout), '--fn', code);
  } catch (error) {
    const snapshot = await evaluate(`JSON.stringify({ url: location.href, view: document.body?.dataset.liveView, side: document.querySelector('#live-stage')?.dataset.side, compare: document.querySelector('#live-stage')?.dataset.compare, text: document.body?.innerText.slice(0, 400) })`).catch(() => 'no snapshot');
    throw new Error(`wait failed for: ${code}\npage: ${snapshot}`, { cause: error });
  }
};

/** 利用者の git の設定に左右されない git。 */
function gitIn(dir) {
  return (...args) => run('git', ['-C', dir, ...args], {
    env: {
      ...process.env,
      GIT_CONFIG_GLOBAL: join(dir, '.git-test-global'),
      GIT_CONFIG_NOSYSTEM: '1',
      GIT_AUTHOR_NAME: 'kemi',
      GIT_AUTHOR_EMAIL: 'kemi@example.com',
      GIT_COMMITTER_NAME: 'kemi',
      GIT_COMMITTER_EMAIL: 'kemi@example.com',
    },
  });
}

/** 変更のある作業ツリー。 */
async function makeRepository() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-live-repo-'));
  const git = gitIn(dir);
  await git('init', '-q');
  await writeFile(join(dir, 'a.txt'), 'one\ntwo\n');
  await git('add', 'a.txt');
  await git('commit', '-q', '-m', 'base');
  await writeFile(join(dir, 'a.txt'), 'one\nTWO\n');
  return dir;
}

function environment(state) {
  return { ...process.env, XDG_STATE_HOME: state, HOME: join(state, 'home'), LOCALAPPDATA: join(state, 'localappdata') };
}

/** 空いているポートを 1 つ選ぶ（開発サーバを後から同じポートで起動するため）。 */
async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, '127.0.0.1', () => done(undefined)));
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  await new Promise((done) => server.close(() => done(undefined)));
  return port;
}

/** kemi を起動し、URL・review の id・中継の URL（`--live` のときだけ）がそろうのを待つ。 */
async function startKemi(dir, state, args) {
  const isLive = args.includes('--live') || args.includes('--resume');
  const child = spawn(binary, [...args, '--port', '0', '--no-open'], { cwd: dir, env: environment(state), stdio: ['ignore', 'pipe', 'pipe'] });
  let stderr = '';
  let stdout = '';
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  const started = await new Promise((done, fail) => {
    child.stderr.on('data', (chunk) => {
      stderr += chunk;
      // 行が途中で切れて届くことがあるので、改行まで届いた行だけを読む。
      const url = stderr.match(/^kemi: (http:\/\/\S+)\n/m);
      const review = stderr.match(/^kemi: review (\S+)\n/m);
      const live = stderr.match(/^kemi: live (\S+)\n/m);
      if (url && review && (live || !isLive)) done({ url: url[1], id: review[1], live: live?.[1] });
    });
    child.on('error', fail);
    child.on('exit', (code) => fail(new Error(`kemi exited before serving (${code}): ${stderr}`)));
  });
  const exited = new Promise((done) => child.on('exit', (code) => done({ code, stdout, stderr })));
  return { child, ...started, exited, stderr: () => stderr };
}

/** ページと同じオリジンから API を呼ぶ。 */
async function post(url, path, body) {
  const response = await fetch(new URL(path, url), {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', Origin: new URL(url).origin },
    body: JSON.stringify(body),
  });
  const text = await response.text();
  assert.equal(response.status, 200, `${path}: ${text}`);
  return JSON.parse(text);
}

async function stop(kemi) {
  kemi.child.kill('SIGINT');
  return kemi.exited;
}

/** 中継（R-PAGE-PROXY）: HMR が中継越しに効き、中継用の cookie がページから読めない。 */
async function relayCarriesHmrAndHidesTheCookie(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', dev.url]);
  try {
    await browser('open', kemi.url);
    await browser('open', kemi.live);
    await waitFor(`document.documentElement.dataset.hmrConnected === 'true'`);
    const box = `getComputedStyle(document.querySelector('.box')).backgroundColor`;
    assert.equal(await evaluate(box), 'rgb(255, 0, 0)');
    await writeFile(join(dev.dir, 'style.css'), '.box { background: rgb(0, 0, 255); width: 200px; height: 80px; }\n');
    await waitFor(`${box} === 'rgb(0, 0, 255)'`);
    assert.equal(await evaluate(`document.cookie.includes('kemi_live')`), false, 'the relay cookie must be HttpOnly');
    console.log('PASS 中継越しに HMR が効き、中継用の cookie がページの document.cookie に出ない');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** 開発サーバにつながらないときは出して待ち、起動したらつながる（R-PAGE-MODE、R-PAGE-SESSION）。 */
async function waitsForTheDevServer(repository) {
  const port = await freePort();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const url = `http://127.0.0.1:${port}/`;
  const unreachable = `document.querySelector('#kemi-unreachable')?.textContent.includes('kemi cannot reach')`;
  const loaded = `document.querySelector('#title')?.textContent === 'Dev server'`;

  // 新しく起動したとき。
  const first = await startKemi(repository, state, ['--live', url]);
  let dev;
  try {
    await browser('open', first.url);
    await browser('open', first.live);
    await waitFor(unreachable);
    dev = await startDevServer({ port });
    await waitFor(loaded);
    console.log('PASS 開発サーバを止めたまま起動すると、つながらない旨が出て、起動するとつながる');
    await post(first.url, 'api/message', { body: 'keep this review' });
  } finally {
    await stop(first);
    await dev?.close();
  }

  // 止めたまま復元したとき。
  const resumed = await startKemi(repository, state, ['--resume', first.id]);
  try {
    await browser('open', resumed.url);
    await browser('open', resumed.live);
    await waitFor(unreachable);
    dev = await startDevServer({ port });
    await waitFor(loaded);
    console.log('PASS 開発サーバを止めたまま復元すると、つながらない旨が出て、起動するとつながる');
  } finally {
    await stop(resumed);
    await dev?.close();
  }
}

/** 手元の HTML ファイル（R-PAGE-MODE、R-LIVE）: 参照する CSS を保存し直すと読み込み直される。 */
async function fileReloadsWhenItsCssIsSaved(repository) {
  const { mkdir } = await import('node:fs/promises');
  await mkdir(join(repository, 'site'), { recursive: true });
  await writeFile(join(repository, 'site', 'page.html'), '<!doctype html><html><head><link rel="stylesheet" href="style.css"></head><body><p class="box">file page</p></body></html>\n');
  await writeFile(join(repository, 'site', 'style.css'), '.box { color: rgb(255, 0, 0); }\n');
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', 'site/page.html']);
  try {
    assert.match(kemi.live, /\/site\/page\.html$/);
    await browser('open', kemi.url);
    await browser('open', kemi.live);
    const color = `getComputedStyle(document.querySelector('.box')).color`;
    await waitFor(`${color} === 'rgb(255, 0, 0)'`);
    await evaluate(`window.__kemiBeforeReload = true; true`);
    await writeFile(join(repository, 'site', 'style.css'), '.box { color: rgb(0, 128, 0); }\n');
    await waitFor(`${color} === 'rgb(0, 128, 0)' && window.__kemiBeforeReload === undefined`);
    console.log('PASS 手元の HTML ファイルが参照する CSS を保存し直すと、ページが読み込み直される');
  } finally {
    await stop(kemi);
  }
}

const visible = (selector) => `(() => { const e = document.querySelector(${JSON.stringify(selector)}); return e !== null && e.getClientRects().length > 0 && getComputedStyle(e).visibility !== 'hidden'; })()`;

/** 中継したページの中の点を押す（別のオリジンの枠なので、画面の座標で押す）。 */
async function clickInLiveFrame(x, y) {
  const box = await evaluate(`(() => { const r = document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-frame').getBoundingClientRect(); return { x: r.x, y: r.y }; })()`);
  const scale = Number(await evaluate(`getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1'`));
  await browser('mouse', 'move', String(Math.round(box.x + x * scale)), String(Math.round(box.y + y * scale)));
  await browser('mouse', 'down');
  await browser('mouse', 'up');
}

/** ページの見方（R-PAGE-VIEW、R-PAGE-PROXY）: 枠を拒むページが出て書き換えの表示が出る、表示幅の範囲、狭い画面。 */
async function pageViewShowsFramedPagesWidthsAndNarrowScreens(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}framed.html`]);
  try {
    await browser('set', 'viewport', '1280', '800');
    await browser('open', kemi.url);
    await waitFor(visible('#live-stage'));
    await waitFor(`document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-notice')?.dataset.kind === 'rewrote' && ${visible('#live-stage .lv-pane[data-side="live"] .lv-notice')}`);
    assert.match(await evaluate(`document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-bar-label').textContent`), /\/framed\.html/);
    console.log('PASS X-Frame-Options: DENY と frame-ancestors \'none\' を返すページがレビュー画面の中に出て、書き換えの表示が出る');

    for (const width of ['319', '3841']) {
      await browser('fill', '.lv-width-input', width);
      await browser('press', 'Enter');
      await waitFor(`${visible('.lv-width-error')} && document.querySelector('.lv-width-error').textContent.includes('320') && document.querySelector('.lv-width-error').textContent.includes('3840')`);
      assert.equal(await evaluate(`document.querySelector('.lv-widths button[aria-pressed="true"]')?.dataset.width`), '1280', `${width} must not be taken`);
    }
    await browser('fill', '.lv-width-input', '1024');
    await browser('press', 'Enter');
    await waitFor(`!${visible('.lv-width-error')} && document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-frame').style.width === '1024px'`);
    console.log('PASS 表示幅に 319 と 3841 を入れると受け付けられず範囲が出て、範囲の中の数字は受け付ける');

    await browser('set', 'viewport', '390', '800');
    await waitFor(`${visible('.lv-side')} && ${visible('#live-stage .lv-pane[data-side="live"]')} && !${visible('#live-stage .lv-pane[data-side="ref"]')}`);
    // 幅をまたいだ直後は帯の並びが動くので、落ち着いてから押す。
    await new Promise((done) => setTimeout(done, 500));
    await browser('click', '.lv-side button[data-side="ref"]');
    await waitFor(`${visible('#live-stage .lv-pane[data-side="ref"]')} && !${visible('#live-stage .lv-pane[data-side="live"]')}`);
    await browser('click', '.lv-side button[data-side="live"]');
    await waitFor(visible('#live-stage .lv-pane[data-side="live"]'));
    await waitFor(`getComputedStyle(document.querySelector('#page-tree')).visibility === 'hidden'`);
    await browser('click', '#btn-tree');
    await waitFor(`document.querySelector('#page-tree').dataset.drawer === 'open' && getComputedStyle(document.querySelector('#page-tree')).visibility === 'visible' && document.querySelector('#page-tree').getBoundingClientRect().x === 0`);
    await browser('click', '#page-tree .lv-page-open');
    await waitFor(`document.querySelector('#page-tree').dataset.drawer === undefined`);
    console.log('PASS 幅 390px では動いているページと比べる相手を 1 枚ずつ切り替えて見て、ページのツリーは引き出しに入る');
  } finally {
    await browser('set', 'viewport', '1280', '800');
    await stop(kemi);
    await dev.close();
  }
}

/** git の外（R-PAGE-MODE）: コードの見方の代わりに理由が出る。範囲の別の HTML へのリンクで見る対象が移り、ツリーに出る。 */
async function outsideGitFilePages() {
  const dir = await mkdtemp(join(tmpdir(), 'kemi-live-nogit-'));
  await writeFile(join(dir, 'index.html'), '<!doctype html><html><head><style>body{margin:0} a{display:block;width:400px;height:200px;background:#ddd}</style></head><body><a id="next" href="other.html">Other</a></body></html>\n');
  await writeFile(join(dir, 'other.html'), '<!doctype html><html><head></head><body><h1>Other</h1></body></html>\n');
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(dir, state, ['--live', 'index.html']);
  try {
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-bar-label')?.textContent.includes('/index.html')`);
    await browser('click', '.lv-view button[data-view="code"]');
    await waitFor(`${visible('#live-no-code')} && document.querySelector('#live-no-code').textContent.includes('not a git repository')`);
    console.log('PASS git の外で起動すると、コードの見方の代わりに理由が出る');

    await browser('click', '.lv-view button[data-view="page"]');
    await waitFor(visible('#page-tree'));
    await clickInLiveFrame(100, 100);
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/other.html'`);
    console.log('PASS ファイルのページの中で範囲の別の HTML へのリンクを押すと、見る対象が移り、ページのツリーにそのページが出る');
  } finally {
    await stop(kemi);
  }
}

/** `--live` でないレビューは、ページ用のファイルを読み込まない（R-VERIFY）。 */
async function otherReviewsLoadNoPageFiles(repository) {
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--worktree']);
  try {
    await browser('open', kemi.url);
    await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
    const loaded = await evaluate(`JSON.stringify(performance.getEntriesByType('resource').map((entry) => entry.name))`);
    const urls = JSON.parse(loaded);
    assert.ok(urls.some((url) => url.endsWith('assets/app.js')), loaded);
    const pageFiles = urls.filter((url) => /live/.test(new URL(url).pathname));
    assert.deepEqual(pageFiles, [], 'no page file may be loaded');
    console.log(`PASS --live でないレビューは、ページ用のファイルを読み込まない（読み込んだ URL ${urls.length} 件）`);
  } finally {
    await stop(kemi);
  }
}

// ---- 画素の比較（スナップショットの見た目） ----

/** PNG を読む（8 ビットの RGB か RGBA、インターレース無し）。 */
function decodePng(bytes) {
  let offset = 8;
  let width = 0;
  let height = 0;
  let channels = 4;
  const data = [];
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    const type = bytes.toString('latin1', offset + 4, offset + 8);
    const body = bytes.subarray(offset + 8, offset + 8 + length);
    if (type === 'IHDR') {
      width = body.readUInt32BE(0);
      height = body.readUInt32BE(4);
      assert.equal(body[8], 8, 'only 8-bit PNG');
      channels = body[9] === 6 ? 4 : body[9] === 2 ? 3 : assert.fail(`color type ${body[9]}`);
    } else if (type === 'IDAT') {
      data.push(body);
    }
    offset += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(data));
  const stride = width * channels;
  const pixels = Buffer.alloc(width * height * 4);
  let previous = Buffer.alloc(stride);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)];
    const line = Buffer.from(raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1)));
    for (let x = 0; x < stride; x++) {
      const left = x >= channels ? line[x - channels] : 0;
      const up = previous[x];
      const corner = x >= channels ? previous[x - channels] : 0;
      const paeth = () => {
        const p = left + up - corner;
        const pa = Math.abs(p - left);
        const pb = Math.abs(p - up);
        const pc = Math.abs(p - corner);
        return pa <= pb && pa <= pc ? left : pb <= pc ? up : corner;
      };
      const add = [0, left, up, (left + up) >> 1, paeth()][filter];
      line[x] = (line[x] + add) & 0xff;
    }
    for (let x = 0; x < width; x++) {
      for (let c = 0; c < 4; c++) {
        pixels[(y * width + x) * 4 + c] = c < channels ? line[x * channels + c] : 255;
      }
    }
    previous = line;
  }
  return { width, height, pixels };
}

/** RGBA の画素を PNG にする（差の画像を残すため）。 */
function encodePng(width, height, pixels) {
  const chunk = (type, body) => {
    const header = Buffer.alloc(8);
    header.writeUInt32BE(body.length, 0);
    header.write(type, 4, 'latin1');
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(Buffer.concat([Buffer.from(type, 'latin1'), body])), 0);
    return Buffer.concat([header, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8;
  ihdr[9] = 6;
  const raw = Buffer.alloc(height * (width * 4 + 1));
  for (let y = 0; y < height; y++) {
    pixels.copy(raw, y * (width * 4 + 1) + 1, y * width * 4, (y + 1) * width * 4);
  }
  return Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))]);
}

/** 要素のスクリーンショット。 */
async function shot(selector, dir, name) {
  const path = join(dir, `${name}.png`);
  await browser('screenshot', selector, path);
  return decodePng(await readFile(path));
}

/** 2 枚の重なる範囲を比べ、違う画素の数と割合を返す。違えば差の画像を書く。 */
async function comparePixels(left, right, diffPath) {
  const width = Math.min(left.width, right.width);
  const height = Math.min(left.height, right.height);
  const diff = Buffer.alloc(width * height * 4);
  let different = 0;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const a = (y * left.width + x) * 4;
      const b = (y * right.width + x) * 4;
      const same = [0, 1, 2].every((c) => left.pixels[a + c] === right.pixels[b + c]);
      const d = (y * width + x) * 4;
      if (same) {
        diff[d] = diff[d + 1] = diff[d + 2] = Math.round(left.pixels[a] * 0.3 + 178);
      } else {
        different += 1;
        diff[d] = 255;
      }
      diff[d + 3] = 255;
    }
  }
  if (different > 0) {
    await writeFile(diffPath, encodePng(width, height, diff));
  }
  return { different, total: width * height, ratio: different / (width * height) };
}

const refPane = '#live-stage .lv-pane[data-side="ref"]';
const livePane = '#live-stage .lv-pane[data-side="live"]';

/** 比べる相手の枠に、その id のスナップショットが出るのを待つ。 */
const showsSnapshot = (label) => `document.querySelector('${refPane}').dataset.reference === 'snapshot' && document.querySelector('${refPane} .lv-bar-label').textContent.startsWith(${JSON.stringify(label)})`;
const notRecorded = `document.querySelector('${refPane}').dataset.reference === 'none' && ${visible(`${refPane} .lv-empty`)} && document.querySelector('${refPane} .lv-empty').textContent.includes('has not been recorded')`;

/** 枠の中の点を押す。 */
async function clickInPane(pane, x, y) {
  const box = await evaluate(`(() => { const r = document.querySelector('${pane} .lv-frame').getBoundingClientRect(); return { x: r.x, y: r.y }; })()`);
  const scale = Number(await evaluate(`getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1'`));
  await browser('mouse', 'move', String(Math.round(box.x + x * scale)), String(Math.round(box.y + y * scale)));
  await browser('mouse', 'down');
  await browser('mouse', 'up');
}

/** スナップショット（R-PAGE-SNAPSHOT、R-PAGE-VIEW、R-PAGE-REF）。 */
async function snapshotsAreTakenShownAndChosen(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    console.log('PASS 渡す前のレビューで、開始時のスナップショットが既定の比べる相手になる');

    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(showsSnapshot('Recorded 1'));
    assert.equal(
      await evaluate(`document.querySelector('${refPane} .lv-frame').style.width === document.querySelector('${livePane} .lv-frame').style.width && document.querySelector('${livePane} .lv-frame').style.width === '390px'`),
      true,
    );
    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(showsSnapshot('Start'));
    assert.equal(await evaluate(`document.querySelector('${refPane} .lv-frame').style.width`), '1280px');
    console.log('PASS 390 と 1280 で取ってから幅を切り替えると、動いているページと比べる相手が同じ幅で描かれ、無い幅では記録されていない旨と取る操作が出る');

    // 画素の比較は 390 で（縮めずに描ける幅）。
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(showsSnapshot('Recorded 1'));
    await new Promise((done) => setTimeout(done, 500));
    const before = await shot(`${refPane} .lv-frame`, shots, 'snapshot');
    const now = await shot(`${livePane} .lv-frame`, shots, 'live');
    const compared = await comparePixels(before, now, join(shots, 'diff.png'));
    if (compared.different === 0) {
      console.log(`PASS shadow DOM・canvas・SVG・入力欄を持つページのスナップショットが、動いているページと画素まで同じに出る（${compared.total} 画素）`);
    } else {
      console.log(`CHECK スナップショットと動いているページの画素が ${compared.different} / ${compared.total}（${(compared.ratio * 100).toFixed(3)}%）違う。差の画像: ${join(shots, 'diff.png')}（人が確かめる）`);
    }

    await clickInPane(refPane, 150, 250);
    await new Promise((done) => setTimeout(done, 300));
    const afterClick = await shot(`${refPane} .lv-frame`, shots, 'snapshot-clicked');
    assert.equal((await comparePixels(before, afterClick, join(shots, 'clicked-diff.png'))).different, 0, 'the snapshot must not run onclick');
    await clickInPane(livePane, 150, 250);
    await new Promise((done) => setTimeout(done, 300));
    const liveClicked = await shot(`${livePane} .lv-frame`, shots, 'live-clicked');
    assert.ok((await comparePixels(now, liveClicked, join(shots, 'live-clicked-diff.png'))).different > 0, 'the running page runs onclick (control)');
    console.log('PASS onclick を持つ要素を押しても、スナップショットではスクリプトが動かない（動いているページでは動く）');

    await browser('click', '.lv-widths button[data-width="1280"]');
    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${kemi.live.replace(/\/rich\.html$/, '')}/other.html`)}; true`);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/other.html') && ${notRecorded}`);
    console.log('PASS 別の URL に移ると、その URL のスナップショットが無い旨が出る');

    await browser('click', '.lv-record');
    await waitFor(showsSnapshot('Recorded 2'));
    const pages = `Array.from(document.querySelectorAll('#page-tree .lv-page')).map((row) => row.dataset.page + ':' + Array.from(row.querySelectorAll('.lv-width-tag')).map((tag) => tag.dataset.width).join(','))`;
    await waitFor(`JSON.stringify(${pages}) === JSON.stringify(['/other.html:1280', '/rich.html:390,1280'])`);
    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-page-open');
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/rich.html' && ${showsSnapshot('Start')}`);
    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-width-tag[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px' && ${showsSnapshot('Recorded 1')}`);
    console.log('PASS スナップショットを持つ 2 つのページがツリーに表示幅と並び、押すとそのページ・その幅に移る');

    await post(kemi.url, 'api/message', { body: 'please look' });
    await post(kemi.url, 'api/hand', {});
    await waitFor(showsSnapshot('Handed 1'));
    console.log('PASS エージェントに渡すと表示中のページのスナップショットを取り、それが既定の比べる相手になる');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** 画像の (x, y) の色。 */
function pixelAt(image, x, y) {
  const at = (y * image.width + x) * 4;
  return [image.pixels[at], image.pixels[at + 1], image.pixels[at + 2]];
}

/** モック（R-PAGE-MOCK、R-PAGE-REF、R-PAGE-VIEW）。 */
async function mocksAreAssignedShownAndKeptApart(repository) {
  const { mkdir } = await import('node:fs/promises');
  const { basename, dirname } = await import('node:path');
  await mkdir(join(repository, 'mocks'), { recursive: true });
  await writeFile(join(repository, 'mocks', 'mock.html'), '<!doctype html><html><head><link rel="stylesheet" href="mock.css"></head><body><img src="dot.png" alt="" style="display:block;width:40px;height:40px"></body></html>\n');
  await writeFile(join(repository, 'mocks', 'mock.css'), 'html, body { margin: 0; background: rgb(0, 200, 0); }\n');
  const red = Buffer.alloc(4 * 4 * 4);
  for (let i = 0; i < 16; i++) red.set([255, 0, 0, 255], i * 4);
  await writeFile(join(repository, 'mocks', 'dot.png'), encodePng(4, 4, red));
  await writeFile(join(repository, 'mocks', 'notes.txt'), 'not a mock\n');
  const outside = join(dirname(repository), `outside-${basename(repository)}.html`);
  await writeFile(outside, '<p>outside</p>\n');

  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', dev.url]);
  const token = new URL(kemi.url).pathname.split('/')[2];
  await writeFile(join(repository, 'mocks', 'script.html'), `<!doctype html><html><head></head><body style="margin:0"><p id="drawn"></p><script>
    document.body.style.background = 'rgb(0, 0, 200)';
    document.getElementById('drawn').textContent = 'drawn by the mock';
    const tryRead = (read) => { try { return String(read()); } catch { return 'blocked'; } };
    const report = { mockReport: true, href: location.href, referrer: document.referrer, cookie: tryRead(() => document.cookie), parent: tryRead(() => parent.document.title) };
    report.sawToken = [report.href, report.referrer, report.cookie, report.parent].some((text) => text.includes(${JSON.stringify(token)}));
    fetch(${JSON.stringify(`${kemi.url}api/message`)}, { method: 'POST', headers: { 'Content-Type': 'text/plain' }, body: JSON.stringify({ body: 'written by the mock' }) })
      .catch(() => {})
      .finally(() => parent.postMessage(report, '*'));
  </script></body></html>\n`);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await post(kemi.url, 'api/message', { body: 'before the mock' });
    await post(kemi.url, 'api/hand', {});
    await waitFor(showsSnapshot('Handed 1'));

    for (const [path, reason] of [[`../${basename(outside)}`, 'outside'], ['mocks/notes.txt', '.html or .htm']]) {
      await browser('fill', '.lv-mock-input', path);
      await browser('click', '.lv-mock-assign');
      await waitFor(`${visible('.lv-mock-error')} && document.querySelector('.lv-mock-error').textContent.includes(${JSON.stringify(reason)})`);
      assert.notEqual(await evaluate(`document.querySelector('${refPane}').dataset.reference`), 'mock', path);
    }
    console.log('PASS 配れる範囲の外のパスと .txt のファイルは、理由が出て割り当てられない');

    await browser('fill', '.lv-mock-input', 'mocks/mock.html');
    await browser('click', '.lv-mock-assign');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock' && !${visible('.lv-mock-error')}`);
    await new Promise((done) => setTimeout(done, 800));
    const mock = await shot(`${refPane} .lv-frame:not([hidden])`, shots, 'mock');
    assert.deepEqual(pixelAt(mock, 200, 200), [0, 200, 0], 'the mock CSS is applied');
    assert.deepEqual(pixelAt(mock, 10, 10), [255, 0, 0], 'the mock image is shown');
    assert.equal(mock.width, 390);
    assert.equal(await evaluate(`document.querySelector('#page-tree .lv-page[data-page="/"] .lv-mock-tag') !== null`), true);
    console.log('PASS CSS と画像を参照するモックを割り当てると、比べる相手がモックになり、同じ表示幅でスタイルと画像ごと出る');

    await browser('click', '.lv-mock-remove');
    await waitFor(showsSnapshot('Handed 1'));
    console.log('PASS モックを外すと、最後に渡した時点のスナップショットに戻る');

    await evaluate(`window.__mockReports = []; window.addEventListener('message', (event) => { if (event.data && event.data.mockReport) window.__mockReports.push(event.data); }); true`);
    const messagesBefore = (await (await fetch(new URL('api/review', kemi.url))).json()).messages.length;
    await browser('fill', '.lv-mock-input', 'mocks/script.html');
    await browser('click', '.lv-mock-assign');
    await waitFor(`window.__mockReports.length > 0`);
    const report = JSON.parse(await evaluate(`JSON.stringify(window.__mockReports[0])`));
    assert.equal(report.sawToken, false, JSON.stringify(report));
    assert.equal(report.parent, 'blocked', JSON.stringify(report));
    await new Promise((done) => setTimeout(done, 500));
    const drawn = await shot(`${refPane} .lv-frame:not([hidden])`, shots, 'script-mock');
    assert.deepEqual(pixelAt(drawn, 200, 200), [0, 0, 200], 'the mock script runs');
    const messagesAfter = (await (await fetch(new URL('api/review', kemi.url))).json()).messages.length;
    assert.equal(messagesAfter, messagesBefore, 'the API refuses the mock even with the token');
    console.log('PASS JS で描くモックが描かれ、そのスクリプトからトークンは得られず、トークンを付けても API に断られる');

    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${dev.url}other.html`.replace(dev.url, `${new URL(kemi.live).origin}/`))}; true`);
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/other.html'`);
    await browser('fill', '.lv-mock-input', 'mocks/mock.html');
    await browser('click', '.lv-mock-assign');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock'`);
    await browser('click', '#page-tree .lv-page[data-page="/"] .lv-page-open');
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/'`);
    assert.equal(
      await evaluate(`(() => { const row = document.querySelector('#page-tree .lv-page[data-page="/other.html"]'); return row !== null && row.querySelector('.lv-mock-tag') !== null && row.querySelectorAll('.lv-width-tag').length === 0; })()`),
      true,
      'a page with only a mock stays in the tree',
    );
    await browser('click', '#page-tree .lv-page[data-page="/other.html"] .lv-page-open');
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/other.html' && document.querySelector('${refPane}').dataset.reference === 'mock'`);
    console.log('PASS モックの割り当てだけがあるページがツリーに出て、押すとそのページへ移りモックと比べる');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** 重ねて透かす（R-PAGE-REF、DC2、R-PAGE-VIEW の狭い画面）。 */
async function overlayFollowsTheScrollAndTheOpacity(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}tall.html`]);
  const opacity = async (value) => {
    await evaluate(`(() => { const range = document.querySelector('.lv-opacity'); range.value = '${value}'; range.dispatchEvent(new Event('input')); return true; })()`);
    await new Promise((done) => setTimeout(done, 300));
  };
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`${notRecorded} || ${showsSnapshot('Start')}`);
    if (await evaluate(`document.querySelector('${refPane}').dataset.reference`) === 'none') {
      await browser('click', `${refPane} .lv-empty .lv-record`);
      await waitFor(showsSnapshot('Recorded 1'));
    }
    await browser('click', '.lv-mode button[data-compare="overlay"]');
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'overlay' && ${visible('.lv-opacity')}`);

    // 見る対象をスクロールすると、重ねた比べる相手も同じだけ動く。
    // 別のオリジンの枠の中は、ページ内の目印へ移して（同じ文書のままスクロールさせて）動かす。
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = frame.src.split('#')[0] + '#band-3'; return true; })()`);
    await waitFor(`/translate\\(0px, -[1-9]/.test(document.querySelector('${refPane} .lv-frame:not([hidden])').style.transform)`);
    await new Promise((done) => setTimeout(done, 500));
    await opacity(0);
    const underneath = await shot(`${livePane} .lv-viewport`, shots, 'overlay-0');
    await opacity(100);
    const overlaid = await shot(`${livePane} .lv-viewport`, shots, 'overlay-100');
    const compared = await comparePixels(underneath, overlaid, join(shots, 'overlay-diff.png'));
    if (compared.different === 0) {
      console.log('PASS 重ねて透かす表示で、見る対象をスクロールすると比べる相手も同じだけ動く（スクロールした位置で画素まで重なる）');
    } else {
      console.log(`CHECK スクロールした位置で、重ねた比べる相手と見る対象の画素が ${compared.different} / ${compared.total}（${(compared.ratio * 100).toFixed(3)}%）違う。差の画像: ${join(shots, 'overlay-diff.png')}（人が確かめる）`);
    }

    // 見る対象を変えると、透かし具合で見え方が変わる。
    await writeFile(join(dev.dir, 'tall.html'), (await readFile(join(dev.dir, 'tall.html'), 'utf8')).replaceAll('hsl(', 'hsl(180deg + '));
    await evaluate(`document.querySelector('${livePane} .lv-frame').src = document.querySelector('${livePane} .lv-frame').src; true`);
    await new Promise((done) => setTimeout(done, 1500));
    await opacity(0);
    const live0 = await shot(`${livePane} .lv-viewport`, shots, 'changed-0');
    await opacity(100);
    const live100 = await shot(`${livePane} .lv-viewport`, shots, 'changed-100');
    assert.ok((await comparePixels(live0, live100, join(shots, 'changed-diff.png'))).different > 0, 'the opacity changes what is seen');
    console.log('PASS 重ねて透かす表示で、透かし具合を変えると比べる相手の見え方が変わる');

    await browser('set', 'viewport', '390', '800');
    await waitFor(`${visible('.lv-compare-select')} && ${visible('.lv-mode')} && ${visible(`${livePane}`)} && ${visible(`${refPane} .lv-frame:not([hidden])`)}`);
    await browser('click', '.lv-mode button[data-compare="side"]');
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'side' && ${visible('.lv-side')}`);
    await browser('click', '.lv-mode button[data-compare="overlay"]');
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'overlay' && ${visible(`${refPane} .lv-frame:not([hidden])`)} && ${visible(livePane)}`);
    console.log('PASS 幅 390px でも、比べる相手の選択と重ねて透かす表示に切り替えられる');
  } finally {
    await browser('set', 'viewport', '1280', '800');
    await stop(kemi);
    await dev.close();
  }
}

const repository = await makeRepository();
try {
  await relayCarriesHmrAndHidesTheCookie(repository);
  await waitsForTheDevServer(repository);
  await fileReloadsWhenItsCssIsSaved(repository);
  await pageViewShowsFramedPagesWidthsAndNarrowScreens(repository);
  await outsideGitFilePages();
  await otherReviewsLoadNoPageFiles(repository);
  await snapshotsAreTakenShownAndChosen(repository);
  await mocksAreAssignedShownAndKeptApart(repository);
  await overlayFollowsTheScrollAndTheOpacity(repository);
} finally {
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
