// @ts-check
// kemi のページ。仮想スクロールで表示中の行だけを DOM に載せる。

import * as api from "./api.js";
import {
  collapseDefault,
  commentLabel,
  filterAndSortFiles,
  formatBytes,
  keyAction,
  lineOffsets,
  statusLabel,
  suggestionAllowed,
  toDisplayLines,
  windowFor,
} from "./model.js";

/** @typedef {import("./model.js").FileEntry} FileEntry */
/** @typedef {import("./model.js").LogicalRow} LogicalRow */
/** @typedef {import("./model.js").DisplayLine} DisplayLine */

const ROW_HEIGHT = 22;
const OVERSCAN = 12;

/**
 * @param {string} selector
 * @returns {HTMLElement}
 */
function must(selector) {
  return /** @type {HTMLElement} */ (document.querySelector(selector));
}

const dom = {
  title: must("#review-title"),
  stats: must("#review-stats"),
  tree: must("#tree"),
  groupHeader: must("#group-header"),
  fileHeader: must("#file-header"),
  notice: must("#notice"),
  viewport: must("#diff-viewport"),
  content: must("#diff-content"),
  footer: must("#approval-footer"),
  modeUnified: must("#mode-unified"),
  modeSplit: must("#mode-split"),
  wrap: must("#toggle-wrap"),
  focus: must("#toggle-focus"),
  sort: must("#toggle-sort"),
  theme: /** @type {HTMLSelectElement} */ (must("#theme-select")),
  selectionBar: must("#selection-bar"),
  commentsList: must("#comments-list"),
  updateBadge: must("#update-badge"),
  submitApproved: must("#submit-approved"),
  submitChanges: must("#submit-changes"),
  overlay: must("#overlay"),
  overlayCard: must("#overlay-card"),
};

/**
 * @typedef {{
 *   file: FileEntry,
 *   group: { id: string, title: string, why: string, watch: string },
 * }} Entry
 */

/** @type {{
 *   review: any,
 *   entries: Entry[],
 *   visible: Entry[],
 *   index: number,
 *   mode: "unified" | "split",
 *   wrap: boolean,
 *   focusOnly: boolean,
 *   sortBySize: boolean,
 *   theme: string,
 *   cache: Map<string, any>,
 *   rows: LogicalRow[],
 *   display: DisplayLine[],
 *   heights: number[],
 *   binary: boolean,
 *   collapsedOverrides: Record<string, boolean>,
 *   rendering: boolean,
 *   highlightMode: string,
 *   highlightCapable: boolean,
 *   highlightEnabled: boolean,
 *   dark: boolean,
 *   cacheKey: string,
 *   comments: any[],
 *   selection: null | { fileId: string, side: string, start: number, end: number, anchor: number },
 *   submitted: boolean,
 *   updateAvailable: boolean,
 * }} */
const state = {
  review: null,
  entries: [],
  visible: [],
  index: 0,
  mode: localStorage.getItem("kemi-mode") === "split" ? "split" : "unified",
  wrap: false,
  focusOnly: false,
  sortBySize: false,
  theme: localStorage.getItem("kemi-theme") || "auto",
  cache: new Map(),
  rows: [],
  display: [],
  heights: [],
  binary: false,
  collapsedOverrides: {},
  rendering: false,
  highlightMode: localStorage.getItem("kemi-highlight") || "auto",
  highlightCapable: false,
  highlightEnabled: false,
  dark: false,
  cacheKey: "",
  comments: /** @type {any[]} */ ([]),
  selection: /** @type {null | { fileId: string, side: string, start: number, end: number, anchor: number }} */ (null),
  submitted: false,
  updateAvailable: false,
};

function currentEntry() {
  return state.visible[state.index];
}

/**
 * @param {string} tag
 * @param {string} [className]
 * @returns {HTMLElement}
 */
function el(tag, className) {
  const element = document.createElement(tag);
  if (className) {
    element.className = className;
  }
  return element;
}

/**
 * @param {string} className
 * @returns {HTMLButtonElement}
 */
