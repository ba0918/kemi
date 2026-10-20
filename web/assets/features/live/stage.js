// @ts-check
// `--live` の舞台の枠の置き方（live.md の R-PAGE-VIEW、live-compare.md の R-PAGE-REF）: 選んだ表示幅で描いて枠に
// 収める倍率、重ねて透かす・末尾より下へずらす・消えた要素の行を押した後の比べる相手のずらし、比べる相手の側の光、
// 等倍で枠より広いページの横のスクロール。スナップショットや画像を作る間、隠れた枠を並べたままにする。

import { dom } from "../../dom.js";
import { fitScale, glowBox, overlayPlacement, revealRemoved, revealScrollLeft, shiftedPlacement } from "../../live-model.js";
import { state } from "../../state.js";
import { ask } from "./channel.js";
import { currentReference, live, liveActions, shell } from "./state.js";

/** 行単位のホイールの 1 行の量（画面のピクセル）。 */
const WHEEL_LINE = 16;

/**
 * ホイールの量を画面のピクセルに直す倍率。ページ単位は、そのとき動かす要素の高さ（ページ単位のときだけ読む。ふつうの
 * ピクセル単位のホイールのたびに配置を読ませない）。
 * @param {WheelEvent} event
 * @param {HTMLElement} page ページ単位の高さを読む要素
 * @returns {number}
 */
export function wheelUnit(event, page) {
  if (event.deltaMode === 1) {
    return WHEEL_LINE;
  }
  if (event.deltaMode === 2) {
    return page.clientHeight;
  }
  return 1;
}

/**
 * 動いているページの枠が隠れているか（狭い画面で比べる相手の側を見ている、コードの見方）。隠れた文書の要素は
 * 並べられていないので、その記述の要素の箱はスナップショットと合わず、変わっていない要素がずれたに見える。
 */
export function liveFrameHidden() {
  return shell === null || shell.liveFrame.getClientRects().length === 0;
}

/**
 * 動いているページの枠を並べた画面の高さ（ページの CSS ピクセル）。並べたままにしている間に読む。
 * @returns {number}
 */
export function laidOutHeight() {
  if (!shell) {
    return 0;
  }
  const height = Number.parseFloat(shell.liveFrame.style.height);
  return Number.isFinite(height) && height > 0 ? height : shell.liveFrame.clientHeight;
}

/**
 * 並べたままにしている間の状態。重なって呼ばれても、並べるのは最初の呼び出し、戻すのは最後に終わった呼び出しだけ
 * （先に終わった方が戻すと、まだ読んでいる方が並べていない文書を読む）。
 * @type {{ running: number, ready: Promise<void> }}
 */
const laidOut = { running: 0, ready: Promise.resolve() };

/**
 * 並べたまま見えなくしている舞台か動いているページの側を、操作もフォーカスも受けないようにする。並べている間に見方や
 * 側や画面の幅が変わると隠れるものも変わるので、そのたびに合わせる。並べていなければどちらも戻す。
 */
export function syncLaidOutInert() {
  if (!shell) {
    return;
  }
  const { stage, livePane } = shell;
  const hidden = (/** @type {HTMLElement} */ element) => getComputedStyle(element).visibility === "hidden";
  const inert = stage.dataset.measuring === undefined ? null : hidden(stage) ? stage : hidden(livePane) ? livePane : null;
  stage.inert = inert === stage;
  livePane.inert = inert === livePane;
}

/**
 * task が終わるまで、動いているページの枠を選んだ幅で並べたままにする。隠れていれば、見えず操作も受けないまま並べて
 * から task を呼ぶ。途中で見方や側を変えて枠が隠れても、並べたままにする。スナップショットの記述や画像を隠れた
 * 文書から作ると、隠れた文書は並べられていないので、要素の箱や文書の高さが並べたページと合わない。
 * @template T
 * @param {() => Promise<T>} task
 * @returns {Promise<T>}
 */
