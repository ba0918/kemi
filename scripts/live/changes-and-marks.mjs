// `--live` のブラウザ検査の場面: 変化の一覧と印（scripts/test-live.mjs から走らせる）。

import assert from 'node:assert/strict';
import { mkdir, mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { visible } from '../lib/browser.mjs';
import { postJson } from '../lib/kemi.mjs';
import { comparePixels, countPixels } from '../lib/png.mjs';
import {
  CHANGING_GEOMETRY,
  SCROLLING_GEOMETRY,
  TABLE_GEOMETRY,
  adoptedCss,
  changingCss,
  changingPage,
  manyCss,
  scrollingCss,
  tablePage,
} from '../live-dev-server.mjs';
import {
  browser,
  evaluate,
  waitFor,
  withLiveReview,
  handInThePage,
  clickUncovered,
  shot,
  refPane,
  livePane,
  chooseAuto,
  startSnapshot,
  recordedSnapshot,
  handedSnapshot,
  waitForSnapshot,
  waitForOption,
  notRecorded,
  setOpacity,
  settled,
  countPageMessages,
  nextPageMessage,
  seenMessages,
  wheelOn,
  assignMockByPath,
  chooseFromMockMenu,
  chooseCompare,
  changeList,
  isBlue,
  shotUntil,
  waitForPixels,
  chooseReference,
  isGlow,
} from './common.mjs';

/** 数え始めてから最後に届いたスクロールの知らせの、ページの縦の位置を読む式。まだ無ければ undefined。 */
const lastScrollY = 'window.__kemiScrollY';

const isRed = ([r, g, b]) => r > 180 && g < 120 && b < 120;
const isGreen = ([r, g, b]) => r < 80 && g > 120 && b < 120;
const isPurple = ([r, g, b]) => r > 150 && r < 210 && g > 140 && g < 190 && b > 195;

/** 見る対象だけの見比べ方でも、変化の一覧と見る対象の側の印が出る（R-PAGE-VIEW、R-PAGE-REF）。 */
export async function changesShowWithThePageAlone(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  await withLiveReview({ repository, page: 'changing.html' }, async ({ kemi, dev }) => {
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
  });
}

/**
 * 表示中のページの変化の一覧の、消えた要素の最初の行。無ければ null（`?.` だけだと一覧がまだ無いときに undefined に
 * なり、`!== null` の待ちが一覧の描かれる前に通って、押しても何も起きない）。
 */
const removedRow = `(${changeList}?.querySelector('.lv-change-main .lv-change[data-kind="removed"]') ?? null)`;

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
export async function changeRowsAreElementsThatLeadToThePage(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const frame = `${livePane} .lv-frame`;
  const wheel = (y) => wheelOn(`${livePane} .lv-capture`, { y, at: 100 });
  await withLiveReview({ repository, page: 'changing.html' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/changing.html')`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${frame}').style.width === '390px' && document.querySelector('${livePane} .lv-notice').hidden && document.querySelector('${refPane}').dataset.reference === 'none'`);
    await browser('click', '.lv-band .lv-record-now');
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.css'), `${changingCss('rgb(214, 69, 69)')}.buy { color: rgb(20, 20, 20); border: 4px solid rgb(0, 150, 0); }\n`);
    // 一覧が今の CSS（変えた背景色）と比べたものになるまで待つ。
    await waitFor(`${changeList}?.dataset.main === '1' && ${mainRows}.length > 0 && ${backgroundsIn(`${mainRows}[0]`)}.includes('rgb(214, 69, 69)')`);
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
    await chooseCompare('side');
    const chosen = await evaluate(`document.querySelector('.lv-compare-select').selectedOptions[0].textContent`);
    assert.ok(chosen.includes(name), `the heading name ${JSON.stringify(name)} is in the choice ${JSON.stringify(chosen)}`);
    await chooseAuto();
    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitForSnapshot(kemi, startSnapshot);
    await chooseCompare('now');
    await waitFor(`(${changeList}?.querySelector('.lv-changes-vs')?.textContent ?? ${JSON.stringify(name)}) !== ${JSON.stringify(name)}`);
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
  });
}

/** /changing.html の末尾の余白の下に、下の方の要素（#low）を足したもの。 */
const changingWithLow = () => changingPage().replace('<div class="tail"></div>', '<div class="tail"></div><p id="low" style="margin:0;height:60px">Low element</p>');

/** /changing.html の末尾の要素（#end）の高さ。 */
const END_HEIGHT = 40;

/** /changing.html の末尾の余白の下に、#low とは別の要素（#end）を足したもの。 */
const changingWithEnd = () => changingPage().replace('<div class="tail"></div>', `<div class="tail"></div><div id="end" style="margin:0;height:${END_HEIGHT}px">End element</div>`);

/**
 * 消えた要素の行（R-PAGE-VIEW、R-PAGE-DIFF、R-PAGE-SNAPSHOT）: スナップショットにあって今のページから消した、下の方の要素が
 * 一覧に出る。見る対象だけのときにその行を押すと並べる見比べ方になり、比べる相手の枠の中でその要素が見える位置にあって
 * 光っている（ずらした形の間も比べる相手をホイールで動かせ、見比べ方を変えると元の形に戻る）。幅 390px では比べる相手の
 * 1 枚に切り替わって同じになる。重ねて透かすときは見比べ方を変えず、その要素が見える位置で光り、見る対象と比べる相手の
 * 位置がそろったまま。
 */
export async function removedRowsLeadToTheSnapshot(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const refView = `${refPane} .lv-viewport`;
  const endRow = `(Array.from(${changeList}?.querySelectorAll('.lv-change-main .lv-change') ?? []).find((item) => item.textContent.includes('End element')) ?? null)`;
  // 枠の上の方（ページの先頭にある青いボタンが写る所）を除いた範囲。ボタンの青は光の色に近い。
  const belowTheButton = [0, 250, 100000, 100000];
  const pressRemovedRow = () => evaluate(`${removedRow}.querySelector('button').click(); true`);
  await withLiveReview({ repository, page: 'changing.html' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/changing.html')`);
    // 下の方の要素を足し、ページの中身が差し替わったことを開始時との一覧で見てから、幅 390px で取る。
    await waitForSnapshot(kemi, startSnapshot);
    await writeFile(join(dev.dir, 'changing.html'), changingWithLow());
    await waitFor(`${mainRows}.some((item) => item.dataset.kind === 'added' && item.textContent.includes('Low element'))`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px' && document.querySelector('${livePane} .lv-notice').hidden`);
    await browser('click', '.lv-band .lv-record-now');
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
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
    await wheelOn(`${refPane} .lv-ref-wheel`, { y: -300, at: 50 });
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
    await waitForSnapshot(kemi, recordedSnapshot(1), `${removedRow} !== null`);
    await pressRemovedRow();
    await waitForPixels(refView, shots, 'removed-narrow', belowTheButton, isGlow);
    await waitForPixels(refView, shots, 'removed-narrow-mark', belowTheButton, isRed);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.side === 'ref' && ${visible(refPane)}`), true);
    console.log('PASS 幅 390px で同じ行を押すと、比べる相手の 1 枚に切り替わり、その要素が見えて光る');

    // 重ねて透かす間に押す、今のページの末尾の要素（増えた要素の行になる）。
    await writeFile(join(dev.dir, 'changing.html'), changingWithEnd());
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(visible(livePane));
    await evaluate(at390);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${removedRow} !== null && ${endRow} !== null`);
    await chooseCompare('overlay');
    await waitFor(`${removedRow} !== null`);
    await pressRemovedRow();
    // 消えた要素は今のページの末尾より下にあるが、光は枠の中に収まる（ページがそこまでスクロールできなくても）。
    await waitFor(`(() => { const v = document.querySelector('${refView}').getBoundingClientRect(); const g = document.querySelector('${refView} .lv-ref-glow'); if (!g || g.hidden) return false; const r = g.getBoundingClientRect(); return r.top >= v.top && r.bottom <= v.bottom; })()`, 2500);
    await waitForPixels(refView, shots, 'removed-overlay', belowTheButton, isGlow);
    assert.equal(await evaluate(`document.querySelector('#live-stage').dataset.compare`), 'overlay');
    // 重ねた比べる相手は、見る対象のスクロールの分だけ外側でずらして描く（そろったまま）。見る対象が下へスクロールしている。
    const shift = await evaluate(`new DOMMatrix(getComputedStyle(document.querySelector('${refPane} .lv-frame:not([hidden])')).transform).m42`);
    assert.ok(shift < -1000, `the overlaid snapshot follows the page scrolled down to the element: ${shift}`);
    console.log('PASS 重ねて透かすときに同じ行を押すと、見比べ方は変わらず、そろったままその要素の位置までスクロールして光る');

    // 末尾より先へずらした後に今のページの末尾の要素の行を押すと、ずらすのをやめ、その要素が枠の下端に見えて光る
    // （ずらしたままだと光は要素より上に描かれる）。重ねた比べる相手は見えなくして、見る対象の光だけを見る。
    await setOpacity(0);
    const liveView = `${livePane} .lv-viewport`;
    const bottom = await evaluate(`document.querySelector('${liveView}').clientHeight`);
    await evaluate(`${endRow}.querySelector('button').click(); true`);
    await waitForPixels(liveView, shots, 'end-after-overscroll', [0, bottom - END_HEIGHT - 20, 100000, bottom], isGlow);
    console.log('PASS 重ねて透かして末尾より先へずらした後に、今のページの末尾の要素の行を押すと、その要素の所で光る');
  });
}

