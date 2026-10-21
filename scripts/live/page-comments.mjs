// `--live` のブラウザ検査の場面: ページへのコメントと画像（scripts/test-live.mjs から走らせる）。

import assert from 'node:assert/strict';
import { mkdtemp, readFile, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { visible } from '../lib/browser.mjs';
import { temporaryDirectory } from '../lib/fixture.mjs';
import { getJson, postJson } from '../lib/kemi.mjs';
import { comparePixels, countPixels, crop, decodePng, pixelAt } from '../lib/png.mjs';
import {
  browser,
  evaluate,
  waitFor,
  startDevServer,
  startKemi,
  withLiveReview,
  handInThePage,
  startWaiting,
  pressHand,
  clickUncovered,
  stop,
  shot,
  refPane,
  livePane,
  startSnapshot,
  recordedSnapshot,
  handedSnapshot,
  waitForSnapshot,
  waitForOption,
  notRecorded,
  STAGE_SCALE,
  stageScale,
  settled,
  nextPageMessage,
  wheelOn,
  clickInPane,
  chooseCompare,
  liveScale,
  isLastBand,
  changeList,
  shotUntil,
  waitForPixels,
  chooseReference,
  draftNumbers,
  chooseTool,
  reviewJson,
  holdRequests,
  heldRequests,
  releaseRequests,
  addPageComment,
  isGlow,
  savePageCommentInThePage,
} from './common.mjs';

/**
 * 画面の「Hand to agent」で渡し、別のプロセスの `kemi wait` が返した JSON を読む。
 */
async function handAndWait(kemi, dir, state) {
  const waiting = startWaiting(kemi, dir, state);
  await pressHand();
  const { code, stdout } = await waiting;
  assert.equal(code, 0, 'kemi wait returns what was handed');
  return JSON.parse(stdout);
}

/** 式が真になるのを待つ。期限までに真にならなければ、message（何を確かめていたかと、そのときの値）で落とす。 */
async function eventually(code, message) {
  try {
    await waitFor(code);
  } catch (error) {
    throw new Error(typeof message === 'function' ? await message() : message, { cause: error });
  }
}

/**
 * 道具の既定と始め方の案内（R-PAGE-COMMENT）: 開いた直後の道具は要素で、ページを押すと場所が置かれる。道具を選び直した
 * だけでは書く欄が開かず、場所を置くと開く。案内はページへのコメントを保存すると消え、そのコメントを消しても出ない。
 * 要素の道具のまま等倍にしても、横にも下端までもスクロールできる。
 */
export async function toolsAndTheHintStartTheFirstComment(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const compose = '#live-compose';
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
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
      // 書く欄は道具を押された状態にする描き直しで一緒に描く（chooseTool がその状態を待つ）。
      await chooseTool(tool);
      assert.equal(await evaluate(visible(compose)), false, `choosing ${tool} opens no comment box`);
    }
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]' && ${visible(compose)}`);
    console.log('PASS 道具を選び直しただけでは書く欄が開かず、場所を置くと開く');

    await savePageCommentInThePage('the button');
    await waitFor(`!${visible('.lv-hint')}`);
    const [saved] = (await reviewJson(kemi)).comments;
    await postJson(kemi.url, 'api/comment', { op: 'delete', id: saved.id });
    await browser('open', kemi.url);
    // 案内を出すかは画面を開いたときに決まり、操作の帯を描くとき（舞台の見比べ方を入れるのと同じ描き直し）に描く。
    await waitFor(`${visible('.lv-tools')} && document.querySelector('#comment-count').textContent === '0' && document.querySelector('#live-stage').dataset.compare !== undefined`);
    assert.equal(await evaluate(visible('.lv-hint')), false, 'the hint does not come back after the comment is deleted');
    console.log('PASS 案内はページへのコメントを保存すると消え、そのコメントを消して開き直しても出ない');
  });

  await withLiveReview({ repository, page: 'tall.html' }, async ({ kemi: tall }) => {
    await browser('open', tall.url);
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden && ${visible('.lv-tools')}`);
    assert.equal(await evaluate(`document.querySelector('.lv-tools button[aria-pressed="true"]')?.dataset.tool`), 'element');
    await browser('click', '.lv-zoom button[data-zoom="full"]');
    await waitFor(`${liveScale} === 1`);
    const viewport = `document.querySelector('${livePane} .lv-viewport')`;
    const height = await evaluate(`Math.floor(${viewport}.getBoundingClientRect().height)`);
    // 道具の層の上で回したホイール。
    await wheelOn(`${livePane} .lv-capture`, { x: 300, at: 100 });
    await waitFor(`${viewport}.scrollLeft > 0`);
    await wheelOn(`${livePane} .lv-capture`, { y: 6000, at: 100 });
    await waitForPixels(`${livePane} .lv-viewport`, shots, 'element-tool-bottom', [0, height - 60, 300, height - 30], isLastBand);
    assert.equal(await evaluate(draftNumbers), '[]', 'scrolling puts no place');
    console.log('PASS 要素の道具のまま等倍にしても、横にも下端までもスクロールできる');
  });
}

/** 見る対象の枠の、縮める前の高さ（ページの CSS ピクセル）。 */
const liveFrameHeight = `document.querySelector('${livePane} .lv-frame').offsetHeight`;

/** 見る対象の枠の外側の高さを、縮めた倍率で割ったもの（枠がそこまで伸びているべき高さ）。 */
const liveRoomHeight = `(document.querySelector('${livePane} .lv-viewport').clientHeight / ${STAGE_SCALE})`;

/**
 * 書く欄を閉じた後の枠の高さ（R-PAGE-COMMENT）: 場所を置いて書く欄を開き、保存しても取り消しても、見る対象の枠の高さは
 * 欄を開く前と同じ。最初の保存で始め方の案内が消えたときも、枠は空いた高さまで伸びる。欄を開いている間に枠を並べ直す
 * きっかけ（道具を選び直す）があっても同じ。
 */
export async function closingTheComposeBoxRestoresTheFrame(repository) {
  const compose = '#live-compose';
  const sameHeight = (expected) => `Math.abs(${liveFrameHeight} - ${expected}) <= 1`;
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden && ${visible('.lv-tools')} && ${visible('.lv-hint')}`);
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]' && ${visible(compose)}`);
    await chooseTool('arrow');
    await savePageCommentInThePage('the first comment');
    await waitFor(`!${visible('.lv-hint')} && !${visible(compose)}`);
    await eventually(sameHeight(liveRoomHeight), async () => `the frame grows into the room the hint left: ${await evaluate(liveFrameHeight)} for ${await evaluate(liveRoomHeight)}`);
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
      await eventually(sameHeight(before), async () => `after ${close}, the frame is as high as before the box opened: ${await evaluate(liveFrameHeight)} for ${before}`);
    }
    console.log('PASS 書く欄を開いて保存しても取り消しても、見る対象の枠の高さは欄を開く前と同じ');
  });
}

