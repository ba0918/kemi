// `--live` のブラウザ検査（scripts/test-live.mjs）の場面が共通に使う補助: kemi と試験用の開発サーバの起動、
// 画面の操作と待ち、比べる相手と変化の一覧の読み方、画素の見方。
// ここの補助が書くグローバルを読む式は、使う領域が 1 つだけでも、書く補助の隣に置く（グローバルの名前を直すときに
// 1 つのファイルで済む）。

import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';

import { browserSession, visible } from '../lib/browser.mjs';
import { gitIn, temporaryDirectory } from '../lib/fixture.mjs';
import { getJson, isolatedEnvironment, launchKemi, postJson, runKemi, stopKemi } from '../lib/kemi.mjs';
import { countPixels, decodePng } from '../lib/png.mjs';
import { until } from '../lib/wait.mjs';
import { startDevServer as startDevServerIn } from '../live-dev-server.mjs';

if (!process.argv[2]) {
  console.error('usage: node scripts/test-live.mjs <kemi-bin> [<scene>...]');
  process.exit(2);
}
const binary = resolve(process.argv[2]);
const session = `kemi-live-${process.pid}`;
// 待ちが尽きたら、ページの状態を添えて失敗にする。
export const { browser, evaluate, waitFor, close } = browserSession(session, {
  timeout: 15000,
  describe: `JSON.stringify({ url: location.href, view: document.body?.dataset.liveView, side: document.querySelector('#live-stage')?.dataset.side, compare: document.querySelector('#live-stage')?.dataset.compare, notice: document.querySelector('.lv-band-notice')?.textContent, text: document.body?.innerText.slice(0, 400) })`,
});

/**
 * 試験用の開発サーバ。ページを置くディレクトリを渡さなければ、検査が通ったら消す一時ディレクトリに置く。
 * @param {{ port?: number, dir?: string }} [options]
 */
export async function startDevServer(options = {}) {
  return startDevServerIn({ ...options, dir: options.dir ?? await temporaryDirectory('kemi-dev-') });
}

/**
 * 変更のある作業ツリー。場面ごとに作り、場面が置いたファイルをほかの場面の作業ツリーに残さない。一時ディレクトリの中の
 * repo に作るので、作業ツリーの外（隣）に置くファイルも一緒に片付く。
 */
export async function makeRepository() {
  const dir = join(await temporaryDirectory('kemi-live-repo-'), 'repo');
  await mkdir(dir);
  const git = gitIn(dir);
  await git('init', '-q');
  await writeFile(join(dir, 'a.txt'), 'one\ntwo\n');
  await git('add', 'a.txt');
  await git('commit', '-q', '-m', 'base');
  await writeFile(join(dir, 'a.txt'), 'one\nTWO\n');
  return dir;
}

/** kemi を起動し、URL・review の id・中継の URL（`--live` のときだけ）がそろうのを待つ。 */
export function startKemi(dir, state, args) {
  return launchKemi(binary, { cwd: dir, state, args, review: true, live: args.includes('--live') || args.includes('--resume') });
}

/**
 * 場面の前後の決まった形。page を渡すと試験用の開発サーバを立て、kemi に `--live <開発サーバの page>` と args を渡して
 * 起動する。page を省くと開発サーバは立てず、args だけで起動する。scene を走らせ、終わったら（落ちても）kemi を止め、
 * 開発サーバを閉じる。
 * @param {{
 *   repository: string,
 *   page?: string,
 *   args?: string[],
 *   prepare?: (dev: Awaited<ReturnType<typeof startDevServer>>) => Promise<void>,
 *   viewportAfter?: [string, string],
 * }} options prepare は kemi を起動する前の開発サーバの準備（ページの書き換えなど）。viewportAfter は、kemi を止める前に
 *   戻す画面の大きさ
 * @param {(review: { kemi: Awaited<ReturnType<typeof startKemi>>, dev: Awaited<ReturnType<typeof startDevServer>> | null, state: string }) => Promise<void>} scene
 */
export async function withLiveReview({ repository, page, args = [], prepare, viewportAfter }, scene) {
  const dev = page === undefined ? null : await startDevServer();
  try {
    if (dev !== null) {
      await prepare?.(dev);
    }
    const state = await temporaryDirectory('kemi-live-state-');
    const kemi = await startKemi(repository, state, dev === null ? args : ['--live', `${dev.url}${page}`, ...args]);
    try {
      await scene({ kemi, dev, state });
    } finally {
      if (viewportAfter) {
        await browser('set', 'viewport', ...viewportAfter);
      }
      await stop(kemi);
    }
  } finally {
    await dev?.close();
  }
}

