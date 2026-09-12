// @ts-check
// 差分の表示そのもの: 仮想スクロールの窓、行の高さの測り直し、上端の行の記録、
// 折りたたみの展開と畳み直し、表示モードと折返し。

import * as api from "../api.js";
import { dom, el, focusKeyWithin, restoreFocusKey } from "../dom.js";
import {
  OVERSCAN,
  ROW_HEIGHT,
  currentEntry,
  originShown,
  state,
} from "../state.js";
import { saveMode } from "../storage.js";
import {
  anchorIndex,
  carryHeights,
  collapseDefault,
  collapseLoadedRows,
  commentedLines,
  displayRowKey,
  lineOffsets,
  navStops,
  placeThreads,
  rangeAfterSkip,
  rowAtOffset,
  toDisplayLines,
  windowFor,
  withRowIndex,
} from "../model.js";
import { renderCommentOrEditor, renderEditor } from "../views/comment.js";
import { renderBlock } from "../views/diff-rows.js";
import { renderFileHeader, renderNotice } from "../views/file-header.js";
import { renderHeader } from "../views/header.js";
import { renderNav, renderRuler } from "../views/nav.js";

/** @typedef {import("../model.js").LogicalRow} LogicalRow */
/** @typedef {import("../state.js").Entry} Entry */

/** 測り直しの後の描き直しを、1 フレームに 1 回だけにする。 */
let rendering = false;

/**
 * 基準値と違う高さの行は、中身が変わったかもしれない。窓の外の行も含めて、次に
 * 描いたときに測り直すよう覚えておく。
 */
function markRowsOffDefaultHeight() {
  state.heights.forEach((height, index) => {
    if (height !== ROW_HEIGHT) {
      state.staleRows.add(index);
    }
  });
}

/**
 * エディタやコメント、由来の理由の出入りで、次の描画で表示中の行の高さを測り直させる。
 * 見えていない行の測った高さは残す。捨てると、上の行が詰まって見ている位置がずれる。
 * 基準値と違う高さの行は中身が変わったかもしれないので、窓の外の行も、次に描いたときに
 * 測り直すよう覚えておく。
 */
export function remeasure() {
  // 利用者の操作で行の高さが変わったら、送った先を画面の同じ位置に置き直すのをやめる。
  // 置き直すと、その先より上で開いたエディタや吹き出しの高さだけスクロール位置が送られ、
  // いま触った行が画面の外へ動く。
  state.landing = null;
  state.measureNext = true;
  markRowsOffDefaultHeight();
}

/**
 * 作り直した行のうち、基準値と違う高さで見えているかもしれない行を、次に描いたときに
 * 測り直すよう覚えておく。測り直した分は measureHeights がスクロール位置で打ち消す。
 */
function markStaleRows() {
  state.measureNext = true;
  markRowsOffDefaultHeight();
  // 吹き出しの分だけ高い行は、高さを引き継げなかったときも測り直させる。
  state.threads.byLine.forEach((_threads, index) => {
    state.staleRows.add(index);
  });
  if (state.originOpen.size > 0) {
    const entry = currentEntry();
    const fileId = entry ? entry.file.id : "";
    state.display.forEach((line, index) => {
      if (line.kind === "origin" && state.originOpen.has(`${fileId}:${line.block}`)) {
        state.staleRows.add(index);
      }
    });
  }
}

/** 表示する行が変わったときや折返しを切り替えたとき、行の高さを基準値へ戻す。 */
export function resetHeights() {
  state.heights = new Array(state.display.length).fill(ROW_HEIGHT);
  state.staleRows = new Set();
  // 位置の帯の印は行の高さから描く。測る行が無くても、古い高さの印を残さない。
  state.rulerDirty = true;
  // 送った先の記録は古い行と高さでの位置なので、もう同じ行を指さない。
  state.landing = null;
}

/**
 * @typedef {{ key: string, row: number, margin: number }} Anchor
 */

/**
 * 表示領域の上端に見えている行と、その行の上端が上端から下に何画素あるか（見切れて
 * いれば負）。表示行を作り直しても、この行を同じ位置に見せるための記録。
 * @returns {Anchor | null}
 */
export function captureAnchor() {
  if (state.display.length === 0) {
    return null;
  }
  const offsets = lineOffsets(state.heights);
  const scrollTop = dom.viewport.scrollTop;
  const index = rowAtOffset(offsets, scrollTop);
  const line = state.display[index];
  if (!line) {
    return null;
  }
  return { key: displayRowKey(line), row: line.row, margin: (offsets[index] ?? 0) - scrollTop };
}

/**
 * 覚えておいた行を元の位置へ戻す。その行が折りたたみに入って消えたときは、ファイルの
 * 同じ位置から後で最初に残っている行を同じ位置に置く。
 * @param {Anchor | null} anchor
 */
