// @ts-check
// `--live` のページの見方の要素（live.md の R-PAGE-VIEW）: 上部の帯、比べる相手と動いている
// ページを並べる舞台、左のページのツリー。状態は持たず、押されたときの処理は呼ぶ側が渡す。
// `--live` のレビューでだけ読み込む。

import { button, el, textEl } from "../dom.js";
import { WIDTH_CHOICES, WIDTH_MAX, WIDTH_MIN } from "../live-model.js";

/**
 * 表示中のページの変化の一覧の中身。`unmarked` は、消えた要素をスナップショットの側に印で示せないこと。
 * @typedef {{
 *   list: import("../live-diff.js").Change[],
 *   shiftedOpen: boolean,
 *   listed: { main: number, shifted: number },
 *   unmarked: boolean,
 * }} ChangeListState
 * 比べられないときに一覧の代わりに出す知らせ。
 * @typedef {{ notice: string }} ChangeNotice
 */

/**
 * @typedef {{
 *   band: HTMLElement,
 *   viewSeg: HTMLElement,
 *   pageButton: HTMLButtonElement,
 *   codeButton: HTMLButtonElement,
 *   widthSeg: HTMLElement,
 *   widthInput: HTMLInputElement,
 *   widthError: HTMLElement,
 *   compareSlot: HTMLElement,
 *   compareSelect: HTMLSelectElement,
 *   recordButton: HTMLButtonElement,
 *   modeSeg: HTMLElement,
 *   opacity: HTMLInputElement,
 *   mockInput: HTMLInputElement,
 *   mockAssign: HTMLButtonElement,
 *   mockRemove: HTMLButtonElement,
 *   mockReload: HTMLButtonElement,
 *   mockError: HTMLElement,
 *   sideSeg: HTMLElement,
 *   stage: HTMLElement,
 *   refPane: HTMLElement,
 *   refBar: HTMLElement,
 *   refLabel: HTMLElement,
 *   refNotice: HTMLElement,
 *   refViewport: HTMLElement,
 *   refFrame: HTMLIFrameElement,
 *   refMockFrame: HTMLIFrameElement,
 *   refEmpty: HTMLElement,
 *   refEmptyText: HTMLElement,
 *   refRecordButton: HTMLButtonElement,
 *   livePane: HTMLElement,
 *   liveBar: HTMLElement,
 *   liveLabel: HTMLElement,
 *   liveNotice: HTMLElement,
 *   liveViewport: HTMLElement,
 *   liveFrame: HTMLIFrameElement,
 *   noCode: HTMLElement,
 *   pageTree: HTMLElement,
 *   toolSeg: HTMLElement,
 *   capture: HTMLElement,
 *   stroke: SVGPolylineElement,
 *   compose: ComposeShell,
 * }} LiveShell
 */

/**
 * 書いているページへのコメントの欄（R-PAGE-COMMENT）。
 * @typedef {{
 *   box: HTMLElement,
 *   list: HTMLElement,
 *   body: HTMLTextAreaElement,
 *   undo: HTMLButtonElement,
 *   cancel: HTMLButtonElement,
 *   save: HTMLButtonElement,
 *   error: HTMLElement,
 *   away: HTMLElement,
 *   awayText: HTMLElement,
 *   back: HTMLButtonElement,
 * }} ComposeShell
 */

/** コメントの場所を置く道具と、ページを普通に触る「操作」（画面モックの案 A の上の帯）。 */
export const TOOLS = /** @type {const} */ ([
  ["element", "Element"],
  ["arrow", "Arrow"],
  ["pen", "Pen"],
  ["interact", "Interact"],
]);

const SVG = "http://www.w3.org/2000/svg";

/**
 * 見方の切り替えの帯と、舞台と、ページのツリーの骨組み。
 * @returns {LiveShell}
 */
