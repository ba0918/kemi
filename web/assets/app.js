// @ts-check
// kemi のページ。仮想スクロールで表示中の行だけを DOM に載せる。

import * as api from "./api.js";
import {
  buildTree,
  collapseDefault,
  commentCountChanges,
  commentLabel,
  commentedLines,
  commitTypeBox,
  currentStopIndex,
  describeComment,
  firstLine,
  hasLoadedStops,
  hasStops,
  navStops,
  nextFileIndex,
  nextStop,
  originJumpTarget,
  rulerMarks,
  seenProgress,
  statusLetter,
  submitSummary,
  unitSwitchOrder,
  unitSwitchTarget,
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
  rangeAfterSkip,
  resolveTheme,
  sideTone,
  suggestionAllowed,
  toDisplayLines,
  treeOrder,
  windowFor,
  withRowIndex,
} from "./model.js";

/** @typedef {import("./model.js").FileEntry} FileEntry */
/** @typedef {import("./model.js").LogicalRow} LogicalRow */
/** @typedef {import("./model.js").DisplayLine} DisplayLine */

const ROW_HEIGHT = 24;
const OVERSCAN = 12;
/** 変更間の移動で、止まる場所を画面の上端からこの分だけ下に置く（前の文脈を見せる）。 */
const NAV_MARGIN = 48;

/** @type {Record<string, string>} */
const UNIT_LABELS = { file: "最終形", commit: "コミットごと" };

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
const ORIGIN_ICON =
  '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4"><circle cx="4" cy="4" r="1.8"/><circle cx="4" cy="12" r="1.8"/><circle cx="12" cy="8" r="1.8"/><path d="M4 5.8v4.4M5.6 4.8 10.4 7.2"/></svg>';

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
  unitSwitch: must("#unit-switch"),
  meta: must("#review-meta"),
  progress: must("#progress"),
  progressBar: must("#progress-bar"),
  progressText: must("#progress-text"),
  ruler: must("#ruler"),
  rulerCanvas: /** @type {HTMLCanvasElement} */ (must("#ruler-canvas")),
  rulerView: must("#ruler-view"),
  nav: must("#nav"),
  navPrev: /** @type {HTMLButtonElement} */ (must("#nav-prev")),
  navNext: /** @type {HTMLButtonElement} */ (must("#nav-next")),
  navPos: must("#nav-pos"),
  toast: must("#toast"),
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
  btnComments: /** @type {HTMLButtonElement} */ (must("#btn-comments")),
  commentCount: must("#comment-count"),
  commentList: must("#comment-list"),
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
 * @typedef {{ fileId: string, side: string, start: number, end: number, anchor: number, wide: boolean, body: string, suggestion: string, suggestionOn: boolean, needsFocus: boolean, editId?: string }} Editor
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
 *   staleRows: Set<number>,
 *   threads: { byLine: Map<number, any[]>, floating: any[] },
 *   binary: boolean,
 *   collapsedOverrides: Record<string, boolean>,
 *   rendering: boolean,
 *   measureNext: boolean,
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
 *   groupHeaderOpen: Map<string, boolean>,
 *   commentOpen: Map<string, boolean>,
 *   commented: Set<number>,
 *   loading: boolean,
 *   navigating: boolean,
 *   origins: Map<string, any>,
 *   originForced: Set<string>,
 *   originOpen: Map<string, string>,
 *   skipRanges: Map<number, any>,
 *   units: any[],
 *   unit: string | null,
 *   reviews: Map<string, any>,
 *   pendingUnit: { unit: string, jump: any } | null,
 *   allComments: any[],
 *   stops: number[],
 *   rulerDirty: boolean,
 *   pendingJump: { side: string, line: number } | "first" | "last" | null,
 *   toastTimer: number,
 *   groupHeads: Map<string, { root: HTMLElement, count: HTMLElement, bar: HTMLElement }>,
 *   modalAction: (() => void) | null,
 *   lastNav: { index: number, top: number, scrollTop: number } | null,
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
  staleRows: new Set(),
  threads: { byLine: new Map(), floating: [] },
  binary: false,
  collapsedOverrides: {},
  rendering: false,
  measureNext: false,
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
  groupHeaderOpen: new Map(),
  commentOpen: new Map(),
  commented: new Set(),
  loading: false,
  navigating: false,
  origins: new Map(),
  originForced: new Set(),
  originOpen: new Map(),
  skipRanges: new Map(),
  units: [],
  unit: null,
  reviews: new Map(),
  pendingUnit: null,
  allComments: [],
  stops: [],
  rulerDirty: true,
  pendingJump: null,
  toastTimer: 0,
  groupHeads: new Map(),
  modalAction: null,
  lastNav: null,
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
 * 解析済みのアイコン。ツリーの項目ごとに markup を解析し直さず、複製して使う。
 * @type {Map<string, SVGElement>}
 */
const parsedIcons = new Map();

/**
 * @param {string} markup
 * @returns {SVGElement}
 */
function svgIcon(markup) {
  let icon = parsedIcons.get(markup);
  if (!icon) {
    const template = document.createElement("template");
    template.innerHTML = markup;
    icon = /** @type {SVGElement} */ (template.content.firstElementChild);
    parsedIcons.set(markup, icon);
  }
  return /** @type {SVGElement} */ (icon.cloneNode(true));
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
  // n / p と j / k が次のファイルを選ぶ順は、ツリーに描かれる順（R-NAV）。
  state.visible = treeOrder(/** @type {Entry[]} */ (files.map((file) => byId.get(file.id))));
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
  if (state.submitted) {
    return;
  }
  state.updateAvailable = false;
  renderUpdateBadge();
  const scrollTop = dom.viewport.scrollTop;
  // 取得中の展開応答が、新しいレビューで消えたキャッシュへ古い行を戻さないようにする。
  state.selectGeneration += 1;
  state.cache.clear();
  state.commentStore.clear();
  state.origins.clear();
  // 取得中の古い単位の応答が、新しいレビューの控えに入らないよう入れ物ごと替える。
  state.reviews = new Map();
  applyReview(await api.getReview(true, state.unit), true);
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
  renderUnitSwitch();
  renderProgress();
  dom.commentCount.textContent = String(state.allComments.length);
  dom.btnComments.setAttribute("aria-label", `コメントの一覧（${state.allComments.length} 件）`);
  dom.btnUnified.setAttribute("aria-pressed", String(state.mode === "unified"));
  dom.btnSplit.setAttribute("aria-pressed", String(state.mode === "split"));
  dom.btnWrap.setAttribute("aria-pressed", String(state.wrap));
  dom.chipFocus.setAttribute("aria-pressed", String(state.focusOnly));
  dom.chipSort.setAttribute("aria-pressed", String(state.sortBySize));
}

/** コミット範囲だけに出す「最終形 | コミットごと」の切り替え（R-UNIT）。 */
function renderUnitSwitch() {
  const focusKey = focusKeyWithin(dom.unitSwitch);
  dom.unitSwitch.textContent = "";
  dom.unitSwitch.hidden = state.units.length === 0;
  for (const status of unitSwitchOrder(state.units)) {
    const unit = String(status.unit);
    const item = button("unit-button");
    const pending = state.pendingUnit && state.pendingUnit.unit === unit;
    let label = UNIT_LABELS[unit] || unit;
    if (status.state === "failed") {
      label = `${label}（作れなかった）`;
      item.classList.add("failed");
      item.title = `作れなかった: ${status.error || ""}（押すと理由と再試行）`;
    } else if (status.state === "building" && pending) {
      label = `${label}（読み込み中…）`;
    }
    item.textContent = label;
    item.dataset.focusKey = `unit:${unit}`;
    item.setAttribute("aria-pressed", String(unit === state.unit));
    item.addEventListener("click", () => void switchUnit(unit, null));
    dom.unitSwitch.append(item);
  }
  restoreFocusKey(dom.unitSwitch, focusKey);
}

/**
 * 描き直しで作り直す操作にフォーカスがあれば、その操作の鍵（data-focus-key）。
 * 作り直した後に同じ鍵の要素へフォーカスを戻し、キーボードの操作を続けられるようにする。
 * @param {HTMLElement} container
 * @returns {string | null}
 */
function focusKeyWithin(container) {
  const active = document.activeElement;
  if (!(active instanceof HTMLElement) || !container.contains(active)) {
    return null;
  }
  return active.dataset.focusKey ?? null;
}