function button(className) {
  const element = /** @type {HTMLButtonElement} */ (el("button", className));
  element.type = "button";
  return element;
}

/**
 * @param {string} tag
 * @param {string} className
 * @param {string} text
 * @returns {HTMLElement}
 */
function textEl(tag, className, text) {
  const element = el(tag, className);
  element.textContent = text;
  return element;
}

/**
 * @param {HTMLElement} parent
 * @param {import("./model.js").Segment[]|null|undefined} segments
 * @param {string|null|undefined} fallback
 */
function appendSegments(parent, segments, fallback) {
  if (!segments || segments.length === 0) {
    parent.append(document.createTextNode(fallback ?? ""));
    return;
  }
  for (const segment of segments) {
    if (segment.changed) {
      const mark = el("span", "changed");
      mark.textContent = segment.text;
      parent.append(mark);
    } else {
      parent.append(document.createTextNode(segment.text));
    }
  }
}

/**
 * @param {any} review
 * @returns {Entry[]}
 */
function flatten(review) {
  /** @type {Entry[]} */
  const entries = [];
  for (const group of review.groups) {
    for (const file of group.files) {
      entries.push({ file, group });
    }
  }
  return entries;
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
  state.visible = /** @type {Entry[]} */ (
    files.map((file) => byId.get(file.id))
  );
  if (keepId !== undefined) {
    const found = state.visible.findIndex((entry) => entry.file.id === keepId);
    if (found >= 0) {
      state.index = found;
    }
  }
  state.index = Math.max(0, Math.min(state.visible.length - 1, state.index));
}

function renderUpdateBadge() {
  dom.updateBadge.hidden = !state.updateAvailable;
}

async function refresh() {
  state.updateAvailable = false;
  renderUpdateBadge();
  const scrollTop = dom.viewport.scrollTop;
  state.cache.clear();
  state.review = await api.getReview(false);
  state.entries = flatten(state.review);
  const keepId = currentEntry() ? currentEntry().file.id : undefined;
  rebuildVisible(keepId);
  renderTopbar();
  renderTree();
  renderFooter();
  if (state.visible.length > 0) {
    await selectIndex(state.index, { scrollTop: false });
    dom.viewport.scrollTop = scrollTop;
    scheduleRender();
  }
}

function renderTopbar() {
  dom.title.textContent = state.review ? state.review.title : "kemi";
  if (state.review) {
    const files = state.review.groups.reduce(
      /** @param {number} sum @param {any} group */
      (sum, group) => sum + group.files.length,
      0,
    );
    dom.stats.textContent = `${files} ファイル`;
  }
  dom.modeUnified.classList.toggle("active", state.mode === "unified");
  dom.modeSplit.classList.toggle("active", state.mode === "split");
  dom.wrap.classList.toggle("active", state.wrap);
  dom.focus.classList.toggle("active", state.focusOnly);
  dom.sort.classList.toggle("active", state.sortBySize);
}

function renderTree() {
  dom.tree.textContent = "";
  const fragment = document.createDocumentFragment();
  let lastGroup = null;
  for (const entry of state.visible) {
    if (entry.group.id !== lastGroup) {
      const heading = el("div", "group-heading");
      heading.append(
        textEl("div", "group-title", entry.group.title || entry.group.id),
      );
      if (entry.group.watch) {
        heading.append(textEl("div", "group-watch", `watch: ${entry.group.watch}`));
      }
      fragment.append(heading);
      lastGroup = entry.group.id;
    }
    const item = button("tree-item");
    item.dataset.fileId = entry.file.id;
    if (currentEntry() && currentEntry().file.id === entry.file.id) {
      item.classList.add("active");
    }
    if (entry.file.seen) {
      item.classList.add("seen");
    }
    const line1 = el("span", "tree-path");
    line1.textContent = entry.file.path;
    const line2 = el("span", "tree-meta");
    line2.append(textEl("span", "badge status", statusLabel(entry.file.status)));
    if (entry.file.focus) {
      line2.append(textEl("span", "badge focus", "重点"));
    }
    if (entry.file.noise) {
      line2.append(textEl("span", "badge noise", "ノイズ"));
    }
    if (!entry.file.binary) {
      line2.append(
        textEl("span", "count add", `+${entry.file.add}`),
        textEl("span", "count del", `-${entry.file.del}`),
      );
    } else {
      line2.append(
        textEl(
          "span",
          "count bytes",
          `${formatBytes(entry.file.old_size)} → ${formatBytes(entry.file.new_size)}`,
        ),
      );
    }
    item.append(line1, line2);
    item.addEventListener("click", () => {
      const index = state.visible.findIndex(
        (candidate) => candidate.file.id === entry.file.id,
      );
      void selectIndex(index, { scrollTop: true });
    });
    fragment.append(item);
  }
  dom.tree.append(fragment);
}

