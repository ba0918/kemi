// `--live` のブラウザ検査の場面: 中継とページの見方（scripts/test-live.mjs から走らせる）。

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { visible } from '../lib/browser.mjs';
import { temporaryDirectory } from '../lib/fixture.mjs';
import { postJson } from '../lib/kemi.mjs';
import { pixelAt } from '../lib/png.mjs';
import {
  browser,
  evaluate,
  waitFor,
  startDevServer,
  startKemi,
  withLiveReview,
  handInThePage,
  stop,
  shot,
  refPane,
  livePane,
  startSnapshot,
  handedSnapshot,
  snapshotTaken,
  waitForSnapshot,
  listedOption,
  waitForOption,
  clickInPane,
  chooseCompare,
  liveScale,
  changeList,
  chooseReference,
  draftNumbers,
  chooseTool,
  reviewJson,
  savePageCommentInThePage,
} from './common.mjs';

/** 空いているポートを 1 つ選ぶ（開発サーバを後から同じポートで起動するため）。 */
async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, '127.0.0.1', () => done(undefined)));
  const address = server.address();
  const port = typeof address === 'object' && address ? address.port : 0;
  await new Promise((done) => server.close(() => done(undefined)));
  return port;
}

/** 中継（R-PAGE-PROXY）: HMR が中継越しに効き、中継用の cookie がページから読めない。 */
export async function relayCarriesHmrAndHidesTheCookie(repository) {
  await withLiveReview({ repository, page: '' }, async ({ kemi, dev }) => {
    await browser('open', kemi.url);
    await browser('open', kemi.live);
    await waitFor(`document.documentElement.dataset.hmrConnected === 'true'`);
    const box = `getComputedStyle(document.querySelector('.box')).backgroundColor`;
    assert.equal(await evaluate(box), 'rgb(255, 0, 0)');
    await writeFile(join(dev.dir, 'style.css'), '.box { background: rgb(0, 0, 255); width: 200px; height: 80px; }\n');
    await waitFor(`${box} === 'rgb(0, 0, 255)'`);
    assert.equal(await evaluate(`document.cookie.includes('kemi_live')`), false, 'the relay cookie must be HttpOnly');
    console.log('PASS 中継越しに HMR が効き、中継用の cookie がページの document.cookie に出ない');
  });
}

/**
 * 開発サーバにつながらないときは出して待ち、起動したらつながる（R-PAGE-MODE、R-PAGE-SESSION）: 開発サーバを止めたまま
 * 新しく起動しても、止めたまま復元しても、つながらない旨が出て、開発サーバを起動するとつながる。
 */
export async function waitsForTheDevServer(repository) {
  const port = await freePort();
  const state = await temporaryDirectory('kemi-live-state-');
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
    await postJson(first.url, 'api/message', { body: 'keep this review' });
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
export async function fileReloadsWhenItsCssIsSaved(repository) {
  await mkdir(join(repository, 'site'), { recursive: true });
  await writeFile(join(repository, 'site', 'page.html'), '<!doctype html><html><head><link rel="stylesheet" href="style.css"></head><body><p class="box">file page</p></body></html>\n');
  await writeFile(join(repository, 'site', 'style.css'), '.box { color: rgb(255, 0, 0); }\n');
  await withLiveReview({ repository, args: ['--live', 'site/page.html'] }, async ({ kemi }) => {
    assert.match(kemi.live, /\/site\/page\.html$/);
    await browser('open', kemi.url);
    await browser('open', kemi.live);
    const color = `getComputedStyle(document.querySelector('.box')).color`;
    await waitFor(`${color} === 'rgb(255, 0, 0)'`);
    await evaluate(`window.__kemiBeforeReload = true; true`);
    await writeFile(join(repository, 'site', 'style.css'), '.box { color: rgb(0, 128, 0); }\n');
    await waitFor(`${color} === 'rgb(0, 128, 0)' && window.__kemiBeforeReload === undefined`);
    console.log('PASS 手元の HTML ファイルが参照する CSS を保存し直すと、ページが読み込み直される');
  });
}

/** ページの見方（R-PAGE-VIEW、R-PAGE-PROXY）: 枠を拒むページが出て書き換えの表示が出る、表示幅の範囲、狭い画面。 */
export async function pageViewShowsFramedPagesWidthsAndNarrowScreens(repository) {
  await withLiveReview({ repository, page: 'framed.html', viewportAfter: ['1280', '800'] }, async ({ kemi }) => {
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
  });
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
export async function modeTabsSwitchTheTopbar(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
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
  });
}

