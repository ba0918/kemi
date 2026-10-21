// @ts-check
// `--live` のページへのコメントを書く（live.md の R-PAGE-COMMENT）。
// 道具を選んでいる間は、動いているページの枠の上に重ねた層で押す・描く操作を受け、枠の中の座標をページに送る。
// どの要素を指すかと文書の座標はページの中（page.js）が決める（別のオリジンで、レビュー画面からは読めない）。
// 矢印とペンの保存する点（丸めて間引いた点）は、ページが返した文書の座標からここで決める。ページの上の描き込みと
// コメントの画像は、書きかけの場所として送り返すこの点で描く。
// 重ねた層は動いているページの枠の上にだけあるので、比べる相手の側では場所を置けない。重ねて透かしている間は
// 比べる相手が操作を受けないので、下の層が受け、見る対象の要素が場所になる（live-compare.md の R-PAGE-REF）。

import * as api from "../../api.js";
import {
  addPlace,
  commentShortName,
  consumeOverscroll,
  draftElsewhere,
  draftSavable,
  editBody,
  editedSpan,
  emptyDraft,
  imageUnsavedNotice,
  placeSets,
  removePlace,
  strayRefs,
  strokePoints,
  undoPlace,
} from "../../live-model.js";
import { state } from "../../state.js";
import { renderConversation } from "../../views/conversation.js";
import { renderHeader } from "../../views/header.js";
import { renderPlaces, renderStrayRefs } from "../../views/live.js";
import { refreshCommentBadges } from "../../views/tree.js";
import { closeSheet } from "../conversation.js";
import { renderTree } from "./changes.js";
import { ask } from "./channel.js";
import { glowAndReveal, laidOutHeight, layoutFrames, overscrolled, whileLaidOut, wheelUnit } from "./stage.js";
import { live, liveActions, pageComments, shell } from "./state.js";

/** ページが置いた場所を決めて返すまで待つ上限。ペンは要素をすべて測るので長めに。 */
const PLACE_TIMEOUT = 5000;

/** ページがコメントの画像を返すまで待つ上限。過ぎたら画像なしで保存する。 */
const IMAGE_TIMEOUT = 15000;

/** これより短い矢印とペンの線は、押しただけとみなして場所にしない（画面のピクセル）。 */
const MIN_STROKE = 4;

/** 描いている途中の線。点は枠の中の画面の座標（ページの CSS ピクセル）と、重ねた層の中の座標。 */
/** @type {{ pointer: number, points: { x: number, y: number }[], drawn: { x: number, y: number }[] } | null} */
let drawing = null;

/**
 * @param {import("../../views/live.js").LiveShell} shell
 */
