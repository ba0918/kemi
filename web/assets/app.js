// @ts-check
// kemi のページ。仮想スクロールで表示中の行だけを DOM に載せる。

import { onHorizontalScroll, onCodeWheel, onHorizontalKey } from "./features/horizontal-scroll.js";
import * as api from "./api.js";
import { bindActions } from "./actions.js";
import {
  collapseAll,
  expandAll,
  expandSkipAt,
  onResize,
  scheduleRender,
  scrollToRulerPosition,
  setMode,
  setWrap,
  showCollapsed,
  toggleOriginReason,
} from "./features/display.js";
import { applyNarrow, closeDrawer, toggleDrawer } from "./features/narrow.js";
import { navigate } from "./features/navigation.js";
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
import {
  applyRenderedView,
  openBlockEditor,
  setRendered,
  toggleRendered,
} from "./features/rendered.js";
import { applyTheme, onSystemThemeChange, stepTheme } from "./features/theme.js";
import {
  addComment,
  clearTapSelection,
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
  tapLine,
} from "./features/comments.js";
import { onUnitEvent, switchUnit } from "./features/units.js";
import {
  closeCommentList,
  closeCommentListOnOutsideClick,
  goToComment,
  toggleCommentList,
} from "./features/comment-list.js";
import { openConfirm } from "./features/submit.js";
import { renderNotice } from "./views/file-header.js";
import { renderTree } from "./views/tree.js";
import { placeNotes, renderHeader, renderUpdateBadge } from "./views/header.js";
import { closeModal, runModalAction } from "./views/overlay.js";
import { dom } from "./dom.js";
import { currentEntry, displayMode, displayWrap, state } from "./state.js";
import { keyAction } from "./model.js";

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
    if (dom.viewMenu.matches(":popover-open") || dom.titleSheet.matches(":popover-open")) {
      // 狭い画面のメニューとシートはブラウザが閉じる。下の入力欄まで閉じない。
      return;
    }
    if (!dom.commentList.hidden) {
      closeCommentList();
      return;
    }
    if (state.drawerOpen) {
      closeDrawer();
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
    displayMode(),
    state.visible.length,
    state.index,
    displayWrap(),
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
  } else if (action.type === "rendered") {
    const entry = currentEntry();
    if (entry) {
      toggleRendered(entry);
    }
  }
}

async function boot() {
  applyReview(await api.getReview(false), true);
  renderHeader();
  renderTree();
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

// 幅を見るのはここだけ。CSS の狭い画面のメディアクエリと同じ文字列（R-NARROW）。
const narrowQuery = window.matchMedia("(max-width: 719.98px)");
state.narrow = narrowQuery.matches;
narrowQuery.addEventListener("change", (event) => applyNarrow(event.matches));
document.addEventListener("keydown", handleKey);
dom.btnTree.addEventListener("click", toggleDrawer);
dom.drawerScrim.addEventListener("click", closeDrawer);
dom.menuWrap.addEventListener("click", () => setWrap(!displayWrap()));
dom.menuFocus.addEventListener("click", toggleFocusOnly);
dom.menuSort.addEventListener("click", toggleSortBySize);
dom.menuTheme.addEventListener("click", stepTheme);
document.addEventListener("mouseup", endSelection);
dom.viewport.addEventListener("click", clearTapSelection);
dom.btnUnified.addEventListener("click", () => setMode("unified"));
dom.btnSplit.addEventListener("click", () => setMode("split"));
dom.btnWrap.addEventListener("click", () => setWrap(!displayWrap()));
dom.chipFocus.addEventListener("click", toggleFocusOnly);
dom.chipSort.addEventListener("click", toggleSortBySize);
dom.btnTheme.addEventListener("click", stepTheme);
dom.notes.addEventListener("beforetoggle", placeNotes);
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
dom.viewport.addEventListener("wheel", onCodeWheel, { passive: false });
dom.horizontal.addEventListener("scroll", onHorizontalScroll);
dom.horizontal.addEventListener("keydown", onHorizontalKey);
window.addEventListener("resize", onResize);
window
  .matchMedia("(prefers-color-scheme: dark)")
  .addEventListener("change", onSystemThemeChange);

bindActions({
  addComment,
  applyRenderedView,
  closeCommentList,
  closeDrawer,
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
  openBlockEditor,
  openFileWideEditor,
  selectIndex,
  setCommentOpen,
  setRendered,
  showCollapsed,
  startSelection,
  switchUnit,
  tapLine,
  toggleHighlight,
  toggleOrigin,
  toggleOriginReason,
  toggleSeen,
});

applyTheme();
boot().catch((error) => {
  dom.notice.hidden = false;
  dom.notice.textContent = `could not load: ${error}`;
});
