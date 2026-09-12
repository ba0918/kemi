// @ts-check
// 上部バー（題、meta、グループ単位、進捗、表示の切り替えの押された状態）、更新バッジ、
// 承認対象のフッタ、送信のボタン。

import { actions } from "../actions.js";
import { button, dom, el, focusKeyWithin, restoreFocusKey, textEl } from "../dom.js";
import { UNIT_LABELS, state } from "../state.js";
import { metaItems, seenProgress, unitSwitchOrder } from "../model.js";

export function renderHeader() {
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
export function renderUnitSwitch() {
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
    item.addEventListener("click", () => actions.switchUnit(unit, null));
    dom.unitSwitch.append(item);
  }
  restoreFocusKey(dom.unitSwitch, focusKey);
}

/** 上部の、表示中のグループ単位の見たの進捗（R-SEEN）。 */
export function renderProgress() {
  const progress = seenProgress(state.entries.map((entry) => entry.file));
  dom.progress.hidden = !state.review || progress.total === 0;
  dom.progressBar.style.width = `${progress.total ? (progress.seen / progress.total) * 100 : 0}%`;
  dom.progressText.textContent = `見た ${progress.seen} / ${progress.total}`;
}

export function renderUpdateBadge() {
  dom.updateBadge.hidden = !state.updateAvailable;
}

export function renderFooter() {
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

/** 送信後は承認と変更要求のボタンを押せなくする（R-SUBMIT）。 */
export function renderSubmitButtons() {
  dom.submitApproved.disabled = state.submitted;
  dom.submitChanges.disabled = state.submitted;
}