export function startComposing(shell) {
  shell.toolSeg.addEventListener("click", (event) => {
    const tool = /** @type {HTMLElement} */ (event.target).closest("button")?.dataset.tool;
    if (tool === "element" || tool === "arrow" || tool === "pen" || tool === "interact") {
      cancelStroke();
      live.tool = tool;
      live.composeError = "";
      liveActions.render();
    }
  });
  const layer = shell.capture;
  layer.addEventListener("pointerdown", (event) => {
    if (live.tool === "interact" || event.button !== 0) {
      return;
    }
    event.preventDefault();
    layer.setPointerCapture(event.pointerId);
    drawing = { pointer: event.pointerId, points: [], drawn: [] };
    extendStroke(event);
  });
  layer.addEventListener("pointermove", (event) => {
    if (drawing?.pointer === event.pointerId && live.tool !== "element") {
      extendStroke(event);
    }
    if (!drawing) {
      void findSavedAt({ x: event.clientX, y: event.clientY });
    }
  });
  layer.addEventListener("pointerleave", () => showSavedTip(null, { x: 0, y: 0 }));
  layer.addEventListener("pointerup", (event) => {
    if (drawing?.pointer === event.pointerId) {
      void finishStroke();
    }
  });
  layer.addEventListener("pointercancel", cancelStroke);
  // 道具を選んでいる間もページをスクロールできるよう、ホイールはページに送る。
  layer.addEventListener(
    "wheel",
    (event) => {
      event.preventDefault();
      const unit = wheelUnit(event, layer);
      // 等倍で枠より広いページは、枠のほうを横にスクロールする。枠が動ききった残りだけをページに送る。
      const viewport = shell.liveViewport;
      const before = viewport.scrollLeft;
      viewport.scrollLeft += event.deltaX * unit;
      const x = event.deltaX * unit - (viewport.scrollLeft - before);
      let y = event.deltaY * unit;
      // 末尾より下へずらしている間は、上へのホイールでまずその分を戻す。
      if (live.overscroll !== null) {
        const consumed = consumeOverscroll({ deltaY: y, beyond: overscrolled(), y: live.overscroll.y });
        if (consumed !== null) {
          live.overscroll = consumed.y === null ? null : { ...live.overscroll, y: consumed.y };
          y = consumed.deltaY;
          layoutFrames();
        }
      }
      shell.liveFrame.contentWindow?.postMessage({ kemi: "live", type: "scroll-by", x, y }, live.origin);
    },
    { passive: false },
  );
  window.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && drawing) {
      cancelStroke();
    }
  });
  const compose = shell.compose;
  compose.undo.addEventListener("click", () => setDraft(undoPlace(live.draft)));
  compose.cancel.addEventListener("click", () => {
    live.composeError = "";
    setDraft(emptyDraft(live.page, live.width));
  });
  compose.save.addEventListener("click", () => void savePageComment());
  compose.back.addEventListener("click", () => showPage(live.draft.url, live.draft.width));
  compose.body.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      void savePageComment();
    }
  });
  // 書き換わった区間は、入力の前の選択と入力の後のカーソルから求める（宙に浮いた参照を字面で見分けられないため）。
  let selection = { start: 0, end: 0 };
  compose.body.addEventListener("beforeinput", () => {
    selection = { start: compose.body.selectionStart, end: compose.body.selectionEnd };
  });
  compose.body.addEventListener("input", () => {
    const value = compose.body.value;
    const edit = editedSpan(selection, compose.body.selectionEnd, value.length - live.draft.body.length);
    setDraft(editBody(live.draft, value, edit));
  });
}

/**
 * 描いている線に点を足す。枠の中の座標はページの CSS ピクセルに戻す（枠は縮めて描いている）。
 * @param {PointerEvent} event
 */
function extendStroke(event) {
  if (!shell || !drawing) {
    return;
  }
  const frame = shell.liveFrame.getBoundingClientRect();
  const layer = shell.capture.getBoundingClientRect();
  drawing.points.push({ x: (event.clientX - frame.left) / live.scale, y: (event.clientY - frame.top) / live.scale });
  drawing.drawn.push({ x: event.clientX - layer.left, y: event.clientY - layer.top });
  shell.stroke.setAttribute("points", drawing.drawn.map((point) => `${point.x},${point.y}`).join(" "));
}

/** 描いている途中の線を取りやめる。 */
function cancelStroke() {
  drawing = null;
  shell?.stroke.setAttribute("points", "");
}

/** 線を描き終えた（押し終えた）。ページに場所を決めてもらい、書いているコメントに足す。 */
async function finishStroke() {
  const stroke = drawing;
  cancelStroke();
  const frame = shell?.liveFrame.contentWindow;
  // 書きかけの場所と別の URL か表示幅では足さない（書く欄がそのことと戻る操作を出している）。
  if (!stroke || !frame || live.tool === "interact" || state.liveSaving || draftElsewhere(live.draft, live.page, live.width)) {
    return;
  }
  const kind = live.tool;
  const first = stroke.drawn[0];
  const length = stroke.drawn.reduce((most, point) => Math.max(most, Math.hypot(point.x - first.x, point.y - first.y)), 0);
  if (kind !== "element" && length < MIN_STROKE) {
    return;
  }
  // 返事を待つ間に別のページや表示幅へ移っても、場所は押したときのページのもの。
  const { page, width } = live;
  const answer = await ask(frame, "place", PLACE_TIMEOUT, {
    kind,
    points: kind === "element" ? stroke.points.slice(0, 1) : stroke.points,
  });
  // 返事を待つ間に保存を始めていれば、書きかけは変えない。
  if (state.liveSaving) {
    return;
  }
  if (answer.kind !== kind) {
    live.composeError = `The place was not put: ${answer.error ?? "the page did not answer"}`;
    renderCompose();
    return;
  }
  const points = kind === "element" ? [] : strokePoints(Array.isArray(answer.points) ? answer.points : []);
  // 間引くと最後の 2 点が重なる矢印とペンは、押しただけと同じく場所にしない（R-PAGE-COMMENT）。
  if (points === null) {
    return;
  }
  live.composeError = "";
  const place = { kind, points, elements: answer.elements ?? [] };
  setDraft(addPlace(live.draft, place, page, width));
}

