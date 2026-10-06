// @ts-check
// `--live` のページの見方の要素（live.md の R-PAGE-VIEW）: 上部の帯、比べる相手と動いている
// ページを並べる舞台、左のページのツリー。状態は持たず、押されたときの処理は呼ぶ側が渡す。
// `--live` のレビューでだけ読み込む。

import { button, el, textEl } from "../dom.js";
import { WIDTH_CHOICES, WIDTH_MAX, WIDTH_MIN } from "../live-model.js";

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
 *   refMarks: HTMLElement,
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
 * }} LiveShell
 */

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

  band.append(viewSeg, widthGroup, compareSlot, el("span", "lv-spacer"), sideSeg);

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
  // 消えた要素の印。スナップショットの枠の中は触れないので、枠と同じ大きさと変形で上に重ねる。
  const refMarks = el("div", "lv-marks");
  refMarks.hidden = true;
  ref.box.append(refFrame, refMockFrame, refMarks, refEmpty);
  const live = pane("live", "Now");
  const liveNotice = el("span", "lv-notice");
  liveNotice.hidden = true;
  live.bar.append(liveNotice);
  const liveFrame = /** @type {HTMLIFrameElement} */ (el("iframe", "lv-frame"));
  liveFrame.title = "Running page";
  live.box.append(liveFrame);
  stage.append(ref.pane, live.pane);

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
    refMarks,
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
  };
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
 * @param {{ onPage: (page: string) => void, onWidth: (page: string, width: number) => void, onShifted: (open: boolean) => void }} handlers
 * @param {{ list: import("../live-diff.js").Change[], shiftedOpen: boolean } | null} changes
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
    row.append(tags);
    if (item.current && changes !== null) {
      row.append(changeList(changes.list, changes.shiftedOpen, handlers.onShifted));
    }
    list.append(row);
  }
  container.append(list);
}

/** 一覧に並べる項目の上限。超えた分は数だけ出す（大きなページで描き直しが重くならないように）。 */
const LISTED_CHANGES = 300;

/**
 * 変化の一覧（R-PAGE-DIFF）。主な変化を上に前後の値つきで、ずれただけは畳んで下に。
 * @param {import("../live-diff.js").Change[]} changes
 * @param {boolean} shiftedOpen
 * @param {(open: boolean) => void} onShifted
 */
function changeList(changes, shiftedOpen, onShifted) {
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
  if (changes.length === 0) {
    box.append(textEl("p", "lv-changes-none", "No changes from the snapshot"));
    return box;
  }
  if (main.length > 0) {
    box.append(changeItems("lv-change-main", main));
  }
  if (shifted.length > 0) {
    const details = /** @type {HTMLDetailsElement} */ (el("details", "lv-shifted"));
    details.open = shiftedOpen;
    details.append(textEl("summary", "", `Shifted only ${shifted.length}`));
    if (shiftedOpen) {
      details.append(changeItems("lv-change-shifted", shifted));
    }
    // 畳んでいる間は項目を作らない（ずれただけは数が多くなりやすい）。
    details.addEventListener("toggle", () => {
      onShifted(details.open);
      if (details.open && !details.querySelector(".lv-change-shifted")) {
        details.append(changeItems("lv-change-shifted", shifted));
      }
    });
    box.append(details);
  }
  return box;
}

/**
 * @param {string} className
 * @param {import("../live-diff.js").Change[]} changes
 */
function changeItems(className, changes) {
  const list = el("ul", `lv-change-list ${className}`);
  for (const change of changes.slice(0, LISTED_CHANGES)) {
    const item = el("li", "lv-change");
    item.dataset.kind = change.kind;
    const what = el("span", "lv-change-what");
    what.append(...changeWhat(change));
    item.append(el("i", "lv-change-dot"), what, textEl("span", "lv-change-where", change.label));
    list.append(item);
  }
  if (changes.length > LISTED_CHANGES) {
    list.append(textEl("li", "lv-change-more", `${changes.length - LISTED_CHANGES} more`));
  }
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

/**
 * 比べる相手の側の印（消えた要素）。位置と大きさはスナップショットの文書の座標で、層ごと枠と同じ
 * 倍率で縮める。
 * @param {HTMLElement} layer
 * @param {number[][]} boxes
 */
export function renderRemovedMarks(layer, boxes) {
  layer.textContent = "";
  for (const [left, top, width, height] of boxes) {
    const mark = el("div", "lv-mark");
    mark.dataset.kind = "removed";
    mark.style.left = `${left}px`;
    mark.style.top = `${top}px`;
    mark.style.width = `${width}px`;
    mark.style.height = `${height}px`;
    layer.append(mark);
  }
}
