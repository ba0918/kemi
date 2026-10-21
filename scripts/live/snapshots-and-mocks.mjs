// `--live` のブラウザ検査の場面: スナップショット・モック・比べる相手（scripts/test-live.mjs から走らせる）。

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, readFile, rm, symlink, writeFile } from 'node:fs/promises';
import { createServer as createHttpServer } from 'node:http';
import { tmpdir } from 'node:os';
import { basename, dirname, join } from 'node:path';
import { isDeepStrictEqual } from 'node:util';

import { visible } from '../lib/browser.mjs';
import { gitIn, temporaryDirectory } from '../lib/fixture.mjs';
import { getJson, postJson } from '../lib/kemi.mjs';
import { comparePixels, countPixels, encodePng, pixelAt } from '../lib/png.mjs';
import { until } from '../lib/wait.mjs';
import { RESOURCE_BOXES, changingCss } from '../live-dev-server.mjs';
import {
  browser,
  evaluate,
  waitFor,
  startDevServer,
  makeRepository,
  startKemi,
  withLiveReview,
  handInThePage,
  stop,
  shot,
  refPane,
  livePane,
  chooseAuto,
  showsSnapshot,
  startSnapshot,
  recordedSnapshot,
  handedSnapshot,
  snapshotTaken,
  waitForSnapshot,
  listedOption,
  waitForOption,
  notRecorded,
  setOpacity,
  settled,
  nextPageMessage,
  clickInPane,
  mockPanel,
  openMockPanel,
  assignMockByPath,
  chooseFromMockMenu,
  chooseCompare,
  liveScale,
  isLastBand,
  changeList,
  isBlue,
  shotUntil,
  waitForPixels,
  chooseTool,
  reviewJson,
  holdRequests,
  releaseRequests,
  addPageComment,
} from './common.mjs';

/** 透かし具合を入れ、重ねた比べる相手がその透かし具合で描かれるのを待つ。 */
async function setOpacityAndWait(value) {
  await setOpacity(value);
  await waitFor(`getComputedStyle(document.querySelector('${refPane} .lv-box')).opacity === '${value / 100}'`);
}

const isYellow = ([r, g, b]) => r > 200 && g > 200 && b < 80;

/**
 * スナップショット（R-PAGE-SNAPSHOT、R-PAGE-VIEW、R-PAGE-REF）: 渡す前は開始時のものが既定の比べる相手。390 と 1280 で
 * 取って幅を切り替えると、動いているページと比べる相手が同じ幅で描かれ、無い幅・別の URL では記録されていない旨と取る
 * 操作が出る。動いているページと画素を比べる（完全に一致しなければ差の画像と割合を出して人の確認に回す）。スナップショット
 * では onclick が動かない。スナップショットを持つ 2 つのページがツリーに表示幅と並び、押すとそのページ・その幅に移る。
 * 渡すと表示中のページのスナップショットを取り、それが既定の比べる相手になる。
 */