/**
 * 会話パネルを畳んだときの渡す案内（R-AGENT-STATE、R-PAGE-COMMENT）: 案内は書く欄を覆わず、コマンドを写せ、場所の×も押せる。
 * 書く欄を閉じると案内は畳んだ帯の位置に戻る。
 */
export async function foldedHandNoteLeavesTheComposeControlsClickable(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]' && !document.querySelector('#rail-hand-note').hidden`);
    const layout = await evaluate(`(() => {
      const note = document.querySelector('#rail-hand-note').getBoundingClientRect();
      const compose = document.querySelector('#live-compose').getBoundingClientRect();
      const remove = document.querySelector('#live-compose .lv-place-remove');
      const r = remove.getBoundingClientRect();
      const hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
      return { noteBottom: note.bottom, composeTop: compose.top, removeUncovered: hit === remove || remove.contains(hit) };
    })()`);
    assert.ok(layout.noteBottom <= layout.composeTop, `the folded hand note must stay above the compose box: ${JSON.stringify(layout)}`);
    assert.ok(layout.removeUncovered, 'the remove control can receive pointer input');
    await browser('click', '#rail-hand-note .hand-copy');
    await waitFor(`document.querySelector('#rail-hand-note .hand-copy').textContent === 'Copied'`);
    await browser('click', '#live-compose .lv-place-remove');
    await waitFor(`${draftNumbers} === '[]' && document.querySelector('#live-compose').hidden`);
    const after = await evaluate(`document.querySelector('#rail-hand-note').getBoundingClientRect().bottom`);
    assert.ok(after > layout.noteBottom, 'the note returns to the folded rail position when the compose box closes');
    console.log('PASS 会話を畳んだ案内はコメント欄を覆わず、コマンドを写せ、場所の×も押せる');
  });
}

/** コメントの画像の頼みに、ページが答えるまで待つ上限（agent-browser の評価は 30 秒で打ち切られ、理由が分からない）。 */
const IMAGE_TIMEOUT = 20000;

/**
 * 中継したページにコメントの画像（R-PAGE-COMMENT）を頼み、返った PNG の base64 か、作れなかった理由を返す。
 * 道具の画面を通さず、レビュー画面と同じオリジンから枠へ頼みを送る。
 */
async function askImage(rect, places) {
  const reply = await evaluate(`new Promise((done) => {
    const frame = document.querySelector('${livePane} .lv-frame');
    const id = 'image-' + Math.random();
    const timer = setTimeout(() => {
      removeEventListener('message', listen);
      done(JSON.stringify({ unanswered: true }));
    }, ${IMAGE_TIMEOUT});
    const listen = (event) => {
      const data = event.data;
      if (event.source !== frame.contentWindow || data?.kemi !== 'live' || data.type !== 'imaged' || data.id !== id) return;
      removeEventListener('message', listen);
      clearTimeout(timer);
      done(JSON.stringify({ png: data.png ?? null, error: data.error ?? null }));
    };
    addEventListener('message', listen);
    const page = new URL(frame.src);
    const view = { page: page.pathname + page.search, width: parseFloat(frame.style.width), height: frame.clientHeight };
    frame.contentWindow.postMessage({ kemi: 'live', type: 'image', id, ...view, rect: ${JSON.stringify(rect)}, places: ${JSON.stringify(places)} }, page.origin);
  })`);
  const answer = JSON.parse(reply);
  // 返事が来なかったことを、作れなかった理由（error）と取り違えない。
  assert.notEqual(answer.unanswered, true, `the page did not answer the image request within ${IMAGE_TIMEOUT} ms`);
  return answer;
}

/**
 * コメントの画像（R-PAGE-COMMENT の画像の段落）: ページの中で写しを描いて作った描き込み無しの画像を、動いているページの
 * 同じ範囲の画素と比べる。細部の違いは仕様が認めるので、差の割合と差の画像を出して人の確認に回す。描き込みを重ねた
 * 画像も残す。インラインのスタイルを止める CSP と、Trusted Types を求める CSP のページでは、画像が作れるか、作れない
 * 理由が返るかを確かめる。
 */
export async function commentImagesLookLikeThePage(repository) {
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
  ];
  for (const { page, rect, places } of pages) {
    await withLiveReview({ repository, page }, async ({ kemi }) => {
      await browser('set', 'viewport', '1280', '900');
      await browser('open', kemi.url);
      await waitForSnapshot(kemi, startSnapshot);
      await browser('click', '.lv-widths button[data-width="390"]');
      await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
      await settled(`${livePane} .lv-frame`);
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
    });
  }

  for (const query of ['csp=1', 'tt=1']) {
    await withLiveReview({ repository, page: `changing.html?${query}` }, async ({ kemi }) => {
      await browser('set', 'viewport', '1280', '900');
      await browser('open', kemi.url);
      await waitForSnapshot(kemi, startSnapshot);
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
    });
  }
}

/** 埋め込めない資源があるとき、不完全な画像を返さず、本文と場所だけを保存する（R-PAGE-COMMENT）。 */
export async function unreadableResourcesMakeNoPartialCommentImage(repository, only = null) {
  const cases = ['resources', 'import', 'link', 'background', 'image', 'canvas'];
  assert.ok(only === null || cases.includes(only), 'unknown resource test case');
  for (const resource of cases.filter((name) => only === null || name === only)) {
    await unembeddedResourceMakesNoImage(repository, resource);
  }
}

