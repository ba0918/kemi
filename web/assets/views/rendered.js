// @ts-check
// 描画表示（R-RENDER）: 描画した文書、ブロックにホバーかタップすると出る `+`、ブロックの
// 直下の吹き出しと入力欄、読み込めなかった画像の枠。

import { actions } from "../actions.js";
import { button, dom, el, focusKeyWithin, restoreFocusKey, textEl } from "../dom.js";
import { currentEntry, state } from "../state.js";
import { formatBytes } from "../model.js";
import { renderCommentOrEditor, renderEditor } from "./comment.js";

/** @typedef {import("../state.js").Editor} Editor */

/**
 * 文書のブロック要素（文書順）。`data-kemi-block` の並びはサーバの `blocks` と同じ順で、
 * 添字で対応づく。
 * @returns {HTMLElement[]}
 */
export function blockElements() {
  return Array.from(dom.renderedDoc.querySelectorAll("[data-kemi-block]"));
}

/**
 * ブロックの上端の、表示領域のスクロール位置で測った高さ。
 * @param {HTMLElement} element
 * @returns {number}
 */
export function blockTop(element) {
  const base = dom.viewport.getBoundingClientRect().top;
  return element.getBoundingClientRect().top - base + dom.viewport.scrollTop;
}

/**
 * 描画表示を描き直す。文書の HTML はサーバが作る（生 HTML はエスケープ済み）。
 */
export function renderRendered() {
  const focus = captureEditorFocus();
  const focusKey = focusKeyWithin(dom.renderedDoc);
  dom.renderedDoc.textContent = "";
  const rendered = state.rendered;
  const entry = currentEntry();
  dom.renderedDoc.hidden = !state.renderedActive;
  if (!state.renderedActive || !rendered || !entry) {
    return;
  }
  if (rendered.kind === "image" && rendered.image) {
    dom.renderedDoc.append(renderImages(rendered.image));
    return;
  }
  const body = el("div", "rendered-body");
  body.innerHTML = rendered.html;
  dom.renderedDoc.append(body);

  const blocks = blockElements();
  if (state.renderedThreads.top.length > 0) {
    const holder = el("div", "kb-notes kb-notes-top");
    for (const comment of state.renderedThreads.top) {
      holder.append(noteItem(renderCommentOrEditor(comment)));
    }
    body.prepend(holder);
  }
  blocks.forEach((element, index) => {
    const notes = state.renderedThreads.byBlock.get(index) || [];
    const editor = editorFor(index);
    if (notes.length === 0 && !editor) {
      return;
    }
    const holder = noteHolder(element);
    for (const comment of notes) {
      holder.append(noteItem(renderCommentOrEditor(comment)));
    }
    if (editor) {
      // 入力欄には、そのブロックの元の行を見せる（R-RENDER）。
      const lines = editor.side === "new" ? rendered.newLines : rendered.oldLines;
      const quote = el("pre", "kb-quote");
      quote.textContent = lines.slice(editor.start - 1, editor.end).join("\n");
      holder.append(quote, renderEditor(editor));
    }
  });

  // 取得が 404 になった画像（symlink・上限超え・存在しないファイル）は、`src` を出さない
  // ものと同じ枠に差し替える。
  for (const image of body.querySelectorAll("img[data-kemi-path]")) {
    image.addEventListener("error", () => replaceWithFrame(/** @type {HTMLImageElement} */ (image)), {
      once: true,
    });
  }

  attachPlus(body, blocks);
  restoreEditorFocus(focus);
  restoreFocusKey(dom.renderedDoc, focusKey);
}

/**
 * 旧と新の画像を並べる（R-RENDER）。2 列では左右、1 列では上下（並びは CSS が
 * `#rendered-doc` の `data-mode` で決める）。改名でバイト列が同じなら 1 枚と "unchanged"。
 * 読み込みに失敗した側はバイト数だけの枠に差し替える。
 * @param {{ old: import("../state.js").ImageSide | null, new: import("../state.js").ImageSide | null, same: boolean }} image
 * @returns {HTMLElement}
 */
function renderImages(image) {
  const panel = el("div", "kb-images");
  if (image.same && image.new) {
    panel.append(imageFigure(image.new, "unchanged", "same"));
    return panel;
  }
  if (image.old) {
    panel.append(imageFigure(image.old, "old", "old"));
  }
  if (image.new) {
    panel.append(imageFigure(image.new, "new", "new"));
  }
  return panel;
}

/**
 * @param {import("../state.js").ImageSide} side
 * @param {string} label
 * @param {string} kind
 * @returns {HTMLElement}
 */