export async function snapshotsAreTakenShownAndChosen(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    console.log('PASS 渡す前のレビューで、開始時のスナップショットが既定の比べる相手になる');

    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1));
    assert.equal(
      await evaluate(`document.querySelector('${refPane} .lv-frame').style.width === document.querySelector('${livePane} .lv-frame').style.width && document.querySelector('${livePane} .lv-frame').style.width === '390px'`),
      true,
    );
    // 手で取ると比べる相手がその時点に切り替わり、選んだ時点は今の幅で探すので、1280 では記録されていない。
    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(notRecorded);
    await chooseAuto();
    await waitForSnapshot(kemi, startSnapshot);
    assert.equal(await evaluate(`document.querySelector('${refPane} .lv-frame').style.width`), '1280px');
    console.log('PASS 390 と 1280 で取ってから幅を切り替えると、動いているページと比べる相手が同じ幅で描かれ、無い幅では記録されていない旨と取る操作が出る');

    // 画素の比較は 390 で（縮めずに描ける幅）。
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitForSnapshot(kemi, recordedSnapshot(1));
    // スナップショットが描かれたことを、1 段目の SVG の青い円で見る。
    const before = await waitForPixels(`${refPane} .lv-frame`, shots, 'snapshot', [0, 0, 390, 80], isBlue);
    const now = await shot(`${livePane} .lv-frame`, shots, 'live');
    const compared = await comparePixels(before, now, join(shots, 'diff.png'));
    if (compared.different === 0) {
      console.log(`PASS shadow DOM・canvas・SVG・入力欄を持つページのスナップショットが、動いているページと画素まで同じに出る（${compared.total} 画素）`);
    } else {
      console.log(`CHECK スナップショットと動いているページの画素が ${compared.different} / ${compared.total}（${(compared.ratio * 100).toFixed(3)}%）違う。差の画像: ${join(shots, 'diff.png')}（人が確かめる）`);
    }

    await chooseTool('interact');
    // スナップショットを押してから動いているページを押し、後に押した方の onclick（ページの背景を黄色にする）が
    // 描かれるのを、ボタンの下の余白で待つ。先に押したスナップショットで onclick が動くなら、もう描かれている。
    await clickInPane(refPane, 150, 250);
    await clickInPane(livePane, 150, 250);
    const liveClicked = await shotUntil(`${livePane} .lv-frame`, shots, 'live-clicked', (image) => countPixels(image, [0, 320, 390, 400], isYellow) > 0);
    const afterClick = await shot(`${refPane} .lv-frame`, shots, 'snapshot-clicked');
    assert.equal((await comparePixels(before, afterClick, join(shots, 'clicked-diff.png'))).different, 0, 'the snapshot must not run onclick');
    assert.ok((await comparePixels(now, liveClicked, join(shots, 'live-clicked-diff.png'))).different > 0, 'the running page runs onclick (control)');
    console.log('PASS onclick を持つ要素を押しても、スナップショットではスクリプトが動かない（動いているページでは動く）');

    await browser('click', '.lv-widths button[data-width="1280"]');
    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${kemi.live.replace(/\/rich\.html$/, '')}/other.html`)}; true`);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/other.html') && ${notRecorded}`);
    console.log('PASS 別の URL に移ると、その URL のスナップショットが無い旨が出る');

    await browser('click', '.lv-record');
    await waitForSnapshot(kemi, recordedSnapshot(2));
    const pages = `Array.from(document.querySelectorAll('#page-tree .lv-page')).map((row) => row.dataset.page + ':' + Array.from(row.querySelectorAll('.lv-width-tag')).map((tag) => tag.dataset.width).join(','))`;
    await waitFor(`JSON.stringify(${pages}) === JSON.stringify(['/other.html:1280', '/rich.html:390,1280'])`);
    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-page-open');
    await waitForSnapshot(kemi, startSnapshot, `document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/rich.html'`);
    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-width-tag[data-width="390"]');
    await waitForSnapshot(kemi, recordedSnapshot(1), `document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    console.log('PASS スナップショットを持つ 2 つのページがツリーに表示幅と並び、押すとそのページ・その幅に移る');

    await postJson(kemi.url, 'api/message', { body: 'please look' });
    await handInThePage(kemi, repository, state);
    await waitForSnapshot(kemi, handedSnapshot(1));
    console.log('PASS エージェントに渡すと表示中のページのスナップショットを取り、それが既定の比べる相手になる');
  });
}

/**
 * スナップショットが要る資源（R-PAGE-SNAPSHOT の同じ見た目）: スナップショットの枠は中継のポートから
 * 読めないので、CSS と画像の参照が中継を指したままだと、その箱が描かれない。別のオリジンの CSS・@import・style 属性の
 * url()・<picture> の <source>・video の poster・SVG の <image>・<input type=image> が、スナップショットでも動いている
 * ページと同じ色に出る。
 */
export async function snapshotsCarryTheirResources(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  await withLiveReview({ repository, page: 'resources.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1));
    const colors = (image) => Object.fromEntries(RESOURCE_BOXES.map(({ name, left, top }) => [name, pixelAt(image, left + 40, top + 40)]));
    const drawn = Object.fromEntries(RESOURCE_BOXES.filter(({ name }) => name !== 'poster').map(({ name, color }) => [name, color]));
    // video の上には操作部が重なるので、色そのものではなく白でない（poster が出た）ことを見る。
    const everyBox = (image) => { const { poster, ...plain } = colors(image); return isDeepStrictEqual(plain, drawn) && !isDeepStrictEqual(poster, [255, 255, 255]); };
    // 箱は資源を読み込んでから描かれるので、描かれるまで撮り直す。
    const live = await shotUntil(`${livePane} .lv-frame`, shots, 'resources-live', everyBox);
    const seen = colors(live);
    const snapshot = await shotUntil(`${refPane} .lv-frame`, shots, 'resources-snapshot', (image) => isDeepStrictEqual(colors(image), seen));
    const { poster, ...plain } = seen;
    assert.deepEqual(plain, drawn, 'the running page draws every box (control)');
    assert.notDeepEqual(poster, [255, 255, 255], 'the running page draws the poster (control)');
    assert.deepEqual(colors(snapshot), seen, `the snapshot draws every box as the running page does: ${join(shots, 'resources-snapshot.png')}`);
    console.log('PASS 別のオリジンの CSS・@import・style 属性の url()・<picture> の <source>・video の poster・SVG の <image>・<input type=image> が、スナップショットでも動いているページと同じに出る');
  });
}

/** パネルの一覧に出ているファイル。 */
const listedMockFiles = `JSON.stringify(Array.from(document.querySelectorAll('${mockPanel} .lv-mock-file')).map((item) => item.dataset.path))`;

/**
 * モック（R-PAGE-MOCK、R-PAGE-REF、R-PAGE-VIEW）: 配れる範囲の外のパスと .txt は理由つきで断る。CSS と画像を参照する
 * モックは同じ表示幅でスタイルと画像ごと出て、外すと最後に渡した時点のスナップショットに戻る。JS で描くモックが描かれ、
 * そのスクリプトからトークンは（referrer からも）得られず、トークンを付けても API に断られる。モックの割り当てだけが
 * あるページがツリーに出て、押すとそのページへ移りモックと比べる。
 */
export async function mocksAreAssignedShownAndKeptApart(repository) {
  await mkdir(join(repository, 'mocks'), { recursive: true });
  await writeFile(join(repository, 'mocks', 'mock.html'), '<!doctype html><html><head><link rel="stylesheet" href="mock.css"></head><body><img src="dot.png" alt="" style="display:block;width:40px;height:40px"></body></html>\n');
  await writeFile(join(repository, 'mocks', 'mock.css'), 'html, body { margin: 0; background: rgb(0, 200, 0); }\n');
  const red = Buffer.alloc(4 * 4 * 4);
  for (let i = 0; i < 16; i++) red.set([255, 0, 0, 255], i * 4);
  await writeFile(join(repository, 'mocks', 'dot.png'), encodePng(4, 4, red));
  await writeFile(join(repository, 'mocks', 'notes.txt'), 'not a mock\n');
  const outside = join(dirname(repository), `outside-${basename(repository)}.html`);
  await writeFile(outside, '<p>outside</p>\n');

  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  await withLiveReview({ repository, page: '' }, async ({ kemi, dev, state }) => {
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
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await postJson(kemi.url, 'api/message', { body: 'before the mock' });
    await handInThePage(kemi, repository, state);
    await waitForSnapshot(kemi, handedSnapshot(1));

    for (const path of [`../${basename(outside)}`, 'mocks/notes.txt']) {
      await assignMockByPath(path);
      await waitFor(`${visible(`${mockPanel} .lv-mock-error`)} && document.querySelector('${mockPanel} .lv-mock-error').textContent.trim() !== ''`);
      assert.notEqual(await evaluate(`document.querySelector('${refPane}').dataset.reference`), 'mock', path);
      await browser('press', 'Escape');
      await waitFor(`!${visible(mockPanel)}`);
    }
    console.log('PASS 配れる範囲の外のパスと .txt のファイルは、理由が出て割り当てられない');

    await assignMockByPath('mocks/mock.html');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock' && !${visible(mockPanel)}`);
    // モックの CSS と画像は後から読み込まれるので、両方が描かれるまで撮り直す。
    const mock = await shotUntil(`${refPane} .lv-frame:not([hidden])`, shots, 'mock', (image) => isDeepStrictEqual(pixelAt(image, 200, 200), [0, 200, 0]) && isDeepStrictEqual(pixelAt(image, 10, 10), [255, 0, 0]));
    assert.deepEqual(pixelAt(mock, 200, 200), [0, 200, 0], 'the mock CSS is applied');
    assert.deepEqual(pixelAt(mock, 10, 10), [255, 0, 0], 'the mock image is shown');
    assert.equal(mock.width, 390);
    assert.equal(await evaluate(`document.querySelector('#page-tree .lv-page[data-page="/"] .lv-mock-tag') !== null`), true);
    console.log('PASS CSS と画像を参照するモックを割り当てると、比べる相手がモックになり、同じ表示幅でスタイルと画像ごと出る');

    await chooseFromMockMenu('lv-mock-remove');
    await waitForSnapshot(kemi, handedSnapshot(1));
    console.log('PASS モックを外すと、最後に渡した時点のスナップショットに戻る');

    await evaluate(`window.__mockReports = []; window.addEventListener('message', (event) => { if (event.data && event.data.mockReport) window.__mockReports.push(event.data); }); true`);
    const messagesBefore = (await reviewJson(kemi)).messages.length;
    await assignMockByPath('mocks/script.html');
    await waitFor(`window.__mockReports.length > 0`);
    const report = JSON.parse(await evaluate(`JSON.stringify(window.__mockReports[0])`));
    assert.equal(report.sawToken, false, JSON.stringify(report));
    assert.equal(report.parent, 'blocked', JSON.stringify(report));
    // モックは API への書き込みが終わってから知らせを送るので、断られたかはもう読める。描いた色は撮り直して待つ。
    const drawn = await shotUntil(`${refPane} .lv-frame:not([hidden])`, shots, 'script-mock', (image) => isDeepStrictEqual(pixelAt(image, 200, 200), [0, 0, 200]));
    assert.deepEqual(pixelAt(drawn, 200, 200), [0, 0, 200], 'the mock script runs');
    const messagesAfter = (await reviewJson(kemi)).messages.length;
    assert.equal(messagesAfter, messagesBefore, 'the API refuses the mock even with the token');
    console.log('PASS JS で描くモックが描かれ、そのスクリプトからトークンは得られず、トークンを付けても API に断られる');

    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${dev.url}other.html`.replace(dev.url, `${new URL(kemi.live).origin}/`))}; true`);
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/other.html'`);
    await assignMockByPath('mocks/mock.html');
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
  });
}