/**
 * 書いているコメントの場所を差し替え、書く欄とページの上の描き込みを描き直す。
 * @param {import("../../live-model.js").PlaceDraft} draft
 */
function setDraft(draft) {
  live.draft = draft;
  showDraftBody();
  renderCompose();
  sendPlaces();
}

/** 書きかけの本文を書く欄に出す（詰め直しで書き換わったとき）。入力位置は同じ所に置く。 */
function showDraftBody() {
  const body = shell?.compose.body;
  if (!body || body.value === live.draft.body) {
    return;
  }
  const caret = body.selectionStart;
  body.value = live.draft.body;
  body.setSelectionRange(Math.min(caret, body.value.length), Math.min(caret, body.value.length));
}

/**
 * 本文の入力位置に `#n` を入れる（R-PAGE-COMMENT の、場所の一覧の番号を押す）。
 * @param {number} n
 */
function insertReference(n) {
  const body = shell?.compose.body;
  if (!body || state.liveSaving) {
    return;
  }
  const edit = { start: body.selectionStart, end: body.selectionEnd };
  body.setRangeText(`#${n}`, edit.start, edit.end, "end");
  body.focus();
  setDraft(editBody(live.draft, body.value, edit));
}

/** ポインタの下の保存したコメントをページに尋ねている間の、次に尋ねる点（尋ねている間に動いた分はまとめる）。 */
let savedAsk = /** @type {{ x: number, y: number } | null} */ (null);
let savedAsking = false;

/**
 * 場所を置く道具の層の上のポインタの下に、保存したコメントの印があるかをページに尋ね、あればその名前を出す
 * （R-PAGE-COMMENT）。層が枠を覆ってポインタがページに届かないので、レビュー画面から尋ねる。
 * @param {{ x: number, y: number }} point 画面の座標
 */
async function findSavedAt(point) {
  savedAsk = point;
  const frame = shell?.liveFrame.contentWindow;
  if (savedAsking || !shell || !frame || pageComments().length === 0) {
    return;
  }
  savedAsking = true;
  try {
    while (savedAsk) {
      const asked = savedAsk;
      savedAsk = null;
      const box = shell.liveFrame.getBoundingClientRect();
      const answer = await ask(frame, "saved-at", PLACE_TIMEOUT, {
        x: (asked.x - box.left) / live.scale,
        y: (asked.y - box.top) / live.scale,
      });
      showSavedTip(typeof answer.comment === "string" ? answer.comment : null, asked);
    }
  } finally {
    savedAsking = false;
  }
}

/**
 * 保存したコメントの短い名前を、ポインタのそばに出す。コメントが無ければ隠す。
 * @param {string | null} id
 * @param {{ x: number, y: number }} point 画面の座標
 */
export function showSavedTip(id, point) {
  if (!shell) {
    return;
  }
  const tip = shell.savedTip;
  const comment = id === null ? undefined : pageComments().find((item) => item.id === id);
  tip.hidden = comment === undefined;
  if (comment === undefined) {
    return;
  }
  tip.textContent = commentShortName(String(comment.body ?? ""));
  tip.dataset.comment = comment.id;
  const box = /** @type {HTMLElement} */ (tip.parentElement).getBoundingClientRect();
  tip.style.left = `${point.x - box.left + 12}px`;
  tip.style.top = `${point.y - box.top + 12}px`;
}

/** 最後にページへ送った描き込み。同じなら送り直さない（会話が描き直されるたびに呼ばれる）。 */
let sentPlaces = "";

/** ページの上に場所を描かせる（描かせる組は live-model.js の placeSets）。 */
export function sendPlaces() {
  const frame = shell?.liveFrame.contentWindow;
  if (!frame) {
    return;
  }
  const sets = placeSets({
    comments: pageComments(),
    draft: live.draft,
    page: live.page,
    width: live.width,
    thread: state.conversation.thread,
  });
  const message = JSON.stringify(sets);
  if (message === sentPlaces) {
    return;
  }
  sentPlaces = message;
  frame.postMessage({ kemi: "live", type: "places", sets }, live.origin);
}