/** git の外（R-PAGE-MODE）: コードの見方の代わりに理由が出る。範囲の別の HTML へのリンクで見る対象が移り、ツリーに出る。 */
export async function outsideGitFilePages() {
  const dir = await temporaryDirectory('kemi-live-nogit-');
  await writeFile(join(dir, 'index.html'), '<!doctype html><html><head><style>body{margin:0} a{display:block;width:400px;height:200px;background:#ddd}</style></head><body><a id="next" href="other.html">Other</a></body></html>\n');
  await writeFile(join(dir, 'other.html'), '<!doctype html><html><head></head><body><h1>Other</h1></body></html>\n');
  await withLiveReview({ repository: dir, args: ['--live', 'index.html'] }, async ({ kemi }) => {
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('#live-stage .lv-pane[data-side="live"] .lv-bar-label')?.textContent.includes('/index.html')`);
    await browser('click', '.lv-view button[data-view="code"]');
    await waitFor(visible('#live-no-code'));
    console.log('PASS git の外で起動すると、コードの見方の代わりに理由が出る');

    await browser('click', '.lv-view button[data-view="page"]');
    await waitFor(visible('#page-tree'));
    await chooseTool('interact');
    await clickInPane(livePane, 100, 100);
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/other.html'`);
    console.log('PASS ファイルのページの中で範囲の別の HTML へのリンクを押すと、見る対象が移り、ページのツリーにそのページが出る');
  });
}

/**
 * git の外のコードの見方から渡す（R-PAGE-MODE、R-PAGE-SNAPSHOT）: git の外のコードの見方は、舞台の代わりに理由だけを出す。
 * そこから渡したときに取るスナップショットも、選んだ幅で並べた文書から記述する。ページを変えずにページの見方に戻せば、
 * それと比べた変化は 0。
 */
export async function handingFromTheCodeViewOutsideGit() {
  const dir = await temporaryDirectory('kemi-live-nogit-');
  await withLiveReview({ repository: dir, page: 'changing.html' }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '800');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot, `${changeList}?.dataset.main === '0'`);
    await postJson(kemi.url, 'api/message', { body: 'please look' });
    await browser('click', '.lv-view button[data-view="code"]');
    await waitFor(`${visible('#live-no-code')} && !${visible('#live-stage')}`);
    await handInThePage(kemi, dir, state);
    await waitForOption(kemi, handedSnapshot(1));
    await browser('click', '.lv-view button[data-view="page"]');
    await chooseReference(kemi, handedSnapshot(1));
    await waitFor(`${changeList} !== null`);
    const counts = `${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`;
    assert.equal(await evaluate(counts), '0/0', 'a snapshot taken when handing from the code view outside git shows no change against the unchanged page');
    console.log('PASS git の外のコードの見方から渡したときのスナップショットと、変えていないページを比べると変化が 0');
  });
}

/**
 * 説明の見方から渡す（R-EXPLANATION-VIEW、R-AGENT-STATE、R-PAGE-SNAPSHOT）: 説明があると説明の見方で始まり、舞台は
 * 隠れている。その間に取る開始時のスナップショットも、説明の見方から渡したときに取るスナップショットも、選んだ幅で
 * 並べた文書から記述する。ページを変えずにページの見方に戻せば、どちらと比べても変化は 0。
 */
