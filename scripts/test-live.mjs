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
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, writeFile } from 'node:fs/promises';
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
    const snapshot = await evaluate(`JSON.stringify({ url: location.href, text: document.body?.innerText.slice(0, 400) })`).catch(() => 'no snapshot');
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
      const url = stderr.match(/^kemi: (http:\/\/\S+)/m);
      const review = stderr.match(/^kemi: review (\S+)/m);
      const live = stderr.match(/^kemi: live (\S+)/m);
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

const repository = await makeRepository();
try {
  await relayCarriesHmrAndHidesTheCookie(repository);
  await waitsForTheDevServer(repository);
  await fileReloadsWhenItsCssIsSaved(repository);
  await pageViewShowsFramedPagesWidthsAndNarrowScreens(repository);
  await outsideGitFilePages();
  await otherReviewsLoadNoPageFiles(repository);
} finally {
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
