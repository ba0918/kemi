// @ts-check
// kemi のページ。仮想スクロールで表示中の行だけを DOM に載せる。

import * as api from "./api.js";
import {
  buildTree,
  collapseDefault,
  commentLabel,
  draftKey,
  filterAndSortFiles,
  formatBytes,
  isDarkTheme,
  keyAction,
  lineAnchor,
  lineHasAnchor,
  lineOffsets,
  metaItems,
  nextHighlightOverride,
  nextTheme,
  placeThreads,
  resolveTheme,
  statusLabel,
  suggestionAllowed,
  toDisplayLines,
  windowFor,
} from "./model.js";

/** @typedef {import("./model.js").FileEntry} FileEntry */
/** @typedef {import("./model.js").LogicalRow} LogicalRow */
/** @typedef {import("./model.js").DisplayLine} DisplayLine */

const ROW_HEIGHT = 24;
const OVERSCAN = 12;

const FILE_ICON =
  '<svg class="fi" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M4 1.5h5l3 3v10H4z"/><path d="M9 1.5v3h3"/></svg>';
const DIR_ICON =
  '<svg class="fi" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M1.5 3.5h4l1.5 2h7.5v7h-13z"/></svg>';
const COMMENT_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M2.5 3.5h11v7h-6l-3 3v-3h-2z"/></svg>';
const COPY_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><rect x="5.5" y="5.5" width="8" height="9" rx="1.5"/><path d="M10.5 5.5V3a1.5 1.5 0 0 0-1.5-1.5H4A1.5 1.5 0 0 0 2.5 3v7A1.5 1.5 0 0 0 4 11.5h1.5"/></svg>';
const EXPAND_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M3 5.5 8 10l5-4.5"/><path d="M3 2.5h10"/></svg>';
const CODE_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M4.5 4 2 8l2.5 4M11.5 4 14 8l-2.5 4M9.5 2.5l-3 11"/></svg>';
const EYE_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3"><path d="M1.5 8s2.4-4 6.5-4 6.5 4 6.5 4-2.4 4-6.5 4-6.5-4-6.5-4z"/><circle cx="8" cy="8" r="1.8"/></svg>';

/** @type {Record<string, string>} */
const THEME_LABELS = {
  auto: "自動",
  light: "light",
  dark: "dark",
  "solarized-light": "solarized light",
  "solarized-dark": "solarized dark",
};

/**
 * @param {string} selector
 * @returns {HTMLElement}
 */
function must(selector) {
  return /** @type {HTMLElement} */ (document.querySelector(selector));
}

const dom = {
  title: must("#review-title"),
  subtitle: must("#review-subtitle"),
  meta: must("#review-meta"),
  tree: must("#tree"),
  groupHeader: must("#group-header"),
  fileHeader: must("#file-header"),
  floating: must("#floating-threads"),
  notice: must("#notice"),
  viewport: must("#diff-viewport"),
  content: must("#diff-content"),
  footer: must("#approval-footer"),
  btnUnified: /** @type {HTMLButtonElement} */ (must("#btn-unified")),
  btnSplit: /** @type {HTMLButtonElement} */ (must("#btn-split")),
  btnWrap: /** @type {HTMLButtonElement} */ (must("#btn-wrap")),
  chipFocus: /** @type {HTMLButtonElement} */ (must("#chip-focus")),
  chipSort: /** @type {HTMLButtonElement} */ (must("#chip-sort")),
  btnTheme: /** @type {HTMLButtonElement} */ (must("#btn-theme")),
  updateBadge: /** @type {HTMLButtonElement} */ (must("#update-badge")),
  submitApproved: /** @type {HTMLButtonElement} */ (must("#btn-approve")),
  submitChanges: /** @type {HTMLButtonElement} */ (must("#btn-changes")),
  modal: must("#modal"),
  modalTitle: must("#modal-title"),
  modalBody: must("#modal-body"),
  modalOk: /** @type {HTMLButtonElement} */ (must("#modal-ok")),
  modalCancel: /** @type {HTMLButtonElement} */ (must("#modal-cancel")),
  overlay: must("#overlay"),
  overlayCard: must("#overlay-card"),
};

/**
 * @typedef {{ file: FileEntry, group: any }} Entry
 * @typedef {{ fileId: string, side: string, start: number, end: number, anchor: number }} Selection
 * @typedef {{ fileId: string, side: string, start: number, end: number, anchor: number, wide: boolean, body: string, suggestion: string, suggestionOn: boolean, needsFocus: boolean }} Editor
 */

