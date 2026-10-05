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
import { applyNarrow, closeDrawer, toggleComments, toggleDrawer } from "./features/narrow.js";
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
  startSelection,
  tapLine,
} from "./features/comments.js";
import { onUnitEvent, switchUnit } from "./features/units.js";
import {
  chooseFilter,
  closeConversation,
  closeThread,
  editFromThread,
  goToComment,
  keepReplyDraft,
  loadCommitGroups,
  markLoaded,
  onConversationScroll,
  openConversation,
  openThread,
  reloadThreadLines,
  setThreadFolded,
  showNewest,
  startResize,
  submitMessage,
  submitOnModEnter,
  submitReply,
  toggleConversation,
} from "./features/conversation.js";
import { openConfirm } from "./features/submit.js";
import {
  applyAgent,
  applyMessage,
  endComposition,
  handToAgent,
  receiveMissed,
  receiveThread,
  replyTo,
  setResolved,
  startComposition,
} from "./features/agent.js";
import { renderConversation } from "./views/conversation.js";
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
    // 狭い画面のシートは重ねた覆いなので閉じる。広い画面の会話パネルは列なので閉じない。
    if (state.narrow && state.conversation.sheetOpen) {
      closeConversation();
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
  // サーバはつながる前の通知を送り直さないので、先に通知につながってから中身を読む（読んだ後、
  // つながる前に起きたことを取りこぼさない）。読んだ中身を描き終えるまでに届いた通知は、
  // 描き終えてから届いた順に取り込む（先に取り込むと、読んだ中身で上書きされる）。
  /** @type {(() => void)[] | null} */
  let held = [];
  // 起動に失敗したページには通知を取り込まない（溜め続けもしない）。
  let failed = false;
  /**
   * @template {any[]} A
   * @param {(...args: A) => void} handler
   * @returns {(...args: A) => void}
   */
  const afterBoot = (handler) => (...args) => {
    if (failed) {
      return;
    }
    if (held) {
      held.push(() => handler(...args));
    } else {
      handler(...args);
    }
  };
  await api.subscribeEvents(
    afterBoot(() => {
      state.updateAvailable = true;
      renderUpdateBadge();
    }),
    afterBoot(() => void onUnitEvent()),
    {
      onThread: afterBoot(receiveThread),
      onMessage: afterBoot(applyMessage),
      onAgent: afterBoot(applyAgent),
      onMissed: afterBoot(receiveMissed),
    },
  );
  try {
    applyReview(await api.getReview(false), true);
    markLoaded();
    renderHeader();
    renderConversation({ toEnd: true });
    void loadCommitGroups();
    renderTree();
    if (state.visible.length > 0) {
      await selectIndex(0, { scrollTop: true });
    } else {
      renderNotice();
    }
  } catch (error) {
    failed = true;
    held = null;
    throw error;
  }
  const tasks = held;
  held = null;
  for (const task of tasks) {
    task();
  }
}

// 幅を見るのはここだけ。CSS の狭い画面のメディアクエリと同じ文字列（R-NARROW）。
const narrowQuery = window.matchMedia("(max-width: 719.98px)");
state.narrow = narrowQuery.matches;
narrowQuery.addEventListener("change", (event) => applyNarrow(event.matches));
document.addEventListener("keydown", handleKey);
document.addEventListener("compositionstart", startComposition);
document.addEventListener("compositionend", endComposition);
dom.btnTree.addEventListener("click", toggleDrawer);
dom.drawerScrim.addEventListener("click", closeDrawer);
dom.menuWrap.addEventListener("click", () => setWrap(!displayWrap()));
dom.menuFocus.addEventListener("click", toggleFocusOnly);
dom.menuSort.addEventListener("click", toggleSortBySize);
dom.menuTheme.addEventListener("click", stepTheme);
dom.menuComments.addEventListener("click", toggleComments);
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
dom.btnHand.addEventListener("click", () => void handToAgent());
dom.railHand.addEventListener("click", () => void handToAgent());
dom.btnComments.addEventListener("click", toggleConversation);
dom.cvRail.addEventListener("click", openConversation);
dom.cvClose.addEventListener("click", closeConversation);
dom.cvResizer.addEventListener("pointerdown", startResize);
dom.cvFilter.addEventListener("click", chooseFilter);
dom.cvCompose.addEventListener("submit", submitMessage);
dom.cvMessage.addEventListener("keydown", submitOnModEnter);
dom.cvReply.addEventListener("submit", submitReply);
dom.cvReplyText.addEventListener("keydown", submitOnModEnter);
dom.cvReplyText.addEventListener("input", keepReplyDraft);
dom.cvNewer.addEventListener("click", showNewest);
dom.cvItems.addEventListener("scroll", onConversationScroll);
dom.cvThreadBody.addEventListener("scroll", onConversationScroll);
dom.submitApproved.addEventListener("click", () => openConfirm("approved"));
dom.submitChanges.addEventListener("click", () => openConfirm("changes_requested"));
dom.modalCancel.addEventListener("click", closeModal);
dom.modalOk.addEventListener("click", runModalAction);
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
  closeThread,
  loadCommitGroups: () => void loadCommitGroups(),
  reloadThreadLines,
  editFromThread: (comment, unit) => void editFromThread(comment, unit),
  openThread,
  replyTo: (comment, body) => void replyTo(comment, body),
  setThreadFolded,
  setResolved: (comment, resolved) => void setResolved(comment, resolved),
  applyRenderedView,
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
