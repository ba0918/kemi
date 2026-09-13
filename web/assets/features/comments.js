// @ts-check
// 行の選択、コメントの入力欄の開閉、コメントの追加・編集・削除、吹き出しの開閉。

import * as api from "../api.js";
import { dom } from "../dom.js";
import { currentEntry, isShowingFile, selectionText, state } from "../state.js";
import { clearDraft, loadDraft } from "../storage.js";
import { commentLabel, draftKey, firstLine } from "../model.js";
import {
  recomputeThreads,
  remeasure,
  remeasureAndRender,
  renderDiff,
  renderFloating,
} from "./display.js";
import { renderCommentList } from "../views/comment-list.js";
import { renderFileHeader } from "../views/file-header.js";
import { renderHeader } from "../views/header.js";
import { openModal, showOverlay } from "../views/overlay.js";
import { refreshCommentBadges } from "../views/tree.js";

/**
 * @param {"old" | "new"} side
 * @param {number} number
 */
export function startSelection(side, number) {
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
export function extendSelection(side, number) {
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

/** ドラッグの終了。行番号の上でボタンを離したかに関わらず、選択の伸ばしを止める。 */
export function endSelection() {
  state.dragging = null;
}

export function openFileWideEditor() {
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
export function openEditorAt(side, number) {
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
  remeasureAndRender();
}

/**
 * コメントの本文と suggestion を編集する。行レンジは変えない（R-COMMENT）。
 * @param {any} comment
 */
export function openCommentEditor(comment) {
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
  remeasureAndRender();
}

export function closeEditor() {
  state.editor = null;
  state.selection = null;
  remeasureAndRender();
}

/**
 * コメントの吹き出しの開閉。行の高さが変わるので、次の描画で測り直させる。
 * @param {string} id
 * @param {boolean} open
 */
export function setCommentOpen(id, open) {
  state.commentOpen.set(id, open);
  remeasureAndRender();
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
 * @param {any} payload
 */
export async function addComment(payload) {
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
    showOverlay("could not add the comment", String(error));
  }
}

/**
 * @param {any} payload
 */
export async function editComment(payload) {
  try {
    const updated = await api.postComment(payload);
    state.editor = null;
    updateComments((comments) =>
      comments.map((item) => (item.id === updated.id ? updated : item)),
    );
  } catch (error) {
    showOverlay("could not edit the comment", String(error));
  }
}

/**
 * 削除は確認を 1 回挟む（R-COMMENT）。
 * @param {any} comment
 */
export function confirmDeleteComment(comment) {
  openModal({
    title: "Delete this comment?",
    body: `${commentLabel(comment)}: ${firstLine(comment.body)}\nDeleted comments are not included in the submitted JSON.`,
    okLabel: "Delete",
    okClass: "btn secondary",
    cancelLabel: "Back",
    action: () => void deleteComment(comment),
  });
}

/**
 * @param {any} comment
 */
async function deleteComment(comment) {
  try {
    await api.postComment({ op: "delete", id: comment.id });
    updateComments((comments) => comments.filter((item) => item.id !== comment.id));
  } catch (error) {
    showOverlay("could not delete the comment", String(error));
  }
}