async function unembeddedResourceMakesNoImage(repository, resource) {
  const page = resource === 'resources' ? 'resources.html' : 'other.html';
  const prepare = async (dev) => {
    const cross = dev.url.replace('127.0.0.1', 'localhost');
    const sources = {
      import: `<style>@import url("${cross}cross-imported.css");</style>`,
      link: `<link rel="stylesheet" href="${cross}cross.css">`,
      background: '<style>.box { background: url("/missing.png"); }</style>',
      image: '<img src="/missing.png" alt="missing resource">',
      canvas: `<canvas id="tainted" width="80" height="80"></canvas><script>const image=new Image();image.onload=()=>{document.querySelector('#tainted').getContext('2d').drawImage(image,0,0);document.body.dataset.ready='true'};image.src='${cross}cross.svg';</script>`,
    };
    if (resource !== 'resources') {
      await writeFile(join(dev.dir, page), `<!doctype html><html><head><style>body{margin:0}.box{width:80px;height:80px}</style>${sources[resource]}</head><body><div class="box cross cross-imported">Target</div></body></html>`);
    }
  };
  await withLiveReview({ repository, page, prepare }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    const snapshot = (await getJson(kemi.url, 'api/snapshots')).snapshots.find((entry) => entry.kind === 'start');
    const html = (await getJson(kemi.url, `api/snapshot/${snapshot.id}`)).html;
    if (resource !== 'canvas') {
      const remaining = resource === 'resources' || resource === 'link' ? 'cross.css' : resource === 'import' ? 'cross-imported.css' : 'missing.png';
      assert.ok(html.includes(remaining), 'the HTML snapshot retains the resource URL');
    }
    const made = await askImage({ x: 0, y: 0, w: 390, h: 200 }, []);
    assert.ok(made.png === null, `${resource}: a resource that cannot be embedded must not produce a partial PNG`);
    assert.ok(made.error?.length > 0, 'the missing resource has a reason');
    await chooseTool('element');
    await clickInPane(livePane, 40, 40);
    await waitFor(`${draftNumbers} === '[1]'`);
    await browser('fill', '#live-compose .lv-compose-body', 'Keep the body and the place');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`document.querySelector('#live-compose').hidden`);
    const saved = (await reviewJson(kemi)).comments;
    assert.equal(saved.length, 1);
    assert.equal(saved[0].body, 'Keep the body and the place');
    assert.equal(saved[0].page.places.length, 1);
    assert.equal(saved[0].page.image, null);
    assert.equal(await evaluate(`document.body.textContent.includes('The comment was saved without its image')`), true);
    if (resource === 'resources') {
      const shots = await mkdtemp(join(tmpdir(), 'kemi-live-image-refused-'));
      await browser('screenshot', join(shots, 'saved-without-image.png'));
      console.log(`CHECK incomplete image refused; saved comment and notice: ${join(shots, 'saved-without-image.png')}`);
    }
    console.log(`PASS ${resource}: image returns a reason without PNG, HTML snapshot remains available, and UI saves the body/place with a notice`);
  });
}

/**
 * コメントの画像の見た目の幅と高さ（R-PAGE-COMMENT の画像の段落）: 画像にする範囲が表示幅より狭く、画面の高さと違っても、
 * 動いているページと同じ表示幅と画面の高さで並べた写しを描く（幅で変わる CSS と vh が範囲の大きさで変わらない）。
 */
export async function commentImagesKeepTheLayoutOfTheViewport(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-images-'));
  await withLiveReview({ repository, page: 'responsive.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
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
  });
}

/** 枠の中の点を順にたどって描く（押して、動かして、離す）。 */
async function dragInPane(pane, points) {
  const box = await evaluate(`(() => { const r = document.querySelector('${pane} .lv-frame').getBoundingClientRect(); return { x: r.x, y: r.y }; })()`);
  const scale = await stageScale();
  const at = ([x, y]) => [String(Math.round(box.x + x * scale)), String(Math.round(box.y + y * scale))];
  await browser('mouse', 'move', ...at(points[0]));
  await browser('mouse', 'down');
  for (const point of points.slice(1)) await browser('mouse', 'move', ...at(point));
  await browser('mouse', 'up');
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
export async function pageCommentPlacesArePutAndSaved(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await evaluate(`window.__kemiErrors = []; addEventListener('error', (event) => window.__kemiErrors.push(String(event.message))); addEventListener('unhandledrejection', (event) => window.__kemiErrors.push(String(event.reason))); true`);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューで押せるので、先に一度渡しておく。
    await postJson(kemi.url, 'api/message', { body: 'let me show you' });
    await handInThePage(kemi, repository, state);

    await chooseTool('element');
    await clickInPane(refPane, 150, 250);
    // 比べる相手の側を押しても場所の頼みは送られず、何も起きないことには合図が無い。
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
    // 押した要素（左上 (0, 200) の 300×100 のボタン）の周りに描いた枠。
    const drawn = await shotUntil(`${livePane} .lv-frame`, shots, 'places-drawn', (shown) => countPixels(shown, [0, 195, 305, 305], isPlaceInk) > 0);
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

    await clickUncovered('#cv-rail');
    await waitFor(`document.querySelector('#conversation').dataset.open === 'true'`);
    // 例外は描いた後の非同期の処理からも出る。出ないことには合図が無いので、その間を待つ。
    await new Promise((done) => setTimeout(done, 300));
    assert.deepEqual(JSON.parse(await evaluate('JSON.stringify(window.__kemiErrors)')), [], 'the review page raises no error with a page comment');
    console.log('PASS ページへのコメントを保存した後も、会話パネルを開いて例外が出ない');
  });
}

/**
 * ペンの場所の要素（R-PAGE-COMMENT）: 囲んだ範囲を丸ごと含む外側の要素は入らず、1 つの要素の内側だけを囲むと、範囲を
 * 含む一番内側の要素 1 つになる。
 */
export async function penPlacesLeaveOutWhatContainsTheLine(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
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
  });
}