export function buildShell() {
  const band = el("div", "lv-band");
  band.id = "live-band";

  const viewSeg = el("div", "lv-seg lv-view");
  viewSeg.setAttribute("role", "group");
  viewSeg.setAttribute("aria-label", "View");
  const pageButton = button("");
  pageButton.textContent = "Page";
  pageButton.dataset.view = "page";
  const codeButton = button("");
  codeButton.textContent = "Code";
  codeButton.dataset.view = "code";
  viewSeg.append(pageButton, codeButton);

  const widthGroup = el("div", "lv-widths lv-page-only");
  widthGroup.append(textEl("span", "lv-label", "Width"));
  const widthSeg = el("div", "lv-seg");
  widthSeg.setAttribute("role", "group");
  widthSeg.setAttribute("aria-label", "Width");
  for (const width of WIDTH_CHOICES) {
    const choice = button("");
    choice.textContent = String(width);
    choice.dataset.width = String(width);
    widthSeg.append(choice);
  }
  const widthInput = /** @type {HTMLInputElement} */ (el("input", "lv-width-input"));
  widthInput.type = "text";
  widthInput.inputMode = "numeric";
  widthInput.placeholder = `${WIDTH_MIN}–${WIDTH_MAX}`;
  widthInput.setAttribute("aria-label", `Width from ${WIDTH_MIN} to ${WIDTH_MAX}`);
  const widthError = el("span", "lv-width-error");
  widthError.setAttribute("role", "alert");
  widthError.hidden = true;
  widthGroup.append(widthSeg, widthInput, widthError);

  const compareSlot = el("div", "lv-compare lv-page-only");
  compareSlot.append(textEl("span", "lv-label", "Compare with"));
  const compareSelect = /** @type {HTMLSelectElement} */ (el("select", "lv-compare-select"));
  compareSelect.setAttribute("aria-label", "Compare with");
  const recordButton = button("lv-record");
  recordButton.textContent = "Record now";
  recordButton.title = "Take a snapshot of the page as it is now";
  compareSlot.append(compareSelect, recordButton);
  const modeSeg = el("div", "lv-seg lv-mode");
  modeSeg.setAttribute("role", "group");
  modeSeg.setAttribute("aria-label", "How to compare");
  for (const [mode, label] of [["side", "Side by side"], ["overlay", "Overlay"]]) {
    const choice = button("");
    choice.textContent = label;
    choice.dataset.compare = mode;
    modeSeg.append(choice);
  }
  const opacity = /** @type {HTMLInputElement} */ (el("input", "lv-opacity"));
  opacity.type = "range";
  opacity.min = "0";
  opacity.max = "100";
  opacity.value = "50";
  opacity.setAttribute("aria-label", "Opacity of the reference");
  opacity.title = "Opacity of the reference";
  compareSlot.append(modeSeg, opacity);
  const mockGroup = el("div", "lv-mock");
  const mockInput = /** @type {HTMLInputElement} */ (el("input", "lv-mock-input"));
  mockInput.type = "text";
  mockInput.placeholder = "Mock path, e.g. docs/mock.html";
  mockInput.setAttribute("aria-label", "Path of the mock HTML file");
  const mockAssign = button("lv-record lv-mock-assign");
  mockAssign.textContent = "Assign mock";
  const mockRemove = button("lv-record lv-mock-remove");
  mockRemove.textContent = "Remove mock";
  const mockReload = button("lv-record lv-mock-reload");
  mockReload.textContent = "Reload mock";
  mockReload.title = "Read the mock file again";
  const mockError = el("span", "lv-width-error lv-mock-error");
  mockError.setAttribute("role", "alert");
  mockError.hidden = true;
  mockGroup.append(mockInput, mockAssign, mockRemove, mockReload, mockError);
  compareSlot.append(mockGroup);

  const sideSeg = el("div", "lv-seg lv-side lv-page-only");
  sideSeg.setAttribute("role", "group");
  sideSeg.setAttribute("aria-label", "Shown side");
  for (const [side, label] of [["live", "Now"], ["ref", "Before"]]) {
    const choice = button("");
    choice.textContent = label;
    choice.dataset.side = side;
    sideSeg.append(choice);
  }

  const toolSeg = el("div", "lv-seg lv-tools lv-page-only");
  toolSeg.setAttribute("role", "group");
  toolSeg.setAttribute("aria-label", "Comment tools");
  for (const [tool, label] of TOOLS) {
    const choice = button("");
    choice.textContent = label;
    choice.dataset.tool = tool;
    choice.title =
      tool === "interact" ? "Use the page as it is" : `Put a place of a comment with the ${label.toLowerCase()} tool`;
    toolSeg.append(choice);
  }

  band.append(viewSeg, widthGroup, compareSlot, el("span", "lv-spacer"), toolSeg, sideSeg);

  const stage = el("div", "lv-stage");
  stage.id = "live-stage";
  const ref = pane("ref", "Before");
  const refNotice = el("span", "lv-notice");
  refNotice.hidden = true;
  ref.bar.append(refNotice);
  // スクリプトを止める（allow-scripts を付けない）。allow-same-origin も付けないので、
  // 中身はレビュー画面と別の不透明なオリジンになる（R-PAGE-SNAPSHOT）。
  const refFrame = /** @type {HTMLIFrameElement} */ (el("iframe", "lv-frame"));
  refFrame.title = "Snapshot";
  refFrame.setAttribute("sandbox", "");
  refFrame.referrerPolicy = "no-referrer";
  refFrame.hidden = true;
  const refEmpty = el("div", "lv-empty");
  const refEmptyText = textEl("p", "", "");
  const refRecordButton = button("btn lv-record");
  refRecordButton.textContent = "Record now";
  refEmpty.append(refEmptyText, refRecordButton);
  // モックのスクリプトは動かすが、allow-same-origin を付けないので不透明なオリジンで動き、
  // レビュー画面にも kemi の API にも届かない（R-PAGE-MOCK）。referrer も送らない。送ると
  // document.referrer からトークンの URL が読める。
  const refMockFrame = /** @type {HTMLIFrameElement} */ (el("iframe", "lv-frame"));
  refMockFrame.title = "Mock";
  refMockFrame.setAttribute("sandbox", "allow-scripts");
  refMockFrame.referrerPolicy = "no-referrer";
  refMockFrame.hidden = true;
  ref.box.append(refFrame, refMockFrame, refEmpty);
  const live = pane("live", "Now");
  const liveNotice = el("span", "lv-notice");
  liveNotice.hidden = true;
  live.bar.append(liveNotice);
  const liveFrame = /** @type {HTMLIFrameElement} */ (el("iframe", "lv-frame"));
  liveFrame.title = "Running page";
  // 道具を選んでいる間だけ枠の上に重ね、押す・描く操作をページより先に受ける（ページのスクリプトに
  // 横取りされないように）。描いている途中の線はここに描く。
  const capture = el("div", "lv-capture");
  capture.hidden = true;
  const strokeSvg = document.createElementNS(SVG, "svg");
  strokeSvg.setAttribute("class", "lv-stroke");
  const stroke = /** @type {SVGPolylineElement} */ (document.createElementNS(SVG, "polyline"));
  strokeSvg.append(stroke);
  capture.append(strokeSvg);
  live.box.append(liveFrame, capture);
  const compose = buildCompose();
  stage.append(ref.pane, live.pane, compose.box);

  const noCode = textEl(
    "div",
    "lv-no-code",
    "This is not a git repository, so there is no code diff to show.",
  );
  noCode.id = "live-no-code";

  const pageTree = el("aside", "tree lv-tree");
  pageTree.id = "page-tree";
  pageTree.setAttribute("aria-label", "Page tree");

  return {
    band,
    viewSeg,
    pageButton,
    codeButton,
    widthSeg,
    widthInput,
    widthError,
    compareSlot,
    compareSelect,
    recordButton,
    modeSeg,
    opacity,
    mockInput,
    mockAssign,
    mockRemove,
    mockReload,
    mockError,
    sideSeg,
    stage,
    refPane: ref.pane,
    refBar: ref.bar,
    refLabel: ref.label,
    refNotice,
    refViewport: ref.viewport,
    refFrame,
    refMockFrame,
    refEmpty,
    refEmptyText,
    refRecordButton,
    livePane: live.pane,
    liveBar: live.bar,
    liveLabel: live.label,
    liveNotice,
    liveViewport: live.viewport,
    liveFrame,
    noCode,
    pageTree,
    toolSeg,
    capture,
    stroke,
    compose,
  };
}