/**
 * スナップショットの中の外部の画像（R-PAGE-MOCK、R-PAGE-PROXY）: ページが referrerpolicy="unsafe-url" を
 * 付けていても、画像のサーバにトークンの URL が Referer として届かない。
 */
export async function snapshotsSendNoTokenToExternalImages(repository) {
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
  try {
    await withLiveReview({ repository, page: `referrer.html?image=${encodeURIComponent(image)}` }, async ({ kemi }) => {
      const token = new URL(kemi.url).pathname.split('/')[2];
      const relay = new URL(kemi.live).origin;
      await browser('set', 'viewport', '1280', '900');
      await browser('open', kemi.url);
      await waitForSnapshot(kemi, startSnapshot);
      // 中継のページと写すときの読み込みは中継のオリジンを referrer にする。それ以外がスナップショットの枠から。
      await until(() => received.some(({ referer }) => !referer?.startsWith(relay)), { timeout: 15000, interval: 100 }).catch(() => {});
      assert.ok(received.some(({ referer }) => !referer?.startsWith(relay)), `the snapshot frame loads the image: ${JSON.stringify(received)}`);
      assert.equal(received.some(({ referer }) => referer?.includes(token)), false, JSON.stringify(received));
      console.log('PASS スナップショットの中の外部の画像は、referrerpolicy="unsafe-url" を付けていても Referer にトークンの URL を受け取らない');
    });
  } finally {
    images.closeAllConnections();
    await new Promise((done) => images.close(() => done(undefined)));
  }
}

/**
 * 重ねて透かす（R-PAGE-REF、DC2、R-PAGE-VIEW の狭い画面）: 見る対象をスクロールすると重ねた比べる相手も同じだけ動き
 * （並べる間にスクロールしてから切り替えても）、透かし具合で見え方が変わる。幅 390px では重ねずに 1 枚ずつ切り替える。
 */