/**
 * 画面の「Hand to agent」で渡す。ボタンはエージェントが kemi wait を呼んだレビューでだけ押せるので、
 * 先に待たせておき、渡して返るのを待つ。
 */
export async function handInThePage(kemi, dir, state) {
  const waiting = startWaiting(kemi, dir, state);
  await pressHand();
  assert.equal((await waiting).code, 0, 'kemi wait returns what was handed');
}

/** kemi wait を走らせる。終わったら終了コードと標準出力で解決する。 */
export function startWaiting(kemi, dir, state) {
  return runKemi(binary, ['wait', kemi.id, '--timeout', '30'], { cwd: dir, env: isolatedEnvironment(state) });
}

/** 「Hand to agent」を押せるようになるのを待って押す。 */
export async function pressHand() {
  const button = `(${visible('#rail-hand')} ? document.querySelector('#rail-hand') : ${visible('#btn-hand')} ? document.querySelector('#btn-hand') : null)`;
  await waitFor(`${button} !== null && !${button}.disabled`);
  await evaluate(`${button}.id`).then((id) => browser('click', `#${id}`));
}

/**
 * 要素を押す。agent-browser は、押す点（要素の中心）が別の要素に覆われているとすぐに失敗するので、押す点で一番上に
 * あるのがその要素（かその中）になるのを待ってから押す。覆われたままなら、そのときの配置を添えて落とす。最初に
 * 覆われていたら、覆っていたものと待った時間を NOTE の行に出す（一時的に覆う原因を後で追えるように）。
 */
export async function clickUncovered(selector) {
  const element = `document.querySelector(${JSON.stringify(selector)})`;
  const center = `(() => { const r = ${element}.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`;
  const layout = `JSON.stringify((() => { const c = ${center}; const top = document.elementFromPoint(c.x, c.y); const box = (e) => e && Object.fromEntries(['x', 'y', 'width', 'height'].map((k) => [k, Math.round(e.getBoundingClientRect()[k])])); return { center: c, element: box(${element}), top: top && top.outerHTML.slice(0, 120), topBox: box(top), scroll: [scrollX, scrollY] }; })())`;
  const uncovered = `(() => { const e = ${element}; if (!e) return false; const c = ${center}; const top = document.elementFromPoint(c.x, c.y); return top !== null && e.contains(top); })()`;
  // 覆われているかと配置は 1 回で読む（2 回に分けると、その間に要素が作り直されて無いときに配置が読めない）。
  const covered = await evaluate(`${element} !== null && !${uncovered} ? ${layout} : null`);
  const started = performance.now();
  try {
    await waitFor(uncovered);
  } catch (error) {
    const stayed = await evaluate(layout).catch(() => 'no layout');
    throw new Error(`${selector} stayed covered at its click point: ${stayed}`, { cause: error });
  }
  if (covered !== null) {
    console.log(`NOTE ${selector} was covered at its click point and was clicked after ${Math.round(performance.now() - started)} ms: ${covered}`);
  }
  await browser('click', selector);
}

export function stop(kemi) {
  return stopKemi(kemi, 'SIGINT');
}

// ---- 画素の比較（スナップショットの見た目） ----

/** 要素のスクリーンショット。 */
export async function shot(selector, dir, name) {
  const path = join(dir, `${name}.png`);
  await browser('screenshot', selector, path);
  return decodePng(await readFile(path));
}

export const refPane = '#live-stage .lv-pane[data-side="ref"]';
export const livePane = '#live-stage .lv-pane[data-side="live"]';

/** 比べる相手の選択を自動に戻す。 */
export async function chooseAuto() {
  await evaluate(`(() => { const select = document.querySelector('.lv-compare-select'); select.value = 'latest'; select.dispatchEvent(new Event('change')); return true; })()`);
}

/** 比べる相手の枠に、その id のスナップショットが出ているかを読む式。 */
export const showsSnapshot = (id) => `document.querySelector('${refPane}').dataset.reference === 'snapshot' && document.querySelector('${refPane}').dataset.snapshot === ${JSON.stringify(id)}`;

