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
//   渡すと取る。手で取ると比べる相手がそれに切り替わって知らせが出て、見る対象だけのときは知らせの操作で並べ、幅 390px では
//   比べる相手の 1 枚にする。その後に渡しても変わらず、読み込み直すと自動に戻る。別のオリジンの CSS・@import・style 属性の url()・<picture> の <source>・video の
//   poster・SVG の <image>・<input type=image> が、スナップショットでも動いているページと同じ色に出る。
// - コメントの画像: ページの中で写しを描いて作った描き込み無しの画像を、動いているページの同じ範囲と画素で比べる
//   （差の割合と差の画像を出して人の確認に回す）。描き込みを重ねた画像も残す。インラインのスタイルを止める CSP と
//   Trusted Types を求める CSP のページでは、画像が作れるか、作れない理由が返る。範囲が表示幅より狭く画面の高さと違っても、
//   動いているページと同じ幅の CSS と vh で描く。
// - ページへのコメント: 要素・ペン・矢印で場所を置くと番号が振られてページの上に描かれ、2 番目を消すと番号と本文の #n が
//   詰まり、渡した JSON も同じ。消した場所を指す #n があると保存できない。一覧の番号を押すと本文に #n が入る。矢印の先の要素は先端の位置の要素。並べた比べる相手の側では場所が増えず、重ねて透かしている間は
//   見る対象の要素が場所になる。ペンの場所には囲んだ範囲を丸ごと含む外側の要素が入らず、1 つの要素の内側だけを囲むと
//   その要素 1 つになる。要素の道具で余白を押しても場所は増えず、余白を指す矢印と余白だけを囲むペンは要素の無い範囲だけの場所になり、画像は文書全体ではなく場所の周りを写す。書きかけの場所と別の表示幅では場所を足せず保存もできず、戻る操作で戻ると足せる。ページが場所を返す前に表示幅を変えても、場所は押したときの幅のものになる。保存している間は書きかけも表示幅もページも変えられず、保存し終えるとまた書ける。幅 390px で比べる相手の側を見ている間にページが読み込まれ直しても、保存すると画像が作られる。保存している間にコードの見方や比べる相手の側へ切り替えても、画像は場所の周りを写す。画像を作る頼みが届く前に動いているページが別のページへ移ると、画像は null になる。描いている間も保存した後も変化の一覧は変わらない。書く欄を閉じると（保存・取り消し）見る対象の枠は欄を開く前の高さに戻り、最初の保存で案内が消えると空いた高さまで伸びる。書きかけの場所の行に乗せるとその場所が光り、押すとそこまでスクロールして光る。スレッドの「ページで見る」は、別の表示幅のコメントでも幅を切り替えてから、そのコメントの場所までスクロールして光らせる。コメントだけがあるページがツリーに
//   コメントの数とともに出て、表示幅の札で移れる。別の幅で付けたコメントのスレッドは付けた幅を出し、押すとそこへ移り、
//   場所の印が出る。保存したコメントの場所は、スレッドを開いている間だけ番号付きで、それ以外は番号の無い小さな印で、
//   乗せるとレビュー画面にそのコメントの短い名前が出る。本文だけを編集できる。保留して復元しても会話パネルとツリーに出る。画面で付けた 3 つの場所を持つ
//   コメントを渡すと、kemi wait に場所と PNG の画像の絶対パスが届き、submit では画像が null になる。CSP の厳しいページでも届く。
// - 保留と復元: コメントと手で取ったスナップショットのあるレビューを保留して復元すると、比べる相手の選択に開始時と
//   手で取ったものが出て、開始時が既定になる。開始時のものは保留の前と同じ id と HTML で 1 つだけ（取り直さない）。
//   モックを割り当てて保留し、モックのファイルを消してから復元すると、読めない旨が出る。
// - モック: 範囲の外と .txt を理由つきで断る。CSS と画像ごと同じ幅で出る。外すとスナップショットに
//   戻る。JS のモックが描かれ、トークンが（referrer からも）得られず API に断られる。モックだけがあるページがツリーに出る。
//   スナップショットの中の外部の画像は、referrerpolicy="unsafe-url" を付けていてもトークンの URL を受け取らない。
// - 重ねて透かす: スクロールがそろう、透かし具合で見え方が変わる、幅 390px でも切り替えられる。
// - 差分: スナップショットを取った後に同じ URL の中身を変えると、読み込み直さずに変化の一覧が変わる。
//   兄弟の途中に足した要素だけが増えたになり、ボタンの背景色の変化が前後の色の見本つきで主な変化に入る。
//   ボタンの文字色・背景色・枠を同時に変えると一覧ではそのボタンが 1 行になり、押すとそのボタンまでスクロールして印が光る。
//   見る対象だけのときも、一覧の見出しの名前が並べたときの比べる相手の選択の文字に含まれる。html と body はずれただけに
//   出ず、body の背景色の変化は主な変化に出る。下の方の消えた要素の行を押すと、見る対象だけなら並べる見比べ方に、幅 390px
//   では比べる相手の 1 枚にしてスナップショットのその要素が見えて光る（その間もホイールで動かせ、見比べ方を変えると元の形に
//   戻る）。重ねて透かすときは見比べ方を変えず、そろったままその位置へ動いて光る。
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
// - 狭い画面の道具: 幅 390px のページの見方では、道具と「Hand to agent」が画面の下の 1 つの浮かぶツールバーにあり、
//   コードの見方では全モード共通の浮かぶ「Hand to agent」だけ。幅 1280px ではどちらも見えない。
// - 要素の多いページ: HTML が 2 MB 未満なら、要素が 7 万を超えてもスナップショットが取れ、変化の一覧が出る。
//   一覧の残りも続きを出す操作ですべて見られる。
import assert from 'node:assert/strict';
import { spawn, execFile } from 'node:child_process';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
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
 * 画面の「Hand to agent」で渡す。ボタンはエージェントが kemi wait を呼んだレビューでだけ押せるので、
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

    // Enter を押さずに欄から離れても効き、効いた後も欄に今の幅が残る。プリセットの幅ならそのボタンが選ばれる。
    await browser('fill', '.lv-width-input', '1000');
    await browser('click', '.lv-widths .lv-label');
    await waitFor(`document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-frame').style.width === '1000px' && document.querySelector('.lv-width-input').value === '1000' && document.querySelector('.lv-widths button[aria-pressed="true"]') === null`);
    await browser('fill', '.lv-width-input', '390');
    await browser('click', '.lv-widths .lv-label');
    await waitFor(`document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-frame').style.width === '390px' && document.querySelector('.lv-widths button[aria-pressed="true"]')?.dataset.width === '390'`);
    await browser('click', '.lv-widths button[data-width="1280"]');
    console.log('PASS 表示幅の欄に 1000 を入れて離れると 1000 で描かれ欄に 1000 が残り、390 を入れると 390 のボタンが選ばれる');

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

/** 上部バーのコード用の操作と数（R-PAGE-MODE）。ページの見方の間は見えない。 */
const CODE_TOPBAR = ['.tools .seg[aria-label="Display mode"]', '#btn-wrap', '#chip-focus', '#chip-sort', '#review-meta', '#progress'];
const codeTopbarShown = (shown) => CODE_TOPBAR.map((selector) => `${shown ? '' : '!'}${visible(selector)}`).join(' && ');

/**
 * モードのタブとページの見方の上部バー（R-PAGE-MODE、R-VIEW の `--live` のタブ）: ページの見方の間は上部バーにコード用の
 * 操作と数が見えず、ページの URL と表示幅とエージェントの状態が見える。コードのタブには変更ファイルの数が出て、押すと
 * コード用の操作と数が見え、ページのタブで戻る。ページへのコメントを保存した後も見たの進捗は戻らない。ページの見方の間に
 * 作業ツリーを変えると更新バッジが出て、押すとコードの見方に切り替わって差分が新しくなる。
 */