export async function overlayFollowsTheScrollAndTheOpacity(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const opacity = setOpacityAndWait;
  await withLiveReview({ repository, page: 'tall.html', viewportAfter: ['1280', '800'] }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`${notRecorded} || ${showsSnapshot(await snapshotTaken(kemi, startSnapshot))}`);
    if (await evaluate(`document.querySelector('${refPane}').dataset.reference`) === 'none') {
      await browser('click', `${refPane} .lv-empty .lv-record`);
      await waitForSnapshot(kemi, recordedSnapshot(1));
    }
    await browser('click', '.lv-mode button[data-compare="overlay"]');
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'overlay' && ${visible('.lv-opacity')}`);

    // 見る対象をスクロールすると、重ねた比べる相手も同じだけ動く。
    // 別のオリジンの枠の中は、ページ内の目印へ移して（同じ文書のままスクロールさせて）動かす。
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = frame.src.split('#')[0] + '#band-3'; return true; })()`);
    await waitFor(`/translate\\(0px, -[1-9]/.test(document.querySelector('${refPane} .lv-frame:not([hidden])').style.transform)`);
    await settled(`${refPane} .lv-frame:not([hidden])`);
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

    // 並べる間にスクロールしてから重ねて透かすへ切り替えても、比べる相手はその位置にそろう（スクロールの知らせで
    // 枠を置き直すのは重ねて透かす間だけなので、切り替えの描き直しが覚えた位置で置く）。並べる間の知らせが届いたかは
    // 見えないので、切り替えて読んで戻すまでを 1 回の評価の中で行い、その間に届く知らせで置き直されたものは読まない。
    const shiftOf = `(transform) => Number(/translate\\(0px, (-?[\\d.]+)px\\)/.exec(transform)?.[1] ?? 0)`;
    const atBand3 = await evaluate(`(${shiftOf})(document.querySelector('${refPane} .lv-frame:not([hidden])').style.transform)`);
    assert.ok(atBand3 < 0, `the overlaid reference moves with the scroll: ${atBand3}`);
    await chooseCompare('side');
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = frame.src.split('#')[0] + '#band-6'; return true; })()`);
    await waitFor(`(() => {
      const choose = (mode) => document.querySelector('.lv-mode button[data-compare="' + mode + '"]').click();
      choose('overlay');
      const shift = (${shiftOf})(document.querySelector('${refPane} .lv-frame:not([hidden])').style.transform);
      choose('side');
      return shift < ${atBand3};
    })()`);
    await chooseCompare('overlay');
    console.log('PASS 並べる間にスクロールしてから重ねて透かすへ切り替えても、比べる相手は切り替えたときにその位置にそろう');

    // 見る対象を変えると、透かし具合で見え方が変わる。
    await writeFile(join(dev.dir, 'tall.html'), (await readFile(join(dev.dir, 'tall.html'), 'utf8')).replaceAll('hsl(', 'hsl(180deg + '));
    // 目印へ移した後の URL（# の付いた今の URL）を入れ直しても、同じ文書の中を移るだけで読み込み直されない。
    await nextPageMessage('page', () => evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = frame.src.split('#')[0]; return true; })()`));
    await settled(`${livePane} .lv-frame`);
    await opacity(0);
    const live0 = await shot(`${livePane} .lv-viewport`, shots, 'changed-0');
    await opacity(100);
    const live100 = await shot(`${livePane} .lv-viewport`, shots, 'changed-100');
    assert.ok((await comparePixels(live0, live100, join(shots, 'changed-diff.png'))).different > 0, 'the opacity changes what is seen');
    console.log('PASS 重ねて透かす表示で、透かし具合を変えると比べる相手の見え方が変わる');

    // 狭い画面では重ねて透かさず、1 枚ずつ切り替えて見る（R-PAGE-VIEW）。
    await browser('set', 'viewport', '390', '800');
    await waitFor(`${visible('.lv-side')} && ${visible(livePane)} && !${visible(refPane)}`);
    await settled('.lv-side button[data-side="ref"]');
    await browser('click', '.lv-side button[data-side="ref"]');
    await waitFor(`${visible(`${refPane} .lv-frame:not([hidden])`)} && !${visible(livePane)} && getComputedStyle(document.querySelector('${refPane} .lv-box')).opacity === '1'`);
    await browser('click', '.lv-side button[data-side="live"]');
    await waitFor(`${visible(livePane)} && !${visible(refPane)}`);
    console.log('PASS 広い画面で重ねて透かすを選んだまま幅 390px にすると、重ねずに 1 枚ずつ切り替えて見る');
  });
}

/**
 * 見比べ方と倍率と読み込み直す操作（R-PAGE-VIEW、R-PAGE-REF）: 開いた直後は見る対象だけで比べる相手の選択が見えず、
 * 並べる・重ねるの間だけ見える。等倍では倍率が 1 と出て、枠の中をスクロールしてページの下端まで見られ、枠に合わせると
 * 縮む。読み込み直す操作で見る対象が読み込み直される。
 */
export async function compareModesScaleAndReload(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  await withLiveReview({ repository, page: 'tall.html' }, async ({ kemi, dev }) => {
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
    await settled(`${refPane} .lv-frame:not([hidden])`);
    await setOpacityAndWait(0);
    const underneath = await shot(`${livePane} .lv-viewport`, shots, 'full-overlay-0');
    await setOpacityAndWait(100);
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
    await until(() => dev.loads('/tall.html') !== before, { timeout: 10000, interval: 100, what: 'the page is loaded again' });
    console.log('PASS 読み込み直す操作を押すと、見る対象のページが読み込み直される');
  });
}