/** 幅 1280 の文書の右下の方に要素（#far）を置いたページ（/tall.html を置き換えて使う）。`far` が false なら置かない。 */
const farPage = (far) => `<!doctype html><html><head><meta charset="utf-8"><script type="module" src="/__hmr.js"></script></head>
<body style="margin:0;width:1280px;height:1600px;position:relative">
<p style="margin:0;padding:20px">Far page</p>
${far ? '<div id="far" style="position:absolute;left:1100px;top:900px;width:120px;height:60px;background:rgb(0, 120, 0)">Far right</div>' : ''}
</body></html>
`;

/**
 * 等倍で枠より広いページ（R-PAGE-DIFF、R-PAGE-COMMENT）: 右の方にあった要素を消してその行を押すと、比べる相手の枠が横にも
 * その要素まで動き、光がその枠の中に見える。右の方の場所を持つコメントの「ページで見る」を押すと、見る対象の枠が横にも
 * その場所まで動く。
 */
export async function wideTargetsAreRevealedSideways(repository) {
  const liveView = `document.querySelector('${livePane} .lv-viewport')`;
  const refView = `document.querySelector('${refPane} .lv-viewport')`;
  const glowInside = `(() => { const v = ${refView}.getBoundingClientRect(); const g = document.querySelector('${refPane} .lv-ref-glow'); if (!g || g.hidden) return false; const r = g.getBoundingClientRect(); return r.left >= v.left && r.right <= v.right && r.top >= v.top && r.bottom <= v.bottom; })()`;
  const prepare = (dev) => writeFile(join(dev.dir, 'tall.html'), farPage(true));
  await withLiveReview({ repository, page: 'tall.html', prepare }, async ({ kemi, dev }) => {
    const far = await postJson(kemi.url, 'api/comment', {
      op: 'add_page',
      page: { url: '/tall.html', width: 1280, places: [{ n: 1, kind: 'arrow', points: [{ x: 1080, y: 1000 }, { x: 1200, y: 1060 }], elements: [] }] },
      body: 'far to the right',
    });
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/tall.html')`);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-zoom button[data-zoom="full"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.transform === 'scale(1)' && ${liveView}.scrollWidth > ${liveView}.clientWidth`);
    await writeFile(join(dev.dir, 'tall.html'), farPage(false));
    await waitFor(`${removedRow} !== null && ${removedRow}.textContent.includes('Far right')`);
    assert.equal(await evaluate(`${refView}.scrollLeft`), 0);
    await evaluate(`${removedRow}.querySelector('button').click(); true`);
    await waitFor(`${refView}.scrollLeft > 0 && ${glowInside}`);
    console.log('PASS 等倍で枠より広いページの右の方の消えた要素の行を押すと、比べる相手の枠が横にもその要素まで動き、光が枠の中に見える');

    // 横に動いた後も、比べる相手の見えている所の上で回したホイールで、ずらした比べる相手が動く（枠の真ん中で受ける）。
    const refFrameNow = `document.querySelector('${refPane} .lv-frame:not([hidden])')`;
    const shifted = `new DOMMatrix(getComputedStyle(${refFrameNow}).transform).m42`;
    const before = await evaluate(shifted);
    await wheelOn(`${refPane} .lv-viewport`, { y: -300 });
    await waitFor(`${shifted} > ${before}`);
    console.log('PASS 横に動いた後も、比べる相手の見えている所の上のホイールでずらした比べる相手が動く');

    await evaluate(`${liveView}.scrollLeft = 0; true`);
    await waitFor(`${liveView}.scrollLeft === 0`);
    await clickUncovered('#cv-rail');
    await browser('click', `.cv-card[data-id="${far.id}"]`);
    await waitFor(`document.querySelector('#cv-thread .cv-go:not(.cv-page-width)') !== null`);
    await browser('click', '#cv-thread .cv-go');
    await waitFor(`${liveView}.scrollLeft > 0`);
    console.log('PASS 等倍で枠より広いページの右の方の場所を持つコメントの「ページで見る」を押すと、見る対象の枠が横にもその場所まで動く');
  });
}