async function modeTabsSwitchTheTopbar(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`${visible('#live-stage')} && document.body.dataset.liveView === 'page'`);
    await waitFor(codeTopbarShown(false));
    const top = '.topbar .lv-topmeta';
    await waitFor(`${visible(top)} && document.querySelector('${top}').textContent.includes('/rich.html') && document.querySelector('${top}').textContent.includes('1280')`);
    await waitFor(`${visible('.topbar .lv-top-agent')} && document.querySelector('.topbar .lv-top-agent').dataset.kemiAgentState === document.querySelector('#agent-status').dataset.kemiAgentState && document.querySelector('.topbar .lv-top-agent').textContent === document.querySelector('#agent-status').textContent`);
    const files = (await reviewJson(kemi)).groups.flatMap((group) => group.files).length;
    assert.ok(files > 0);
    await waitFor(`document.querySelector('.topbar .lv-view button[data-view="code"]').textContent.includes(${JSON.stringify(String(files))})`);
    await browser('click', '.lv-widths button[data-width="768"]');
    await waitFor(`document.querySelector('${top}').textContent.includes('768')`);
    console.log('PASS ページの見方の上部バーに、コード用の操作と数が見えず、ページの URL・表示幅・エージェントの状態が見え、コードのタブに変更ファイルの数が出る');

    await browser('click', '.topbar .lv-view button[data-view="code"]');
    await waitFor(`document.body.dataset.liveView === 'code' && ${codeTopbarShown(true)} && !${visible(top)}`);
    assert.equal(await evaluate(visible('#live-band')), false, 'the code view shows no page-view controls');
    await browser('click', '.topbar .lv-view button[data-view="page"]');
    await waitFor(`document.body.dataset.liveView === 'page' && ${codeTopbarShown(false)}`);
    console.log('PASS コードのタブを押すとコード用の操作と数が見えてページの見方の操作の帯は見えず、ページのタブで戻る');

    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await savePageCommentInThePage('the button');
    await waitFor(`document.querySelector('#comment-count').textContent === '1'`);
    assert.equal(await evaluate(visible('#progress')), false, 'the seen progress stays hidden after saving a page comment');
    console.log('PASS ページへのコメントを保存した後も、ページの見方では見たの進捗が見えない');

    await writeFile(join(repository, 'a.txt'), 'one\nTWO\nthree from the agent\n');
    await waitFor(visible('#update-badge'));
    await browser('click', '#update-badge');
    await waitFor(`document.body.dataset.liveView === 'code' && Array.from(document.querySelectorAll('[data-kemi-row]')).some((row) => row.textContent.includes('three from the agent'))`);
    console.log('PASS ページの見方の間に作業ツリーを変えると更新バッジが出て、押すとコードの見方に切り替わり差分が新しくなる');
  } finally {
    await writeFile(join(repository, 'a.txt'), 'one\nTWO\n');
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
    await chooseTool('interact');
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

/** 比べる相手の選択を自動に戻す。 */
async function chooseAuto() {
  await evaluate(`(() => { const select = document.querySelector('.lv-compare-select'); select.value = 'latest'; select.dispatchEvent(new Event('change')); return true; })()`);
}

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
    await chooseCompare('side');
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
    // 手で取ると比べる相手がその時点に切り替わり、選んだ時点は今の幅で探すので、1280 では記録されていない。
    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(notRecorded);
    await chooseAuto();
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

    await chooseTool('interact');
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
    await chooseCompare('side');
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
    await chooseCompare('side');
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
    await chooseCompare('side');
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

    // 狭い画面では重ねて透かさず、1 枚ずつ切り替えて見る（R-PAGE-VIEW）。
    await browser('set', 'viewport', '390', '800');
    await waitFor(`${visible('.lv-side')} && ${visible(livePane)} && !${visible(refPane)}`);
    await new Promise((done) => setTimeout(done, 500));
    await browser('click', '.lv-side button[data-side="ref"]');
    await waitFor(`${visible(`${refPane} .lv-frame:not([hidden])`)} && !${visible(livePane)} && getComputedStyle(document.querySelector('${refPane} .lv-box')).opacity === '1'`);
    await browser('click', '.lv-side button[data-side="live"]');
    await waitFor(`${visible(livePane)} && !${visible(refPane)}`);
    console.log('PASS 広い画面で重ねて透かすを選んだまま幅 390px にすると、重ねずに 1 枚ずつ切り替えて見る');
  } finally {
    await browser('set', 'viewport', '1280', '800');
    await stop(kemi);
    await dev.close();
  }
}

/** 見比べ方を切り替え、舞台がその見比べ方になるのを待つ。 */
async function chooseCompare(mode) {
  await waitFor(visible(`.lv-mode button[data-compare="${mode}"]`));
  await browser('click', `.lv-mode button[data-compare="${mode}"]`);
  await waitFor(`document.querySelector('#live-stage').dataset.compare === '${mode}'`);
}

/** 見る対象の見出しに出ている倍率。 */
const liveScale = `Number(document.querySelector('${livePane} .lv-bar-label').textContent.match(/×([0-9.]+)/)?.[1] ?? NaN)`;

/** /tall.html の最後の帯の色（hsl(324, 70%, 70%)）。 */
const isLastBand = ([r, g, b]) => r > 215 && g > 105 && g < 145 && b > 170 && b < 210;

/**
 * 見比べ方と倍率と読み込み直す操作（R-PAGE-VIEW、R-PAGE-REF）: 開いた直後は見る対象だけで比べる相手の選択が見えず、
 * 並べる・重ねるの間だけ見える。等倍では倍率が 1 と出て、枠の中をスクロールしてページの下端まで見られ、枠に合わせると
 * 縮む。読み込み直す操作で見る対象が読み込み直される。
 */
async function compareModesScaleAndReload(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}tall.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`${visible(livePane)} && document.querySelector('#live-stage').dataset.compare === 'now' && document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/tall.html')`);
    assert.equal(await evaluate(`${visible('.lv-compare-select')} || ${visible(refPane)}`), false, 'only the running page is shown at first');
    for (const mode of ['side', 'overlay']) {
      await chooseCompare(mode);
      await waitFor(visible('.lv-compare-select'));
    }
    await chooseCompare('now');
    await waitFor(`!${visible('.lv-compare-select')} && !${visible(refPane)}`);
    console.log('PASS 開いた直後は見る対象だけで比べる相手の選択が見えず、並べる・重ねるに切り替えると見え、見る対象だけに戻すと見えない');

    await waitFor(`${liveScale} < 1`);
    await browser('click', '.lv-zoom button[data-zoom="full"]');
    await waitFor(`${liveScale} === 1 && document.querySelector('${livePane} .lv-viewport').scrollWidth > document.querySelector('${livePane} .lv-viewport').clientWidth`);
    const middle = await evaluate(`(() => { const r = document.querySelector('${livePane} .lv-viewport').getBoundingClientRect(); return { x: Math.round(r.x + r.width / 2), y: Math.round(r.y + r.height / 2), height: Math.floor(r.height) }; })()`);
    // 別のオリジンの枠にはホイールが届かないので、ページを押してから End を押す。
    await chooseTool('interact');
    await browser('mouse', 'move', String(middle.x), String(middle.y));
    await browser('mouse', 'down');
    await browser('mouse', 'up');
    await browser('press', 'End');
    await waitForPixels(`${livePane} .lv-viewport`, shots, 'full-bottom', [0, middle.height - 60, 300, middle.height - 30], isLastBand);
    await browser('click', '.lv-zoom button[data-zoom="fit"]');
    await waitFor(`${liveScale} < 1`);
    console.log('PASS 等倍にすると倍率が 1 と出て、枠の中でスクロールしてページの下端まで見られ、枠に合わせると縮む');

    // 等倍で横と縦にスクロールしたまま重ねても、比べる相手（開始時のスナップショット。ページは変えていない）が重なる。
    await browser('click', '.lv-zoom button[data-zoom="full"]');
    await chooseCompare('overlay');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'snapshot'`);
    await evaluate(`document.querySelector('${livePane} .lv-viewport').scrollLeft = 200; true`);
    await waitFor(`document.querySelector('${refPane} .lv-viewport').scrollLeft === 200 && /translate\\(0px, -[1-9]/.test(document.querySelector('${refPane} .lv-frame:not([hidden])').style.transform)`);
    await new Promise((done) => setTimeout(done, 500));
    const setOpacity = async (value) => {
      await evaluate(`(() => { const range = document.querySelector('.lv-opacity'); range.value = '${value}'; range.dispatchEvent(new Event('input')); return true; })()`);
      await new Promise((done) => setTimeout(done, 300));
    };
    await setOpacity(0);
    const underneath = await shot(`${livePane} .lv-viewport`, shots, 'full-overlay-0');
    await setOpacity(100);
    const overlaid = await shot(`${livePane} .lv-viewport`, shots, 'full-overlay-100');
    const compared = await comparePixels(underneath, overlaid, join(shots, 'full-overlay-diff.png'));
    if (compared.different === 0) {
      console.log('PASS 等倍で横と縦にスクロールしたまま重ねて透かしても、比べる相手が見る対象と画素まで重なる');
    } else {
      console.log(`CHECK 等倍で重ねた比べる相手と見る対象の画素が ${compared.different} / ${compared.total}（${(compared.ratio * 100).toFixed(3)}%）違う。差の画像: ${join(shots, 'full-overlay-diff.png')}（人が確かめる）`);
    }
    await chooseCompare('now');
    await browser('click', '.lv-zoom button[data-zoom="fit"]');

    const before = dev.loads('/tall.html');
    await browser('click', '.lv-reload');
    for (const until = Date.now() + 10000; dev.loads('/tall.html') === before;) {
      assert.ok(Date.now() < until, 'the page is loaded again');
      await new Promise((done) => setTimeout(done, 100));
    }
    console.log('PASS 読み込み直す操作を押すと、見る対象のページが読み込み直される');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 比べる相手の名前と見出し（R-PAGE-REF）と、見る対象だけのときの知らせ・覚える範囲（R-PAGE-VIEW）: 2 回渡してから並べると、
 * 選択に出ている名前は 2 回目に渡した時点の項目の名前を含む。重ねて透かす見出しは、透かし具合の途中・両端と見る対象だけの
 * ときで互いに違う。見る対象だけのまま 2 MB を超えるページを手で取ると、取れなかったことが見える。並べる・等倍にしてから
 * 画面を読み込み直すと、見る対象だけ・枠に合わせるに戻る。
 */
async function referenceNamesHeadingsAndNotices(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}tall.html`]);
  const opacity = async (value) => {
    await evaluate(`(() => { const range = document.querySelector('.lv-opacity'); range.value = '${value}'; range.dispatchEvent(new Event('input')); return true; })()`);
  };
  const heading = `document.querySelector('.lv-stage-name').textContent`;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/tall.html')`);
    for (const body of ['first', 'second']) {
      await post(kemi.url, 'api/message', { body });
      await handInThePage(kemi, repository, state);
    }
    const handed = (await getJson(kemi.url, 'api/snapshots')).snapshots.filter((snapshot) => snapshot.kind === 'handed' && snapshot.width === 1280);
    assert.equal(handed.length, 2);
    await chooseCompare('side');
    const second = `document.querySelector('.lv-compare-select option[value="${handed[1].id}"]')`;
    await waitFor(`${second} !== null && document.querySelector('.lv-compare-select').value === 'latest' && document.querySelector('.lv-compare-select').selectedOptions[0].textContent.includes(${second}.textContent)`);
    console.log('PASS 2 回渡してから並べると、選択に出ている名前が選択肢の 2 回目に渡した時点の項目の名前を含む');

    await chooseCompare('overlay');
    const headings = [];
    for (const value of [0, 50, 100]) {
      await opacity(value);
      headings.push(await evaluate(heading));
    }
    await chooseCompare('now');
    headings.push(await evaluate(heading));
    assert.equal(new Set(headings).size, 4, JSON.stringify(headings));
    console.log('PASS 重ねて透かす見出しは、透かし具合の 0・途中・100 と見る対象だけのときで互いに違う');

    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${kemi.live.replace(/\/tall\.html$/, '')}/big.html`)}; true`);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/big.html') && document.querySelector('${livePane} .lv-notice').hidden`);
    await browser('click', '.lv-band .lv-record-now');
    await waitFor(`${visible('.lv-band .lv-notice')} && document.querySelector('.lv-band .lv-notice').textContent.trim() !== ''`, 30000);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare`), 'now');
    console.log('PASS 見る対象だけのまま 2 MB を超えるページを手で取ると、取れなかったことが見える');

    await chooseCompare('side');
    await browser('click', '.lv-zoom button[data-zoom="full"]');
    await waitFor(`${liveScale} === 1`);
    await browser('open', kemi.url);
    await waitFor(`${visible(livePane)} && document.querySelector('#live-stage').dataset.compare === 'now' && document.querySelector('.lv-zoom button[data-zoom="fit"]').getAttribute('aria-pressed') === 'true'`);
    console.log('PASS 並べると等倍にしてから画面を読み込み直すと、見る対象だけ・枠に合わせるに戻る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** 帯の知らせ（取れたこと・取れなかったこと）と、その操作。 */
const bandNotice = '#live-band .lv-band-notice';
const bandNoticeAction = `${bandNotice} .lv-notice-action`;

/**
 * 手で取ったときの比べる相手（R-PAGE-REF、R-PAGE-SNAPSHOT）: 見る対象だけの見比べ方で手で取ると、見比べ方は変わらず、
 * 比べる相手が取ったスナップショットになり、取ったことの知らせが出る。知らせの操作を押すと並べる見比べ方になり、取った
 * スナップショットが並ぶ。その後に渡しても比べる相手は取ったスナップショットのままで、レビュー画面を読み込み直すと自動に
 * 戻る。幅 390px で手で取ると知らせの操作で比べる相手の 1 枚に切り替わり、取ったスナップショットが出る。
 */
async function recordingNowSwitchesTheReference(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`${showsSnapshot('Start')} && document.querySelector('#live-stage').dataset.compare === 'now'`);
    await browser('click', '.lv-band .lv-record-now');
    await waitFor(`${showsSnapshot('Recorded 1')} && ${visible(bandNotice)} && document.querySelector('${bandNotice}').dataset.kind === 'done'`);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare`), 'now', 'recording keeps the way of comparing');
    await browser('click', bandNoticeAction);
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'side' && ${visible(refPane)} && ${showsSnapshot('Recorded 1')}`);
    console.log('PASS 見る対象だけで手で取ると、見比べ方は変わらず比べる相手が取ったものになって知らせが出て、知らせの操作で並べるとそれが並ぶ');

    await post(kemi.url, 'api/message', { body: 'please look' });
    await handInThePage(kemi, repository, state);
    await new Promise((done) => setTimeout(done, 1000));
    assert.equal(await evaluate(showsSnapshot('Recorded 1')), true, 'handing does not move the reference away from the recorded snapshot');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitFor(`document.querySelector('.lv-compare-select').value === 'latest' && ${showsSnapshot('Handed 1')}`);
    console.log('PASS 手で取った後に渡しても比べる相手は取ったもののままで、読み込み直すと自動に戻る');

    await browser('set', 'viewport', '390', '844');
    await browser('open', kemi.url);
    await waitFor(`${visible(livePane)} && ${visible('#live-band .lv-menu-button')} && document.querySelector('#live-stage').dataset.side === 'live'`);
    await browser('click', '#live-band .lv-menu-button');
    await waitFor(visible('.lv-menu .lv-record-now'));
    await evaluate(`document.querySelector('.lv-menu .lv-record-now').click(); document.querySelector('.lv-menu').hidePopover(); true`);
    await waitFor(`${showsSnapshot('Recorded 2')} && ${visible(bandNoticeAction)}`);
    await browser('click', bandNoticeAction);
    await waitFor(`document.querySelector('#live-stage').dataset.side === 'ref' && ${visible(refPane)} && ${showsSnapshot('Recorded 2')}`);
    console.log('PASS 幅 390px で手で取ると知らせが出て、その操作で比べる相手の 1 枚に切り替わり、取ったものが出る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 並べたときの上端（R-PAGE-REF）: パスが 200 文字のモックを割り当てて並べ、見出しが長くなる幅にしても、両方のページの
 * 上端の差が 1px 以内。
 */
async function sideBySidePagesShareTheirTop(repository) {
  // git が無視する場所に置き、ほかの検査の作業ツリーの差分を変えない。
  await writeFile(join(repository, '.git', 'info', 'exclude'), 'ignored-mocks/\n');
  await mkdir(join(repository, 'ignored-mocks'), { recursive: true });
  const path = `ignored-mocks/${'m'.repeat(200 - 'ignored-mocks/'.length - '.html'.length)}.html`;
  assert.equal(path.length, 200);
  await writeFile(join(repository, path), '<!doctype html><html><body style="margin:0"><p>mock</p></body></html>\n');
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}other.html`]);
  try {
    await browser('set', 'viewport', '900', '800');
    await browser('open', kemi.url);
    await waitFor(visible('.lv-mock-input'));
    await browser('fill', '.lv-mock-input', path);
    await browser('click', '.lv-mock-assign');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock'`);
    await chooseCompare('side');
    await waitFor(`${visible(`${refPane} .lv-viewport`)} && ${visible(`${livePane} .lv-viewport`)}`);
    const gap = await evaluate(`Math.abs(document.querySelector('${refPane} .lv-viewport').getBoundingClientRect().top - document.querySelector('${livePane} .lv-viewport').getBoundingClientRect().top)`);
    assert.ok(gap <= 1, `the tops differ by ${gap}px`);
    console.log(`PASS パスが 200 文字のモックを並べても、両方のページの上端の差が 1px 以内（${gap}px）`);
  } finally {
    await stop(kemi);
    await dev.close();
    await rm(join(repository, 'ignored-mocks'), { recursive: true, force: true });
  }
}