/**
 * 比べる相手の名前と見出し（R-PAGE-REF）と、見る対象だけのときの知らせ・覚える範囲（R-PAGE-VIEW）: 2 回渡してから並べると、
 * 選択に出ている名前は 2 回目に渡した時点の項目の名前を含む。重ねて透かす見出しは、透かし具合の途中・両端と見る対象だけの
 * ときで互いに違う。見る対象だけのまま 2 MB を超えるページを手で取ると、取れなかったことが見える。並べる・等倍にしてから
 * 画面を読み込み直すと、見る対象だけ・枠に合わせるに戻る。
 */
export async function referenceNamesHeadingsAndNotices(repository) {
  const heading = `document.querySelector('.lv-stage-name').textContent`;
  await withLiveReview({ repository, page: 'tall.html' }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/tall.html')`);
    for (const body of ['first', 'second']) {
      await postJson(kemi.url, 'api/message', { body });
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
      await setOpacity(value);
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
  });
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
export async function recordingNowSwitchesTheReference(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot, `document.querySelector('#live-stage').dataset.compare === 'now'`);
    await browser('click', '.lv-band .lv-record-now');
    await waitForSnapshot(kemi, recordedSnapshot(1), `${visible(bandNotice)} && document.querySelector('${bandNotice}').dataset.kind === 'done'`);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare`), 'now', 'recording keeps the way of comparing');
    await browser('click', bandNoticeAction);
    await waitForSnapshot(kemi, recordedSnapshot(1), `document.querySelector('#live-stage').dataset.compare === 'side' && ${visible(refPane)}`);
    console.log('PASS 見る対象だけで手で取ると、見比べ方は変わらず比べる相手が取ったものになって知らせが出て、知らせの操作で並べるとそれが並ぶ');

    await postJson(kemi.url, 'api/message', { body: 'please look' });
    await handInThePage(kemi, repository, state);
    // 渡したときに取ったスナップショットが選択に出てから（比べる相手を選び直す描き直しの後に）、比べる相手を見る。
    await waitForOption(kemi, handedSnapshot(1));
    assert.equal(await evaluate(showsSnapshot(await snapshotTaken(kemi, recordedSnapshot(1)))), true, 'handing does not move the reference away from the recorded snapshot');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, handedSnapshot(1), `document.querySelector('.lv-compare-select').value === 'latest'`);
    console.log('PASS 手で取った後に渡しても比べる相手は取ったもののままで、読み込み直すと自動に戻る');

    await browser('set', 'viewport', '390', '844');
    await browser('open', kemi.url);
    await waitFor(`${visible(livePane)} && ${visible('#live-band .lv-menu-button')} && document.querySelector('#live-stage').dataset.side === 'live'`);
    await browser('click', '#live-band .lv-menu-button');
    await waitFor(visible('.lv-menu .lv-record-now'));
    await evaluate(`document.querySelector('.lv-menu .lv-record-now').click(); document.querySelector('.lv-menu').hidePopover(); true`);
    await waitForSnapshot(kemi, recordedSnapshot(2), `${visible(bandNoticeAction)}`);
    await browser('click', bandNoticeAction);
    await waitForSnapshot(kemi, recordedSnapshot(2), `document.querySelector('#live-stage').dataset.side === 'ref' && ${visible(refPane)}`);
    console.log('PASS 幅 390px で手で取ると知らせが出て、その操作で比べる相手の 1 枚に切り替わり、取ったものが出る');
  });
}

/**
 * 開始時からの変化の案内（R-PAGE-REF、R-PAGE-SNAPSHOT）: 開始時からの変化に案内と比較基準を取り直す操作が出て、記録中は
 * 重ねて押せず、記録とコメントを残して比べる相手を取り直したものに切り替える。手で取った時点や渡した時点との変化には
 * 開始時の案内を出さず、狭い画面の一覧からも取り直せる。記録に失敗したら理由を出し、比べる相手を変えず、また押せる。
 */
export async function changesFromStartExplainHowToCompareFromNow(repository) {
  const guide = '#page-tree .lv-start-comparison';
  const useCurrent = `${guide} button`;
  await withLiveReview({ repository, page: 'changing.html' }, async ({ kemi, dev, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot, `document.querySelector('#page-tree .lv-changes')?.dataset.main === '0'`);
    assert.equal(await evaluate(visible(guide)), false, 'an unchanged page needs no initialization guidance');
    const start = (await getJson(kemi.url, 'api/snapshots')).snapshots.find((item) => item.kind === 'start');
    await writeFile(join(dev.dir, 'changing.css'), `${changingCss()}.buy { transform: translateX(30px); }`);
    await waitFor(`${visible(guide)} && document.querySelector('#page-tree .lv-changes').dataset.main === '0' && Number(document.querySelector('#page-tree .lv-changes').dataset.shifted) > 0`);
    await writeFile(join(dev.dir, 'changing.css'), changingCss());
    await waitFor(`!${visible(guide)} && document.querySelector('#page-tree .lv-changes').dataset.shifted === '0'`);
    await addPageComment(kemi, '/changing.html', 1280, 'keep this comment');
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(214, 69, 69)'));
    await waitFor(`${visible(guide)} && !document.querySelector('${useCurrent}').disabled`);
    const target = await evaluate(`(() => { const r = document.querySelector('${useCurrent}').getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; })()`);
    await browser('mouse', 'move', String(Math.round(target.x)), String(Math.round(target.y)));
    await browser('mouse', 'down');
    await writeFile(join(dev.dir, 'changing.css'), `${changingCss('rgb(214, 69, 69)')}.buy { border: 4px solid rgb(0, 150, 0); }`);
    await waitFor(`document.querySelector('#page-tree .lv-change-main').textContent.includes('border')`);
    await holdRequests('capture');
    await browser('mouse', 'up');
    await waitFor(`document.querySelector('${useCurrent}')?.disabled && document.querySelector('.lv-band .lv-record-now').disabled`);
    await releaseRequests();
    await waitForSnapshot(kemi, recordedSnapshot(1), `!${visible(guide)}`);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare`), 'now', 'choosing a new baseline keeps the comparison mode');
    const recorded = (await getJson(kemi.url, 'api/snapshots')).snapshots;
    assert.ok(recorded.some((item) => item.id === start.id), 'the original Start remains recorded');
    assert.equal(recorded.filter((item) => item.kind === 'manual').length, 1, 'one action creates one new record');
    assert.equal((await reviewJson(kemi)).comments.length, 1, 'changing the baseline keeps existing comments');
    assert.equal(await evaluate(`document.querySelector('${bandNotice}').dataset.kind`), 'done', 'the record in use is confirmed');
    console.log('PASS 開始時からの変化に案内と比較基準を取り直す操作が出て、記録中は重ねて押せず、記録とコメントを残して比較相手を切り替える');

    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`Number(document.querySelector('#page-tree .lv-changes')?.dataset.main) > 0`);
    assert.equal(await evaluate(visible(guide)), false, 'later changes against a manual record are not presented as initialization changes');
    await postJson(kemi.url, 'api/message', { body: 'compare after this hand-over' });
    await handInThePage(kemi, repository, state);
    await chooseAuto();
    await waitForSnapshot(kemi, handedSnapshot(1));
    await writeFile(join(dev.dir, 'changing.css'), changingCss());
    await waitFor(`Number(document.querySelector('#page-tree .lv-changes')?.dataset.main) > 0`);
    assert.equal(await evaluate(visible(guide)), false, 'changes against a handed record are not presented as initialization changes');

    await chooseCompare('side');
    await browser('select', '.lv-compare-select', start.id);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(214, 69, 69)'));
    await waitFor(visible(guide));
    await browser('set', 'viewport', '390', '844');
    await waitFor(`document.body.dataset.liveView === 'page'`);
    await browser('click', '#btn-tree');
    await waitFor(`${visible(useCurrent)} && document.querySelector('#page-tree').getBoundingClientRect().left >= 0`);
    await browser('click', useCurrent);
    await waitForSnapshot(kemi, recordedSnapshot(2), `!${visible(guide)}`);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.side`), 'live', 'recording from the narrow tree does not switch the shown side');
    console.log('PASS 手で取った時点や渡した時点との変化には開始時の案内を出さず、狭い画面の一覧からも比較基準を取り直せる');

    await browser('set', 'viewport', '1280', '900');
    await chooseCompare('side');
    await browser('select', '.lv-compare-select', start.id);
    await writeFile(join(dev.dir, 'changing.html'), `<!doctype html><p style="overflow-wrap:anywhere">${'x'.repeat(2_200_000)}</p>`);
    await waitFor(`${visible(useCurrent)} && !document.querySelector('${useCurrent}').disabled`);
    await browser('click', useCurrent);
    await waitFor(`document.querySelector('${bandNotice}').textContent.includes('larger than 2 MB')`);
    assert.equal(await evaluate(`document.querySelector('.lv-compare-select').value`), start.id, 'failed recording keeps the prior reference');
    assert.equal(await evaluate(`document.querySelector('${useCurrent}').disabled`), false, 'failed recording releases the recording state');
    assert.equal((await getJson(kemi.url, 'api/snapshots')).snapshots.filter((item) => item.kind === 'manual').length, 2, 'a failed record does not create a new baseline');
    console.log('PASS 記録に失敗した場合は理由を出し、比較相手を変えず、再操作できる');
  });
}

/** 空のディレクトリ（git の外）に HTML を書く。 */
async function plainDirectory(files) {
  const dir = await temporaryDirectory('kemi-live-plain-');
  for (const file of files) {
    await mkdir(join(dir, file, '..'), { recursive: true });
    await writeFile(join(dir, file), '<!doctype html><p>plain</p>\n');
  }
  return dir;
}

/**
 * モックのパネルの一覧（R-PAGE-MOCK）: 作業ツリーに追跡中の HTML、無視されない未追跡の HTML、git が無視する HTML、`.git/` の中の
 * HTML、範囲の外を指すシンボリックリンクの HTML、`.txt` を置いて開くと、前の 2 つだけが出て、検索の欄で絞り込める。git が無視する
 * HTML はパスの欄に入れると割り当てられる。git の外の起動したディレクトリの下の HTML が出る。HTML を 201 個置くと 200 件だけが
 * 出て、出している数 200 と全体の数 201 が示され、検索で絞ると出ていなかったファイルが出る。
 */
export async function mockPanelListsTheFilesToChoose() {
  const repository = await makeRepository();
  const git = gitIn(repository);
  const outside = await plainDirectory(['secret.html']);
  await mkdir(join(repository, 'pages'), { recursive: true });
  await writeFile(join(repository, 'pages', 'tracked.html'), '<!doctype html><p>tracked</p>\n');
  await writeFile(join(repository, '.gitignore'), 'pages/ignored.html\n');
  await git('add', 'pages/tracked.html', '.gitignore');
  await git('commit', '-q', '-m', 'tracked page');
  await writeFile(join(repository, 'pages', 'untracked.html'), '<!doctype html><p>untracked</p>\n');
  await writeFile(join(repository, 'pages', 'ignored.html'), '<!doctype html><p>ignored</p>\n');
  await writeFile(join(repository, 'pages', 'notes.txt'), 'not html\n');
  await writeFile(join(repository, '.git', 'inside.html'), '<!doctype html><p>inside</p>\n');
  await symlink(join(outside, 'secret.html'), join(repository, 'pages', 'link.html'));
  const dev = await startDevServer();
  let kemi = await startKemi(repository, await temporaryDirectory('kemi-live-state-'), ['--live', `${dev.url}other.html`]);
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    const options = `JSON.stringify(Array.from(document.querySelector('.lv-compare-select').options).filter((option) => !option.disabled).map((option) => option.value))`;
    assert.equal(JSON.parse(await evaluate(options)).at(-1), 'mock-file', 'the mock file comes last among the choices');
    await openMockPanel();
    assert.ok((await evaluate(`document.querySelector('.lv-compare-select').value`)) !== 'mock-file', 'choosing the mock file opens the panel without changing the choice');
    await waitFor(`${listedMockFiles} === JSON.stringify(['pages/tracked.html', 'pages/untracked.html'])`);
    await browser('fill', `${mockPanel} .lv-mock-search`, 'UNTR');
    await waitFor(`${listedMockFiles} === JSON.stringify(['pages/untracked.html'])`);
    console.log('PASS モックのパネルには追跡中と無視されない未追跡の HTML だけが出て、検索の欄で絞り込める');

    await browser('fill', `${mockPanel} .lv-mock-path`, 'pages/ignored.html');
    await browser('click', `${mockPanel} .lv-mock-path-assign`);
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock' && document.querySelector('${refPane} .lv-bar-label').textContent.includes('pages/ignored.html') && !${visible(mockPanel)}`);
    assert.ok((await evaluate(`document.querySelector('.lv-compare-select').selectedOptions[0].textContent`)).includes('ignored.html'));
    console.log('PASS git が無視する HTML は、パネルのパスの欄に入れると割り当てられ、選択にファイルの名前が出る');
  } finally {
    await stop(kemi);
  }

  const plain = await plainDirectory(['index.html', 'sub/Other.HTML', 'notes.txt']);
  kemi = await startKemi(plain, await temporaryDirectory('kemi-live-state-'), ['--live', `${dev.url}other.html`]);
  try {
    await browser('open', kemi.url);
    await chooseCompare('side');
    await openMockPanel();
    await waitFor(`${listedMockFiles} === JSON.stringify(['index.html', 'sub/Other.HTML'])`);
    console.log('PASS git の外で起動すると、起動したディレクトリの下の HTML が一覧に出る');
  } finally {
    await stop(kemi);
  }

  const many = await plainDirectory(Array.from({ length: 201 }, (_, index) => `m${String(index).padStart(3, '0')}.html`));
  kemi = await startKemi(many, await temporaryDirectory('kemi-live-state-'), ['--live', `${dev.url}other.html`]);
  try {
    await browser('open', kemi.url);
    await chooseCompare('side');
    await openMockPanel();
    const count = `document.querySelector('${mockPanel} .lv-mock-count')`;
    await waitFor(`JSON.parse(${listedMockFiles}).length === 200 && ${count}.dataset.shown === '200' && ${count}.dataset.total === '201'`);
    assert.equal(JSON.parse(await evaluate(listedMockFiles)).includes('m200.html'), false);
    await browser('fill', `${mockPanel} .lv-mock-search`, 'm200');
    await waitFor(`${listedMockFiles} === JSON.stringify(['m200.html'])`);
    console.log('PASS HTML を 201 個置くと 200 件だけが出て、出している数と全体の数が示され、検索で絞ると出ていなかったファイルが出る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * モックのメニュー（R-PAGE-MOCK、R-PAGE-VIEW）: 割り当てたモックを選択の横のメニューで外すと、取り消す操作つきの知らせが出て、
 * 押すと同じモックが比べる相手に戻る。見る対象だけのときは、変化の一覧の代わりに割り当てたファイルの名前の見出しが出て、
 * そこから開いたメニューでモックを外せる。
 */
export async function mockMenuRemovesAndUndoes(repository) {
  await mkdir(join(repository, 'mocks'), { recursive: true });
  await writeFile(join(repository, 'mocks', 'menu-mock.html'), '<!doctype html><p>menu mock</p>\n');
  const mockShown = `document.querySelector('${refPane}').dataset.reference === 'mock' && document.querySelector('${refPane} .lv-bar-label').textContent.includes('mocks/menu-mock.html')`;
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await assignMockByPath('mocks/menu-mock.html');
    await waitFor(mockShown);
    await chooseFromMockMenu('lv-mock-remove');
    await waitForSnapshot(kemi, startSnapshot, `${visible(bandNoticeAction)}`);
    await browser('click', bandNoticeAction);
    await waitFor(mockShown);
    console.log('PASS 割り当てたモックを選択の横のメニューで外すと、取り消す操作つきの知らせが出て、押すと同じモックに戻る');

    await chooseCompare('now');
    const heading = `${changeList}?.querySelector('.lv-changes-vs')?.textContent ?? ''`;
    await waitFor(`(${heading}).includes('menu-mock.html') && document.querySelector('#page-tree .lv-change') === null`);
    await evaluate(`${changeList}.querySelector('.lv-mock-menu-button').click(); true`);
    await waitFor(visible('.lv-mock-menu .lv-mock-remove'));
    await browser('click', '.lv-mock-menu .lv-mock-remove');
    await waitForSnapshot(kemi, startSnapshot, `${changeList}?.dataset.main !== undefined`);
    console.log('PASS 見る対象だけのとき、変化の一覧の見出しに割り当てたファイルの名前が出て、そこから開いたメニューでモックを外せる');

    // 外した後に別のモックを選び直したら、前に外したモックへ戻す操作は残さない（選び直すまで比べる相手を変えない。R-PAGE-REF）。
    await writeFile(join(repository, 'mocks', 'menu-mock-b.html'), '<!doctype html><p>menu mock b</p>\n');
    await waitFor(visible(bandNoticeAction));
    await chooseCompare('side');
    await assignMockByPath('mocks/menu-mock-b.html');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock' && document.querySelector('${refPane} .lv-bar-label').textContent.includes('mocks/menu-mock-b.html')`);
    assert.equal(await evaluate(visible(bandNoticeAction)), false, 'no undo is left that would replace the newly chosen mock');
    console.log('PASS モックを外した後に別のモックを割り当てると、前のモックへ戻す取り消しの操作は残らない');

    // 取り消しが断られたら（外した後にファイルが消えた）、理由を画面に出す（R-PAGE-MOCK）。
    await chooseFromMockMenu('lv-mock-remove');
    await waitForSnapshot(kemi, startSnapshot, `${visible(bandNoticeAction)}`);
    await rm(join(repository, 'mocks', 'menu-mock-b.html'));
    await browser('click', bandNoticeAction);
    await waitFor(`${visible(bandNotice)} && document.querySelector('${bandNotice}').dataset.kind !== 'done' && !${visible(bandNoticeAction)}`);
    assert.equal(await evaluate(showsSnapshot(await snapshotTaken(kemi, startSnapshot))), true, 'the mock that could not be assigned again is not the reference');
    console.log('PASS 外したモックのファイルが消えてから取り消すと、割り当てられず、その理由が帯に出る');

    // 断られた知らせが出ていても、見る対象だけのときに手で取れば、比べる相手がそれに切り替わったことと、それを並べて見る操作が出る（R-PAGE-REF）。
    await chooseCompare('now');
    await browser('click', '.lv-band .lv-record-now');
    await waitForSnapshot(kemi, recordedSnapshot(1), `document.querySelector('${bandNotice}').dataset.kind === 'done' && ${visible(bandNoticeAction)}`);
    console.log('PASS 取り消しが断られた知らせが出ていても、見る対象だけのときに手で取ると、取れたことの知らせとそれを見る操作が出る');
  });
}