function restoreAnchor(anchor) {
  if (!anchor || state.display.length === 0) {
    return;
  }
  const index = anchorIndex(state.display, anchor);
  if (index < 0) {
    return;
  }
  const offsets = lineOffsets(state.heights);
  // 全体の高さを先に広げる。古い高さのままでは、戻す位置が末尾で切り詰められる。
  dom.content.style.height = `${offsets[offsets.length - 1] || 0}px`;
  const top = offsets[index] ?? 0;
  dom.viewport.scrollTop = Math.max(0, top - anchor.margin);
  // この後の測り直しで上の行が伸び縮みしても、この行を同じ位置へ置き直す。
  state.landing = {
    index,
    top,
    scrollTop: dom.viewport.scrollTop,
    margin: anchor.margin,
  };
}

/** 送った直後で、利用者がまだスクロールしていなければ、その送り先。 */
function activeLanding() {
  const landing = state.landing;
  if (landing && Math.abs(dom.viewport.scrollTop - landing.scrollTop) < 2) {
    return landing;
  }
  return null;
}

/**
 * 描き直しでエディタを作り直す前に、フォーカス中の欄と選択位置を覚える。
 * @returns {{ field: string, start: number, end: number } | null}
 */
function captureEditorFocus() {
  const active = document.activeElement;
  if (!(active instanceof HTMLTextAreaElement)) {
    return null;
  }
  const field = active.dataset.editorField;
  if (!field || !dom.content.contains(active)) {
    return null;
  }
  return { field, start: active.selectionStart, end: active.selectionEnd };
}

/**
 * @param {{ field: string, start: number, end: number } | null} focus
 */
function restoreEditorFocus(focus) {
  if (!focus) {
    return;
  }
  const field = dom.content.querySelector(
    `[data-editor-field="${focus.field}"]`,
  );
  if (!(field instanceof HTMLTextAreaElement)) {
    return;
  }
  field.focus({ preventScroll: true });
  field.setSelectionRange(focus.start, focus.end);
}

export function renderDiff() {
  const entry = currentEntry();
  const focus = captureEditorFocus();
  const focusKey = focusKeyWithin(dom.content);
  dom.content.textContent = "";
  if (
    !entry ||
    state.binary ||
    collapseDefault(entry.file, state.collapsedOverrides)
  ) {
    dom.content.style.height = "0px";
    renderNav([0]);
    renderRuler([0]);
    return;
  }
  const offsets = lineOffsets(state.heights);
  const totalHeight = offsets[offsets.length - 1] || 0;
  dom.content.style.height = `${totalHeight}px`;
  const window = windowFor(
    offsets,
    dom.viewport.scrollTop,
    dom.viewport.clientHeight,
    OVERSCAN,
  );
  if (state.wrap) {
    dom.content.dataset.wrap = "on";
  } else {
    delete dom.content.dataset.wrap;
  }
  const fragment = document.createDocumentFragment();
  for (let index = window.start; index < window.end; index += 1) {
    const block = renderBlock(state.display[index], index);
    block.style.top = `${offsets[index]}px`;
    fragment.append(block);
  }
  dom.content.append(fragment);
  restoreEditorFocus(focus);
  restoreFocusKey(dom.content, focusKey);
  renderNav(offsets);
  renderRuler(offsets);
  const needsMeasure =
    state.measureNext ||
    state.staleRows.size > 0 ||
    state.wrap ||
    state.threads.byLine.size > 0 ||
    state.originOpen.size > 0 ||
    Boolean(state.editor && !state.editor.wide);
  state.measureNext = false;
  if (needsMeasure) {
    measureHeights(window.start, offsets);
  }
}

/** ファイル全体へのコメント（と、表示行に見つからないコメント）を、ヘッダの下に同じ吹き出しで出す。 */
export function renderFloating() {
  const focusKey = focusKeyWithin(dom.floating);
  dom.floating.textContent = "";
  const floating = state.threads.floating;
  const wideEditor = state.editor && state.editor.wide && !state.editor.editId ? state.editor : null;
  if (floating.length === 0 && !wideEditor) {
    dom.floating.hidden = true;
    return;
  }
  dom.floating.hidden = false;
  if (wideEditor) {
    dom.floating.append(renderEditor(wideEditor));
  }
  for (const comment of floating) {
    const row = el("div", "bal-row floating");
    row.append(renderCommentOrEditor(comment));
    dom.floating.append(row);
  }
  restoreFocusKey(dom.floating, focusKey);
}

/**
 * @param {number} start
 * @param {number[]} offsets 描いたときの各行の上端
 */
