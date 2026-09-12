// @ts-check
// kemi のページ。仮想スクロールで表示中の行だけを DOM に載せる。

import * as api from "./api.js";
import { bindActions } from "./actions.js";
import {
  collapseAll,
  expandAll,
  expandSkipAt,
  onResize,
  renderDiff,
  renderFloating,
  scheduleRender,
  scrollToRulerPosition,
  setMode,
  setWrap,
  showCollapsed,
  toggleOriginReason,
} from "./features/display.js";
import {
  navigate,
} from "./features/navigation.js";
import {
  applyReview,
  copyPath,
  refresh,
  selectIndex,
  toggleFocusOnly,
  toggleHighlight,
  toggleOrigin,
  toggleSeen,
  toggleSortBySize,
} from "./features/files.js";
import { applyTheme, onSystemThemeChange, stepTheme } from "./features/theme.js";
import {
  addComment,
  closeEditor,
  confirmDeleteComment,
  editComment,
  endSelection,
  extendSelection,
  openCommentEditor,
  openEditorAt,
  openFileWideEditor,
  setCommentOpen,
  startSelection,
} from "./features/comments.js";
import { onUnitEvent, switchUnit } from "./features/units.js";
import {
  closeCommentList,
  closeCommentListOnOutsideClick,
  goToComment,
  toggleCommentList,
} from "./features/comment-list.js";
import {
  renderFileHeader,
  renderNotice,
} from "./views/file-header.js";
import {
  renderTree,
} from "./views/tree.js";
import {
  renderFooter,
  renderHeader,
  renderSubmitButtons,
  renderUpdateBadge,
} from "./views/header.js";
import {
  closeModal,
  runModalAction,
  showCompletion,
  showOverlay,
} from "./views/overlay.js";
import {
  dom,
  el,
  textEl,
} from "./dom.js";
import {
  UNIT_LABELS,
  currentEntry,
  state,
} from "./state.js";
import {
  submitSummary,
  keyAction,
} from "./model.js";

/** @typedef {import("./model.js").FileEntry} FileEntry */
/** @typedef {import("./model.js").LogicalRow} LogicalRow */
/** @typedef {import("./model.js").DisplayLine} DisplayLine */
/** @typedef {import("./state.js").Entry} Entry */
/** @typedef {import("./state.js").Editor} Editor */

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
document.addEventListener("mouseup", endSelection);
dom.btnUnified.addEventListener("click", () => setMode("unified"));
dom.btnSplit.addEventListener("click", () => setMode("split"));
dom.btnWrap.addEventListener("click", () => setWrap(!state.wrap));
dom.chipFocus.addEventListener("click", toggleFocusOnly);
dom.chipSort.addEventListener("click", toggleSortBySize);
dom.btnTheme.addEventListener("click", stepTheme);
dom.updateBadge.addEventListener("click", () => void refresh());
dom.submitApproved.addEventListener("click", () => openConfirm("approved"));
dom.submitChanges.addEventListener("click", () => openConfirm("changes_requested"));
dom.modalCancel.addEventListener("click", closeModal);
dom.modalOk.addEventListener("click", runModalAction);
dom.btnComments.addEventListener("click", toggleCommentList);
document.addEventListener("click", closeCommentListOnOutsideClick);
dom.navPrev.addEventListener("click", () => void navigate(-1));
dom.navNext.addEventListener("click", () => void navigate(1));
dom.ruler.addEventListener("click", scrollToRulerPosition);
dom.viewport.addEventListener("scroll", scheduleRender);
window.addEventListener("resize", onResize);
window
  .matchMedia("(prefers-color-scheme: dark)")
  .addEventListener("change", onSystemThemeChange);

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
