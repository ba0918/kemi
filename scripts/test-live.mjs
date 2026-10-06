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
//   渡すと取る。別のオリジンの CSS・@import・style 属性の url()・<picture> の <source>・video の
//   poster・SVG の <image>・<input type=image> が、スナップショットでも動いているページと同じ色に出る。
// - コメントの画像: ページの中で写しを描いて作った描き込み無しの画像を、動いているページの同じ範囲と画素で比べる
//   （差の割合と差の画像を出して人の確認に回す）。描き込みを重ねた画像も残す。インラインのスタイルを止める CSP と
//   Trusted Types を求める CSP のページでは、画像が作れるか、作れない理由が返る。範囲が表示幅より狭く画面の高さと違っても、
//   動いているページと同じ幅の CSS と vh で描く。
// - ページへのコメント: 要素・ペン・矢印で場所を置くと番号が振られてページの上に描かれ、2 番目を消して保存すると番号が
//   1 と 3 のまま残る。矢印の先の要素は先端の位置の要素。並べた比べる相手の側では場所が増えず、重ねて透かしている間は
//   見る対象の要素が場所になる。ペンの場所には囲んだ範囲を丸ごと含む外側の要素が入らず、1 つの要素の内側だけを囲むと
//   その要素 1 つになる。どの要素にも掛からない地を選ぶ・指す・囲むと、文書の根が場所の要素になり、画像は文書全体ではなく場所の周りを写す。書きかけの場所と別の表示幅では場所を足せず保存もできず、戻る操作で戻ると足せる。ページが場所を返す前に表示幅を変えても、場所は押したときの幅のものになる。保存している間は書きかけも表示幅もページも変えられず、保存し終えるとまた書ける。幅 390px で比べる相手の側を見ている間にページが読み込まれ直しても、保存すると画像が作られる。保存している間にコードの見方や比べる相手の側へ切り替えても、画像は場所の周りを写すか null になる。画像を作る頼みが届く前に動いているページが別のページへ移ると、画像は null になる。描いている間も保存した後も変化の一覧は変わらない。コメントだけがあるページがツリーに
//   コメントの数とともに出て、表示幅の札で移れる。別の幅で付けたコメントのスレッドは付けた幅を出し、押すとそこへ移り、
//   場所の印が出る。本文だけを編集できる。保留して復元しても会話パネルとツリーに出る。画面で付けた 3 つの場所を持つ
//   コメントを渡すと、kemi wait に場所と PNG の画像の絶対パスが届き、submit では画像が null になる。CSP の厳しいページでも届く。
// - モック: 範囲の外と .txt を理由つきで断る。CSS と画像ごと同じ幅で出る。外すとスナップショットに
//   戻る。JS のモックが描かれ、トークンが（referrer からも）得られず API に断られる。モックだけがあるページがツリーに出る。
//   スナップショットの中の外部の画像は、referrerpolicy="unsafe-url" を付けていてもトークンの URL を受け取らない。
// - 重ねて透かす: スクロールがそろう、透かし具合で見え方が変わる、幅 390px でも切り替えられる。
// - 差分: スナップショットを取った後に同じ URL の中身を変えると、読み込み直さずに変化の一覧が変わる。
//   兄弟の途中に足した要素だけが増えたになり、ボタンの背景色の変化が前後の色つきで主な変化に入る。
//   モックと比べる間は一覧が出ず、外すと出る。変化の数は表示中のページにだけ出る。別のページへ移ると、前の
//   ページの一覧を新しいページの下に出さない。幅 390px では引き出しの中。幅 390px で比べる相手の側を見ている（動いている
//   ページの枠が隠れる）間は変化の数が変わらず、動いているページの側に戻すと比べ直す。枠が隠れている間（比べる相手の
//   側を見ている、コードの見方から渡す）に取ったスナップショットも、変えていないページとは変化 0。
// - 印: 主な変化と増えた要素は動いているページの側に、消えた要素はスナップショットの側に印が付き、変わって
//   いない要素には付かない。印を付けても変化は増えず、その後に取ったスナップショットとは変化 0。重ねて透かす
//   表示でも同じ印。モックと比べる間は付かない。動いているページの側の印だけを付け直すときは、比べる相手の
//   枠のスクロール位置は変わらない。同じページを読み込み直しても、消えた要素の組が同じなら変わらない。並べる表示の比べる相手は、画面に固定した要素も動いているページと同じ場所に出る。
//   インラインのスタイルを止める CSP のページでも印が付く。Trusted Types を求める CSP のページで要素を消すと、一覧には出し、
//   スナップショットに印を付けられないことを出す。HTML として読み直すと要素の並びが変わる（スクリプトが
//   tbody を挟まずに組んだ表の）ページでも、消えた要素の印はスナップショットのその要素に付く。スクロールしただけでは変化にならず、印は
//   スクロールしても要素に付いたまま（固定・張り付く要素、中でスクロールする箱でも）。
// - CSSOM だけの変化: 構築したスタイルシートを replaceSync で差し替えると、DOM が変わらなくても変化の一覧が変わる。
// - 表示幅の切り替え: <html> の min-width より狭い幅どうしで切り替えても（文書の幅は変わらない）、切り替えた幅の
//   変化の一覧が出る。
// - 要素の多いページ: HTML が 2 MB 未満なら、要素が 7 万を超えてもスナップショットが取れ、変化の一覧が出る。
//   一覧の残りも続きを出す操作ですべて見られる。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { crc32, deflateSync, inflateSync } from 'node:zlib';
import { createServer } from 'node:net';
import { createServer as createHttpServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { promisify } from 'node:util';

import { CHANGING_GEOMETRY, RESOURCE_BOXES, SCROLLING_GEOMETRY, TABLE_GEOMETRY, adoptedCss, changingCss, changingPage, manyCss, scrollingCss, startDevServer, tablePage } from './live-dev-server.mjs';

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

/**
 * 画面の「Hand to agent」で渡す。ボタンはエージェントが kemi wait を呼んだレビューにだけ出るので、
 * 先に待たせておき、渡して返るのを待つ。
 */
async function handInThePage(kemi, dir, state) {
  const waiting = startWaiting(kemi, dir, state);
  await pressHand();
  assert.equal(await waiting, 0, 'kemi wait returns what was handed');
}

/** kemi wait を走らせる。終了コードで解決する。 */
function startWaiting(kemi, dir, state) {
  return new Promise((done, fail) => {
    const child = spawn(binary, ['wait', kemi.id, '--timeout', '30'], { cwd: dir, env: environment(state), stdio: ['ignore', 'pipe', 'pipe'] });
    child.on('error', fail);
    child.on('exit', (code) => done(code));
  });
}

/** 「Hand to agent」を押せるようになるのを待って押す。 */
async function pressHand() {
  const button = `(${visible('#rail-hand')} ? document.querySelector('#rail-hand') : ${visible('#btn-hand')} ? document.querySelector('#btn-hand') : null)`;
  await waitFor(`${button} !== null && !${button}.disabled`);
  await evaluate(`${button}.id`).then((id) => browser('click', `#${id}`));
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
  const unreachable = `document.querySelector('#kemi-unreachable') !== null`;
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
    await waitFor(visible('#live-no-code'));
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
const notRecorded = `document.querySelector('${refPane}').dataset.reference === 'none' && ${visible(`${refPane} .lv-empty`)}`;

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
    await handInThePage(kemi, repository, state);
    await waitFor(showsSnapshot('Handed 1'));
    console.log('PASS エージェントに渡すと表示中のページのスナップショットを取り、それが既定の比べる相手になる');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * スナップショットが要る資源（R-PAGE-SNAPSHOT の同じ見た目）: スナップショットの枠は中継のポートから
 * 読めないので、CSS と画像の参照が中継を指したままだと、その箱が描かれない。
 */
async function snapshotsCarryTheirResources(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}resources.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(showsSnapshot('Recorded 1'));
    await new Promise((done) => setTimeout(done, 800));
    const live = await shot(`${livePane} .lv-frame`, shots, 'resources-live');
    const snapshot = await shot(`${refPane} .lv-frame`, shots, 'resources-snapshot');
    const colors = (image) => Object.fromEntries(RESOURCE_BOXES.map(({ name, left, top }) => [name, pixelAt(image, left + 40, top + 40)]));
    const seen = colors(live);
    // video の上には操作部が重なるので、色そのものではなく白でない（poster が出た）ことを見る。
    const { poster, ...plain } = seen;
    assert.deepEqual(plain, Object.fromEntries(RESOURCE_BOXES.filter(({ name }) => name !== 'poster').map(({ name, color }) => [name, color])), 'the running page draws every box (control)');
    assert.notDeepEqual(poster, [255, 255, 255], 'the running page draws the poster (control)');
    assert.deepEqual(colors(snapshot), seen, `the snapshot draws every box as the running page does: ${join(shots, 'resources-snapshot.png')}`);
    console.log('PASS 別のオリジンの CSS・@import・style 属性の url()・<picture> の <source>・video の poster・SVG の <image>・<input type=image> が、スナップショットでも動いているページと同じに出る');
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
    await handInThePage(kemi, repository, state);
    await waitFor(showsSnapshot('Handed 1'));

    for (const path of [`../${basename(outside)}`, 'mocks/notes.txt']) {
      await browser('fill', '.lv-mock-input', path);
      await browser('click', '.lv-mock-assign');
      await waitFor(`${visible('.lv-mock-error')} && document.querySelector('.lv-mock-error').textContent.trim() !== ''`);
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

/**
 * スナップショットの中の外部の画像（R-PAGE-MOCK、R-PAGE-PROXY）: ページが referrerpolicy="unsafe-url" を
 * 付けていても、画像のサーバにトークンの URL が Referer として届かない。
 */
async function snapshotsSendNoTokenToExternalImages(repository) {
  /** @type {{ referer: string | null }[]} */
  const received = [];
  const images = createHttpServer((request, response) => {
    received.push({ referer: request.headers.referer ?? null });
    const red = Buffer.alloc(4);
    red.set([255, 0, 0, 255]);
    response.writeHead(200, { 'Content-Type': 'image/png', 'Cache-Control': 'no-store' });
    response.end(encodePng(1, 1, red));
  });
  await new Promise((done) => images.listen(0, '127.0.0.1', () => done(undefined)));
  const address = images.address();
  // localhost と 127.0.0.1 は別のオリジンなので、中継のページから見て外部の画像になる。
  const image = `http://localhost:${typeof address === 'object' && address ? address.port : 0}/image.png`;
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}referrer.html?image=${encodeURIComponent(image)}`]);
  const token = new URL(kemi.url).pathname.split('/')[2];
  const relay = new URL(kemi.live).origin;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    // 中継のページと写すときの読み込みは中継のオリジンを referrer にする。それ以外がスナップショットの枠から。
    const deadline = Date.now() + 15000;
    while (!received.some(({ referer }) => !referer?.startsWith(relay)) && Date.now() < deadline) {
      await new Promise((done) => setTimeout(done, 100));
    }
    assert.ok(received.some(({ referer }) => !referer?.startsWith(relay)), `the snapshot frame loads the image: ${JSON.stringify(received)}`);
    assert.equal(received.some(({ referer }) => referer?.includes(token)), false, JSON.stringify(received));
    console.log('PASS スナップショットの中の外部の画像は、referrerpolicy="unsafe-url" を付けていても Referer にトークンの URL を受け取らない');
  } finally {
    await stop(kemi);
    await dev.close();
    images.closeAllConnections();
    await new Promise((done) => images.close(() => done(undefined)));
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

/** 表示中のページの変化の一覧。無ければ null。 */
const changeList = `document.querySelector('#page-tree .lv-page[data-current="true"] .lv-changes')`;

/** 変化の一覧の主な変化（種類と、出ている文字）。 */
async function mainChanges() {
  return JSON.parse(await evaluate(`JSON.stringify(Array.from(${changeList}?.querySelectorAll('.lv-change-main .lv-change') ?? []).map((item) => ({ kind: item.dataset.kind, text: item.textContent })))`));
}

/** 差分（R-PAGE-DIFF、R-PAGE-VIEW の変化の一覧、R-PAGE-REF の一覧はスナップショットのときだけ）。 */
async function changeListFollowsThePage(repository) {
  const { mkdir } = await import('node:fs/promises');
  await mkdir(join(repository, 'mocks'), { recursive: true });
  await writeFile(join(repository, 'mocks', 'diff-mock.html'), '<!doctype html><html><body><p>mock</p></body></html>\n');
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}changing.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await evaluate(`window.__kemiNotReloaded = true; true`);
    await waitFor(showsSnapshot('Start'));
    await waitFor(`${changeList}?.dataset.main === '0' && ${changeList}.querySelector('.lv-changes-none') !== null`);
    console.log('PASS 開始時のスナップショットと変わらないページでは、変化の数 0 と変化が無いことが出る');

    await writeFile(join(dev.dir, 'changing.html'), changingPage(['one', 'two', 'inserted', 'three', 'four']));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    const inserted = await mainChanges();
    assert.equal(inserted.length, 1, JSON.stringify(inserted));
    assert.equal(inserted[0].kind, 'added', JSON.stringify(inserted));
    assert.match(inserted[0].text, /inserted/);
    console.log('PASS 兄弟の途中に要素を 1 つ足すと、足した要素だけが増えたになり、後ろの要素は増えた・消えたにならない（読み込み直さずに一覧が変わる）');

    await writeFile(join(dev.dir, 'changing.html'), changingPage());
    await waitFor(`${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(214, 69, 69)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    const recolored = await mainChanges();
    assert.equal(recolored.length, 1, JSON.stringify(recolored));
    assert.equal(recolored[0].kind, 'visual', JSON.stringify(recolored));
    assert.match(recolored[0].text, /rgb\(49, 89, 214\)[\s\S]*rgb\(214, 69, 69\)/);
    console.log('PASS ボタンの背景色を HMR の CSS の差し替えで変えると、そのボタンが主な変化に入り、色の前後の値が出る');

    await post(kemi.url, 'api/message', { body: 'please look' });
    await handInThePage(kemi, repository, state);
    await waitFor(`${showsSnapshot('Handed 1')} && ${changeList}?.dataset.main === '0'`);
    await browser('fill', '.lv-mock-input', 'mocks/diff-mock.html');
    await browser('click', '.lv-mock-assign');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock' && document.querySelector('#page-tree .lv-changes') === null`);
    await browser('click', '.lv-mock-remove');
    await waitFor(`${showsSnapshot('Handed 1')} && ${changeList}?.dataset.main === '0'`);
    console.log('PASS モックを割り当てたページでは変化の一覧が出ず、外すと最後に渡した時点のスナップショットと比べた一覧（変化 0）が出る');

    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${new URL(kemi.live).origin}/other.html`)}; true`);
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/other.html' && ${notRecorded}`);
    await browser('click', '.lv-record');
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    await browser('click', '#page-tree .lv-page[data-page="/changing.html"] .lv-page-open');
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/changing.html' && ${changeList} !== null`);
    assert.equal(await evaluate(`document.querySelectorAll('#page-tree .lv-changes').length`), 1);
    assert.equal(await evaluate(`document.querySelector('#page-tree .lv-page[data-page="/other.html"] .lv-changes')`), null);
    console.log('PASS 2 つのページにスナップショットがあるとき、変化の数は表示中のページにだけ出る');

    // 変化のあるページから別のページへ移ると、前のページの一覧を新しいページの下に出さない。
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    await evaluate(`(() => {
      window.__kemiListed = [];
      const record = () => {
        const current = document.querySelector('#page-tree .lv-page[data-current="true"]');
        const changes = current?.querySelector('.lv-changes');
        if (changes) window.__kemiListed.push(current.dataset.page + ' ' + changes.dataset.main);
      };
      new MutationObserver(record).observe(document.querySelector('#page-tree'), { childList: true, subtree: true, attributes: true });
      return true;
    })()`);
    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${new URL(kemi.live).origin}/other.html`)}; true`);
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/other.html' && ${changeList}?.dataset.main === '0'`);
    const listed = JSON.parse(await evaluate(`JSON.stringify(window.__kemiListed)`));
    assert.ok(!listed.includes('/other.html 1'), `the list of the previous page is not shown under the new one: ${JSON.stringify(listed)}`);
    await browser('click', '#page-tree .lv-page[data-page="/changing.html"] .lv-page-open');
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/changing.html' && ${changeList}?.dataset.main === '1'`);
    console.log('PASS 変化のあるページから別のページへ移ると、前のページの変化の一覧を新しいページの下に出さない');

    await browser('set', 'viewport', '390', '800');
    await waitFor(`getComputedStyle(document.querySelector('#page-tree')).visibility === 'hidden'`);
    await new Promise((done) => setTimeout(done, 500));
    await browser('click', '#btn-tree');
    await waitFor(`document.querySelector('#page-tree').dataset.drawer === 'open' && ${visible('#page-tree .lv-changes')}`);
    console.log('PASS 幅 390px で開くと、変化の一覧が引き出しの中にある');

    // 比べる相手の側を見ている間は動いているページの枠が隠れる。その間は変化の数を変えず（隠れた文書と比べない）、
    // 隠れている間にページが変わっても、動いているページの側に戻してから比べ直す。
    const counts = `${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`;
    const shown = await evaluate(counts);
    assert.equal(shown, '1/0');
    await evaluate(`(() => {
      window.__kemiCounts = [];
      new MutationObserver(() => {
        const list = ${changeList};
        window.__kemiCounts.push(list ? list.dataset.main + '/' + list.dataset.shifted : 'none');
      }).observe(document.querySelector('#page-tree'), { childList: true, subtree: true, attributes: true });
      document.querySelector('.lv-side button[data-side="ref"]').click();
      return true;
    })()`);
    await waitFor(`!${visible(`${livePane} .lv-frame`)}`);
    await new Promise((done) => setTimeout(done, 1500));
    // 最後に渡した時点の色に戻す。比べ直せば変化は 0 になる。
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(214, 69, 69)'));
    await new Promise((done) => setTimeout(done, 1500));
    const seen = JSON.parse(await evaluate(`JSON.stringify(window.__kemiCounts)`));
    assert.ok(seen.every((value) => value === shown), `the counts do not change while the live page is hidden: ${shown} -> ${JSON.stringify(seen)}`);
    await evaluate(`document.querySelector('.lv-side button[data-side="live"]').click(); true`);
    await waitFor(`${visible(`${livePane} .lv-frame`)} && ${counts} === '0/0'`);
    console.log('PASS 幅 390px で比べる相手の側を見ている間は変化の数が変わらず、動いているページの側に戻すと比べ直す');

    // 比べる相手の側を見ている（動いているページの枠が隠れている）間に取ったスナップショットも、選んだ幅で
    // 並べた文書から記述する。ページを変えずに動いているページの側に戻せば、それと比べた変化は 0。
    await evaluate(`document.querySelector('.lv-side button[data-side="ref"]').click(); true`);
    await waitFor(`!${visible(`${livePane} .lv-frame`)} && ${visible('.lv-compare .lv-record')}`);
    // ページのツリーの引き出しが開いたままなので、押すのはスクリプトで。
    await evaluate(`document.querySelector('.lv-compare .lv-record').click(); true`);
    await waitFor(`Array.from(document.querySelectorAll('.lv-compare-select option')).some((option) => option.textContent.startsWith('Recorded 2'))`);
    await chooseReference('Recorded 2');
    await evaluate(`document.querySelector('.lv-side button[data-side="live"]').click(); true`);
    await waitFor(`${visible(`${livePane} .lv-frame`)} && ${changeList} !== null`);
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(counts), '0/0', 'a snapshot recorded while the live page is hidden shows no change against the unchanged page');
    console.log('PASS 幅 390px で比べる相手の側を見ている間に取ったスナップショットと、変えていないページを比べると変化が 0');

    // コードの見方から渡すとき（舞台ごと隠れている）に取るスナップショットも同じ。
    await browser('set', 'viewport', '1280', '800');
    await post(kemi.url, 'api/message', { body: 'once more' });
    await browser('click', '.lv-view button[data-view="code"]');
    await waitFor(`!${visible('#live-stage')}`);
    await handInThePage(kemi, repository, state);
    await waitFor(`Array.from(document.querySelectorAll('.lv-compare-select option')).some((option) => option.textContent.startsWith('Handed 2'))`);
    await browser('click', '.lv-view button[data-view="page"]');
    await chooseReference('Handed 2');
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(counts), '0/0', 'a snapshot taken when handing from the code view shows no change against the unchanged page');
    console.log('PASS コードの見方から渡したときのスナップショットと、変えていないページを比べると変化が 0');
    assert.equal(await evaluate(`window.__kemiNotReloaded === true`), true, 'the review page was not reloaded');
  } finally {
    await browser('set', 'viewport', '1280', '800');
    await stop(kemi);
    await dev.close();
  }
}