export async function handingFromTheExplanationView(repository) {
  // 説明のファイルは作業ツリーの外に置く（レビューの差分に入れない）。
  const explanation = join(await temporaryDirectory('kemi-live-explanation-'), 'explanation.json');
  await writeFile(explanation, JSON.stringify({ sections: [{ id: 'overview', title: 'Overview', blocks: [{ id: 'note', part: { type: 'text', body: 'Read the page before handing.' }, references: [] }] }] }));
  // 一覧の見出しの名前は、選んだ比べる相手の選択の文字に含まれる。選び直した直後の前の相手の一覧を読まない。
  const listedAgainstChosen = `(() => { const vs = ${changeList}?.querySelector('.lv-changes-vs'); return Boolean(vs) && document.querySelector('.lv-compare-select').selectedOptions[0].textContent.includes(vs.textContent); })()`;
  await withLiveReview({ repository, page: 'changing.html', args: ['--explanation', explanation] }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '800');
    await browser('open', kemi.url);
    const startId = await snapshotTaken(kemi, startSnapshot);
    await waitFor(`document.body.dataset.reviewView === 'explanation' && document.body.dataset.liveView === 'page' && !${visible('#live-stage')} && ${listedOption(startId)}`);
    await postJson(kemi.url, 'api/message', { body: 'please look' });
    await handInThePage(kemi, repository, state);
    await waitForOption(kemi, handedSnapshot(1));
    assert.equal(await evaluate(`document.body.dataset.reviewView`), 'explanation', 'handing keeps the explanation view');
    await browser('click', '.lv-view button[data-view="page"]');
    await waitFor(`document.body.dataset.reviewView === 'page' && ${visible('#live-stage')}`);
    const counts = `${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`;
    const against = {};
    for (const [label, which] of [['Handed 1', handedSnapshot(1)], ['Start', startSnapshot]]) {
      await chooseReference(kemi, which);
      await waitFor(listedAgainstChosen);
      against[label] = await evaluate(counts);
    }
    assert.deepEqual(against, { 'Handed 1': '0/0', Start: '0/0' }, 'snapshots taken in the explanation view show no change against the unchanged page');
    console.log('PASS 説明の見方で始めて取った開始時のスナップショットも、説明の見方から渡したときのスナップショットも、変えていないページと比べると変化が 0');
  });
}

/** `--live` でないレビューは、ページ用のファイルを読み込まない（R-VERIFY）。 */
export async function otherReviewsLoadNoPageFiles(repository) {
  await withLiveReview({ repository, args: ['--worktree'] }, async ({ kemi }) => {
    await browser('open', kemi.url);
    await waitFor(`document.querySelectorAll('[data-kemi-row]').length > 0`);
    const loaded = await evaluate(`JSON.stringify(performance.getEntriesByType('resource').map((entry) => entry.name))`);
    const urls = JSON.parse(loaded);
    assert.ok(urls.some((url) => url.endsWith('assets/app.js')), loaded);
    const pageFiles = urls.filter((url) => /live/.test(new URL(url).pathname));
    assert.deepEqual(pageFiles, [], 'no page file may be loaded');
    console.log(`PASS --live でないレビューは、ページ用のファイルを読み込まない（読み込んだ URL ${urls.length} 件）`);
  });
}

/** 選んだ表示幅の外（R-PAGE-VIEW）: ページの外はレビュー画面の背景になり、ページ自身の白い背景は変わらない。 */
export async function pageSurroundingsMatchTheReviewWithoutRepaintingThePage(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1920', '1000');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    for (let count = 0; count < 3 && await evaluate('document.documentElement.dataset.theme') !== 'dark'; count++) {
      await browser('click', '#btn-theme');
    }
    const layout = await evaluate(`(() => {
      const pane = document.querySelector('${livePane}');
      const viewport = pane.querySelector('.lv-viewport');
      const page = pane.querySelector('.lv-frame');
      return { viewportWidth: viewport.clientWidth, pageWidth: page.getBoundingClientRect().width, reviewBackground: getComputedStyle(pane).backgroundColor };
    })()`);
    assert.ok(layout.viewportWidth > layout.pageWidth + 100, 'the viewport has visible space beyond the chosen page width');
    const image = await shot(`${livePane} .lv-viewport`, shots, 'page-surrounding');
    const reviewColor = layout.reviewBackground.match(/\d+/g).slice(0, 3).map(Number);
    assert.deepEqual(pixelAt(image, image.width - 12, 120).slice(0, 3), reviewColor, 'outside the page uses the review background instead of white');
    assert.deepEqual(pixelAt(image, Math.floor(layout.pageWidth - 20), 120).slice(0, 3), [255, 255, 255], 'the actual white page remains white');
    console.log('PASS 選択したページ幅の外はレビュー画面の背景になり、ページ自身の白い背景は変わらない');
  });
}