/**
 * 光の枠と画像の範囲（R-PAGE-COMMENT）: 大きな要素を拾ったペンでも、光の枠と画像は描いた範囲に収まる。要素の枠は要素の箱、
 * 矢印の枠は線と先端の要素の範囲になる。
 */
export async function penGlowAndImageStayAroundTheDrawnArea(repository) {
  const prepare = (dev) => writeFile(join(dev.dir, 'rich.html'), `<!doctype html><html><head><style>
    body { margin:0; background:#161a20; color:#e4e8ee; font:16px sans-serif; }
    header { height:60px; background:#222731; }
    #layout { display:flex; height:1000px; }
    nav { width:260px; flex:none; padding:20px; box-sizing:border-box; background:#222731; }
    main { flex:1; }
    button { display:block; width:180px; height:40px; margin:0 0 20px; }
  </style></head><body><header>Header</header><div id="layout"><nav><button>First</button><button>Second</button></nav><main>Content</main></div></body></html>`);
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const frame = `${livePane} .lv-frame`;
  await withLiveReview({ repository, page: 'rich.html', prepare }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await evaluate(`window.addEventListener('message', (event) => {
      if (event.data?.kemi !== 'live') return;
      if (event.data.type === 'placed') window.__boundsPlace = event.data;
      if (event.data.type === 'glowed') window.__boundsGlow = event.data.rect;
    }); true`);
    await keepSavedImage();
    await chooseTool('pen');
    // ヘッダーと左の列をまたぐ囲み。拾われる要素には、描線より幅の広い入れ物も含まれる。
    await dragInPane(livePane, [[20, 40], [220, 40], [220, 260], [20, 260], [20, 42]]);
    await waitFor(`${draftNumbers} === '[1]' && window.__boundsPlace?.kind === 'pen'`);
    const pen = await evaluate('window.__boundsPlace');
    const width = Math.max(...pen.points.map((point) => point.x)) - Math.min(...pen.points.map((point) => point.x));
    const height = Math.max(...pen.points.map((point) => point.y)) - Math.min(...pen.points.map((point) => point.y));
    assert.ok(pen.elements.some(({ rect }) => rect.w > width), 'the selected metadata reaches outside the drawn pen area');
    await browser('click', '#live-compose .lv-place-show');
    await waitFor('window.__boundsGlow !== undefined');
    const glow = await evaluate('window.__boundsGlow');
    assert.ok(Math.abs(glow.w - width) < 0.05, `the pen glow follows the drawn width (${glow.w} vs ${width}), not selected elements`);
    assert.ok(Math.abs(glow.h - height) < 0.05, `the pen glow follows the drawn height (${glow.h} vs ${height})`);
    const scale = await stageScale();
    await waitForPixels(frame, shots, 'pen-outline', [Math.round((width + 20) * scale), 50, Math.round((width + 30) * scale), 150], isGlow);
    await savePageCommentInThePage('the drawn pen area');
    const image = decodePng(Buffer.from(await evaluate('window.__kemiSavedImage'), 'base64'));
    assert.ok(image.width < 1280, 'the pen image is cropped around the drawing instead of spanning the page');
    console.log('PASS 大きな要素を拾ったペンでも、光の枠と画像は描いた範囲に収まる');

    await evaluate('window.__boundsGlow = undefined; window.__boundsPlace = undefined; true');
    await chooseTool('element');
    await clickInPane(livePane, 110, 100);
    await waitFor(`${draftNumbers} === '[1]' && window.__boundsPlace?.kind === 'element'`);
    const element = await evaluate('window.__boundsPlace.elements[0].rect');
    await browser('click', '#live-compose .lv-place-show');
    await waitFor('window.__boundsGlow !== undefined');
    const elementGlow = await evaluate('window.__boundsGlow');
    assert.equal(elementGlow.w, element.w);
    assert.equal(elementGlow.h, element.h);
    await browser('click', '#live-compose .lv-compose-cancel');

    await evaluate('window.__boundsGlow = undefined; window.__boundsPlace = undefined; true');
    await chooseTool('arrow');
    await dragInPane(livePane, [[320, 220], [250, 160], [110, 100]]);
    await waitFor(`${draftNumbers} === '[1]' && window.__boundsPlace?.kind === 'arrow'`);
    const arrow = await evaluate('window.__boundsPlace');
    await browser('click', '#live-compose .lv-place-show');
    await waitFor('window.__boundsGlow !== undefined');
    const arrowGlow = await evaluate('window.__boundsGlow');
    const target = arrow.elements[0].rect;
    assert.ok(Math.abs(arrowGlow.w - (Math.max(arrow.points[0].x, target.x + target.w) - Math.min(arrow.points.at(-1).x, target.x))) < 0.05);
    assert.ok(Math.abs(arrowGlow.h - (Math.max(arrow.points[0].y, target.y + target.h) - Math.min(arrow.points.at(-1).y, target.y))) < 0.05);
    console.log('PASS 要素の枠は要素の箱、矢印の枠は線と先端の要素の範囲を維持する');
  });
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
export async function placesOnTheBackgroundAreAreasOnly(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
    await postJson(kemi.url, 'api/message', { body: 'let me show you' });
    await handInThePage(kemi, repository, state);
    await keepSavedImage();
    // ボタン（下端 300）と 2 段の並びより下は、どの要素も描かれていない余白。
    await chooseTool('element');
    // 押した所の場所をページに頼み、ページの答えが届いてから、場所が増えていないことを見る。
    await nextPageMessage('placed', () => clickInPane(livePane, 350, 450));
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
  });
}

/**
 * ページの上の印（R-PAGE-COMMENT）: c1 を保存してから新しいコメントを書き始めると、c1 の場所には番号が出ず、書きかけの
 * 場所にだけ番号が出る。要素の道具のまま c1 の印にポインタを乗せると、レビュー画面に c1 の短い名前が出る。c1 のスレッドを
 * 開くと c1 の場所に番号が出る。番号の札と描き込みはページの中の紫で見る（印の層は閉じた shadow root の中で、別の
 * オリジンの枠なので要素は読めない）。
 */
