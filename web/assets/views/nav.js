// @ts-check
// 右下の「前の変更 / 次の変更」と、スクロールバーの横の位置の帯。

import { dom } from "../dom.js";
import { NAV_MARGIN, currentEntry, reachableStops, state } from "../state.js";
import { navCurrentIndex, rulerMarks } from "../model.js";

/** @typedef {import("../model.js").DisplayLine} DisplayLine */

/**
 * 「現在」と n / p の移り先を決める表示の状態（R-NAV）。末尾まで進めたかどうかは、
 * 実際に切り詰めるブラウザの値で見る。
 * @returns {{ scrollTop: number, viewportHeight: number, contentHeight: number }}
 */
export function navView() {
  return {
    scrollTop: dom.viewport.scrollTop,
    viewportHeight: dom.viewport.clientHeight,
    contentHeight: dom.viewport.scrollHeight,
  };
}

/**
 * 右下の「前の変更 / 次の変更」と「現在 / 全体」（R-NAV）。
 * @param {number[]} offsets
 */
export function renderNav(offsets) {
  const entry = currentEntry();
  dom.nav.hidden = !entry;
  if (!entry) {
    return;
  }
  const tops = reachableStops().map((index) => offsets[index] ?? 0);
  const current = navCurrentIndex(tops, navView(), NAV_MARGIN);
  dom.navPos.textContent = `${current < 0 ? "–" : current + 1} / ${tops.length}`;
  dom.navPrev.disabled = state.loading || state.navigating;
  dom.navNext.disabled = state.loading || state.navigating;
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
export function renderRuler(offsets) {
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
      const kinds = state.display.map(rulerKind);
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