/** 見る対象だけの見比べ方でも、変化の一覧と見る対象の側の印が出る（R-PAGE-VIEW、R-PAGE-REF）。 */
async function changesShowWithThePageAlone(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}changing.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/changing.html')`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px' && document.querySelector('${livePane} .lv-notice').hidden`);
    await browser('click', '.lv-band .lv-record-now');
    await waitFor(`${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    await waitForPixels(`${livePane} .lv-frame`, shots, 'alone-marks', changingRegions(false).button, isRed);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare`), 'now');
    console.log('PASS 見る対象だけのまま CSS を変えると、変化の一覧と見る対象の側の印が出る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 道具の既定と始め方の案内（R-PAGE-COMMENT）: 開いた直後の道具は要素で、ページを押すと場所が置かれる。道具を選び直した
 * だけでは書く欄が開かず、場所を置くと開く。案内はページへのコメントを保存すると消え、そのコメントを消しても出ない。
 * 要素の道具のまま等倍にしても、横にも下端までもスクロールできる。
 */
async function toolsAndTheHintStartTheFirstComment(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  const compose = '#live-compose';
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden && ${visible('.lv-tools')}`);
    assert.equal(await evaluate(`document.querySelector('.lv-tools button[aria-pressed="true"]')?.dataset.tool`), 'element');
    await waitFor(`${visible('.lv-hint')} && !${visible(compose)}`);
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]' && ${visible(compose)}`);
    console.log('PASS 開いた直後の道具は要素で、案内が出ていて、ページを押すと場所が置かれる');

    await browser('click', `${compose} .lv-compose-cancel`);
    await waitFor(`!${visible(compose)}`);
    for (const tool of ['arrow', 'pen', 'interact', 'element']) {
      await chooseTool(tool);
      await new Promise((done) => setTimeout(done, 200));
      assert.equal(await evaluate(visible(compose)), false, `choosing ${tool} opens no comment box`);
    }
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]' && ${visible(compose)}`);
    console.log('PASS 道具を選び直しただけでは書く欄が開かず、場所を置くと開く');

    await savePageCommentInThePage('the button');
    await waitFor(`!${visible('.lv-hint')}`);
    const [saved] = (await reviewJson(kemi)).comments;
    await post(kemi.url, 'api/comment', { op: 'delete', id: saved.id });
    await browser('open', kemi.url);
    await waitFor(`${visible('.lv-tools')} && document.querySelector('#comment-count').textContent === '0'`);
    await new Promise((done) => setTimeout(done, 500));
    assert.equal(await evaluate(visible('.lv-hint')), false, 'the hint does not come back after the comment is deleted');
    console.log('PASS 案内はページへのコメントを保存すると消え、そのコメントを消して開き直しても出ない');
  } finally {
    await stop(kemi);
    await dev.close();
  }

  const tallDev = await startDevServer();
  const tall = await startKemi(repository, await mkdtemp(join(tmpdir(), 'kemi-live-state-')), ['--live', `${tallDev.url}tall.html`]);
  try {
    await browser('open', tall.url);
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden && ${visible('.lv-tools')}`);
    assert.equal(await evaluate(`document.querySelector('.lv-tools button[aria-pressed="true"]')?.dataset.tool`), 'element');
    await browser('click', '.lv-zoom button[data-zoom="full"]');
    await waitFor(`${liveScale} === 1`);
    const viewport = `document.querySelector('${livePane} .lv-viewport')`;
    const height = await evaluate(`Math.floor(${viewport}.getBoundingClientRect().height)`);
    // agent-browser のホイールは押した位置に届かない（画面の左上に届く）ので、道具の層の上で回したホイールを作って送る。
    const wheel = (x, y) => evaluate(`(() => { const layer = document.querySelector('${livePane} .lv-capture'); const r = layer.getBoundingClientRect(); return layer.dispatchEvent(new WheelEvent('wheel', { deltaX: ${x}, deltaY: ${y}, clientX: r.x + 100, clientY: r.y + 100, bubbles: true, cancelable: true })); })()`);
    await wheel(300, 0);
    await waitFor(`${viewport}.scrollLeft > 0`);
    await wheel(0, 6000);
    await waitForPixels(`${livePane} .lv-viewport`, shots, 'element-tool-bottom', [0, height - 60, 300, height - 30], isLastBand);
    assert.equal(await evaluate(draftNumbers), '[]', 'scrolling puts no place');
    console.log('PASS 要素の道具のまま等倍にしても、横にも下端までもスクロールできる');
  } finally {
    await stop(tall);
    await tallDev.close();
  }
}

/** 見る対象の枠の、縮める前の高さ（ページの CSS ピクセル）。 */
const liveFrameHeight = `document.querySelector('${livePane} .lv-frame').offsetHeight`;

/** 見る対象の枠の外側の高さを、縮めた倍率で割ったもの（枠がそこまで伸びているべき高さ）。 */
const liveRoomHeight = `(document.querySelector('${livePane} .lv-viewport').clientHeight / Number(getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1'))`;

/**
 * 書く欄を閉じた後の枠の高さ（R-PAGE-COMMENT）: 場所を置いて書く欄を開き、保存しても取り消しても、見る対象の枠の高さは
 * 欄を開く前と同じ。最初の保存で始め方の案内が消えたときも、枠は空いた高さまで伸びる。欄を開いている間に枠を並べ直す
 * きっかけ（道具を選び直す）があっても同じ。
 */
async function closingTheComposeBoxRestoresTheFrame(repository) {
  const dev = await startDevServer();
  const kemi = await startKemi(repository, await mkdtemp(join(tmpdir(), 'kemi-live-state-')), ['--live', `${dev.url}rich.html`]);
  const compose = '#live-compose';
  const sameHeight = (expected) => `Math.abs(${liveFrameHeight} - ${expected}) <= 1`;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden && ${visible('.lv-tools')} && ${visible('.lv-hint')}`);
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]' && ${visible(compose)}`);
    await chooseTool('arrow');
    await savePageCommentInThePage('the first comment');
    await waitFor(`!${visible('.lv-hint')} && !${visible(compose)}`);
    await new Promise((done) => setTimeout(done, 300));
    assert.ok(
      await evaluate(sameHeight(liveRoomHeight)),
      `the frame grows into the room the hint left: ${await evaluate(liveFrameHeight)} for ${await evaluate(liveRoomHeight)}`,
    );
    console.log('PASS 最初の保存で始め方の案内が消えると、見る対象の枠が空いた高さまで伸びる');

    await chooseTool('element');
    const before = await evaluate(liveFrameHeight);
    for (const close of ['save', 'cancel']) {
      await clickInPane(livePane, 150, 250);
      await waitFor(`${draftNumbers} === '[1]' && ${visible(compose)}`);
      await chooseTool('arrow');
      await chooseTool('element');
      if (close === 'save') {
        await savePageCommentInThePage('another comment');
      } else {
        await browser('click', `${compose} .lv-compose-cancel`);
      }
      await waitFor(`!${visible(compose)}`);
      await new Promise((done) => setTimeout(done, 300));
      assert.ok(await evaluate(sameHeight(before)), `after ${close}, the frame is as high as before the box opened: ${await evaluate(liveFrameHeight)} for ${before}`);
    }
    console.log('PASS 書く欄を開いて保存しても取り消しても、見る対象の枠の高さは欄を開く前と同じ');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 狭い画面の道具と「エージェントに渡す」（R-PAGE-VIEW の狭い画面、R-AGENT-STATE、R-NARROW）: 幅 390px のページの見方では、
 * 道具と「Hand to agent」が画面の下の 1 つの浮かぶツールバーにあり、全モード共通の浮かぶ「Hand to agent」は出ない。
 * `kemi wait` の前は、そのそばに `kemi wait <id>` を写す操作が出る。道具を切り替えて場所を置ける。コードの見方では
 * 全モード共通の浮かぶ「Hand to agent」だけ。幅 1280px ではどちらの浮かぶものも見えない。
 */
async function narrowToolsFloatWithHand(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '844');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden && ${visible('.lv-tools')}`);
    assert.equal(await evaluate(`${visible('.lv-toolbar .lv-hand')} || ${visible('#hand-float')}`), false, 'nothing floats on a wide screen');
    console.log('PASS 幅 1280px では、ページの見方の浮かぶ「Hand to agent」も全モード共通のものも見えない');

    await browser('set', 'viewport', '390', '844');
    await waitFor(`${visible(livePane)} && ${visible('.lv-toolbar .lv-hand')} && ${visible('.lv-tools')}`);
    const bar = JSON.parse(await evaluate(`JSON.stringify(document.querySelector('.lv-toolbar').getBoundingClientRect())`));
    const hand = JSON.parse(await evaluate(`JSON.stringify(document.querySelector('.lv-toolbar .lv-hand').getBoundingClientRect())`));
    assert.ok(bar.bottom > 844 - 80 && bar.bottom <= 844, `the tools float at the bottom: ${JSON.stringify(bar)}`);
    assert.ok(hand.top >= bar.top && hand.bottom <= bar.bottom, `Hand to agent is in the floating toolbar: ${JSON.stringify(hand)}`);
    assert.equal(await evaluate(visible('#hand-float')), false, 'the floating button of every mode is not shown twice');
    // kemi wait の前は、浮かぶ「Hand to agent」のそばに kemi wait <id> を写す操作が出る（R-AGENT-STATE）。
    const commands = `Array.from(document.querySelectorAll('.hand-command')).filter((item) => item.getClientRects().length > 0).map((item) => item.textContent)`;
    assert.deepEqual(JSON.parse(await evaluate(`JSON.stringify(${commands})`)), [`kemi wait ${kemi.id}`], 'the command to copy is shown beside the floating Hand to agent');
    assert.equal(await evaluate(`Array.from(document.querySelectorAll('.hand-copy')).filter((item) => item.getClientRects().length > 0).length`), 1, 'the command can be copied');
    await chooseTool('pen');
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    console.log('PASS 幅 390px のページの見方では、道具と「Hand to agent」が画面の下の浮かぶツールバーにあり、道具を切り替えて場所を置ける');

    await browser('click', '#live-compose .lv-compose-cancel');
    await browser('click', '.topbar .lv-view button[data-view="code"]');
    await waitFor(`document.body.dataset.liveView === 'code' && ${visible('#hand-float')} && !${visible('.lv-toolbar .lv-hand')}`);
    console.log('PASS 幅 390px のコードの見方では、全モード共通の浮かぶ「Hand to agent」だけが出る');
  } finally {
    await browser('set', 'viewport', '1280', '800');
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 狭い画面のページの見方（R-PAGE-VIEW の狭い画面、R-NARROW の `--live` の 1 段目のタブ、R-PAGE-MODE）: 幅 390px で開くと、
 * ページの見方の操作の帯が 1 行に収まり、その行にエージェントの状態があり、見たの進捗が見えず、重ねて透かす操作が無い。
 * 帯の「…」のメニューに比べる相手の選択・手で取る操作・枠に合わせると等倍・モックのファイルがある。上部バーの「…」の
 * メニューには表示の操作のうちテーマだけがある。1 段目のタブでコードの見方に切り替わる。
 */
async function narrowPageViewFitsOneRow(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  const band = '#live-band';
  try {
    await browser('set', 'viewport', '390', '844');
    await browser('open', kemi.url);
    await waitFor(`${visible(livePane)} && ${visible('.lv-side')} && ${visible(`${band} .lv-menu-button`)}`);
    const tops = await evaluate(`JSON.stringify(Array.from(document.querySelector('${band}').children).filter((child) => child.getClientRects().length > 0 && getComputedStyle(child).visibility !== 'hidden').map((child) => Math.round(child.getBoundingClientRect().top)))`);
    assert.ok(JSON.parse(tops).length >= 4 && new Set(JSON.parse(tops)).size === 1, `the band is one row: ${tops}`);
    await waitFor(`${visible(`${band} .lv-band-agent`)} && document.querySelector('${band} .lv-band-agent').dataset.kemiAgentState === document.querySelector('#agent-status').dataset.kemiAgentState`);
    assert.equal(await evaluate(visible('#progress')), false, 'the seen progress is not shown');
    assert.equal(await evaluate(`${visible('.lv-mode button[data-compare="overlay"]')} || ${visible('.lv-opacity')}`), false, 'no overlay on a narrow screen');
    await browser('click', `${band} .lv-menu-button`);
    await waitFor(`${visible('.lv-menu .lv-compare-select')} && ${visible('.lv-menu .lv-record-now')} && ${visible('.lv-menu .lv-zoom')} && ${visible('.lv-menu .lv-mock-input')}`);
    await browser('click', '.lv-menu .lv-zoom button[data-zoom="full"]');
    await waitFor(`${liveScale} === 1 && ${visible('.lv-menu')}`);
    await browser('press', 'Escape');
    await waitFor(`!${visible('.lv-menu')}`);
    console.log('PASS 幅 390px のページの見方では、操作の帯が 1 行でエージェントの状態があり、見たの進捗と重ねて透かす操作が無く、帯のメニューに比べる相手の選択・手で取る操作・枠に合わせると等倍・モックのファイルがある');

    await browser('click', '#btn-more');
    await waitFor(visible('#view-menu'));
    assert.deepEqual(
      JSON.parse(await evaluate(`JSON.stringify(Array.from(document.querySelectorAll('#view-menu .menu-item')).filter((item) => item.getClientRects().length > 0).map((item) => item.id))`)),
      ['menu-theme'],
    );
    await browser('press', 'Escape');
    console.log('PASS 幅 390px のページの見方では、上部バーの「…」のメニューに表示の操作のうちテーマだけがある');

    await waitFor(visible('.topbar .lv-view button[data-view="code"] .lv-modetab-short'));
    await browser('click', '.topbar .lv-view button[data-view="code"]');
    await waitFor(`document.body.dataset.liveView === 'code' && ${visible('#progress')}`);
    console.log('PASS 幅 390px で上部バーの 1 段目のタブを押すと、コードの見方に切り替わる');
  } finally {
    await browser('set', 'viewport', '1280', '800');
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 開いたまま狭い画面との境をまたぐ（R-PAGE-VIEW の狭い画面、R-PAGE-REF）: 等倍で見る対象だけのまま狭くすると、帯のメニューに
 * 比べる相手の選択が出る。比べる相手の 1 枚を見ている間は、見出しが比べる相手の名前（選択に出ている名前に含まれる）になり、
 * その 1 枚の見出しに倍率が出て、表示幅より狭い枠を横にスクロールできる。
 * 動いているページに戻すと見出しは見る対象だけのときと同じになり、広くすると比べる相手の選択がまた隠れる。並べたまま
 * 狭くしても、見出しは見る対象だけのときと同じになる。
 */
async function crossingTheNarrowWidthFollowsTheShownPage(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}tall.html`]);
  const heading = `document.querySelector('.lv-stage-name').textContent`;
  const refViewport = `document.querySelector('${refPane} .lv-viewport')`;
  const refScale = `Number(document.querySelector('${refPane} .lv-bar-label').textContent.match(/×([0-9.]+)/)?.[1] ?? NaN)`;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`${visible(livePane)} && document.querySelector('#live-stage').dataset.compare === 'now' && document.querySelector('${refPane}').dataset.reference === 'snapshot'`);
    const nowHeading = await evaluate(heading);
    await browser('click', '.lv-zoom button[data-zoom="full"]');
    await waitFor(`${liveScale} === 1`);
    await browser('set', 'viewport', '390', '844');
    await waitFor(`${visible('.lv-side')} && ${visible('#live-band .lv-menu-button')}`);
    // 幅をまたいだ直後は帯の並びが動くので、落ち着いてから押す。
    await new Promise((done) => setTimeout(done, 500));
    await browser('click', '#live-band .lv-menu-button');
    await waitFor(visible('.lv-menu .lv-compare-select'));
    await browser('press', 'Escape');
    await waitFor(`!${visible('.lv-menu')}`);
    console.log('PASS 等倍で見る対象だけのまま幅 390px にすると、帯のメニューに比べる相手の選択がある');

    await browser('click', '.lv-side button[data-side="ref"]');
    await waitFor(`${visible(refPane)} && !${visible(livePane)}`);
    await waitFor(`${heading} !== '' && ${heading} !== ${JSON.stringify(nowHeading)} && document.querySelector('.lv-compare-select').selectedOptions[0].textContent.includes(${heading})`);
    await waitFor(`${refScale} === 1`);
    // 横のスクロールはホイールが (0, 0) に落ちて試せないので、表示幅より狭い枠がはみ出た分をスクロールで見せるかを見る。
    await waitFor(`${refViewport}.scrollWidth > ${refViewport}.clientWidth && getComputedStyle(${refViewport}).overflowX !== 'hidden'`);
    console.log('PASS 幅 390px で比べる相手の 1 枚を見ると、見出しがその名前になり、その 1 枚の見出しに倍率が出て、等倍で横にスクロールできる');

    await browser('click', '.lv-side button[data-side="live"]');
    await waitFor(`${visible(livePane)} && ${heading} === ${JSON.stringify(nowHeading)}`);
    await browser('set', 'viewport', '1280', '900');
    await waitFor(`!${visible('.lv-side')} && !${visible('.lv-compare-select')} && !${visible(refPane)}`);
    console.log('PASS 動いているページに戻すと見出しが見る対象だけのときと同じになり、広い画面に戻すと見る対象だけのまま比べる相手の選択が隠れる');

    await chooseCompare('side');
    await browser('set', 'viewport', '390', '844');
    await waitFor(`${visible('.lv-side')} && ${visible(livePane)} && ${heading} === ${JSON.stringify(nowHeading)}`);
    console.log('PASS 並べたまま幅 390px にすると、見出しが見る対象だけのときと同じになる');
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

