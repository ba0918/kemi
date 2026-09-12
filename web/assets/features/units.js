// @ts-check
// グループ単位（最終形 / コミットごと）の切り替えと、作れなかった単位の作り直し。

import * as api from "../api.js";
import { dom } from "../dom.js";
import { currentEntry, flatten, state, unitLabel } from "../state.js";
import { unitSwitchTarget } from "../model.js";
import { renderDiff } from "./display.js";
import { revealInTree } from "./navigation.js";
import { applyReview, selectEntry } from "./files.js";
import { renderFileHeader, renderGroupHeader, renderNotice } from "../views/file-header.js";
import { renderFooter, renderHeader, renderUnitSwitch } from "../views/header.js";
import { setModalText, showOverlay, showToast } from "../views/overlay.js";
import { renderTree } from "../views/tree.js";

/** @typedef {import("../state.js").Entry} Entry */

/**
 * グループ単位を切り替える（R-UNIT）。同じパスのファイル（コミットごとではそのパスを含む
 * 最初のコミット）を出す。由来やコメント一覧から移るときは、そのファイルの該当行を出す。
 * 移り先が無ければ（履歴の書き換えで消えたなど）別のファイルへは移らず、切り替えもせず
 * `missing` を呼ぶ。
 * @param {string} unit
 * @param {{ find: (entries: Entry[]) => number, side: string, line: number | null, missing: () => void } | null} jump
 */
export async function switchUnit(unit, jump) {
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
    showToast(`${unitLabel(unit)}を読み込み中です`);
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
  dom.modalTitle.textContent = `${unitLabel(unit)}の単位を作れなかった`;
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

/** もう片方の単位の作成の状態が変わった。待っている切り替えがあれば続ける。 */
export async function onUnitEvent() {
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