/** @returns {ComposeShell} */
function buildCompose() {
  const box = el("section", "lv-compose");
  box.id = "live-compose";
  box.setAttribute("aria-label", "Comment on the page");
  box.hidden = true;
  const head = textEl("div", "lv-compose-head", "Places of this comment");
  const list = el("ol", "lv-places");
  // 書きかけの場所と別の URL か表示幅を見ているときの知らせと、そこへ戻る操作（R-PAGE-COMMENT）。
  const away = el("div", "lv-compose-away");
  away.hidden = true;
  const awayText = el("span", "lv-compose-away-text");
  const back = button("lv-record lv-compose-back");
  away.append(awayText, back);
  const body = /** @type {HTMLTextAreaElement} */ (el("textarea", "lv-compose-body"));
  body.rows = 3;
  body.placeholder = "Refer to the places by their numbers (Cmd/Ctrl+Enter to save)";
  body.setAttribute("aria-label", "Comment");
  const error = el("span", "lv-width-error lv-compose-error");
  error.setAttribute("role", "alert");
  error.hidden = true;
  const actions = el("div", "lv-compose-actions");
  const undo = button("lv-record lv-compose-undo");
  undo.textContent = "Undo last place";
  const cancel = button("btn secondary lv-compose-cancel");
  cancel.textContent = "Discard";
  const save = button("btn primary lv-compose-save");
  save.textContent = "Comment";
  actions.append(undo, error, el("span", "lv-spacer"), cancel, save);
  box.append(head, away, list, body, actions);
  return { box, list, body, undo, cancel, save, error, away, awayText, back };
}