function renderGroupAndFile() {
  const entry = currentEntry();
  dom.groupHeader.textContent = "";
  dom.fileHeader.textContent = "";
  if (!entry) {
    return;
  }
  dom.groupHeader.append(
    textEl("span", "group-label", entry.group.title || entry.group.id),
  );
  if (entry.group.watch) {
    dom.groupHeader.append(textEl("span", "group-watch", entry.group.watch));
  }

  dom.fileHeader.append(textEl("span", "file-path", entry.file.path));
  if (entry.file.old_path) {
    dom.fileHeader.append(textEl("span", "file-old", `← ${entry.file.old_path}`));
  }
  dom.fileHeader.append(
    textEl("span", "badge status", statusLabel(entry.file.status)),
  );
  if (!entry.file.binary) {
    dom.fileHeader.append(
      textEl("span", "count add", `+${entry.file.add}`),
      textEl("span", "count del", `-${entry.file.del}`),
    );
  }
  if (entry.file.focus) {
    dom.fileHeader.append(textEl("span", "badge focus", "重点"));
  }
  if (entry.file.note) {
    dom.fileHeader.append(textEl("span", "file-note", entry.file.note));
  }
  const seen = button("toggle seen-button");
  seen.textContent = entry.file.seen ? "見た ✓" : "見た";
  seen.classList.toggle("active", entry.file.seen);
  seen.addEventListener("click", () => void toggleSeen(entry.file));
  dom.fileHeader.append(seen);

  const highlightButton = button("toggle highlight-button");
  if (state.highlightEnabled) {
    highlightButton.textContent = "ハイライト off";
  } else if (state.highlightCapable) {
    highlightButton.textContent = "ハイライト on";
  } else {
    highlightButton.textContent = "このファイルで有効化";
  }
  highlightButton.classList.toggle("active", state.highlightEnabled);
  highlightButton.addEventListener("click", () => {
    if (state.highlightEnabled) {
      state.highlightMode = "off";
    } else if (state.highlightCapable) {
      state.highlightMode = "auto";
    } else {
      state.highlightMode = "on";
    }
    localStorage.setItem("kemi-highlight", state.highlightMode);
    void selectIndex(state.index, { scrollTop: false });
  });
  dom.fileHeader.append(highlightButton);
}

function renderNotice() {
  dom.notice.hidden = true;
  dom.notice.textContent = "";
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  if (state.binary) {
    dom.notice.hidden = false;
    dom.notice.append(
      textEl(
        "span",
        "notice-text",
        `バイナリ: ${formatBytes(entry.file.old_size)} → ${formatBytes(entry.file.new_size)}`,
      ),
    );
    return;
  }
  if (collapseDefault(entry.file, state.collapsedOverrides)) {
    dom.notice.hidden = false;
    const label = entry.file.noise ? "ノイズ" : "折りたたみ";
    dom.notice.append(textEl("span", "notice-text", `${label}: 内容を畳んでいます`));
    const open = button("open-button");
    open.textContent = "表示する";
    open.addEventListener("click", () => {
      state.collapsedOverrides[entry.file.id] = false;
      void api
        .postState({ file_id: entry.file.id, collapsed: false })
        .catch(() => undefined);
      renderNotice();
      scheduleRender();
    });
    dom.notice.append(open);
  }
}