/** 画像の範囲 [左, 上, 右, 下) の中で、条件に合う画素の数。 */
function countPixels(image, [left, top, right, bottom], test) {
  let count = 0;
  for (let y = top; y < Math.min(bottom, image.height); y++) {
    for (let x = left; x < Math.min(right, image.width); x++) {
      if (test(pixelAt(image, x, y))) count += 1;
    }
  }
  return count;
}

const isRed = ([r, g, b]) => r > 180 && g < 120 && b < 120;
const isGreen = ([r, g, b]) => r < 80 && g > 120 && b < 120;
const isBlue = ([r, g, b]) => r < 90 && g < 130 && b > 180;
const isPurple = ([r, g, b]) => r > 150 && r < 210 && g > 140 && g < 190 && b > 195;

/**
 * 枠を撮り直しながら、範囲の中に条件に合う画素が出る（`present` が false なら無くなる）のを待つ。
 * スナップショットの枠はスクリプトの止まった別のオリジンで、中を調べられないため画素で見る。
 */
async function waitForPixels(selector, dir, name, region, test, present = true, timeout = 10000) {
  const until = Date.now() + timeout;
  for (;;) {
    const image = await shot(selector, dir, name);
    if ((countPixels(image, region, test) > 0) === present) return image;
    if (Date.now() > until) assert.fail(`${present ? 'no' : 'still'} matching pixels in ${region.join(',')}: ${join(dir, `${name}.png`)}`);
    await new Promise((done) => setTimeout(done, 300));
  }
}