/**
 * どのスナップショットか（種類と、その種類の中で取った順の番号）。見出しの名前（Start、Recorded n、Handed n）の
 * 付け方は web/assets/live-model.test.js が確かめるので、ここでは文言でなく api/snapshots の種類と順で見分ける。
 * @typedef {{ kind: 'start' | 'manual' | 'handed', n: number }} Which
 */
/** @type {Which} */
export const startSnapshot = { kind: 'start', n: 1 };
/** @returns {Which} */
export const recordedSnapshot = (n) => ({ kind: 'manual', n });
/** @returns {Which} */
export const handedSnapshot = (n) => ({ kind: 'handed', n });

/**
 * そのスナップショットが kemi に預けられるのを待ち、その id を返す。
 * @param {Which} which
 */
export async function snapshotTaken(kemi, { kind, n }, timeout = 15000) {
  return until(async () => (await getJson(kemi.url, 'api/snapshots')).snapshots.filter((snapshot) => snapshot.kind === kind)[n - 1]?.id, { timeout, what: `snapshot ${kind} ${n}` });
}

/**
 * 比べる相手の枠にそのスナップショットが出るのを待つ（also を渡せば、それも成り立つまで）。その id を返す。
 * @param {Which} which
 * @param {string} [also] ページの中で読む式
 */
export async function waitForSnapshot(kemi, which, also, timeout) {
  const id = await snapshotTaken(kemi, which, timeout);
  await waitFor(also === undefined ? showsSnapshot(id) : `${showsSnapshot(id)} && ${also}`, timeout);
  return id;
}

/** 比べる相手の選択に、その id のスナップショットが出ているかを読む式。 */
export const listedOption = (id) => `Array.from(document.querySelectorAll('.lv-compare-select option')).some((option) => option.value === ${JSON.stringify(id)})`;

/**
 * 比べる相手の選択に、そのスナップショットが出るのを待つ。その id を返す。
 * @param {Which} which
 */
export async function waitForOption(kemi, which) {
  const id = await snapshotTaken(kemi, which);
  await waitFor(listedOption(id));
  return id;
}
export const notRecorded = `document.querySelector('${refPane}').dataset.reference === 'none' && ${visible(`${refPane} .lv-empty`)}`;

/** 舞台の縮めた倍率（--lv-scale）を読む式。 */
export const STAGE_SCALE = `Number(getComputedStyle(document.querySelector('#live-stage')).getPropertyValue('--lv-scale') || '1')`;

/** 舞台の縮めた倍率。 */
export async function stageScale() {
  return Number(await evaluate(STAGE_SCALE));
}

/** 透かし具合の操作に値を入れる。 */
export async function setOpacity(value) {
  await evaluate(`(() => { const range = document.querySelector('.lv-opacity'); range.value = '${value}'; range.dispatchEvent(new Event('input')); return true; })()`);
}

/** settled の待つ上限（ms）。 */
const SETTLE_TIMEOUT = 5000;

/**
 * 要素の置き方（位置・大きさ・transform）と舞台の倍率が、5 フレーム続けて変わらなくなるのを待つ。表示幅や画面の
 * 大きさを変えた後、スクロールさせた後に、枠の中の座標で押したり撮ったりする前に使う。
 */
export async function settled(selector) {
  const result = await evaluate(`new Promise((done) => {
    const read = () => {
      const element = document.querySelector(${JSON.stringify(selector)});
      if (!element) return 'missing';
      const r = element.getBoundingClientRect();
      return [r.x, r.y, r.width, r.height, getComputedStyle(element).transform, ${STAGE_SCALE}].join(' ');
    };
    const deadline = performance.now() + ${SETTLE_TIMEOUT};
    let previous = read();
    let stable = 0;
    const check = () => {
      const now = read();
      stable = now === previous && now !== 'missing' ? stable + 1 : 0;
      previous = now;
      if (stable >= 5) done('settled');
      else if (performance.now() > deadline) done('still moving: ' + now);
      else requestAnimationFrame(check);
    };
    requestAnimationFrame(check);
  })`);
  assert.equal(result, 'settled', `${selector} did not settle within ${SETTLE_TIMEOUT} ms`);
}