function renderDiff() {
  const entry = currentEntry();
  dom.content.textContent = "";
  if (!entry || state.binary) {
    dom.content.style.height = "0px";
    return;
  }
  if (collapseDefault(entry.file, state.collapsedOverrides)) {
    dom.content.style.height = "0px";
    return;
  }
  const offsets = lineOffsets(state.heights);
  const totalHeight = offsets[offsets.length - 1] || 0;
  dom.content.style.height = `${totalHeight}px`;
  const window = windowFor(
    offsets,
    dom.viewport.scrollTop,
    dom.viewport.clientHeight,
    OVERSCAN,
  );
  if (state.wrap) {
    dom.content.dataset.wrap = "on";
  } else {
    delete dom.content.dataset.wrap;
  }
  const fragment = document.createDocumentFragment();
  for (let index = window.start; index < window.end; index += 1) {
    const element = renderLine(state.display[index]);
    element.style.top = `${offsets[index]}px`;
    fragment.append(element);
  }
  dom.content.append(fragment);
  if (state.wrap) {
    measureHeights(window.start);
  }
}

/**
 * @param {DisplayLine} line
 * @returns {HTMLElement}
 */
function renderLine(line) {
  const row = el("div", `row kind-${line.kind}`);
  row.dataset.kemiRow = "1";
  if (line.kind === "skip") {
    const skip = line.skip;
    const expand = button("expand-button");
    expand.textContent = `… ${skip && skip.count ? skip.count : 0} 行を表示`;
    expand.addEventListener("click", () => void expandSkip(line));
    row.append(expand);
    return row;
  }
  if (state.mode === "split") {
    row.classList.add("split");
    row.append(
      sideCell("old", line.oldLine, line.oldSegments),
      sideCell("new", line.newLine, line.newSegments),
    );
    return row;
  }
  row.append(
    numberCell("old", line.oldLine),
    numberCell("new", line.newLine),
    textEl("span", "sign", signFor(line.kind)),
  );
  const code = el("span", "code");
  if (line.newLine) {
    fillCode(code, line.newLine, line.newSegments);
  } else if (line.oldLine) {
    fillCode(code, line.oldLine, line.oldSegments);
  }
  row.append(code);
  return row;
}

/**
 * @param {string} kind
 * @returns {string}
 */
function signFor(kind) {
  switch (kind) {
    case "insert":
    case "replace-new":
      return "+";
    case "delete":
    case "replace-old":
      return "-";
    default:
      return " ";
  }
}

/**
 * @param {"old" | "new"} side
 * @param {import("./model.js").Line|null} line
 * @param {import("./model.js").Segment[]} segments
 * @returns {HTMLElement}
 */
function sideCell(side, line, segments) {
  const cell = el("span", "cell");
  const code = el("span", "code");
  fillCode(code, line, segments);
  cell.append(numberCell(side, line), code);
  return cell;
}

/**
 * @param {HTMLElement} code
 * @param {import("./model.js").Line|null} line
 * @param {import("./model.js").Segment[]} segments
 */
function fillCode(code, line, segments) {
  if (line && line.html) {
    code.innerHTML = line.html;
    return;
  }
  appendSegments(code, segments, line ? line.text : "");
}

/**
 * @param {"old" | "new"} side
 * @param {import("./model.js").Line|null} line
 * @returns {HTMLElement}
 */
function numberCell(side, line) {
  const number = textEl("span", "num", line ? String(line.number) : "");
  if (line && !state.submitted) {
    number.classList.add("clickable");
    number.addEventListener("click", (event) =>
      handleLineClick(side, Number(line.number), event.shiftKey),
    );
  }
  if (line && selectionContains(side, Number(line.number))) {
    number.classList.add("selected");
  }
  return number;
}

/**
 * @param {"old" | "new"} side
 * @param {number} number
 * @returns {boolean}
 */
function selectionContains(side, number) {
  const selection = state.selection;
  return Boolean(
    selection &&
      selection.side === side &&
      number >= selection.start &&
      number <= selection.end,
  );
}

/**
 * @param {"old" | "new"} side
 * @param {number} number
 * @param {boolean} extend
 */