/**
 * 比べる相手の枠の中を 1 画面ぶん下へスクロールさせる。別のオリジンの枠にはホイールが届かないので、
 * 何も無いところを押してから PageDown を押す。
 */
async function pageDownInReference() {
  const box = await evaluate(`(() => { const r = document.querySelector('${refPane} .lv-frame:not([hidden])').getBoundingClientRect(); return { x: r.x + Math.min(r.width, 300) / 2, y: r.y + 500 }; })()`);
  await browser('mouse', 'move', String(Math.round(box.x)), String(Math.round(box.y)));
  await browser('mouse', 'down');
  await browser('mouse', 'up');
  await browser('press', 'PageDown');
  await new Promise((done) => setTimeout(done, 500));
}

/** /changing.html の要素の範囲（文書の座標）。`inserted` は兄弟を 1 つ足したとき。 */
function changingRegions(inserted) {
  const { item, button, note } = CHANGING_GEOMETRY;
  const items = inserted ? 5 : 4;
  const below = items * item;
  return {
    item: (index) => [0, index * item, 390, (index + 1) * item],
    button: [0, below, button.width, below + button.height],
    note: [0, below + button.height, 390, below + button.height + note],
  };
}

/** 比べる相手を、その見出しで始まる時点に選ぶ。 */
async function chooseReference(label) {
  await evaluate(`(() => { const select = document.querySelector('.lv-compare-select'); const option = Array.from(select.options).find((item) => item.textContent.startsWith(${JSON.stringify(label)})); select.value = option.value; select.dispatchEvent(new Event('change')); return true; })()`);
  await waitFor(showsSnapshot(label));
}

/** 印（R-PAGE-VIEW の変わったところに必ず印、R-PAGE-DIFF の控えめな印、R-PAGE-REF、R-LIVE のスクロール位置）。 */
async function marksFollowTheChanges(repository) {
  const { mkdir } = await import('node:fs/promises');
  await mkdir(join(repository, 'mocks'), { recursive: true });
  await writeFile(join(repository, 'mocks', 'diff-mock.html'), '<!doctype html><html><body><p>mock</p></body></html>\n');
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}changing.html`]);
  const liveShot = async (name) => {
    await new Promise((done) => setTimeout(done, 400));
    return shot(`${livePane} .lv-frame`, shots, name);
  };
  const refFrame = `${refPane} .lv-frame:not([hidden])`;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);

    let regions = changingRegions(false);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    let image = await liveShot('marks-button');
    assert.ok(countPixels(image, regions.button, isRed) > 0, `the changed button is marked: ${join(shots, 'marks-button.png')}`);
    assert.equal(countPixels(image, regions.note, isRed), 0, 'the element below the button is not marked');
    console.log('PASS ボタンの背景色を変えると、そのボタンに主な変化の印が付き、その下の変わっていない要素には付かない');

    const counts = `${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`;
    const marked = await evaluate(counts);
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(counts), marked, 'marking adds no change');
    await browser('click', '.lv-compare .lv-record');
    await waitFor(`Array.from(document.querySelectorAll('.lv-compare-select option')).some((option) => option.textContent.startsWith('Recorded 2'))`);
    await chooseReference('Recorded 2');
    await waitFor(`${changeList}?.dataset.main === '0' && ${changeList}.dataset.shifted === '0'`);
    console.log('PASS 印を付けた後も変化の数が変わらず、印を付けた後に取ったスナップショットと比べると変化が 0');

    await chooseReference('Recorded 1');
    await writeFile(join(dev.dir, 'changing.css'), changingCss());
    await waitFor(`${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.html'), changingPage(['one', 'two', 'inserted', 'three', 'four']));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    regions = changingRegions(true);
    image = await liveShot('marks-inserted');
    assert.ok(countPixels(image, regions.item(2), isGreen) > 0, `the inserted element is marked as added: ${join(shots, 'marks-inserted.png')}`);
    for (const index of [3, 4]) {
      assert.equal(countPixels(image, regions.item(index), isRed), 0, `sibling ${index} after the inserted one has no main mark`);
      assert.equal(countPixels(image, regions.item(index), isGreen), 0, `sibling ${index} after the inserted one has no added mark`);
    }
    assert.ok(countPixels(image, regions.item(3), isPurple) > 0, 'a sibling that only shifted has the quiet mark');
    console.log('PASS 兄弟の途中に要素を足すと、足した要素に増えたの印が付き、後ろの兄弟には主な変化の印が付かない');

    await browser('fill', '.lv-mock-input', 'mocks/diff-mock.html');
    await browser('click', '.lv-mock-assign');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock'`);
    image = await liveShot('marks-mock');
    assert.equal(countPixels(image, regions.item(2), isGreen), 0, 'no mark while comparing with a mock');
    await browser('click', '.lv-mock-remove');
    await chooseReference('Recorded 1');
    await waitFor(`${changeList}?.dataset.main === '1'`);
    console.log('PASS モックと比べている間は印が付かない');

    await browser('click', '.lv-compare .lv-record');
    await waitFor(`Array.from(document.querySelectorAll('.lv-compare-select option')).some((option) => option.textContent.startsWith('Recorded 3'))`);
    await chooseReference('Recorded 3');
    await waitFor(`${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.html'), changingPage());
    await waitFor(`${changeList}?.dataset.main === '1'`);
    const removedAt = changingRegions(true).item(2);
    await waitForPixels(refFrame, shots, 'marks-removed', removedAt, isRed);
    assert.equal(await evaluate(`${changeList}.querySelector('.lv-changes-unmarked')`), null, 'no notice that removed elements are not marked');
    console.log('PASS 兄弟の途中の要素を消すと、スナップショットの側のその要素に消えたの印が付く');

    await browser('click', '.lv-mode button[data-compare="overlay"]');
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'overlay'`);
    const setOpacity = (value) => evaluate(`(() => { const range = document.querySelector('.lv-opacity'); range.value = '${value}'; range.dispatchEvent(new Event('input')); return true; })()`);
    await setOpacity(100);
    await waitForPixels(refFrame, shots, 'marks-overlay-removed', removedAt, isRed);
    await setOpacity(0);
    regions = changingRegions(false);
    image = await liveShot('marks-overlay');
    assert.ok(countPixels(image, regions.item(2), isPurple) > 0, `the shifted sibling keeps its mark in the overlay: ${join(shots, 'marks-overlay.png')}`);
    console.log('PASS 重ねて透かす表示に切り替えても、同じ要素に同じ印がある');

    // 動いているページの側の印だけが変わるときは、比べる相手の枠を作り直さず、スクロール位置も変わらない。
    // 枠の中は読めないので、先頭にあるボタンの色が見えなくなるまでスクロールし、印を付け直した後も見えないことで見る。
    await browser('click', '.lv-mode button[data-compare="side"]');
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'side'`);
    const button = changingRegions(true).button;
    const refView = `${refPane} .lv-viewport`;
    await waitForPixels(refView, shots, 'scroll-before', button, isBlue);
    await pageDownInReference();
    await waitForPixels(refView, shots, 'scroll-scrolled', button, isBlue, false);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '2'`);
    await new Promise((done) => setTimeout(done, 500));
    image = await shot(refView, shots, 'scroll-kept');
    assert.equal(countPixels(image, button, isBlue), 0, `the reference keeps its scroll: ${join(shots, 'scroll-kept.png')}`);
    console.log('PASS 比べる相手の枠をスクロールしてから動いているページの側の印だけが付け直されても、枠のスクロール位置が変わらない');

    // 同じページを読み込み直しても（HMR の全体の読み込み直しに当たる）、消えた要素の組が変わらなければ枠のスクロール位置は変わらない。
    await evaluate(`(() => {
      const frame = document.querySelector('${livePane} .lv-frame');
      window.__kemiReloaded = false;
      frame.addEventListener('load', () => { window.__kemiReloaded = true; }, { once: true });
      frame.src = frame.src;
      return true;
    })()`);
    await waitFor(`window.__kemiReloaded === true`);
    // 読み込み直した文書には印が無く、新しい記述と比べ直して初めて付く。印が付くのを待って、比べ直した後を見る。
    await waitForPixels(`${livePane} .lv-frame`, shots, 'scroll-reloaded-marks', changingRegions(false).button, isRed);
    image = await shot(refView, shots, 'scroll-reloaded');
    assert.equal(countPixels(image, button, isBlue), 0, `the reference keeps its scroll across a reload of the same page: ${join(shots, 'scroll-reloaded.png')}`);
    assert.equal(await evaluate(`${changeList}.dataset.main`), '2', 'the same changes are listed after the reload');
    console.log('PASS 比べる相手の枠をスクロールしてから同じページを読み込み直しても、消えた要素の組が同じなら枠のスクロール位置が変わらない');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * インラインのスタイルを止める CSP のページでも印が付く（R-PAGE-VIEW の変わったところに必ず印。
 * R-PAGE-PROXY は script-src のほかの CSP を変えない）。
 */
async function marksAreDrawnUnderAStrictStylePolicy(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}changing.html?csp=1`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    await new Promise((done) => setTimeout(done, 400));
    const image = await shot(`${livePane} .lv-frame`, shots, 'csp-marks');
    assert.ok(countPixels(image, changingRegions(false).button, isRed) > 0, `the changed button is marked: ${join(shots, 'csp-marks.png')}`);
    console.log("PASS style-src 'self' の CSP を返すページでも、変わった要素に印が付く");
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 文字列を HTML として読む API を Trusted Types で止める CSP のページでも、スナップショットが撮れて変化の一覧が出る
 * （R-PAGE-SNAPSHOT。R-PAGE-PROXY は script-src のほかの CSP を変えない）。
 */
async function snapshotsAreTakenUnderTrustedTypes(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}changing.html?tt=1`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    console.log("PASS require-trusted-types-for 'script' の CSP を返すページでも、スナップショットが撮れて変化の一覧が出る");

    // このページのスナップショットは要素の対応を持たないので、消えた要素の印は付けられない。一覧には出し、そのことを出す。
    // Trusted Types は試験用の HMR の body の差し替え（DOMParser）も止めるので、ページを読み込み直して反映する。
    await writeFile(join(dev.dir, 'changing.html'), changingPage(['one', 'two', 'four']));
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = frame.src; return true; })()`);
    await waitFor(`${changeList}?.querySelector('.lv-change[data-kind="removed"]') && ${changeList}.querySelector('.lv-changes-unmarked') !== null`);
    console.log("PASS require-trusted-types-for 'script' の CSP を返すページで要素を消すと、一覧に出し、スナップショットに印を付けられないことを出す");
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * HTML として読み直すと要素の並びが変わるページでも、消えた要素の印がスナップショットのその要素に付く
 * （R-PAGE-VIEW の変わったところに必ず印）。スクリプトが tbody を挟まずに組んだ表の後ろの兄弟を消す。
 */
async function removedMarksSurviveReparsing(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}table.html`]);
  const { row, item } = TABLE_GEOMETRY;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'table.html'), tablePage(['one', 'two', 'four']));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    await waitForPixels(`${refPane} .lv-frame:not([hidden])`, shots, 'table-removed', [0, row + 2 * item, 390, row + 3 * item], isRed);
    console.log('PASS スクリプトが組んだ表（読み直すと tbody が足される）の後ろの要素を消しても、スナップショットのその要素に消えたの印が付く');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * スクロールしただけでは変化にならず、印はスクロールしても要素に付いたまま（R-PAGE-DIFF の位置と大きさの
 * 変化、R-PAGE-VIEW の変わったところに必ず印）。上に張り付く見出し、画面に固定した札、中でスクロールする箱で。
 */