/**
 * 狭い画面の道具と「エージェントに渡す」（R-PAGE-VIEW の狭い画面、R-AGENT-STATE、R-NARROW）: 幅 390px のページの見方では、
 * 道具と「Hand to agent」が画面の下の 1 つの浮かぶツールバーにあり、全モード共通の浮かぶ「Hand to agent」は出ない。
 * `kemi wait` の前は、そのそばに `kemi wait <id>` を写す操作が出る。道具を切り替えて場所を置ける。コードの見方では
 * 全モード共通の浮かぶ「Hand to agent」だけ。幅 1280px ではどちらの浮かぶものも見えない。
 */
export async function narrowToolsFloatWithHand(repository) {
  await withLiveReview({ repository, page: 'rich.html', viewportAfter: ['1280', '800'] }, async ({ kemi }) => {
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
  });
}

/**
 * 狭い画面のページの見方（R-PAGE-VIEW の狭い画面、R-NARROW の `--live` の 1 段目のタブ、R-PAGE-MODE）: 幅 390px で開くと、
 * ページの見方の操作の帯が 1 行に収まり、その行にエージェントの状態があり、見たの進捗が見えず、重ねて透かす操作が無い。
 * 帯の「…」のメニューに比べる相手の選択・手で取る操作・枠に合わせると等倍・モックのファイルがある。上部バーの「…」の
 * メニューには表示の操作のうちテーマだけがある。1 段目のタブでコードの見方に切り替わる。
 */
export async function narrowPageViewFitsOneRow(repository) {
  const band = '#live-band';
  await withLiveReview({ repository, page: 'rich.html', viewportAfter: ['1280', '800'] }, async ({ kemi }) => {
    await browser('set', 'viewport', '390', '844');
    await browser('open', kemi.url);
    await waitFor(`${visible(livePane)} && ${visible('.lv-side')} && ${visible(`${band} .lv-menu-button`)}`);
    const tops = await evaluate(`JSON.stringify(Array.from(document.querySelector('${band}').children).filter((child) => child.getClientRects().length > 0 && getComputedStyle(child).visibility !== 'hidden').map((child) => Math.round(child.getBoundingClientRect().top)))`);
    assert.ok(JSON.parse(tops).length >= 4 && new Set(JSON.parse(tops)).size === 1, `the band is one row: ${tops}`);
    await waitFor(`${visible(`${band} .lv-band-agent`)} && document.querySelector('${band} .lv-band-agent').dataset.kemiAgentState === document.querySelector('#agent-status').dataset.kemiAgentState`);
    assert.equal(await evaluate(visible('#progress')), false, 'the seen progress is not shown');
    assert.equal(await evaluate(`${visible('.lv-mode button[data-compare="overlay"]')} || ${visible('.lv-opacity')}`), false, 'no overlay on a narrow screen');
    await browser('click', `${band} .lv-menu-button`);
    await waitFor(`${visible('.lv-menu .lv-compare-select')} && ${visible('.lv-menu .lv-record-now')} && ${visible('.lv-menu .lv-zoom')} && ${visible('.lv-menu .lv-mock-open')}`);
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
  });
}

/**
 * 開いたまま狭い画面との境をまたぐ（R-PAGE-VIEW の狭い画面、R-PAGE-REF）: 等倍で見る対象だけのまま狭くすると、帯のメニューに
 * 比べる相手の選択が出る。比べる相手の 1 枚を見ている間は、見出しが比べる相手の名前（選択に出ている名前に含まれる）になり、
 * その 1 枚の見出しに倍率が出て、表示幅より狭い枠を横にスクロールできる。
 * 動いているページに戻すと見出しは見る対象だけのときと同じになり、広くすると比べる相手の選択がまた隠れる。並べたまま
 * 狭くしても、見出しは見る対象だけのときと同じになる。
 */
export async function crossingTheNarrowWidthFollowsTheShownPage(repository) {
  const heading = `document.querySelector('.lv-stage-name').textContent`;
  const refViewport = `document.querySelector('${refPane} .lv-viewport')`;
  const refScale = `Number(document.querySelector('${refPane} .lv-bar-label').textContent.match(/×([0-9.]+)/)?.[1] ?? NaN)`;
  await withLiveReview({ repository, page: 'tall.html', viewportAfter: ['1280', '800'] }, async ({ kemi }) => {
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
  });
}