function handleLineClick(side, number, extend) {
  if (state.submitted) {
    return;
  }
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  const selection = state.selection;
  if (
    selection &&
    selection.side === side &&
    selection.fileId === entry.file.id &&
    !extend &&
    selection.start === number &&
    selection.end === number
  ) {
    state.selection = null;
  } else if (
    extend &&
    selection &&
    selection.side === side &&
    selection.fileId === entry.file.id
  ) {
    state.selection = {
      fileId: entry.file.id,
      side,
      anchor: selection.anchor,
      start: Math.min(selection.anchor, number),
      end: Math.max(selection.anchor, number),
    };
  } else {
    state.selection = {
      fileId: entry.file.id,
      side,
      anchor: number,
      start: number,
      end: number,
    };
  }
  renderSelectionBar();
  renderDiff();
}

function selectionText() {
  const selection = state.selection;
  if (!selection) {
    return "";
  }
  const texts = [];
  for (const row of state.rows) {
    const line = selection.side === "new" ? row.new : row.old;
    if (
      line &&
      Number(line.number) >= selection.start &&
      Number(line.number) <= selection.end
    ) {
      texts.push(line.text);
    }
  }
  return texts.join("\n");
}

function renderSelectionBar() {
  dom.selectionBar.textContent = "";
  const selection = state.selection;
  if (!selection || state.submitted) {
    dom.selectionBar.hidden = true;
    return;
  }
  dom.selectionBar.hidden = false;
  const range =
    selection.start === selection.end
      ? `${selection.start}`
      : `${selection.start}–${selection.end}`;
  const side = selection.side === "new" ? "新側" : "旧側";
  dom.selectionBar.append(
    textEl("span", "selection-label", `${side} ${range} にコメント`),
  );
  const add = button("add-comment-button");
  add.textContent = "コメントを追加";
  add.addEventListener("click", () => openCommentForm(selection));
  dom.selectionBar.append(add);
  const clear = button("cancel-button");
  clear.textContent = "選択解除";
  clear.addEventListener("click", () => {
    state.selection = null;
    renderSelectionBar();
    renderDiff();
  });
  dom.selectionBar.append(clear);
}

/**
 * @param {{ fileId: string, side: string, start: number, end: number }} selection
 */
function openCommentForm(selection) {
  dom.selectionBar.textContent = "";
  const form = el("form", "comment-form");
  const body = document.createElement("textarea");
  body.placeholder = "本文";
  body.rows = 3;
  form.append(body);

  let suggestion = null;
  if (suggestionAllowed(selection.side)) {
    const label = el("label", "suggestion-row");
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    const suggestionText = document.createElement("textarea");
    suggestionText.placeholder = "置換後の全文（空なら行の削除）";
    suggestionText.rows = 2;
    suggestionText.value = selectionText();
    label.append(checkbox, document.createTextNode(" suggestion"), suggestionText);
    form.append(label);
    suggestion = { checkbox, textarea: suggestionText };
  }

  const submitButton = /** @type {HTMLButtonElement} */ (el("button", "add-comment-button"));
  submitButton.textContent = "追加";
  const cancel = button("cancel-button");
  cancel.textContent = "キャンセル";
  cancel.addEventListener("click", () => {
    state.selection = null;
    renderSelectionBar();
    renderDiff();
  });
  form.append(submitButton, cancel);

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const payload = /** @type {any} */ ({
      op: "add",
      file_id: selection.fileId,
      side: selection.side,
      start_line: selection.start,
      end_line: selection.end,
      body: body.value,
    });
    if (suggestion && suggestion.checkbox.checked) {
      payload.suggestion = suggestion.textarea.value;
    }
    void addComment(payload);
  });
  dom.selectionBar.append(form);
  body.focus();
}

/**
 * @param {any} payload
 */
async function addComment(payload) {
  try {
    const comment = await api.postComment(payload);
    state.comments.push(comment);
    state.selection = null;
    renderSelectionBar();
    renderDiff();
    renderComments();
  } catch (error) {
    showOverlay("コメントを追加できません", String(error));
  }
}