export async function whileLaidOut(task) {
  if (!shell) {
    return task();
  }
  const { stage, liveFrame } = shell;
  if (laidOut.running === 0) {
    const wasHidden = liveFrameHidden();
    stage.dataset.measuring = "true";
    syncLaidOutInert();
    laidOut.ready = Promise.resolve();
    if (wasHidden) {
      liveFrame.getBoundingClientRect();
      // 枠の大きさがページの文書に届くのを待つ（タブが裏にあって描かれないときも長くは待たない）。
      laidOut.ready = new Promise((done) => {
        requestAnimationFrame(() => requestAnimationFrame(() => done(undefined)));
        setTimeout(done, 200);
      });
    }
  }
  laidOut.running += 1;
  try {
    await laidOut.ready;
    return await task();
  } finally {
    laidOut.running -= 1;
    if (laidOut.running === 0) {
      delete stage.dataset.measuring;
      syncLaidOutInert();
    }
  }
}

/** 比べる相手の側の光を出しておく長さ（ミリ秒。ページの中の光と同じ）。 */
const REF_GLOW = 2800;

/** @type {ReturnType<typeof setTimeout> | null} */
let refGlowTimer = null;

/** 比べる相手をずらした形を保つ条件: 比べる相手・見比べ方・ページ・表示幅（狭い画面では見ている側も）。 */
function refShiftKey() {
  const reference = currentReference();
  const id = reference.type === "snapshot" ? reference.snapshot.id : reference.type;
  return [id, live.compare, live.page, live.width, state.narrow ? live.side : ""].join("\n");
}

/**
 * 消えた要素の行を押した: 比べる相手の側のその要素の位置まで動かし、光らせる（R-PAGE-DIFF）。見る対象だけのときは並べる
 * 見比べ方に、狭い画面では比べる相手の 1 枚に切り替えてから（R-PAGE-VIEW）。比べる相手の枠はスクリプトを止めていて中を
 * 動かせないので、中身の高さで描いて外側をずらす（R-PAGE-SNAPSHOT）。重ねて透かすときは見比べ方を変えず、見る対象を
 * その位置までスクロールし、重ねた比べる相手もそろって動く。
 * @param {number} index 比べる相手の側の記述の中の要素の番号
 */
export function showRemovedElement(index) {
  if (!shell) {
    return;
  }
  const reference = currentReference();
  const id = reference.type === "snapshot" ? reference.snapshot.id : "";
  const element = live.changesFrom?.snapshot === id ? live.descriptions.get(id)?.elements[index] : undefined;
  if (!element) {
    return;
  }
  if (state.narrow) {
    if (live.side !== "ref") {
      liveActions.setSide("ref");
    }
  } else if (live.compare === "now") {
    live.compare = "side";
    liveActions.render();
  }
  const [left, top, width, height] = element.box;
  live.refGlow = { snapshot: id, box: element.box };
  if (refGlowTimer !== null) {
    clearTimeout(refGlowTimer);
  }
  refGlowTimer = setTimeout(() => {
    refGlowTimer = null;
    live.refGlow = null;
    layoutFrames();
  }, REF_GLOW);
  const overlaid = !state.narrow && live.compare === "overlay";
  const reveal = revealRemoved({
    top,
    height,
    viewHeight: shell.refViewport.clientHeight / live.scale,
    pageHeight: live.scroll.height,
    snapshotHeight: live.descriptions.get(id)?.height ?? 0,
    overlaid,
  });
  if (overlaid) {
    // 今のページが短くてそこまでスクロールできないときは、届かない分だけ両方の枠を外側でずらす（そろえたまま）。
    live.overscroll = reveal.overscroll ? { key: refShiftKey(), y: reveal.y } : null;
    shell.liveFrame.contentWindow?.postMessage({ kemi: "live", type: "scroll-by", x: 0, y: reveal.y - live.scroll.y }, live.origin);
  } else {
    live.refShift = { key: refShiftKey(), y: reveal.y };
  }
  layoutFrames();
  // 重ねた比べる相手は、ページの横のスクロールの分だけ左にずらして描いている（layoutFrames）。描いている所で合わせる。
  revealSideways(shell.refViewport, overlaid ? left - live.scroll.x : left, width);
}