/** 変化の一覧の行（主な変化）。 */
const mainRows = `Array.from(${changeList}?.querySelectorAll('.lv-change-main .lv-change') ?? [])`;

/** 行の中の要素の背景色（色の見本を見分ける）。 */
const backgroundsIn = (row) => `Array.from(${row}.querySelectorAll('*')).map((element) => getComputedStyle(element).backgroundColor)`;

/**
 * 変化の一覧の行（R-PAGE-DIFF、R-PAGE-VIEW の一覧の見出し）: ボタンの文字色・背景色・枠を同時に変えると、一覧ではその
 * ボタンが 1 行になり、色の前後が色の見本で出る。その行を押すと、見る対象がそのボタンまでスクロールし、ボタンの印が光る。
 * 見る対象だけの見比べ方で、一覧の見出しの名前が、並べる見比べ方に切り替えたときの比べる相手の選択の文字に含まれる。
 * 兄弟の途中に要素を足しても `html` と `body` はずれただけに出ず、`body` の背景色を変えると `body` が主な変化に出る。
 */
async function changeRowsAreElementsThatLeadToThePage(repository) {
  const dev = await startDevServer();
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, await mkdtemp(join(tmpdir(), 'kemi-live-state-')), ['--live', `${dev.url}changing.html`]);
  const frame = `${livePane} .lv-frame`;
  const wheel = (y) => evaluate(`(() => { const layer = document.querySelector('${livePane} .lv-capture'); const r = layer.getBoundingClientRect(); return layer.dispatchEvent(new WheelEvent('wheel', { deltaY: ${y}, clientX: r.x + 100, clientY: r.y + 100, bubbles: true, cancelable: true })); })()`);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/changing.html')`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${frame}').style.width === '390px' && document.querySelector('${livePane} .lv-notice').hidden && document.querySelector('${refPane}').dataset.reference === 'none'`);
    await browser('click', '.lv-band .lv-record-now');
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.css'), `${changingCss('rgb(214, 69, 69)')}.buy { color: rgb(20, 20, 20); border: 4px solid rgb(0, 150, 0); }\n`);
    await waitFor(`${changeList}?.dataset.main === '1'`);
    await new Promise((done) => setTimeout(done, 500));
    assert.equal(await evaluate(`${mainRows}.length`), 1, await evaluate(`JSON.stringify(${mainRows}.map((row) => row.textContent))`));
    const row = `${mainRows}[0]`;
    assert.match(await evaluate(`${row}.textContent`), /button[\s\S]*Buy/);
    const backgrounds = JSON.parse(await evaluate(`JSON.stringify(${backgroundsIn(row)})`));
    const from = backgrounds.indexOf('rgb(49, 89, 214)');
    assert.ok(from !== -1 && backgrounds.indexOf('rgb(214, 69, 69)', from + 1) > from, `the background colors before and after are shown as colors: ${JSON.stringify(backgrounds)}`);
    console.log('PASS ボタンの文字色・背景色・枠を同時に変えると、一覧ではそのボタンが 1 行で、色の前後が色の見本で出る');

    await wheel(1500);
    const around = [0, 100, 220, 200];
    await waitForPixels(frame, shots, 'scrolled-away', around, isRed, false);
    await browser('click', '#page-tree .lv-page[data-current="true"] .lv-changes .lv-change-main .lv-change');
    await waitForPixels(frame, shots, 'button-shown', around, isGlow);
    await waitForPixels(frame, shots, 'button-shown-mark', around, isRed);
    console.log('PASS その行を押すと、見る対象がそのボタンまでスクロールし、ボタンの印が光る');

    const name = await evaluate(`${changeList}.querySelector('.lv-changes-vs').textContent`);
    assert.ok(name.includes('Recorded 1'), name);
    await chooseCompare('side');
    const chosen = await evaluate(`document.querySelector('.lv-compare-select').selectedOptions[0].textContent`);
    assert.ok(chosen.includes(name), `the heading name ${JSON.stringify(name)} is in the choice ${JSON.stringify(chosen)}`);
    await chooseAuto();
    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(showsSnapshot('Start'));
    await chooseCompare('now');
    await waitFor(`${changeList}?.querySelector('.lv-changes-vs')?.textContent.includes('Start')`);
    const auto = await evaluate(`${changeList}.querySelector('.lv-changes-vs').textContent`);
    await chooseCompare('side');
    assert.ok((await evaluate(`document.querySelector('.lv-compare-select').selectedOptions[0].textContent`)).includes(auto), auto);
    await chooseCompare('now');
    console.log('PASS 見る対象だけの見比べ方で、一覧の見出しの名前が、並べたときの比べる相手の選択の文字に含まれる（自動のときも）');

    await writeFile(join(dev.dir, 'changing.css'), changingCss());
    await writeFile(join(dev.dir, 'changing.html'), changingPage(['one', 'two', 'inserted', 'three', 'four']));
    await waitFor(`${changeList}?.dataset.main === '1' && Number(${changeList}.dataset.shifted) > 0`);
    await evaluate(`(() => { const details = ${changeList}.querySelector('details'); details.open = true; return true; })()`);
    await waitFor(`${changeList}.querySelectorAll('.lv-change-shifted .lv-change').length > 0`);
    const shiftedTags = JSON.parse(await evaluate(`JSON.stringify(Array.from(${changeList}.querySelectorAll('.lv-change-shifted .lv-change')).map((item) => item.dataset.tag))`));
    assert.ok(!shiftedTags.includes('html') && !shiftedTags.includes('body'), JSON.stringify(shiftedTags));
    assert.equal(Number(await evaluate(`${changeList}.dataset.shifted`)), shiftedTags.length);
    await writeFile(join(dev.dir, 'changing.html'), changingPage());
    await writeFile(join(dev.dir, 'changing.css'), `${changingCss()}body { background: rgb(255, 250, 230); }\n`);
    await waitFor(`${mainRows}.some((item) => item.dataset.tag === 'body')`);
    console.log('PASS 兄弟の途中に要素を足しても html と body はずれただけに出ず、body の背景色を変えると body が主な変化に出る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/** /changing.html の末尾の余白の下に、下の方の要素（#low）を足したもの。 */
const changingWithLow = () => changingPage().replace('<div class="tail"></div>', '<div class="tail"></div><p id="low" style="margin:0;height:60px">Low element</p>');

/**
 * 消えた要素の行（R-PAGE-VIEW、R-PAGE-DIFF、R-PAGE-SNAPSHOT）: スナップショットにあって今のページから消した、下の方の要素が
 * 一覧に出る。見る対象だけのときにその行を押すと並べる見比べ方になり、比べる相手の枠の中でその要素が見える位置にあって
 * 光っている。幅 390px では比べる相手の 1 枚に切り替わって同じになる。重ねて透かすときは見比べ方を変えず、その要素が
 * 見える位置で光り、見る対象と比べる相手の位置がそろったまま。
 */
async function removedRowsLeadToTheSnapshot(repository) {
  const dev = await startDevServer();
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, await mkdtemp(join(tmpdir(), 'kemi-live-state-')), ['--live', `${dev.url}changing.html`]);
  const refView = `${refPane} .lv-viewport`;
  const removedRow = `${changeList}?.querySelector('.lv-change-main .lv-change[data-kind="removed"]')`;
  // 枠の上の方（ページの先頭にある青いボタンが写る所）を除いた範囲。ボタンの青は光の色に近い。
  const belowTheButton = [0, 250, 100000, 100000];
  const pressRemovedRow = () => evaluate(`${removedRow}.querySelector('button').click(); true`);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/changing.html')`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await writeFile(join(dev.dir, 'changing.html'), changingWithLow());
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px' && document.querySelector('${livePane} .lv-notice').hidden`);
    await new Promise((done) => setTimeout(done, 1000));
    await browser('click', '.lv-band .lv-record-now');
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.html'), changingPage());
    await waitFor(`${removedRow} !== null && ${removedRow}.textContent.includes('Low element')`);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare`), 'now');
    // 光は数秒で消えるので、押したらすぐに撮り始める。
    await pressRemovedRow();
    await waitForPixels(refView, shots, 'removed-side', belowTheButton, isGlow);
    await waitForPixels(refView, shots, 'removed-side-mark', belowTheButton, isRed);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare === 'side' && ${visible(refPane)}`), true);
    console.log('PASS 下の方の消えた要素が一覧に出て、見る対象だけのときにその行を押すと並べる見比べ方になり、比べる相手の側でその要素が見えて光る');

    // ずらした形の間も、比べる相手の上のホイールで動かせる。見比べ方を変えると、元の形（枠の中で自分でスクロールする形）に戻る。
    const refFrameNow = `document.querySelector('${refPane} .lv-frame:not([hidden])')`;
    const shifted = `new DOMMatrix(getComputedStyle(${refFrameNow}).transform).m42`;
    const down = await evaluate(shifted);
    assert.ok(down < 0, `the snapshot is moved up to the element: ${down}`);
    await evaluate(`(() => { const layer = document.querySelector('${refView} .lv-ref-wheel'); const r = layer.getBoundingClientRect(); return layer.dispatchEvent(new WheelEvent('wheel', { deltaY: -300, clientX: r.x + 50, clientY: r.y + 50, bubbles: true, cancelable: true })); })()`);
    await waitFor(`${shifted} > ${down}`);
    await chooseCompare('now');
    await chooseCompare('side');
    await waitFor(`${shifted} === 0 && Math.abs(${refFrameNow}.getBoundingClientRect().height - document.querySelector('${refView}').clientHeight) <= 1`);
    console.log('PASS ずらした形の間も比べる相手をホイールで動かせ、見比べ方を変えると元の形に戻る');

    // 読み込み直すと表示幅は既定に戻る。取った幅に合わせると、自動はその幅で取ったものを選ぶ。
    const at390 = `document.querySelector('.lv-widths button[data-width="390"]').click(); true`;
    await browser('set', 'viewport', '390', '844');
    await browser('open', kemi.url);
    await waitFor(visible(livePane));
    await evaluate(at390);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${removedRow} !== null`);
    await pressRemovedRow();
    await waitForPixels(refView, shots, 'removed-narrow', belowTheButton, isGlow);
    await waitForPixels(refView, shots, 'removed-narrow-mark', belowTheButton, isRed);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.side === 'ref' && ${visible(refPane)}`), true);
    console.log('PASS 幅 390px で同じ行を押すと、比べる相手の 1 枚に切り替わり、その要素が見えて光る');

    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(visible(livePane));
    await evaluate(at390);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${removedRow} !== null`);
    await chooseCompare('overlay');
    await waitFor(`${removedRow} !== null`);
    await pressRemovedRow();
    await waitForPixels(refView, shots, 'removed-overlay', belowTheButton, isGlow);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare`), 'overlay');
    // 重ねた比べる相手は、見る対象のスクロールの分だけ外側でずらして描く（そろったまま）。見る対象が下へスクロールしている。
    const shift = await evaluate(`new DOMMatrix(getComputedStyle(document.querySelector('${refPane} .lv-frame:not([hidden])')).transform).m42`);
    assert.ok(shift < -1000, `the overlaid snapshot follows the page scrolled down to the element: ${shift}`);
    console.log('PASS 重ねて透かすときに同じ行を押すと、見比べ方は変わらず、そろったままその要素の位置までスクロールして光る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
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
    await chooseCompare('side');
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
    assert.equal(recolored[0].kind, 'main', JSON.stringify(recolored));
    // 色の前後は色の見本で出る（行の中の要素の背景色に、前の色、続いて今の色がある）。
    const swatches = JSON.parse(await evaluate(`JSON.stringify(${backgroundsIn(`${mainRows}[0]`)})`));
    const was = swatches.indexOf('rgb(49, 89, 214)');
    assert.ok(was !== -1 && swatches.indexOf('rgb(214, 69, 69)', was + 1) > was, JSON.stringify(swatches));
    console.log('PASS ボタンの背景色を HMR の CSS の差し替えで変えると、そのボタンが主な変化に入り、色の前後が出る');

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
    // 狭い画面では手で取る操作は帯の「…」のメニューの中。ページのツリーの引き出しが開いたままなので、押すのはスクリプトで。
    await evaluate(`document.querySelector('#live-band .lv-menu-button').click(); true`);
    await waitFor(`!${visible(`${livePane} .lv-frame`)} && ${visible('.lv-menu .lv-record-now')}`);
    await evaluate(`document.querySelector('.lv-menu .lv-record-now').click(); document.querySelector('.lv-menu').hidePopover(); true`);
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
    await chooseCompare('side');
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
    await browser('click', '.lv-band .lv-record-now');
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

    await browser('click', '.lv-band .lv-record-now');
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
    await chooseCompare('side');
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
    await chooseCompare('side');
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
    await chooseCompare('side');
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
    await chooseCompare('side');
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
    assert.equal(recolored[0].kind, 'main', JSON.stringify(recolored));
    const swatches = JSON.parse(await evaluate(`JSON.stringify(${backgroundsIn(`${mainRows}[0]`)})`));
    const was = swatches.indexOf('rgb(49, 89, 214)');
    assert.ok(was !== -1 && swatches.indexOf('rgb(214, 69, 69)', was + 1) > was, JSON.stringify(swatches));
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
    await chooseCompare('side');
    await waitFor(`${showsSnapshot('Start')} && ${changeList}?.dataset.main === '0'`);
    await browser('click', '.lv-widths button[data-width="768"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 1')} && ${changeList}?.dataset.main === '0'`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitFor(`${showsSnapshot('Recorded 2')} && ${changeList}?.dataset.main === '0'`);
    // 390 で取ったものに切り替わっているので、768 では記録されていない。768 で取ったものを選び直す。
    await browser('click', '.lv-widths button[data-width="768"]');
    await waitFor(notRecorded);
    await chooseReference('Recorded 1');
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
 * ページへのコメントの場所（R-PAGE-COMMENT、R-PAGE-REF、R-SUBMIT）: 要素・矢印・ペンで場所を置き、本文に `#1 と #3 と 3` と
 * 書いて 2 番目を消すと、番号が 1 と 2 に詰まり本文が `#1 と #2 と 3` になり、保存して渡すと `kemi wait` の JSON も同じ。
 * 矢印の先の要素は先端の位置の要素。並べた比べる相手の側では場所が増えず、重ねて透かしている間に要素を選ぶと見る対象の
 * 要素が場所になる。消した場所を指す `#1` が本文にある間は保存できず、本文から消すとできる。一覧の番号を押すと本文に
 * `#n` が入る。保存した後も画面は例外を出さない。
 */