function renderComments() {
  dom.commentsList.textContent = "";
  const fileWide = button("file-wide-button");
  fileWide.textContent = "ファイル全体にコメント";
  fileWide.disabled = state.submitted;
  fileWide.addEventListener("click", () => openFileWideForm());
  dom.commentsList.append(fileWide);
  for (const comment of state.comments) {
    dom.commentsList.append(commentThread(comment));
  }
}

function openFileWideForm() {
  if (state.submitted) {
    return;
  }
  const form = el("form", "comment-form");
  const body = document.createElement("textarea");
  body.placeholder = "ファイル全体への本文";
  body.rows = 3;
  const submitButton = /** @type {HTMLButtonElement} */ (el("button", "add-comment-button"));
  submitButton.textContent = "追加";
  form.append(body, submitButton);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const entry = currentEntry();
    if (!entry) {
      return;
    }
    void addComment({
      op: "add",
      file_id: entry.file.id,
      side: "new",
      body: body.value,
    });
  });
  dom.commentsList.prepend(form);
  body.focus();
}

/**
 * @param {any} comment
 * @returns {HTMLElement}
 */
function commentThread(comment) {
  const thread = el("div", "thread");
  const head = el("div", "thread-head");
  head.append(textEl("span", "thread-where", commentLabel(comment)));
  if (comment.outdated) {
    head.append(textEl("span", "badge outdated", "古い"));
  }
  if (comment.resolved) {
    head.append(textEl("span", "badge resolved", "解決済み"));
  }
  thread.append(head);

  if (comment.quote && comment.quote.length > 0) {
    const quote = el("pre", "thread-quote");
    quote.textContent = comment.quote.join("\n");
    thread.append(quote);
  }
  const body = el("div", "thread-body");
  body.textContent = comment.body;
  thread.append(body);

  if (comment.suggestion) {
    const suggestion = el("pre", "thread-suggestion");
    suggestion.textContent =
      comment.suggestion.replacement === ""
        ? "（行の削除）"
        : comment.suggestion.replacement;
    thread.append(suggestion);
  }
  for (const reply of comment.replies || []) {
    thread.append(textEl("div", "thread-reply", reply));
  }

  if (!state.submitted) {
    const replyForm = el("form", "reply-form");
    const input = document.createElement("input");
    input.placeholder = "返信";
    const send = /** @type {HTMLButtonElement} */ (el("button", "reply-button"));
    send.textContent = "返信";
    replyForm.append(input, send);
    replyForm.addEventListener("submit", (event) => {
      event.preventDefault();
      void sendReply(comment.id, input.value);
    });
    thread.append(replyForm);

    const resolve = button("resolve-button");
    resolve.textContent = comment.resolved ? "解決を戻す" : "解決";
    resolve.addEventListener("click", () => void toggleResolve(comment));
    thread.append(resolve);
  }
  return thread;
}

/**
 * @param {string} id
 * @param {string} body
 */
async function sendReply(id, body) {
  if (body === "") {
    return;
  }
  try {
    const updated = await api.postComment({ op: "reply", id, body });
    replaceComment(updated);
    renderComments();
  } catch (error) {
    showOverlay("返信できません", String(error));
  }
}

/**
 * @param {any} comment
 */
async function toggleResolve(comment) {
  try {
    const updated = await api.postComment({
      op: "resolve",
      id: comment.id,
      resolved: !comment.resolved,
    });
    replaceComment(updated);
    renderComments();
  } catch (error) {
    showOverlay("解決状態を変えられません", String(error));
  }
}

/**
 * @param {any} updated
 */
function replaceComment(updated) {
  state.comments = state.comments.map((comment) =>
    comment.id === updated.id ? updated : comment,
  );
}

/**
 * @param {"approved" | "changes_requested"} verdict
 */
