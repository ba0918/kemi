// @ts-check
// 変更間の移動（n / p）と、移った先の表示。

import * as api from "../api.js";
import { actions } from "../actions.js";
import { dom } from "../dom.js";
import {
  NAV_MARGIN,
  commentsOf,
  currentEntry,
  fileCacheKey,
  reachableStops,
  state,
} from "../state.js";
import {
  hasLoadedStops,
  hasStops,
  lineHasAnchor,
  lineOffsets,
  navNextTarget,
  navPrevTarget,
  nextFileIndex,
} from "../model.js";
import { scheduleRender } from "./display.js";
import { navView } from "../views/nav.js";
import { showToast } from "../views/overlay.js";

/** @typedef {import("../state.js").Entry} Entry */

/**
 * 移ったファイルが見えるよう、ツリーのグループとディレクトリをそこまで開く。
 * @param {Entry} entry
 */
export function revealInTree(entry) {
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
 * 表示行 `index` を、上端から NAV_MARGIN の位置へ送る。
 * @param {number} index
 * @param {number[]} offsets
 */
function scrollToRow(index, offsets) {
  const top = offsets[index] ?? 0;
  dom.viewport.scrollTop = Math.max(0, top - NAV_MARGIN);
  // 折り返しのある行は、初めて描いたときに基準値より高くなる。送り先より上でそれが
  // 起きるとこの行が下へずれるので、送り先を覚えておき、measureHeights が測り終える
  // まで同じ位置へ置き直す。置く場所を決めるためだけの記録で、「現在」や n / p の
  // 行き先はこれを見ない（表示の状態から毎回決める。R-NAV）。
  state.landing = { index, top, scrollTop: dom.viewport.scrollTop, margin: NAV_MARGIN };
  scheduleRender();
}

/**
 * n / p: ファイルの中の次（前）の止まる場所へ。端では、見えている順で次（前）のファイルの
 * 最初（最後）の止まる場所へ移る。最後（最初）のファイルでは止まって知らせる（R-NAV）。
 * @param {1 | -1} direction
 */
export async function navigate(direction) {
  const entry = currentEntry();
  if (!entry || state.loading || state.navigating || state.submitted) {
    return;
  }
  const offsets = lineOffsets(state.heights);
  const stops = reachableStops();
  const tops = stops.map((index) => offsets[index] ?? 0);
  const view = navView();
  const index =
    direction > 0
      ? navNextTarget(tops, view, NAV_MARGIN)
      : navPrevTarget(tops, view, NAV_MARGIN);
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
        await actions.selectIndex(next, { scrollTop: true });
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
export function applyPendingJump() {
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
