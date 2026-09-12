// @ts-check
// レビューの取り込みと、ファイルの選択・見た・由来・ツリーの絞り込み。

import * as api from "../api.js";
import { dom } from "../dom.js";
import {
  currentEntry,
  fileCacheKey,
  flatten,
  isShowingFile,
  originKey,
  originShown,
  state,
} from "../state.js";
import {
  filterAndSortFiles,
  nextHighlightOverride,
  treeOrder,
} from "../model.js";
import {
  captureAnchor,
  recomputeDisplay,
  renderDiff,
  renderFloating,
  resetHeights,
  scheduleRender,
} from "./display.js";
import { applyPendingJump } from "./navigation.js";
import { renderFileHeader, renderGroupHeader, renderNotice } from "../views/file-header.js";
import {
  renderFooter,
  renderHeader,
  renderProgress,
  renderUpdateBadge,
} from "../views/header.js";
import { showOverlay } from "../views/overlay.js";
import { renderTree, updateGroupHead } from "../views/tree.js";

/** @typedef {import("../model.js").FileEntry} FileEntry */
/** @typedef {import("../state.js").Entry} Entry */

/**
 * レビューの応答を表示に取り込む。コメントは両方の単位の分を持つので、取得し直した
 * 応答（fresh）のときだけ置き換える。
 * @param {any} review
 * @param {boolean} fresh
 */
export function applyReview(review, fresh) {
  state.review = review;
  state.unit = review.unit ?? null;
  if (Array.isArray(review.units)) {
    state.units = review.units;
  }
  if (fresh) {
    state.allComments = review.comments || [];
  }
  if (state.unit) {
    state.reviews.set(state.unit, review);
  }
  state.entries = flatten(review);
  state.treeItems = new Map();
  state.treeVersion += 1;
  rebuildVisible();
}

/**
 * @param {string} [keepId]
 */
function rebuildVisible(keepId) {
  const byId = new Map(state.entries.map((entry) => [entry.file.id, entry]));
  const files = filterAndSortFiles(
    [...byId.values()].map((entry) => entry.file),
    { focusOnly: state.focusOnly, sortBySize: state.sortBySize },
  );
  // n / p と j / k が次のファイルを選ぶ順は、ツリーに描かれる順（R-NAV）。
  state.visible = treeOrder(/** @type {Entry[]} */ (files.map((file) => byId.get(file.id))));
  state.treeVersion += 1;
  if (keepId !== undefined) {
    const found = state.visible.findIndex((entry) => entry.file.id === keepId);
    if (found >= 0) {
      state.index = found;
      state.current = state.visible[found];
    }
  }
  state.index = Math.max(0, Math.min(state.visible.length - 1, state.index));
}

export async function refresh() {
  if (state.submitted) {
    return;
  }
  state.updateAvailable = false;
  renderUpdateBadge();
  const scrollTop = dom.viewport.scrollTop;
  // 取得中の展開応答が、新しいレビューで消えたキャッシュへ古い行を戻さないようにする。
  state.selectGeneration += 1;
  state.cache.clear();
  state.commentStore.clear();
  state.origins.clear();
  // 取得中の古い単位の応答が、新しいレビューの控えに入らないよう入れ物ごと替える。
  state.reviews = new Map();
  applyReview(await api.getReview(true, state.unit), true);
  const current = currentEntry();
  const keepId = current ? current.file.id : undefined;
  const keep =
    keepId === undefined
      ? undefined
      : state.entries.find((entry) => entry.file.id === keepId);
  rebuildVisible(keepId);
  renderHeader();
  renderTree();
  renderFooter();
  if (!keep && state.visible.length === 0) {
    state.current = null;
    renderGroupHeader();
    renderFileHeader();
    renderNotice();
    return;
  }
  if (keep) {
    await selectEntry(keep, { scrollTop: false });
  } else {
    await selectIndex(state.index, { scrollTop: false });
  }
  // 読み直しの前に見ていた位置へ戻す。
  dom.viewport.scrollTop = scrollTop;
  scheduleRender();
}

/**
 * @param {number} index
 * @param {{ scrollTop?: boolean }} [options]
 */
export async function selectIndex(index, options = { scrollTop: true }) {
  if (state.visible.length === 0) {
    renderGroupHeader();
    renderFileHeader();
    renderNotice();
    return;
  }
  state.index = Math.max(0, Math.min(state.visible.length - 1, index));
  await selectEntry(state.visible[state.index], options);
}

/**
 * @param {Entry} entry
 * @param {{ scrollTop?: boolean, keepEditor?: boolean }} [options]
 */