async function submitReview(verdict) {
  if (state.submitted) {
    return;
  }
  try {
    await api.submit(verdict);
    state.submitted = true;
    state.selection = null;
    renderSelectionBar();
    renderComments();
    showOverlay(
      verdict === "approved" ? "承認しました" : "変更要求を送りました",
      "kemi は注釈の JSON を出力して終了しました。",
    );
  } catch (error) {
    showOverlay("送信できませんでした", String(error));
  }
}

/**
 * @param {string} title
 * @param {string|null} detail
 */
function showOverlay(title, detail) {
  dom.overlay.hidden = false;
  dom.overlayCard.textContent = "";
  dom.overlayCard.append(textEl("h2", "overlay-title", title));
  if (detail) {
    dom.overlayCard.append(textEl("p", "overlay-detail", detail));
  }
  const close = button("overlay-close");
  close.textContent = "閉じる";
  close.addEventListener("click", () => {
    dom.overlay.hidden = true;
  });
  dom.overlayCard.append(close);
}

/**
 * @param {number} start
 */
function measureHeights(start) {
  const children = Array.from(dom.content.children);
  let changed = false;
  children.forEach((child, offset) => {
    const index = start + offset;
    const height = /** @type {HTMLElement} */ (child).offsetHeight;
    if (height > 0 && state.heights[index] !== height) {
      state.heights[index] = height;
      changed = true;
    }
  });
  if (changed && !state.rendering) {
    state.rendering = true;
    requestAnimationFrame(() => {
      state.rendering = false;
      renderDiff();
    });
  }
}

function scheduleRender() {
  requestAnimationFrame(() => renderDiff());
}

/**
 * @param {DisplayLine} line
 */
async function expandSkip(line) {
  const entry = currentEntry();
  const skip = line.skip;
  if (!entry || !skip || skip.from === undefined || skip.to === undefined) {
    return;
  }
  const data = await api.getFile(
    entry.file.id,
    { from: skip.from, to: skip.to },
    {
      dark: state.dark,
      highlight: state.highlightMode === "auto" ? undefined : state.highlightMode,
    },
  );
  const replacement = /** @type {LogicalRow[]} */ (data.rows);
  if (data.next !== null && data.next !== undefined) {
    replacement.push({
      kind: "skip",
      old: null,
      new: null,
      old_segments: [],
      new_segments: [],
      count: skip.to - Number(data.next),
      from: Number(data.next),
      to: skip.to,
    });
  }
  state.rows.splice(line.logicalIndex, 1, ...replacement);
  state.cache.set(state.cacheKey, { ...state.cache.get(state.cacheKey), rows: state.rows });
  recomputeDisplay();
  renderDiff();
}

function recomputeDisplay() {
  state.display = toDisplayLines(state.rows, state.mode);
  state.heights = new Array(state.display.length).fill(ROW_HEIGHT);
}

/**
 * @param {number} index
 * @param {{ scrollTop?: boolean }} [options]
 */
async function selectIndex(index, options = { scrollTop: true }) {
  if (state.visible.length === 0) {
    return;
  }
  state.index = Math.max(0, Math.min(state.visible.length - 1, index));
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  const id = entry.file.id;
  const key = `${id}|${state.dark ? 1 : 0}|${state.highlightMode}`;
  state.cacheKey = key;
  if (!state.cache.has(key)) {
    const data = await api.getFile(id, null, {
      dark: state.dark,
      highlight: state.highlightMode === "auto" ? undefined : state.highlightMode,
    });
    state.cache.set(key, { ...data, rows: data.rows || [] });
  }
  const data = state.cache.get(key);
  state.rows = data.rows || [];
  state.binary = Boolean(data.binary);
  state.highlightCapable = Boolean(data.highlight && data.highlight.capable);
  state.highlightEnabled = Boolean(data.highlight && data.highlight.enabled);
  state.comments = data.comments || [];
  state.selection = null;
  if (options.scrollTop) {
    dom.viewport.scrollTop = 0;
  }
  recomputeDisplay();
  renderTree();
  renderGroupAndFile();
  renderSelectionBar();
  renderNotice();
  renderComments();
  renderDiff();
}

/**
 * @param {FileEntry} file
 */