const PLACE_KINDS = { element: "Element", arrow: "Arrow", pen: "Pen" };

/**
 * 書いているコメントの場所の一覧。番号・種類・指している要素と、一覧から外す ×。
 * @param {ComposeShell} compose
 * @param {import("../live-model.js").Place[]} places
 * @param {(n: number) => void} onRemove
 * @param {boolean} locked 保存している間は外せない
 */
export function renderPlaces(compose, places, onRemove, locked) {
  compose.list.textContent = "";
  for (const place of places) {
    const row = el("li", "lv-place");
    row.dataset.n = String(place.n);
    const first = place.elements[0];
    if (first) {
      row.dataset.selector = first.selector;
    }
    const what =
      place.kind === "pen"
        ? `${place.elements.length} element${place.elements.length === 1 ? "" : "s"} inside`
        : first
          ? `${place.kind === "arrow" ? "→ " : ""}${first.selector}${first.text ? ` “${first.text.slice(0, 40)}”` : ""}`
          : "no element";
    const remove = button("lv-place-remove");
    remove.textContent = "×";
    remove.title = `Remove place ${place.n}`;
    remove.setAttribute("aria-label", `Remove place ${place.n}`);
    remove.disabled = locked;
    remove.addEventListener("click", () => onRemove(place.n));
    row.append(textEl("span", "lv-place-n", String(place.n)), textEl("span", "lv-place-kind", PLACE_KINDS[place.kind]), textEl("span", "lv-place-what", what), remove);
    compose.list.append(row);
  }
}

/**
 * 舞台の 1 枚。上に何を出しているかの帯、下にページを縮めて収める枠。
 * @param {string} side
 * @param {string} title
 */
function pane(side, title) {
  const element = el("section", "lv-pane");
  element.dataset.side = side;
  const bar = el("div", "lv-bar");
  const label = el("span", "lv-bar-label");
  bar.append(textEl("b", "", title), label);
  const viewport = el("div", "lv-viewport");
  const box = el("div", "lv-box");
  viewport.append(box);
  element.append(bar, viewport);
  return { pane: element, bar, label, viewport, box };
}

