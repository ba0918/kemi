// @ts-check
// kemi のページ。仮想スクロールで表示中の行だけを DOM に載せる。

import * as api from "./api.js";
import {
  collapseDefault,
  filterAndSortFiles,
  formatBytes,
  keyAction,
  lineOffsets,
  statusLabel,
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
      sideCell(line.oldLine, line.oldSegments),
      sideCell(line.newLine, line.newSegments),
    );
    return row;
  }
  const oldNumber = line.oldLine ? String(line.oldLine.number) : "";
  const newNumber = line.newLine ? String(line.newLine.number) : "";
  row.append(
    textEl("span", "num", oldNumber),
    textEl("span", "num", newNumber),
    textEl("span", "sign", signFor(line.kind)),
  );
  const code = el("span", "code");
  if (line.newLine) {
    appendSegments(code, line.newSegments, line.newLine.text);
  } else if (line.oldLine) {
    appendSegments(code, line.oldSegments, line.oldLine.text);
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
 * @param {import("./model.js").Line|null} line
 * @param {import("./model.js").Segment[]} segments
 * @returns {HTMLElement}
 */
function sideCell(line, segments) {
  const cell = el("span", "cell");
  const number = el("span", "num");
  number.textContent = line ? String(line.number) : "";
  const code = el("span", "code");
  if (line) {
    appendSegments(code, segments, line.text);
  }
  cell.append(number, code);
  return cell;
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
  const data = await api.getFile(entry.file.id, {
    from: skip.from,
    to: skip.to,
  });
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
  state.cache.set(entry.file.id, { ...state.cache.get(entry.file.id), rows: state.rows });
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
  if (!state.cache.has(id)) {
    const data = await api.getFile(id);
    state.cache.set(id, { ...data, rows: data.rows || [] });
  }
  const data = state.cache.get(id);
  state.rows = data.rows || [];
  state.binary = Boolean(data.binary);
  if (options.scrollTop) {
    dom.viewport.scrollTop = 0;
  }
  recomputeDisplay();
  renderTree();
  renderGroupAndFile();
  renderNotice();
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
  document.documentElement.dataset.theme = resolved;
  dom.theme.value = state.theme;
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
  state.review = await api.getReview();
  state.entries = flatten(state.review);
  state.visible = state.entries.slice();
  renderTopbar();
  renderTree();
  renderFooter();
  if (state.visible.length > 0) {
    await selectIndex(0, { scrollTop: true });
  }
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