/**
 * 等倍で枠より広いページの中の箱（文書の横の位置と幅）が見えるよう、枠の横のスクロールを合わせる（R-PAGE-DIFF、
 * R-PAGE-COMMENT）。見る対象と比べる相手の枠は横のスクロールをそろえているので、両方に入れる（狭い画面で隠れている側の
 * 枠は動かないので、見えている側の枠で測る）。
 * @param {HTMLElement} viewport 箱が見えている側の枠の外側
 * @param {number} left
 * @param {number} width
 */
function revealSideways(viewport, left, width) {
  if (!shell) {
    return;
  }
  const scrollLeft = revealScrollLeft({
    left: left * live.scale,
    width: width * live.scale,
    scrollLeft: viewport.scrollLeft,
    viewportWidth: viewport.clientWidth,
  });
  shell.liveViewport.scrollLeft = scrollLeft;
  shell.refViewport.scrollLeft = scrollLeft;
}

/**
 * 比べる相手をずらした形の間、外側のずれを動かす。値は画面のピクセルで、正なら右・下の方を見せる。
 * @param {number} x
 * @param {number} y
 */
function moveShiftedReference(x, y) {
  if (!shell || live.refShift === null) {
    return;
  }
  shell.refViewport.scrollLeft += x;
  live.refShift = { ...live.refShift, y: live.refShift.y + y / live.scale };
  layoutFrames();
}

/** 比べる相手をずらした形の間、キーで動かす量（画面のピクセル。矢印のキー）。 */
const SHIFT_KEY_STEP = 40;

/**
 * 比べる相手をずらした形の間、比べる相手の上で受ける操作（ホイール・指やマウスで引く・キー）で外側のずれを動かす。
 * ずらした形の比べる相手は中身の高さで描いていて、枠の中では動かないため（狭い画面では指で引くしかない）。
 * @param {HTMLElement} layer 比べる相手の上に重ねた層
 */
export function startShiftedReferenceMoves(layer) {
  layer.addEventListener(
    "wheel",
    (event) => {
      if (!shell || live.refShift === null) {
        return;
      }
      event.preventDefault();
      const unit = wheelUnit(event, shell.refViewport);
      moveShiftedReference(event.deltaX * unit, event.deltaY * unit);
    },
    { passive: false },
  );
  /** @type {{ pointer: number, x: number, y: number } | null} */
  let dragging = null;
  layer.addEventListener("pointerdown", (event) => {
    dragging = { pointer: event.pointerId, x: event.clientX, y: event.clientY };
    try {
      layer.setPointerCapture(event.pointerId);
    } catch {
      // 捕まえられなくても、層の上で引いている間は動かせる。
    }
  });
  layer.addEventListener("pointermove", (event) => {
    if (dragging?.pointer !== event.pointerId) {
      return;
    }
    // 引いた向きと逆に中身が動く（指で紙を引くのと同じ）。
    moveShiftedReference(dragging.x - event.clientX, dragging.y - event.clientY);
    dragging = { pointer: event.pointerId, x: event.clientX, y: event.clientY };
  });
  const stop = () => {
    dragging = null;
  };
  layer.addEventListener("pointerup", stop);
  layer.addEventListener("pointercancel", stop);
  layer.addEventListener("keydown", (event) => {
    if (!shell || live.refShift === null) {
      return;
    }
    const page = shell.refViewport.clientHeight * 0.9;
    const moves = /** @type {Record<string, [number, number]>} */ ({
      ArrowUp: [0, -SHIFT_KEY_STEP],
      ArrowDown: [0, SHIFT_KEY_STEP],
      ArrowLeft: [-SHIFT_KEY_STEP, 0],
      ArrowRight: [SHIFT_KEY_STEP, 0],
      PageUp: [0, -page],
      PageDown: [0, page],
      " ": [0, event.shiftKey ? -page : page],
      Home: [0, -Infinity],
      End: [0, Infinity],
    });
    const move = moves[event.key];
    if (!move) {
      return;
    }
    event.preventDefault();
    // Home と End は端まで（layoutFrames が中身の高さに収める）。
    const [x, y] = move;
    moveShiftedReference(x, Number.isFinite(y) ? y : y > 0 ? Number.MAX_SAFE_INTEGER : -Number.MAX_SAFE_INTEGER);
  });
}

