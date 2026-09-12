// @ts-check
// kemi のページ。仮想スクロールで表示中の行だけを DOM に載せる。

import * as api from "./api.js";
import { bindActions } from "./actions.js";
import {
  collapseAll,
  expandAll,
  expandSkipAt,
  onResize,
  recomputeDisplay,
  recomputeThreads,
  remeasure,
  renderDiff,
  renderFloating,
  resetHeights,
  captureAnchor,
  scheduleRender,
  scrollToRulerPosition,
  setMode,
  setWrap,
  showCollapsed,
  toggleOriginReason,
} from "./features/display.js";
import { renderCommentList } from "./views/comment-list.js";
import {
  renderFileHeader,
  renderGroupHeader,
  renderNotice,
} from "./views/file-header.js";
import {
  refreshCommentBadges,
  renderTree,
  updateGroupHead,
} from "./views/tree.js";
import {
  renderFooter,
  renderHeader,
  renderProgress,
  renderSubmitButtons,
  renderUnitSwitch,
  renderUpdateBadge,
} from "./views/header.js";
import { navView } from "./views/nav.js";
import {
  closeModal,
  runModalAction,
  setModalText,
  showCompletion,
  showOverlay,
  showToast,
} from "./views/overlay.js";
import {
  dom,
  el,
  textEl,
} from "./dom.js";
import {
  NAV_MARGIN,
  THEME_LABELS,
  UNIT_LABELS,
  commentsOf,
  currentEntry,
  fileCacheKey,
  flatten,
  isShowingFile,
  originKey,
  originShown,
  reachableStops,
  selectionText,
  state,
} from "./state.js";
import {
  clearDraft,
  loadDraft,
  saveTheme,
} from "./storage.js";
import {
  commentLabel,
  firstLine,
  hasLoadedStops,
  hasStops,
  navNextTarget,
  navPrevTarget,
  nextFileIndex,
  submitSummary,
  unitSwitchTarget,
  draftKey,
  filterAndSortFiles,
  isDarkTheme,
  keyAction,
  lineHasAnchor,
  lineOffsets,
  nextHighlightOverride,
  nextTheme,
  resolveTheme,
  treeOrder,
} from "./model.js";

/** @typedef {import("./model.js").FileEntry} FileEntry */
/** @typedef {import("./model.js").LogicalRow} LogicalRow */
/** @typedef {import("./model.js").DisplayLine} DisplayLine */
/** @typedef {import("./state.js").Entry} Entry */
/** @typedef {import("./state.js").Editor} Editor */

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

async function refresh() {
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
  if (keep) {
    await selectEntry(keep, { scrollTop: false });
    dom.viewport.scrollTop = scrollTop;
    scheduleRender();
  } else if (state.visible.length > 0) {
    await selectIndex(state.index, { scrollTop: false });
    dom.viewport.scrollTop = scrollTop;
    scheduleRender();
  } else {
    state.current = null;
    renderGroupHeader();
    renderFileHeader();
    renderNotice();
  }
}

/**
 * ハイライトを、有効 → 無効 → 自動の順に切り替える。表示色の読み直しだけなので、
 * 読んでいた位置は残す。
 * @param {Entry} entry
 */