/**
 * 並べたときの上端（R-PAGE-REF）: パスが 200 文字のモックを割り当てて並べ、見出しが長くなる幅にしても、両方のページの
 * 上端の差が 1px 以内。
 */
export async function sideBySidePagesShareTheirTop(repository) {
  await mkdir(join(repository, 'mocks'), { recursive: true });
  const path = `mocks/${'m'.repeat(200 - 'mocks/'.length - '.html'.length)}.html`;
  assert.equal(path.length, 200);
  await writeFile(join(repository, path), '<!doctype html><html><body style="margin:0"><p>mock</p></body></html>\n');
  await withLiveReview({ repository, page: 'other.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '900', '800');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await assignMockByPath(path);
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock'`);
    await waitFor(`${visible(`${refPane} .lv-viewport`)} && ${visible(`${livePane} .lv-viewport`)}`);
    const gap = await evaluate(`Math.abs(document.querySelector('${refPane} .lv-viewport').getBoundingClientRect().top - document.querySelector('${livePane} .lv-viewport').getBoundingClientRect().top)`);
    assert.ok(gap <= 1, `the tops differ by ${gap}px`);
    console.log(`PASS パスが 200 文字のモックを並べても、両方のページの上端の差が 1px 以内（${gap}px）`);
  });
}