/** 幅 3000 の文書の左の方に要素（#near）を置いたページ（/tall.html を置き換えて使う）。`near` が false なら置かない。 */
const scrollingWidePage = (near) => `<!doctype html><html><head><meta charset="utf-8"><script type="module" src="/__hmr.js"></script></head>
<body style="margin:0;width:3000px;height:1600px;position:relative">
<p style="margin:0;padding:20px">Wide page</p>
${near ? '<div id="near" style="position:absolute;left:600px;top:300px;width:120px;height:60px;background:rgb(0, 120, 0)">Near the edge</div>' : ''}
</body></html>
`;

/**
 * 重ねて透かす等倍で、動いているページの中を横にスクロールしているとき（R-PAGE-DIFF）: 消えた要素の行を押すと、重ねた比べる
 * 相手の中でその要素が描かれている所が見えるよう枠を合わせ、光が枠の中に見える（重ねた比べる相手はページの横のスクロールの分だけ
 * 左にずれている）。
 */
export async function overlaidRemovedElementsAreRevealedWhereTheyAreDrawn(repository) {
  const refView = `document.querySelector('${refPane} .lv-viewport')`;
  const refFrameNow = `document.querySelector('${refPane} .lv-frame:not([hidden])')`;
  const glowInside = `(() => { const v = ${refView}.getBoundingClientRect(); const g = document.querySelector('${refPane} .lv-ref-glow'); if (!g || g.hidden) return false; const r = g.getBoundingClientRect(); return r.left >= v.left && r.right <= v.right && r.top >= v.top && r.bottom <= v.bottom; })()`;
  const prepare = (dev) => writeFile(join(dev.dir, 'tall.html'), scrollingWidePage(true));
  await withLiveReview({ repository, page: 'tall.html', prepare }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '900', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/tall.html')`);
    await chooseCompare('overlay');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-zoom button[data-zoom="full"]');
    await writeFile(join(dev.dir, 'tall.html'), scrollingWidePage(false));
    await waitFor(`${removedRow} !== null && ${removedRow}.textContent.includes('Near the edge')`);
    // 動いているページの中を横にスクロールする。重ねた比べる相手はその分だけ左にずれる。
    await evaluate(`document.querySelector('${livePane} .lv-frame').contentWindow.postMessage({ kemi: 'live', type: 'scroll-by', x: 400, y: 0 }, '*'); true`);
    await waitFor(`new DOMMatrix(getComputedStyle(${refFrameNow}).transform).m41 === -400`);
    await evaluate(`${removedRow}.querySelector('button').click(); true`);
    await waitFor(glowInside);
    console.log('PASS 重ねて透かす等倍で動いているページを横にスクロールしているとき、消えた要素の行を押すと、その要素が描かれている所の光が枠の中に見える');
  });
}