/** 動いているページからの知らせを種類ごとに数え始める（レビュー画面を開くたびに数え直す）。 */
export async function countPageMessages() {
  await evaluate(`(() => {
    if (!window.__kemiSeen) {
      window.__kemiSeen = {};
      addEventListener('message', (event) => {
        if (event.data?.kemi !== 'live') return;
        window.__kemiSeen[event.data.type] = (window.__kemiSeen[event.data.type] ?? 0) + 1;
        if (event.data.type === 'scroll') window.__kemiScrollY = event.data.y;
      });
    }
    return true;
  })()`);
}

/** 数え始めてから届いた、その種類の知らせの数を読む式。 */
export const seenMessages = (type) => `(window.__kemiSeen?.[${JSON.stringify(type)}] ?? 0)`;

/** 数え始めてから最後に届いたスクロールの知らせの、ページの縦の位置を読む式。まだ無ければ undefined。 */
export const lastScrollY = 'window.__kemiScrollY';

/** action を行い、その後に動いているページからその種類の知らせが届くのを待つ。 */
export async function nextPageMessage(type, action) {
  await countPageMessages();
  const before = await evaluate(seenMessages(type));
  await action();
  await waitFor(`${seenMessages(type)} > ${before}`);
}

/**
 * 要素の上で回したホイールを作って送る（agent-browser のホイールは押した位置に届かず、画面の左上に届く）。at は要素の
 * 左上からの位置で、その要素に送る。at を省くと要素の真ん中で、そこで一番上にある要素に送る。
 * @param {string} selector
 * @param {{ x?: number, y?: number, at?: number }} [wheel] x と y は横と縦の量
 */
export async function wheelOn(selector, { x = 0, y = 0, at } = {}) {
  const target = at === undefined
    ? `const px = r.left + r.width / 2; const py = r.top + r.height / 2; const target = document.elementFromPoint(px, py);`
    : `const px = r.x + ${at}; const py = r.y + ${at}; const target = element;`;
  await evaluate(`(() => { const element = document.querySelector(${JSON.stringify(selector)}); const r = element.getBoundingClientRect(); ${target} return target.dispatchEvent(new WheelEvent('wheel', { deltaX: ${x}, deltaY: ${y}, clientX: px, clientY: py, bubbles: true, cancelable: true })); })()`);
}

/** 枠の中の点を押す。 */
export async function clickInPane(pane, x, y) {
  const box = await evaluate(`(() => { const r = document.querySelector('${pane} .lv-frame').getBoundingClientRect(); return { x: r.x, y: r.y }; })()`);
  const scale = await stageScale();
  await browser('mouse', 'move', String(Math.round(box.x + x * scale)), String(Math.round(box.y + y * scale)));
  await browser('mouse', 'down');
  await browser('mouse', 'up');
}

/** モックのパネル（R-PAGE-MOCK）。 */
export const mockPanel = '.lv-mock-panel';

/** 比べる相手の選択肢の最後の「モックのファイル」を選んで、モックのパネルを開く。 */
export async function openMockPanel() {
  await evaluate(`(() => { const select = document.querySelector('.lv-compare-select'); select.value = 'mock-file'; select.dispatchEvent(new Event('change')); return true; })()`);
  await waitFor(visible(mockPanel));
}

/** モックのパネルのパスの欄にパスを入れて割り当てる。 */
export async function assignMockByPath(path) {
  await openMockPanel();
  await browser('fill', `${mockPanel} .lv-mock-path`, path);
  await browser('click', `${mockPanel} .lv-mock-path-assign`);
}

/** 見えているモックのメニューの操作（読み直す・外す）を押す。 */
export async function chooseFromMockMenu(item) {
  await evaluate(`(() => { const button = Array.from(document.querySelectorAll('.lv-mock-menu-button')).find((element) => element.getClientRects().length > 0); button.click(); return true; })()`);
  await waitFor(visible(`.lv-mock-menu .${item}`));
  await browser('click', `.lv-mock-menu .${item}`);
}

/** 見比べ方を切り替え、舞台がその見比べ方になるのを待つ。 */
export async function chooseCompare(mode) {
  await waitFor(visible(`.lv-mode button[data-compare="${mode}"]`));
  await browser('click', `.lv-mode button[data-compare="${mode}"]`);
  await waitFor(`document.querySelector('#live-stage').dataset.compare === '${mode}'`);
}

/** 見る対象の見出しに出ている倍率。 */
export const liveScale = `Number(document.querySelector('${livePane} .lv-bar-label').textContent.match(/×([0-9.]+)/)?.[1] ?? NaN)`;