/** ページが要素を見せて光らせるまで待つ上限。 */
const GLOW_TIMEOUT = 5000;

/**
 * 変化の一覧の行を押した: 見る対象のその要素までスクロールし、その要素の印を光らせる（R-PAGE-DIFF）。等倍で枠より
 * 広いページでは、枠の横のスクロールもその要素が見える位置に合わせる。狭い画面で比べる相手の側を見ていたら、
 * 動いているページの側に切り替えてから。
 * @param {number} index 今の側の記述の中の要素の番号
 */
export async function showChangedElement(index) {
  if (!shell) {
    return;
  }
  if (live.side !== "live") {
    liveActions.setSide("live");
  }
  const frame = shell.liveFrame.contentWindow;
  if (!frame) {
    return;
  }
  await glowAndReveal(frame, "glow-element", { index, mode: "flash" });
}

/**
 * ページに要素か場所までスクロールして光らせてもらい、返った箱（文書の横の位置と幅）が見えるよう、等倍で枠より広い
 * ページでは枠の横のスクロールも合わせる。末尾より下へずらしていれば、頼む前にやめる。
 * @param {Window} frame
 * @param {"glow-element" | "glow-places"} type
 * @param {Record<string, unknown>} details 頼みごとの中身
 */
export async function glowAndReveal(frame, type, details) {
  endOverscroll();
  const answer = await ask(frame, type, GLOW_TIMEOUT, details);
  const rect = answer?.rect;
  if (shell && rect && typeof rect.x === "number" && typeof rect.w === "number") {
    revealSideways(shell.liveViewport, rect.x, rect.w);
  }
}

/**
 * 重ねて透かす間に、ページのスクロールで届かない分（文書の座標）。ページが見せたい位置までスクロールしきれなかった分で、
 * ページのスクロールが返るたびに変わる。
 * @returns {number}
 */
export function overscrolled() {
  return live.overscroll === null ? 0 : Math.max(0, live.overscroll.y - live.scroll.y);
}

/**
 * 末尾より下へずらすのをやめる。ページの上の何かを見せる（ページにスクロールを頼む）前に呼ぶ。ずらしたままだと、ページが
 * 末尾までしかスクロールできないときにスクロールの知らせが来ず、見せたものがずらした分だけ上に描かれる。
 */
function endOverscroll() {
  if (live.overscroll !== null) {
    live.overscroll = null;
    layoutFrames();
  }
}