/**
 * 比べる相手の選択の中身。先頭は既定（決まった順で選ぶ）。
 * @param {HTMLSelectElement} select
 * @param {{ value: string, label: string }[]} options
 * @param {string} chosen
 */
export function renderCompareOptions(select, options, chosen) {
  select.textContent = "";
  for (const option of options) {
    const element = /** @type {HTMLOptionElement} */ (el("option"));
    element.value = option.value;
    element.textContent = option.label;
    select.append(element);
  }
  select.value = chosen;
}

/**
 * ページのツリー（R-PAGE-VIEW）。ページを押すとそのページへ、表示幅の札を押すとその幅で
 * そのページへ移る。表示中のページの下には、比べた結果があれば変化の一覧を出す。
 * @param {HTMLElement} container
 * @param {import("../live-model.js").PageTreeItem[]} items
 * @param {{
 *   onPage: (page: string) => void,
 *   onWidth: (page: string, width: number) => void,
 *   onShifted: (open: boolean) => void,
 *   onListed: (group: "main" | "shifted", count: number) => void,
 * }} handlers
 * @param {ChangeListState | ChangeNotice | null} changes
 */
export function renderPageTree(container, items, handlers, changes) {
  container.textContent = "";
  container.append(textEl("div", "lv-tree-head", `Pages ${items.length}`));
  const list = el("ul", "lv-pages");
  for (const item of items) {
    const row = el("li", "lv-page");
    row.dataset.page = item.page;
    if (item.current) {
      row.dataset.current = "true";
    }
    const open = button("lv-page-open");
    open.title = item.page;
    open.append(textEl("span", "lv-page-path", item.page));
    open.addEventListener("click", () => handlers.onPage(item.page));
    row.append(open);
    const tags = el("span", "lv-page-tags");
    for (const width of item.widths) {
      const tag = button("lv-width-tag");
      tag.textContent = String(width);
      tag.dataset.width = String(width);
      tag.title = `Show ${item.page} at ${width}px`;
      tag.addEventListener("click", () => handlers.onWidth(item.page, width));
      tags.append(tag);
    }
    if (item.mock) {
      tags.append(textEl("span", "lv-mock-tag", "mock"));
    }
    if (item.comments > 0) {
      const count = textEl("span", "lv-comment-count", String(item.comments));
      count.title = item.comments === 1 ? "1 comment on this page" : `${item.comments} comments on this page`;
      tags.append(count);
    }
    row.append(tags);
    if (item.current && changes !== null) {
      row.append(changeList(changes, handlers));
    }
    list.append(row);
  }
  container.append(list);
}

/** 変化の一覧の中の操作。描き直したときに同じ操作へフォーカスを移すために見分ける。 */
const FOCUSABLE_IN_CHANGES = [".lv-shifted > summary", ".lv-change-main .lv-change-show", ".lv-change-shifted .lv-change-show"];

/**
 * 表示中のページの下の変化の一覧だけを描き直す（R-PAGE-DIFF）。ページの行とその操作は作り直さない（動き続ける
 * ページで比べ直すたびに、押そうとしている操作やフォーカスが入れ替わらないように）。一覧の中の操作に
 * フォーカスがあれば、描き直した一覧の同じ操作に移す。表示中のページの行がまだ無ければ何もしない
 * （renderPageTree が描く）。
 * @param {HTMLElement} container renderPageTree で描いたツリー
 * @param {{ onShifted: (open: boolean) => void, onListed: (group: "main" | "shifted", count: number) => void }} handlers
 * @param {ChangeListState | ChangeNotice | null} changes
 */
export function renderChanges(container, handlers, changes) {
  const row = container.querySelector('.lv-page[data-current="true"]');
  if (!row) {
    return;
  }
  const old = row.querySelector(":scope > .lv-changes");
  const active = container.ownerDocument.activeElement;
  const focused = old && active && old.contains(active) ? FOCUSABLE_IN_CHANGES.find((selector) => active.matches(selector)) : undefined;
  if (changes === null) {
    old?.remove();
    return;
  }
  const box = changeList(changes, handlers);
  if (old) {
    old.replaceWith(box);
  } else {
    row.append(box);
  }
  if (focused) {
    /** @type {HTMLElement | null} */ (box.querySelector(focused))?.focus();
  }
}