function measureHeights(start, offsets) {
  const children = Array.from(dom.content.children);
  const scrollTop = dom.viewport.scrollTop;
  // 送った直後は、送り先の行より上の行の伸び縮みをすべて打ち消す。初めて測る行
  // （折り返した行など）が基準値より高いと、送り先が下へずれて見えなくなるため。
  // 利用者の操作の後は打ち消さない（remeasure が送り先を捨てる）。
  const landing = activeLanding();
  let changed = false;
  let shift = 0;
  let growth = 0;
  children.forEach((child, offset) => {
    const index = start + offset;
    const stale = state.staleRows.delete(index);
    const height = /** @type {HTMLElement} */ (child).offsetHeight;
    if (height > 0 && state.heights[index] !== height) {
      const delta = height - state.heights[index];
      const above = landing
        ? index < landing.index
        : stale && offsets[index + 1] <= scrollTop;
      if (above) {
        shift += delta;
      }
      growth += delta;
      state.heights[index] = height;
      changed = true;
    }
  });
  if (changed) {
    state.rulerDirty = true;
  }
  if (shift !== 0) {
    // 見ている位置より上で古い高さのまま残っていた行の伸び縮みを、スクロール位置で
    // 打ち消す。ずれた配置のまま表示されないよう、同じ描画のうちに描き直す。
    // 全体の高さを先に広げる。古い高さのままでは、送った位置が末尾で切り詰められる。
    dom.content.style.height = `${(offsets[offsets.length - 1] || 0) + growth}px`;
    if (landing) {
      const top = (offsets[landing.index] ?? landing.top) + shift;
      dom.viewport.scrollTop = Math.max(0, top - landing.margin);
      state.landing = {
        index: landing.index,
        top,
        scrollTop: dom.viewport.scrollTop,
        margin: landing.margin,
      };
    } else {
      dom.viewport.scrollTop = scrollTop + shift;
    }
    renderDiff();
    return;
  }
  if (!changed) {
    // 測り終えて動かなくなった。ここから先、置き直すものはもう無い。
    state.landing = null;
  }
  if (changed && !rendering) {
    rendering = true;
    requestAnimationFrame(() => {
      rendering = false;
      renderDiff();
    });
  }
}

export function scheduleRender() {
  requestAnimationFrame(() => renderDiff());
}

export function recomputeThreads() {
  state.threads = placeThreads(state.display, state.comments);
  state.commented = commentedLines(state.display, state.comments);
  state.stops = navStops(state.display, state.comments);
  state.rulerDirty = true;
}

/**
 * 表示行を作り直す。作り直しても、上端に見えていた行は同じ位置に残す。
 * @param {Anchor | null} [anchor] 上端に見えていた行。省略するといまの表示から取る。
 */
export function recomputeDisplay(anchor = captureAnchor()) {
  const entry = currentEntry();
  const fileId = entry ? entry.file.id : null;
  // 高さを引き継げるのは、同じファイルを作り直すときだけ。行の見分けはファイルの中の
  // 位置と種類だけで決まるので、別のファイルへ移ったときに引き継ぐと、そのファイルに
  // 無いコメントの吹き出しの高さが同じ位置の行に付いてしまう。
  const sameFile = fileId !== null && fileId === state.displayFileId;
  const previous = sameFile ? state.display : [];
  const previousHeights = sameFile ? state.heights : [];
  state.display = toDisplayLines(withRowIndex(state.rows), state.mode, {
    origin: originShown(),
  });
  state.displayFileId = fileId;
  resetHeights();
  // 測った高さは残る行へ移す。捨てると、見えていない上の行が基準値に詰まって
  // 読んでいた位置が上へずれる。
  state.heights = carryHeights(previous, previousHeights, state.display, ROW_HEIGHT);
  state.skipRanges = new Map();
  state.rows.forEach((row, index) => {
    if (row.kind === "skip") {
      const range = rangeAfterSkip(state.rows, index);
      if (range) {
        state.skipRanges.set(index, range);
      }
    }
  });
  recomputeThreads();
  markStaleRows();
  restoreAnchor(anchor);
}

/**
 * 1 つの折りたたみを、サーバの残りが尽きるまで展開する。
 * @param {number} index
 * @returns {Promise<boolean>} 展開できたら true。
 */