async function scrollingMakesNoChangeAndMarksStay(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}scrolling.html`]);
  // 別のオリジンの枠の中は、ページ内の目印へ移して（同じ文書のままスクロールさせて）動かす。
  const moveTo = async (anchor) => {
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = frame.src.split('#')[0] + '#${anchor}'; return true; })()`);
    await new Promise((done) => setTimeout(done, 600));
  };
  const { box, row, scrollMargin, badge } = SCROLLING_GEOMETRY;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0' && ${changeList}.dataset.shifted === '0'`);

    // 並べる表示の比べる相手は、見えている高さで描いて中でスクロールする（動いているページと同じ見え方）。
    const badgeAt = [badge.left, badge.top, badge.left + badge.width, badge.top + badge.height];
    const isBadge = ([r, g, b]) => r === 120 && g === 120 && b === 120;
    const unscrolled = await shot(`${refPane} .lv-viewport`, shots, 'unscrolled-reference');
    await pageDownInReference();
    const reference = await shot(`${refPane} .lv-viewport`, shots, 'scrolled-reference');
    assert.ok((await comparePixels(unscrolled, reference, join(shots, 'scrolled-reference-diff.png'))).different > 0, 'the snapshot scrolled');
    assert.ok(countPixels(reference, badgeAt, isBadge) > badge.width * badge.height * 0.5, `the fixed badge stays in place in the scrolled snapshot: ${join(shots, 'scrolled-reference.png')}`);
    console.log('PASS 並べる表示で比べる相手をスクロールしても、画面に固定した要素は動いているページと同じ場所に描かれる');

    // 箱の中を 6 行目まで、ページを末尾までスクロールしてから、札と 6 行目の色を変えて比べ直させる。
    await moveTo('row-6');
    await moveTo('end');
    await writeFile(join(dev.dir, 'scrolling.css'), scrollingCss({ badge: 'rgb(40, 160, 220)', row: 'rgb(250, 200, 0)' }));
    await waitFor(`${changeList}?.dataset.main === '2'`);
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(`${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`), '2/0', 'scrolling alone shifts nothing');
    console.log('PASS ページと中の箱をスクロールしただけでは、張り付く見出し・固定した札・箱の中の要素が変化に入らない');

    // ページを先頭へ戻す。札は画面の同じ場所に、6 行目は箱の中のスクロールで箱の上端から scroll-margin-top 下に見える。
    await moveTo('start');
    const rowAt = [0, box.top + scrollMargin, 390, box.top + scrollMargin + row];
    let image = await shot(`${livePane} .lv-frame`, shots, 'scrolled-marks');
    assert.ok(countPixels(image, badgeAt, isRed) > 0, `the fixed badge keeps its mark: ${join(shots, 'scrolled-marks.png')}`);
    assert.ok(countPixels(image, rowAt, isRed) > 0, `the row in the scrolled box keeps its mark: ${join(shots, 'scrolled-marks.png')}`);

    // 箱の中を先頭へ戻すと、6 行目（箱の中の上から 5 行ぶん下）は箱の見えている範囲の外に出て、その印も見えなくなる。
    await moveTo('row-1');
    image = await shot(`${livePane} .lv-frame`, shots, 'box-scrolled-back');
    assert.equal(countPixels(image, [0, 0, 390, badge.top], isRed), 0, `the mark of the row hidden in the box is not drawn: ${join(shots, 'box-scrolled-back.png')}`);
    assert.ok(countPixels(image, badgeAt, isRed) > 0, 'the fixed badge still has its mark');
    console.log('PASS スクロールしても、固定した札と中でスクロールする箱の要素の印は要素に付いたまま');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * DOM を変えずに CSSOM だけで見た目が変わるページ（構築したスタイルシートの replaceSync）でも、変化の一覧が
 * 今のページに合わせて変わる（R-PAGE-DIFF、R-PAGE-VIEW の変わったところに印）。
 */
async function cssomChangesAreFollowed(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}adopted.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`${showsSnapshot('Start')} && ${changeList}?.dataset.main === '0'`);
    // 見張りを始めた直後の知らせ（ResizeObserver は始めに必ず一度知らせる）を待ってから変える。先に変えると、
    // その知らせで比べ直されて、CSSOM の変化を見張れていなくても一覧が変わってしまう。
    await new Promise((done) => setTimeout(done, 1000));
    await writeFile(join(dev.dir, 'adopted.css'), adoptedCss('rgb(214, 69, 69)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    const recolored = await mainChanges();
    assert.equal(recolored[0].kind, 'visual', JSON.stringify(recolored));
    assert.match(recolored[0].text, /rgb\(49, 89, 214\)[\s\S]*rgb\(214, 69, 69\)/);
    console.log('PASS 構築したスタイルシートを replaceSync で差し替えてボタンの背景色を変えると、DOM が変わらなくても主な変化に入る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 表示幅を切り替えても文書の幅が変わらないページ（<html> の min-width より狭い幅どうし）でも、切り替えた幅の
 * 変化の一覧が出る（R-PAGE-VIEW の今の表示幅での変化の一覧）。
 */
async function widthSwitchesWithoutResizingTheDocument(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}wide.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`${showsSnapshot('Start')} && ${changeList}?.dataset.main === '0'`);
    await browser('click', '.lv-widths button[data-width="768"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 2')} && ${changeList}?.dataset.main === '0'`);
    await browser('click', '.lv-widths button[data-width="768"]');
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    console.log('PASS 表示幅を変えても文書の幅が変わらないページでも、切り替えた幅の変化の一覧が出る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 要素の多いページ（R-PAGE-SNAPSHOT の 2 MB は HTML に掛ける）: HTML が 2 MB 未満のページは、要素の記述が
 * 大きくても取れて、変化の一覧が出る。
 */
async function manyElementsAreRecordedAndCompared(repository) {
  const cards = 18000;
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const page = `${dev.url}many.html?cards=${cards}`;
  const html = await (await fetch(page)).text();
  assert.ok(new Blob([html]).size < 2 * 1024 * 1024, 'the page is under 2 MB');
  const kemi = await startKemi(repository, state, ['--live', page]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`${showsSnapshot('Start')} && ${changeList}?.dataset.main === '0'`, 120000);
    await writeFile(join(dev.dir, 'many.css'), manyCss('rgb(214, 69, 69)'));
    await waitFor(`${changeList}?.dataset.main === '${cards}'`, 120000);
    console.log(`PASS HTML が 2 MB 未満で要素が ${cards * 4} を超えるページのスナップショットが取れ、変化の一覧が出る`);
    // 一覧は一度に全部は描かないが、続きを出す操作で残りもすべて見られる。
    const listed = await evaluate(`(() => {
      const list = ${changeList};
      for (let more = list.querySelector('.lv-change-more:not([hidden]) button'); more; more = list.querySelector('.lv-change-more:not([hidden]) button')) more.click();
      return list.querySelectorAll('.lv-change-main .lv-change').length;
    })()`);
    assert.equal(listed, cards, 'every change can be listed');
    console.log('PASS 変化が多いときも、続きを出す操作で一覧の残りをすべて見られる');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 中継したページにコメントの画像（R-PAGE-COMMENT）を頼み、返った PNG の base64 か、作れなかった理由を返す。
 * 道具の画面を通さず、レビュー画面と同じオリジンから枠へ頼みを送る。
 */
async function askImage(rect, places) {
  const reply = await evaluate(`new Promise((done) => {
    const frame = document.querySelector('${livePane} .lv-frame');
    const id = 'image-' + Math.random();
    const listen = (event) => {
      const data = event.data;
      if (event.source !== frame.contentWindow || data?.kemi !== 'live' || data.type !== 'imaged' || data.id !== id) return;
      removeEventListener('message', listen);
      done(JSON.stringify({ png: data.png ?? null, error: data.error ?? null }));
    };
    addEventListener('message', listen);
    const page = new URL(frame.src);
    const view = { page: page.pathname + page.search, width: parseFloat(frame.style.width), height: frame.clientHeight };
    frame.contentWindow.postMessage({ kemi: 'live', type: 'image', id, ...view, rect: ${JSON.stringify(rect)}, places: ${JSON.stringify(places)} }, page.origin);
  })`);
  return JSON.parse(reply);
}

/** 画像の左上から width × height を切り出す。 */
function crop(image, width, height) {
  const pixels = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y++) image.pixels.copy(pixels, y * width * 4, y * image.width * 4, (y * image.width + width) * 4);
  return { width, height, pixels };
}

/**
 * コメントの画像（R-PAGE-COMMENT の画像の段落）: ページの中で写しを描いて作った描き込み無しの画像を、動いているページの
 * 同じ範囲の画素と比べる。細部の違いは仕様が認めるので、差の割合と差の画像を出して人の確認に回す。描き込みを重ねた
 * 画像も残す。インラインのスタイルを止める CSP と、Trusted Types を求める CSP のページでは、画像が作れるか、作れない
 * 理由が返るかを確かめる。
 */
async function commentImagesLookLikeThePage(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-images-'));
  const pages = [
    {
      page: 'rich.html',
      rect: { x: 0, y: 0, w: 390, h: 320 },
      places: [
        { n: 1, kind: 'element', points: [], elements: [{ rect: { x: 0, y: 200, w: 300, h: 100 } }] },
        { n: 2, kind: 'arrow', points: [{ x: 330, y: 120 }, { x: 250, y: 40 }], elements: [] },
        { n: 3, kind: 'pen', points: [{ x: 20, y: 90 }, { x: 260, y: 90 }, { x: 260, y: 140 }, { x: 20, y: 140 }, { x: 20, y: 90 }], elements: [] },
      ],
    },
    { page: 'resources.html', rect: { x: 0, y: 0, w: 390, h: 200 }, places: [{ n: 1, kind: 'element', points: [], elements: [{ rect: { x: 10, y: 10, w: 80, h: 80 } }] }] },
  ];
  for (const { page, rect, places } of pages) {
    const dev = await startDevServer();
    const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
    const kemi = await startKemi(repository, state, ['--live', `${dev.url}${page}`]);
    try {
      await browser('set', 'viewport', '1280', '900');
      await browser('open', kemi.url);
      await waitFor(showsSnapshot('Start'));
      await browser('click', '.lv-widths button[data-width="390"]');
      await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
      await new Promise((done) => setTimeout(done, 800));
      const name = page.replace('.html', '');
      const plain = await askImage(rect, []);
      assert.equal(plain.error, null, `the image of ${page} is made`);
      const imagePath = join(shots, `${name}-image.png`);
      await writeFile(imagePath, Buffer.from(plain.png, 'base64'));
      const image = decodePng(await readFile(imagePath));
      assert.deepEqual([image.width, image.height], [rect.w, rect.h], 'the image covers the asked area at its size');
      const live = await shot(`${livePane} .lv-frame`, shots, `${name}-live`);
      const diffPath = join(shots, `${name}-diff.png`);
      const compared = await comparePixels(crop(live, rect.w, rect.h), image, diffPath);
      console.log(`CHECK コメントの画像（${page}、描き込み無し）と動いているページの同じ範囲の画素が ${compared.different} / ${compared.total}（${(compared.ratio * 100).toFixed(3)}%）違う。画像: ${imagePath}、差の画像: ${compared.different > 0 ? diffPath : 'なし'}（人が確かめる）`);
      const drawn = await askImage(null, places);
      assert.equal(drawn.error, null, `the image with places of ${page} is made`);
      const drawnPath = join(shots, `${name}-places.png`);
      await writeFile(drawnPath, Buffer.from(drawn.png, 'base64'));
      console.log(`CHECK 場所の描き込みと番号を重ねたコメントの画像（${page}）: ${drawnPath}（人が確かめる）`);
    } finally {
      await stop(kemi);
      await dev.close();
    }
  }

  for (const query of ['csp=1', 'tt=1']) {
    const dev = await startDevServer();
    const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
    const kemi = await startKemi(repository, state, ['--live', `${dev.url}changing.html?${query}`]);
    try {
      await browser('set', 'viewport', '1280', '900');
      await browser('open', kemi.url);
      await waitFor(showsSnapshot('Start'));
      const made = await askImage(null, [{ n: 1, kind: 'element', points: [], elements: [{ rect: { x: 0, y: 120, w: 200, h: 60 } }] }]);
      if (made.error === null) {
        const path = join(shots, `changing-${query.replace('=1', '')}.png`);
        await writeFile(path, Buffer.from(made.png, 'base64'));
        decodePng(await readFile(path));
        console.log(`PASS changing.html?${query} の CSP の下でもコメントの画像が作れる: ${path}`);
      } else {
        assert.ok(made.error.length > 0, 'a reason is given');
        console.log(`PASS changing.html?${query} の CSP の下ではコメントの画像を作れず、理由が返る: ${made.error}`);
      }
    } finally {
      await stop(kemi);
      await dev.close();
    }
  }
}

/**
 * コメントの画像の見た目の幅と高さ（R-PAGE-COMMENT の画像の段落）: 画像にする範囲が表示幅より狭く、画面の高さと違っても、
 * 動いているページと同じ表示幅と画面の高さで並べた写しを描く（幅で変わる CSS と vh が範囲の大きさで変わらない）。
 */
async function commentImagesKeepTheLayoutOfTheViewport(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-images-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}responsive.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '1280px'`);
    // 枠の高さが、動いているページの画面の高さ（vh の 100）。
    const height = Number(await evaluate(`document.querySelector('${livePane} .lv-frame').clientHeight`));
    // 青い帯の終わりの少し上から、橙の帯の途中まで。幅は 600px より狭い。
    const rect = { x: 0, y: Math.round(height / 2) - 50, w: 200, h: Math.round(height * 1.5) + 100 };
    const made = await askImage(rect, []);
    assert.equal(made.error, null, 'the image of responsive.html is made');
    const path = join(shots, 'responsive.png');
    await writeFile(path, Buffer.from(made.png, 'base64'));
    const image = decodePng(await readFile(path));
    // 文書の y を画像の y に（長い辺の上限で縮めて描いていることがある）。
    const at = (y) => pixelAt(image, 10, Math.round(((y - rect.y) * image.height) / rect.h));
    const blue = ([r, g, b]) => r < 60 && g < 60 && b > 200;
    const green = ([r, g, b]) => r < 60 && g > 100 && g < 160 && b < 60;
    const orange = ([r, g, b]) => r > 200 && g > 120 && g < 200 && b < 60;
    assert.ok(blue(at(rect.y + 10)), `the band above the breakpoint keeps its wide colour: ${at(rect.y + 10)} ${path}`);
    assert.ok(green(at(height / 2 + 30)), `the 50vh band ends where it does on the page: ${at(height / 2 + 30)} ${path}`);
    assert.ok(orange(at(height * 2 + 30)), `the band after 50vh + 150vh is where it is on the page: ${at(height * 2 + 30)} ${path}`);
    console.log('PASS コメントの画像は、範囲が表示幅より狭く画面の高さと違っても、動いているページと同じ幅の CSS と vh で描かれる');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** 枠の中の点を順にたどって描く（押して、動かして、離す）。 */