function imageFigure(side, label, kind) {
  const figure = el("figure", `kb-image kb-image-${kind}`);
  const picture = /** @type {HTMLImageElement} */ (el("img"));
  picture.src = side.url;
  picture.alt = label;
  picture.addEventListener(
    "error",
    () => {
      const frame = el("div", "kb-img-frame kb-image-broken");
      frame.append(textEl("span", "kb-img-alt", "Cannot display this image"), textEl("span", "kb-img-path", formatBytes(side.size)));
      picture.replaceWith(frame);
    },
    { once: true },
  );
  const caption = el("figcaption");
  caption.append(textEl("span", "kb-image-label", label), textEl("span", "kb-image-size", formatBytes(side.size)));
  figure.append(picture, caption);
  return figure;
}

/**
 * ブロックの入力欄。ブロックの `+` から開いたものだけをそのブロックの直下に出す。
 * @param {number} index
 * @returns {Editor | null}
 */
function editorFor(index) {
  const editor = state.editor;
  const entry = currentEntry();
  const block = state.rendered ? state.rendered.blocks[index] : undefined;
  if (!editor || !entry || !block || editor.wide || editor.editId || editor.fileId !== entry.file.id) {
    return null;
  }
  if (editor.side !== block.side || editor.start !== block.start || editor.end !== block.end) {
    return null;
  }
  return editor;
}

/**
 * ブロックの直下に吹き出しを置く入れ物。表の行と、リストの項目は、その入れ物の中に
 * 収まる要素で包む。
 * @param {HTMLElement} element
 * @returns {HTMLElement}
 */
function noteHolder(element) {
  const holder = el("div", "kb-notes");
  if (element.tagName === "TR") {
    const row = el("tr", "kb-note-row");
    const cell = el("td");
    cell.setAttribute("colspan", "100");
    cell.append(holder);
    row.append(cell);
    element.after(row);
    return holder;
  }
  if (element.tagName === "LI") {
    const item = el("li", "kb-note-li");
    item.append(holder);
    element.after(item);
    return holder;
  }
  element.after(holder);
  return holder;
}

/**
 * @param {HTMLElement} content
 * @returns {HTMLElement}
 */
function noteItem(content) {
  const item = el("div", "kb-note");
  item.append(content);
  return item;
}

/**
 * @param {HTMLImageElement} image
 */
function replaceWithFrame(image) {
  const path = image.dataset.kemiPath || "";
  const frame = el("span", "kb-img-frame");
  frame.title = path;
  frame.append(textEl("span", "kb-img-alt", image.alt), textEl("span", "kb-img-path", path));
  image.replaceWith(frame);
}

/**
 * ホバーかタップしたブロックの左上に出す `+`。要素は 1 つで、ブロックを追いかける（`<hr>` や
 * `<tr>` には子要素を置けないため）。タップ（R-NARROW）はホバーの無い端末のための入口で、
 * 同じ要素を同じ位置に出す。
 * @param {HTMLElement} body
 * @param {HTMLElement[]} blocks
 */
function attachPlus(body, blocks) {
  if (state.submitted) {
    return;
  }
  const plus = button("kb-plus");
  plus.textContent = "+";
  plus.title = "Comment on this block";
  plus.setAttribute("aria-label", "Comment on this block");
  plus.hidden = true;
  let hovered = -1;
  dom.renderedDoc.append(plus);
  /** @param {Event} event */
  const showPlusAt = (event) => {
    const target = /** @type {HTMLElement} */ (event.target);
    const block = target.closest("[data-kemi-block]");
    if (!(block instanceof HTMLElement) || !body.contains(block)) {
      return;
    }
    hovered = blocks.indexOf(block);
    const base = dom.renderedDoc.getBoundingClientRect();
    const rect = block.getBoundingClientRect();
    plus.style.top = `${Math.round(rect.top - base.top)}px`;
    plus.style.left = `${Math.round(Math.max(0, rect.left - base.left - 24))}px`;
    plus.hidden = false;
  };
  body.addEventListener("mouseover", showPlusAt);
  body.addEventListener("click", showPlusAt);
  dom.renderedDoc.addEventListener("mouseleave", () => {
    plus.hidden = true;
  });
  plus.addEventListener("click", () => {
    if (hovered >= 0) {
      actions.openBlockEditor(hovered);
    }
  });
}

/**
 * 描き直しで入力欄を作り直す前に、フォーカス中の欄と選択位置を覚える。
 * @returns {{ field: string, start: number, end: number } | null}
 */
function captureEditorFocus() {
  const active = document.activeElement;
  if (!(active instanceof HTMLTextAreaElement) || !dom.renderedDoc.contains(active)) {
    return null;
  }
  const field = active.dataset.editorField;
  return field ? { field, start: active.selectionStart, end: active.selectionEnd } : null;
}

/**
 * @param {{ field: string, start: number, end: number } | null} focus
 */
function restoreEditorFocus(focus) {
  if (!focus) {
    return;
  }
  const field = dom.renderedDoc.querySelector(`[data-editor-field="${focus.field}"]`);
  if (field instanceof HTMLTextAreaElement) {
    field.focus({ preventScroll: true });
    field.setSelectionRange(focus.start, focus.end);
  }
}