/**
 * 差分（R-PAGE-DIFF、R-PAGE-VIEW の変化の一覧、R-PAGE-REF の一覧はスナップショットのときだけ）: スナップショットを取った後に
 * 同じ URL の中身を変えると、読み込み直さずに変化の一覧が変わる。兄弟の途中に足した要素だけが増えたになり、ボタンの
 * 背景色の変化が前後の色の見本つきで主な変化に入る。モックと比べる間は一覧が出ず、外すと出る。変化の数は表示中のページに
 * だけ出て、別のページへ移ると前のページの一覧を新しいページの下に出さない。幅 390px では引き出しの中にある。幅 390px で
 * 比べる相手の側を見ている（動いているページの枠が隠れる）間はページに記述を頼まず、変化の数が変わらず、動いているページの側に
 * 戻すと比べ直す。
 * 枠が隠れている間に取ったスナップショットも、コードの見方から渡したときのスナップショットも、変えていないページとは変化 0。
 */
export async function changeListFollowsThePage(repository) {
  await mkdir(join(repository, 'mocks'), { recursive: true });
  await writeFile(join(repository, 'mocks', 'diff-mock.html'), '<!doctype html><html><body><p>mock</p></body></html>\n');
  await withLiveReview({ repository, page: 'changing.html', viewportAfter: ['1280', '800'] }, async ({ kemi, dev, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await evaluate(`window.__kemiNotReloaded = true; true`);
    await waitForSnapshot(kemi, startSnapshot);
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

    await postJson(kemi.url, 'api/message', { body: 'please look' });
    await handInThePage(kemi, repository, state);
    await waitForSnapshot(kemi, handedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    await assignMockByPath('mocks/diff-mock.html');
    // モックと比べている間は、一覧の代わりにモックの見出しだけが出る（変化の行も数も無い）。
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock' && document.querySelector('#page-tree .lv-change') === null && ${changeList}?.dataset.main === undefined`);
    await chooseFromMockMenu('lv-mock-remove');
    await waitForSnapshot(kemi, handedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    console.log('PASS モックを割り当てたページでは変化の一覧が出ず、外すと最後に渡した時点のスナップショットと比べた一覧（変化 0）が出る');

    await evaluate(`document.querySelector('${livePane} .lv-frame').src = ${JSON.stringify(`${new URL(kemi.live).origin}/other.html`)}; true`);
    await waitFor(`document.querySelector('#page-tree .lv-page[data-current="true"]')?.dataset.page === '/other.html' && ${notRecorded}`);
    await browser('click', '.lv-record');
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
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
    // 幅をまたいだ直後は上部バーの並びが変わるので、押すボタンが動かなくなってから押す。
    await settled('#btn-tree');
    await browser('click', '#btn-tree');
    await waitFor(`document.querySelector('#page-tree').dataset.drawer === 'open' && ${visible('#page-tree .lv-changes')}`);
    console.log('PASS 幅 390px で開くと、変化の一覧が引き出しの中にある');

    // 比べる相手の側を見ている間は動いているページの枠が隠れる。その間は変化の数を変えず（隠れた文書と比べない）、
    // 隠れている間にページが変わっても、動いているページの側に戻してから比べ直す。
    const counts = `${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`;
    const shown = await evaluate(counts);
    assert.equal(shown, '1/0');
    // 比べ直すきっかけはページの変化の知らせなので、知らせが届いてから数を見る。隠すとページの大きさが変わり、
    // ページはまずそれを知らせる。
    await nextPageMessage('changed', () => evaluate(`(() => {
      window.__kemiCounts = [];
      new MutationObserver(() => {
        const list = ${changeList};
        window.__kemiCounts.push(list ? list.dataset.main + '/' + list.dataset.shifted : 'none');
      }).observe(document.querySelector('#page-tree'), { childList: true, subtree: true, attributes: true });
      document.querySelector('.lv-side button[data-side="ref"]').click();
      return true;
    })()`));
    await waitFor(`!${visible(`${livePane} .lv-frame`)}`);
    const described = await evaluate(seenMessages('described'));
    // 最後に渡した時点の色に戻す。比べ直せば変化は 0 になる。
    await nextPageMessage('changed', () => writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(214, 69, 69)')));
    // 届いた知らせは比べ直すきっかけでしかなく、比べ直したら記述を頼む往復を挟んでから数が変わる。比べ直さないことには
    // 合図が無いので、間を置いてから見る。
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(seenMessages('described')), described, 'the review page asks the hidden live page for no description');
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
    await waitForOption(kemi, recordedSnapshot(2));
    await chooseReference(kemi, recordedSnapshot(2));
    await evaluate(`document.querySelector('.lv-side button[data-side="live"]').click(); true`);
    await waitFor(`${visible(`${livePane} .lv-frame`)} && ${changeList} !== null`);
    // 選び直した後に比べ直すかは記述が古いかで決まり、比べ直さないときは知らせが無いので、間を置いてから数を見る。
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(counts), '0/0', 'a snapshot recorded while the live page is hidden shows no change against the unchanged page');
    console.log('PASS 幅 390px で比べる相手の側を見ている間に取ったスナップショットと、変えていないページを比べると変化が 0');

    // コードの見方から渡すとき（舞台ごと隠れている）に取るスナップショットも同じ。
    await browser('set', 'viewport', '1280', '800');
    await postJson(kemi.url, 'api/message', { body: 'once more' });
    await browser('click', '.lv-view button[data-view="code"]');
    await waitFor(`!${visible('#live-stage')}`);
    await handInThePage(kemi, repository, state);
    await waitForOption(kemi, handedSnapshot(2));
    await browser('click', '.lv-view button[data-view="page"]');
    await chooseReference(kemi, handedSnapshot(2));
    // 選び直した後に比べ直すかは記述が古いかで決まり、比べ直さないときは知らせが無いので、間を置いてから数を見る。
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(counts), '0/0', 'a snapshot taken when handing from the code view shows no change against the unchanged page');
    console.log('PASS コードの見方から渡したときのスナップショットと、変えていないページを比べると変化が 0');
    assert.equal(await evaluate(`window.__kemiNotReloaded === true`), true, 'the review page was not reloaded');
  });
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

/**
 * 印（R-PAGE-VIEW の変わったところに印、R-PAGE-DIFF の控えめな印、R-PAGE-REF、R-LIVE のスクロール位置）: 主な変化と
 * 増えた要素は動いているページの側に、消えた要素はスナップショットの側に印が付き、変わっていない要素には付かない。印を
 * 付けても変化は増えず、その後に取ったスナップショットとは変化 0。重ねて透かす表示でも同じ印。モックと比べる間は付かない。
 * 動いているページの側の印だけを付け直すときは、比べる相手の枠のスクロール位置は変わらない。同じページを読み込み直しても、
 * 消えた要素の組が同じなら変わらない。
 */
export async function marksFollowTheChanges(repository) {
  await mkdir(join(repository, 'mocks'), { recursive: true });
  await writeFile(join(repository, 'mocks', 'diff-mock.html'), '<!doctype html><html><body><p>mock</p></body></html>\n');
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const liveFrame = `${livePane} .lv-frame`;
  const refFrame = `${refPane} .lv-frame:not([hidden])`;
  await withLiveReview({ repository, page: 'changing.html' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);

    let regions = changingRegions(false);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    // 印はページが比べ直した後に付くので、付くまで撮り直す。
    let image = await shotUntil(liveFrame, shots, 'marks-button', (shown) => countPixels(shown, regions.button, isRed) > 0);
    assert.ok(countPixels(image, regions.button, isRed) > 0, `the changed button is marked: ${join(shots, 'marks-button.png')}`);
    assert.equal(countPixels(image, regions.note, isRed), 0, 'the element below the button is not marked');
    console.log('PASS ボタンの背景色を変えると、そのボタンに主な変化の印が付き、その下の変わっていない要素には付かない');

    const counts = `${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`;
    const marked = await evaluate(counts);
    // 印が変化に数えられるなら、ページが間を置いて変化を知らせて比べ直される。知らせが来ないことには合図が無い。
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(counts), marked, 'marking adds no change');
    await browser('click', '.lv-band .lv-record-now');
    await waitForOption(kemi, recordedSnapshot(2));
    await chooseReference(kemi, recordedSnapshot(2));
    await waitFor(`${changeList}?.dataset.main === '0' && ${changeList}.dataset.shifted === '0'`);
    console.log('PASS 印を付けた後も変化の数が変わらず、印を付けた後に取ったスナップショットと比べると変化が 0');

    await chooseReference(kemi, recordedSnapshot(1));
    await writeFile(join(dev.dir, 'changing.css'), changingCss());
    await waitFor(`${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.html'), changingPage(['one', 'two', 'inserted', 'three', 'four']));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    regions = changingRegions(true);
    image = await shotUntil(liveFrame, shots, 'marks-inserted', (shown) => countPixels(shown, regions.item(2), isGreen) > 0 && countPixels(shown, regions.item(3), isPurple) > 0);
    assert.ok(countPixels(image, regions.item(2), isGreen) > 0, `the inserted element is marked as added: ${join(shots, 'marks-inserted.png')}`);
    for (const index of [3, 4]) {
      assert.equal(countPixels(image, regions.item(index), isRed), 0, `sibling ${index} after the inserted one has no main mark`);
      assert.equal(countPixels(image, regions.item(index), isGreen), 0, `sibling ${index} after the inserted one has no added mark`);
    }
    assert.ok(countPixels(image, regions.item(3), isPurple) > 0, 'a sibling that only shifted has the quiet mark');
    console.log('PASS 兄弟の途中に要素を足すと、足した要素に増えたの印が付き、後ろの兄弟には主な変化の印が付かない');

    await assignMockByPath('mocks/diff-mock.html');
    await waitFor(`document.querySelector('${refPane}').dataset.reference === 'mock'`);
    // 足した要素に付いていた印が外れるのを待つ。
    image = await waitForPixels(liveFrame, shots, 'marks-mock', regions.item(2), isGreen, false);
    assert.equal(countPixels(image, regions.item(2), isGreen), 0, 'no mark while comparing with a mock');
    await chooseFromMockMenu('lv-mock-remove');
    await chooseReference(kemi, recordedSnapshot(1));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    console.log('PASS モックと比べている間は印が付かない');

    await browser('click', '.lv-band .lv-record-now');
    await waitForOption(kemi, recordedSnapshot(3));
    await chooseReference(kemi, recordedSnapshot(3));
    await waitFor(`${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.html'), changingPage());
    await waitFor(`${changeList}?.dataset.main === '1'`);
    const removedAt = changingRegions(true).item(2);
    await waitForPixels(refFrame, shots, 'marks-removed', removedAt, isRed);
    assert.equal(await evaluate(`${changeList}.querySelector('.lv-changes-unmarked')`), null, 'no notice that removed elements are not marked');
    console.log('PASS 兄弟の途中の要素を消すと、スナップショットの側のその要素に消えたの印が付く');

    await browser('click', '.lv-mode button[data-compare="overlay"]');
    await waitFor(`document.querySelector('#live-stage').dataset.compare === 'overlay'`);
    await setOpacity(100);
    await waitForPixels(refFrame, shots, 'marks-overlay-removed', removedAt, isRed);
    await setOpacity(0);
    regions = changingRegions(false);
    image = await shotUntil(liveFrame, shots, 'marks-overlay', (shown) => countPixels(shown, regions.item(2), isPurple) > 0);
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
    // 比べる相手の枠が作り直されると、読み込まれて先頭が描かれるまで間がある。作り直されないことには合図が無い。
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
  });
}

/** 変化の印を隠すアイコン（R-PAGE-VIEW）。隠すのは印だけで、一覧と行を押したときの光は残る。 */
export async function marksCanBeHidden(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const liveFrame = `${livePane} .lv-frame`;
  const refFrame = `${refPane} .lv-frame:not([hidden])`;
  const marksButton = `document.querySelector('.lv-marks')`;
  await withLiveReview({ repository, page: 'changing.html' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    // 兄弟を足し、ページの中身が差し替わったことを開始時との一覧で見てから、幅 390px で取る。
    await writeFile(join(dev.dir, 'changing.html'), changingPage(['one', 'two', 'inserted', 'three', 'four']));
    await waitFor(`${mainRows}.some((item) => item.dataset.kind === 'added' && item.textContent.includes('inserted'))`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.html'), changingPage());
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '2'`);
    const button = changingRegions(false).button;
    const removedAt = changingRegions(true).item(2);
    await waitForPixels(liveFrame, shots, 'shown-live', button, isRed);
    await waitForPixels(refFrame, shots, 'shown-ref', removedAt, isRed);
    assert.equal(await evaluate(`${marksButton}.getAttribute('aria-pressed')`), 'true', 'marks are shown at first');

    await browser('click', '.lv-marks');
    await waitFor(`${marksButton}.getAttribute('aria-pressed') === 'false'`);
    await waitForPixels(liveFrame, shots, 'hidden-live', button, isRed, false);
    await waitForPixels(refFrame, shots, 'hidden-ref', removedAt, isRed, false);
    assert.equal(await evaluate(`${changeList}.dataset.main`), '2', 'the change list stays while the marks are hidden');
    console.log('PASS 印を隠すアイコンを押すと、見る対象の側とスナップショットの側の印が消え、変化の一覧と数は残る');

    await evaluate(`${changeList}.querySelector('.lv-change-main .lv-change[data-kind="removed"] button').click(); true`);
    await waitFor(`${visible(`${refPane} .lv-ref-glow`)}`, 2500);
    console.log('PASS 印を隠したまま消えた要素の行を押すと、スナップショットの側でその要素が光る');

    await browser('click', '.lv-marks');
    await waitFor(`${marksButton}.getAttribute('aria-pressed') === 'true'`);
    await waitForPixels(liveFrame, shots, 'again-live', button, isRed);
    await waitForPixels(refFrame, shots, 'again-ref', removedAt, isRed);
    console.log('PASS 印を隠すアイコンをもう一度押すと、両方の側の印が戻る');
  });
}

/**
 * インラインのスタイルを止める CSP のページでも印が付く（R-PAGE-VIEW の変わったところに印。
 * R-PAGE-PROXY は script-src のほかの CSP を変えない）。
 */
export async function marksAreDrawnUnderAStrictStylePolicy(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  await withLiveReview({ repository, page: 'changing.html?csp=1' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    const image = await shotUntil(`${livePane} .lv-frame`, shots, 'csp-marks', (shown) => countPixels(shown, changingRegions(false).button, isRed) > 0);
    assert.ok(countPixels(image, changingRegions(false).button, isRed) > 0, `the changed button is marked: ${join(shots, 'csp-marks.png')}`);
    console.log("PASS style-src 'self' の CSP を返すページでも、変わった要素に印が付く");
  });
}

/**
 * 文字列を HTML として読む API を Trusted Types で止める CSP のページでも、スナップショットが撮れて変化の一覧が出る
 * （R-PAGE-SNAPSHOT。R-PAGE-PROXY は script-src のほかの CSP を変えない）。
 */
export async function snapshotsAreTakenUnderTrustedTypes(repository) {
  await withLiveReview({ repository, page: 'changing.html?tt=1' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'changing.css'), changingCss('rgb(250, 200, 0)'));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    console.log("PASS require-trusted-types-for 'script' の CSP を返すページでも、スナップショットが撮れて変化の一覧が出る");

    // このページのスナップショットは要素の対応を持たないので、消えた要素の印は付けられない。一覧には出し、そのことを出す。
    // Trusted Types は試験用の HMR の body の差し替え（DOMParser）も止めるので、ページを読み込み直して反映する。
    await writeFile(join(dev.dir, 'changing.html'), changingPage(['one', 'two', 'four']));
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = frame.src; return true; })()`);
    await waitFor(`${changeList}?.querySelector('.lv-change[data-kind="removed"]') && ${changeList}.querySelector('.lv-changes-unmarked') !== null`);
    console.log("PASS require-trusted-types-for 'script' の CSP を返すページで要素を消すと、一覧に出し、スナップショットに印を付けられないことを出す");
  });
}

/**
 * HTML として読み直すと要素の並びが変わるページでも、消えた要素の印がスナップショットのその要素に付く
 * （R-PAGE-VIEW の変わったところに印）。スクリプトが tbody を挟まずに組んだ表の後ろの兄弟を消す。
 */
export async function removedMarksSurviveReparsing(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const { row, item } = TABLE_GEOMETRY;
  await withLiveReview({ repository, page: 'table.html' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    await writeFile(join(dev.dir, 'table.html'), tablePage(['one', 'two', 'four']));
    await waitFor(`${changeList}?.dataset.main === '1'`);
    await waitForPixels(`${refPane} .lv-frame:not([hidden])`, shots, 'table-removed', [0, row + 2 * item, 390, row + 3 * item], isRed);
    console.log('PASS スクリプトが組んだ表（読み直すと tbody が足される）の後ろの要素を消しても、スナップショットのその要素に消えたの印が付く');
  });
}

/**
 * スクロールしただけでは変化にならず、印はスクロールしても要素に付いたまま（R-PAGE-DIFF の位置と大きさの
 * 変化、R-PAGE-VIEW の変わったところに印）。上に張り付く見出し、画面に固定した札、中でスクロールする箱で。並べる表示の
 * 比べる相手は、画面に固定した要素も動いているページと同じ場所に出る。
 */
export async function scrollingMakesNoChangeAndMarksStay(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  // 別のオリジンの枠の中は、ページ内の目印へ移して（同じ文書のままスクロールさせて）動かす。
  const moveTo = (anchor) => evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = frame.src.split('#')[0] + '#${anchor}'; return true; })()`);
  const { box, row, scrollMargin, badge } = SCROLLING_GEOMETRY;
  await withLiveReview({ repository, page: 'scrolling.html' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0' && ${changeList}.dataset.shifted === '0'`);

    // 並べる表示の比べる相手は、見えている高さで描いて中でスクロールする（動いているページと同じ見え方）。
    const badgeAt = [badge.left, badge.top, badge.left + badge.width, badge.top + badge.height];
    const isBadge = ([r, g, b]) => r === 120 && g === 120 && b === 120;
    const unscrolled = await shot(`${refPane} .lv-viewport`, shots, 'unscrolled-reference');
    await pageDownInReference();
    const reference = await shotUntil(`${refPane} .lv-viewport`, shots, 'scrolled-reference', async (shown) => (await comparePixels(unscrolled, shown, join(shots, 'scrolled-reference-diff.png'))).different > 0);
    assert.ok((await comparePixels(unscrolled, reference, join(shots, 'scrolled-reference-diff.png'))).different > 0, 'the snapshot scrolled');
    assert.ok(countPixels(reference, badgeAt, isBadge) > badge.width * badge.height * 0.5, `the fixed badge stays in place in the scrolled snapshot: ${join(shots, 'scrolled-reference.png')}`);
    console.log('PASS 並べる表示で比べる相手をスクロールしても、画面に固定した要素は動いているページと同じ場所に描かれる');

    // 箱の中を 6 行目まで、ページを末尾までスクロールしてから、札と 6 行目の色を変えて比べ直させる。
    await countPageMessages();
    await moveTo('row-6');
    await moveTo('end');
    // 末尾の目印へ移ると、ページが下までスクロールしたことを知らせる（箱の中だけのスクロールは知らせない）。
    await waitFor(`${lastScrollY} > ${badge.top}`);
    await writeFile(join(dev.dir, 'scrolling.css'), scrollingCss({ badge: 'rgb(40, 160, 220)', row: 'rgb(250, 200, 0)' }));
    await waitFor(`${changeList}?.dataset.main === '2'`);
    // ページは変化を間を置いてまとめて知らせ、続けて知らせることもある。もう比べ直さないことには合図が無い。
    await new Promise((done) => setTimeout(done, 1500));
    assert.equal(await evaluate(`${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`), '2/0', 'scrolling alone shifts nothing');
    console.log('PASS ページと中の箱をスクロールしただけでは、張り付く見出し・固定した札・箱の中の要素が変化に入らない');

    // ページを先頭へ戻す。札は画面の同じ場所に、6 行目は箱の中のスクロールで箱の上端から scroll-margin-top 下に見える。
    // 先頭へ戻ったことは印の位置で見る。CSS の差し替えの間にページが先頭へ戻っていることがあり、そのときは
    // スクロールの知らせが来ない。
    await moveTo('start');
    const rowAt = [0, box.top + scrollMargin, 390, box.top + scrollMargin + row];
    let image = await shotUntil(`${livePane} .lv-frame`, shots, 'scrolled-marks', (shown) => countPixels(shown, badgeAt, isRed) > 0 && countPixels(shown, rowAt, isRed) > 0);
    assert.ok(countPixels(image, badgeAt, isRed) > 0, `the fixed badge keeps its mark: ${join(shots, 'scrolled-marks.png')}`);
    assert.ok(countPixels(image, rowAt, isRed) > 0, `the row in the scrolled box keeps its mark: ${join(shots, 'scrolled-marks.png')}`);

    // 箱の中を先頭へ戻すと、6 行目（箱の中の上から 5 行ぶん下）は箱の見えている範囲の外に出て、その印も見えなくなる。
    await moveTo('row-1');
    image = await shotUntil(`${livePane} .lv-frame`, shots, 'box-scrolled-back', (shown) => countPixels(shown, [0, 0, 390, badge.top], isRed) === 0);
    assert.equal(countPixels(image, [0, 0, 390, badge.top], isRed), 0, `the mark of the row hidden in the box is not drawn: ${join(shots, 'box-scrolled-back.png')}`);
    assert.ok(countPixels(image, badgeAt, isRed) > 0, 'the fixed badge still has its mark');
    console.log('PASS スクロールしても、固定した札と中でスクロールする箱の要素の印は要素に付いたまま');
  });
}