/** /tall.html の最後の帯の色（hsl(324, 70%, 70%)）。 */
export const isLastBand = ([r, g, b]) => r > 215 && g > 105 && g < 145 && b > 170 && b < 210;

/** 表示中のページの変化の一覧。無ければ null。 */
export const changeList = `document.querySelector('#page-tree .lv-page[data-current="true"] .lv-changes')`;

export const isBlue = ([r, g, b]) => r < 90 && g < 130 && b > 180;

/**
 * 枠を撮り直しながら、accept が真を返す画像になるのを待つ。期限を過ぎたら最後に撮った画像を返す（呼ぶ側が、何を
 * 確かめていたかを添えて落とす）。スナップショットの枠はスクリプトの止まった別のオリジンで、中を調べられないため
 * 画素で見る。
 */
export async function shotUntil(selector, dir, name, accept, timeout = 10000) {
  const deadline = Date.now() + timeout;
  for (;;) {
    const image = await shot(selector, dir, name);
    if (await accept(image) || Date.now() > deadline) return image;
    // 撮り直す間隔。
    await new Promise((done) => setTimeout(done, 300));
  }
}

/** 枠を撮り直しながら、範囲の中に条件に合う画素が出る（`present` が false なら無くなる）のを待つ。 */
export async function waitForPixels(selector, dir, name, region, test, present = true, timeout = 10000) {
  const matches = (image) => (countPixels(image, region, test) > 0) === present;
  const image = await shotUntil(selector, dir, name, matches, timeout);
  if (!matches(image)) assert.fail(`${present ? 'no' : 'still'} matching pixels in ${region.join(',')}: ${join(dir, `${name}.png`)}`);
  return image;
}

/** 比べる相手を、そのスナップショットに選ぶ。 */
export async function chooseReference(kemi, which) {
  const id = await waitForOption(kemi, which);
  await evaluate(`(() => { const select = document.querySelector('.lv-compare-select'); select.value = ${JSON.stringify(id)}; select.dispatchEvent(new Event('change')); return true; })()`);
  await waitFor(showsSnapshot(id));
}

/** 書いている途中のコメントの場所の番号。 */
export const draftNumbers = `JSON.stringify(Array.from(document.querySelectorAll('#live-compose .lv-place')).map((row) => Number(row.dataset.n)))`;

/** 道具を選ぶ。 */
export async function chooseTool(tool) {
  await browser('click', `.lv-tools button[data-tool="${tool}"]`);
  await waitFor(`document.querySelector('.lv-tools button[data-tool="${tool}"]').getAttribute('aria-pressed') === 'true'`);
}

/** kemi の API からレビューを読む（ページへのコメントを確かめる）。 */
export async function reviewJson(kemi) {
  return getJson(kemi.url, 'api/review');
}

/**
 * レビュー画面から動いているページへの、その種類の頼みを預かる（返事の遅いページを作る）。releaseRequests で送る。
 * 預かっている間は枠の contentWindow が差し替わり、ページからの知らせは読まれない。
 */
export async function holdRequests(...types) {
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
export const heldRequests = `(window.__kemiHeld ?? []).length`;

/** 枠を元に戻し、預かった頼み（type を渡せばその種類のものだけ）をページに送る。残りは次に送るまで預かったまま。 */
export async function releaseRequests(type) {
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

/** API でページへのコメントを足す（画面の操作は pageCommentPlacesArePutAndSaved で確かめる）。 */
export async function addPageComment(kemi, url, width, body) {
  return postJson(kemi.url, 'api/comment', {
    op: 'add_page',
    page: { url, width, places: [{ n: 1, kind: 'element', points: [], elements: [{ selector: '#button', text: 'Press', rect: { x: 0, y: 200, w: 300, h: 100 } }] }] },
    body,
  });
}

/** 場所や要素を光らせる光（画面モックの --ai の青）。 */
export const isGlow = ([r, g, b]) => r < 95 && g < 110 && b > 140 && b - r > 60;

/** 書いたページへのコメントを保存し、書く欄が空くのを待つ。 */
export async function savePageCommentInThePage(body) {
  await browser('fill', '#live-compose .lv-compose-body', body);
  await browser('click', '#live-compose .lv-compose-save');
  await waitFor(`${draftNumbers} === '[]'`);
}