/**
 * @param {HTMLElement} container
 * @param {string | null} key
 */
function restoreFocusKey(container, key) {
  if (!key) {
    return;
  }
  const next = container.querySelector(`[data-focus-key="${CSS.escape(key)}"]`);
  if (next instanceof HTMLElement && next !== document.activeElement) {
    next.focus({ preventScroll: true });
  }
}

/** 上部の、表示中のグループ単位の見たの進捗（R-SEEN）。 */
function renderProgress() {
  const progress = seenProgress(state.entries.map((entry) => entry.file));
  dom.progress.hidden = !state.review || progress.total === 0;
  dom.progressBar.style.width = `${progress.total ? (progress.seen / progress.total) * 100 : 0}%`;
  dom.progressText.textContent = `見た ${progress.seen} / ${progress.total}`;
}

/**
 * グループの見出しの題。コミットごとでは件名の種類を枠で示し、件名から外す。
 * @param {HTMLElement} parent
 * @param {any} group
 * @param {string} className
 */
function appendGroupTitle(parent, group, className) {
  const title = el("span", className);
  const box = state.unit === "commit" ? commitTypeBox(group.title || "") : null;
  if (box) {
    title.append(textEl("span", "ctype", box.type), document.createTextNode(box.title));
  } else {
    title.append(document.createTextNode(group.title || group.id));
  }
  parent.append(title);
}

/**
 * @param {string} groupId
 * @returns {{ seen: number, total: number, done: boolean }}
 */
function groupProgressFor(groupId) {
  return seenProgress(
    state.entries.filter((entry) => entry.group.id === groupId).map((entry) => entry.file),
  );
}

/**
 * ツリーのグループ見出しの進捗。全部見たら数の代わりに「閲」の印を出す。
 * @param {string} groupId
 */
function updateGroupHead(groupId) {
  const head = state.groupHeads.get(groupId);
  if (!head) {
    return;
  }
  const progress = groupProgressFor(groupId);
  head.root.classList.toggle("done", progress.done);
  head.count.textContent = "";
  if (progress.done) {
    const seal = textEl("span", "seal", "閲");
    seal.title = "すべて見た";
    head.count.append(seal);
  } else {
    head.count.append(textEl("span", "g-count", `${progress.seen} / ${progress.total}`));
  }
  head.bar.style.width = `${progress.total ? (progress.seen / progress.total) * 100 : 0}%`;
}

/**
 * ファイルのコメント（ファイル全体のコメントを含む）。コメントの JSON は submit の契約の
 * 形でファイル id を持たないので、サーバが id を振るのと同じ（グループ, パス）で対応付ける。
 * @param {Entry} entry
 * @returns {any[]}
 */
function commentsOf(entry) {
  return state.allComments.filter(
    (comment) => comment.group_id === entry.group.id && comment.path === entry.file.path,
  );
}

/**
 * コメントの件数が変わったファイルの、ツリーの項目だけを描き直す。ツリー全体は
 * 作り直さない（コミットごとで数万項目あると、1 回のコメントで固まる）。
 * @param {any[]} before 変わる前のすべてのコメント
 */
function refreshCommentBadges(before) {
  for (const changed of commentCountChanges(before, state.allComments)) {
    for (const entry of state.visible) {
      if (
        entry.group.id === changed.group_id &&
        entry.file.path === changed.path &&
        state.treeItems.has(entry.file.id)
      ) {
        treeItem(entry, entry.file.path.split("/").pop() ?? entry.file.path, state.treeItems);
      }
    }
  }
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
  state.groupHeads = new Map();
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
    const count = el("span", "g-progress");
    head.append(caret);
    appendGroupTitle(head, group, "gtitle");
    head.append(count);
    head.addEventListener("click", () => {
      const nextOpen = groupEl.dataset.open === "false";
      groupEl.dataset.open = nextOpen ? "true" : "false";
      state.groupOpen.set(group.id, nextOpen);
      head.setAttribute("aria-expanded", String(nextOpen));
      caret.textContent = nextOpen ? "▾" : "▸";
    });
    const barTrack = el("div", "g-bar");
    const bar = el("i");
    barTrack.append(bar);
    groupEl.append(head, barTrack);
    // ツリーのグループ見出しには why と watch を出さない（グループ帯に出す）。
    const body = el("div", "group-body");
    const list = el("ul", "files");
    appendNodes(list, nodes, group, items);
    body.append(list);
    groupEl.append(body);
    fragment.append(groupEl);
    state.groupHeads.set(group.id, { root: groupEl, count, bar });
    updateGroupHead(group.id);
  }
  dom.tree.append(fragment);
  // 使い回したボタンに前の選択の印が残ると、2 つのファイルが選ばれて見える。印を外してから
  // 選び直させる。
  if (state.treeActiveId) {
    items.get(state.treeActiveId)?.classList.remove("active");
  }
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
  // 見たは左端のチェックで示す。取り消し線は削除と見分けがつかないので使わない。
  item.classList.toggle("seen", entry.file.seen);
  const letter = statusLetter(entry.file.status);
  item.append(
    el("span", "chk"),
    svgIcon(FILE_ICON),
    textEl("span", "fname", label),
  );
  const comments = commentsOf(entry).length;
  if (comments > 0) {
    item.append(textEl("span", "cbadge", `💬 ${comments}`));
  }
  if (entry.file.focus) {
    item.append(textEl("span", "badge-focus", "重要"));
  }
  if (entry.file.noise) {
    item.append(textEl("span", "badge-noise", "ノイズ"));
  }
  item.append(textEl("span", `sl ${letter}`, letter), fileStatsEl(entry.file));
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

/** 本文側のグループ帯。why は既定で畳み（開閉はページを開いている間だけ覚える）、watch は常に出す。 */
function renderGroupHeader() {
  const entry = currentEntry();
  dom.groupHeader.textContent = "";
  if (!entry) {
    return;
  }
  const open = state.groupHeaderOpen.get(entry.group.id) === true;
  dom.groupHeader.dataset.open = open ? "true" : "false";
  const line = el("div", "gh-line");
  appendGroupTitle(line, entry.group, "gh-title");
  const progress = groupProgressFor(entry.group.id);
  line.append(textEl("span", "gh-st", `${progress.seen} / ${progress.total} 見た`));
  if (entry.group.why) {
    const toggle = button("gh-toggle");
    const label = () => (dom.groupHeader.dataset.open === "true" ? "説明を畳む" : "説明を開く");
    toggle.textContent = label();
    toggle.setAttribute("aria-expanded", String(open));
    toggle.addEventListener("click", () => {
      const nextOpen = dom.groupHeader.dataset.open !== "true";
      dom.groupHeader.dataset.open = nextOpen ? "true" : "false";
      state.groupHeaderOpen.set(entry.group.id, nextOpen);
      toggle.setAttribute("aria-expanded", String(nextOpen));
      toggle.textContent = label();
    });
    line.append(toggle);
  }
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
  const focusKey = focusKeyWithin(dom.fileHeader);
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
  const letter = statusLetter(entry.file.status);
  dom.fileHeader.append(textEl("span", `sl ${letter}`, letter));
  const comments = commentsOf(entry).length;
  if (comments > 0) {
    const badge = textEl("span", "cbadge", `💬 ${comments}`);
    badge.title = `このファイルのコメント ${comments} 件（ファイル全体へのコメントを含む）`;
    dom.fileHeader.append(badge);
  }
  dom.fileHeader.append(fileStatsEl(entry.file));
  if (entry.file.focus) {
    dom.fileHeader.append(textEl("span", "badge focus", "重要"));
  }
  if (entry.file.note) {
    dom.fileHeader.append(textEl("span", "note", entry.file.note));
  }
  dom.fileHeader.append(el("span", "spacer"));

  // 取得が終わるまでは、前のファイルに操作が届かないよう、ヘッダの操作を無効にする。
  const busy = state.loading;
  const comment = iconButton("ファイル全体にコメント", COMMENT_ICON);
  comment.disabled = state.submitted || busy;
  comment.dataset.focusKey = "file-comment";
  comment.addEventListener("click", openFileWideEditor);
  dom.fileHeader.append(comment);

  const copy = iconButton("パスをコピー", COPY_ICON);
  copy.dataset.focusKey = "file-copy";
  copy.addEventListener("click", () => void copyPath(entry.file.path, copy));
  dom.fileHeader.append(copy);

  const fullyExpanded = allLinesExpanded();
  const expand = iconButton(
    fullyExpanded ? "すべて折りたたむ" : "すべての行を展開",
    EXPAND_ICON,
  );
  expand.disabled = state.submitted || state.binary || busy;
  expand.dataset.focusKey = "file-expand";
  expand.addEventListener("click", () => {
    if (fullyExpanded) {
      collapseAll();
    } else {
      void expandAll();
    }
  });
  dom.fileHeader.append(expand);

  const highlight = iconButton(highlightTitle(), CODE_ICON);
  highlight.disabled = busy;
  highlight.dataset.focusKey = "file-highlight";
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

  if (originAvailable() && !state.binary && !state.highlightCapable) {
    const forced = state.originForced.has(entry.file.id);
    const origin = iconButton(
      forced ? "このファイルの由来を隠す" : "このファイルで由来を求める",
      ORIGIN_ICON,
    );
    origin.disabled = busy;
    origin.dataset.focusKey = "file-origin";
    origin.setAttribute("aria-pressed", String(forced));
    origin.addEventListener("click", () => {
      if (forced) {
        state.originForced.delete(entry.file.id);
      } else {
        state.originForced.add(entry.file.id);
      }
      recomputeDisplay();
      renderFileHeader();
      renderDiff();
      void loadOrigin(entry);
    });
    dom.fileHeader.append(origin);
  }

  // 見たは文字付きのチェック。キー v でも付け外しできる（R-SEEN）。
  const seen = button(`seen-toggle${entry.file.seen ? " on" : ""}`);
  seen.title = entry.file.seen ? "見たを取り消す（v）" : "見たにする（v）";
  seen.disabled = busy;
  seen.dataset.focusKey = "file-seen";
  seen.setAttribute("aria-pressed", String(entry.file.seen));
  seen.append(el("span", "chk"), document.createTextNode("見た"), textEl("kbd", "", "v"));
  seen.addEventListener("click", () => void toggleSeen(entry.file));
  dom.fileHeader.append(seen);
  restoreFocusKey(dom.fileHeader, focusKey);
}