/**
 * 一覧に一度に足す項目の数。大きなページで描き直しが重くならないよう、最初はこの数だけ並べ、
 * 残りは続きを出す操作で足す。
 */
const LISTED_CHANGES = 300;

/**
 * 変化の一覧（R-PAGE-DIFF）。主な変化を上に前後の値つきで、ずれただけは畳んで下に。比べられないときは、
 * 一覧の代わりにそのことを出す（R-PAGE-VIEW）。
 * @param {ChangeListState | ChangeNotice} state
 * @param {{ onShifted: (open: boolean) => void, onListed: (group: "main" | "shifted", count: number) => void }} handlers
 */
function changeList(state, handlers) {
  if ("notice" in state) {
    const box = el("div", "lv-changes lv-changes-failed");
    box.append(textEl("p", "lv-changes-notice", state.notice));
    return box;
  }
  const changes = state.list;
  const main = changes.filter((change) => change.kind !== "shifted");
  const shifted = changes.filter((change) => change.kind === "shifted");
  const box = el("div", "lv-changes");
  box.dataset.main = String(main.length);
  box.dataset.shifted = String(shifted.length);
  const head = el("div", "lv-changes-head");
  head.append(
    textEl("span", "", `Changes ${main.length}`),
    textEl("span", "", `${main.length} main · ${shifted.length} shifted`),
  );
  box.append(head);
  if (state.unmarked) {
    box.append(textEl("p", "lv-changes-unmarked", "Not marked: removed elements cannot be located in the snapshot"));
  }
  if (changes.length === 0) {
    box.append(textEl("p", "lv-changes-none", "No changes from the snapshot"));
    return box;
  }
  if (main.length > 0) {
    box.append(changeItems("lv-change-main", main, state.listed.main, (count) => handlers.onListed("main", count)));
  }
  if (shifted.length > 0) {
    const details = /** @type {HTMLDetailsElement} */ (el("details", "lv-shifted"));
    const shiftedItems = () =>
      changeItems("lv-change-shifted", shifted, state.listed.shifted, (count) => handlers.onListed("shifted", count));
    details.open = state.shiftedOpen;
    details.append(textEl("summary", "", `Shifted only ${shifted.length}`));
    if (state.shiftedOpen) {
      details.append(shiftedItems());
    }
    // 畳んでいる間は項目を作らない（ずれただけは数が多くなりやすい）。
    details.addEventListener("toggle", () => {
      handlers.onShifted(details.open);
      if (details.open && !details.querySelector(".lv-change-shifted")) {
        details.append(shiftedItems());
      }
    });
    box.append(details);
  }
  return box;
}

/**
 * 変化の項目を `listed` 個まで並べ、残りがあれば続きを出す操作を置く。押すと次の LISTED_CHANGES 個を足し、
 * 並べた数を `onListed` に知らせる（描き直しても同じ数まで並べるため）。
 * @param {string} className
 * @param {import("../live-diff.js").Change[]} changes
 * @param {number} listed
 * @param {(count: number) => void} onListed
 */
function changeItems(className, changes, listed, onListed) {
  const list = el("ul", `lv-change-list ${className}`);
  let shown = 0;
  const more = el("li", "lv-change-more");
  const remaining = textEl("span", "", "");
  const showMore = button("lv-change-show");
  showMore.textContent = "Show more";
  more.append(remaining, showMore);
  /** @param {number} count */
  const showUpTo = (count) => {
    const items = changes.slice(shown, count).map((change) => {
      const item = el("li", "lv-change");
      item.dataset.kind = change.kind;
      const what = el("span", "lv-change-what");
      what.append(...changeWhat(change));
      item.append(el("i", "lv-change-dot"), what, textEl("span", "lv-change-where", change.label));
      return item;
    });
    more.before(...items);
    shown = Math.min(count, changes.length);
    remaining.textContent = `${changes.length - shown} more `;
    more.hidden = shown >= changes.length;
  };
  list.append(more);
  showUpTo(Math.max(listed, LISTED_CHANGES));
  showMore.addEventListener("click", () => {
    showUpTo(shown + LISTED_CHANGES);
    onListed(shown);
  });
  return list;
}

