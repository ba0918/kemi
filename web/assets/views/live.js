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
 *   sideSeg: HTMLElement,
 *   stage: HTMLElement,
 *   refPane: HTMLElement,
 *   refBar: HTMLElement,
 *   refViewport: HTMLElement,
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
    sideSeg,
    stage,
    refPane: ref.pane,
    refBar: ref.bar,
    refViewport: ref.viewport,
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
 * ページのツリー（R-PAGE-VIEW）。ページを押すとそのページへ、表示幅の札を押すとその幅で
 * そのページへ移る。
 * @param {HTMLElement} container
 * @param {import("../live-model.js").PageTreeItem[]} items
 * @param {{ onPage: (page: string) => void, onWidth: (page: string, width: number) => void }} handlers
 */
export function renderPageTree(container, items, handlers) {
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
    list.append(row);
  }
  container.append(list);
}