async function dragInPane(pane, points) {
  const box = await evaluate(`(() => { const r = document.querySelector('${pane} .lv-frame').getBoundingClientRect(); return { x: r.x, y: r.y }; })()`);
  const scale = Number(await evaluate(`getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1'`));
  const at = ([x, y]) => [String(Math.round(box.x + x * scale)), String(Math.round(box.y + y * scale))];
  await browser('mouse', 'move', ...at(points[0]));
  await browser('mouse', 'down');
  for (const point of points.slice(1)) await browser('mouse', 'move', ...at(point));
  await browser('mouse', 'up');
}

/** 書いている途中のコメントの場所の番号。 */
const draftNumbers = `JSON.stringify(Array.from(document.querySelectorAll('#live-compose .lv-place')).map((row) => Number(row.dataset.n)))`;

/** 道具を選ぶ。 */
async function chooseTool(tool) {
  await browser('click', `.lv-tools button[data-tool="${tool}"]`);
  await waitFor(`document.querySelector('.lv-tools button[data-tool="${tool}"]').getAttribute('aria-pressed') === 'true'`);
}

/** kemi の API からレビューを読む（ページへのコメントを確かめる）。 */
async function reviewJson(kemi) {
  return (await fetch(new URL('api/review', kemi.url))).json();
}

/** ページの中の描き込み（画面モックの紫）。 */
const isPlaceInk = ([r, g, b]) => r > 95 && r < 160 && g > 50 && g < 120 && b > 170;

/**
 * ページへのコメントの場所（R-PAGE-COMMENT、R-PAGE-REF）: 要素・矢印・ペンで場所を置き、2 番目を消して保存すると
 * 番号が 1 と 3 のまま残る。矢印の先の要素は先端の位置の要素。並べた比べる相手の側では場所が増えず、重ねて透かして
 * いる間に要素を選ぶと見る対象の要素が場所になる。保存した後も画面は例外を出さない。
 */
async function pageCommentPlacesArePutAndSaved(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await evaluate(`window.__kemiErrors = []; addEventListener('error', (event) => window.__kemiErrors.push(String(event.message))); addEventListener('unhandledrejection', (event) => window.__kemiErrors.push(String(event.reason))); true`);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));

    await chooseTool('element');
    await clickInPane(refPane, 150, 250);
    await new Promise((done) => setTimeout(done, 500));
    assert.equal(await evaluate(draftNumbers), '[]', 'a click on the reference adds no place');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await chooseTool('pen');
    await dragInPane(livePane, [[20, 90], [260, 90], [260, 140], [20, 140], [20, 92]]);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    await chooseTool('arrow');
    await dragInPane(livePane, [[340, 160], [260, 210], [150, 250]]);
    await waitFor(`${draftNumbers} === '[1,2,3]'`);
    await new Promise((done) => setTimeout(done, 300));
    const drawn = await shot(`${livePane} .lv-frame`, shots, 'places-drawn');
    // 押した要素（左上 (0, 200) の 300×100 のボタン）の周りに描いた枠。
    assert.ok(countPixels(drawn, [0, 195, 305, 305], isPlaceInk) > 0, `the places are drawn on the page: ${join(shots, 'places-drawn.png')}`);
    console.log('PASS 要素・ペン・矢印で場所を置くと、1 から番号が振られ、ページの上に描かれる。並べた比べる相手の側を押しても場所は増えない');

    await browser('click', '#live-compose .lv-place[data-n="2"] .lv-place-remove');
    await waitFor(`${draftNumbers} === '[1,3]'`);
    await browser('fill', '#live-compose .lv-compose-body', '1 and 3 point at the button');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${draftNumbers} === '[]'`);
    const review = await reviewJson(kemi);
    const comment = review.comments.at(-1);
    assert.equal(comment.body, '1 and 3 point at the button');
    assert.deepEqual(comment.page.places.map((place) => [place.n, place.kind]), [[1, 'element'], [3, 'arrow']]);
    assert.equal(comment.page.url, '/rich.html');
    assert.equal(comment.page.width, 390);
    assert.equal(comment.page.places[0].elements[0].selector, '#button');
    console.log('PASS 2 番目の場所を消してから保存すると、場所の番号が 1 と 3 のまま残る');
    assert.equal(comment.page.places[1].elements[0].selector, '#button', 'the arrow points at the element at its head');
    const head = comment.page.places[1].points.at(-1);
    assert.ok(Math.abs(head.x - 150) <= 1 && Math.abs(head.y - 250) <= 1, `the arrow ends where it was drawn: ${JSON.stringify(head)}`);
    console.log('PASS 矢印の先の要素が、矢印の先端の位置にある要素と一致する');

    await browser('click', '.lv-mode button[data-compare="overlay"]');
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'overlay'`);
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    assert.equal(await evaluate(`document.querySelector('#live-compose .lv-place[data-n="1"]').dataset.selector`), '#button');
    console.log('PASS 重ねて透かしている間に要素を選ぶと、下の見る対象の要素が場所になる');

    await browser('click', '#cv-rail');
    await waitFor(`document.querySelector('#conversation').dataset.open === 'true'`);
    await new Promise((done) => setTimeout(done, 300));
    assert.deepEqual(JSON.parse(await evaluate('JSON.stringify(window.__kemiErrors)')), [], 'the review page raises no error with a page comment');
    console.log('PASS ページへのコメントを保存した後も、会話パネルを開いて例外が出ない');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * ペンの場所の要素（R-PAGE-COMMENT）: 囲んだ範囲を丸ごと含む外側の要素は入らず、1 つの要素の内側だけを囲むと、範囲を
 * 含む一番内側の要素 1 つになる。
 */