export async function savedCommentsAreQuietMarks(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  // ボタン（左上 (0, 200) の 300×100）の左上の番号の札の辺りと、2 段目の入力欄（左上 (12, 96) 付近）の辺り。
  const buttonCorner = [0, 185, 40, 230];
  const fieldCorner = [0, 80, 60, 125];
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await savePageCommentInThePage('The button label is too long\nsecond line');
    const [c1] = (await reviewJson(kemi)).comments;
    await clickInPane(livePane, 40, 105);
    await waitFor(`${draftNumbers} === '[1]' && document.querySelector('#live-compose .lv-place[data-n="1"]').dataset.selector === '#field'`);
    // 書きかけの場所の番号が描かれてから、保存した場所に番号が無いことを見る。
    const drafting = await shotUntil(`${livePane} .lv-frame`, shots, 'saved-and-draft', (shown) => countPixels(shown, fieldCorner, isPlaceInk) > 0);
    assert.equal(countPixels(drafting, buttonCorner, isPlaceInk), 0, `the saved comment shows no number: ${join(shots, 'saved-and-draft.png')}`);
    assert.ok(countPixels(drafting, fieldCorner, isPlaceInk) > 0, `the draft place shows its number: ${join(shots, 'saved-and-draft.png')}`);
    console.log('PASS c1 を保存してから新しいコメントを書き始めると、c1 の場所に番号が出ず、書きかけの場所にだけ番号が出る');

    // 要素の道具の層にポインタの動きを送る（agent-browser のマウスは層の上の動きを確かに届けられないため）。
    const scale = await stageScale();
    await evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame').getBoundingClientRect(); const layer = document.querySelector('${livePane} .lv-capture'); return layer.dispatchEvent(new PointerEvent('pointermove', { pointerId: 7, clientX: frame.x + 300 * ${scale}, clientY: frame.y + 200 * ${scale}, bubbles: true })); })()`);
    await waitFor(`${visible(`${livePane} .lv-saved-tip`)} && document.querySelector('${livePane} .lv-saved-tip').textContent.includes('The button label is too long')`);
    assert.equal(await evaluate(draftNumbers), '[1]', 'moving the pointer puts no place');
    console.log('PASS 要素の道具のまま c1 の印にポインタを乗せると、レビュー画面に c1 の短い名前が出る');

    await clickUncovered('#cv-rail');
    await browser('click', `.cv-card[data-id="${c1.id}"]`);
    await waitFor(`document.querySelector('#conversation').dataset.open === 'true'`);
    const opened = await stageScale();
    const corner = buttonCorner.map((value) => Math.round(value * opened));
    await waitForPixels(`${livePane} .lv-frame`, shots, 'saved-focused', corner, isPlaceInk);
    console.log('PASS c1 のスレッドを開くと、c1 の場所に番号が出る');
  });
}

/**
 * 書きかけのコメントの場所の URL と表示幅（R-PAGE-COMMENT）: 別の表示幅では場所が足されず、書きかけの URL と表示幅に
 * 戻る操作で戻ると場所を足せ、保存したコメントはその表示幅を持つ。
 */
export async function draftPlacesStayAtTheirWidth(repository) {
  const firstSelector = `document.querySelector('#live-compose .lv-place[data-n="1"]')?.dataset.selector`;
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]' && ${firstSelector} === '#button'`);

    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '1280px' && ${visible('#live-compose .lv-compose-back')}`);
    await settled(`${livePane} .lv-frame`);
    // 1280 では入力欄（左上の 2 段目）を押す。
    await clickInPane(livePane, 40, 100);
    // 別の表示幅では場所をページに頼まないので、何も起きないことには合図が無い。
    await new Promise((done) => setTimeout(done, 800));
    assert.equal(await evaluate(`${draftNumbers} + ' ' + ${firstSelector}`), '[1] #button', 'no place is added at another width');
    assert.equal(await evaluate(`document.querySelector('#live-compose .lv-compose-save').disabled`), true, 'the comment is not saved at another width');
    console.log('PASS 書きかけのコメントの場所と別の表示幅では場所を足せず、保存もできない');

    await browser('click', '#live-compose .lv-compose-back');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px' && !${visible('#live-compose .lv-compose-back')}`);
    await settled(`${livePane} .lv-frame`);
    await clickInPane(livePane, 40, 100);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    await savePageCommentInThePage('kept at 390');
    const comment = (await reviewJson(kemi)).comments.at(-1);
    assert.deepEqual([comment.page.url, comment.page.width, comment.page.places.map((place) => place.n)], ['/rich.html', 390, [1, 2]]);
    console.log('PASS 書きかけの URL と表示幅に戻る操作で戻ると場所を足せ、保存したコメントはその表示幅を持つ');
  });
}

/**
 * 遅れて届いた場所（R-PAGE-COMMENT）: ページが場所を返す前に表示幅を変えても、場所は押したときの表示幅のものとして
 * 書きかけに入り、別の幅では戻る操作が出る。
 */
export async function latePlacesKeepTheirWidth(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
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
  });
}

/**
 * 保存している間の書きかけ（R-PAGE-COMMENT）: 保存の要求が返るまで、場所を足す・取り消す・消す・本文を書き換えることは
 * できず、保存し終えると書く欄が空いて、また書ける。保存したコメントは保存を押したときの場所と本文を持つ。
 */
export async function draftsStayWhileSaving(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
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
    // 保存している間は場所をページに頼まないので、何も起きないことには合図が無い。
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
    // 保存している間は表示幅もページも切り替えないので、何も起きないことには合図が無い。
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
  });
}

/**
 * 狭い画面で比べる相手の側を見ながらの保存（R-PAGE-COMMENT、R-NARROW）: 動いているページの枠が隠れている間にページが
 * 読み込まれ直しても、コメントの画像は作られる。
 */
export async function narrowReferenceSideSavesTheImage(repository) {
  await withLiveReview({ repository, page: 'rich.html', viewportAfter: ['1280', '900'] }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await postJson(kemi.url, 'api/message', { body: 'let me show you' });
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューでだけ押せるので、先に一度渡しておく。
    await handInThePage(kemi, repository, state);
    await settled(`${livePane} .lv-frame`);
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
    await nextPageMessage('page', () => browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-page-open'));
    await waitFor(`document.querySelector('#page-tree').dataset.drawer === undefined && !${visible(livePane)}`);
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
  });
}