/** 会話の中身が変わった（コメントが増えた・消えた、スレッドを開いた）。印とページのツリーを合わせる。 */
export function refreshPageComments() {
  sendPlaces();
  renderTree();
}

/**
 * 「ページで見る」で別のページへ移ったときの、読み込みと描き込みが済んだら光らせるコメント。
 * @type {{ comment: string, page: string, width: number } | null}
 */
let glowAfterLoad = null;

/** 動いているページの文書が読み込まれた（読み込み直した、移った）。 */
export function placesAfterLoad() {
  // 読み込み直した文書には前の描き込みが無いので、描き直させる。
  sentPlaces = "";
  sendPlaces();
  // 「ページで見る」で移ったページなら、描き込みが済んだので場所を光らせる。
  const waiting = glowAfterLoad;
  glowAfterLoad = null;
  if (waiting && waiting.page === live.page && waiting.width === live.width) {
    void glowPlaces(waiting.comment, null, "flash");
  }
}

/**
 * ページへのコメントを、付けた URL と表示幅のページの見方で見せ、その場所までスクロールして光らせる（R-PAGE-COMMENT）。
 * 別のページへ移るときは、読み込みと描き込みが済んでから光らせる。狭い画面では会話のシートを閉じてページを見せる。
 * @param {any} comment
 */
export function showPageComment(comment) {
  // 保存している間は移れない（showPage が何もしない）ので、狭い画面のシートも閉じない。
  if (!comment.page || state.liveSaving) {
    return;
  }
  if (state.narrow) {
    closeSheet();
  }
  const moved = showPage(comment.page.url, comment.page.width);
  renderConversation();
  if (moved) {
    glowAfterLoad = { comment: comment.id, page: comment.page.url, width: comment.page.width };
    return;
  }
  glowAfterLoad = null;
  // 表示幅を変えたときは、枠の新しい大きさがページに届いてから（並べ直した後の文書の座標で）光らせる。
  requestAnimationFrame(() => requestAnimationFrame(() => void glowPlaces(comment.id, null, "flash")));
}

/**
 * ページの上の場所を光らせる（R-PAGE-COMMENT）。`flash` は場所までスクロールして明滅させ、`on` は `off` まで光らせる。
 * `flash` では、等倍で枠より広いページの枠の横のスクロールも、その場所が見える位置に合わせる。
 * @param {string | null} comment 保存したコメントの id。null なら書きかけの場所
 * @param {number | null} n 場所の番号。null ならそのコメントのすべての場所
 * @param {"on" | "off" | "flash"} mode
 */
async function glowPlaces(comment, n, mode) {
  const frame = shell?.liveFrame.contentWindow;
  if (!shell || !frame) {
    return;
  }
  if (mode !== "flash") {
    frame.postMessage({ kemi: "live", type: "glow-places", comment, n, mode, scroll: false }, live.origin);
    return;
  }
  await glowAndReveal(frame, "glow-places", { comment, n, mode, scroll: true });
}

/**
 * そのページをその表示幅で、ページの見方で見せる。狭い画面では動いているページの側を見せる（比べる相手の側を
 * 見ていると、動いているページと、その上のコメントの場所が隠れたままになる）。別のページへ移ったかを返す。
 * @param {string} url
 * @param {number} width
 * @returns {boolean}
 */
function showPage(url, width) {
  // 保存している間は表示幅もページも変えられないので、見方だけを切り替えることもしない。
  if (state.liveSaving) {
    return false;
  }
  if (live.view !== "page") {
    liveActions.setView("page");
  }
  if (live.side !== "live") {
    liveActions.setSide("live");
  }
  if (live.width !== width) {
    liveActions.setWidth(width);
  }
  if (live.page !== url) {
    liveActions.openPage(url);
    return true;
  }
  return false;
}

