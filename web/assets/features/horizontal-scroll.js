// @ts-check
import { dom } from "../dom.js";
import { currentEntry, displayMode, displayWrap, state } from "../state.js";
import { horizontalEntry, measureHorizontal } from "../model.js";

export function invalidateHorizontalWidth() {
  state.horizontalMeasurePending = true;
}

export function syncHorizontal() {
  state.horizontal = horizontalEntry(state.horizontal, currentEntry()?.file.id ?? null);
  const active = displayMode() === "split" && !displayWrap();
  const codes = Array.from(dom.content.querySelectorAll(".split .code"));
  if (!active || state.loading || codes.length === 0) {
    dom.horizontal.hidden = true;
    dom.content.style.setProperty("--code-offset", "0px");
    return;
  }
  let measured = 0;
  let viewport = Infinity;
  for (const code of codes) {
    const text = code.firstElementChild;
    if (text) measured = Math.max(measured, text.getBoundingClientRect().width);
    viewport = Math.min(viewport, code.clientWidth - parseFloat(getComputedStyle(code).paddingRight));
  }
  state.horizontal = measureHorizontal(state.horizontal, measured, viewport, state.horizontalMeasurePending);
  state.horizontalMeasurePending = false;
  const range = Math.max(0, state.horizontal.width - viewport);
  dom.horizontal.hidden = range === 0;
  dom.horizontalWidth.style.width = `${dom.horizontal.clientWidth + range}px`;
  dom.horizontal.scrollLeft = state.horizontal.left;
  dom.content.style.setProperty("--code-offset", `${-state.horizontal.left}px`);
}

export function onHorizontalScroll() {
  if (dom.horizontal.hidden) return;
  state.horizontal = { ...state.horizontal, left: dom.horizontal.scrollLeft };
  dom.content.style.setProperty("--code-offset", `${-state.horizontal.left}px`);
}

/** @param {WheelEvent} event */
export function onCodeWheel(event) {
  if (dom.horizontal.hidden || !(event.target instanceof Element) || !event.target.closest(".split .code")) return;
  const delta = event.deltaX || (event.shiftKey ? event.deltaY : 0);
  if (!delta) return;
  const scale = event.deltaMode === 1 ? 24 : event.deltaMode === 2 ? dom.horizontal.clientWidth : 1;
  dom.horizontal.scrollLeft += delta * scale;
  onHorizontalScroll();
  // 縦だけの入力は通常の仮想スクロールへ渡す。
  event.preventDefault();
}

/** @param {KeyboardEvent} event */
export function onHorizontalKey(event) {
  /** @type {Record<string, number>} */
  const steps = {
    ArrowLeft: -40,
    ArrowRight: 40,
    Home: -Infinity,
    End: Infinity,
    PageUp: -dom.horizontal.clientWidth,
    PageDown: dom.horizontal.clientWidth,
  };
  const delta = steps[event.key];
  if (delta === undefined) return;
  event.preventDefault();
  dom.horizontal.scrollLeft = Number.isFinite(delta) ? dom.horizontal.scrollLeft + delta : delta < 0 ? 0 : dom.horizontal.scrollWidth;
  onHorizontalScroll();
}