/** @type {{
 *   review: any,
 *   entries: Entry[],
 *   visible: Entry[],
 *   current: Entry|null,
 *   index: number,
 *   mode: "unified" | "split",
 *   wrap: boolean,
 *   focusOnly: boolean,
 *   sortBySize: boolean,
 *   theme: string,
 *   cache: Map<string, any>,
 *   commentStore: Map<string, any[]>,
 *   rows: LogicalRow[],
 *   display: DisplayLine[],
 *   heights: number[],
 *   threads: { byLine: Map<number, any[]>, floating: any[] },
 *   binary: boolean,
 *   collapsedOverrides: Record<string, boolean>,
 *   rendering: boolean,
 *   highlightOverrides: Map<string, "on" | "off">,
 *   highlightCapable: boolean,
 *   highlightEnabled: boolean,
 *   dark: boolean,
 *   cacheKey: string,
 *   selectGeneration: number,
 *   comments: any[],
 *   selection: Selection | null,
 *   editor: Editor | null,
 *   dragging: { fileId: string, side: string } | null,
 *   submitted: boolean,
 *   updateAvailable: boolean,
 *   pendingVerdict: "approved" | "changes_requested" | null,
 *   treeItems: Map<string, HTMLButtonElement>,
 *   treeActiveId: string | null,
 *   treeVersion: number,
 *   treeRenderedVersion: number,
 *   groupOpen: Map<string, boolean>,
 *   dirOpen: Map<string, boolean>,
 * }} */
const state = {
  review: null,
  entries: [],
  visible: [],
  current: null,
  index: 0,
  mode: localStorage.getItem("kemi-mode") === "split" ? "split" : "unified",
  wrap: false,
  focusOnly: false,
  sortBySize: false,
  theme: localStorage.getItem("kemi-theme") || "auto",
  cache: new Map(),
  commentStore: new Map(),
  rows: [],
  display: [],
  heights: [],
  threads: { byLine: new Map(), floating: [] },
  binary: false,
  collapsedOverrides: {},
  rendering: false,
  highlightOverrides: new Map(),
  highlightCapable: false,
  highlightEnabled: false,
  dark: false,
  cacheKey: "",
  selectGeneration: 0,
  comments: [],
  selection: null,
  editor: null,
  dragging: null,
  submitted: false,
  updateAvailable: false,
  pendingVerdict: null,
  treeItems: new Map(),
  treeActiveId: null,
  treeVersion: 0,
  treeRenderedVersion: -1,
  groupOpen: new Map(),
  dirOpen: new Map(),
};

function currentEntry() {
  return state.current;
}

/**
 * 表示中のファイルが指定のファイルと一致するか。
 *
 * 応答待ちの間に別ファイルへ切り替えて戻っても表示を更新できるよう、
 * 一致は選択の世代ではなくファイル id で判定する。
 * @param {string} fileId
 * @returns {boolean}
 */