/**
 * 保存している間に見方や側を変えたときの画像（R-PAGE-COMMENT、R-NARROW）: 画像を作る頼みがページに届く前に、コードの
 * 見方か、狭い画面で比べる相手の側に切り替えても、動いているページは並べたままなので、画像は作られ、場所の周り
 * （場所の要素の矩形に上下の余白を足した高さ）を写す。場所の一部を切り落とした画像にも、null にもならない。
 */
export async function switchingWhileSavingKeepsTheImageAroundThePlace(repository) {
  await withLiveReview({ repository, page: 'tall.html', viewportAfter: ['1280', '900'] }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
    await keepSavedImage();
    for (const away of ['view', 'side']) {
      await chooseTool('element');
      // 2 番目の帯（文書の y 300〜600）。
      await clickInPane(livePane, 150, 450);
      await waitFor(`${draftNumbers} === '[1]'`);
      if (away === 'side') {
        await browser('set', 'viewport', '390', '844');
        await waitFor(visible('.lv-side'));
        // 幅をまたいだ直後は書く欄の置き場所が変わるので、押す保存のボタンが動かなくなってから書いて押す。
        await settled('#live-compose .lv-compose-save');
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
        await settled(`${livePane} .lv-frame`);
      }
    }
  });
}

/**
 * 保存と渡す前のスナップショットが重なったとき（R-PAGE-COMMENT、R-PAGE-SNAPSHOT）: ページの見方で保存を始め、画像を
 * 作る頼みがページに届く前にコードの見方へ切り替えて渡す。保存が先に終わっても、まだ取っている渡す前のスナップショットは
 * 選んだ幅で並べた文書から記述する。ページを変えずにページの見方へ戻せば、それと比べた変化は 0。
 */
export async function savingAndHandingKeepThePageLaidOut(repository) {
  await withLiveReview({ repository, page: 'rich.html', viewportAfter: ['1280', '900'] }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await postJson(kemi.url, 'api/message', { body: 'let me show you' });
    // 「Hand to agent」は kemi wait が一度呼ばれたレビューでだけ押せるので、先に一度渡しておく。
    await handInThePage(kemi, repository, state);
    await settled(`${livePane} .lv-frame`);
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await browser('fill', '#live-compose .lv-compose-body', 'saved while handing');
    await holdRequests('image', 'capture');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${heldRequests} === 1`);
    await browser('click', '.lv-view button[data-view="code"]');
    await waitFor(`document.body.dataset.liveView === 'code'`);
    await postJson(kemi.url, 'api/message', { body: 'once more' });
    const waiting = startWaiting(kemi, repository, state);
    await pressHand();
    await waitFor(`${heldRequests} === 2`);
    await releaseRequests('image');
    await waitFor(`${draftNumbers} === '[]'`, 30000);
    await releaseRequests();
    assert.equal((await waiting).code, 0, 'kemi wait returns what was handed');
    await waitForOption(kemi, handedSnapshot(2));
    await browser('click', '.lv-view button[data-view="page"]');
    await chooseReference(kemi, handedSnapshot(2));
    await waitFor(`${changeList} !== null`);
    // 選び直した後に比べ直すかは記述が古いかで決まり、比べ直さないときは知らせが無いので、間を置いてから数を見る。
    await new Promise((done) => setTimeout(done, 1500));
    const counts = `${changeList}.dataset.main + '/' + ${changeList}.dataset.shifted`;
    assert.equal(await evaluate(counts), '0/0', 'a snapshot taken while a save ends shows no change against the unchanged page');
    console.log('PASS 保存が終わっても、重なって取っていた渡す前のスナップショットと、変えていないページを比べると変化が 0');
  });
}

/**
 * 画像を作る間にページが移ったとき（R-PAGE-COMMENT）: 画像を作る頼みがページに届く前に、動いているページが別のページへ
 * 移っても（ページの中のリンクやスクリプトで移るのと同じく、枠の src を変えて移す）、移った先のページの画像を場所の周りの
 * 画像として保存しない。画像は null になり、コメントと場所は保存される。
 */
export async function movingWhileSavingMakesNoImageOfAnotherPage(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await settled(`${livePane} .lv-frame`);
    await chooseTool('element');
    await clickInPane(livePane, 150, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await keepSavedImage();
    await browser('fill', '#live-compose .lv-compose-body', 'saved while the page moved');
    await holdRequests('image');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${heldRequests} === 1`);
    await nextPageMessage('page', () => evaluate(`(() => { const frame = document.querySelector('${livePane} .lv-frame'); frame.src = new URL('/other.html', frame.src).href; return true; })()`));
    await releaseRequests();
    await waitFor(`${draftNumbers} === '[]'`, 30000);
    const comment = (await reviewJson(kemi)).comments.at(-1);
    assert.deepEqual([comment.body, comment.page.url], ['saved while the page moved', '/rich.html']);
    assert.equal(await evaluate('window.__kemiSavedImage ?? null'), null, 'no image of the page the frame moved to is saved');
    console.log('PASS 画像を作る頼みが届く前に動いているページが別のページへ移ると、移った先の画像を保存せず、画像は null になる');
  });
}

/** kemi の描き込みは変化の一覧に入らない（② の印の層の決まりに倣う）: 場所を描いている間も保存した後も、変化の数が変わらない。 */
export async function placesMakeNoChange(repository) {
  await withLiveReview({ repository, page: 'changing.html' }, async ({ kemi }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await chooseCompare('side');
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(notRecorded);
    await browser('click', `${refPane} .lv-empty .lv-record`);
    await waitForSnapshot(kemi, recordedSnapshot(1), `${changeList}?.dataset.main === '0' && ${changeList}?.dataset.shifted === '0'`);
    await chooseTool('element');
    await clickInPane(livePane, 100, 150);
    await chooseTool('pen');
    await dragInPane(livePane, [[5, 5], [150, 5], [150, 100], [5, 100]]);
    await waitFor(`${draftNumbers} === '[1,2]'`);
    // 描き込みが変化に数えられるなら、ページが間を置いて変化を知らせて比べ直される。知らせが来ないことには合図が無い。
    await new Promise((done) => setTimeout(done, 1200));
    assert.equal(await evaluate(`${changeList}?.dataset.main + ',' + ${changeList}?.dataset.shifted`), '0,0', 'drawing places changes nothing');
    await browser('fill', '#live-compose .lv-compose-body', 'no change');
    await browser('click', '#live-compose .lv-compose-save');
    await waitFor(`${draftNumbers} === '[]'`);
    // 描き込みが変化に数えられるなら、ページが間を置いて変化を知らせて比べ直される。知らせが来ないことには合図が無い。
    await new Promise((done) => setTimeout(done, 1200));
    assert.equal(await evaluate(`${changeList}?.dataset.main + ',' + ${changeList}?.dataset.shifted`), '0,0', 'saving a page comment changes nothing');
    console.log('PASS 場所を描いている間と保存した後で、変化の一覧の数が変わらない');
  });
}

