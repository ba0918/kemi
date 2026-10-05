// @ts-check
// コメントの札と、コメントを書く欄（新しく書く・編集する）。

import { actions } from "../actions.js";
import { button, el, textEl } from "../dom.js";
import { state } from "../state.js";
import { saveDraft } from "../storage.js";
import { commentLabel, draftKey, suggestionAllowed, threadChip } from "../model.js";

/** @typedef {import("../state.js").Editor} Editor */

/**
 * 編集中ならエディタ、そうでなければコメントの札。
 * @param {any} comment
 * @returns {HTMLElement}
 */
export function renderCommentOrEditor(comment) {
  const editor = state.editor;
  if (editor && editor.editId === comment.id) {
    return renderEditor(editor);
  }
  return renderChip(comment);
}

/**
 * コメントの 1 行の札（R-VIEW）。本文の 1 行目と返信の数を示し、押すと会話パネルでその
 * スレッドを開く。そのスレッドを最後に開いた後に届いたエージェントの返信があれば新着の印を
 * 付ける。差分の中で本文を広げる吹き出しは持たない。畳んだスレッド（解決したものを
 * 含む）の札は場所と 1 行目だけに縮める（R-AGENT-HAND）。作成者は出さない。
 * @param {any} comment
 * @returns {HTMLElement}
 */
function renderChip(comment) {
  const chip = threadChip(comment, state.conversation.read, state.conversation.folded);
  const classes = ["cchip"];
  if (comment.outdated) {
    classes.push("outdated");
  }
  if (chip.folded) {
    classes.push("folded");
  }
  const element = button(classes.join(" "));
  element.dataset.focusKey = `comment:${comment.id}`;
  element.dataset.id = comment.id;
  element.title = comment.body;
  element.append(
    document.createTextNode("💬"),
    textEl("span", "where", commentLabel(comment)),
    textEl("span", "tx", chip.first),
  );
  if (!chip.folded) {
    if (comment.suggestion) {
      element.append(textEl("span", "badge-suggest", "Suggestion"));
    }
    if (chip.replies > 0) {
      element.append(
        textEl("span", "reply-count", chip.replies === 1 ? "1 reply" : `${chip.replies} replies`),
      );
    }
  }
  if (chip.resolved) {
    element.append(textEl("span", "t-resolved-mark", "Resolved"));
  }
  if (chip.unread) {
    element.classList.add("unread");
    element.append(textEl("span", "unread-mark", "New"));
  }
  element.addEventListener("click", () => actions.openThread(comment.id));
  return element;
}

/**
 * @param {Editor} editor
 * @returns {HTMLFormElement}
 */
export function renderEditor(editor) {
  const form = /** @type {HTMLFormElement} */ (el("form", "editor"));
  const body = document.createElement("textarea");
  body.placeholder = editor.wide
    ? "Comment on the whole file"
    : "Comment on this line (Cmd/Ctrl+Enter to save)";
  body.rows = 3;
  body.dataset.editorField = "body";
  const key = draftKey(
    editor.fileId,
    editor.wide
      ? null
      : { side: editor.side, start: editor.start, end: editor.end },
  );
  body.value = editor.body;
  body.addEventListener("input", () => {
    editor.body = body.value;
    // 下書きは新しいコメントだけに残す。編集中の本文はコメント自身にある。
    if (!editor.editId) {
      saveDraft(key, body.value);
    }
  });
  body.addEventListener("keydown", (event) => {
    if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
      event.preventDefault();
      form.requestSubmit();
    }
  });
  form.append(body);

  /** @type {{ checkbox: HTMLInputElement, textarea: HTMLTextAreaElement } | null} */
  let suggestion = null;
  if (!editor.wide && suggestionAllowed(editor.side)) {
    const row = el("div", "suggestion-row");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = editor.suggestionOn;
    checkbox.addEventListener("change", () => {
      editor.suggestionOn = checkbox.checked;
    });
    const textarea = document.createElement("textarea");
    textarea.placeholder = "Full replacement text (empty deletes the line)";
    textarea.value = editor.suggestion;
    textarea.dataset.editorField = "suggestion";
    textarea.addEventListener("input", () => {
      editor.suggestion = textarea.value;
    });
    row.append(
      checkbox,
      document.createTextNode("Write the full replacement text as a suggestion"),
      textarea,
    );
    form.append(row);
    suggestion = { checkbox, textarea };
  }

  const buttons = el("div", "row");
  const cancel = button("btn");
  cancel.textContent = "Cancel";
  cancel.addEventListener("click", () => actions.closeEditor());
  const submit = /** @type {HTMLButtonElement} */ (el("button", "btn primary"));
  submit.type = "submit";
  submit.textContent = editor.editId ? "Save" : "Comment";
  buttons.append(cancel, submit);
  form.append(buttons);

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (editor.editId) {
      actions.editComment({
        op: "edit",
        id: editor.editId,
        body: body.value,
        suggestion: suggestion && suggestion.checkbox.checked ? suggestion.textarea.value : null,
      });
      return;
    }
    const payload = /** @type {any} */ ({
      op: "add",
      file_id: editor.fileId,
      side: editor.side,
      body: body.value,
    });
    if (editor.wide) {
      payload.start_line = null;
      payload.end_line = null;
    } else {
      payload.start_line = editor.start;
      payload.end_line = editor.end;
    }
    if (suggestion && suggestion.checkbox.checked) {
      payload.suggestion = suggestion.textarea.value;
    }
    actions.addComment(payload);
  });

  if (editor.needsFocus) {
    window.requestAnimationFrame(() => {
      // 先に描き直されていたら、その描画に focus を任せる。
      if (!body.isConnected) {
        return;
      }
      editor.needsFocus = false;
      body.focus();
    });
  }
  return form;
}