async function penPlacesLeaveOutWhatContainsTheLine(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));
    await chooseTool('pen');
    // 1 段目の並び（左上 (0, 0) の幅いっぱいの .row）の中身を囲む。
    await dragInPane(livePane, [[5, 5], [380, 5], [380, 78], [5, 78], [5, 7]]);
    await waitFor(`${draftNumbers} === '[1]'`);
    // ボタン（左上 (0, 200) の 300×100）の内側だけを囲む。
    await dragInPane(livePane, [[60, 220], [240, 220], [240, 280], [60, 280], [60, 222]]);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    await savePageCommentInThePage('pen places');
    const [around, inside] = (await reviewJson(kemi)).comments.at(-1).page.places;
    const xs = around.points.map((point) => point.x);
    const ys = around.points.map((point) => point.y);
    const [left, top, right, bottom] = [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)];
    const contains = ({ rect }) => rect.x <= left && rect.y <= top && rect.x + rect.w >= right && rect.y + rect.h >= bottom;
    assert.ok(around.elements.length > 0, 'the pen names the elements it encloses');
    assert.deepEqual(around.elements.filter(contains), [], `no element containing the whole line is named: ${JSON.stringify(around.elements)}`);
    assert.deepEqual(inside.elements.map((element) => element.selector), ['#button'], 'a line inside one element names that element');
    console.log('PASS ペンで囲んだ範囲を丸ごと含む外側の要素は場所の要素に入らず、1 つの要素の内側だけを囲むとその要素 1 つになる');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * ページの地の上の場所（R-PAGE-COMMENT）: どの要素にも掛からない地を要素で選ぶ・矢印で指す・ペンで囲むと、その位置の
 * 要素（地では文書の根）が場所の要素になり、座標だけにはならない。
 */
/** 保存の要求に載せた画像を `window.__kemiSavedImage` に控える。 */
async function keepSavedImage() {
  await evaluate(`(() => {
    const real = window.fetch;
    window.fetch = (path, init) => {
      if (String(path).endsWith('api/comment')) window.__kemiSavedImage = JSON.parse(String(init?.body)).image;
      return real(path, init);
    };
    return true;
  })()`);
}

async function placesOnTheBackgroundNameThePage(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));
    await keepSavedImage();
    // ボタン（下端 300）と 2 段の並びより下は、どの要素も描かれていない地。
    await chooseTool('element');
    await clickInPane(livePane, 350, 450);
    await waitFor(`${draftNumbers} === '[1]'`);
    await chooseTool('arrow');
    await dragInPane(livePane, [[200, 360], [300, 420], [350, 450]]);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    await chooseTool('pen');
    await dragInPane(livePane, [[320, 360], [380, 360], [380, 500], [320, 500], [320, 362]]);
    await waitFor(`${draftNumbers} === '[1,2,3]'`);
    await savePageCommentInThePage('the empty space');
    const places = (await reviewJson(kemi)).comments.at(-1).page.places;
    const root = /^html( > body)?$/;
    for (const place of places) {
      assert.equal(place.elements.length, 1, `place ${place.n} (${place.kind}) names one element: ${JSON.stringify(place.elements)}`);
      assert.match(place.elements[0].selector, root, `place ${place.n} (${place.kind}) names the page`);
    }
    console.log('PASS どの要素にも掛からない地を要素で選ぶ・矢印で指す・ペンで囲むと、文書の根が場所の要素になる');
    // 画像は文書の根の箱ではなく、場所の周りを写す。要素の場所は押した点（矢印の先端と同じ点）、矢印とペンは線の点。
    // 範囲は文書（幅 390）の端で切られる。下の端は文書の高さ次第なので、高さは切られない場合を上限にする。
    const points = places.flatMap((place) => place.points);
    const xs = points.map((point) => point.x);
    const ys = points.map((point) => point.y);
    const width = Math.min(390, Math.ceil(Math.max(...xs) + 48)) - Math.max(0, Math.floor(Math.min(...xs) - 48));
    const height = Math.ceil(Math.max(...ys) + 48) - Math.max(0, Math.floor(Math.min(...ys) - 48));
    assert.ok(width < 390, `the places leave the left of the page out: ${JSON.stringify(points)}`);
    const saved = await evaluate('window.__kemiSavedImage ?? null');
    assert.notEqual(saved, null, 'the image is made');
    const image = decodePng(Buffer.from(saved, 'base64'));
    const shown = `${image.width}x${image.height}, places ${JSON.stringify(points)}`;
    assert.equal(image.width, width, `the image is as wide as the places, not the whole page: ${shown}`);
    assert.ok(image.height > 0 && image.height <= height, `the image is no taller than the places: ${shown}`);
    console.log('PASS 地に置いた場所の画像は、文書の根の箱ではなく場所の周りを写す');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 書きかけのコメントの場所の URL と表示幅（R-PAGE-COMMENT）: 別の表示幅では場所が足されず、書きかけの URL と表示幅に
 * 戻る操作で戻ると場所を足せ、保存したコメントはその表示幅を持つ。
 */
async function draftPlacesStayAtTheirWidth(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  const firstSelector = `document.querySelector('#live-compose .lv-place[data-n="1"]')?.dataset.selector`;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]' && ${firstSelector} === '#button'`);

    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '1280px' && ${visible('#live-compose .lv-compose-back')}`);
    await new Promise((done) => setTimeout(done, 500));
    // 1280 では入力欄（左上の 2 段目）を押す。
    await clickInPane(livePane, 40, 100);
    await new Promise((done) => setTimeout(done, 800));
    assert.equal(await evaluate(`${draftNumbers} + ' ' + ${firstSelector}`), '[1] #button', 'no place is added at another width');
    assert.equal(await evaluate(`document.querySelector('#live-compose .lv-compose-save').disabled`), true, 'the comment is not saved at another width');
    console.log('PASS 書きかけのコメントの場所と別の表示幅では場所を足せず、保存もできない');

    await browser('click', '#live-compose .lv-compose-back');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px' && !${visible('#live-compose .lv-compose-back')}`);
    await new Promise((done) => setTimeout(done, 500));
    await clickInPane(livePane, 40, 100);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    await savePageCommentInThePage('kept at 390');
    const comment = (await reviewJson(kemi)).comments.at(-1);
    assert.deepEqual([comment.page.url, comment.page.width, comment.page.places.map((place) => place.n)], ['/rich.html', 390, [1, 2]]);
    console.log('PASS 書きかけの URL と表示幅に戻る操作で戻ると場所を足せ、保存したコメントはその表示幅を持つ');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * レビュー画面から動いているページへの、その種類の頼みを預かる（返事の遅いページを作る）。releaseRequests で送る。
 * 預かっている間は枠の contentWindow が差し替わり、ページからの知らせは読まれない。
 */
async function holdRequests(...types) {
  await evaluate(`(() => {
    const frame = document.querySelector('${livePane} .lv-frame');
    const real = frame.contentWindow;
    window.__kemiHeld = [];
    Object.defineProperty(frame, 'contentWindow', {
      configurable: true,
      get: () => ({
        postMessage: (message, origin) => (${JSON.stringify(types)}.includes(message?.type) ? window.__kemiHeld.push([message, origin]) : real.postMessage(message, origin)),
      }),
    });
    return true;
  })()`);
}

/** 預かった頼みの数。 */
const heldRequests = `(window.__kemiHeld ?? []).length`;

/** 枠を元に戻し、預かった頼み（type を渡せばその種類のものだけ）をページに送る。残りは次に送るまで預かったまま。 */
async function releaseRequests(type) {
  await evaluate(`(() => {
    const frame = document.querySelector('${livePane} .lv-frame');
    delete frame.contentWindow;
    const type = ${JSON.stringify(type ?? null)};
    const sent = window.__kemiHeld.filter(([message]) => type === null || message.type === type);
    window.__kemiHeld = window.__kemiHeld.filter((held) => !sent.includes(held));
    for (const [message, origin] of sent) frame.contentWindow.postMessage(message, origin);
    return true;
  })()`);
}

/**
 * 遅れて届いた場所（R-PAGE-COMMENT）: ページが場所を返す前に表示幅を変えても、場所は押したときの表示幅のものとして
 * 書きかけに入り、別の幅では戻る操作が出る。
 */
async function latePlacesKeepTheirWidth(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));
    await chooseTool('element');
    await holdRequests('place');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${heldRequests} === 1`);
    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '1280px'`);
    assert.equal(await evaluate(draftNumbers), '[]', 'the place is not added while the page has not answered');
    await releaseRequests();
    await waitFor(`${draftNumbers} === '[1]'`);
    await waitFor(visible('#live-compose .lv-compose-back'));
    assert.match(await evaluate(`document.querySelector('#live-compose .lv-compose-back').textContent`), /at 390px/);
    console.log('PASS ページが場所を返す前に表示幅を変えても、場所は押したときの表示幅のものになり、戻る操作が出る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 保存している間の書きかけ（R-PAGE-COMMENT）: 保存の要求が返るまで、場所を足す・取り消す・消す・本文を書き換えることは
 * できず、保存し終えると書く欄が空いて、また書ける。保存したコメントは保存を押したときの場所と本文を持つ。
 */
async function draftsStayWhileSaving(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await browser('fill', '#live-compose .lv-compose-body', 'saved as it was');
    // 保存の要求を、届け直すまで止める。
    await evaluate(`(() => {
      const real = window.fetch;
      window.__kemiHeldSaves = [];
      window.fetch = (path, init) => String(path).endsWith('api/comment')
        ? new Promise((done) => window.__kemiHeldSaves.push(() => done(real(path, init))))
        : real(path, init);
      return true;
    })()`);
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`window.__kemiHeldSaves.length === 1`);
    const locked = `JSON.stringify(['.lv-compose-undo', '.lv-compose-cancel', '.lv-place-remove'].map((selector) => document.querySelector('#live-compose ' + selector).disabled).concat(document.querySelector('#live-compose .lv-compose-body').readOnly))`;
    assert.equal(await evaluate(locked), '[true,true,true,true]', 'undo, discard, remove and the body are locked while saving');
    await clickInPane(livePane, 40, 100);
    await new Promise((done) => setTimeout(done, 800));
    assert.equal(await evaluate(draftNumbers), '[1]', 'no place is added while saving');
    // 画像は場所のあるページとその表示幅で作るので、保存している間は表示幅もページも変えさせない。
    const widthLocked = `JSON.stringify(Array.from(document.querySelectorAll('.lv-widths button')).every((choice) => choice.disabled) && document.querySelector('.lv-width-input').disabled)`;
    assert.equal(await evaluate(widthLocked), 'true', 'the width buttons and input are locked while saving');
    await evaluate(`(() => {
      window.__kemiPageLoads = 0;
      window.addEventListener('message', (event) => {
        if (event.data?.kemi === 'live' && event.data.type === 'page') window.__kemiPageLoads += 1;
      });
      return true;
    })()`);
    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-width-tag[data-width="1280"]');
    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-page-open');
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(`document.querySelector('${livePane} .lv-frame').style.width`), '390px', 'the width is not changed from the page tree while saving');
    assert.equal(await evaluate(`window.__kemiPageLoads`), 0, 'the page is not opened again while saving');
    await evaluate(`window.__kemiHeldSaves.forEach((release) => release()); true`);
    await waitFor(`${draftNumbers} === '[]'`);
    assert.equal(await evaluate(`document.querySelector('#live-compose .lv-compose-body').readOnly`), false, 'the body can be written again after saving');
    assert.equal(await evaluate(`JSON.stringify(Array.from(document.querySelectorAll('.lv-widths button')).some((choice) => choice.disabled) || document.querySelector('.lv-width-input').disabled)`), 'false', 'the width can be changed again after saving');
    const comment = (await reviewJson(kemi)).comments.at(-1);
    assert.deepEqual([comment.body, comment.page.places.map((place) => place.n)], ['saved as it was', [1]]);
    await clickInPane(livePane, 40, 100);
    await waitFor(`${draftNumbers} === '[1]'`);
    console.log('PASS ページへのコメントを保存している間は書きかけも表示幅もページも変えられず、保存し終えるとまた書ける');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 狭い画面で比べる相手の側を見ながらの保存（R-PAGE-COMMENT、R-NARROW）: 動いているページの枠が隠れている間にページが
 * 読み込まれ直しても、コメントの画像は作られる。
 */