function toggleHighlight(entry) {
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
function toggleOrigin(entry) {
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
async function copyPath(path, element) {
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
 * コメントの本文と suggestion を編集する。行レンジは変えない（R-COMMENT）。
 * @param {any} comment
 */
function openCommentEditor(comment) {
  const entry = currentEntry();
  if (!entry || state.submitted) {
    return;
  }
  const wide = comment.start_line === null || comment.start_line === undefined;
  state.selection = null;
  state.editor = {
    fileId: entry.file.id,
    side: comment.side,
    start: wide ? 0 : Number(comment.start_line),
    end: wide ? 0 : Number(comment.end_line ?? comment.start_line),
    anchor: wide ? 0 : Number(comment.end_line ?? comment.start_line),
    wide,
    body: comment.body,
    suggestion: comment.suggestion ? comment.suggestion.replacement : "",
    suggestionOn: Boolean(comment.suggestion),
    needsFocus: true,
    editId: comment.id,
  };
  remeasure();
  renderDiff();
  renderFloating();
}

/**
 * 削除は確認を 1 回挟む（R-COMMENT）。
 * @param {any} comment
 */
function confirmDeleteComment(comment) {
  dom.modalTitle.textContent = "コメントを削除しますか？";
  setModalText(`${commentLabel(comment)}: ${firstLine(comment.body)}\n削除したコメントは送信する JSON に含まれません。`);
  dom.modalOk.textContent = "削除";
  dom.modalOk.className = "btn secondary";
  dom.modalCancel.textContent = "戻る";
  state.modalAction = () => void deleteComment(comment);
  dom.modal.hidden = false;
}

/**
 * 表示とキャッシュのコメントを差し替える。
 * @param {(comments: any[]) => any[]} change
 */
function updateComments(change) {
  const before = state.allComments;
  state.allComments = change(state.allComments);
  for (const [id, comments] of state.commentStore) {
    state.commentStore.set(id, change(comments));
  }
  state.comments = change(state.comments);
  recomputeThreads();
  remeasure();
  refreshCommentBadges(before);
  renderHeader();
  renderFileHeader();
  renderDiff();
  renderFloating();
  if (!dom.commentList.hidden) {
    renderCommentList();
  }
}

/**
 * @param {any} comment
 */
async function deleteComment(comment) {
  try {
    await api.postComment({ op: "delete", id: comment.id });
    updateComments((comments) => comments.filter((item) => item.id !== comment.id));
  } catch (error) {
    showOverlay("コメントを削除できません", String(error));
  }
}

/**
 * コメントの吹き出しの開閉。行の高さが変わるので、次の描画で測り直させる。
 * @param {string} id
 * @param {boolean} open
 */
function setCommentOpen(id, open) {
  state.commentOpen.set(id, open);
  remeasure();
  renderDiff();
  renderFloating();
}

function closeEditor() {
  state.editor = null;
  state.selection = null;
  remeasure();
  renderDiff();
  renderFloating();
}

function openFileWideEditor() {
  if (state.submitted) {
    return;
  }
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  state.selection = null;
  state.editor = {
    fileId: entry.file.id,
    side: "new",
    start: 0,
    end: 0,
    anchor: 0,
    wide: true,
    body: loadDraft(draftKey(entry.file.id, null)),
    suggestion: "",
    suggestionOn: false,
    needsFocus: true,
  };
  renderDiff();
  renderFloating();
}

/**
 * @param {string} side
 * @param {number} number
 */
function openEditorAt(side, number) {
  if (state.submitted) {
    return;
  }
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  const selection = state.selection;
  let start = number;
  let end = number;
  if (
    selection &&
    selection.fileId === entry.file.id &&
    selection.side === side &&
    number >= selection.start &&
    number <= selection.end
  ) {
    start = selection.start;
    end = selection.end;
  } else {
    state.selection = {
      fileId: entry.file.id,
      side,
      start,
      end,
      anchor: number,
    };
  }
  state.editor = {
    fileId: entry.file.id,
    side,
    start,
    end,
    anchor: number,
    wide: false,
    body: loadDraft(draftKey(entry.file.id, { side, start, end })),
    suggestion: selectionText(),
    suggestionOn: false,
    needsFocus: true,
  };
  remeasure();
  renderDiff();
  renderFloating();
}

/**
 * @param {"old" | "new"} side
 * @param {number} number
 */
function startSelection(side, number) {
  if (state.submitted) {
    return;
  }
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  state.dragging = { fileId: entry.file.id, side };
  state.selection = {
    fileId: entry.file.id,
    side,
    anchor: number,
    start: number,
    end: number,
  };
  renderDiff();
}

/**
 * @param {string} side
 * @param {number} number
 */
function extendSelection(side, number) {
  const drag = state.dragging;
  const entry = currentEntry();
  if (!drag || !entry || drag.fileId !== entry.file.id || drag.side !== side) {
    return;
  }
  const selection = state.selection;
  const anchor = selection ? selection.anchor : number;
  state.selection = {
    fileId: entry.file.id,
    side,
    anchor,
    start: Math.min(anchor, number),
    end: Math.max(anchor, number),
  };
  renderDiff();
}

/**
 * @param {any} payload
 */
async function addComment(payload) {
  try {
    const comment = await api.postComment(payload);
    const before = state.allComments;
    state.allComments = [...state.allComments, comment];
    state.commentOpen.set(comment.id, true);
    refreshCommentBadges(before);
    renderHeader();
    // 応答までに別のファイルへ切り替わっていても、足すのは送信先の
    // コメントだけ。表示中の state は送信先を表示中のときだけ更新する。
    const stored = state.commentStore.get(payload.file_id);
    if (stored) {
      const comments = [...stored, comment];
      state.commentStore.set(payload.file_id, comments);
      if (isShowingFile(payload.file_id)) {
        state.comments = comments;
        state.editor = null;
        state.selection = null;
        recomputeThreads();
        renderDiff();
        renderFloating();
        renderFileHeader();
      }
    }
    const selection =
      payload.start_line === null || payload.start_line === undefined
        ? null
        : {
            side: payload.side,
            start: payload.start_line,
            end: payload.end_line,
          };
    clearDraft(draftKey(payload.file_id, selection));
  } catch (error) {
    showOverlay("コメントを追加できません", String(error));
  }
}

/**
 * @param {any} payload
 */
async function editComment(payload) {
  try {
    const updated = await api.postComment(payload);
    state.editor = null;
    updateComments((comments) =>
      comments.map((item) => (item.id === updated.id ? updated : item)),
    );
  } catch (error) {
    showOverlay("コメントを編集できません", String(error));
  }
}

/**
 * 送信の前の確認。取り消せないことと verdict に加えて、送るコメントの件数（両方の
 * グループ単位の合計）と、表示中の単位の見たファイル数を出す（R-SUBMIT）。
 * @param {"approved" | "changes_requested"} verdict
 */
function openConfirm(verdict) {
  if (state.submitted) {
    return;
  }
  state.modalAction = () => void submitReview(verdict);
  dom.modalCancel.textContent = "戻る";
  const approve = verdict === "approved";
  const summary = submitSummary(
    state.allComments,
    state.entries.map((entry) => entry.file),
  );
  dom.modalTitle.textContent = approve ? "承認して終了しますか？" : "変更要求で終了しますか？";
  dom.modalBody.textContent = "";
  const list = el("dl", "sum");
  const seenLabel = state.unit ? `見たファイル（${UNIT_LABELS[state.unit] || state.unit}）` : "見たファイル";
  list.append(
    textEl("dt", "", "コメント"),
    textEl("dd", "", `${summary.comments} 件（うち suggestion 付き ${summary.suggestions} 件）`),
    textEl("dt", "", seenLabel),
    textEl("dd", "", `${summary.seen} / ${summary.total}`),
  );
  dom.modalBody.append(list);
  if (summary.unseen > 0) {
    dom.modalBody.append(
      textEl("p", "warn", `まだ見ていないファイルが ${summary.unseen} あります。`),
    );
  }
  dom.modalBody.append(
    textEl(
      "p",
      "",
      `レビューを終了して、${approve ? "承認" : "変更要求"}の verdict とコメントを実行ターミナルへ JSON で返します。この操作は取り消せません。`,
    ),
  );
  dom.modalOk.textContent = approve ? "承認して終了" : "変更要求で終了";
  dom.modalOk.className = approve ? "btn primary" : "btn secondary";
  dom.modal.hidden = false;
}

/**
 * @param {"approved" | "changes_requested"} verdict
 */
async function submitReview(verdict) {
  if (state.submitted) {
    return;
  }
  try {
    const answer = await api.submit(verdict);
    state.submitted = true;
    state.selection = null;
    state.editor = null;
    renderSubmitButtons();
    renderDiff();
    renderFloating();
    renderFileHeader();
    showCompletion(verdict, answer);
  } catch (error) {
    showOverlay("送信できませんでした", String(error));
  }
}

/**
 * 表示行 `index` を、上端から NAV_MARGIN の位置へ送る。
 * @param {number} index
 * @param {number[]} offsets
 */
function scrollToRow(index, offsets) {
  const top = offsets[index] ?? 0;
  dom.viewport.scrollTop = Math.max(0, top - NAV_MARGIN);
  // 折り返しのある行は、初めて描いたときに基準値より高くなる。送り先より上でそれが
  // 起きるとこの行が下へずれるので、送り先を覚えておき、measureHeights が測り終える
  // まで同じ位置へ置き直す。置く場所を決めるためだけの記録で、「現在」や n / p の
  // 行き先はこれを見ない（表示の状態から毎回決める。R-NAV）。
  state.landing = { index, top, scrollTop: dom.viewport.scrollTop, margin: NAV_MARGIN };
  scheduleRender();
}

/**
 * 移ったファイルが見えるよう、ツリーのグループとディレクトリをそこまで開く。
 * @param {Entry} entry
 */
function revealInTree(entry) {
  let changed = state.groupOpen.get(entry.group.id) === false;
  state.groupOpen.set(entry.group.id, true);
  const segments = entry.file.path.split("/");
  let prefix = "";
  for (let index = 0; index < segments.length - 1; index += 1) {
    prefix = prefix ? `${prefix}/${segments[index]}` : segments[index];
    const key = `${entry.group.id}:${prefix}`;
    if (state.dirOpen.get(key) === false) {
      changed = true;
    }
    state.dirOpen.set(key, true);
  }
  if (changed) {
    state.treeVersion += 1;
  }
}

/**
 * n / p: ファイルの中の次（前）の止まる場所へ。端では、見えている順で次（前）のファイルの
 * 最初（最後）の止まる場所へ移る。最後（最初）のファイルでは止まって知らせる（R-NAV）。
 * @param {1 | -1} direction
 */
async function navigate(direction) {
  const entry = currentEntry();
  if (!entry || state.loading || state.navigating || state.submitted) {
    return;
  }
  const offsets = lineOffsets(state.heights);
  const stops = reachableStops();
  const tops = stops.map((index) => offsets[index] ?? 0);
  const view = navView();
  const index =
    direction > 0
      ? navNextTarget(tops, view, NAV_MARGIN)
      : navPrevTarget(tops, view, NAV_MARGIN);
  if (index !== null) {
    scrollToRow(stops[index], offsets);
    return;
  }
  const visible = state.visible;
  const files = visible.map((candidate) => candidate.file);
  const here = files.findIndex((file) => file.id === entry.file.id);
  const generation = state.selectGeneration;
  state.navigating = true;
  scheduleRender();
  try {
    let from = here < 0 ? state.index : here;
    for (;;) {
      const next = nextFileIndex(files, from, direction, (file) => {
        const candidate = visible.find((item) => item.file.id === file.id);
        return hasStops(file, candidate ? commentsOf(candidate) : [], state.collapsedOverrides);
      });
      if (next === null) {
        showToast(direction > 0 ? "最後の変更です" : "最初の変更です");
        return;
      }
      // 増減数があっても、改行コードだけの変更などは表示で変更ブロックにならない。
      // 内容を読んで止まる場所が無ければ、移らずにその次を探す（R-NAV）。
      const data = await loadForNavigation(visible[next], generation);
      if (generation !== state.selectGeneration || state.visible !== visible) {
        // 読んでいる間に別のファイルが選ばれたか、並びが変わった。
        return;
      }
      if (data === null || hasLoadedStops(data.rows || [], commentsOf(visible[next]))) {
        state.navigating = false;
        revealInTree(visible[next]);
        state.pendingJump = direction > 0 ? "first" : "last";
        await selectIndex(next, { scrollTop: true });
        return;
      }
      from = next;
    }
  } finally {
    state.navigating = false;
    scheduleRender();
  }
}

/**
 * n / p の移り先の候補の行データを読み、キャッシュに入れる。読めなければ null を返し、
 * 移った先でいつもどおり読み込みの失敗を出す。
 * @param {Entry} entry
 * @param {number} generation 読み始めた時の選択の世代
 * @returns {Promise<any>}
 */
async function loadForNavigation(entry, generation) {
  const key = fileCacheKey(entry);
  if (state.cache.has(key)) {
    return state.cache.get(key);
  }
  /** @type {any} */
  let data;
  try {
    data = await api.getFile(entry.file.id, null, {
      dark: state.dark,
      highlight: state.highlightOverrides.get(entry.file.id),
    });
  } catch {
    return null;
  }
  if (generation !== state.selectGeneration) {
    // 読み直し（テーマや更新）で消えたキャッシュへ、古い行を戻さない。
    return null;
  }
  const rows = data.rows || [];
  const stored = { ...data, rows, collapsedRows: rows };
  state.cache.set(key, stored);
  return stored;
}

/** ファイルを表示し終えた後の移り先（次のファイルの最初の変更や、由来の該当行）へ移る。 */
function applyPendingJump() {
  const jump = state.pendingJump;
  state.pendingJump = null;
  if (!jump) {
    return;
  }
  const offsets = lineOffsets(state.heights);
  const stops = reachableStops();
  if (jump === "first" || jump === "last") {
    if (stops.length > 0) {
      scrollToRow(jump === "first" ? stops[0] : stops[stops.length - 1], offsets);
    }
    return;
  }
  const index = state.display.findIndex((line) => lineHasAnchor(line, jump.side, jump.line));
  if (index >= 0) {
    scrollToRow(index, offsets);
  }
}

/**
 * レビューの応答を表示に取り込む。コメントは両方の単位の分を持つので、取得し直した
 * 応答（fresh）のときだけ置き換える。
 * @param {any} review
 * @param {boolean} fresh
 */
function applyReview(review, fresh) {
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
 * グループ単位を切り替える（R-UNIT）。同じパスのファイル（コミットごとではそのパスを含む
 * 最初のコミット）を出す。由来やコメント一覧から移るときは、そのファイルの該当行を出す。
 * 移り先が無ければ（履歴の書き換えで消えたなど）別のファイルへは移らず、切り替えもせず
 * `missing` を呼ぶ。
 * @param {string} unit
 * @param {{ find: (entries: Entry[]) => number, side: string, line: number | null, missing: () => void } | null} jump
 */
async function switchUnit(unit, jump) {
  if (state.submitted || (unit === state.unit && !jump)) {
    return;
  }
  const status = state.units.find((candidate) => candidate.unit === unit);
  if (!status) {
    return;
  }
  if (status.state === "failed") {
    openUnitFailure(status);
    return;
  }
  if (status.state !== "ready") {
    state.pendingUnit = { unit, jump };
    renderUnitSwitch();
    showToast(`${UNIT_LABELS[unit] || unit}を読み込み中です`);
    // 手元の状態が古いこともあるので読み直す。できていればそのまま切り替わる。
    void onUnitEvent();
    return;
  }
  state.pendingUnit = null;
  const path = currentEntry()?.file.path ?? "";
  let review = unit === state.unit ? state.review : state.reviews.get(unit);
  const fresh = !review;
  if (!review) {
    try {
      review = await api.getReview(false, unit);
    } catch (error) {
      showOverlay("グループ単位を切り替えられません", String(error));
      return;
    }
  }
  if (jump && jump.find(flatten(review)) < 0) {
    // 読んだ単位は控えておき、コメント一覧で消えたコミットを見分けられるようにする。
    if (fresh) {
      state.reviews.set(unit, review);
    }
    renderUnitSwitch();
    jump.missing();
    return;
  }
  applyReview(review, fresh);
  let index = jump ? jump.find(state.entries) : -1;
  if (index < 0) {
    index = unitSwitchTarget(state.entries, path);
  }
  renderHeader();
  renderFooter();
  if (index < 0) {
    state.current = null;
    renderTree();
    renderGroupHeader();
    renderFileHeader();
    renderNotice();
    renderDiff();
    return;
  }
  const entry = state.entries[index];
  if (jump) {
    state.pendingJump = jump.line === null ? null : { side: jump.side, line: jump.line };
    revealInTree(entry);
  }
  const visibleIndex = state.visible.findIndex((candidate) => candidate.file.id === entry.file.id);
  if (visibleIndex >= 0) {
    state.index = visibleIndex;
  }
  await selectEntry(entry, { scrollTop: true });
}

/**
 * 作れなかった単位の理由と、作り直しの操作。
 * @param {any} status
 */
function openUnitFailure(status) {
  const unit = String(status.unit);
  dom.modalTitle.textContent = `${UNIT_LABELS[unit] || unit}の単位を作れなかった`;
  setModalText(`理由: ${status.error || "不明"}\nレビューはこのまま続けられます。`);
  dom.modalOk.textContent = "再試行";
  dom.modalOk.className = "btn primary";
  dom.modalCancel.textContent = "閉じる";
  state.modalAction = () => {
    state.pendingUnit = { unit, jump: null };
    api
      .retryUnit(unit)
      .then((answer) => {
        state.units = answer.units || state.units;
        renderUnitSwitch();
      })
      .catch((error) => showOverlay("作り直せませんでした", String(error)));
  };
  dom.modal.hidden = false;
}

function openCommentList() {
  renderCommentList();
  dom.commentList.hidden = false;
  dom.btnComments.setAttribute("aria-expanded", "true");
  void loadCommitGroups();
}

/**
 * コメント一覧で消えたコミットを見分けられるよう、作ってあるコミットごとの単位を
 * まだ読んでいなければ読む（再取得の後は表示中の単位しか控えていない）。
 */
async function loadCommitGroups() {
  const status = state.units.find((candidate) => candidate.unit === "commit");
  if (!status || status.state !== "ready" || state.reviews.has("commit")) {
    return;
  }
  const reviews = state.reviews;
  try {
    const review = await api.getReview(false, "commit");
    if (!reviews.has("commit")) {
      reviews.set("commit", review);
    }
  } catch {
    // 読めなければ消えたかどうかを決めないまま。押したときに移り先を確かめる。
    return;
  }
  if (reviews === state.reviews && !dom.commentList.hidden) {
    renderCommentList();
  }
}

function closeCommentList() {
  dom.commentList.hidden = true;
  dom.btnComments.setAttribute("aria-expanded", "false");
}

/**
 * コメント一覧から、そのコメントの単位へ切り替えて、その行へ移る。
 * @param {any} comment
 * @param {string | null} unit
 */
async function goToComment(comment, unit) {
  const line =
    comment.start_line === null || comment.start_line === undefined
      ? null
      : Number(comment.start_line);
  state.commentOpen.set(comment.id, true);
  /** @param {Entry[]} entries */
  const find = (entries) =>
    entries.findIndex(
      (entry) => entry.group.id === comment.group_id && entry.file.path === comment.path,
    );
  if (unit && unit !== state.unit) {
    // 移り先が無ければ、一覧を開き直して（消えたコミットならそう示して）本文を見せる。
    const missing = () => {
      openCommentList();
      showToast("移り先のファイルが見つかりません");
    };
    await switchUnit(unit, { find, side: comment.side, line, missing });
    return;
  }
  const index = find(state.entries);
  if (index < 0) {
    return;
  }
  const entry = state.entries[index];
  revealInTree(entry);
  state.pendingJump = line === null ? null : { side: comment.side, line };
  const visibleIndex = state.visible.findIndex((candidate) => candidate.file.id === entry.file.id);
  if (visibleIndex >= 0) {
    state.index = visibleIndex;
  }
  await selectEntry(entry, { scrollTop: true });
}

/** もう片方の単位の作成の状態が変わった。待っている切り替えがあれば続ける。 */
async function onUnitEvent() {
  if (state.submitted) {
    return;
  }
  const review = await api.getReview(false);
  state.units = review.units || [];
  renderUnitSwitch();
  const pending = state.pendingUnit;
  if (!pending) {
    return;
  }
  const status = state.units.find((candidate) => candidate.unit === pending.unit);
  if (status && status.state === "ready") {
    void switchUnit(pending.unit, pending.jump);
  } else if (status && status.state === "failed") {
    state.pendingUnit = null;
    renderUnitSwitch();
    openUnitFailure(status);
  }
}

/**
 * @param {number} index
 * @param {{ scrollTop?: boolean }} [options]
 */
async function selectIndex(index, options = { scrollTop: true }) {
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
async function selectEntry(entry, options = { scrollTop: true }) {
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
async function toggleSeen(file) {
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

function applyTheme() {
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  const resolved = resolveTheme(state.theme, prefersDark);
  const dark = isDarkTheme(resolved);
  const changed = state.dark !== dark;
  state.dark = dark;
  state.rulerDirty = true;
  document.documentElement.dataset.theme = resolved;
  const current =
    THEME_LABELS[state.theme] || THEME_LABELS.auto;
  const next = THEME_LABELS[nextTheme(state.theme)] || "";
  dom.btnTheme.title = `テーマ: ${current}（クリックで ${next}）`;
  if (changed && !state.submitted) {
    state.cache.clear();
    const entry = currentEntry();
    if (entry) {
      // テーマ切替は表示色の再取得だけ。入力中のエディタは閉じない。
      void selectEntry(entry, { scrollTop: false, keepEditor: true });
    }
  } else if (!changed) {
    // 明暗が同じでもプリセットが変われば追加・削除の色が変わる。位置の帯を描き直す。
    scheduleRender();
  }
}

/**
 * @param {KeyboardEvent} event
 */
function handleKey(event) {
  if (state.submitted) {
    // 送信後は完了画面だけを出す。ファイルの移動や見たの切り替えはしない。
    return;
  }
  if (event.key === "Escape") {
    if (!dom.modal.hidden) {
      closeModal();
      return;
    }
    if (!dom.commentList.hidden) {
      closeCommentList();
      return;
    }
    if (state.editor) {
      closeEditor();
      return;
    }
  }
  const target = /** @type {HTMLElement} */ (event.target);
  if (
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.isContentEditable
  ) {
    return;
  }
  if (event.ctrlKey || event.metaKey || event.altKey || !dom.modal.hidden) {
    return;
  }
  const action = keyAction(
    event.key,
    state.mode,
    state.visible.length,
    state.index,
    state.wrap,
  );
  if (action.type === "file") {
    void selectIndex(Number(action.index), { scrollTop: true });
  } else if (action.type === "mode") {
    setMode(action.mode === "split" ? "split" : "unified");
  } else if (action.type === "nav") {
    void navigate(action.direction === -1 ? -1 : 1);
  } else if (action.type === "seen") {
    const entry = currentEntry();
    if (entry) {
      void toggleSeen(entry.file);
    }
  } else if (action.type === "wrap") {
    setWrap(Boolean(action.value));
  }
}

async function boot() {
  applyReview(await api.getReview(false), true);
  renderHeader();
  renderTree();
  renderFooter();
  if (state.visible.length > 0) {
    await selectIndex(0, { scrollTop: true });
  } else {
    renderNotice();
  }
  api.subscribeEvents(
    () => {
      state.updateAvailable = true;
      renderUpdateBadge();
    },
    () => void onUnitEvent(),
  );
}

document.addEventListener("keydown", handleKey);
document.addEventListener("mouseup", () => {
  state.dragging = null;
});
dom.btnUnified.addEventListener("click", () => setMode("unified"));
dom.btnSplit.addEventListener("click", () => setMode("split"));
dom.btnWrap.addEventListener("click", () => setWrap(!state.wrap));
dom.chipFocus.addEventListener("click", () => {
  // R-FOCUS: このフィルタはツリーだけを絞り、本文の表示は変えない。
  state.focusOnly = !state.focusOnly;
  const current = currentEntry();
  rebuildVisible(current ? current.file.id : undefined);
  renderHeader();
  renderTree();
});
dom.chipSort.addEventListener("click", () => {
  state.sortBySize = !state.sortBySize;
  const current = currentEntry();
  rebuildVisible(current ? current.file.id : undefined);
  renderHeader();
  renderTree();
});
dom.btnTheme.addEventListener("click", () => {
  state.theme = nextTheme(state.theme);
  saveTheme(state.theme);
  applyTheme();
});
dom.updateBadge.addEventListener("click", () => void refresh());
dom.submitApproved.addEventListener("click", () => openConfirm("approved"));
dom.submitChanges.addEventListener("click", () => openConfirm("changes_requested"));
dom.modalCancel.addEventListener("click", closeModal);
dom.modalOk.addEventListener("click", runModalAction);
dom.btnComments.addEventListener("click", () => {
  if (dom.commentList.hidden) {
    openCommentList();
  } else {
    closeCommentList();
  }
});
document.addEventListener("click", (event) => {
  const target = /** @type {Node} */ (event.target);
  if (
    !dom.commentList.hidden &&
    !dom.commentList.contains(target) &&
    !dom.btnComments.contains(target)
  ) {
    closeCommentList();
  }
});
dom.navPrev.addEventListener("click", () => void navigate(-1));
dom.navNext.addEventListener("click", () => void navigate(1));
dom.ruler.addEventListener("click", scrollToRulerPosition);
dom.viewport.addEventListener("scroll", scheduleRender);
window.addEventListener("resize", onResize);
window
  .matchMedia("(prefers-color-scheme: dark)")
  .addEventListener("change", () => {
    if (state.theme === "auto") {
      applyTheme();
    }
  });

bindActions({
  addComment,
  closeCommentList,
  closeEditor,
  collapseAll,
  confirmDeleteComment,
  copyPath,
  editComment,
  expandAll,
  expandSkipAt,
  extendSelection,
  goToComment,
  openCommentEditor,
  openEditorAt,
  openFileWideEditor,
  selectIndex,
  setCommentOpen,
  showCollapsed,
  startSelection,
  switchUnit,
  toggleHighlight,
  toggleOrigin,
  toggleOriginReason,
  toggleSeen,
});

applyTheme();
boot().catch((error) => {
  dom.notice.hidden = false;
  dom.notice.textContent = `読み込みに失敗しました: ${error}`;
});