async function toggleSeen(file) {
  const next = !file.seen;
  file.seen = next;
  renderTree();
  renderGroupAndFile();
  try {
    await api.postState({ file_id: file.id, seen: next });
  } catch {
    // 表示は先に更新し、失敗は次回の取得で戻る
  }
}

function applyTheme() {
  const resolved =
    state.theme === "auto"
      ? window.matchMedia("(prefers-color-scheme: dark)").matches
        ? "dark"
        : "light"
      : state.theme;
  const dark = resolved === "dark" || resolved === "solarized-dark";
  const changed = state.dark !== dark;
  state.dark = dark;
  document.documentElement.dataset.theme = resolved;
  dom.theme.value = state.theme;
  if (changed) {
    state.cache.clear();
    if (currentEntry()) {
      void selectIndex(state.index, { scrollTop: false });
    }
  }
}

/**
 * @param {"unified" | "split"} mode
 */
function setMode(mode) {
  state.mode = mode;
  localStorage.setItem("kemi-mode", mode);
  recomputeDisplay();
  renderTopbar();
  renderDiff();
}

function renderFooter() {
  dom.footer.textContent = "";
  const approval = state.review ? state.review.approval || [] : [];
  if (approval.length === 0) {
    return;
  }
  const label = textEl("span", "footer-label", "承認対象");
  dom.footer.append(label);
  for (const item of approval) {
    const row = el("span", "approval-item");
    row.append(
      textEl("span", "approval-path", item.path),
      textEl("span", "approval-identity", item.identity),
    );
    dom.footer.append(row);
  }
}

/**
 * @param {KeyboardEvent} event
 */
function handleKey(event) {
  const target = /** @type {HTMLElement} */ (event.target);
  if (
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.isContentEditable
  ) {
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
  } else if (action.type === "wrap") {
    state.wrap = Boolean(action.value);
    dom.wrap.classList.toggle("active", state.wrap);
    scheduleRender();
  }
}

async function boot() {
  state.review = await api.getReview(false);
  state.entries = flatten(state.review);
  state.visible = state.entries.slice();
  renderTopbar();
  renderTree();
  renderFooter();
  if (state.visible.length > 0) {
    await selectIndex(0, { scrollTop: true });
  }
  api.subscribeEvents(() => {
    state.updateAvailable = true;
    renderUpdateBadge();
  });
}

document.addEventListener("keydown", handleKey);
dom.modeUnified.addEventListener("click", () => setMode("unified"));
dom.modeSplit.addEventListener("click", () => setMode("split"));
dom.wrap.addEventListener("click", () => {
  state.wrap = !state.wrap;
  dom.wrap.classList.toggle("active", state.wrap);
  scheduleRender();
});
dom.focus.addEventListener("click", () => {
  state.focusOnly = !state.focusOnly;
  const keepId = currentEntry() ? currentEntry().file.id : undefined;
  rebuildVisible(keepId);
  renderTopbar();
  renderTree();
  void selectIndex(state.index, { scrollTop: false });
});
dom.sort.addEventListener("click", () => {
  state.sortBySize = !state.sortBySize;
  const keepId = currentEntry() ? currentEntry().file.id : undefined;
  rebuildVisible(keepId);
  renderTopbar();
  renderTree();
  void selectIndex(state.index, { scrollTop: false });
});
dom.theme.addEventListener("change", () => {
  state.theme = dom.theme.value;
  localStorage.setItem("kemi-theme", state.theme);
  applyTheme();
});
dom.updateBadge.addEventListener("click", () => void refresh());
dom.submitApproved.addEventListener("click", () => void submitReview("approved"));
dom.submitChanges.addEventListener("click", () => void submitReview("changes_requested"));
dom.viewport.addEventListener("scroll", scheduleRender);
window.addEventListener("resize", scheduleRender);
window
  .matchMedia("(prefers-color-scheme: dark)")
  .addEventListener("change", () => {
    if (state.theme === "auto") {
      applyTheme();
    }
  });

applyTheme();
boot().catch((error) => {
  dom.notice.hidden = false;
  dom.notice.textContent = `読み込みに失敗しました: ${error}`;
});