export function renderCompose() {
  if (!shell) {
    return;
  }
  const compose = shell.compose;
  const places = live.draft.places;
  const away = draftElsewhere(live.draft, live.page, live.width);
  compose.away.hidden = away === null;
  if (away) {
    compose.awayText.textContent = `These places are on ${away.url} at ${away.width}px. Places can be added and the comment saved there.`;
    compose.back.textContent = `Back to ${away.url} at ${away.width}px`;
  }
  // 道具を選んだだけでは開かず、最初の場所を置いたときに開く（R-PAGE-COMMENT）。
  compose.box.hidden = live.view !== "page" || (places.length === 0 && live.draft.body === "");
  // 保存している間は書きかけを変えさせない（保存し終えると書く欄を空けるので、その間の変更は消えてしまう）。
  renderPlaces(
    compose,
    places,
    {
      remove: (n) => setDraft(removePlace(live.draft, n)),
      insert: insertReference,
      glow: (n, mode) => void glowPlaces(null, n, mode),
    },
    state.liveSaving,
  );
  const stray = strayRefs(live.draft);
  renderStrayRefs(compose, stray, (ref) => {
    compose.body.focus();
    compose.body.setSelectionRange(ref.start, ref.end);
  });
  compose.undo.disabled = places.length === 0 || state.liveSaving;
  compose.cancel.disabled = state.liveSaving;
  compose.body.readOnly = state.liveSaving;
  compose.save.disabled = !draftSavable(live.draft, live.page, live.width) || state.liveSaving || state.submitted;
  compose.error.hidden = live.composeError === "";
  compose.error.textContent = live.composeError;
}

/**
 * 書いたコメントを保存する。ページに場所の周りの画像を作らせ、コメントと 1 回で送る。画像を作れなければ
 * 画像なしで保存する（R-PAGE-COMMENT の画像は作れないこともある）。保存した後は場所を変えない。
 */
async function savePageComment() {
  if (!shell || state.liveSaving || state.submitted) {
    return;
  }
  const draft = live.draft;
  if (!draftSavable(draft, live.page, live.width)) {
    return;
  }
  const body = draft.body.trim();
  setSaving(true);
  const frame = shell.liveFrame.contentWindow;
  let image = null;
  let imageError = "";
  if (frame) {
    // 狭い画面で比べる相手の側を見ていると、動いているページは並べられておらず画像を作れない。画像を決める値（ページ・
    // 表示幅・並べた画面の高さ・場所）は頼みに載せ、ページはそれだけで画像を作り、自分の場所や幅が違えば断る。
    const answer = await whileLaidOut(() =>
      ask(frame, "image", IMAGE_TIMEOUT, {
        page: draft.url,
        width: draft.width,
        height: laidOutHeight(),
        places: draft.places,
      }),
    );
    image = typeof answer.png === "string" ? answer.png : null;
    if (image === null && typeof answer.error === "string") imageError = answer.error;
  }
  // 保存する場所の形は R-SUBMIT の `page.places`。
  const places = draft.places.map(({ n, kind, points, elements }) => ({ n, kind, points, elements }));
  const request = { op: "add_page", page: { url: draft.url, width: draft.width, places }, body };
  try {
    const comment = await api.postComment({ ...request, image }).catch((error) => {
      // 本文と場所に画像を足すと要求の上限を超えるときは、画像なしで保存する（画像は作れないこともある）。
      // 上限を超えた要求はサーバが読まずに断るので、送り直しても二重にはならない。
      if (image !== null && error?.status === api.TOO_LARGE) {
        return api.postComment({ ...request, image: null });
      }
      throw error;
    });
    const before = state.allComments;
    state.allComments = [...state.allComments, comment];
    live.pageCommentSaved = true;
    const notice = imageUnsavedNotice(comment) || (imageError ? `The comment was saved without its image: ${imageError}` : "");
    if (notice !== "") {
      live.refNotice = notice;
      liveActions.renderReference();
    }
    refreshCommentBadges(before);
    renderHeader();
    renderConversation();
    live.composeError = "";
    live.draft = emptyDraft(live.page, live.width);
    showDraftBody();
    sendPlaces();
  } catch (error) {
    live.composeError = `Not saved: ${error instanceof Error ? error.message : String(error)}`;
  } finally {
    setSaving(false);
  }
}

/**
 * 保存の始まりと終わり。保存している間は書きかけ・表示幅・ページを変える操作を使えないと出す。
 * @param {boolean} saving
 */
function setSaving(saving) {
  state.liveSaving = saving;
  liveActions.renderBand();
  renderCompose();
  renderTree();
  renderConversation();
}