/**
 * 保存したページへのコメント（R-PAGE-COMMENT、R-PAGE-VIEW、R-PAGE-SESSION）: コメントだけがあるページがツリーに
 * コメントの数とともに出て、表示幅の札でそのページのその幅に移る。別の幅で付けたコメントのスレッドは付けた幅を出し、
 * 押すとその幅に切り替わり、場所の印がページの上に出る。本文だけを編集できる。保留して復元しても出る。
 */
export async function savedPageCommentsAreListedShownAndSwitched(repository) {
  const dev = await startDevServer();
  const state = await temporaryDirectory('kemi-live-state-');
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  let kemi = await startKemi(repository, state, ['--live', `${dev.url}rich.html`]);
  try {
    const other = await addPageComment(kemi, '/other.html', 390, 'only a comment here');
    const rich = await addPageComment(kemi, '/rich.html', 390, 'the button is too wide');
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    const commentCount = (page) => `document.querySelector('#page-tree .lv-page[data-page="${page}"] .lv-comment-count')?.textContent`;
    await waitFor(`${commentCount('/other.html')} === '1' && ${commentCount('/rich.html')} === '1'`);
    await browser('click', '#page-tree .lv-page[data-page="/other.html"] .lv-width-tag[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/other.html') && document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    console.log('PASS コメントだけがあるページがページのツリーにコメントの数とともに出て、表示幅の札を押すとそのページのその幅に切り替わる');

    await browser('click', '#page-tree .lv-page[data-page="/rich.html"] .lv-page-open');
    await browser('click', '.lv-widths button[data-width="1280"]');
    await waitFor(`document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/rich.html') && document.querySelector('${livePane} .lv-frame').style.width === '1280px'`);
    await clickUncovered('#cv-rail');
    await browser('click', `.cv-card[data-id="${rich.id}"]`);
    await waitFor(`document.querySelector('#cv-thread .cv-page-width')?.textContent === '390px'`);
    await browser('click', '#cv-thread .cv-page-width');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px' && document.querySelector('${livePane} .lv-bar-label').textContent.startsWith('/rich.html') && document.querySelector('#cv-thread .cv-page-width') === null`);
    console.log('PASS 390 で付けたコメントのスレッドを 1280 で開くと付けた幅が出て、押すと 390 に切り替わる');
    await settled(`${livePane} .lv-frame`);
    // 会話パネルを開いた分だけ枠が縮んでいるので、ボタンの周りの範囲も同じ倍率で縮める。
    const scale = await stageScale();
    const around = [0, 195, 305, 305].map((value) => Math.round(value * scale));
    const marked = await shotUntil(`${livePane} .lv-frame`, shots, 'saved-places', (shown) => countPixels(shown, around, isPlaceInk) > 0);
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
    await clickUncovered('#cv-rail');
    await waitFor(`document.querySelector('.cv-card[data-id="${other.id}"]') !== null && document.querySelector('.cv-card[data-id="${rich.id}"]') !== null`);
    console.log('PASS ページへのコメントを持つレビューを保留して復元すると、会話パネルとページのツリーにそのコメントが出る');
  } finally {
    await stop(kemi);
    await dev.close();
  }
}

/**
 * 枠の画像のうち、ページの x より右（ページの CSS ピクセル）。/tall.html の帯の文字（左端の「Band n」）は、帯の色に
 * 混ざって光や描き込みの色に近い画素を作るので、それより右だけを見る。
 */