function isShowingFile(fileId) {
  const entry = currentEntry();
  return entry !== null && entry.file.id === fileId;
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
 * @param {string} markup
 * @returns {SVGElement}
 */
function svgIcon(markup) {
  const template = document.createElement("template");
  template.innerHTML = markup;
  return /** @type {SVGElement} */ (template.content.firstElementChild);
}

/**
 * @param {string} title
 * @param {string} markup
 * @returns {HTMLButtonElement}
 */
function iconButton(title, markup) {
  const element = button("iconbtn");
  element.title = title;
  element.setAttribute("aria-label", title);
  element.append(svgIcon(markup));
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

/**
 * @param {string} groupId
 * @returns {{ files: number, add: number, del: number }}
 */
function groupStatsFor(groupId) {
  let files = 0;
  let add = 0;
  let del = 0;
  for (const entry of state.entries) {
    if (entry.group.id === groupId) {
      files += 1;
      add += entry.file.add;
      del += entry.file.del;
    }
  }
  return { files, add, del };
}

/**
 * @param {FileEntry} file
 * @returns {HTMLElement}
 */
function fileStatsEl(file) {
  const stats = el("span", "st");
  if (file.binary) {
    stats.append(
      document.createTextNode(
        `${formatBytes(file.old_size)} → ${formatBytes(file.new_size)}`,
      ),
    );
  } else {
    stats.append(
      textEl("span", "p", `+${file.add}`),
      textEl("span", "m", `−${file.del}`),
    );
  }
  return stats;
}

function renderUpdateBadge() {
  dom.updateBadge.hidden = !state.updateAvailable;
}

async function refresh() {
  state.updateAvailable = false;
  renderUpdateBadge();
  const scrollTop = dom.viewport.scrollTop;
  // 取得中の展開応答が、新しいレビューで消えたキャッシュへ古い行を戻さないようにする。
  state.selectGeneration += 1;
  state.cache.clear();
  state.commentStore.clear();
  state.review = await api.getReview(true);
  state.entries = flatten(state.review);
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

function renderHeader() {
  const review = state.review;
  dom.title.textContent = review ? review.title : "kemi";
  const subtitle = review ? review.subtitle : "";
  dom.subtitle.textContent = subtitle || "";
  dom.subtitle.hidden = !subtitle;
  dom.meta.textContent = "";
  if (review) {
    for (const item of metaItems(review)) {
      const span = el("span");
      if (item.label) {
        span.append(textEl("b", "", item.label));
      }
      span.append(document.createTextNode(item.value));
      dom.meta.append(span);
    }
  }
  dom.btnUnified.setAttribute("aria-pressed", String(state.mode === "unified"));
  dom.btnSplit.setAttribute("aria-pressed", String(state.mode === "split"));
  dom.btnWrap.setAttribute("aria-pressed", String(state.wrap));
  dom.chipFocus.setAttribute("aria-pressed", String(state.focusOnly));
  dom.chipSort.setAttribute("aria-pressed", String(state.sortBySize));
}

function renderTree() {
  if (state.treeVersion !== state.treeRenderedVersion) {
    rebuildTree();
    state.treeRenderedVersion = state.treeVersion;
  }
  updateTreeActive();
}

function rebuildTree() {
  const groups = buildTree(state.visible);
  /** 前回のボタンを使い回す。同じファイルの項目は作り直さない。 */
  const items = new Map(state.treeItems);
  dom.tree.textContent = "";
  const fragment = document.createDocumentFragment();
  for (const { group, nodes } of groups) {
    const open = state.groupOpen.get(group.id) !== false;
    const groupEl = el("div", "group");
    groupEl.dataset.group = group.id;
    groupEl.dataset.open = open ? "true" : "false";
    const head = button("group-head");
    head.title = group.title || group.id;
    head.setAttribute("aria-expanded", String(open));
    const caret = textEl("span", "caret", open ? "▾" : "▸");
    const stats = groupStatsFor(group.id);
    head.append(
      caret,
      textEl("span", "gtitle", group.title || group.id),
      textEl(
        "span",
        "st",
        `${stats.files} files  +${stats.add} −${stats.del}`,
      ),
    );
    head.addEventListener("click", () => {
      const nextOpen = groupEl.dataset.open === "false";
      groupEl.dataset.open = nextOpen ? "true" : "false";
      state.groupOpen.set(group.id, nextOpen);
      head.setAttribute("aria-expanded", String(nextOpen));
      caret.textContent = nextOpen ? "▾" : "▸";
    });
    groupEl.append(head);
    const body = el("div", "group-body");
    if (group.why) {
      body.append(textEl("p", "why", group.why));
    }
    if (group.watch) {
      const watch = el("div", "watch");
      watch.append(
        textEl("b", "", "見てほしい点"),
        document.createTextNode(group.watch),
      );
      body.append(watch);
    }
    const list = el("ul", "files");
    appendNodes(list, nodes, group, items);
    body.append(list);
    groupEl.append(body);
    fragment.append(groupEl);
  }
  dom.tree.append(fragment);
  state.treeItems = items;
  state.treeActiveId = null;
}

/**
 * @param {HTMLElement} parent
 * @param {import("./model.js").TreeNode[]} nodes
 * @param {any} group
 * @param {Map<string, HTMLButtonElement>} items
 */
function appendNodes(parent, nodes, group, items) {
  for (const node of nodes) {
    if (node.type === "dir") {
      const li = el("li", "dir");
      const key = `${group.id}:${node.path}`;
      const open = state.dirOpen.get(key) !== false;
      li.dataset.open = open ? "true" : "false";
      const head = button("dir-head");
      head.setAttribute("aria-expanded", String(open));
      const caret = textEl("span", "caret", open ? "▾" : "▸");
      head.append(caret, svgIcon(DIR_ICON), document.createTextNode(node.name));
      head.addEventListener("click", () => {
        const nextOpen = li.dataset.open === "false";
        li.dataset.open = nextOpen ? "true" : "false";
        state.dirOpen.set(key, nextOpen);
        head.setAttribute("aria-expanded", String(nextOpen));
        caret.textContent = nextOpen ? "▾" : "▸";
      });
      const ul = el("ul");
      appendNodes(ul, node.children, group, items);
      li.append(head, ul);
      parent.append(li);
    } else {
      const li = el("li");
      li.append(treeItem({ file: node.file, group }, node.name, items));
      parent.append(li);
    }
  }
}

/**
 * @param {Entry} entry
 * @param {string} label
 * @param {Map<string, HTMLButtonElement>} items
 * @returns {HTMLButtonElement}
 */
function treeItem(entry, label, items) {
  let item = items.get(entry.file.id);
  if (!item) {
    item = button("file");
    item.dataset.fileId = entry.file.id;
    item.addEventListener("click", () => {
      const index = state.visible.findIndex(
        (candidate) => candidate.file.id === item?.dataset.fileId,
      );
      void selectIndex(index, { scrollTop: true });
    });
    items.set(entry.file.id, item);
  }
  item.textContent = "";
  item.title = entry.file.path;
  item.classList.toggle("seen", entry.file.seen);
  item.append(
    svgIcon(FILE_ICON),
    textEl("span", "fname", label),
    textEl("span", "badge status", statusLabel(entry.file.status)),
    fileStatsEl(entry.file),
  );
  if (entry.file.focus) {
    item.append(textEl("span", "badge-focus", "重要"));
  }
  if (entry.file.noise) {
    item.append(textEl("span", "badge-noise", "ノイズ"));
  }
  return item;
}

function updateTreeActive() {
  const entry = currentEntry();
  const nextId = entry ? entry.file.id : null;
  if (nextId === state.treeActiveId) {
    return;
  }
  if (state.treeActiveId) {
    state.treeItems.get(state.treeActiveId)?.classList.remove("active");
  }
  if (nextId) {
    state.treeItems.get(nextId)?.classList.add("active");
  }
  state.treeActiveId = nextId;
}

function renderGroupHeader() {
  const entry = currentEntry();
  dom.groupHeader.textContent = "";
  if (!entry) {
    return;
  }
  const stats = groupStatsFor(entry.group.id);
  const line = el("div", "gh-line");
  line.append(
    textEl("span", "gh-title", entry.group.title || entry.group.id),
    textEl("span", "gh-st", `+${stats.add} −${stats.del} / ${stats.files} files`),
  );
  dom.groupHeader.append(line);
  if (entry.group.why) {
    dom.groupHeader.append(textEl("p", "gh-why", entry.group.why));
  }
  if (entry.group.watch) {
    const watch = el("div", "gh-watch");
    watch.append(
      textEl("b", "", "見てほしい点"),
      document.createTextNode(entry.group.watch),
    );
    dom.groupHeader.append(watch);
  }
}

function highlightTitle() {
  if (state.highlightEnabled) {
    return "ハイライトを切る";
  }
  if (state.highlightCapable) {
    return "ハイライトを有効にする";
  }
  return "このファイルでハイライトを有効にする";
}

function renderFileHeader() {
  dom.fileHeader.textContent = "";
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  dom.fileHeader.append(textEl("span", "path", entry.file.path));
  if (entry.file.old_path) {
    dom.fileHeader.append(
      textEl("span", "file-old", `← ${entry.file.old_path}`),
    );
  }
  dom.fileHeader.append(
    textEl("span", "badge status", statusLabel(entry.file.status)),
  );
  dom.fileHeader.append(fileStatsEl(entry.file));
  if (entry.file.focus) {
    dom.fileHeader.append(textEl("span", "badge focus", "重要"));
  }
  if (entry.file.note) {
    dom.fileHeader.append(textEl("span", "note", entry.file.note));
  }
  dom.fileHeader.append(el("span", "spacer"));

  const comment = iconButton("ファイル全体にコメント", COMMENT_ICON);
  comment.disabled = state.submitted;
  comment.addEventListener("click", openFileWideEditor);
  dom.fileHeader.append(comment);

  const copy = iconButton("パスをコピー", COPY_ICON);
  copy.addEventListener("click", () => void copyPath(entry.file.path, copy));
  dom.fileHeader.append(copy);

  const fullyExpanded = allLinesExpanded();
  const expand = iconButton(
    fullyExpanded ? "すべて折りたたむ" : "すべての行を展開",
    EXPAND_ICON,
  );
  expand.disabled = state.submitted || state.binary;
  expand.addEventListener("click", () => {
    if (fullyExpanded) {
      collapseAll();
    } else {
      void expandAll();
    }
  });
  dom.fileHeader.append(expand);

  const highlight = iconButton(highlightTitle(), CODE_ICON);
  highlight.classList.toggle("active", state.highlightEnabled);
  highlight.setAttribute("aria-pressed", String(state.highlightEnabled));
  highlight.addEventListener("click", () => {
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
  });
  dom.fileHeader.append(highlight);

  const seen = iconButton(entry.file.seen ? "見た（取り消す）" : "見た", EYE_ICON);
  seen.setAttribute("aria-pressed", String(entry.file.seen));
  seen.addEventListener("click", () => void toggleSeen(entry.file));
  dom.fileHeader.append(seen);
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

function renderNotice() {
  dom.notice.hidden = true;
  dom.notice.textContent = "";
  const entry = currentEntry();
  if (!entry) {
    if (state.review) {
      dom.notice.hidden = false;
      dom.notice.textContent = "表示するファイルがありません";
    }
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
    const open = button("btn");
    open.textContent = "表示する";
    open.addEventListener("click", () => {
      state.collapsedOverrides[entry.file.id] = false;
      void api
        .postState({ file_id: entry.file.id, collapsed: false })
        .catch(() => undefined);
      renderNotice();
      renderDiff();
    });
    dom.notice.append(open);
  }
}

function renderFloating() {
  dom.floating.textContent = "";
  const floating = state.threads.floating;
  const wideEditor = state.editor && state.editor.wide ? state.editor : null;
  if (floating.length === 0 && !wideEditor) {
    dom.floating.hidden = true;
    return;
  }
  dom.floating.hidden = false;
  if (wideEditor) {
    dom.floating.append(renderEditor(wideEditor));
  }
  for (const comment of floating) {
    const wrap = el("div", "floating-thread");
    wrap.append(textEl("div", "floating-where", commentLabel(comment)));
    wrap.append(renderThread(comment));
    dom.floating.append(wrap);
  }
}

/**
 * @param {any} comment
 * @returns {HTMLElement}
 */
function renderThread(comment) {
  const thread = el("div", "thread");
  if (comment.outdated) {
    thread.classList.add("outdated");
  }
  const head = el("div", "t-head");
  head.append(textEl("span", "t-role", "あなた"));
  if (comment.suggestion) {
    head.append(textEl("span", "badge-focus", "提案"));
  }
  if (comment.resolved) {
    head.append(textEl("span", "badge resolved", "解決済み"));
  }
  thread.append(head);

  thread.append(textEl("div", "t-body", comment.body));

  if (comment.suggestion) {
    const box = el("div", "t-suggestion");
    box.append(textEl("div", "sug-head", "提案された変更"));
    const pre = el("pre");
    pre.textContent =
      comment.suggestion.replacement === ""
        ? "（行の削除）"
        : comment.suggestion.replacement;
    box.append(pre);
    thread.append(box);
    thread.append(
      textEl(
        "div",
        "t-note",
        "この提案はコメントと一緒に JSON でエージェントへ渡る（適用はエージェント）。",
      ),
    );
  }

  if (comment.outdated) {
    thread.append(
      textEl(
        "div",
        "t-outdated",
        "古いコメント — この後にファイルが変更されています（行番号は作成時のまま）",
      ),
    );
  }

  return thread;
}

/**
 * @param {Editor} editor
 * @returns {HTMLFormElement}
 */
function renderEditor(editor) {
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
    saveDraft(key, body.value);
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

  const actions = el("div", "row");
  const cancel = button("btn");
  cancel.textContent = "やめる";
  cancel.addEventListener("click", closeEditor);
  const submit = /** @type {HTMLButtonElement} */ (el("button", "btn primary"));
  submit.type = "submit";
  submit.textContent = "コメント";
  actions.append(cancel, submit);
  form.append(actions);

  form.addEventListener("submit", (event) => {
    event.preventDefault();
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
    void addComment(payload);
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

function closeEditor() {
  state.editor = null;
  state.selection = null;
  resetHeights();
  renderDiff();
  renderFloating();
}

/** エディタやスレッドの出入りで測り直す前に、行の高さを基準値へ戻す。 */
function resetHeights() {
  state.heights = new Array(state.display.length).fill(ROW_HEIGHT);
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
  renderDiff();
  renderFloating();
}

function renderDiff() {
  const entry = currentEntry();
  const focus = captureEditorFocus();
  dom.content.textContent = "";
  if (
    !entry ||
    state.binary ||
    collapseDefault(entry.file, state.collapsedOverrides)
  ) {
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
    const block = renderBlock(state.display[index], index);
    block.style.top = `${offsets[index]}px`;
    fragment.append(block);
  }
  dom.content.append(fragment);
  restoreEditorFocus(focus);
  const needsMeasure =
    state.wrap ||
    state.threads.byLine.size > 0 ||
    Boolean(state.editor && !state.editor.wide);
  if (needsMeasure) {
    measureHeights(window.start);
  }
}

/**
 * 描き直しでエディタを作り直す前に、フォーカス中の欄と選択位置を覚える。
 * @returns {{ field: string, start: number, end: number } | null}
 */
function captureEditorFocus() {
  const active = document.activeElement;
  if (!(active instanceof HTMLTextAreaElement)) {
    return null;
  }
  const field = active.dataset.editorField;
  if (!field || !dom.content.contains(active)) {
    return null;
  }
  return { field, start: active.selectionStart, end: active.selectionEnd };
}

/**
 * @param {{ field: string, start: number, end: number } | null} focus
 */
function restoreEditorFocus(focus) {
  if (!focus) {
    return;
  }
  const field = dom.content.querySelector(
    `[data-editor-field="${focus.field}"]`,
  );
  if (!(field instanceof HTMLTextAreaElement)) {
    return;
  }
  field.focus({ preventScroll: true });
  field.setSelectionRange(focus.start, focus.end);
}

/**
 * @param {DisplayLine} line
 * @param {number} index
 * @returns {HTMLDivElement}
 */
function renderBlock(line, index) {
  const block = /** @type {HTMLDivElement} */ (el("div", "row-block"));
  block.dataset.kemiRow = "1";
  block.append(renderLine(line));
  const threads = state.threads.byLine.get(index);
  if (threads) {
    for (const comment of threads) {
      const row = el("div", "thread-row");
      row.append(renderThread(comment));
      block.append(row);
    }
  }
  const editorState = state.editor;
  const entry = currentEntry();
  if (
    editorState &&
    !editorState.wide &&
    editorState.fileId === (entry ? entry.file.id : "") &&
    lineHasAnchor(line, editorState.side, editorState.anchor)
  ) {
    block.append(renderEditor(editorState));
  }
  return block;
}

/**
 * @param {DisplayLine} line
 * @returns {HTMLElement}
 */
function renderLine(line) {
  const row = el("div", `row kind-${line.kind}`);
  if (line.kind === "skip") {
    const skip = line.skip;
    const expand = button("expand-button");
    expand.textContent = `… ${skip && skip.count ? skip.count : 0} 行を表示`;
    expand.addEventListener("click", () => {
      void expandSkipAt(line.logicalIndex).then(() => renderFileHeader());
    });
    row.append(expand);
    return row;
  }
  const anchor = lineAnchor(line);
  const canComment = Boolean(anchor) && !state.submitted;
  const plusSide = anchor ? anchor.side : null;
  if (state.mode === "split") {
    row.classList.add("split");
    row.append(
      sideCell(
        "old",
        line.oldLine,
        line.oldSegments,
        canComment && plusSide === "old",
      ),
      sideCell(
        "new",
        line.newLine,
        line.newSegments,
        canComment && plusSide === "new",
      ),
    );
    return row;
  }
  row.append(
    numberCell("old", line.oldLine, canComment && plusSide === "old"),
    numberCell("new", line.newLine, canComment && plusSide === "new"),
    textEl("span", "mk", signFor(line.kind)),
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
      return "−";
    default:
      return "";
  }
}

/**
 * @param {"old" | "new"} side
 * @param {import("./model.js").Line|null} line
 * @param {import("./model.js").Segment[]} segments
 * @param {boolean} withPlus
 * @returns {HTMLElement}
 */
function sideCell(side, line, segments, withPlus) {
  const cell = el("span", "cell");
  const code = el("span", "code");
  fillCode(code, line, segments);
  cell.append(numberCell(side, line, withPlus), code);
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
 * @param {boolean} withPlus
 * @returns {HTMLElement}
 */
function numberCell(side, line, withPlus) {
  const cell = el("span", "no-cell");
  const number = textEl("span", "num", line ? String(line.number) : "");
  if (line && !state.submitted) {
    const value = Number(line.number);
    number.addEventListener("mousedown", (event) => {
      event.preventDefault();
      startSelection(side, value);
    });
    number.addEventListener("mouseenter", () => extendSelection(side, value));
    if (selectionContains(side, value)) {
      number.classList.add("selected");
    }
  }
  cell.append(number);
  if (line && withPlus) {
    const plus = button("line-add-btn");
    plus.textContent = "+";
    plus.title = "この行にコメント";
    plus.setAttribute("aria-label", "この行にコメント");
    plus.addEventListener("click", (event) => {
      event.stopPropagation();
      openEditorAt(side, Number(line.number));
    });
    cell.append(plus);
  }
  return cell;
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

/**
 * @param {string} key
 * @returns {string}
 */
function loadDraft(key) {
  return localStorage.getItem(key) || "";
}

/**
 * 下書きは入力のたびに残す（CONTEXT.md の下書き）。
 * @param {string} key
 * @param {string} value
 */
function saveDraft(key, value) {
  if (value === "") {
    localStorage.removeItem(key);
  } else {
    localStorage.setItem(key, value);
  }
}

/**
 * @param {any} payload
 */
async function addComment(payload) {
  try {
    const comment = await api.postComment(payload);
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
    localStorage.removeItem(draftKey(payload.file_id, selection));
  } catch (error) {
    showOverlay("コメントを追加できません", String(error));
  }
}

/**
 * @param {"approved" | "changes_requested"} verdict
 */
function openConfirm(verdict) {
  state.pendingVerdict = verdict;
  const approve = verdict === "approved";
  dom.modalTitle.textContent = approve
    ? "承認しますか？"
    : "変更要求として送信しますか？";
  dom.modalBody.textContent = approve
    ? "レビューを終了して、承認の verdict とコメントを実行ターミナルへ JSON で返します。この操作は取り消せません。"
    : "レビューを終了して、変更要求の verdict とコメントを実行ターミナルへ JSON で返します。この操作は取り消せません。";
  dom.modalOk.textContent = approve ? "承認して終了" : "変更要求で終了";
  dom.modalOk.className = approve ? "btn primary" : "btn danger";
  dom.modal.hidden = false;
}

function closeModal() {
  dom.modal.hidden = true;
  state.pendingVerdict = null;
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
    state.editor = null;
    renderDiff();
    renderFloating();
    renderFileHeader();
    showOverlay(
      verdict === "approved" ? "承認しました" : "変更要求を送りました",
      "kemi はコメントの JSON を出力して終了しました。",
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
 * 1 つの折りたたみを、サーバの残りが尽きるまで展開する。
 * @param {number} index
 * @returns {Promise<boolean>} 展開できたら true。
 */
async function expandSkipAt(index) {
  const entry = currentEntry();
  const skip = state.rows[index];
  if (
    !entry ||
    !skip ||
    skip.kind !== "skip" ||
    skip.from === undefined ||
    skip.to === undefined
  ) {
    return false;
  }
  const generation = state.selectGeneration;
  const cacheKey = state.cacheKey;
  /** @type {LogicalRow[]} */
  const replacement = [];
  let from = Number(skip.from);
  const to = Number(skip.to);
  while (from < to) {
    const data = await api.getFile(
      entry.file.id,
      { from, to },
      {
        dark: state.dark,
        highlight: state.highlightOverrides.get(entry.file.id),
      },
    );
    if (generation !== state.selectGeneration || cacheKey !== state.cacheKey) {
      // 取得中にファイルや表示条件が変わった。古い応答で表示とキャッシュを上書きしない。
      return false;
    }
    replacement.push(...(data.rows || []));
    if (data.next === null || data.next === undefined) {
      break;
    }
    from = Number(data.next);
  }
  if (state.rows[index] !== skip) {
    // 同じ折りたたみが先に展開された。取得前の位置へ挿すと表示行が重複する。
    return false;
  }
  const rows = state.rows.slice();
  rows.splice(index, 1, ...replacement);
  state.rows = rows;
  state.cache.set(cacheKey, { ...state.cache.get(cacheKey), rows });
  recomputeDisplay();
  renderDiff();
  return true;
}

async function expandAll() {
  const entry = currentEntry();
  if (!entry) {
    return;
  }
  if (collapseDefault(entry.file, state.collapsedOverrides)) {
    state.collapsedOverrides[entry.file.id] = false;
    void api
      .postState({ file_id: entry.file.id, collapsed: false })
      .catch(() => undefined);
    renderNotice();
  }
  const generation = state.selectGeneration;
  const cacheKey = state.cacheKey;
  let index = 0;
  while (index < state.rows.length) {
    if (state.rows[index].kind !== "skip") {
      index += 1;
      continue;
    }
    const expanded = await expandSkipAt(index);
    if (!expanded) {
      break;
    }
    index += 1;
  }
  if (generation !== state.selectGeneration || cacheKey !== state.cacheKey) {
    // 取得中に別のファイルへ切り替わった。新しい選択が描画する。
    return;
  }
  renderFileHeader();
  renderDiff();
}

function allLinesExpanded() {
  const data = state.cache.get(state.cacheKey);
  if (!data || !data.collapsedRows) {
    return false;
  }
  return (
    state.rows !== data.collapsedRows &&
    !state.rows.some((row) => row.kind === "skip")
  );
}

function collapseAll() {
  const data = state.cache.get(state.cacheKey);
  if (!data || !data.collapsedRows || state.rows === data.collapsedRows) {
    return;
  }
  state.rows = data.collapsedRows;
  state.cache.set(state.cacheKey, { ...data, rows: data.collapsedRows });
  recomputeDisplay();
  renderFileHeader();
  renderDiff();
}

function recomputeThreads() {
  state.threads = placeThreads(state.display, state.comments);
}

function recomputeDisplay() {
  state.display = toDisplayLines(state.rows, state.mode);
  state.heights = new Array(state.display.length).fill(ROW_HEIGHT);
  recomputeThreads();
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
  state.current = entry;
  const id = entry.file.id;
  const override = state.highlightOverrides.get(id);
  const key = `${id}|${state.dark ? 1 : 0}|${override ?? "auto"}`;
  state.cacheKey = key;
  const generation = ++state.selectGeneration;
  if (!state.cache.has(key)) {
    const data = await api.getFile(id, null, {
      dark: state.dark,
      highlight: override,
    });
    if (generation !== state.selectGeneration) {
      // 取得中に別のファイルが選ばれた。古い応答で表示を上書きしない。
      return;
    }
    const rows = data.rows || [];
    state.cache.set(key, { ...data, rows, collapsedRows: rows });
  }
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
  recomputeDisplay();
  renderTree();
  renderGroupHeader();
  renderFileHeader();
  renderNotice();
  renderFloating();
  renderDiff();
}

/**
 * @param {FileEntry} file
 */
async function toggleSeen(file) {
  const next = !file.seen;
  file.seen = next;
  state.treeItems.get(file.id)?.classList.toggle("seen", next);
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
  document.documentElement.dataset.theme = resolved;
  const current =
    THEME_LABELS[state.theme] || THEME_LABELS.auto;
  const next = THEME_LABELS[nextTheme(state.theme)] || "";
  dom.btnTheme.title = `テーマ: ${current}（クリックで ${next}）`;
  if (changed) {
    state.cache.clear();
    const entry = currentEntry();
    if (entry) {
      // テーマ切替は表示色の再取得だけ。入力中のエディタは閉じない。
      void selectEntry(entry, { scrollTop: false, keepEditor: true });
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
  renderHeader();
  renderDiff();
}

function renderFooter() {
  dom.footer.textContent = "";
  const approval = state.review ? state.review.approval || [] : [];
  dom.footer.hidden = approval.length === 0;
  if (approval.length === 0) {
    return;
  }
  dom.footer.append(textEl("span", "footer-label", "承認対象"));
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
  if (event.key === "Escape") {
    if (!dom.modal.hidden) {
      closeModal();
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
    state.heights = new Array(state.display.length).fill(ROW_HEIGHT);
    renderHeader();
    renderDiff();
  }
}

async function boot() {
  state.review = await api.getReview(false);
  state.entries = flatten(state.review);
  state.visible = state.entries.slice();
  state.treeVersion += 1;
  renderHeader();
  renderTree();
  renderFooter();
  if (state.visible.length > 0) {
    await selectIndex(0, { scrollTop: true });
  } else {
    renderNotice();
  }
  api.subscribeEvents(() => {
    state.updateAvailable = true;
    renderUpdateBadge();
  });
}

document.addEventListener("keydown", handleKey);
document.addEventListener("mouseup", () => {
  state.dragging = null;
});
dom.btnUnified.addEventListener("click", () => setMode("unified"));
dom.btnSplit.addEventListener("click", () => setMode("split"));
dom.btnWrap.addEventListener("click", () => {
  state.wrap = !state.wrap;
  state.heights = new Array(state.display.length).fill(ROW_HEIGHT);
  renderHeader();
  renderDiff();
});
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
  localStorage.setItem("kemi-theme", state.theme);
  applyTheme();
});
dom.updateBadge.addEventListener("click", () => void refresh());
dom.submitApproved.addEventListener("click", () => openConfirm("approved"));
dom.submitChanges.addEventListener("click", () => openConfirm("changes_requested"));
dom.modalCancel.addEventListener("click", closeModal);
dom.modalOk.addEventListener("click", () => {
  const verdict = state.pendingVerdict;
  closeModal();
  if (verdict) {
    void submitReview(verdict);
  }
});
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