/**
 * 変化の中身の文。見た目と文字は前後の値を並べる。
 * @param {import("../live-diff.js").Change} change
 * @returns {(Node | string)[]}
 */
function changeWhat(change) {
  switch (change.kind) {
    case "visual":
      return [`${change.property} `, textEl("s", "", change.was), " → ", textEl("b", "", change.is)];
    case "text":
      return ["Text ", textEl("s", "", change.was), " → ", textEl("b", "", change.is)];
    case "added":
      return [change.is === "" ? "Added" : `Added: ${change.is}`];
    case "removed":
      return [change.was === "" ? "Removed" : `Removed: ${change.was}`];
    case "shifted":
      return ["Moved or resized"];
  }
}

/** 消えた要素の印。スナップショットの枠の中は触れないので、写しの HTML に属性と <style> を足して描く。 */
const REMOVED_MARK_STYLE = "[data-kemi-removed] { outline: 2px solid rgb(214, 69, 69) !important; outline-offset: -2px !important; }";

/** 記述の番号と HTML を読み直した文書での要素の位置の対応を入れる <meta> の name（web/live/page.js の ELEMENT_MAP）。 */
const ELEMENT_MAP = "kemi-elements";

/**
 * 比べる相手の側の印（消えた要素）を付けたスナップショットの HTML。HTML を読み直すと要素の並びが DOM と
 * 変わることがあるので、ページがスナップショットに添えた対応（記述の番号ごとの、読み直した文書での要素の
 * 位置。web/live/page.js の elementMap）で要素を探し、属性を付ける。位置は対応の <meta> を外してから、
 * 要素を文書の順に（<template> は中身を）数える。shadow root の中には文書の <style> が効かないので、
 * 印を付けた shadow root ごとにも <style> を足す。`mapped` は、ページが対応を添えていたか（無ければ印は付けられない）。
 * @param {string} html
 * @param {number[]} indices 印を付ける要素の記述の番号
 * @returns {{ html: string, mapped: boolean }}
 */
export function markRemovedInSnapshot(html, indices) {
  const parsed = new DOMParser().parseFromString(html, "text/html");
  const map = parsed.head.querySelector(`meta[name="${ELEMENT_MAP}"]`);
  const runs = (map?.getAttribute("content") ?? "")
    .split(" ")
    .filter((run) => run !== "")
    .map((run) => run.split(",").map(Number));
  map?.remove();
  /** @type {Set<number>} */
  const wanted = new Set();
  for (const index of indices) {
    const run = runs.find(([, first, count]) => index >= first && index < first + count);
    if (run) {
      wanted.add(run[0] + index - run[1]);
    }
  }
  /** @type {Element[]} */
  const marked = [];
  let position = 0;
  /** @param {Element} element */
  const visit = (element) => {
    if (wanted.has(position)) {
      marked.push(element);
    }
    position += 1;
    const children = element instanceof HTMLTemplateElement ? element.content.children : element.children;
    for (const child of [...children]) {
      visit(child);
    }
  };
  visit(parsed.documentElement);
  /** @type {Set<Node>} */
  const styled = new Set();
  for (const element of marked) {
    element.setAttribute("data-kemi-removed", "");
    const root = element.getRootNode();
    if (!styled.has(root)) {
      styled.add(root);
      const style = parsed.createElement("style");
      style.textContent = REMOVED_MARK_STYLE;
      (root instanceof DocumentFragment ? root : parsed.head).append(style);
    }
  }
  const doctype = parsed.doctype ? `<!doctype ${parsed.doctype.name}>` : "";
  return { html: doctype + parsed.documentElement.outerHTML, mapped: map !== null };
}