/** 最終形（由来を持つ単位）を表示しているか。 */
function originAvailable() {
  return Boolean(state.review && state.review.unit === "file");
}

/** 表示中のファイルで由来の行を出すか。上限を超えるファイルは有効にしたときだけ。 */
function originShown() {
  const entry = currentEntry();
  if (!entry || !originAvailable() || state.binary) {
    return false;
  }
  return state.highlightCapable || state.originForced.has(entry.file.id);
}

/**
 * @param {string} fileId
 * @returns {string}
 */
function originKey(fileId) {
  return `${fileId}|${state.originForced.has(fileId) ? 1 : 0}`;
}

/** 表示中のファイルの由来。取得中は "pending"、まだなら undefined。 */
function currentOrigin() {
  const entry = currentEntry();
  return entry ? state.origins.get(originKey(entry.file.id)) : undefined;
}

/**
 * 由来は差分の表示を待たせず、後から取りに行って付ける（R-ORIGIN）。
 * @param {Entry} entry
 */
async function loadOrigin(entry) {
  const id = entry.file.id;
  const key = originKey(id);
  if (!originShown() || state.origins.has(key)) {
    return;
  }
  state.origins.set(key, "pending");
  try {
    const data = await api.getOrigin(id, state.originForced.has(id));
    /** @type {Map<number, any>} */
    const blocks = new Map();
    for (const block of data.blocks || []) {
      blocks.set(Number(block.row), block);
    }
    state.origins.set(key, { ...data, blocks });
  } catch (error) {
    state.origins.set(key, { failed: String(error), blocks: new Map() });
  }
  if (isShowingFile(id)) {
    renderDiff();
  }
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
  if (state.loading) {
    dom.notice.hidden = false;
    dom.notice.append(textEl("span", "notice-text", "読み込み中…"));
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

/** ファイル全体へのコメント（と、表示中の行に見つからないコメント）を、ヘッダの下に吹き出しで出す。 */
/** ファイル全体へのコメント（と、表示行に見つからないコメント）を、ヘッダの下に同じ吹き出しで出す。 */
function renderFloating() {
  const focusKey = focusKeyWithin(dom.floating);
  dom.floating.textContent = "";
  const floating = state.threads.floating;
  const wideEditor = state.editor && state.editor.wide && !state.editor.editId ? state.editor : null;
  if (floating.length === 0 && !wideEditor) {
    dom.floating.hidden = true;
    return;
  }
  dom.floating.hidden = false;
  if (wideEditor) {
    dom.floating.append(renderEditor(wideEditor));
  }
  for (const comment of floating) {
    const row = el("div", "bal-row floating");
    row.append(renderCommentOrEditor(comment));
    dom.floating.append(row);
  }
  restoreFocusKey(dom.floating, focusKey);
}

/**
 * 編集中ならエディタ、そうでなければコメント（札か吹き出し）。
 * @param {any} comment
 * @returns {HTMLElement}
 */
function renderCommentOrEditor(comment) {
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
    chip.addEventListener("click", () => {
      state.commentOpen.set(comment.id, true);
      remeasure();
      renderDiff();
      renderFloating();
    });
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
  const actions = el("span", "acts");
  if (!state.submitted) {
    const edit = button("");
    edit.textContent = "編集";
    edit.addEventListener("click", () => openCommentEditor(comment));
    const remove = button("");
    remove.textContent = "削除";
    remove.addEventListener("click", () => confirmDeleteComment(comment));
    actions.append(edit, remove);
  }
  const fold = button("");
  fold.textContent = "畳む";
  fold.dataset.focusKey = `comment:${comment.id}`;
  fold.addEventListener("click", () => {
    state.commentOpen.set(comment.id, false);
    remeasure();
    renderDiff();
    renderFloating();
  });
  actions.append(fold);
  head.append(actions);
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
 * コメントの本文と suggestion を編集する。行レンジは変えない（R-COMMENT）。
 * @param {any} comment
 */
function openCommentEditor(comment) {
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
  remeasure();
  renderDiff();
  renderFloating();
}

/**
 * 削除は確認を 1 回挟む（R-COMMENT）。
 * @param {any} comment
 */
function confirmDeleteComment(comment) {
  dom.modalTitle.textContent = "コメントを削除しますか？";
  setModalText(`${commentLabel(comment)}: ${firstLine(comment.body)}\n削除したコメントは送信する JSON に含まれません。`);
  dom.modalOk.textContent = "削除";
  dom.modalOk.className = "btn secondary";
  dom.modalCancel.textContent = "戻る";
  state.modalAction = () => void deleteComment(comment);
  dom.modal.hidden = false;
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
 * @param {any} comment
 */
async function deleteComment(comment) {
  try {
    await api.postComment({ op: "delete", id: comment.id });
    updateComments((comments) => comments.filter((item) => item.id !== comment.id));
  } catch (error) {
    showOverlay("コメントを削除できません", String(error));
  }
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

  const actions = el("div", "row");
  const cancel = button("btn");
  cancel.textContent = "やめる";
  cancel.addEventListener("click", closeEditor);
  const submit = /** @type {HTMLButtonElement} */ (el("button", "btn primary"));
  submit.type = "submit";
  submit.textContent = editor.editId ? "保存" : "コメント";
  actions.append(cancel, submit);
  form.append(actions);

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (editor.editId) {
      void editComment({
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
  remeasure();
  renderDiff();
  renderFloating();
}

/**
 * エディタやコメント、由来の理由の出入りで、次の描画で表示中の行の高さを測り直させる。
 * 見えていない行の測った高さは残す。捨てると、上の行が詰まって見ている位置がずれる。
 * 基準値と違う高さの行は中身が変わったかもしれないので、窓の外の行も、次に描いたときに
 * 測り直すよう覚えておく。
 */
function remeasure() {
  state.measureNext = true;
  state.heights.forEach((height, index) => {
    if (height !== ROW_HEIGHT) {
      state.staleRows.add(index);
    }
  });
}

/** 表示する行が変わったときや折返しを切り替えたとき、行の高さを基準値へ戻す。 */
function resetHeights() {
  state.heights = new Array(state.display.length).fill(ROW_HEIGHT);
  state.staleRows = new Set();
  // 位置の帯の印は行の高さから描く。測る行が無くても、古い高さの印を残さない。
  state.rulerDirty = true;
  // 移った先の記録は古い行と高さでの位置なので、もう同じ行を指さない。
  state.lastNav = null;
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
  const focusKey = focusKeyWithin(dom.content);
  dom.content.textContent = "";
  if (
    !entry ||
    state.binary ||
    collapseDefault(entry.file, state.collapsedOverrides)
  ) {
    dom.content.style.height = "0px";
    renderNav([0]);
    renderRuler([0]);
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
  restoreFocusKey(dom.content, focusKey);
  renderNav(offsets);
  renderRuler(offsets);
  const needsMeasure =
    state.measureNext ||
    state.staleRows.size > 0 ||
    state.wrap ||
    state.threads.byLine.size > 0 ||
    state.originOpen.size > 0 ||
    Boolean(state.editor && !state.editor.wide);
  state.measureNext = false;
  if (needsMeasure) {
    measureHeights(window.start, offsets);
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
  const rendered = renderLine(line);
  if (state.commented.has(index)) {
    rendered.classList.add("commented");
  }
  block.append(rendered);
  if (line.kind === "origin") {
    const reason = renderOriginReason(line);
    if (reason) {
      block.append(reason);
    }
  }
  const threads = state.threads.byLine.get(index);
  if (threads) {
    for (const comment of threads) {
      // 吹き出しは範囲の最後の行の直下で、コードの列の位置から始める。
      const row = el("div", `bal-row mode-${state.mode} side-${comment.side}`);
      row.append(renderCommentOrEditor(comment));
      block.append(row);
    }
  }
  const editorState = state.editor;
  const entry = currentEntry();
  if (
    editorState &&
    !editorState.wide &&
    !editorState.editId &&
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
  if (line.kind === "origin") {
    return renderOriginLine(line);
  }
  const tone = state.mode === "split" ? "" : sideTone(line.kind, null);
  const row = el("div", `row kind-${line.kind}${tone ? ` tone-${tone}` : ""}`);
  if (line.kind === "skip") {
    const skip = line.skip;
    const expand = button("expand-button");
    expand.textContent = `↕ ${skip && skip.count ? skip.count : 0} 行を表示`;
    expand.disabled = state.loading;
    expand.addEventListener("click", () => {
      void expandSkipAt(line.logicalIndex).then(() => renderFileHeader());
    });
    row.append(expand);
    const range = state.skipRanges.get(line.logicalIndex);
    if (range) {
      row.append(textEl("span", "skip-range", rangeLabel(range)));
    }
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
        sideTone(line.kind, "old"),
      ),
      sideCell(
        "new",
        line.newLine,
        line.newSegments,
        canComment && plusSide === "new",
        sideTone(line.kind, "new"),
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
 * 折りたたみ行に出す、下に続く範囲の旧・新の行番号。
 * @param {{ old: { start: number, end: number } | null, new: { start: number, end: number } | null }} range
 * @returns {string}
 */
function rangeLabel(range) {
  /** @param {{ start: number, end: number }} span */
  const text = (span) => (span.start === span.end ? `${span.start}` : `${span.start}–${span.end}`);
  const parts = [];
  if (range.old) {
    parts.push(`旧 ${text(range.old)}`);
  }
  if (range.new) {
    parts.push(`新 ${text(range.new)}`);
  }
  return parts.join(" → ");
}

/**
 * 変更ブロックのすぐ上の由来の行（R-ORIGIN）。計算中は行だけ先に出して印を置く。
 * @param {DisplayLine} line
 * @returns {HTMLElement}
 */
function renderOriginLine(line) {
  const row = el("div", "row origin-row");
  row.append(textEl("span", "origin-label", "由来"));
  const origin = currentOrigin();
  if (!origin || origin === "pending") {
    row.append(textEl("span", "origin-pending", "計算中…"));
    return row;
  }
  if (origin.failed) {
    row.append(textEl("span", "origin-unknown", "求められませんでした"));
    return row;
  }
  const block = origin.blocks.get(Number(line.block));
  const entries = block ? block.entries : [];
  const openKey = `${currentEntry()?.file.id}:${line.block}`;
  for (const entry of entries) {
    const commit = (origin.commits || {})[entry.sha] || { subject: "", body: "" };
    const short = String(entry.sha).slice(0, 7);
    const item = button("origin-entry");
    item.dataset.focusKey = `origin:${openKey}:${entry.sha}`;
    item.textContent = entry.merge ? `マージ ${short}` : `${short} ${commit.subject}`;
    item.title = commit.subject || short;
    const open = state.originOpen.get(openKey) === entry.sha;
    item.setAttribute("aria-expanded", String(open));
    item.addEventListener("click", () => {
      if (open) {
        state.originOpen.delete(openKey);
      } else {
        state.originOpen.set(openKey, entry.sha);
      }
      remeasure();
      renderDiff();
    });
    row.append(item);
  }
  if (!block || block.unknown === "all") {
    row.append(textEl("span", "origin-unknown", "特定できない"));
  } else if (block.unknown === "some") {
    row.append(textEl("span", "origin-unknown", "一部特定できない"));
  }
  return row;
}

/**
 * 由来を押したときに開く、そのコミットの理由（本文）。
 * @param {DisplayLine} line
 * @returns {HTMLElement | null}
 */
function renderOriginReason(line) {
  const entry = currentEntry();
  const origin = currentOrigin();
  if (!entry || !origin || origin === "pending" || origin.failed) {
    return null;
  }
  const sha = state.originOpen.get(`${entry.file.id}:${line.block}`);
  if (!sha) {
    return null;
  }
  const commit = (origin.commits || {})[sha] || { subject: "", body: "", merge: false };
  const panel = el("div", "origin-reason");
  const head = el("div", "origin-reason-head");
  head.append(
    textEl("span", "origin-sha", String(sha).slice(0, 7)),
    textEl("span", "origin-subject", commit.subject),
  );
  panel.append(head);
  panel.append(
    textEl("p", "origin-body", commit.body || "（本文はありません）"),
  );
  const block = origin.blocks.get(Number(line.block));
  const target = block ? block.entries.find((/** @type {any} */ item) => item.sha === sha) : null;
  // マージの由来は理由を開くだけで、移り先を持たない。
  if (target && !target.merge && target.target) {
    const jump = button("btn origin-jump");
    jump.textContent = "このコミットで見る";
    jump.addEventListener("click", () => {
      const to = target.target;
      void switchUnit("commit", {
        find: (entries) => originJumpTarget(entries, sha, to),
        side: to.side,
        line: to.line,
        missing: () => showToast("移り先のファイルが見つかりません"),
      });
    });
    panel.append(jump);
  }
  return panel;
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
 * @param {string} tone
 * @returns {HTMLElement}
 */
function sideCell(side, line, segments, withPlus, tone) {
  const cell = el("span", `cell${tone ? ` side-${tone}` : ""}`);
  const code = el("span", "code");
  fillCode(code, line, segments);
  const sign = tone === "del" ? "−" : tone === "add" ? "+" : "";
  cell.append(numberCell(side, line, withPlus), textEl("span", "mk", sign), code);
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
    localStorage.removeItem(draftKey(payload.file_id, selection));
  } catch (error) {
    showOverlay("コメントを追加できません", String(error));
  }
}

/**
 * @param {any} payload
 */
async function editComment(payload) {
  try {
    const updated = await api.postComment(payload);
    state.editor = null;
    updateComments((comments) =>
      comments.map((item) => (item.id === updated.id ? updated : item)),
    );
  } catch (error) {
    showOverlay("コメントを編集できません", String(error));
  }
}

/**
 * 送信の前の確認。取り消せないことと verdict に加えて、送るコメントの件数（両方の
 * グループ単位の合計）と、表示中の単位の見たファイル数を出す（R-SUBMIT）。
 * @param {"approved" | "changes_requested"} verdict
 */
function openConfirm(verdict) {
  if (state.submitted) {
    return;
  }
  state.pendingVerdict = verdict;
  state.modalAction = () => void submitReview(verdict);
  dom.modalCancel.textContent = "戻る";
  const approve = verdict === "approved";
  const summary = submitSummary(
    state.allComments,
    state.entries.map((entry) => entry.file),
  );
  dom.modalTitle.textContent = approve ? "承認して終了しますか？" : "変更要求で終了しますか？";
  dom.modalBody.textContent = "";
  const list = el("dl", "sum");
  const seenLabel = state.unit ? `見たファイル（${UNIT_LABELS[state.unit] || state.unit}）` : "見たファイル";
  list.append(
    textEl("dt", "", "コメント"),
    textEl("dd", "", `${summary.comments} 件（うち suggestion 付き ${summary.suggestions} 件）`),
    textEl("dt", "", seenLabel),
    textEl("dd", "", `${summary.seen} / ${summary.total}`),
  );
  dom.modalBody.append(list);
  if (summary.unseen > 0) {
    dom.modalBody.append(
      textEl("p", "warn", `まだ見ていないファイルが ${summary.unseen} あります。`),
    );
  }
  dom.modalBody.append(
    textEl(
      "p",
      "",
      `レビューを終了して、${approve ? "承認" : "変更要求"}の verdict とコメントを実行ターミナルへ JSON で返します。この操作は取り消せません。`,
    ),
  );
  dom.modalOk.textContent = approve ? "承認して終了" : "変更要求で終了";
  dom.modalOk.className = approve ? "btn primary" : "btn secondary";
  dom.modal.hidden = false;
}

/**
 * 確認ダイアログの本文を、改行を保つ段落 1 つにする。
 * @param {string} text
 */
function setModalText(text) {
  dom.modalBody.textContent = "";
  dom.modalBody.append(textEl("p", "", text));
}

function closeModal() {
  dom.modal.hidden = true;
  state.pendingVerdict = null;
  state.modalAction = null;
}

/**
 * @param {"approved" | "changes_requested"} verdict
 */
async function submitReview(verdict) {
  if (state.submitted) {
    return;
  }
  try {
    const answer = await api.submit(verdict);
    state.submitted = true;
    state.selection = null;
    state.editor = null;
    renderSubmitButtons();
    renderDiff();
    renderFloating();
    renderFileHeader();
    showCompletion(verdict, answer);
  } catch (error) {
    showOverlay("送信できませんでした", String(error));
  }
}

/** 送信後は承認と変更要求のボタンを押せなくする（R-SUBMIT）。 */
function renderSubmitButtons() {
  dom.submitApproved.disabled = state.submitted;
  dom.submitChanges.disabled = state.submitted;
}

/**
 * 送信後の完了画面。結果の JSON（stdout と同じ）と、写す操作、結果ファイルの保存先を出す。
 * 承認のときは「閲」の印を押す（R-SUBMIT, R-RESULT）。
 * @param {"approved" | "changes_requested"} verdict
 * @param {any} answer submit の応答（result と saved）
 */
function showCompletion(verdict, answer) {
  const result = answer && answer.result ? answer.result : answer;
  const saved = (answer && answer.saved) || {};
  const json = JSON.stringify(result);
  const comments = Array.isArray(result && result.comments) ? result.comments : [];
  dom.overlay.hidden = false;
  dom.overlayCard.textContent = "";
  dom.overlayCard.className = "finish";
  if (verdict === "approved") {
    const seal = textEl("span", "seal big", "閲");
    seal.setAttribute("aria-hidden", "true");
    dom.overlayCard.append(seal);
  }
  const body = el("div", "finish-body");
  body.append(
    textEl("h2", "overlay-title", verdict === "approved" ? "承認を送りました" : "変更要求を送りました"),
    textEl(
      "p",
      "overlay-detail",
      "kemi は結果を標準出力に書いて終了しました。エージェントが反応しないときは、この JSON をコピーして会話に貼れば済みます。",
    ),
  );
  const row = el("div", "finish-row");
  const copy = button("btn primary");
  copy.textContent = "JSON をコピー";
  copy.addEventListener("click", () => {
    navigator.clipboard
      .writeText(json)
      .then(() => {
        copy.textContent = "コピーしました";
      })
      .catch((error) => {
        copy.textContent = `コピーできません: ${error}`;
      });
  });
  const suggestions = comments.filter((/** @type {any} */ comment) => comment.suggestion).length;
  row.append(copy, textEl("span", "finish-meta", `コメント ${comments.length} 件 / suggestion ${suggestions} 件`));
  body.append(row);
  if (saved.error) {
    body.append(textEl("p", "finish-save failed", `保存できませんでした: ${saved.error}`));
  } else if (saved.path) {
    body.append(textEl("p", "finish-save", `結果ファイル: ${saved.path}（kemi --result で読めます）`));
  } else if (saved.dir) {
    body.append(textEl("p", "finish-save", `結果ファイルの保存先: ${saved.dir}`));
  }
  const pre = el("pre", "finish-json");
  pre.textContent = json;
  body.append(pre);
  dom.overlayCard.append(body);
}

/**
 * @param {string} title
 * @param {string|null} detail
 */
function showOverlay(title, detail) {
  if (state.submitted) {
    // 送信後はサーバが止まっているので、失敗を出しても直せない。完了画面（結果の JSON と
    // その写し）を失敗の知らせで置き換えない（R-SUBMIT）。
    return;
  }
  dom.overlay.hidden = false;
  dom.overlayCard.textContent = "";
  dom.overlayCard.className = "";
  dom.overlayCard.append(textEl("h2", "overlay-title", title));
  if (detail) {
    dom.overlayCard.append(textEl("p", "overlay-detail", detail));
  }
  const close = button("btn overlay-close");
  close.textContent = "閉じる";
  close.addEventListener("click", () => {
    dom.overlay.hidden = true;
  });
  dom.overlayCard.append(close);
}

/**
 * @param {number} start
 * @param {number[]} offsets 描いたときの各行の上端
 */
function measureHeights(start, offsets) {
  const children = Array.from(dom.content.children);
  const scrollTop = dom.viewport.scrollTop;
  // 移動の直後は、移った先の行より上の行の伸び縮みをすべて打ち消す。初めて測る行
  // （折り返した行など）が基準値より高いと、移った先が下へずれて見えなくなるため。
  const nav = activeNav();
  let changed = false;
  let shift = 0;
  let growth = 0;
  children.forEach((child, offset) => {
    const index = start + offset;
    const stale = state.staleRows.delete(index);
    const height = /** @type {HTMLElement} */ (child).offsetHeight;
    if (height > 0 && state.heights[index] !== height) {
      const delta = height - state.heights[index];
      const above = nav ? index < nav.index : stale && offsets[index + 1] <= scrollTop;
      if (above) {
        shift += delta;
      }
      growth += delta;
      state.heights[index] = height;
      changed = true;
    }
  });
  if (changed) {
    state.rulerDirty = true;
  }
  if (shift !== 0) {
    // 見ている位置より上で古い高さのまま残っていた行の伸び縮みを、スクロール位置で
    // 打ち消す。ずれた配置のまま表示されないよう、同じ描画のうちに描き直す。
    // 全体の高さを先に広げる。古い高さのままでは、送った位置が末尾で切り詰められる。
    dom.content.style.height = `${(offsets[offsets.length - 1] || 0) + growth}px`;
    if (nav) {
      const top = (offsets[nav.index] ?? nav.top) + shift;
      dom.viewport.scrollTop = Math.max(0, top - NAV_MARGIN);
      state.lastNav = { index: nav.index, top, scrollTop: dom.viewport.scrollTop };
    } else {
      dom.viewport.scrollTop = scrollTop + shift;
    }
    renderDiff();
    return;
  }
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
 * 幅が変わると折返しの高さも変わる。本文は renderDiff が測り直す。
 */
function scheduleResize() {
  requestAnimationFrame(() => {
    renderDiff();
  });
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
  // 50 万行の展開では、行を引数に展開する splice が引数の上限を超えて失敗する。
  const rows = state.rows.slice(0, index).concat(replacement, state.rows.slice(index + 1));
  state.rows = rows;
  state.cache.set(cacheKey, { ...state.cache.get(cacheKey), rows });
  recomputeDisplay();
  renderDiff();
  renderFloating();
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
  renderFloating();
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
  renderFloating();
}

/** 表示中のファイルで止まれる場所。畳まれたノイズやバイナリでは止まらない。 */
function reachableStops() {
  const entry = currentEntry();
  if (!entry || state.binary || collapseDefault(entry.file, state.collapsedOverrides)) {
    return [];
  }
  return state.stops;
}

/**
 * 右下の「前の変更 / 次の変更」と「現在 / 全体」（R-NAV）。
 * @param {number[]} offsets
 */
function renderNav(offsets) {
  const entry = currentEntry();
  dom.nav.hidden = !entry;
  if (!entry) {
    return;
  }
  const tops = reachableStops().map((index) => offsets[index] ?? 0);
  const current = currentStopIndex(tops, navPosition());
  dom.navPos.textContent = `${current < 0 ? "–" : current + 1} / ${tops.length}`;
  dom.navPrev.disabled = state.loading || state.navigating;
  dom.navNext.disabled = state.loading || state.navigating;
}

/**
 * 表示行 `index` を、上端から NAV_MARGIN の位置へ送る。
 * @param {number} index
 * @param {number[]} offsets
 */
function scrollToRow(index, offsets) {
  const top = offsets[index] ?? 0;
  dom.viewport.scrollTop = Math.max(0, top - NAV_MARGIN);
  // 末尾近くでは止まる場所を上端まで送れない。移った先を覚えておき、利用者が
  // スクロールするまではそこを今の位置として扱う（同じ場所で n が空回りしないように）。
  // 移った先の行より上の行を初めて測って高さが変わったら、measureHeights がこの行を
  // 同じ位置に保つ。
  state.lastNav = { index, top, scrollTop: dom.viewport.scrollTop };
  scheduleRender();
}

/** 移動の後、利用者がまだスクロールしていなければ、その移動の記録。 */
function activeNav() {
  const last = state.lastNav;
  if (last && Math.abs(dom.viewport.scrollTop - last.scrollTop) < 2) {
    return last;
  }
  return null;
}

/** 変更間の移動で使う今の位置。 */
function navPosition() {
  const nav = activeNav();
  return nav ? nav.top : dom.viewport.scrollTop + NAV_MARGIN;
}

/**
 * @param {string} message
 */
function showToast(message) {
  dom.toast.textContent = message;
  dom.toast.hidden = false;
  window.clearTimeout(state.toastTimer);
  state.toastTimer = window.setTimeout(() => {
    dom.toast.hidden = true;
  }, 2_000);
}

/**
 * 移ったファイルが見えるよう、ツリーのグループとディレクトリをそこまで開く。
 * @param {Entry} entry
 */
function revealInTree(entry) {
  let changed = state.groupOpen.get(entry.group.id) === false;
  state.groupOpen.set(entry.group.id, true);
  const segments = entry.file.path.split("/");
  let prefix = "";
  for (let index = 0; index < segments.length - 1; index += 1) {
    prefix = prefix ? `${prefix}/${segments[index]}` : segments[index];
    const key = `${entry.group.id}:${prefix}`;
    if (state.dirOpen.get(key) === false) {
      changed = true;
    }
    state.dirOpen.set(key, true);
  }
  if (changed) {
    state.treeVersion += 1;
  }
}

/**
 * n / p: ファイルの中の次（前）の止まる場所へ。端では、見えている順で次（前）のファイルの
 * 最初（最後）の止まる場所へ移る。最後（最初）のファイルでは止まって知らせる（R-NAV）。
 * @param {1 | -1} direction
 */
async function navigate(direction) {
  const entry = currentEntry();
  if (!entry || state.loading || state.navigating || state.submitted) {
    return;
  }
  const offsets = lineOffsets(state.heights);
  const stops = reachableStops();
  const tops = stops.map((index) => offsets[index] ?? 0);
  const index = nextStop(tops, navPosition(), direction);
  if (index !== null) {
    scrollToRow(stops[index], offsets);
    return;
  }
  const visible = state.visible;
  const files = visible.map((candidate) => candidate.file);
  const here = files.findIndex((file) => file.id === entry.file.id);
  const generation = state.selectGeneration;
  state.navigating = true;
  scheduleRender();
  try {
    let from = here < 0 ? state.index : here;
    for (;;) {
      const next = nextFileIndex(files, from, direction, (file) => {
        const candidate = visible.find((item) => item.file.id === file.id);
        return hasStops(file, candidate ? commentsOf(candidate) : [], state.collapsedOverrides);
      });
      if (next === null) {
        showToast(direction > 0 ? "最後の変更です" : "最初の変更です");
        return;
      }
      // 増減数があっても、改行コードだけの変更などは表示で変更ブロックにならない。
      // 内容を読んで止まる場所が無ければ、移らずにその次を探す（R-NAV）。
      const data = await loadForNavigation(visible[next], generation);
      if (generation !== state.selectGeneration || state.visible !== visible) {
        // 読んでいる間に別のファイルが選ばれたか、並びが変わった。
        return;
      }
      if (data === null || hasLoadedStops(data.rows || [], commentsOf(visible[next]))) {
        state.navigating = false;
        revealInTree(visible[next]);
        state.pendingJump = direction > 0 ? "first" : "last";
        await selectIndex(next, { scrollTop: true });
        return;
      }
      from = next;
    }
  } finally {
    state.navigating = false;
    scheduleRender();
  }
}

/**
 * n / p の移り先の候補の行データを読み、キャッシュに入れる。読めなければ null を返し、
 * 移った先でいつもどおり読み込みの失敗を出す。
 * @param {Entry} entry
 * @param {number} generation 読み始めた時の選択の世代
 * @returns {Promise<any>}
 */
async function loadForNavigation(entry, generation) {
  const key = fileCacheKey(entry);
  if (state.cache.has(key)) {
    return state.cache.get(key);
  }
  /** @type {any} */
  let data;
  try {
    data = await api.getFile(entry.file.id, null, {
      dark: state.dark,
      highlight: state.highlightOverrides.get(entry.file.id),
    });
  } catch {
    return null;
  }
  if (generation !== state.selectGeneration) {
    // 読み直し（テーマや更新）で消えたキャッシュへ、古い行を戻さない。
    return null;
  }
  const rows = data.rows || [];
  const stored = { ...data, rows, collapsedRows: rows };
  state.cache.set(key, stored);
  return stored;
}

/** ファイルを表示し終えた後の移り先（次のファイルの最初の変更や、由来の該当行）へ移る。 */
function applyPendingJump() {
  const jump = state.pendingJump;
  state.pendingJump = null;
  if (!jump) {
    return;
  }
  const offsets = lineOffsets(state.heights);
  const stops = reachableStops();
  if (jump === "first" || jump === "last") {
    if (stops.length > 0) {
      scrollToRow(jump === "first" ? stops[0] : stops[stops.length - 1], offsets);
    }
    return;
  }
  const index = state.display.findIndex((line) => lineHasAnchor(line, jump.side, jump.line));
  if (index >= 0) {
    scrollToRow(index, offsets);
  }
}

/**
 * @param {DisplayLine} line
 * @param {number} index
 * @returns {string}
 */
function rulerKind(line, index) {
  if (state.threads.byLine.has(index)) {
    return "note";
  }
  switch (line.kind) {
    case "delete":
    case "replace-old":
      return "del";
    case "insert":
    case "replace-new":
    case "replace":
      return "add";
    default:
      return "";
  }
}

/**
 * スクロールバーの横の位置の帯。印は帯の高さに縮めて描くので、全行を DOM に描かない。
 * @param {number[]} offsets
 */
function renderRuler(offsets) {
  const height = dom.ruler.clientHeight;
  const total = offsets[offsets.length - 1] || 0;
  const canvas = dom.rulerCanvas;
  const ratio = window.devicePixelRatio || 1;
  const width = dom.ruler.clientWidth;
  if (canvas.height !== Math.round(height * ratio) || canvas.width !== Math.round(width * ratio)) {
    canvas.width = Math.round(width * ratio);
    canvas.height = Math.round(height * ratio);
    state.rulerDirty = true;
  }
  if (state.rulerDirty) {
    state.rulerDirty = false;
    const context = canvas.getContext("2d");
    if (context) {
      context.setTransform(ratio, 0, 0, ratio, 0, 0);
      context.clearRect(0, 0, width, height);
      const styles = getComputedStyle(document.documentElement);
      /** @type {Record<string, string>} */
      const colors = {
        add: styles.getPropertyValue("--add-ink").trim(),
        del: styles.getPropertyValue("--del-ink").trim(),
        note: styles.getPropertyValue("--note-line").trim(),
      };
      const stops = reachableStops().length > 0 || state.display.length > 0;
      const kinds = stops ? state.display.map(rulerKind) : [];
      for (const mark of rulerMarks(kinds, offsets, height)) {
        context.fillStyle = colors[mark.kind];
        if (mark.kind === "note") {
          context.fillRect(1, mark.top, width - 2, Math.max(3, mark.bottom - mark.top));
        } else {
          context.fillRect(3, mark.top, width - 6, mark.bottom - mark.top);
        }
      }
    }
  }
  const viewportHeight = dom.viewport.clientHeight;
  dom.rulerView.hidden = total <= viewportHeight;
  if (total > 0) {
    dom.rulerView.style.top = `${(dom.viewport.scrollTop / total) * height}px`;
    dom.rulerView.style.height = `${Math.max(4, (viewportHeight / total) * height)}px`;
  }
}

/**
 * レビューの応答を表示に取り込む。コメントは両方の単位の分を持つので、取得し直した
 * 応答（fresh）のときだけ置き換える。
 * @param {any} review
 * @param {boolean} fresh
 */
function applyReview(review, fresh) {
  state.review = review;
  state.unit = review.unit ?? null;
  if (Array.isArray(review.units)) {
    state.units = review.units;
  }
  if (fresh) {
    state.allComments = review.comments || [];
  }
  if (state.unit) {
    state.reviews.set(state.unit, review);
  }
  state.entries = flatten(review);
  state.treeItems = new Map();
  state.treeVersion += 1;
  rebuildVisible();
}

/**
 * グループ単位を切り替える（R-UNIT）。同じパスのファイル（コミットごとではそのパスを含む
 * 最初のコミット）を出す。由来やコメント一覧から移るときは、そのファイルの該当行を出す。
 * 移り先が無ければ（履歴の書き換えで消えたなど）別のファイルへは移らず、切り替えもせず
 * `missing` を呼ぶ。
 * @param {string} unit
 * @param {{ find: (entries: Entry[]) => number, side: string, line: number | null, missing: () => void } | null} jump
 */
async function switchUnit(unit, jump) {
  if (state.submitted || (unit === state.unit && !jump)) {
    return;
  }
  const status = state.units.find((candidate) => candidate.unit === unit);
  if (!status) {
    return;
  }
  if (status.state === "failed") {
    openUnitFailure(status);
    return;
  }
  if (status.state !== "ready") {
    state.pendingUnit = { unit, jump };
    renderUnitSwitch();
    showToast(`${UNIT_LABELS[unit] || unit}を読み込み中です`);
    // 手元の状態が古いこともあるので読み直す。できていればそのまま切り替わる。
    void onUnitEvent();
    return;
  }
  state.pendingUnit = null;
  const path = currentEntry()?.file.path ?? "";
  let review = unit === state.unit ? state.review : state.reviews.get(unit);
  const fresh = !review;
  if (!review) {
    try {
      review = await api.getReview(false, unit);
    } catch (error) {
      showOverlay("グループ単位を切り替えられません", String(error));
      return;
    }
  }
  if (jump && jump.find(flatten(review)) < 0) {
    // 読んだ単位は控えておき、コメント一覧で消えたコミットを見分けられるようにする。
    if (fresh) {
      state.reviews.set(unit, review);
    }
    renderUnitSwitch();
    jump.missing();
    return;
  }
  applyReview(review, fresh);
  let index = jump ? jump.find(state.entries) : -1;
  if (index < 0) {
    index = unitSwitchTarget(state.entries, path);
  }
  renderHeader();
  renderFooter();
  if (index < 0) {
    state.current = null;
    renderTree();
    renderGroupHeader();
    renderFileHeader();
    renderNotice();
    renderDiff();
    return;
  }
  const entry = state.entries[index];
  if (jump) {
    state.pendingJump = jump.line === null ? null : { side: jump.side, line: jump.line };
    revealInTree(entry);
  }
  const visibleIndex = state.visible.findIndex((candidate) => candidate.file.id === entry.file.id);
  if (visibleIndex >= 0) {
    state.index = visibleIndex;
  }
  await selectEntry(entry, { scrollTop: true });
}

/**
 * 作れなかった単位の理由と、作り直しの操作。
 * @param {any} status
 */
function openUnitFailure(status) {
  const unit = String(status.unit);
  dom.modalTitle.textContent = `${UNIT_LABELS[unit] || unit}の単位を作れなかった`;
  setModalText(`理由: ${status.error || "不明"}\nレビューはこのまま続けられます。`);
  dom.modalOk.textContent = "再試行";
  dom.modalOk.className = "btn primary";
  dom.modalCancel.textContent = "閉じる";
  state.modalAction = () => {
    state.pendingUnit = { unit, jump: null };
    api
      .retryUnit(unit)
      .then((answer) => {
        state.units = answer.units || state.units;
        renderUnitSwitch();
      })
      .catch((error) => showOverlay("作り直せませんでした", String(error)));
  };
  dom.modal.hidden = false;
}

/** コミットごとの単位の、グループ id と件名（読んであれば）。 */
function commitGroups() {
  const review = state.reviews.get("commit");
  if (!review) {
    return null;
  }
  return new Map(review.groups.map((/** @type {any} */ group) => [group.id, group.title]));
}

/** 上部の入口から開く、すべてのグループ単位のコメントの一覧（常設のパネルではない）。 */
function renderCommentList() {
  dom.commentList.textContent = "";
  const head = el("div", "cl-head");
  head.append(textEl("b", "", `コメント ${state.allComments.length} 件`));
  const close = button("cl-close");
  close.textContent = "閉じる";
  close.addEventListener("click", closeCommentList);
  head.append(close);
  dom.commentList.append(head);
  if (state.allComments.length === 0) {
    dom.commentList.append(textEl("p", "cl-empty", "まだコメントはありません"));
    return;
  }
  const context = { range: state.units.length > 0, commitGroups: commitGroups() };
  const list = el("ul", "cl-items");
  for (const comment of state.allComments) {
    const info = describeComment(comment, context);
    const item = el("li", "cl-item");
    const target = button("cl-target");
    target.disabled = info.vanished;
    const meta = el("span", "cl-meta");
    if (info.vanished) {
      meta.append(textEl("span", "cl-unit vanished", "消えたコミット"));
    } else if (info.unit) {
      meta.append(textEl("span", "cl-unit", UNIT_LABELS[info.unit] || info.unit));
    }
    meta.append(
      textEl("span", "cl-path", comment.path),
      textEl("span", "cl-where", commentLabel(comment)),
    );
    if (comment.outdated || info.vanished) {
      meta.append(textEl("span", "t-outdated-mark", "古いコメント"));
    }
    target.append(meta);
    if (info.subject) {
      target.append(textEl("span", "cl-subject", info.subject));
    }
    target.append(textEl("span", "cl-first", firstLine(comment.body)));
    if (!info.vanished) {
      target.addEventListener("click", () => {
        closeCommentList();
        void goToComment(comment, info.unit);
      });
    }
    item.append(target);
    // 消えたコミットのコメントは移り先が無いので、本文をここで見せる。
    if (info.vanished) {
      item.append(textEl("p", "cl-body", comment.body));
    }
    list.append(item);
  }
  dom.commentList.append(list);
}

function openCommentList() {
  renderCommentList();
  dom.commentList.hidden = false;
  dom.btnComments.setAttribute("aria-expanded", "true");
  void loadCommitGroups();
}

/**
 * コメント一覧で消えたコミットを見分けられるよう、作ってあるコミットごとの単位を
 * まだ読んでいなければ読む（再取得の後は表示中の単位しか控えていない）。
 */
async function loadCommitGroups() {
  const status = state.units.find((candidate) => candidate.unit === "commit");
  if (!status || status.state !== "ready" || state.reviews.has("commit")) {
    return;
  }
  const reviews = state.reviews;
  try {
    const review = await api.getReview(false, "commit");
    if (!reviews.has("commit")) {
      reviews.set("commit", review);
    }
  } catch {
    // 読めなければ消えたかどうかを決めないまま。押したときに移り先を確かめる。
    return;
  }
  if (reviews === state.reviews && !dom.commentList.hidden) {
    renderCommentList();
  }
}

function closeCommentList() {
  dom.commentList.hidden = true;
  dom.btnComments.setAttribute("aria-expanded", "false");
}

/**
 * コメント一覧から、そのコメントの単位へ切り替えて、その行へ移る。
 * @param {any} comment
 * @param {string | null} unit
 */
async function goToComment(comment, unit) {
  const line =
    comment.start_line === null || comment.start_line === undefined
      ? null
      : Number(comment.start_line);
  state.commentOpen.set(comment.id, true);
  /** @param {Entry[]} entries */
  const find = (entries) =>
    entries.findIndex(
      (entry) => entry.group.id === comment.group_id && entry.file.path === comment.path,
    );
  if (unit && unit !== state.unit) {
    // 移り先が無ければ、一覧を開き直して（消えたコミットならそう示して）本文を見せる。
    const missing = () => {
      openCommentList();
      showToast("移り先のファイルが見つかりません");
    };
    await switchUnit(unit, { find, side: comment.side, line, missing });
    return;
  }
  const index = find(state.entries);
  if (index < 0) {
    return;
  }
  const entry = state.entries[index];
  revealInTree(entry);
  state.pendingJump = line === null ? null : { side: comment.side, line };
  const visibleIndex = state.visible.findIndex((candidate) => candidate.file.id === entry.file.id);
  if (visibleIndex >= 0) {
    state.index = visibleIndex;
  }
  await selectEntry(entry, { scrollTop: true });
}

/** もう片方の単位の作成の状態が変わった。待っている切り替えがあれば続ける。 */
async function onUnitEvent() {
  if (state.submitted) {
    return;
  }
  const review = await api.getReview(false);
  state.units = review.units || [];
  renderUnitSwitch();
  const pending = state.pendingUnit;
  if (!pending) {
    return;
  }
  const status = state.units.find((candidate) => candidate.unit === pending.unit);
  if (status && status.state === "ready") {
    void switchUnit(pending.unit, pending.jump);
  } else if (status && status.state === "failed") {
    state.pendingUnit = null;
    renderUnitSwitch();
    openUnitFailure(status);
  }
}

function recomputeThreads() {
  state.threads = placeThreads(state.display, state.comments);
  state.commented = commentedLines(state.display, state.comments);
  state.stops = navStops(state.display, state.comments);
  state.rulerDirty = true;
}

function recomputeDisplay() {
  state.display = toDisplayLines(withRowIndex(state.rows), state.mode, {
    origin: originShown(),
  });
  resetHeights();
  state.skipRanges = new Map();
  state.rows.forEach((row, index) => {
    if (row.kind === "skip") {
      const range = rangeAfterSkip(state.rows, index);
      if (range) {
        state.skipRanges.set(index, range);
      }
    }
  });
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
 * ファイルの行データのキャッシュの鍵。着色は表示色とハイライトの指定で変わる。
 * @param {Entry} entry
 * @returns {string}
 */
function fileCacheKey(entry) {
  const id = entry.file.id;
  const override = state.highlightOverrides.get(id);
  return `${id}|${state.dark ? 1 : 0}|${override ?? "auto"}`;
}

/**
 * @param {Entry} entry
 * @param {{ scrollTop?: boolean, keepEditor?: boolean }} [options]
 */
async function selectEntry(entry, options = { scrollTop: true }) {
  if (state.submitted) {
    // 送信後はサーバが止まっていて、まだ読んでいないファイルは取れない。完了画面を残す。
    return;
  }
  state.current = entry;
  state.lastNav = null;
  const id = entry.file.id;
  const override = state.highlightOverrides.get(id);
  const key = fileCacheKey(entry);
  state.cacheKey = key;
  const generation = ++state.selectGeneration;
  if (!state.cache.has(key)) {
    // ヘッダは取得を待たずに新しいファイルへ切り替え、取得中は操作できなくする。
    state.loading = true;
    state.rows = [];
    state.display = [];
    resetHeights();
    state.threads = { byLine: new Map(), floating: [] };
    renderTree();
    renderGroupHeader();
    renderFileHeader();
    renderNotice();
    renderFloating();
    renderDiff();
    /** @type {any} */
    let data;
    try {
      data = await api.getFile(id, null, {
        dark: state.dark,
        highlight: override,
      });
    } catch (error) {
      if (generation === state.selectGeneration) {
        // 読み込み中のまま止まらないよう戻す。選び直せば取り直す。
        state.loading = false;
        renderFileHeader();
        renderNotice();
        renderDiff();
        showOverlay("ファイルを読み込めません", String(error));
      }
      return;
    }
    if (generation !== state.selectGeneration) {
      // 取得中に別のファイルが選ばれた。古い応答で表示を上書きしない。
      return;
    }
    const rows = data.rows || [];
    state.cache.set(key, { ...data, rows, collapsedRows: rows });
  }
  state.loading = false;
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
  applyPendingJump();
  void loadOrigin(entry);
}

/**
 * @param {FileEntry} file
 */
async function toggleSeen(file) {
  if (state.submitted || state.loading || !isShowingFile(file.id)) {
    return;
  }
  const next = !file.seen;
  file.seen = next;
  state.treeItems.get(file.id)?.classList.toggle("seen", next);
  const entry = currentEntry();
  if (entry) {
    updateGroupHead(entry.group.id);
    renderGroupHeader();
  }
  renderProgress();
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
  state.rulerDirty = true;
  document.documentElement.dataset.theme = resolved;
  const current =
    THEME_LABELS[state.theme] || THEME_LABELS.auto;
  const next = THEME_LABELS[nextTheme(state.theme)] || "";
  dom.btnTheme.title = `テーマ: ${current}（クリックで ${next}）`;
  if (changed && !state.submitted) {
    state.cache.clear();
    const entry = currentEntry();
    if (entry) {
      // テーマ切替は表示色の再取得だけ。入力中のエディタは閉じない。
      void selectEntry(entry, { scrollTop: false, keepEditor: true });
    }
  } else if (!changed) {
    // 明暗が同じでもプリセットが変われば追加・削除の色が変わる。位置の帯を描き直す。
    scheduleRender();
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
  if (state.submitted) {
    // 送信後は完了画面だけを出す。ファイルの移動や見たの切り替えはしない。
    return;
  }
  if (event.key === "Escape") {
    if (!dom.modal.hidden) {
      closeModal();
      return;
    }
    if (!dom.commentList.hidden) {
      closeCommentList();
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
    state.mode,
    state.visible.length,
    state.index,
    state.wrap,
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
    state.wrap = Boolean(action.value);
    resetHeights();
    renderHeader();
    renderDiff();
  }
}

async function boot() {
  applyReview(await api.getReview(false), true);
  renderHeader();
  renderTree();
  renderFooter();
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

document.addEventListener("keydown", handleKey);
document.addEventListener("mouseup", () => {
  state.dragging = null;
});
dom.btnUnified.addEventListener("click", () => setMode("unified"));
dom.btnSplit.addEventListener("click", () => setMode("split"));
dom.btnWrap.addEventListener("click", () => {
  state.wrap = !state.wrap;
  resetHeights();
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
  const action = state.modalAction;
  closeModal();
  if (action) {
    action();
  }
});
dom.btnComments.addEventListener("click", () => {
  if (dom.commentList.hidden) {
    openCommentList();
  } else {
    closeCommentList();
  }
});
document.addEventListener("click", (event) => {
  const target = /** @type {Node} */ (event.target);
  if (
    !dom.commentList.hidden &&
    !dom.commentList.contains(target) &&
    !dom.btnComments.contains(target)
  ) {
    closeCommentList();
  }
});
dom.navPrev.addEventListener("click", () => void navigate(-1));
dom.navNext.addEventListener("click", () => void navigate(1));
dom.ruler.addEventListener("click", (event) => {
  const rect = dom.ruler.getBoundingClientRect();
  const total = lineOffsets(state.heights).at(-1) || 0;
  const ratio = (event.clientY - rect.top) / Math.max(1, rect.height);
  dom.viewport.scrollTop = Math.max(0, ratio * total - dom.viewport.clientHeight / 2);
  scheduleRender();
});
dom.viewport.addEventListener("scroll", scheduleRender);
window.addEventListener("resize", () => {
  state.rulerDirty = true;
  scheduleResize();
});
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
