// @ts-check
// 左のペイン（ファイルツリー、説明の目次、--live のページのツリー）の幅と開閉。広い画面でだけ働く。
// 狭い画面では左のペインは引き出しで、境界を出さない（style.css の狭い画面の規則）。
// 幅と開閉は見方（説明・コード・ページ）ごとに持ち、表示の好みとして覚える（R-SERVE）。

import { dom } from "../dom.js";
import { state } from "../state.js";
import { clampSidePaneWidth, sidePaneDefaultWidth, sidePaneKeyWidth } from "../model.js";
import { loadSidePane, saveSidePane } from "../storage.js";
import { onResize } from "./display.js";

/** @typedef {{ width: number, collapsed: boolean }} SidePane */

/** @type {Partial<Record<typeof state.view, SidePane>>} */
const panes = {};

/** いまの見方の左のペイン。 */
function currentPane() {
  const view = state.view;
  let pane = panes[view];
  if (!pane) {
    const stored = loadSidePane(view);
    pane = { width: stored.width ?? sidePaneDefaultWidth(view), collapsed: stored.collapsed };
    panes[view] = pane;
  }
  return pane;
}

/** いまレイアウトに収まる幅。覚えている幅は変えない（窓を広げ直したら元の幅に戻す）。 */
function shownWidth() {
  return clampSidePaneWidth(currentPane().width, dom.layout.clientWidth);
}

/** いまの見方の幅と開閉を、レイアウトと境界の操作に映す。見方が変わったときにも呼ぶ。 */
export function syncSidePane() {
  const pane = currentPane();
  const width = shownWidth();
  dom.layout.style.setProperty("--side-width", pane.collapsed ? "0px" : `${width}px`);
  dom.layout.toggleAttribute("data-side-collapsed", pane.collapsed);
  dom.sideSplitter.setAttribute("aria-valuemin", String(clampSidePaneWidth(0, dom.layout.clientWidth)));
  dom.sideSplitter.setAttribute("aria-valuemax", String(Math.floor(clampSidePaneWidth(Infinity, dom.layout.clientWidth))));
  dom.sideSplitter.setAttribute("aria-valuenow", String(pane.collapsed ? 0 : Math.round(width)));
  const label = pane.collapsed ? "Show the side pane" : "Hide the side pane";
  dom.sideToggle.title = label;
  dom.sideToggle.setAttribute("aria-label", label);
  dom.sideToggle.setAttribute("aria-expanded", String(!pane.collapsed));
}

/**
 * 差分の列の幅が変わるので、折返しの高さと帯を測り直させる。ページの見方の舞台は、自分で大きさの変化を見ている。
 */
function layoutChanged() {
  syncSidePane();
  if (state.view === "code") {
    onResize();
  }
}

/** @param {number} width */
function resizeTo(width) {
  currentPane().width = clampSidePaneWidth(width, dom.layout.clientWidth);
  layoutChanged();
}

function toggleCollapsed() {
  const pane = currentPane();
  pane.collapsed = !pane.collapsed;
  layoutChanged();
  saveSidePane(state.view, pane);
}

/** @param {KeyboardEvent} event */
function onSplitterKey(event) {
  if (state.narrow) {
    return;
  }
  if (event.key === "Enter") {
    event.preventDefault();
    toggleCollapsed();
    return;
  }
  if (currentPane().collapsed) {
    return;
  }
  const width = sidePaneKeyWidth(event.key, shownWidth(), dom.layout.clientWidth);
  if (width !== null) {
    event.preventDefault();
    resizeTo(width);
    saveSidePane(state.view, currentPane());
  }
}

/** 境界を掴んで幅を変える。幅は離したときに覚える。 @param {PointerEvent} event */
function startDrag(event) {
  if (state.narrow || currentPane().collapsed || event.button !== 0) {
    return;
  }
  event.preventDefault();
  const splitter = dom.sideSplitter;
  splitter.focus();
  const startX = event.clientX;
  const startWidth = shownWidth();
  splitter.setPointerCapture(event.pointerId);
  /** @param {PointerEvent} move */
  const onMove = (move) => resizeTo(startWidth + move.clientX - startX);
  const onUp = () => {
    splitter.removeEventListener("pointermove", onMove);
    splitter.removeEventListener("pointerup", onUp);
    splitter.removeEventListener("pointercancel", onUp);
    saveSidePane(state.view, currentPane());
  };
  splitter.addEventListener("pointermove", onMove);
  splitter.addEventListener("pointerup", onUp);
  splitter.addEventListener("pointercancel", onUp);
}

/** 境界の操作をつなぐ。起動時に 1 回だけ呼ぶ。 */
export function setupSidePane() {
  dom.sideSplitter.addEventListener("keydown", onSplitterKey);
  dom.sideSplitter.addEventListener("pointerdown", startDrag);
  dom.sideToggle.addEventListener("click", toggleCollapsed);
  // 窓の幅が変わると上限（レイアウトの半分）も変わる。
  new ResizeObserver(() => {
    if (!state.narrow) {
      syncSidePane();
    }
  }).observe(dom.layout);
  syncSidePane();
}