/**
 * 保留と復元（R-PAGE-SESSION、R-PAGE-REF、R-PAGE-MOCK）: コメントと手で取ったスナップショットのあるレビューを保留して
 * 復元すると、比べる相手の選択に開始時と手で取ったものが出て、開始時が既定になる。開始時のものは保留の前と同じ id と
 * HTML で 1 つだけ（取り直さない）。モックを割り当てて保留し、モックのファイルを消してから復元すると、読めない旨が出る。
 */
export async function snapshotsAndMocksComeBackAfterResuming(repository) {
  await mkdir(join(repository, 'mocks'), { recursive: true });
  const mockFile = join(repository, 'mocks', 'resumed.html');
  await writeFile(mockFile, '<!doctype html><p>mock to be deleted</p>\n');
  const dev = await startDevServer();
  const state = await temporaryDirectory('kemi-live-state-');
  let kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  let start;
  try {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-band .lv-record-now');
    await waitForOption(kemi, recordedSnapshot(1));
    const [listed] = (await getJson(kemi.url, 'api/snapshots')).snapshots.filter((snapshot) => snapshot.kind === 'start');
    start = await getJson(kemi.url, `api/snapshot/${listed.id}`);
    await addPageComment(kemi, '/rich.html', 1280, 'keep this review');
    await postJson(kemi.url, 'api/mock', { page: '/other.html', path: 'mocks/resumed.html' });
  } finally {
    await stop(kemi);
  }
  await rm(mockFile);

  kemi = await startKemi(repository, state, ['--resume', kemi.id]);
  try {
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot, listedOption(await snapshotTaken(kemi, recordedSnapshot(1))));
    await waitFor(`document.querySelector('#page-tree .lv-page[data-page="/rich.html"] .lv-comment-count')?.textContent === '1'`);
    // 動いているページが読み込まれた後も、開始時のものを取り直さない。取り直すなら、ページの読み込みの知らせの後に
    // 写しを頼んで預ける。預けないことには合図が無いので、読み込まれてからその間を待つ。
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden`);
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
