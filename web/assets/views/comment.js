// @ts-check
// コメントの札と吹き出し、コメントを書く欄。

import { actions } from "../actions.js";
import { button, el, textEl } from "../dom.js";
import { state } from "../state.js";
import { saveDraft } from "../storage.js";
import { commentLabel, draftKey, firstLine, suggestionAllowed } from "../model.js";

/** @typedef {import("../state.js").Editor} Editor */

/**
 * 編集中ならエディタ、そうでなければコメント（札か吹き出し）。
 * @param {any} comment
 * @returns {HTMLElement}
 */
export function renderCommentOrEditor(comment) {
  const editor = state.editor;
  if (editor && editor.editId === comment.id) {
    return renderEditor(editor);
  }
  return renderComment(comment);
}

/**
 * コメントを、既定では本文の 1 行目を見せる札に畳んで出し、押すと吹き出しで本文をすべて
 * 見せる。付けた直後のコメントは開いて出し、別のファイルへ移って戻っても開いたままに
 * する（addComment が開いた状態として覚える）。開閉はページを開いている間だけ覚える。
 * @param {any} comment
 * @returns {HTMLElement}
 */
function renderComment(comment) {
  const where = commentLabel(comment);
  const open = state.commentOpen.get(comment.id) === true;
  if (!open) {
    const chip = button(`cchip${comment.outdated ? " outdated" : ""}`);
    // 札と吹き出しの「畳む」は同じ鍵を持ち、開閉の後もフォーカスが行き来する。
    chip.dataset.focusKey = `comment:${comment.id}`;
    chip.title = comment.body;
    chip.append(
      document.createTextNode("💬"),
      textEl("span", "where", where),
      textEl("span", "tx", firstLine(comment.body)),
    );
    if (comment.suggestion) {
      chip.append(textEl("span", "badge-suggest", "提案"));
    }
    chip.addEventListener("click", () => actions.setCommentOpen(comment.id, true));
    return chip;
  }
  const balloon = el("div", `bal${comment.outdated ? " outdated" : ""}`);
  const head = el("div", "bh");
  head.append(textEl("span", "where", where));
  if (comment.suggestion) {
    head.append(textEl("span", "badge-suggest", "提案"));
  }
  if (comment.outdated) {
    head.append(textEl("span", "t-outdated-mark", "古いコメント"));
  }
  const acts = el("span", "acts");
  if (!state.submitted) {
    const edit = button("");
    edit.textContent = "編集";
    edit.addEventListener("click", () => actions.openCommentEditor(comment));
    const remove = button("");
    remove.textContent = "削除";
    remove.addEventListener("click", () => actions.confirmDeleteComment(comment));
    acts.append(edit, remove);
  }
  const fold = button("");
  fold.textContent = "畳む";
  fold.dataset.focusKey = `comment:${comment.id}`;
  fold.addEventListener("click", () => actions.setCommentOpen(comment.id, false));
  acts.append(fold);
  head.append(acts);
  balloon.append(head, textEl("p", "t-body", comment.body));

  if (comment.suggestion) {
    const box = el("div", "t-suggestion");
    box.append(textEl("div", "sug-head", "提案された変更"));
    const pre = el("pre");
    pre.textContent =
      comment.suggestion.replacement === ""
        ? "（行の削除）"
        : comment.suggestion.replacement;
    box.append(pre);
    balloon.append(box);
    balloon.append(
      textEl(
        "div",
        "t-note",
        "この提案はコメントと一緒に JSON でエージェントへ渡る（適用はエージェント）。",
      ),
    );
  }
  if (comment.outdated) {
    balloon.append(
      textEl(
        "div",
        "t-outdated",
        "古いコメント — この後にファイルが変更されています（行番号は作成時のまま）",
      ),
    );
  }
  return balloon;
}

/**
 * @param {Editor} editor
 * @returns {HTMLFormElement}
 */
export function renderEditor(editor) {
  const form = /** @type {HTMLFormElement} */ (el("form", "editor"));
  const body = document.createElement("textarea");
  body.placeholder = editor.wide
    ? "ファイル全体へのコメント"
    : "この行へのコメント（Cmd/Ctrl+Enter で記録）";
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
    textarea.placeholder = "置換後の全文（空なら行の削除）";
    textarea.value = editor.suggestion;
    textarea.dataset.editorField = "suggestion";
    textarea.addEventListener("input", () => {
      editor.suggestion = textarea.value;
    });
    row.append(
      checkbox,
      document.createTextNode("suggestion として置換後の全文を書く"),
      textarea,
    );
    form.append(row);
    suggestion = { checkbox, textarea };
  }

  const buttons = el("div", "row");
  const cancel = button("btn");
  cancel.textContent = "やめる";
  cancel.addEventListener("click", () => actions.closeEditor());
  const submit = /** @type {HTMLButtonElement} */ (el("button", "btn primary"));
  submit.type = "submit";
  submit.textContent = editor.editId ? "保存" : "コメント";
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
