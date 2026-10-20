// @ts-check
// 行の選択、コメントの入力欄の開閉、コメントの追加・編集・削除。

import * as api from "../api.js";
import { actions } from '../actions.js';
import { currentEntry, isShowingFile, selectionText, state } from "../state.js";
import { clearDraft, loadDraft } from "../storage.js";
import { commentLabel, draftKey, hasLines, shownLineNumbers, tapSelection } from "../model.js";
import { firstLine, replaceComment } from "../conversation-model.js";
import {
  recomputeThreads,
  remeasure,
  remeasureAndRender,
  renderDiff,
  renderFloating,
} from "./display.js";
import { refreshRendered } from "./rendered.js";
import { renderConversation } from "../views/conversation.js";
import { renderFileHeader } from "../views/file-header.js";
import { renderHeader } from "../views/header.js";
import { openModal, showOverlay } from "../views/overlay.js";
import { refreshCommentBadges } from "../views/tree.js";

/**
 * 行番号の上で押し下げて選んでいる間の、そのファイルと側。押していなければ null。
 * @type {{ fileId: string, side: string } | null}
 */
let dragging = null;

/**
 * @param {"old" | "new"} side
 * @param {number} number
 */
export function startSelection(side, number) {
  if (state.submitted || state.narrow) {
    // 狭い画面では押し下げとホバーの選択（ドラッグ）は働かず、タップで選ぶ（R-NARROW）。
    return;
  }
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  dragging = { fileId: entry.file.id, side };
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
  const drag = dragging;
  const entry = currentEntry();
  if (!drag || !entry || drag.fileId !== entry.file.id || drag.side !== side || state.narrow) {
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
  dragging = null;
}

/**
 * 狭い画面の行番号のタップ（R-NARROW）。次の選択は純粋関数が決め、既存の選択の入れ物に
 * 入れる。広い画面では何もしない（そこでは押し下げとホバーが選ぶ）。
 * @param {"old" | "new"} side
 * @param {number} number
 */
export function tapLine(side, number) {
  const entry = currentEntry();
  if (!state.narrow || state.submitted || !entry) {
    return;
  }
  const current =
    state.selection && state.selection.fileId === entry.file.id ? state.selection : null;
  const next = tapSelection(current, { side, number }, shownLineNumbers(state.display, side));
  state.selection = next ? { fileId: entry.file.id, ...next } : null;
  renderDiff();
}

/**
 * 狭い画面で、行番号以外を押すと選択を解除する（R-NARROW）。`+`、入力欄、札と
 * 描画表示は除く。入力欄が開いている間は、その範囲を示す選択を残す。
 * @param {MouseEvent} event
 */
export function clearTapSelection(event) {
  const target = /** @type {HTMLElement} */ (event.target);
  if (
    !state.narrow ||
    !state.selection ||
    state.editor ||
    target.closest(".num, .line-add-btn, .editor, .cchip, #rendered-doc")
  ) {
    return;
  }
  state.selection = null;
  renderDiff();
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
  const wide = !hasLines(comment);
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
  refreshRendered();
}

export function closeEditor() {
  state.editor = null;
  state.selection = null;
  remeasureAndRender();
  refreshRendered();
}

/**
 * ページのコメントを差し替えて描き直す。行の高さは測り直すが、見ている位置は動かさない
 * （見ている位置より上で伸びた分は測り直しが打ち消す）。
 * @param {(comments: any[]) => any[]} change
 */
export function updateComments(change) {
  const before = state.allComments;
  state.allComments = change(state.allComments);
  actions.explanationChanged();
  recomputeThreads();
  remeasure();
  refreshCommentBadges(before);
  renderHeader();
  renderFileHeader();
  renderDiff();
  renderFloating();
  refreshRendered();
  renderConversation();
}

/**
 * @param {any} payload
 */
export async function addComment(payload) {
  try {
    const comment = await api.postComment(payload);
    const before = state.allComments;
    state.allComments = [...state.allComments, comment];
    refreshCommentBadges(before);
    renderHeader();
    renderConversation();
    // 応答までに別のファイルへ切り替わっていても、コメントは送信先のファイルに付く（グループと
    // パスで引く）。差分の描き直しは、送信先を表示中のときだけ。
    if (isShowingFile(payload.file_id)) {
      state.editor = null;
      state.selection = null;
      recomputeThreads();
      renderDiff();
      renderFloating();
      renderFileHeader();
      refreshRendered();
    }
    const selection = hasLines(payload)
      ? {
          side: payload.side,
          start: payload.start_line,
          end: payload.end_line,
        }
      : null;
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
    updateComments((comments) => replaceComment(comments, updated));
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
    if (state.conversation.thread === comment.id) {
      state.conversation.thread = null;
    }
    updateComments((comments) => comments.filter((item) => item.id !== comment.id));
  } catch (error) {
    showOverlay("could not delete the comment", String(error));
  }
}