async function rightOf(x) {
  const scale = await stageScale();
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
export async function placesGlowFromTheListAndTheThread(repository) {
  const shots = await mkdtemp(join(tmpdir(), 'kemi-live-shots-'));
  const frame = `${livePane} .lv-frame`;
  const wheel = (y) => wheelOn(`${livePane} .lv-capture`, { y, at: 100 });
  // 下の方（文書の y 2500 付近、帯 9 の中）を指す、帯の文字から離れた矢印。保存したコメントの場所に使う。
  const arrowLow = (width, left) => ({ url: '/tall.html', width, places: [{ n: 1, kind: 'arrow', points: [{ x: left, y: 2450 }, { x: left + 130, y: 2560 }], elements: [] }] });
  await withLiveReview({ repository, page: 'tall.html' }, async ({ kemi }) => {
    const c1 = await postJson(kemi.url, 'api/comment', { op: 'add_page', page: arrowLow(1280, 700), body: 'low at 1280' });
    const c2 = await postJson(kemi.url, 'api/comment', { op: 'add_page', page: arrowLow(390, 200), body: 'low at 390' });
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitFor(`document.querySelector('${livePane} .lv-notice').hidden && ${visible('.lv-tools')}`);
    await settled(`${livePane} .lv-frame`);
    // 押す場所はページのスクロールの位置から決まるので、スクロールしたことをページが知らせるのを待つ。
    await nextPageMessage('scroll', () => wheel(6000));
    // 帯 8 を場所にする（帯は幅いっぱいなので、描き込みと光の上下の辺が右側に写る）。
    await clickInPane(livePane, 600, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    const right = await rightOf(300);
    // ページが場所を描いてから、光が出ていないことを見る。
    await waitForPixels(frame, shots, 'draft-drawn', right, isPlaceInk);
    await waitForPixels(frame, shots, 'draft-before', right, isGlow, false);
    const row = '#live-compose .lv-place[data-n="1"]';
    await pointAt(row);
    await waitForPixels(frame, shots, 'draft-hovered', right, isGlow);
    await pointAt('#live-compose .lv-compose-head');
    await waitForPixels(frame, shots, 'draft-left', right, isGlow, false);
    console.log('PASS 書きかけの場所の一覧の行に乗せるとその場所が光り、離れると光が消える');

    await browser('focus', '#live-compose .lv-compose-body');
    await pointAt(row);
    await waitForPixels(frame, shots, 'draft-before-redraw', right, isGlow);
    // 本文に入力すると場所の一覧も描き直す。ポインタは、描き直される行の上に残す。
    await browser('press', 'a');
    await pointAt('#live-compose .lv-compose-head');
    await waitForPixels(frame, shots, 'draft-after-redraw', await rightOf(300), isGlow, false);
    console.log('PASS ホバー中に本文を入力して一覧が描き直されても、離れた後に光が残らない');

    await pointAt(row);
    await waitForPixels(frame, shots, 'draft-before-remove', await rightOf(300), isGlow);
    await browser('focus', `${row} .lv-place-remove`);
    await browser('press', 'Enter');
    await waitFor(`${draftNumbers} === '[]'`);
    await waitForPixels(frame, shots, 'draft-after-remove', await rightOf(300), isGlow, false);
    console.log('PASS ホバー中の場所を一覧から外すと、mouseleaveを待たずに光が消える');

    await clickInPane(livePane, 600, 250);
    await waitFor(`${draftNumbers} === '[1]'`);
    await pointAt(row);
    await waitForPixels(frame, shots, 'draft-before-hiding', await rightOf(300), isGlow);
    await browser('focus', '.lv-modetab[data-view="code"]');
    await browser('press', 'Enter');
    await waitFor(`document.body.dataset.liveView === 'code'`);
    await browser('focus', '.lv-modetab[data-view="page"]');
    await browser('press', 'Enter');
    await waitFor(`document.body.dataset.liveView === 'page'`);
    await pointAt('#live-compose .lv-compose-head');
    await waitForPixels(frame, shots, 'draft-after-hiding', await rightOf(300), isGlow, false);
    console.log('PASS ホバー中にコードの見方へ切り替えて戻っても、光が残らない');

    await pointAt(row);
    await browser('click', `${row} .lv-place-show`);
    await pointAt('#live-compose .lv-compose-head');
    await browser('fill', '#live-compose .lv-compose-body', 'flash survives redraw');
    await waitForPixels(frame, shots, 'draft-click-survives-leaving-and-redraw', await rightOf(300), isGlow);
    await waitForPixels(frame, shots, 'draft-click-finishes', await rightOf(300), isGlow, false);
    console.log('PASS クリックで始まった光は、ホバー終了や一覧の描き直しでは消えず、点滅後に消える');

    await wheel(-6000);
    await waitForPixels(frame, shots, 'draft-top', right, isPlaceInk, false);
    await browser('click', `${row} .lv-place-what`);
    await waitForPixels(frame, shots, 'draft-shown', right, isGlow);
    await waitForPixels(frame, shots, 'draft-shown-place', right, isPlaceInk);
    console.log('PASS 下の方に置いた場所の一覧の行を押すと、見る対象がその場所までスクロールして光る');

    await browser('click', '#live-compose .lv-compose-cancel');
    await wheel(-6000);
    await clickUncovered('#cv-rail');
    await browser('click', `.cv-card[data-id="${c1.id}"]`);
    await waitFor(`document.querySelector('#cv-thread .cv-go:not(.cv-page-width)') !== null`);
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
    await waitFor(`document.querySelector('#cv-thread .cv-page-width') !== null`);
    await browser('click', '#cv-thread .cv-page-width');
    await waitFor(`document.querySelector('${frame}').style.width === '390px'`);
    const narrow = await rightOf(150);
    await waitForPixels(frame, shots, 'other-width-shown', narrow, isGlow);
    await waitForPixels(frame, shots, 'other-width-shown-place', narrow, isPlaceInk);
    console.log('PASS 別の表示幅で付けたコメントの「ページで見る」でも、幅が切り替わってからその場所までスクロールして光る');
  });
}

/**
 * 渡すことと submit（R-PAGE-COMMENT の成功条件 1、R-AGENT-EVENTS、R-SUBMIT）: 画面で要素・矢印・ペンの 3 つの場所を
 * 持つコメントを付けて渡すと、`kemi wait` に 3 つの場所と画像の絶対パスが届き、そのファイルが PNG として読める。submit の
 * JSON には `page` が入り、画像は null。インラインのスタイルを止める CSP と Trusted Types を求める CSP のページでも、
 * コメントと場所は届く。
 */
export async function handedPageCommentsReachWaitAndSubmit(repository) {
  await withLiveReview({ repository, page: 'rich.html' }, async ({ kemi, state }) => {
    await browser('set', 'viewport', '1280', '900');
    await browser('open', kemi.url);
    await waitForSnapshot(kemi, startSnapshot);
    await browser('click', '.lv-widths button[data-width="390"]');
    await waitFor(`document.querySelector('${livePane} .lv-frame').style.width === '390px'`);
    await postJson(kemi.url, 'api/message', { body: 'let me show you' });
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

    await postJson(kemi.url, 'api/submit', { verdict: 'approved' });
    const { code, stdout } = await kemi.exited;
    assert.equal(code, 0);
    const submitted = JSON.parse(stdout).comments.find((item) => item.id === comment.id);
    assert.deepEqual(submitted.page.places, comment.page.places);
    assert.equal(submitted.page.image, null);
    console.log('PASS 同じコメントを submit すると、JSON の page に同じ場所が入り、image は null');
  });

  for (const query of ['csp=1', 'tt=1']) {
    await withLiveReview({ repository, page: `changing.html?${query}` }, async ({ kemi, state: strictState }) => {
      await browser('set', 'viewport', '1280', '900');
      await browser('open', kemi.url);
      await waitForSnapshot(kemi, startSnapshot);
      await postJson(kemi.url, 'api/message', { body: 'start' });
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
    });
  }
}