export async function selectEntry(entry, options = { scrollTop: true }) {
  if (state.submitted) {
    // 送信後はサーバが止まっていて、まだ読んでいないファイルは取れない。完了画面を残す。
    return;
  }
  // テーマや構文ハイライトの切り替えは同じファイルを読み直すだけ。読んでいた位置を残す。
  const anchor =
    !options.scrollTop && state.current && state.current.file.id === entry.file.id
      ? captureAnchor()
      : null;
  state.current = entry;
  state.landing = null;
  const id = entry.file.id;
  const override = state.highlightOverrides.get(id);
  const key = fileCacheKey(entry);
  state.cacheKey = key;
  const generation = ++state.selectGeneration;
  if (!state.cache.has(key)) {
    // ヘッダは取得を待たずに新しいファイルへ切り替え、取得中は操作できなくする。
    state.loading = true;
    state.rows = [];
    state.display = [];
    state.displayFileId = null;
    resetHeights();
    state.threads = { byLine: new Map(), floating: [] };
    renderTree();
    renderGroupHeader();
    renderFileHeader();
    renderNotice();
    renderFloating();
    renderDiff();
    /** @type {any} */
    let data;
    try {
      data = await api.getFile(id, null, {
        dark: state.dark,
        highlight: override,
      });
    } catch (error) {
      if (generation === state.selectGeneration) {
        // 読み込み中のまま止まらないよう戻す。選び直せば取り直す。
        state.loading = false;
        renderFileHeader();
        renderNotice();
        renderDiff();
        showOverlay("ファイルを読み込めません", String(error));
      }
      return;
    }
    if (generation !== state.selectGeneration) {
      // 取得中に別のファイルが選ばれた。古い応答で表示を上書きしない。
      return;
    }
    const rows = data.rows || [];
    state.cache.set(key, { ...data, rows, collapsedRows: rows });
  }
  state.loading = false;
  const data = state.cache.get(key);
  state.rows = data.rows || [];
  state.binary = Boolean(data.binary);
  state.highlightCapable = Boolean(data.highlight && data.highlight.capable);
  state.highlightEnabled = Boolean(data.highlight && data.highlight.enabled);
  const storedComments = state.commentStore.get(id);
  state.comments = storedComments || data.comments || [];
  state.commentStore.set(id, state.comments);
  if (!options.keepEditor) {
    state.selection = null;
    state.editor = null;
  }
  if (options.scrollTop) {
    dom.viewport.scrollTop = 0;
  }
  recomputeDisplay(anchor);
  renderTree();
  renderGroupHeader();
  renderFileHeader();
  renderNotice();
  renderFloating();
  renderDiff();
  applyPendingJump();
  void loadOrigin(entry);
}

/**
 * @param {FileEntry} file
 */
export async function toggleSeen(file) {
  if (state.submitted || state.loading || !isShowingFile(file.id)) {
    return;
  }
  const next = !file.seen;
  file.seen = next;
  state.treeItems.get(file.id)?.classList.toggle("seen", next);
  const entry = currentEntry();
  if (entry) {
    updateGroupHead(entry.group.id);
    renderGroupHeader();
  }
  renderProgress();
  renderFileHeader();
  try {
    await api.postState({ file_id: file.id, seen: next });
  } catch {
    // 表示は先に更新し、失敗は次回の取得で戻る
  }
}

/**
 * 由来は差分の表示を待たせず、後から取りに行って付ける（R-ORIGIN）。
 * @param {Entry} entry
 */
async function loadOrigin(entry) {
  const id = entry.file.id;
  const key = originKey(id);
  if (!originShown() || state.origins.has(key)) {
    return;
  }
  state.origins.set(key, "pending");
  try {
    const data = await api.getOrigin(id, state.originForced.has(id));
    /** @type {Map<number, any>} */
    const blocks = new Map();
    for (const block of data.blocks || []) {
      blocks.set(Number(block.row), block);
    }
    state.origins.set(key, { ...data, blocks });
  } catch (error) {
    state.origins.set(key, { failed: String(error), blocks: new Map() });
  }
  if (isShowingFile(id)) {
    renderDiff();
  }
}

/**
 * @param {string} path
 * @param {HTMLButtonElement} element
 */
export async function copyPath(path, element) {
  try {
    await navigator.clipboard.writeText(path);
    element.classList.add("copied");
    element.title = "コピーしました";
    window.setTimeout(() => {
      element.classList.remove("copied");
      element.title = "パスをコピー";
    }, 1_500);
  } catch (error) {
    showOverlay("コピーできません", String(error));
  }
}

/**
 * ハイライトを、有効 → 無効 → 自動の順に切り替える。表示色の読み直しだけなので、
 * 読んでいた位置は残す。
 * @param {Entry} entry
 */
export function toggleHighlight(entry) {
  const next = nextHighlightOverride(
    state.highlightEnabled,
    state.highlightCapable,
  );
  if (next === null) {
    state.highlightOverrides.delete(entry.file.id);
  } else {
    state.highlightOverrides.set(entry.file.id, next);
  }
  void selectEntry(entry, { scrollTop: false });
}

/**
 * 上限を超えるファイルで、由来を求めるかどうかを切り替える。
 * @param {Entry} entry
 */
export function toggleOrigin(entry) {
  if (state.originForced.has(entry.file.id)) {
    state.originForced.delete(entry.file.id);
  } else {
    state.originForced.add(entry.file.id);
  }
  recomputeDisplay();
  renderFileHeader();
  renderDiff();
  void loadOrigin(entry);
}

/** 重要が付いたファイルだけを出す（R-FOCUS: ツリーだけを絞り、本文の表示は変えない）。 */
export function toggleFocusOnly() {
  state.focusOnly = !state.focusOnly;
  rebuildAndRenderTree();
}

/** 増減数の多い順に並べ替える。 */
export function toggleSortBySize() {
  state.sortBySize = !state.sortBySize;
  rebuildAndRenderTree();
}

/** ツリーに出す並びを作り直し、上部バーとツリーを描き直す。表示中のファイルは変えない。 */
function rebuildAndRenderTree() {
  const current = currentEntry();
  rebuildVisible(current ? current.file.id : undefined);
  renderHeader();
  renderTree();
}