async function pageCommentPlacesArePutAndSaved(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await evaluate(`window.__kemiErrors = []; addEventListener('error', (event) => window.__kemiErrors.push(String(event.message))); addEventListener('unhandledrejection', (event) => window.__kemiErrors.push(String(event.reason))); true`);
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await new Promise((done) => setTimeout(done, 500));
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューで押せるので、先に一度渡しておく。
    await post(kemi.url, 'api/message', { body: 'let me show you' });
    await handInThePage(kemi, repository, state);

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

    const body = `document.querySelector('#live-compose .lv-compose-body').value`;
    await browser('fill', '#live-compose .lv-compose-body', '#1 と #3 と 3');
    await browser('click', '#live-compose .lv-place[data-n="2"] .lv-place-remove');
    await waitFor(`${draftNumbers} === '[1,2]'`);
    assert.equal(await evaluate(body), '#1 と #2 と 3');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${draftNumbers} === '[]'`);
    const answer = await handAndWait(kemi, repository, state);
    const comment = answer.events.flatMap((event) => event.comments ?? []).find((change) => change.comment.page)?.comment;
    assert.ok(comment, `a page comment is handed: ${JSON.stringify(answer)}`);
    assert.equal(comment.body, '#1 と #2 と 3');
    assert.deepEqual(comment.page.places.map((place) => [place.n, place.kind]), [[1, 'element'], [2, 'arrow']]);
    assert.equal(comment.page.url, '/rich.html');
    assert.equal(comment.page.width, 390);
    assert.equal(comment.page.places[0].elements[0].selector, '#button');
    console.log('PASS 本文に #1 と #3 と 3 と書いて 2 番目の場所を消すと、番号が 1 と 2 に詰まって本文が #1 と #2 と 3 になり、渡した JSON も同じ');
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

    // 場所が 0 になると、それだけで保存は押せないので、場所を 2 つにしておく。
    await chooseTool('pen');
    await dragInPane(livePane, [[20, 90], [260, 90], [260, 140], [20, 140], [20, 92]]);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    const save = `document.querySelector('#live-compose .lv-compose-save')`;
    await browser('fill', '#live-compose .lv-compose-body', 'see #1');
    await waitFor(`!${save}.disabled`);
    await browser('click', '#live-compose .lv-place[data-n="1"] .lv-place-remove');
    await waitFor(`${draftNumbers} === '[1]' && ${visible('#live-compose .lv-compose-stray')} && ${save}.disabled`);
    assert.equal(await evaluate(body), 'see #1');
    await browser('fill', '#live-compose .lv-compose-body', 'see');
    await waitFor(`!${save}.disabled && !${visible('#live-compose .lv-compose-stray')}`);
    console.log('PASS 消した場所を指す #1 が本文にあると、その箇所が示されて保存を押せず、本文から消すと押せる');

    await evaluate(`(() => { const area = document.querySelector('#live-compose .lv-compose-body'); area.setSelectionRange(3, 3); return true; })()`);
    await browser('click', '#live-compose .lv-place[data-n="1"] .lv-place-n');
    await waitFor(`${body} === 'see#1'`);
    console.log('PASS 場所の一覧の番号を押すと、本文の入力位置に #n が入る');

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

/**
 * ページの余白の上の場所（R-PAGE-COMMENT、R-SUBMIT）: 要素の道具で `html` か `body` にしか当たらない余白を押しても場所は
 * 増えない。余白を指す矢印と余白だけを囲むペンは、要素の無い「範囲だけ」の場所になり（一覧に要素が出ない）、渡した
 * `kemi wait` の JSON で `elements` が `[]` で `points` を持ち、場所ごとの画像は無くコメントの画像が 1 つある。画像は
 * 文書全体ではなく場所の周りを写す。
 */
async function placesOnTheBackgroundAreAreasOnly(repository) {
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
    await post(kemi.url, 'api/message', { body: 'let me show you' });
    await handInThePage(kemi, repository, state);
    await keepSavedImage();
    // ボタン（下端 300）と 2 段の並びより下は、どの要素も描かれていない余白。
    await chooseTool('element');
    await clickInPane(livePane, 350, 450);
    await new Promise((done) => setTimeout(done, 800));
    assert.equal(await evaluate(draftNumbers), '[]', 'a click on the background adds no place');
    console.log('PASS 要素の道具でページの余白を押しても場所が増えない');

    await chooseTool('arrow');
    await dragInPane(livePane, [[200, 360], [300, 420], [350, 450]]);
    await waitFor(`${draftNumbers} === '[1]'`);
    await chooseTool('pen');
    await dragInPane(livePane, [[320, 360], [380, 360], [380, 500], [320, 500], [320, 362]]);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    const rows = JSON.parse(await evaluate(`JSON.stringify(Array.from(document.querySelectorAll('#live-compose .lv-place')).map((row) => row.dataset.selector ?? null))`));
    assert.deepEqual(rows, [null, null]);
    console.log('PASS 余白を指す矢印と余白だけを囲むペンの場所の行には、要素が出ず範囲だけと出る');

    await savePageCommentInThePage('the empty space');
    const answer = await handAndWait(kemi, repository, state);
    const comment = answer.events.flatMap((event) => event.comments ?? []).find((change) => change.comment.page)?.comment;
    assert.ok(comment, `a page comment is handed: ${JSON.stringify(answer)}`);
    for (const place of comment.page.places) {
      assert.deepEqual(place.elements, [], `place ${place.n} (${place.kind}) names no element`);
      assert.ok(place.points.length > 0, `place ${place.n} (${place.kind}) has its points`);
      assert.deepEqual(Object.keys(place).sort(), ['elements', 'kind', 'n', 'points'], `place ${place.n} has no image of its own`);
    }
    assert.ok(typeof comment.page.image === 'string' && comment.page.image !== '', `the comment has one image: ${comment.page.image}`);
    console.log('PASS 余白を指す矢印と余白だけを囲むペンは、kemi wait の JSON で elements が [] で points を持ち、画像はコメントに 1 つ');
    const saved = await evaluate('window.__kemiSavedImage ?? null');
    assert.notEqual(saved, null, 'the image is made');
    const image = decodePng(Buffer.from(saved, 'base64'));
    const points = comment.page.places.flatMap((place) => place.points);
    const shown = `${image.width}x${image.height}, places ${JSON.stringify(points)}`;
    assert.ok(image.width > 0 && image.width < 390, `the image is narrower than the page: ${shown}`);
    console.log('PASS 余白に置いた場所の画像は、文書全体ではなく場所の周りを写す');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * ページの上の印（R-PAGE-COMMENT）: c1 を保存してから新しいコメントを書き始めると、c1 の場所には番号が出ず、書きかけの
 * 場所にだけ番号が出る。要素の道具のまま c1 の印にポインタを乗せると、レビュー画面に c1 の短い名前が出る。c1 のスレッドを
 * 開くと c1 の場所に番号が出る。番号の札と描き込みはページの中の紫で見る（印の層は閉じた shadow root の中で、別の
 * オリジンの枠なので要素は読めない）。
 */
async function savedCommentsAreQuietMarks(repository) {
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  // ボタン（左上 (0, 200) の 300×100）の左上の番号の札の辺りと、2 段目の入力欄（左上 (12, 96) 付近）の辺り。
  const buttonCorner = [0, 185, 40, 230];
  const fieldCorner = [0, 80, 60, 125];
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
    await savePageCommentInThePage('The button label is too long\nsecond line');
    const [c1] = (await reviewJson(kemi)).comments;
    await clickInPane(livePane, 40, 105);
    await waitFor(`${draftNumbers} === '[1]' && document.querySelector('#live-compose .lv-place[data-n="1"]').dataset.selector === '#field'`);
    await new Promise((done) => setTimeout(done, 500));
    const drafting = await shot(`${livePane} .lv-frame`, shots, 'saved-and-draft');
    assert.equal(countPixels(drafting, buttonCorner, isPlaceInk), 0, `the saved comment shows no number: ${join(shots, 'saved-and-draft.png')}`);
    assert.ok(countPixels(drafting, fieldCorner, isPlaceInk) > 0, `the draft place shows its number: ${join(shots, 'saved-and-draft.png')}`);
    console.log('PASS c1 を保存してから新しいコメントを書き始めると、c1 の場所に番号が出ず、書きかけの場所にだけ番号が出る');

    // 要素の道具の層にポインタの動きを送る（agent-browser のマウスは層の上の動きを確かに届けられないため）。
    const scale = Number(await evaluate(`getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1'`));
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame').getBoundingClientRect(); const layer = document.querySelector('${livePane} .lv-capture'); return layer.dispatchEvent(new PointerEvent('pointermove', { pointerId: 7, clientX: frame.x + 300 * ${scale}, clientY: frame.y + 200 * ${scale}, bubbles: true })); })()`);
    await waitFor(`${visible(`${livePane} .lv-saved-tip`)} && document.querySelector('${livePane} .lv-saved-tip').textContent.includes('The button label is too long')`);
    assert.equal(await evaluate(draftNumbers), '[1]', 'moving the pointer puts no place');
    console.log('PASS 要素の道具のまま c1 の印にポインタを乗せると、レビュー画面に c1 の短い名前が出る');

    await browser('click', '#cv-rail');
    await browser('click', `.cv-card[data-id="${c1.id}"]`);
    await waitFor(`document.querySelector('#conversation').dataset.open === 'true'`);
    const opened = await evaluate(`getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1'`);
    const corner = buttonCorner.map((value) => Math.round(value * Number(opened)));
    await waitForPixels(`${livePane} .lv-frame`, shots, 'saved-focused', corner, isPlaceInk);
    console.log('PASS c1 のスレッドを開くと、c1 の場所に番号が出る');
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
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューでだけ押せるので、先に一度渡しておく。
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
 * 見方か、狭い画面で比べる相手の側に切り替えても、動いているページは並べたままなので、画像は作られ、場所の周り
 * （場所の要素の矩形に上下の余白を足した高さ）を写す。場所の一部を切り落とした画像にも、null にもならない。
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
      assert.notEqual(saved, null, `the image is made after switching the ${away}`);
      const image = decodePng(Buffer.from(saved, 'base64'));
      assert.equal(image.height, Math.round(rect.h) + 96, `the image keeps the whole place after switching the ${away}: ${image.width}x${image.height}, place ${JSON.stringify(rect)}`);
      console.log(`PASS 保存している間に${away === 'view' ? 'コードの見方' : '比べる相手の側'}へ切り替えても、画像は場所の周りを写す`);
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
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューでだけ押せるので、先に一度渡しておく。
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
    await chooseCompare('side');
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

/** 場所や要素を光らせる光（画面モックの --ai の青）。 */
const isGlow = ([r, g, b]) => r < 95 && g < 110 && b > 140 && b - r > 60;

/**
 * 枠の画像のうち、ページの x より右（ページの CSS ピクセル）。/tall.html の帯の文字（左端の「Band n」）は、帯の色に
 * 混ざって光や描き込みの色に近い画素を作るので、それより右だけを見る。
 */
async function rightOf(x) {
  const scale = Number(await evaluate(`getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1'`));
  return [Math.round(x * scale), 0, 100000, 100000];
}

/** 要素の真ん中へ（レビュー画面の文書の中の）ポインタを動かす。 */
async function pointAt(selector) {
  const { x, y } = await evaluate(`(() => { const r = document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
  await browser('mouse', 'move', String(Math.round(x)), String(Math.round(y)));
}

/**
 * 場所を光らせる（R-PAGE-COMMENT）: 書きかけの場所の一覧の行に乗せている間はその場所が光り、離れると消える。下の方に置いた
 * 場所の行を押すと、見る対象がその場所までスクロールして光る。保存したコメントのスレッドの「ページで見る」を押すと、
 * 下の方のそのコメントの場所までスクロールして光る。別の表示幅で付けたコメントでも、幅が切り替わってから同じになる。
 */
async function placesGlowFromTheListAndTheThread(repository) {
  const dev = await startDevServer();
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const kemi = await startKemi(repository, await mkdtemp(join(tmpdir(), 'kemi-live-state-')), ['--live', `${dev.url}tall.html`]);
  const frame = `${livePane} .lv-frame`;
  const wheel = (y) => evaluate(`(() => { const layer = document.querySelector('${livePane} .lv-capture'); const r = layer.getBoundingClientRect(); return layer.dispatchEvent(new WheelEvent('wheel', { deltaY: ${y}, clientX: r.x + 100, clientY: r.y + 100, bubbles: true, cancelable: true })); })()`);
  // 下の方（文書の y 2500 付近、帯 9 の中）を指す、帯の文字から離れた矢印。保存したコメントの場所に使う。
  const arrowLow = (width, left) => ({ url: '/tall.html', width, places: [{ n: 1, kind: 'arrow', points: [{ x: left, y: 2450 }, { x: left + 130, y: 2560 }], elements: [] }] });
  try {
    const c1 = await post(kemi.url, 'api/comment', { op: 'add_page', page: arrowLow(1280, 700), body: 'low at 1280' });
    const c2 = await post(kemi.url, 'api/comment', { op: 'add_page', page: arrowLow(390, 200), body: 'low at 390' });
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden && ${visible('.lv-tools')}`);
    await new Promise((done) => setTimeout(done, 500));
    await wheel(6000);
    await new Promise((done) => setTimeout(done, 500));
    // 帯 8 を場所にする（帯は幅いっぱいなので、描き込みと光の上下の辺が右側に写る）。
    await clickInPane(livePane, 600, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await new Promise((done) => setTimeout(done, 500));
    const right = await rightOf(300);
    await waitForPixels(frame, shots, 'draft-before', right, isGlow, false);
    const row = '#live-compose .lv-place[data-n="1"]';
    await pointAt(row);
    await waitForPixels(frame, shots, 'draft-hovered', right, isGlow);
    await pointAt('#live-compose .lv-compose-head');
    await waitForPixels(frame, shots, 'draft-left', right, isGlow, false);
    console.log('PASS 書きかけの場所の一覧の行に乗せるとその場所が光り、離れると光が消える');

    await wheel(-6000);
    await waitForPixels(frame, shots, 'draft-top', right, isPlaceInk, false);
    await browser('click', `${row} .lv-place-what`);
    await waitForPixels(frame, shots, 'draft-shown', right, isGlow);
    await waitForPixels(frame, shots, 'draft-shown-place', right, isPlaceInk);
    console.log('PASS 下の方に置いた場所の一覧の行を押すと、見る対象がその場所までスクロールして光る');

    await browser('click', '#live-compose .lv-compose-cancel');
    await wheel(-6000);
    await browser('click', '#cv-rail');
    await browser('click', `.cv-card[data-id="${c1.id}"]`);
    await waitFor(`document.querySelector('#cv-thread .cv-go')?.textContent === 'Show on page'`);
    const opened = await rightOf(300);
    await waitForPixels(frame, shots, 'saved-top', opened, isPlaceInk, false);
    await browser('click', '#cv-thread .cv-go');
    await waitForPixels(frame, shots, 'saved-shown', opened, isGlow);
    await waitForPixels(frame, shots, 'saved-shown-place', opened, isPlaceInk);
    console.log('PASS ページの上端からスレッドの「ページで見る」を押すと、下の方のそのコメントの場所までスクロールして光る');

    await waitForPixels(frame, shots, 'saved-faded', opened, isGlow, false);
    await wheel(-6000);
    await browser('click', '#cv-thread .cv-back');
    await browser('click', `.cv-card[data-id="${c2.id}"]`);
    await waitFor(`document.querySelector('#cv-thread .cv-page-width')?.textContent === '390px'`);
    await browser('click', '#cv-thread .cv-page-width');
    await waitFor(`document.querySelector('${frame}').style.width === '390px'`);
    const narrow = await rightOf(150);
    await waitForPixels(frame, shots, 'other-width-shown', narrow, isGlow);
    await waitForPixels(frame, shots, 'other-width-shown-place', narrow, isPlaceInk);
    console.log('PASS 別の表示幅で付けたコメントの「ページで見る」でも、幅が切り替わってからその場所までスクロールして光る');
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
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューでだけ押せるので、先に一度渡しておく。
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

/** ページと同じオリジンから API を読む。 */
async function getJson(url, path) {
  const response = await fetch(new URL(path, url));
  assert.equal(response.status, 200, path);
  return response.json();
}

/**
 * 保留と復元（R-PAGE-SESSION、R-PAGE-REF、R-PAGE-MOCK）: スナップショットとモックの割り当てが戻り、開始時の
 * スナップショットが既定の比べる相手のまま取り直されない。消したモックは読めない旨が出る。
 */
async function snapshotsAndMocksComeBackAfterResuming(repository) {
  await mkdir(join(repository, 'mocks'), { recursive: true });
  const mockFile = join(repository, 'mocks', 'resumed.html');
  await writeFile(mockFile, '<!doctype html><p>mock to be deleted</p>\n');
  const dev = await startDevServer();
  const state = await mkdtemp(join(tmpdir(), 'kemi-live-state-'));
  const options = `Array.from(document.querySelectorAll('.lv-compare-select option')).map((option) => option.textContent)`;
  let kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  let start;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitFor(showsSnapshot('Start'));
    await browser('click', '.lv-band .lv-record-now');
    await waitFor(`${options}.some((label) => label.startsWith('Recorded 1'))`);
    const [listed] = (await getJson(kemi.url, 'api/snapshots')).snapshots.filter((snapshot) => snapshot.kind === 'start');
    start = await getJson(kemi.url, `api/snapshot/${listed.id}`);
    await addPageComment(kemi, '/rich.html', 1280, 'keep this review');
    await post(kemi.url, 'api/mock', { page: '/other.html', path: 'mocks/resumed.html' });
  } finally {
    await stop(kemi);
  }
  await rm(mockFile);

  kemi = await startKemi(repository, state, ['--resume', kemi.id]);
  try {
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitFor(`${showsSnapshot('Start')} && ${options}.some((label) => label.startsWith('Recorded 1'))`);
    await waitFor(`document.querySelector('#page-tree .lv-page[data-page="/rich.html"] .lv-comment-count')?.textContent === '1'`);
    // 動いているページが読み込まれた後も、開始時のものを取り直さない。
    await new Promise((done) => setTimeout(done, 1500));
    const starts = (await getJson(kemi.url, 'api/snapshots')).snapshots.filter((snapshot) => snapshot.kind === 'start');
    assert.equal(starts.length, 1, JSON.stringify(starts));
    const restored = await getJson(kemi.url, `api/snapshot/${starts[0].id}`);
    assert.equal(restored.id, start.id);
    assert.equal(restored.html, start.html);
    console.log('PASS コメントと手で取ったスナップショットのあるレビューを保留して復元すると、開始時と手で取ったものが選択に出て開始時が既定になり、開始時のものは同じ id と HTML で 1 つだけ');

    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${kemi.live.replace(/\/rich\.html$/, '')}/other.html`)}; true`);
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock' && ${visible(`${refPane} .lv-empty`)} && !${visible(`${refPane} .lv-empty .lv-record`)}`);
    console.log('PASS モックを割り当てて保留し、モックのファイルを消してから復元すると、比べる相手の場所に読めない旨が出る');
  } finally {
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
  await modeTabsSwitchTheTopbar(repository);
  await outsideGitFilePages();
  await otherReviewsLoadNoPageFiles(repository);
  await snapshotsAreTakenShownAndChosen(repository);
  await snapshotsCarryTheirResources(repository);
  await commentImagesLookLikeThePage(repository);
  await commentImagesKeepTheLayoutOfTheViewport(repository);
  await mocksAreAssignedShownAndKeptApart(repository);
  await snapshotsSendNoTokenToExternalImages(repository);
  await overlayFollowsTheScrollAndTheOpacity(repository);
  await compareModesScaleAndReload(repository);
  await referenceNamesHeadingsAndNotices(repository);
  await recordingNowSwitchesTheReference(repository);
  await sideBySidePagesShareTheirTop(repository);
  await changesShowWithThePageAlone(repository);
  await toolsAndTheHintStartTheFirstComment(repository);
  await closingTheComposeBoxRestoresTheFrame(repository);
  await narrowPageViewFitsOneRow(repository);
  await narrowToolsFloatWithHand(repository);
  await crossingTheNarrowWidthFollowsTheShownPage(repository);
  await changeListFollowsThePage(repository);
  await changeRowsAreElementsThatLeadToThePage(repository);
  await removedRowsLeadToTheSnapshot(repository);
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
  await placesOnTheBackgroundAreAreasOnly(repository);
  await savedCommentsAreQuietMarks(repository);
  await draftPlacesStayAtTheirWidth(repository);
  await latePlacesKeepTheirWidth(repository);
  await draftsStayWhileSaving(repository);
  await narrowReferenceSideSavesTheImage(repository);
  await switchingWhileSavingKeepsTheImageAroundThePlace(repository);
  await savingAndHandingKeepThePageLaidOut(repository);
  await movingWhileSavingMakesNoImageOfAnotherPage(repository);
  await placesMakeNoChange(repository);
  await savedPageCommentsAreListedShownAndSwitched(repository);
  await placesGlowFromTheListAndTheThread(repository);
  await handedPageCommentsReachWaitAndSubmit(repository);
  await snapshotsAndMocksComeBackAfterResuming(repository);
} finally {
  await run('agent-browser', ['--session', session, 'close']).catch(() => {});
}