/**
 * DOM を変えずに CSSOM だけで見た目が変わるページ（構築したスタイルシートの replaceSync）でも、変化の一覧が
 * 今のページに合わせて変わる（R-PAGE-DIFF、R-PAGE-VIEW の変わったところに印）。
 */
export async function cssomChangesAreFollowed(repository) {
  await withLiveReview({ repository, page: 'adopted.html' }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot, `${changeList}?.dataset.main === '0'`);
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
  });
}

/**
 * 表示幅を切り替えても文書の幅が変わらないページ（<html> の min-width より狭い幅どうし）でも、切り替えた幅の
 * 変化の一覧が出る（R-PAGE-VIEW の今の表示幅での変化の一覧）。
 */
export async function widthSwitchesWithoutResizingTheDocument(repository) {
  await withLiveReview({ repository, page: 'wide.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot, `${changeList}?.dataset.main === '0'`);
    await browser('click', '.lv-widths button[data-width="768"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(2), `${changeList}?.dataset.main === '0'`);
    // 390 で取ったものに切り替わっているので、768 では記録されていない。768 で取ったものを選び直す。
    await browser('click', '.lv-widths button[data-width="768"]');
    await waitFor(notRecorded);
    await chooseReference(kemi, recordedSnapshot(1));
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0'`);
    console.log('PASS 表示幅を変えても文書の幅が変わらないページでも、切り替えた幅の変化の一覧が出る');
  });
}

/**
 * 要素の多いページ（R-PAGE-SNAPSHOT の 2 MB は HTML に掛ける）: HTML が 2 MB 未満のページは、要素の記述が
 * 大きくても（要素が 7 万を超えても）取れて、変化の一覧が出る。一覧の残りも続きを出す操作ですべて見られる。
 */
export async function manyElementsAreRecordedAndCompared(repository) {
  const cards = 18000;
  const page = `many.html?cards=${cards}`;
  const prepare = async (dev) => {
    const html = await (await fetch(`${dev.url}${page}`)).text();
    assert.ok(new Blob([html]).size < 2 * 1024 * 1024, 'the page is under 2 MB');
  };
  await withLiveReview({ repository, page, prepare }, async ({ kemi, dev }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot, `${changeList}?.dataset.main === '0'`, 120000);
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
  });
}