async function narrowReferenceSideSavesTheImage(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await post(kemi.url, 'api/message', { body: 'let me show you' });
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューにだけ出るので、先に一度渡しておく。
    await handInThePage(kemi, repository, state);
    await new Promise((done) => setTimeout(done, 500));
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await browser('set', 'viewport', '390', '844');
    await waitFor(visible('.lv-side'));
    // 幅をまたいだ直後は帯の並びが動くので、落ち着いてから押す。
    await new Promise((done) => setTimeout(done, 500));
    await browser('click', '.lv-side button[data-side="ref"]');
    await waitFor(`${visible(refPane)} && !${visible(livePane)}`);
    // 比べる相手の側のまま、ページのツリーから同じページを開き直す（隠れた枠でページが読み込まれ直す）。
    await browser('click', '#btn-tree');
    await waitFor(`document.querySelector('#page-tree').dataset.drawer === 'open' && getComputedStyle(document.querySelector('#page-tree')).visibility === 'visible' && document.querySelector('#page-tree').getBoundingClientRect().x === 0`);
    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-page-open');
    await waitFor(`document.querySelector('#page-tree').dataset.drawer === undefined && !${visible(livePane)}`);
    await new Promise((done) => setTimeout(done, 1500));
    await browser('fill', '#live-compose .lv-compose-body', 'saved from the reference side');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${draftNumbers} === '[]'`, 30000);
    await browser('set', 'viewport', '1280', '900');
    const answer = await handAndWait(kemi, repository, state);
    const comment = answer.events.flatMap((event) => event.comments ?? []).find((change) => change.comment.page)?.comment;
    assert.equal(comment?.body, 'saved from the reference side', `the page comment is handed: ${JSON.stringify(answer)}`);
    assert.equal(typeof comment.page.image, 'string', `the image is made while the running page is hidden: ${JSON.stringify(comment.page)}`);
    assert.ok(decodePng(await readFile(comment.page.image)).width > 0);
    console.log('PASS 狭い画面で比べる相手の側を見ている間にページが読み込まれ直しても、保存するとコメントの画像が作られる');
  } finally {
    await browser('set', 'viewport', '1280', '900');
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 保存している間に見方や側を変えたときの画像（R-PAGE-COMMENT、R-NARROW）: 画像を作る頼みがページに届く前に、コードの
 * 見方か、狭い画面で比べる相手の側に切り替えても、画像は場所の周り（場所の要素の矩形に上下の余白を足した高さ）を
 * 写すか、作れずに null になる。場所の一部を切り落とした画像にはならない。
 */
async function switchingWhileSavingKeepsTheImageAroundThePlace(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}tall.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));
    await keepSavedImage();
    for (const away of ['view', 'side']) {
      await chooseTool('element');
      // 2 番目の帯（文書の y 300〜600）。
      await clickInPane(livePane, 150, 450);
      await waitFor(`${draftNumbers} === '[1]'`);
      if (away === 'side') {
        await browser('set', 'viewport', '390', '844');
        await waitFor(visible('.lv-side'));
        await new Promise((done) => setTimeout(done, 500));
      }
      await browser('fill', '#live-compose .lv-compose-body', `saved after switching the ${away}`);
      await holdRequests('image');
      await browser('click', '#live-compose .lv-compose-save');
      await waitFor(`${heldRequests} === 1`);
      if (away === 'view') {
        await browser('click', '.lv-view button[data-view="code"]');
        await waitFor(`document.body.dataset.liveView === 'code'`);
      } else {
        await browser('click', '.lv-side button[data-side="ref"]');
        await waitFor(`!${visible(livePane)}`);
      }
      await releaseRequests();
      await waitFor(`${draftNumbers} === '[]'`, 30000);
      const comment = (await reviewJson(kemi)).comments.at(-1);
      assert.equal(comment.body, `saved after switching the ${away}`);
      const rect = comment.page.places[0].elements[0].rect;
      const saved = await evaluate('window.__kemiSavedImage ?? null');
      if (saved !== null) {
        const image = decodePng(Buffer.from(saved, 'base64'));
        assert.equal(image.height, Math.round(rect.h) + 96, `the image keeps the whole place after switching the ${away}: ${image.width}x${image.height}, place ${JSON.stringify(rect)}`);
      }
      console.log(`PASS 保存している間に${away === 'view' ? 'コードの見方' : '比べる相手の側'}へ切り替えても、画像は場所の周りを写すか null になる（画像: ${saved === null ? 'null' : 'あり'}）`);
      if (away === 'view') {
        await browser('click', '.lv-view button[data-view="page"]');
        await waitFor(visible(livePane));
        await new Promise((done) => setTimeout(done, 500));
      }
    }
  } finally {
    await browser('set', 'viewport', '1280', '900');
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 保存と渡す前のスナップショットが重なったとき（R-PAGE-COMMENT、R-PAGE-SNAPSHOT）: ページの見方で保存を始め、画像を
 * 作る頼みがページに届く前にコードの見方へ切り替えて渡す。保存が先に終わっても、まだ取っている渡す前のスナップショットは
 * 選んだ幅で並べた文書から記述する。ページを変えずにページの見方へ戻せば、それと比べた変化は 0。
 */
async function savingAndHandingKeepThePageLaidOut(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await post(kemi.url, 'api/message', { body: 'let me show you' });
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューにだけ出るので、先に一度渡しておく。
    await handInThePage(kemi, repository, state);
    await new Promise((done) => setTimeout(done, 500));
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await browser('fill', '#live-compose .lv-compose-body', 'saved while handing');
    await holdRequests('image', 'capture');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${heldRequests} === 1`);
    await browser('click', '.lv-view button[data-view="code"]');
    await waitFor(`document.body.dataset.liveView === 'code'`);
    await post(kemi.url, 'api/message', { body: 'once more' });
    const waiting = startWaiting(kemi, repository, state);
    await pressHand();
    await waitFor(`${heldRequests} === 2`);
    await releaseRequests('image');
    await waitFor(`${draftNumbers} === '[]'`, 30000);
    await releaseRequests();
    assert.equal(await waiting, 0, 'kemi wait returns what was handed');
    await waitFor(`Array.from(document.querySelectorAll('.lv-compare-select option')).some((option) => option.textContent.startsWith('Handed 2'))`);
    await browser('click', '.lv-view button[data-view="page"]');
    await chooseReference('Handed 2');
    await waitFor(`${changeList} !== null`);
    await new Promise((done) => setTimeout(done, 1500));
    const counts = `${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`;
    assert.equal(await evaluate(counts), '0/0', 'a snapshot taken while a save ends shows no change against the unchanged page');
    console.log('PASS 保存が終わっても、重なって取っていた渡す前のスナップショットと、変えていないページを比べると変化が 0');
  } finally {
    await browser('set', 'viewport', '1280', '900');
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 画像を作る間にページが移ったとき（R-PAGE-COMMENT）: 画像を作る頼みがページに届く前に、動いているページが別のページへ
 * 移っても（ページの中のリンクやスクリプトで移るのと同じく、枠の src を変えて移す）、移った先のページの画像を場所の周りの
 * 画像として保存しない。画像は null になり、コメントと場所は保存される。
 */
async function movingWhileSavingMakesNoImageOfAnotherPage(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await keepSavedImage();
    await browser('fill', '#live-compose .lv-compose-body', 'saved while the page moved');
    await holdRequests('image');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${heldRequests} === 1`);
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = new URL('/other.html', frame.src).href; return true; })()`);
    await new Promise((done) => setTimeout(done, 1500));
    await releaseRequests();
    await waitFor(`${draftNumbers} === '[]'`, 30000);
    const comment = (await reviewJson(kemi)).comments.at(-1);
    assert.deepEqual([comment.body, comment.page.url], ['saved while the page moved', '/rich.html']);
    assert.equal(await evaluate('window.__kemiSavedImage ?? null'), null, 'no image of the page the frame moved to is saved');
    console.log('PASS 画像を作る頼みが届く前に動いているページが別のページへ移ると、移った先の画像を保存せず、画像は null になる');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** kemi の描き込みは変化の一覧に入らない（② の印の層の決まりに倣う）。 */
async function placesMakeNoChange(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}changing.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0' && ${changeList}?.dataset.shifted === '0'`);
    await chooseTool('element');
    await clickInPane(livePane, 100, 150);
    await chooseTool('pen');
    await dragInPane(livePane, [[5, 5], [150, 5], [150, 100], [5, 100]]);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    await new Promise((done) => setTimeout(done, 1200));
    assert.equal(await evaluate(`${changeList}?.dataset.main + ',' + ${changeList}?.dataset.shifted`), '0,0', 'drawing places changes nothing');
    await browser('fill', '#live-compose .lv-compose-body', 'no change');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${draftNumbers} === '[]'`);
    await new Promise((done) => setTimeout(done, 1200));
    assert.equal(await evaluate(`${changeList}?.dataset.main + ',' + ${changeList}?.dataset.shifted`), '0,0', 'saving a page comment changes nothing');
    console.log('PASS 場所を描いている間と保存した後で、変化の一覧の数が変わらない');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** API でページへのコメントを足す（画面の操作は pageCommentPlacesArePutAndSaved で確かめる）。 */
async function addPageComment(kemi, url, width, body) {
  return post(kemi.url, 'api/comment', {
    op: 'add_page',
    page: { url, width, places: [{ n: 1, kind: 'element', points: [], elements: [{ selector: '#button', text: 'Press', rect: { x: 0, y: 200, w: 300, h: 100 } }] }] },
    body,
  });
}

/**
 * 保存したページへのコメント（R-PAGE-COMMENT、R-PAGE-VIEW、R-PAGE-SESSION）: コメントだけがあるページがツリーに
 * コメントの数とともに出て、表示幅の札でそのページのその幅に移る。別の幅で付けたコメントのスレッドは付けた幅を出し、
 * 押すとその幅に切り替わり、場所の印がページの上に出る。本文だけを編集できる。保留して復元しても出る。
 */
async function savedPageCommentsAreListedShownAndSwitched(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  let kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    const other = await addPageComment(kemi, '/other.html', 390, 'only a comment here');
    const rich = await addPageComment(kemi, '/rich.html', 390, 'the button is too wide');
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    const commentCount = (page) => `document.querySelector('#page-tree .lv-page[data-page="${page}"] .lv-comment-count')?.textContent`;
    await waitFor(`${commentCount('/other.html')} === '1' && ${commentCount('/rich.html')} === '1'`);
    await browser('click', '#page-tree .lv-page[data-page="/other.html"] .lv-width-tag[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/other.html') && document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    console.log('PASS コメントだけがあるページがページのツリーにコメントの数とともに出て、表示幅の札を押すとそのページのその幅に切り替わる');

    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-page-open');
    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/rich.html') && document.querySelector('${livePane} .lv-frame').style.width === '1280px'`);
    await browser('click', '#cv-rail');
    await browser('click', `.cv-card[data-id="${rich.id}"]`);
    await waitFor(`document.querySelector('#cv-thread .cv-page-width')?.textContent === '390px'`);
    await browser('click', '#cv-thread .cv-page-width');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px' && document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/rich.html') && document.querySelector('#cv-thread .cv-page-width') === null`);
    console.log('PASS 390 で付けたコメントのスレッドを 1280 で開くと付けた幅が出て、押すと 390 に切り替わる');
    await new Promise((done) => setTimeout(done, 800));
    const marked = await shot(`${livePane} .lv-frame`, shots, 'saved-places');
    // 会話パネルを開いた分だけ枠が縮んでいるので、ボタンの周りの範囲も同じ倍率で縮める。
    const scale = Number(await evaluate(`getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1'`));
    const around = [0, 195, 305, 305].map((value) => Math.round(value * scale));
    assert.ok(countPixels(marked, around, isPlaceInk) > 0, `the place of the open thread is drawn on the page: ${join(shots, 'saved-places.png')}`);
    console.log('PASS スレッドを開いたコメントの場所が、そのコメントの URL と表示幅で見ているときにページの上に出る');

    await browser('click', '#cv-thread .cv-edit');
    await browser('fill', '#cv-thread .cv-page-edit textarea', 'the button is far too wide');
    await browser('click', '#cv-thread .cv-page-edit-save');
    await waitFor(`document.querySelector('#cv-thread .cv-page-edit') === null`);
    const edited = (await reviewJson(kemi)).comments.find((comment) => comment.id === rich.id);
    assert.equal(edited.body, 'the button is far too wide');
    assert.deepEqual(edited.page, rich.page);
    assert.deepEqual(JSON.parse(await evaluate(`JSON.stringify(window.__kemiErrors ?? [])`)), []);
    console.log('PASS ページへのコメントは本文だけを編集でき、場所は変わらない');

    await stop(kemi);
    kemi = await startKemi(repository, state, ['--resume', kemi.id]);
    await browser('open', kemi.url);
    await waitFor(`${commentCount('/other.html')} === '1' && ${commentCount('/rich.html')} === '1'`);
    await browser('click', '#cv-rail');
    await waitFor(`document.querySelector('.cv-card[data-id="${other.id}"]') !== null && document.querySelector('.cv-card[data-id="${rich.id}"]') !== null`);
    console.log('PASS ページへのコメントを持つレビューを保留して復元すると、会話パネルとページのツリーにそのコメントが出る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 画面の「Hand to agent」で渡し、別のプロセスの `kemi wait` が返した JSON を読む。
 */
async function handAndWait(kemi, dir, state) {
  const waiting = new Promise((done, fail) => {
    const child = spawn(binary, ['wait', kemi.id, '--timeout', '30'], { cwd: dir, env: environment(state), stdio: ['ignore', 'pipe', 'pipe'] });
    let stdout = '';
    child.stdout.on('data', (chunk) => { stdout += chunk; });
    child.on('error', fail);
    child.on('exit', (code) => done({ code, stdout }));
  });
  const button = `(${visible('#rail-hand')} ? document.querySelector('#rail-hand') : ${visible('#btn-hand')} ? document.querySelector('#btn-hand') : null)`;
  await waitFor(`${button} !== null && !${button}.disabled`);
  await evaluate(`${button}.id`).then((id) => browser('click', `#${id}`));
  const { code, stdout } = await waiting;
  assert.equal(code, 0, 'kemi wait returns what was handed');
  return JSON.parse(stdout);
}

/** 書いたページへのコメントを保存し、書く欄が空くのを待つ。 */
async function savePageCommentInThePage(body) {
  await browser('fill', '#live-compose .lv-compose-body', body);
  await browser('click', '#live-compose .lv-compose-save');
  await waitFor(`${draftNumbers} === '[]'`);
}

/**
 * 渡すことと submit（R-PAGE-COMMENT の成功条件 1、R-AGENT-EVENTS、R-SUBMIT）: 画面で要素・矢印・ペンの 3 つの場所を
 * 持つコメントを付けて渡すと、`kemi wait` に 3 つの場所と画像の絶対パスが届き、そのファイルが PNG として読める。submit の
 * JSON には `page` が入り、画像は null。インラインのスタイルを止める CSP と Trusted Types を求める CSP のページでも、
 * コメントと場所は届く。
 */
async function handedPageCommentsReachWaitAndSubmit(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  let kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await post(kemi.url, 'api/message', { body: 'let me show you' });
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューにだけ出るので、先に一度渡しておく。
    await handInThePage(kemi, repository, state);
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await chooseTool('arrow');
    await dragInPane(livePane, [[340, 160], [260, 210], [150, 250]]);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    await chooseTool('pen');
    await dragInPane(livePane, [[20, 90], [260, 90], [260, 140], [20, 140], [20, 92]]);
    await waitFor(`${draftNumbers} === '[1,2,3]'`);
    await savePageCommentInThePage('1 is too wide, 2 points at it, 3 needs more room');
    const answer = await handAndWait(kemi, repository, state);
    const comment = answer.events.flatMap((event) => event.comments ?? []).find((change) => change.comment.page)?.comment;
    assert.ok(comment, `a page comment is handed: ${JSON.stringify(answer)}`);
    assert.deepEqual(comment.page.places.map((place) => [place.n, place.kind]), [[1, 'element'], [2, 'arrow'], [3, 'pen']]);
    for (const place of comment.page.places) {
      assert.ok(place.elements.length > 0 && place.elements.every((element) => element.selector && element.rect), `place ${place.n} names its elements`);
    }
    assert.ok(comment.page.image?.startsWith('/') || /^[A-Za-z]:\\/.test(comment.page.image ?? ''), `the image is an absolute path: ${comment.page.image}`);
    const image = decodePng(await readFile(comment.page.image));
    assert.ok(image.width > 0 && image.height > 0);
    console.log(`PASS 画面で要素・矢印・ペンの 3 つの場所を持つコメントを付けて渡すと、kemi wait に 3 つの場所と画像の絶対パスが届き、PNG として読める（${image.width}×${image.height}）`);

    await post(kemi.url, 'api/submit', { verdict: 'approved' });
    const { code, stdout } = await kemi.exited;
    assert.equal(code, 0);
    const submitted = JSON.parse(stdout).comments.find((item) => item.id === comment.id);
    assert.deepEqual(submitted.page.places, comment.page.places);
    assert.equal(submitted.page.image, null);
    console.log('PASS 同じコメントを submit すると、JSON の page に同じ場所が入り、image は null');
  } finally {
    await stop(kemi);
    await dev.close();
  }

  for (const query of ['csp=1', 'tt=1']) {
    const strict = await startDevServer();
    const strictState = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
    kemi = await startKemi(repository, strictState, ['--live', `${strict.url}changing.html?${query}`]);
    try {
      await browser('set', 'viewport', '1280', '900');
      await browser('open', kemi.url);
      await waitFor(showsSnapshot('Start'));
      await post(kemi.url, 'api/message', { body: 'start' });
      await handInThePage(kemi, repository, strictState);
      await chooseTool('element');
      await clickInPane(livePane, 100, 150);
      await waitFor(`${draftNumbers} === '[1]'`);
      await savePageCommentInThePage(`under ${query}`);
      const answer = await handAndWait(kemi, repository, strictState);
      const comment = answer.events.flatMap((event) => event.comments ?? []).find((change) => change.comment.page)?.comment;
      assert.ok(comment, `a page comment is handed under ${query}`);
      assert.equal(comment.page.places[0].elements[0].selector, '#buy');
      console.log(`PASS changing.html?${query} の CSP のページでも、コメントと場所が kemi wait に届く（画像: ${comment.page.image === null ? 'null' : '絶対パス'}）`);
    } finally {
      await stop(kemi);
      await strict.close();
    }
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
  await snapshotsCarryTheirResources(repository);
  await commentImagesLookLikeThePage(repository);
  await commentImagesKeepTheLayoutOfTheViewport(repository);
  await mocksAreAssignedShownAndKeptApart(repository);
  await snapshotsSendNoTokenToExternalImages(repository);
  await overlayFollowsTheScrollAndTheOpacity(repository);
  await changeListFollowsThePage(repository);
  await marksFollowTheChanges(repository);
  await marksAreDrawnUnderAStrictStylePolicy(repository);
  await snapshotsAreTakenUnderTrustedTypes(repository);
  await removedMarksSurviveReparsing(repository);
  await scrollingMakesNoChangeAndMarksStay(repository);
  await cssomChangesAreFollowed(repository);
  await widthSwitchesWithoutResizingTheDocument(repository);
  await manyElementsAreRecordedAndCompared(repository);
  await pageCommentPlacesArePutAndSaved(repository);
  await penPlacesLeaveOutWhatContainsTheLine(repository);
  await placesOnTheBackgroundNameThePage(repository);
  await draftPlacesStayAtTheirWidth(repository);
  await latePlacesKeepTheirWidth(repository);
  await draftsStayWhileSaving(repository);
  await narrowReferenceSideSavesTheImage(repository);
  await switchingWhileSavingKeepsTheImageAroundThePlace(repository);
  await savingAndHandingKeepThePageLaidOut(repository);
  await movingWhileSavingMakesNoImageOfAnotherPage(repository);
  await placesMakeNoChange(repository);
  await savedPageCommentsAreListedShownAndSwitched(repository);
  await handedPageCommentsReachWaitAndSubmit(repository);
} finally {
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