export async function expandSkipAt(index) {
  const entry = currentEntry();
  const skip = state.rows[index];
  if (
    !entry ||
    !skip ||
    skip.kind !== "skip" ||
    skip.from === undefined ||
    skip.to === undefined
  ) {
    return false;
  }
  const generation = state.selectGeneration;
  const cacheKey = state.cacheKey;
  /** @type {LogicalRow[]} */
  const replacement = [];
  let from = Number(skip.from);
  const to = Number(skip.to);
  while (from < to) {
    const data = await api.getFile(
      entry.file.id,
      { from, to },
      {
        dark: state.dark,
        highlight: state.highlightOverrides.get(entry.file.id),
      },
    );
    if (generation !== state.selectGeneration || cacheKey !== state.cacheKey) {
      // 取得中にファイルや表示条件が変わった。古い応答で表示とキャッシュを上書きしない。
      return false;
    }
    replacement.push(...(data.rows || []));
    if (data.next === null || data.next === undefined) {
      break;
    }
    from = Number(data.next);
  }
  if (state.rows[index] !== skip) {
    // 同じ折りたたみが先に展開された。取得前の位置へ挿すと表示行が重複する。
    return false;
  }
  // 50 万行の展開では、行を引数に展開する splice が引数の上限を超えて失敗する。
  const rows = state.rows.slice(0, index).concat(replacement, state.rows.slice(index + 1));
  state.rows = rows;
  state.cache.set(cacheKey, { ...state.cache.get(cacheKey), rows });
  recomputeDisplay();
  renderDiff();
  renderFloating();
  return true;
}

export async function expandAll() {
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  if (collapseDefault(entry.file, state.collapsedOverrides)) {
    markCollapsedShown(entry);
  }
  const generation = state.selectGeneration;
  const cacheKey = state.cacheKey;
  let index = 0;
  while (index < state.rows.length) {
    if (state.rows[index].kind !== "skip") {
      index += 1;
      continue;
    }
    const expanded = await expandSkipAt(index);
    if (!expanded) {
      break;
    }
    index += 1;
  }
  if (generation !== state.selectGeneration || cacheKey !== state.cacheKey) {
    // 取得中に別のファイルへ切り替わった。新しい選択が描画する。
    return;
  }
  renderFileHeader();
  renderDiff();
  renderFloating();
}

export function collapseAll() {
  const data = state.cache.get(state.cacheKey);
  if (!data || !data.collapsedRows || state.rows === data.collapsedRows) {
    return;
  }
  // 最初に読んだ畳んだ形には、展開してから付けたコメントの行が無い。かといって畳む形を
  // サーバから読み直すと、更新バッジを押す前にディスクの新しい内容へ差し替わり（R-LIVE）、
  // ファイルが消えていればレビューが終わる。いまページにある行から畳み直す。
  const rows = collapseLoadedRows(state.rows, state.comments, Number(data.context));
  state.rows = rows;
  state.cache.set(state.cacheKey, { ...data, rows, collapsedRows: rows });
  recomputeDisplay();
  renderFileHeader();
  renderDiff();
  renderFloating();
}

/**
 * 畳んでいた内容（ノイズなど）を表示する。
 * @param {Entry} entry
 */
export function showCollapsed(entry) {
  markCollapsedShown(entry);
  renderDiff();
}

/**
 * 畳んでいた内容を以降は開いた形で出すと決め、サーバにも覚えさせる。行の描き直しは
 * 呼ぶ側が行う（全行の展開では、展開し終えてから 1 回だけ描き直す）。
 * @param {Entry} entry
 */
function markCollapsedShown(entry) {
  state.collapsedOverrides[entry.file.id] = false;
  void api
    .postState({ file_id: entry.file.id, collapsed: false })
    .catch(() => undefined);
  renderNotice();
}

/**
 * 由来の行のコミットを押したときの、そのコミットの理由の開閉。行の高さが変わるので
 * 測り直す。
 * @param {string} openKey
 * @param {string} sha
 */
export function toggleOriginReason(openKey, sha) {
  if (state.originOpen.get(openKey) === sha) {
    state.originOpen.delete(openKey);
  } else {
    state.originOpen.set(openKey, sha);
  }
  remeasure();
  renderDiff();
}

/**
 * @param {"unified" | "split"} mode
 */
export function setMode(mode) {
  state.mode = mode;
  saveMode(mode);
  recomputeDisplay();
  renderHeader();
  renderDiff();
}

/**
 * 折返しを切り替える。行の高さが変わるので測り直すが、上端に見えていた行は動かさない。
 * @param {boolean} wrap
 */
export function setWrap(wrap) {
  const anchor = captureAnchor();
  state.wrap = wrap;
  resetHeights();
  markStaleRows();
  restoreAnchor(anchor);
  renderHeader();
  renderDiff();
}

/**
 * @param {MouseEvent} event
 */
export function scrollToRulerPosition(event) {
  const rect = dom.ruler.getBoundingClientRect();
  const total = lineOffsets(state.heights).at(-1) || 0;
  const ratio = (event.clientY - rect.top) / Math.max(1, rect.height);
  dom.viewport.scrollTop = Math.max(0, ratio * total - dom.viewport.clientHeight / 2);
  scheduleRender();
}

/** 窓の幅が変わると折返しの高さも変わる。帯を描き直し、本文は renderDiff が測り直す。 */
export function onResize() {
  state.rulerDirty = true;
  scheduleRender();
}