/** 選んだ表示幅で描き、枠に収まらなければ両方に同じ倍率をかけて縮める（R-PAGE-VIEW）。 */
export function layoutFrames() {
  if (!shell) {
    return;
  }
  const compose = shell.compose.box.getBoundingClientRect();
  const composeClearance = live.view === "page" && !state.narrow && compose.height > 0
    ? Math.max(0, dom.conversation.getBoundingClientRect().bottom - compose.top + 12)
    : 0;
  // 畳んだ帯の案内は画面の上に浮かぶため、コメント欄が開いたらその操作を覆わない高さへ逃がす。
  dom.railHandNote.style.setProperty("--lv-compose-clearance", `${composeClearance}px`);
  if (live.view !== "page") {
    return;
  }
  // 狭い画面では片方が隠れているので、見えている方の枠で測る。どちらも同じ大きさ。
  const viewport = shell.liveViewport.clientWidth > 0 ? shell.liveViewport : shell.refViewport;
  // 等倍では縮めず、枠より広い分は枠を横にスクロールして見る。縦はページの中でスクロールする。
  const scale = live.zoom === "full" ? 1 : fitScale(viewport.clientWidth, live.width);
  shell.liveFrame.style.width = `${live.width}px`;
  // 横のスクロールバーの分だけ低くなった枠の高さで描く（幅を決めてから読む）。
  const height = viewport.clientHeight / scale;
  const overlaid = live.compare === "overlay" && !state.narrow;
  if (live.overscroll !== null && (!overlaid || live.overscroll.key !== refShiftKey())) {
    live.overscroll = null;
  }
  // 末尾より下へずらしている分（重ねて透かすときだけ）。見る対象の枠も同じだけ上へずらし、比べる相手とそろえる。
  const beyond = overscrolled();
  shell.liveFrame.style.height = `${height}px`;
  shell.liveFrame.style.transform = beyond > 0 ? `translate(0px, ${-beyond * scale}px) scale(${scale})` : `scale(${scale})`;
  // 道具の層は、横にスクロールして出てくる所も含めてページの上を覆う。
  shell.capture.style.width = `${Math.max(viewport.clientWidth, live.width * scale)}px`;
  shell.capture.style.height = `${viewport.clientHeight}px`;
  // 重ねて透かすときは、比べる相手を中身の高さで描き、見る対象のスクロールの分だけずらす。狭い画面では
  // 重ねず 1 枚ずつ見る（R-PAGE-VIEW）。
  // 消えた要素の行を押した後は、比べる相手を中身の高さで描いて外側でずらす（比べる相手・見比べ方・ページ・表示幅の
  // どれかが変わるまで）。
  if (live.refShift !== null && (overlaid || live.refShift.key !== refShiftKey())) {
    live.refShift = null;
  }
  const reference = currentReference();
  const shownId = reference.type === "snapshot" ? reference.snapshot.id : "";
  /** @type {{ x: number, y: number } | null} 比べる相手の文書の左上が、枠の外側のどこにあるか（画面のピクセル） */
  let origin = null;
  let placement = { height, transform: `scale(${scale})` };
  if (overlaid) {
    // 比べる相手は、今のページより長くても末尾まで描く（消えた要素が末尾の下にあることがある）。
    placement = overlayPlacement({
      scale,
      viewportHeight: viewport.clientHeight,
      scrollX: live.scroll.x,
      scrollY: live.scroll.y + beyond,
      contentHeight: Math.max(live.scroll.height, live.descriptions.get(shownId)?.height ?? 0),
    });
    origin = { x: -live.scroll.x * scale, y: -(live.scroll.y + beyond) * scale };
  } else if (live.refShift !== null) {
    const shifted = shiftedPlacement({
      y: live.refShift.y,
      viewHeight: height,
      snapshotHeight: live.descriptions.get(shownId)?.height ?? 0,
      scale,
    });
    live.refShift = { ...live.refShift, y: shifted.y };
    placement = { height: shifted.height, transform: shifted.transform };
    origin = shifted.origin;
  }
  for (const target of [shell.refFrame, shell.refMockFrame]) {
    target.style.width = `${live.width}px`;
    target.style.height = `${placement.height}px`;
    target.style.transform = placement.transform;
  }
  shell.refWheel.hidden = live.refShift === null;
  const glow = live.refGlow !== null && live.refGlow.snapshot === shownId && origin !== null ? live.refGlow.box : null;
  shell.refGlow.hidden = glow === null;
  if (glow !== null && origin !== null) {
    const box = glowBox({ origin, box: glow, scale });
    shell.refGlow.style.left = `${box.left}px`;
    shell.refGlow.style.top = `${box.top}px`;
    shell.refGlow.style.width = `${box.width}px`;
    shell.refGlow.style.height = `${box.height}px`;
  }
  shell.stage.style.setProperty("--lv-scale", String(scale));
  if (live.scale !== scale) {
    live.scale = scale;
    liveActions.renderBand();
  }
}
